use std::{
    cell::{Cell, RefCell},
    f64,
    rc::Rc,
    time::{Duration, Instant},
};

use executor::{
    executor::{Executor, PlaybackAdvance, SeekToResult, TextRenderQuality},
    scene_snapshot::SceneSnapshot,
    time::Timestamp,
};
use futures::{
    FutureExt, StreamExt,
    channel::mpsc::UnboundedSender,
    future::{self, LocalBoxFuture},
};
use smol::Timer;
use stdlib::registry::registry;
use structs::{
    futures::yield_now,
    rope::{Rope, TextAggregate},
};

use crate::services::ServiceManagerMessage;

use super::{ExecutionMessage, ExecutionService, ParameterValue, PlaybackMode, default_bytecode};

struct SharedRuntimeState {
    target: Cell<Timestamp>,
    current_timestamp: Cell<Timestamp>,
    is_playing: Cell<bool>,
    has_compiler_error: Cell<bool>,
    has_runtime_error: Cell<bool>,
    playback_mode: Cell<PlaybackMode>,
    library_sections: Cell<usize>,
    pending_param_updates: RefCell<Vec<(String, ParameterValue)>>,
    last_update_at: Cell<Instant>,
    snapshot_requested: Cell<bool>,
}

impl SharedRuntimeState {
    fn new(executor: &Executor) -> Self {
        Self {
            target: Cell::new(executor.user_to_internal_timestamp(Timestamp::default())),
            current_timestamp: Cell::new(executor.state.timestamp),
            is_playing: Cell::new(false),
            has_compiler_error: Cell::new(true),
            has_runtime_error: Cell::new(false),
            playback_mode: Cell::new(PlaybackMode::Preview),
            library_sections: Cell::new(
                executor
                    .user_to_internal_timestamp(Timestamp::default())
                    .slide,
            ),
            pending_param_updates: RefCell::new(Vec::new()),
            last_update_at: Cell::new(Instant::now()),
            snapshot_requested: Cell::new(false),
        }
    }

    fn user_to_internal_timestamp(&self, user_ts: Timestamp) -> Timestamp {
        Timestamp {
            slide: user_ts.slide + self.library_sections.get(),
            time: user_ts.time,
        }
    }

    fn needs_work(&self) -> bool {
        self.snapshot_requested.get()
            || !self.pending_param_updates.borrow().is_empty()
            || self.is_playing.get()
            || self.target.get() != self.current_timestamp.get()
    }

    fn cancel_runtime_work(&self) {
        self.is_playing.set(false);
        self.has_runtime_error.set(true);
    }

    fn seek_requires_reset(&self, target: Timestamp) -> bool {
        target < self.current_timestamp.get()
            || target < self.target.get()
            || self.has_runtime_error.get()
    }

    fn clear_pending_parameter_updates(&self) {
        self.pending_param_updates.borrow_mut().clear();
    }
}

enum MessageEffect {
    KeepFuture,
    ResetFuture,
}

struct RuntimeState {
    version: usize,
    executor: Rc<RefCell<Executor>>,
    shared: Rc<SharedRuntimeState>,
    root_text_rope: Rope<TextAggregate>,
}

impl RuntimeState {
    fn new() -> Self {
        let executor = Executor::new(default_bytecode(), registry().func_table());
        let shared = SharedRuntimeState::new(&executor);
        Self {
            version: 0,
            executor: Rc::new(RefCell::new(executor)),
            shared: Rc::new(shared),
            root_text_rope: Rope::default(),
        }
    }

    fn requires_future_reset(&self, message: &ExecutionMessage) -> bool {
        match message {
            ExecutionMessage::UpdateBytecode { .. } | ExecutionMessage::SetPlaybackMode(_) => true,
            ExecutionMessage::SeekTo { target } => {
                let target = self.shared.user_to_internal_timestamp(*target);
                self.shared.seek_requires_reset(target)
            }
            ExecutionMessage::UpdateParameters { .. } | ExecutionMessage::TogglePlay => false,
        }
    }

