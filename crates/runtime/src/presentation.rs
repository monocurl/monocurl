use std::{collections::VecDeque, time::Duration};

use executor::{
    error::{ExecutorError, RuntimeError},
    executor::{Executor, LiveCheckpoint, PlaybackAdvance, SeekToResult},
    scene_snapshot::SceneSnapshot,
    time::Timestamp,
};

use super::{
    ExecutionSnapshot, ExecutionStatus, LoadingSnapshotSink, ParameterSnapshot, PlaybackEngine,
    PlaybackMode, RuntimeIteration, SharedRuntimeState, apply_pending_parameter_updates,
    clamp_target_to_valid_timestamp, max_slide, parameter_snapshot, runtime_snapshot,
};

/// presentation pre-rendering tunables.
const PRESENTATION_LOOKAHEAD_SECONDS: f64 = 6.0;
/// frames the producer is allowed to render per scheduler tick before yielding
/// back so consumer emission and incoming commands can interleave.
const PRESENTATION_PRODUCE_BATCH: usize = 1;

/// a single optimistically pre-rendered presentation frame. holds only what the
/// viewport needs to draw plus the parameter state so animating param sliders track.
pub(super) struct PreRenderedFrame {
    pub(super) timestamp: Timestamp,
    pub(super) scene: SceneSnapshot,
    pub(super) parameters: ParameterSnapshot,
    pub(super) completed_slide_duration: Option<f64>,
}

/// presentation-mode playback state: one live producer (the executor) running
/// ahead of the consumer, a bounded queue of pre-rendered frames, and a parked
/// "trailing reference" the producer can be reset to on a param update.
pub(super) struct PresentationState {
    /// pre-rendered frames for `(consumer_timestamp, producer_frontier]`, in order
    pub(super) queue: VecDeque<PreRenderedFrame>,
    /// parked reference at `<= consumer_timestamp` with all applied param updates;
    /// absent while paused slider updates have suspended optimistic production
    pub(super) trailing: Option<LiveCheckpoint>,
    /// the frame currently shown by the viewport (the consumer position)
    pub(super) display: Option<PreRenderedFrame>,
    /// internal timestamp of `display`
    pub(super) consumer_timestamp: Timestamp,
    /// wall-clock anchor + frames consumed since, used to map real time -> frames
    pub(super) play_anchor_wall: f64,
    pub(super) consumed_frames: u64,
    /// a speculative producer error, surfaced only once the consumer drains to it
    pub(super) deferred_error: Option<(Timestamp, Vec<RuntimeError>)>,
    /// producer reached the scene end or a quarantined error
    pub(super) producer_done: bool,
    /// producer is parked at a slide boundary until the consumer reaches it
    pub(super) producer_blocked_at_boundary: bool,
    /// paused parameter updates keep the live head at the displayed timestamp
    /// and delay rebuilding the optimistic baseline until playback resumes
    pub(super) production_suspended_by_param_update: bool,
    /// the display must be (re)built at `consumer_timestamp` (after a seek/reset)
    pub(super) needs_establish: bool,
}

impl PresentationState {
    pub(super) fn new(consumer_timestamp: Timestamp) -> Self {
        Self {
            queue: VecDeque::new(),
            trailing: None,
            display: None,
            consumer_timestamp,
            play_anchor_wall: 0.0,
            consumed_frames: 0,
            deferred_error: None,
            producer_done: false,
            producer_blocked_at_boundary: false,
            production_suspended_by_param_update: false,
            needs_establish: true,
        }
    }

    pub(super) fn frame_dt() -> f64 {
        PlaybackMode::Presentation.default_time_interval()
    }

    pub(super) fn lookahead_frames() -> usize {
        (PRESENTATION_LOOKAHEAD_SECONDS / Self::frame_dt()).ceil() as usize
    }

