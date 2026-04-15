use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use bytecode::{Bytecode, Instruction, SectionBytecode, SectionFlags};
use executor::{
    executor::{Executor, SeekPrimitiveResult, SeekToResult, StepResult},
    time::Timestamp,
};
use futures::{FutureExt, StreamExt, channel::mpsc::{UnboundedReceiver, UnboundedSender}, future};
use smol::Timer;
use stdlib::registry::registry;

use crate::services::ServiceManagerMessage;

const DEFAULT_PLAYBACK_FPS: f64 = 60.0;

pub struct ExecutionSnapshot {
    pub current_timestamp: Timestamp,
    pub errors: Vec<String>,
    pub slide_count: usize,
    pub slide_durations: Vec<Option<f64>>,
}

pub enum ExecutionMessage {
    UpdateBytecode {
        bytecode: Bytecode,
    },
    SetPlaybackFrameRate {
        fps: f64,
    },
    UpdateParameter,
    TogglePlay,
    SeekTo {
        target: Timestamp,
    },
}

pub struct ExecutionService {
    rx: UnboundedReceiver<ExecutionMessage>,
    sm_tx: UnboundedSender<ServiceManagerMessage>,
}

enum PendingEvent {
    Closed,
    Message(ExecutionMessage),
    Ready,
}

enum SeekEvent {
    Closed,
    Message(ExecutionMessage),
    Ready(SeekToResult),
}

fn default_bytecode() -> Bytecode {
    let mut section = SectionBytecode::new(SectionFlags { is_stdlib: true, is_library: true });
    section.instructions.push(Instruction::EndOfExecutionHead);
    Bytecode::new(vec![Arc::new(section)])
}

fn playback_interval(fps: f64) -> Duration {
    Duration::from_secs_f64(1.0 / fps)
}

impl ExecutionService {
    pub fn new(
        rx: UnboundedReceiver<ExecutionMessage>,
        sm_tx: UnboundedSender<ServiceManagerMessage>,
    ) -> Self {
        Self { rx, sm_tx }
    }

    /// spawn the executor OS thread and return immediately.
    /// the thread owns self and runs until the message channel closes.
    pub fn run(self) {
        std::thread::spawn(move || {
            smol::block_on(self.run_loop());
        });
    }

    async fn run_loop(mut self) {
        let native_funcs = registry().func_table();
        let mut executor = Executor::new(default_bytecode(), native_funcs);
        let mut target = Timestamp::default();
        let mut is_playing = false;
        let mut playback_frame_rate = DEFAULT_PLAYBACK_FPS;
        let mut last_play_step_at = None;

        loop {
            let mut is_closed = false;
            while let Ok(message) = self.rx.try_next() {
                match message {
                    Some(msg) => self.handle_message(
                        msg,
                        &mut executor,
                        &mut target,
                        &mut is_playing,
                        &mut playback_frame_rate,
                        &mut last_play_step_at,
                    ),
                    None => {
                        is_closed = true;
                        break;
                    }
                }
            }

            if is_closed {
                break;
            }

            if is_playing {
                if executor.state.has_errors() {
                    is_playing = false;
                    target = executor.state.timestamp;
                    last_play_step_at = None;
                    continue;
                }

                let frame_interval = playback_interval(playback_frame_rate);
                let last_step_at = last_play_step_at.get_or_insert_with(Instant::now);
                let now = Instant::now();
                let next_step_at = *last_step_at + frame_interval;

                if now < next_step_at {
                    let next_message = FutureExt::fuse(self.rx.next());
                    let sleep = FutureExt::fuse(Timer::after(next_step_at - now));
                    futures::pin_mut!(next_message, sleep);

                    match future::select(next_message, sleep).await {
                        future::Either::Left((Some(msg), _)) => {
                            self.handle_message(
                                msg,
                                &mut executor,
                                &mut target,
                                &mut is_playing,
                                &mut playback_frame_rate,
                                &mut last_play_step_at,
                            );
                            continue;
                        }
                        future::Either::Left((None, _)) => break,
                        future::Either::Right((_, _)) => {}
                    }
                }

                let step_started_at = Instant::now();
                let dt = step_started_at.duration_since(*last_step_at).as_secs_f64();

                let play_event = {
                    let next_message = FutureExt::fuse(self.rx.next());
                    let play_step = FutureExt::fuse(Self::play_step(&mut executor, dt, &mut is_playing));
                    futures::pin_mut!(next_message, play_step);

                    match future::select(next_message, play_step).await {
                        future::Either::Left((Some(msg), _)) => PendingEvent::Message(msg),
                        future::Either::Left((None, _)) => PendingEvent::Closed,
                        future::Either::Right((_, _)) => PendingEvent::Ready,
                    }
                };

                match play_event {
                    PendingEvent::Message(msg) => {
                        self.handle_message(
                            msg,
                            &mut executor,
                            &mut target,
                            &mut is_playing,
                            &mut playback_frame_rate,
                            &mut last_play_step_at,
                        );
                    }
                    PendingEvent::Closed => break,
                    PendingEvent::Ready => {
                        last_play_step_at = Some(step_started_at);
                        target = executor.state.timestamp;
                        self.emit_snapshot(&executor);
                    }
                }
            } else if executor.state.timestamp != target && !executor.state.has_errors() {
                let seek_target = target;
                let seek_event = {
                    let next_message = FutureExt::fuse(self.rx.next());
                    let seek = FutureExt::fuse(executor.seek_to(seek_target));
                    futures::pin_mut!(next_message, seek);

                    match future::select(next_message, seek).await {
                        future::Either::Left((Some(msg), _)) => SeekEvent::Message(msg),
                        future::Either::Left((None, _)) => SeekEvent::Closed,
                        future::Either::Right((result, _)) => SeekEvent::Ready(result),
                    }
                };

                match seek_event {
                    SeekEvent::Message(msg) => {
                        self.handle_message(
                            msg,
                            &mut executor,
                            &mut target,
                            &mut is_playing,
                            &mut playback_frame_rate,
                            &mut last_play_step_at,
                        );
                    }
                    SeekEvent::Closed => break,
                    SeekEvent::Ready(result) => {
                        match result {
                            SeekToResult::SeekedTo(reached) => target = reached,
                            SeekToResult::Error(_) => target = executor.state.timestamp,
                        }
                        self.emit_snapshot(&executor);
                    }
                }
            } else {
                match self.rx.next().await {
                    None => break,
                    Some(msg) => {
                        self.handle_message(
                            msg,
                            &mut executor,
                            &mut target,
                            &mut is_playing,
                            &mut playback_frame_rate,
                            &mut last_play_step_at,
                        );
                    }
                }
            }
        }
    }