    fn apply_message(&mut self, message: ExecutionMessage) -> MessageEffect {
        match message {
            ExecutionMessage::UpdateBytecode {
                bytecode,
                root_text_rope,
                version,
            } => {
                self.version = version;
                self.root_text_rope = root_text_rope;
                self.shared.is_playing.set(false);
                self.shared.has_runtime_error.set(false);
                self.shared.clear_pending_parameter_updates();

                if let Some(bytecode) = bytecode {
                    let mut executor = self.executor.borrow_mut();
                    let old_user_timestamp =
                        executor.internal_to_user_timestamp(self.shared.target.get());
                    executor.update_bytecode(bytecode);
                    self.shared.library_sections.set(
                        executor
                            .user_to_internal_timestamp(Timestamp::default())
                            .slide,
                    );
                    let target = executor.user_to_internal_timestamp(old_user_timestamp);
                    self.shared.target.set(target);
                    executor.restore_live_state_to_cache_point(target);
                    self.shared.current_timestamp.set(executor.state.timestamp);
                    self.shared.has_compiler_error.set(false);
                } else {
                    self.shared.has_compiler_error.set(true);
                }

                self.shared.snapshot_requested.set(true);
                MessageEffect::ResetFuture
            }
            ExecutionMessage::SetPlaybackMode(playback_mode) => {
                log::info!(
                    "playback mode -> {}",
                    match playback_mode {
                        PlaybackMode::Presentation => "presentation",
                        PlaybackMode::Preview => "preview",
                    }
                );
                self.shared.is_playing.set(false);
                self.shared.playback_mode.set(playback_mode);
                self.shared.has_runtime_error.set(false);

                let mut executor = self.executor.borrow_mut();
                executor.set_text_render_quality(match playback_mode {
                    PlaybackMode::Presentation => TextRenderQuality::High,
                    PlaybackMode::Preview => TextRenderQuality::Normal,
                });
                executor.clear_cache();
                let target = executor.user_to_internal_timestamp(Timestamp::default());
                self.shared.target.set(target);
                executor.restore_live_state_to_cache_point(target);
                self.shared.current_timestamp.set(executor.state.timestamp);
                self.shared.library_sections.set(
                    executor
                        .user_to_internal_timestamp(Timestamp::default())
                        .slide,
                );
                self.shared.snapshot_requested.set(true);

                MessageEffect::ResetFuture
            }
            ExecutionMessage::SeekTo { target } => {
                log::info!("seek_to {:?}", target);
                let target = self.shared.user_to_internal_timestamp(target);
                let reset_future = self.shared.seek_requires_reset(target);
                self.shared.is_playing.set(false);
                self.shared.target.set(target);
                self.shared.snapshot_requested.set(true);

                if reset_future {
                    let mut executor = self.executor.borrow_mut();
                    executor.restore_live_state_to_cache_point(target);
                    self.shared.current_timestamp.set(executor.state.timestamp);
                    self.shared.has_runtime_error.set(false);
                    MessageEffect::ResetFuture
                } else {
                    MessageEffect::KeepFuture
                }
            }
            ExecutionMessage::TogglePlay => {
                let is_playing = !self.shared.is_playing.get();
                self.shared.is_playing.set(is_playing);
                self.shared.snapshot_requested.set(true);
                log::info!(
                    "playback toggled -> {}",
                    if is_playing { "playing" } else { "paused" }
                );
                if is_playing {
                    self.shared.last_update_at.set(Instant::now());
                }
                MessageEffect::KeepFuture
            }
            ExecutionMessage::UpdateParameters { updates } => {
                self.shared
                    .pending_param_updates
                    .borrow_mut()
                    .extend(updates);
                self.shared.snapshot_requested.set(true);
                MessageEffect::KeepFuture
            }
        }
    }

    fn play_session(
        &self,
        sm_tx: UnboundedSender<ServiceManagerMessage>,
    ) -> LocalBoxFuture<'static, ()> {
        let executor = Rc::clone(&self.executor);
        let shared = Rc::clone(&self.shared);
        let root_text_rope = self.root_text_rope.clone();
        let version = self.version;

        async move {
            let mut executor = executor.borrow_mut();

            loop {
                run_play_session_iteration(
                    &mut executor,
                    &shared,
                    &sm_tx,
                    &root_text_rope,
                    version,
                )
                .await;
                yield_now().await;
            }
        }
        .boxed_local()
    }
}