    pub(super) fn reset(&mut self, consumer_timestamp: Timestamp, now: f64) {
        self.queue.clear();
        self.trailing = None;
        self.display = None;
        self.consumer_timestamp = consumer_timestamp;
        self.play_anchor_wall = now;
        self.consumed_frames = 0;
        self.deferred_error = None;
        self.producer_done = false;
        self.producer_blocked_at_boundary = false;
        self.production_suspended_by_param_update = false;
        self.needs_establish = true;
    }

    fn reanchor_clock(&mut self, now: f64) {
        self.play_anchor_wall = now;
        self.consumed_frames = 0;
    }

    pub(super) fn has_pending_production(&self) -> bool {
        !self.producer_done
            && !self.producer_blocked_at_boundary
            && !self.production_suspended_by_param_update
            && self.deferred_error.is_none()
            && self.queue.len() < Self::lookahead_frames()
    }

    fn buffered(&self) -> bool {
        !self.queue.is_empty()
    }
}

pub(super) struct PresentationEngine;

impl PlaybackEngine for PresentationEngine {
    async fn run_iteration(
        executor: &mut Executor,
        shared: &SharedRuntimeState,
        now_seconds: f64,
        iteration: &mut RuntimeIteration,
        loading_snapshot_sink: &mut LoadingSnapshotSink<'_>,
    ) {
        let mut pres = shared.presentation.borrow_mut();
        presentation_iteration_body(
            executor,
            shared,
            &mut pres,
            now_seconds,
            iteration,
            loading_snapshot_sink,
        )
        .await;

        // pin reported/seek timestamps to the consumer so idle states don't spin
        // `needs_work` and a playing consumer doesn't re-trigger establish, and mirror
        // remaining producer work into the Cell `needs_work` reads.
        shared.current_timestamp.set(pres.consumer_timestamp);
        shared.target.set(pres.consumer_timestamp);
        shared.presentation_pending.set(
            !shared.has_compiler_error.get()
                && !shared.has_runtime_error.get()
                && (pres.needs_establish || pres.has_pending_production()),
        );
    }
}

async fn presentation_iteration_body(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    pres: &mut PresentationState,
    now_seconds: f64,
    iteration: &mut RuntimeIteration,
    loading_snapshot_sink: &mut LoadingSnapshotSink<'_>,
) {
    if shared.has_compiler_error.get() {
        if shared.snapshot_requested.get() {
            shared.current_timestamp.set(pres.consumer_timestamp);
            iteration
                .snapshots
                .push(runtime_snapshot(executor, shared, false, None));
            shared.snapshot_requested.set(false);
        }
        return;
    }

    if shared.has_runtime_error.get() {
        if shared.snapshot_requested.get() {
            iteration
                .snapshots
                .push(presentation_display_snapshot(executor, shared, pres));
            shared.snapshot_requested.set(false);
        }
        return;
    }

    clamp_target_to_valid_timestamp(executor, shared);

    // (re)establish the displayed frame at the consumer position after a seek/reset.
    // playback-driven consumer advance is NOT a seek, so only `needs_establish` triggers this.
    if pres.needs_establish {
        let seek_target = shared.target.get();
        presentation_establish_display(
            executor,
            shared,
            pres,
            seek_target,
            now_seconds,
            iteration,
            loading_snapshot_sink,
        )
        .await;
        if shared.has_runtime_error.get() {
            shared.snapshot_requested.set(false);
            return;
        }
    }

    // service parameter updates with trailing-reference recovery
    if !shared.pending_param_updates.borrow().is_empty() {
        presentation_apply_param_updates(executor, shared, pres, iteration).await;
        if shared.has_runtime_error.get() {
            shared.snapshot_requested.set(false);
            return;
        }
    }
    let resume_production = shared.presentation_resume_production.replace(false);
    if resume_production {
        pres.production_suspended_by_param_update = false;
    }
    presentation_unblock_producer_at_consumer_boundary(executor, pres);

    if shared.is_playing.get() {
        // re-anchor right before consuming so the play-start frame maps to index 0,
        // even if this iteration began (with a now-stale clock) while still paused.
        if shared.presentation_reanchor.replace(false) {
            pres.reanchor_clock(now_seconds);
        }
        let had_buffer = pres.buffered();
        presentation_consume(executor, shared, pres, now_seconds, iteration).await;

        // If playback is consuming an optimistic buffer, keep the producer moving in
        // fixed 60fps steps toward the lookahead horizon. Once the consumer catches
        // the producer, leave this tick in real-time catch-up mode.
        if shared.is_playing.get() && (had_buffer || pres.buffered() || resume_production) {
            presentation_produce(executor, pres).await;
        }
    } else {
        presentation_produce(executor, pres).await;
    }

    presentation_handle_drained(executor, shared, pres, iteration);

    if shared.snapshot_requested.get() {
        iteration
            .snapshots
            .push(presentation_display_snapshot(executor, shared, pres));
        shared.snapshot_requested.set(false);
    }

    if shared.is_playing.get() {
        iteration.next_frame_interval =
            Some(Duration::from_secs_f64(PresentationState::frame_dt()));
    }
}

