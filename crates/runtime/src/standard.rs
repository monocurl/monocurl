use std::time::Duration;

use executor::{
    executor::{Executor, PlaybackAdvance, SeekToResult},
    time::Timestamp,
};

use super::{
    LoadingSnapshotSink, PlaybackEngine, RuntimeIteration, SharedRuntimeState,
    apply_pending_parameter_updates, capture_scene_snapshot, clamp_target_to_valid_timestamp,
    max_slide, runtime_snapshot,
};

pub(super) struct StandardEngine;

impl PlaybackEngine for StandardEngine {
    async fn run_iteration(
        executor: &mut Executor,
        shared: &SharedRuntimeState,
        now_seconds: f64,
        iteration: &mut RuntimeIteration,
        loading_snapshot_sink: &mut LoadingSnapshotSink<'_>,
    ) {
        if apply_pending_parameter_updates(executor, shared) {
            shared.snapshot_requested.set(true);
        }

        if shared.has_compiler_error.get() {
            if shared.snapshot_requested.get() {
                shared.current_timestamp.set(shared.target.get());
                iteration
                    .snapshots
                    .push(runtime_snapshot(executor, shared, false, None));
                shared.snapshot_requested.set(false);
            }
            return;
        }

        clamp_target_to_valid_timestamp(executor, shared);
        shared.current_timestamp.set(executor.state.timestamp);

        if executor.state.has_errors() {
            shared.cancel_runtime_work();
            if shared.snapshot_requested.get() {
                iteration
                    .snapshots
                    .push(runtime_snapshot(executor, shared, false, None));
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
                target,
                current,
                iteration,
                loading_snapshot_sink,
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
            iteration
                .snapshots
                .push(runtime_snapshot(executor, shared, false, scene_snapshot));
            shared.snapshot_requested.set(false);
            return;
        }

        if shared.is_playing.get() {
            playback_iteration(
                executor,
                shared,
                now_seconds,
                iteration,
                loading_snapshot_sink,
            )
            .await;
        }
    }
}

async fn sync_to_target(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    target: Timestamp,
    current: Timestamp,
    iteration: &mut RuntimeIteration,
    loading_snapshot_sink: &mut LoadingSnapshotSink<'_>,
) {
    loading_snapshot_sink.emit_or_defer(iteration, runtime_snapshot(executor, shared, true, None));

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
    iteration
        .snapshots
        .push(runtime_snapshot(executor, shared, false, scene_snapshot));
}

async fn playback_iteration(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    now_seconds: f64,
    iteration: &mut RuntimeIteration,
    loading_snapshot_sink: &mut LoadingSnapshotSink<'_>,
) {
    let elapsed = (now_seconds - shared.last_update_at.get()).max(0.0);
    let frame_interval =
        Duration::from_secs_f64(shared.playback_mode.get().default_time_interval());
    let max_slide = max_slide(executor, shared.playback_mode.get());

    shared.last_update_at.set(now_seconds);

    loading_snapshot_sink.emit_or_defer(iteration, runtime_snapshot(executor, shared, true, None));

    match executor.advance_playback(max_slide, elapsed).await {
        Ok(PlaybackAdvance::Advanced) => {}
        Ok(PlaybackAdvance::PreparedSection) => {
            shared.last_update_at.set(now_seconds);
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
    iteration
        .snapshots
        .push(runtime_snapshot(executor, shared, false, scene_snapshot));
    shared.snapshot_requested.set(false);

    if shared.is_playing.get() {
        iteration.next_frame_interval = Some(frame_interval);
    }
}