async fn run_play_session_iteration(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    sm_tx: &UnboundedSender<ServiceManagerMessage>,
    root_text_rope: &Rope<TextAggregate>,
    version: usize,
) {
    if apply_pending_parameter_updates(executor, shared) {
        shared.snapshot_requested.set(true);
    }

    if shared.has_compiler_error.get() {
        if shared.snapshot_requested.get() {
            shared.current_timestamp.set(shared.target.get());
            emit_runtime_snapshot(
                executor,
                shared,
                sm_tx,
                root_text_rope,
                version,
                false,
                None,
            )
            .await;
            shared.snapshot_requested.set(false);
        }
        return;
    }

    clamp_target_to_valid_timestamp(executor, shared);
    shared.current_timestamp.set(executor.state.timestamp);

    if executor.state.has_errors() {
        shared.cancel_runtime_work();
        if shared.snapshot_requested.get() {
            emit_runtime_snapshot(
                executor,
                shared,
                sm_tx,
                root_text_rope,
                version,
                false,
                None,
            )
            .await;
            shared.snapshot_requested.set(false);
        }
        return;
    }

    let current = executor.state.timestamp;
    let target = shared.target.get();

    if target != current {
        sync_to_target(
            executor,
            shared,
            sm_tx,
            root_text_rope,
            version,
            target,
            current,
        )
        .await;
        shared.snapshot_requested.set(false);
        return;
    }

    if shared.snapshot_requested.get() {
        let scene_snapshot = capture_scene_snapshot(executor, shared)
            .await
            .ok()
            .flatten();
        emit_runtime_snapshot(
            executor,
            shared,
            sm_tx,
            root_text_rope,
            version,
            false,
            scene_snapshot,
        )
        .await;
        shared.snapshot_requested.set(false);
        return;
    }

    if shared.is_playing.get() {
        playback_iteration(executor, shared, sm_tx, root_text_rope, version).await;
    }
}

fn apply_pending_parameter_updates(executor: &mut Executor, shared: &SharedRuntimeState) -> bool {
    let updates = shared.pending_param_updates.take();
    let applied_parameters = !updates.is_empty();

    for (name, value) in updates {
        let Some(value) = ExecutionService::runtime_value_from_parameter(&value) else {
            log::warn!("parameter update failed for {}: unsupported value", name);
            continue;
        };
        if let Err(error) = executor.update_parameter(&name, value) {
            log::warn!("parameter update failed for {}: {}", name, error);
        }
    }

    applied_parameters
}

async fn sync_to_target(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    sm_tx: &UnboundedSender<ServiceManagerMessage>,
    root_text_rope: &Rope<TextAggregate>,
    version: usize,
    target: Timestamp,
    current: Timestamp,
) {
    emit_runtime_snapshot(executor, shared, sm_tx, root_text_rope, version, true, None).await;

    let result = if target < current {
        executor.seek_to(target).await
    } else {
        executor.advance_to_target(target).await
    };

    let target_superseded = shared.target.get() != target;
    match result {
        SeekToResult::SeekedTo(reached) => {
            shared.current_timestamp.set(reached);
            if !target_superseded {
                shared.target.set(reached);
            }

            if executor.state.has_errors() {
                shared.cancel_runtime_work();
            }
        }
        SeekToResult::Error(_) => {
            shared.cancel_runtime_work();
        }
    }

    if target_superseded {
        return;
    }

    let scene_snapshot = capture_scene_snapshot(executor, shared)
        .await
        .ok()
        .flatten();
    emit_runtime_snapshot(
        executor,
        shared,
        sm_tx,
        root_text_rope,
        version,
        false,
        scene_snapshot,
    )
    .await;
}