/// advance the producer head from a restored cache point up to `consumer_target`,
/// capture the displayed frame there, and park the trailing reference.
#[allow(clippy::too_many_arguments)]
async fn presentation_establish_display(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    pres: &mut PresentationState,
    consumer_target: Timestamp,
    now_seconds: f64,
    iteration: &mut RuntimeIteration,
    loading_snapshot_sink: &mut LoadingSnapshotSink<'_>,
) {
    // a backward seek (or a clamp after the slide count shrank) can leave the
    // restored head ahead of the target; rebase to a cache point we can advance from.
    if executor.state.timestamp > consumer_target {
        executor.restore_live_state_to_cache_point(consumer_target);
    }

    if executor.state.timestamp != consumer_target {
        loading_snapshot_sink
            .emit_or_defer(iteration, runtime_snapshot(executor, shared, true, None));
        match executor.advance_to_target(consumer_target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(_) => {
                shared.cancel_runtime_work();
                iteration
                    .snapshots
                    .push(runtime_snapshot(executor, shared, false, None));
                return;
            }
        }
    }

    pres.consumer_timestamp = executor.state.timestamp;
    pres.queue.clear();
    pres.producer_done = false;
    pres.producer_blocked_at_boundary = false;
    pres.production_suspended_by_param_update = false;
    pres.deferred_error = None;
    pres.reanchor_clock(now_seconds);

    if !presentation_capture_display(executor, shared, pres).await {
        iteration
            .snapshots
            .push(runtime_snapshot(executor, shared, false, None));
        return;
    }

    pres.trailing = Some(executor.capture_live_checkpoint());
    pres.needs_establish = false;

    shared.current_timestamp.set(pres.consumer_timestamp);
    shared.target.set(pres.consumer_timestamp);
    iteration
        .snapshots
        .push(presentation_display_snapshot(executor, shared, pres));
}