    async fn play_step(executor: &mut Executor, dt: f64, is_playing: &mut bool) {
        loop {
            match executor.seek_primitive_anim().await {
                SeekPrimitiveResult::PrimitiveAnim => break,
                SeekPrimitiveResult::EndOfSection => {
                    if executor.state.timestamp.slide + 1 < executor.slide_count() {
                        executor.advance_section();
                    } else {
                        *is_playing = false;
                        return;
                    }
                }
                SeekPrimitiveResult::Error(e) => {
                    executor.state.error(e.to_string());
                    *is_playing = false;
                    return;
                }
            }
        }

        match executor.step_primitive_anims(dt.max(f64::MIN_POSITIVE)).await {
            StepResult::Continue | StepResult::EndOfAllAnims => {}
            StepResult::Error(e) => {
                executor.state.error(e.to_string());
                *is_playing = false;
            }
        }
    }

    fn handle_message(
        &self,
        msg: ExecutionMessage,
        executor: &mut Executor,
        target: &mut Timestamp,
        is_playing: &mut bool,
        playback_frame_rate: &mut f64,
        last_play_step_at: &mut Option<Instant>,
    ) {
        match msg {
            ExecutionMessage::UpdateBytecode { bytecode } => {
                executor.update_bytecode(bytecode);
                *target = executor.state.timestamp;
                *last_play_step_at = Some(Instant::now());
                self.emit_snapshot(executor);
            }
            ExecutionMessage::SetPlaybackFrameRate { fps } => {
                if fps.is_finite() && fps > 0.0 {
                    *playback_frame_rate = fps;
                    *last_play_step_at = Some(Instant::now());
                }
            }
            ExecutionMessage::SeekTo { target: t } => {
                *is_playing = false;
                *target = t;
                *last_play_step_at = None;
            }
            ExecutionMessage::TogglePlay => {
                *is_playing = !*is_playing;
                *target = executor.state.timestamp;
                if *is_playing {
                    *last_play_step_at = Some(Instant::now());
                } else {
                    *last_play_step_at = None;
                }
            }
            ExecutionMessage::UpdateParameter => {
                // TODO: apply parameter change and re-evaluate stateful dependents
            }
        }
    }

    fn emit_snapshot(&self, executor: &Executor) {
        let snapshot = ExecutionSnapshot {
            current_timestamp: executor.state.timestamp,
            errors: executor.state.errors.clone(),
            slide_count: executor.slide_count(),
            slide_durations: executor.slide_durations(),
        };

        self.sm_tx
            .unbounded_send(ServiceManagerMessage::ExecutionStateUpdated { snapshot })
            .ok();
    }
}