async fn playback_iteration(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    sm_tx: &UnboundedSender<ServiceManagerMessage>,
    root_text_rope: &Rope<TextAggregate>,
    version: usize,
) {
    let tick_started_at = Instant::now();
    let elapsed = tick_started_at
        .duration_since(shared.last_update_at.get())
        .as_secs_f64();
    let frame_interval =
        Duration::from_secs_f64(shared.playback_mode.get().default_time_interval());
    let max_slide = max_slide(executor, shared.playback_mode.get());

    shared.last_update_at.set(tick_started_at);

    emit_runtime_snapshot(executor, shared, sm_tx, root_text_rope, version, true, None).await;

    match executor.advance_playback(max_slide, elapsed).await {
        Ok(PlaybackAdvance::Advanced) => {}
        Ok(PlaybackAdvance::PreparedSection) => {
            shared.last_update_at.set(Instant::now());
        }
        Ok(PlaybackAdvance::Finished) => {
            shared.is_playing.set(false);
        }
        Err(_) => {
            shared.cancel_runtime_work();
        }
    }

    shared.current_timestamp.set(executor.state.timestamp);
    shared.target.set(executor.state.timestamp);

    let scene_snapshot = capture_scene_snapshot(executor, shared)
        .await
        .ok()
        .flatten();
    emit_runtime_snapshot(
        executor,
        shared,
        sm_tx,
        root_text_rope,
        version,
        false,
        scene_snapshot,
    )
    .await;
    shared.snapshot_requested.set(false);

    let frame_elapsed = tick_started_at.elapsed();
    if shared.is_playing.get() && frame_interval > frame_elapsed {
        Timer::after(frame_interval - frame_elapsed).await;
    }
}

fn max_slide(executor: &Executor, playback_mode: PlaybackMode) -> usize {
    match playback_mode {
        PlaybackMode::Presentation if executor.state.timestamp.time.is_infinite() => {
            (executor.state.timestamp.slide + 1).min(executor.total_sections())
        }
        PlaybackMode::Presentation => executor.state.timestamp.slide,
        PlaybackMode::Preview => executor.total_sections(),
    }
}

fn clamp_target_to_valid_timestamp(executor: &Executor, shared: &SharedRuntimeState) {
    let min = executor.user_to_internal_timestamp(Timestamp::default());
    let mut target = shared.target.get();
    if target <= min {
        target = min;
    }

    if target.slide >= executor.total_sections() {
        target.slide = executor.total_sections() - 1;
        target.time = f64::INFINITY;
    }

    shared.target.set(target);
}

async fn capture_scene_snapshot(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
) -> Result<Option<SceneSnapshot>, ()> {
    if shared.has_compiler_error.get() || executor.state.has_errors() {
        return Ok(None);
    }

    shared.snapshot_requested.set(true);
    match ExecutionService::capture_stable_scene_snapshot(executor).await {
        Ok(scene_snapshot) => Ok(Some(scene_snapshot)),
        Err(_) => {
            shared.cancel_runtime_work();
            Err(())
        }
    }
}

async fn emit_runtime_snapshot(
    executor: &Executor,
    shared: &SharedRuntimeState,
    sm_tx: &UnboundedSender<ServiceManagerMessage>,
    root_text_rope: &Rope<TextAggregate>,
    version: usize,
    is_loading: bool,
    scene_snapshot: Option<SceneSnapshot>,
) {
    let current_timestamp = display_timestamp_for_target(executor, shared.target.get());

    ExecutionService::emit_snapshot(
        sm_tx,
        executor,
        root_text_rope,
        current_timestamp,
        shared.has_compiler_error.get(),
        shared.is_playing.get(),
        is_loading,
        shared.playback_mode.get(),
        version,
        scene_snapshot,
    )
    .await;
}