/// bring the live head back to the consumer position (restoring the trailing
/// reference + reseeking when the producer ran ahead), apply the pending updates,
/// recapture the display, then either re-park the trailing reference or suspend
/// paused lookahead until playback resumes.
async fn presentation_apply_param_updates(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    pres: &mut PresentationState,
    iteration: &mut RuntimeIteration,
) {
    let producer_ahead =
        !pres.queue.is_empty() || executor.state.timestamp != pres.consumer_timestamp;
    if producer_ahead {
        match &pres.trailing {
            Some(trailing) => executor.restore_live_checkpoint(trailing),
            None => executor.restore_live_state_to_cache_point(pres.consumer_timestamp),
        }
        if executor.state.timestamp != pres.consumer_timestamp {
            match executor.advance_to_target(pres.consumer_timestamp).await {
                SeekToResult::SeekedTo(_) => {}
                SeekToResult::Error(_) => {
                    shared.cancel_runtime_work();
                    iteration
                        .snapshots
                        .push(runtime_snapshot(executor, shared, false, None));
                    return;
                }
            }
        }
    }

    apply_pending_parameter_updates(executor, shared);
    if executor.state.has_errors() {
        shared.cancel_runtime_work();
        iteration
            .snapshots
            .push(runtime_snapshot(executor, shared, false, None));
        return;
    }

    pres.queue.clear();
    pres.producer_done = false;
    pres.producer_blocked_at_boundary = false;
    pres.deferred_error = None;
    pres.production_suspended_by_param_update = false;

    if !presentation_capture_display(executor, shared, pres).await {
        iteration
            .snapshots
            .push(runtime_snapshot(executor, shared, false, None));
        return;
    }

    if shared.is_playing.get() {
        // "copy the optimistic head": the trailing reference must include the update
        pres.trailing = Some(executor.capture_live_checkpoint());
    } else {
        // keep paused slider drags cheap: stay at the displayed timestamp and
        // delay the full heap checkpoint until lookahead is needed again.
        pres.trailing = None;
        pres.production_suspended_by_param_update = true;
    }

    shared.current_timestamp.set(pres.consumer_timestamp);
    iteration
        .snapshots
        .push(presentation_display_snapshot(executor, shared, pres));
}

/// render up to a bounded batch of look-ahead frames from the live producer head.
async fn presentation_produce(executor: &mut Executor, pres: &mut PresentationState) {
    let dt = PresentationState::frame_dt();
    let mut produced = 0;

    if !pres.has_pending_production() {
        return;
    }
    presentation_ensure_trailing_checkpoint(executor, pres);

    while pres.has_pending_production() && produced < PRESENTATION_PRODUCE_BATCH {
        if presentation_producer_at_scene_end(executor) {
            pres.producer_done = true;
            break;
        }
        if presentation_producer_waiting_for_consumer_boundary(executor, pres) {
            pres.producer_blocked_at_boundary = true;
            break;
        }

        let max_slide = presentation_producer_max_slide(executor, pres);
        match executor.produce_frame(max_slide, dt).await {
            Ok((advance, scene, completed_slide_duration)) => {
                pres.producer_blocked_at_boundary = false;
                let parameters = parameter_snapshot(executor);
                pres.queue.push_back(PreRenderedFrame {
                    timestamp: executor.state.timestamp,
                    scene,
                    parameters,
                    completed_slide_duration,
                });
                produced += 1;
                if matches!(advance, PlaybackAdvance::Finished) {
                    if presentation_producer_at_scene_end(executor) {
                        pres.producer_done = true;
                    } else if presentation_producer_waiting_for_consumer_boundary(executor, pres) {
                        pres.producer_blocked_at_boundary = true;
                    }
                }
            }
            Err(error) => {
                presentation_quarantine_producer_error(executor, pres, error);
            }
        }
    }
}

fn presentation_ensure_trailing_checkpoint(executor: &Executor, pres: &mut PresentationState) {
    if pres.trailing.is_none() && executor.state.timestamp == pres.consumer_timestamp {
        pres.trailing = Some(executor.capture_live_checkpoint());
    }
}

pub(super) fn presentation_producer_max_slide(
    executor: &Executor,
    pres: &PresentationState,
) -> usize {
    if presentation_producer_can_cross_boundary(executor, pres) {
        (executor.state.timestamp.slide + 1).min(executor.total_sections())
    } else {
        executor.state.timestamp.slide
    }
}

fn presentation_producer_at_scene_end(executor: &Executor) -> bool {
    executor.state.timestamp.time.is_infinite()
        && executor.state.timestamp.slide + 1 >= executor.total_sections()
}

fn presentation_producer_can_cross_boundary(executor: &Executor, pres: &PresentationState) -> bool {
    executor.state.timestamp.time.is_infinite()
        && pres.consumer_timestamp.time.is_infinite()
        && pres.consumer_timestamp.slide == executor.state.timestamp.slide
}

fn presentation_producer_waiting_for_consumer_boundary(
    executor: &Executor,
    pres: &PresentationState,
) -> bool {
    executor.state.timestamp.time.is_infinite()
        && !presentation_producer_at_scene_end(executor)
        && !presentation_producer_can_cross_boundary(executor, pres)
}

