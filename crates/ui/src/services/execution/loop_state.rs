use std::{
    f64,
    pin::pin,
    time::{Duration, Instant},
};

use executor::{
    executor::{Executor, SeekPrimitiveAnimSkipResult, SeekToResult},
    scene_snapshot::SceneSnapshot,
    time::Timestamp,
};
use futures::channel::mpsc::UnboundedSender;
use futures::{StreamExt, future};
use smol::Timer;
use stdlib::registry::registry;
use structs::rope::{Rope, TextAggregate};

use crate::services::ServiceManagerMessage;

use super::{ExecutionMessage, ExecutionService, PlaybackMode, default_bytecode};

struct RuntimeState {
    version: usize,
    has_compiler_error: bool,
    executor: Executor,
    target: Timestamp,
    is_playing: bool,
    has_seeked_for_play: bool,
    playback_mode: PlaybackMode,
    root_text_rope: Rope<TextAggregate>,
    last_update_at: Instant,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            version: 0,
            has_compiler_error: true,
            executor: Executor::new(default_bytecode(), registry().func_table()),
            target: Timestamp::default(),
            is_playing: false,
            has_seeked_for_play: false,
            playback_mode: PlaybackMode::Preview,
            root_text_rope: Rope::default(),
            last_update_at: Instant::now(),
        }
    }

    fn apply_message(&mut self, message: ExecutionMessage) {
        match message {
            ExecutionMessage::UpdateBytecode {
                bytecode,
                root_text_rope,
                version,
            } => {
                self.version = version;
                self.is_playing = false;
                self.root_text_rope = root_text_rope;
                if let Some(bytecode) = bytecode {
                    let old_user_timestamp = self.executor.internal_to_user_timestamp(self.target);
                    self.executor.update_bytecode(bytecode);
                    self.target = self.executor.user_to_internal_timestamp(old_user_timestamp);
                    self.has_compiler_error = false;
                } else {
                    self.has_compiler_error = true;
                }
            }
            ExecutionMessage::SetPlaybackMode(playback_mode) => {
                log::info!(
                    "playback mode -> {}",
                    match playback_mode {
                        PlaybackMode::Presentation => "presentation",
                        PlaybackMode::Preview => "preview",
                    }
                );
                self.is_playing = false;
                self.playback_mode = playback_mode;
                self.executor.clear_cache();
                self.target = self
                    .executor
                    .user_to_internal_timestamp(Timestamp::default());
                self.has_seeked_for_play = false;
            }
            ExecutionMessage::SeekTo { target } => {
                log::info!("seek_to {:?}", target);
                self.is_playing = false;
                self.target = self.executor.user_to_internal_timestamp(target);
            }
            ExecutionMessage::TogglePlay => {
                self.is_playing = !self.is_playing;
                log::info!(
                    "playback toggled -> {}",
                    if self.is_playing { "playing" } else { "paused" }
                );
                if self.is_playing {
                    self.last_update_at = Instant::now();
                    self.has_seeked_for_play = false;
                }
            }
            ExecutionMessage::UpdateParameters { updates } => {
                for (name, value) in updates {
                    let Some(value) = ExecutionService::runtime_value_from_parameter(&value) else {
                        log::warn!("parameter update failed for {}: unsupported value", name);
                        continue;
                    };
                    if let Err(error) = self.executor.update_parameter(&name, value) {
                        log::warn!("parameter update failed for {}: {}", name, error);
                    }
                }
            }
        }
    }

    async fn sync_to_target(&mut self, sm_tx: &UnboundedSender<ServiceManagerMessage>) {
        self.clamp_target_to_valid_timestamp();
        self.emit_snapshot(sm_tx, true, None).await;

        self.has_seeked_for_play = true;
        if self.has_compiler_error {
            return;
        }

        match self.executor.seek_to(self.target).await {
            SeekToResult::SeekedTo(reached) => {
                if self.executor.state.has_errors() {
                    self.cancel_runtime_work();
                } else {
                    self.target = reached;
                }
            }
            SeekToResult::Error(_) => {
                self.cancel_runtime_work();
            }
        }

        let scene_snapshot = self.capture_scene_snapshot().await.ok().flatten();
        self.emit_snapshot(sm_tx, false, scene_snapshot).await;
    }

    async fn run_playback_until_paused(&mut self, sm_tx: &UnboundedSender<ServiceManagerMessage>) {
        while self.is_playing {
            self.playback_iteration(sm_tx).await;
        }
    }

    async fn playback_iteration(&mut self, sm_tx: &UnboundedSender<ServiceManagerMessage>) {
        let tick_started_at = Instant::now();
        let elapsed = (tick_started_at - self.last_update_at).as_secs_f64();
        let target_dt = self.playback_mode.default_time_interval().max(elapsed);
        let max_slide = self.max_slide();

        match self.executor.seek_primitive_anim_skip(max_slide).await {
            SeekPrimitiveAnimSkipResult::PrimitiveAnim => {}
            SeekPrimitiveAnimSkipResult::NoAnimsLeft => {
                self.advance_section_without_materializing().await;
                self.is_playing = false;
            }
            SeekPrimitiveAnimSkipResult::Error(_) => {
                self.cancel_runtime_work();
            }
        }

        if self.is_playing {
            match self.executor.advance_playback(max_slide, target_dt).await {
                Ok(true) => {}
                Ok(false) => {
                    self.is_playing = false;
                }
                Err(_) => {
                    self.cancel_runtime_work();
                }
            }
        }

        self.refresh_target_from_executor();

        let scene_snapshot = self.capture_scene_snapshot().await.ok().flatten();
        self.emit_snapshot(sm_tx, false, scene_snapshot).await;

        let full_elapsed = Instant::now()
            .duration_since(self.last_update_at)
            .as_secs_f64();
        self.last_update_at = Instant::now();
        if self.is_playing && target_dt > full_elapsed {
            Timer::after(Duration::from_secs_f64(target_dt - full_elapsed)).await;
        }
    }

    fn max_slide(&self) -> usize {
        match self.playback_mode {
            PlaybackMode::Presentation => self.executor.state.timestamp.slide,
            PlaybackMode::Preview => self.executor.total_sections(),
        }
    }

    fn clamp_target_to_valid_timestamp(&mut self) {
        if self.target.slide >= self.executor.total_sections() {
            self.target.slide = self.executor.total_sections() - 1;
            self.target.time = f64::INFINITY;
        }
    }

    fn cancel_runtime_work(&mut self) {
        self.is_playing = false;
    }

    fn refresh_target_from_executor(&mut self) {
        if !self.has_compiler_error {
            self.target = self.executor.state.timestamp;
        }
    }

    async fn advance_section_without_materializing(&mut self) {
        if self.executor.state.has_errors() {
            self.cancel_runtime_work();
            return;
        }

        if self.executor.state.timestamp.slide + 1 >= self.executor.total_sections() {
            return;
        }

        self.executor.advance_section().await;
        if self.executor.state.has_errors() {
            self.cancel_runtime_work();
        }
    }

    async fn capture_scene_snapshot(&mut self) -> Result<Option<SceneSnapshot>, ()> {
        if self.has_compiler_error || self.executor.state.has_errors() {
            return Ok(None);
        }

        match ExecutionService::capture_stable_scene_snapshot(&mut self.executor).await {
            Ok(scene_snapshot) => Ok(Some(scene_snapshot)),
            Err(_) => {
                self.cancel_runtime_work();
                Err(())
            }
        }
    }

    async fn emit_snapshot(
        &mut self,
        sm_tx: &UnboundedSender<ServiceManagerMessage>,
        is_loading: bool,
        scene_snapshot: Option<SceneSnapshot>,
    ) {
        let mut current_timestamp = self.executor.internal_to_user_timestamp(self.target);
        if current_timestamp.time.is_infinite() {
            debug_assert_eq!(self.target.slide, self.executor.total_sections() - 1);
            current_timestamp.time = self
                .executor
                .real_slide_durations()
                .last()
                .copied()
                .flatten()
                .or(self
                    .executor
                    .real_minimum_slide_durations()
                    .last()
                    .copied()
                    .flatten())
                .unwrap_or_default();
        }

        ExecutionService::emit_snapshot(
            sm_tx,
            &self.executor,
            &self.root_text_rope,
            current_timestamp,
            self.has_compiler_error,
            self.is_playing,
            is_loading,
            self.playback_mode,
            self.version,
            scene_snapshot,
        )
        .await;
    }
}

impl ExecutionService {
    pub fn new(
        rx: futures::channel::mpsc::UnboundedReceiver<ExecutionMessage>,
        sm_tx: futures::channel::mpsc::UnboundedSender<crate::services::ServiceManagerMessage>,
    ) -> Self {
        Self { rx, sm_tx }
    }

    pub fn run(self) {
        std::thread::spawn(move || {
            smol::block_on(self.run_loop());
        });
    }

    async fn run_loop(mut self) {
        let mut runtime = RuntimeState::new();
        let sm_tx = self.sm_tx.clone();

        let Some(mut message) = self.rx.next().await else {
            return;
        };

        loop {
            runtime.apply_message(message);

            let state_update = async {
                if !runtime.is_playing || !runtime.has_seeked_for_play {
                    runtime.sync_to_target(&sm_tx).await;
                }
                runtime.run_playback_until_paused(&sm_tx).await;
            };

            match future::select(self.rx.next(), pin!(state_update)).await {
                future::Either::Left((Some(next_message), _)) => {
                    message = next_message;
                }
                future::Either::Left((None, _)) => break,
                future::Either::Right((_, _)) => {
                    message = match self.rx.next().await {
                        Some(next_message) => next_message,
                        None => break,
                    };
                }
            }
        }
    }
}
