use std::{
    pin::pin, sync::Arc, time::{Duration, Instant}
};

use bytecode::{Bytecode, Instruction, SectionBytecode, SectionFlags};
use executor::{
    executor::{Executor, SeekPrimitiveAnimSkipResult, SeekToResult},
    time::Timestamp,
};
use futures::{StreamExt, channel::mpsc::{UnboundedReceiver, UnboundedSender}, future};
use smol::Timer;
use stdlib::registry::registry;

use crate::{services::ServiceManagerMessage, state::diagnostics::Diagnostic};

pub struct ExecutionSnapshot {
    pub current_timestamp: Timestamp,
    pub status: ExecutionStatus,
    pub slide_count: usize,
    pub slide_durations: Vec<Option<f64>>,
}

pub enum PlaybackMode {
    Presentation,
    Preview
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    Playing,
    Paused,
    Seeking,
    RuntimeError,
    CompileError
}

impl PlaybackMode {
    pub fn default_time_interval(&self) -> f64 {
        match self {
            PlaybackMode::Presentation => 1.0 / 60.0,
            PlaybackMode::Preview => 1.0 / 30.0,
        }
    }
}

pub enum ExecutionMessage {
    UpdateBytecode {
        bytecode: Option<Bytecode>,
        version: usize,
    },
    SetPlaybackMode(PlaybackMode),
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

fn default_bytecode() -> Bytecode {
    let mut section = SectionBytecode::new(SectionFlags { is_stdlib: true, is_library: true, is_init: false });
    section.instructions.push(Instruction::EndOfExecutionHead);
    Bytecode::new(vec![Arc::new(section)])
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

        let mut version = 0;
        let mut has_compiler_error = true;
        let mut executor = Executor::new(default_bytecode(), native_funcs);
        let mut target = Timestamp::default();
        let mut is_playing = false;
        let mut has_seeked_for_play = false;
        let mut playback_mode = PlaybackMode::Preview;

        let mut last_update_at = Instant::now();

        let Some(mut message) = self.rx.next().await else {
            return;
        };

        loop {
            match message {
                ExecutionMessage::UpdateBytecode { bytecode, version: nversion } => {
                    version = nversion;
                    is_playing = false;
                    if let Some(bytecode) = bytecode {
                        let old_user_timestamp = executor.internal_to_user_timestamp(target);
                        executor.update_bytecode(bytecode);
                        target = executor.user_to_internal_timestamp(old_user_timestamp);
                        has_compiler_error = false;
                    }
                    else {
                        has_compiler_error = true;
                    }
                }
                ExecutionMessage::SetPlaybackMode(ctx) => {
                    log::info!("playback mode -> {}", match ctx {
                        PlaybackMode::Presentation => "presentation",
                        PlaybackMode::Preview => "preview",
                    });
                    is_playing = false;
                    playback_mode = ctx;
                }
                ExecutionMessage::SeekTo { target: t } => {
                    log::info!("seek_to {:?}", t);
                    is_playing = false;
                    target = executor.user_to_internal_timestamp(t);
                }
                ExecutionMessage::TogglePlay => {
                    is_playing = !is_playing;
                    log::info!("playback toggled -> {}", if is_playing { "playing" } else { "paused" });
                    if is_playing {
                        last_update_at = Instant::now();
                        has_seeked_for_play = false;
                    }
                }
                ExecutionMessage::UpdateParameter => {
                    todo!("TODO")
                }
            }

            let state_update = async {

                if !is_playing || !has_seeked_for_play {
                    Self::emit_snapshot(&self.sm_tx, &executor, has_compiler_error, is_playing, true, version);

                    has_seeked_for_play = true;
                    match executor.seek_to(target).await {
                        SeekToResult::SeekedTo(reached) => {
                            target = reached;
                        }
                        SeekToResult::Error(_) => {
                            target = executor.state.timestamp;
                            is_playing = false;
                        }
                    }

                    Self::emit_snapshot(&self.sm_tx, &executor, has_compiler_error, is_playing, false, version);
                }

                while is_playing {
                    let time = Instant::now();
                    let elapsed = (time - last_update_at).as_secs_f64();
                    let target_dt = playback_mode.default_time_interval().max(elapsed);

                    let max_slide = match playback_mode {
                        PlaybackMode::Presentation => executor.state.timestamp.slide,
                        PlaybackMode::Preview => executor.total_sections(),
                    };

                    match executor.seek_primitive_anim_skip(max_slide).await {
                        SeekPrimitiveAnimSkipResult::PrimitiveAnim => {},
                        SeekPrimitiveAnimSkipResult::NoAnimsLeft => {
                            // even in presentation mode, actually advance
                            if executor.state.timestamp.slide + 1 < max_slide {
                                executor.advance_section().await;
                            }
                            is_playing = false;
                        }
                        SeekPrimitiveAnimSkipResult::Error(_) => {
                            is_playing = false;
                        }
                    }

                    // if still deciding to play
                    if is_playing {
                        match executor.advance_playback(max_slide, target_dt).await {
                            Ok(still_has_work) => {
                                if !still_has_work {
                                    is_playing = false;
                                }
                            }
                            Err(_) => is_playing = false
                        }

                        target = executor.state.timestamp;

                        Self::emit_snapshot(&self.sm_tx, &executor, has_compiler_error, is_playing, false, version);

                        let full_elapsed = Instant::now().duration_since(last_update_at).as_secs_f64();
                        last_update_at = Instant::now();
                        if target_dt > full_elapsed {
                            Timer::after(Duration::from_secs_f64(target_dt - full_elapsed)).await;
                        }
                    }
                    else {
                        Self::emit_snapshot(&self.sm_tx, &executor, has_compiler_error, is_playing, false, version);
                    }
                }
            };

            match future::select(self.rx.next(), pin!(state_update)).await {
                future::Either::Left((Some(msg), _)) => {
                    message = msg;
                }
                future::Either::Left((None, _)) => break,
                future::Either::Right((_, _)) => {
                    message = match self.rx.next().await {
                        Some(msg) => msg,
                        None => break,
                    };
                }
            }
        }
    }

    fn emit_snapshot(sm_tx: &UnboundedSender<ServiceManagerMessage>, executor: &Executor, has_compiler_error: bool, is_playing: bool, is_loading: bool, version: usize) {
        let status = if has_compiler_error {
            ExecutionStatus::CompileError
        }
        else if !executor.state.errors.is_empty() {
            ExecutionStatus::RuntimeError
        }
        else if is_loading {
            ExecutionStatus::Seeking
        }
        else if is_playing {
            ExecutionStatus::Playing
        }
        else {
            ExecutionStatus::Paused
        };

        let snapshot = ExecutionSnapshot {
            current_timestamp: executor.internal_to_user_timestamp( executor.state.timestamp),
            status,
            slide_count: executor.real_slide_count(),
            slide_durations: executor.real_slide_durations(),
        };

        sm_tx
            .unbounded_send(ServiceManagerMessage::ExecutionStateUpdated { snapshot })
            .ok();

        let diagnostics = executor.state.errors
            .iter()
            .map(|(msg, span)| Diagnostic {
                dtype: crate::state::diagnostics::DiagnosticType::RuntimeError,
                span: span.clone(),
                title: "Runtime Error".into(),
                message: msg.clone(),
            })
            .collect();

        sm_tx
            .unbounded_send(ServiceManagerMessage::UpdateRuntimeDiagnostics { diagnostics, version })
            .ok();
    }
}