fn presentation_unblock_producer_at_consumer_boundary(
    executor: &Executor,
    pres: &mut PresentationState,
) {
    if pres.producer_blocked_at_boundary && presentation_producer_can_cross_boundary(executor, pres)
    {
        pres.producer_blocked_at_boundary = false;
    }
}

/// record a speculative producer error without surfacing it (the consumer will,
/// once it drains to that timestamp) and restore a clean live head for later recovery.
fn presentation_quarantine_producer_error(
    executor: &mut Executor,
    pres: &mut PresentationState,
    error: ExecutorError,
) {
    let err_ts = executor.state.timestamp;
    let mut errors = executor.state.errors.clone();
    if errors.is_empty() {
        errors.push(executor.record_runtime_error_at_root(error));
    }
    if let Some(trailing) = &pres.trailing {
        executor.restore_live_checkpoint(trailing);
    }
    pres.deferred_error = Some((err_ts, errors));
    pres.producer_done = true;
    pres.producer_blocked_at_boundary = false;
}

pub(super) async fn presentation_consume(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    pres: &mut PresentationState,
    now_seconds: f64,
    iteration: &mut RuntimeIteration,
) {
    let dt = PresentationState::frame_dt();
    let target_index = ((now_seconds - pres.play_anchor_wall).max(0.0) / dt).floor() as u64;
    if target_index <= pres.consumed_frames {
        return;
    }

    let mut changed = false;
    // serve from the pre-rendered buffer, dropping intermediate frames
    while pres.consumed_frames < target_index {
        let Some(frame) = pres.queue.front() else {
            break;
        };
        if !presentation_can_consume_frame(executor, pres.consumer_timestamp, frame.timestamp) {
            shared.is_playing.set(false);
            break;
        }
        let frame = pres.queue.pop_front().expect("front checked above");
        pres.consumed_frames += 1;
        pres.consumer_timestamp = frame.timestamp;
        pres.display = Some(frame);
        changed = true;
        if pres.consumer_timestamp.time.is_infinite() {
            shared.is_playing.set(false);
            let slide_duration = pres
                .display
                .as_ref()
                .and_then(|frame| frame.completed_slide_duration);
            executor.commit_current_slide_cache_if_complete(slide_duration);
            presentation_unblock_producer_at_consumer_boundary(executor, pres);
            break;
        }
    }

    // buffer underflow: the producer can't keep up. rather than play the fixed-dt
    // frames in slow motion, advance the live head directly by the wall-clock
    // deficit (a single coarse step, matching the old real-time behavior).
    if shared.is_playing.get()
        && pres.consumed_frames < target_index
        && !pres.producer_done
        && pres.deferred_error.is_none()
    {
        let deficit = (target_index - pres.consumed_frames) as f64 * dt;
        let max_slide = max_slide(executor, PlaybackMode::Presentation);
        match executor.produce_frame(max_slide, deficit).await {
            Ok((advance, scene, completed_slide_duration)) => {
                let parameters = parameter_snapshot(executor);
                pres.consumer_timestamp = executor.state.timestamp;
                pres.display = Some(PreRenderedFrame {
                    timestamp: pres.consumer_timestamp,
                    scene,
                    parameters,
                    completed_slide_duration,
                });
                pres.consumed_frames = target_index;
                changed = true;
                if matches!(advance, PlaybackAdvance::Finished) {
                    if presentation_producer_at_scene_end(executor) {
                        pres.producer_done = true;
                    } else if presentation_producer_waiting_for_consumer_boundary(executor, pres) {
                        pres.producer_blocked_at_boundary = true;
                    }
                    if pres.consumer_timestamp.time.is_infinite() {
                        shared.is_playing.set(false);
                        executor.commit_current_slide_cache_if_complete(completed_slide_duration);
                        presentation_unblock_producer_at_consumer_boundary(executor, pres);
                    }
                }
            }
            Err(error) => {
                presentation_quarantine_producer_error(executor, pres, error);
            }
        }
    }

    if changed {
        shared.current_timestamp.set(pres.consumer_timestamp);
        iteration
            .snapshots
            .push(presentation_display_snapshot(executor, shared, pres));
    }
}