fn display_timestamp_for_target(executor: &Executor, target: Timestamp) -> Timestamp {
    executor.internal_to_user_timestamp(target)
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
        let mut play_future = Some(runtime.play_session(self.sm_tx.clone()));

        loop {
            let Some(message) = self
                .next_message(&runtime, play_future.as_mut().expect("play future"))
                .await
            else {
                break;
            };

            let reset_future = runtime.requires_future_reset(&message);
            if reset_future {
                drop(play_future.take());
            }

            let effect = runtime.apply_message(message);
            debug_assert!(matches!(
                (reset_future, effect),
                (true, MessageEffect::ResetFuture) | (false, MessageEffect::KeepFuture)
            ));

            if reset_future {
                play_future = Some(runtime.play_session(self.sm_tx.clone()));
            }
        }
    }

    async fn next_message(
        &mut self,
        runtime: &RuntimeState,
        play_future: &mut LocalBoxFuture<'static, ()>,
    ) -> Option<ExecutionMessage> {
        future::poll_fn(|cx| {
            if let std::task::Poll::Ready(message) = self.rx.poll_next_unpin(cx) {
                return std::task::Poll::Ready(message);
            }

            if runtime.shared.needs_work() {
                let _ = play_future.as_mut().poll(cx);
            }

            std::task::Poll::Pending
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use executor::{state::ExecutionState, time::Timestamp, value::Value};
    use structs::rope::{Rope, TextAggregate};

    use super::{RuntimeState, default_bytecode};
    use crate::services::execution::{ExecutionMessage, ParameterValue};

    #[test]
    fn update_bytecode_restores_live_executor_state_to_cache_point() {
        let mut runtime = RuntimeState::new();

        let child = runtime
            .executor
            .borrow_mut()
            .state
            .alloc_stack((0, 0), Some(ExecutionState::ROOT_STACK_IDX), None)
            .expect("child stack");
        runtime
            .executor
            .borrow_mut()
            .state
            .stack_mut(child)
            .push(Value::Integer(7));
        runtime
            .executor
            .borrow_mut()
            .state
            .execution_heads
            .insert(child);

        runtime.apply_message(ExecutionMessage::UpdateBytecode {
            bytecode: Some(default_bytecode()),
            root_text_rope: Rope::<TextAggregate>::default(),
            version: 1,
        });

        let executor = runtime.executor.borrow();
        assert_eq!(executor.state.alive_stack_count, 1);
        assert_eq!(
            executor
                .state
                .execution_heads
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![ExecutionState::ROOT_STACK_IDX]
        );
        assert_eq!(
            executor
                .state
                .stack(ExecutionState::ROOT_STACK_IDX)
                .stack_len(),
            0
        );
    }

    #[test]
    fn forward_seek_keeps_play_session_future() {
        let runtime = RuntimeState::new();
        let current = runtime
            .shared
            .user_to_internal_timestamp(Timestamp::new(1, 1.0));
        runtime.shared.current_timestamp.set(current);
        runtime.shared.target.set(current);

        assert!(!runtime.requires_future_reset(&ExecutionMessage::SeekTo {
            target: Timestamp::new(1, 2.0),
        }));
    }

    #[test]
    fn backward_seek_resets_play_session_future() {
        let runtime = RuntimeState::new();
        let current = runtime
            .shared
            .user_to_internal_timestamp(Timestamp::new(1, 1.0));
        runtime.shared.current_timestamp.set(current);
        runtime.shared.target.set(current);

        assert!(runtime.requires_future_reset(&ExecutionMessage::SeekTo {
            target: Timestamp::new(1, 0.5),
        }));
    }

    #[test]
    fn parameter_updates_keep_play_session_future() {
        let runtime = RuntimeState::new();
        let message = ExecutionMessage::UpdateParameters {
            updates: HashMap::from([("speed".into(), ParameterValue::Float(2.0))]),
        };

        assert!(!runtime.requires_future_reset(&message));
    }

    #[test]
    fn runtime_error_recovery_seek_resets_play_session_future() {
        let runtime = RuntimeState::new();
        runtime.shared.has_runtime_error.set(true);

        assert!(runtime.requires_future_reset(&ExecutionMessage::SeekTo {
            target: Timestamp::new(1, 2.0),
        }));
    }

    #[test]
    fn scene_snapshot_capture_keeps_runtime_pollable_until_snapshot_emits() {
        let runtime = RuntimeState::new();
        runtime.shared.has_compiler_error.set(false);
        let current = runtime.executor.borrow().state.timestamp;
        runtime.shared.current_timestamp.set(current);
        runtime.shared.target.set(current);
        assert!(!runtime.shared.needs_work());

        smol::block_on(async {
            let mut executor = runtime.executor.borrow_mut();
            super::capture_scene_snapshot(&mut executor, &runtime.shared)
                .await
                .expect("snapshot capture should not record a runtime error")
                .expect("default scene snapshot should be available");
        });

        assert!(runtime.shared.needs_work());
    }
}