fn presentation_can_consume_frame(
    executor: &Executor,
    consumer_timestamp: Timestamp,
    frame_timestamp: Timestamp,
) -> bool {
    frame_timestamp.slide <= presentation_consumer_max_slide(executor, consumer_timestamp)
}

fn presentation_consumer_max_slide(executor: &Executor, consumer_timestamp: Timestamp) -> usize {
    if consumer_timestamp.time.is_infinite() {
        (consumer_timestamp.slide + 1).min(executor.total_sections())
    } else {
        consumer_timestamp.slide
    }
}

/// once the buffer is exhausted, surface a quarantined error or stop at scene end.
fn presentation_handle_drained(
    executor: &Executor,
    shared: &SharedRuntimeState,
    pres: &mut PresentationState,
    iteration: &mut RuntimeIteration,
) {
    if !(pres.queue.is_empty() && pres.producer_done) {
        return;
    }

    if pres.deferred_error.is_some() {
        shared.cancel_runtime_work();
        shared.current_timestamp.set(pres.consumer_timestamp);
        iteration
            .snapshots
            .push(presentation_display_snapshot(executor, shared, pres));
        return;
    }

    if shared.is_playing.get() {
        shared.is_playing.set(false);
        pres.needs_establish = true;
        shared.current_timestamp.set(pres.consumer_timestamp);
        iteration
            .snapshots
            .push(presentation_display_snapshot(executor, shared, pres));
    }
}

async fn presentation_capture_display(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    pres: &mut PresentationState,
) -> bool {
    match executor.capture_stable_scene_snapshot().await {
        Ok(scene) => {
            let parameters = parameter_snapshot(executor);
            pres.display = Some(PreRenderedFrame {
                timestamp: pres.consumer_timestamp,
                scene,
                parameters,
                completed_slide_duration: None,
            });
            true
        }
        Err(_) => {
            shared.cancel_runtime_work();
            false
        }
    }
}

fn presentation_display_snapshot(
    executor: &Executor,
    shared: &SharedRuntimeState,
    pres: &PresentationState,
) -> ExecutionSnapshot {
    let status = if shared.has_compiler_error.get() {
        ExecutionStatus::CompileError
    } else if shared.has_runtime_error.get() {
        ExecutionStatus::RuntimeError
    } else if shared.is_playing.get() {
        ExecutionStatus::Playing
    } else {
        ExecutionStatus::Paused
    };

    let errors = match &pres.deferred_error {
        Some((_, errors)) if status == ExecutionStatus::RuntimeError => errors.clone(),
        _ => executor.state.errors.clone(),
    };

    let consumer_user = executor.internal_to_user_timestamp(pres.consumer_timestamp);
    let (background, camera, camera_version, meshes) = match &pres.display {
        Some(frame) => (
            Some(frame.scene.background),
            Some(frame.scene.camera.clone()),
            Some(frame.scene.camera_version),
            Some(frame.scene.meshes.clone()),
        ),
        None => (None, None, None, None),
    };
    let parameters = pres.display.as_ref().map(|frame| frame.parameters.clone());
    let transcript = (status != ExecutionStatus::CompileError)
        .then(|| executor.state.transcript.sections.clone());

    ExecutionSnapshot {
        background,
        camera,
        camera_version,
        meshes,
        errors,
        current_timestamp: consumer_user,
        target_timestamp: consumer_user,
        status,
        is_loading: false,
        slide_count: executor.real_slide_count(),
        slide_names: executor.real_slide_names(),
        slide_durations: executor.real_slide_durations(),
        minimum_slide_durations: executor.real_minimum_slide_durations(),
        parameters,
        transcript,
    }
}
