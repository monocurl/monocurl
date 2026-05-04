use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::Duration,
};

use bytecode::{Bytecode, Instruction, SectionBytecode, SectionFlags};
use executor::{
    camera::camera_value_from_snapshot,
    executor::{Executor, PlaybackAdvance, SeekToResult, StdlibFunc, TextRenderQuality},
    heap::{VRc, with_heap},
    scene_snapshot::{BackgroundSnapshot, CameraSnapshot, SceneSnapshot},
    state::LeaderKind,
    time::Timestamp,
    transcript::SectionTranscript,
    value::{MeshAttributePathSegment, Value, container::List},
};
use geo::mesh::Mesh;
use stdlib::registry::registry;

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterValue {
    Int(i64),
    VectorInt(Vec<i64>),
    Float(f64),
    VectorFloat(Vec<f64>),
    Complex { re: f64, im: f64 },
    Camera(CameraSnapshot),
    Other,
}

impl ParameterValue {
    pub fn is_supported_control(&self) -> bool {
        match self {
            Self::Int(_) | Self::Float(_) | Self::Complex { .. } => true,
            Self::VectorInt(values) => values.len() == 2,
            Self::VectorFloat(values) => values.len() == 2,
            Self::Camera(_) | Self::Other => false,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PresentationUpdateTarget {
    Param {
        leader_index: usize,
    },
    MeshAttribute {
        leader_index: usize,
        path: Vec<MeshAttributePathSegment>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterEntrySnapshot {
    pub target: PresentationUpdateTarget,
    pub name: String,
    pub value: ParameterValue,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshAttributeSnapshot {
    pub target: Option<PresentationUpdateTarget>,
    pub name: String,
    pub value: ParameterValue,
    pub children: Vec<MeshAttributeSnapshot>,
}

impl MeshAttributeSnapshot {
    pub fn has_supported_control(&self) -> bool {
        self.value.is_supported_control()
            || self
                .children
                .iter()
                .any(MeshAttributeSnapshot::has_supported_control)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshEntrySnapshot {
    pub leader_index: usize,
    pub name: String,
    pub locked: bool,
    pub attributes: Vec<MeshAttributeSnapshot>,
}

impl MeshEntrySnapshot {
    pub fn has_supported_control(&self) -> bool {
        self.attributes
            .iter()
            .any(MeshAttributeSnapshot::has_supported_control)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParameterSnapshot {
    pub params: Vec<ParameterEntrySnapshot>,
    pub meshes: Vec<MeshEntrySnapshot>,
}

pub struct ExecutionSnapshot {
    pub background: Option<BackgroundSnapshot>,
    pub camera: Option<CameraSnapshot>,
    pub camera_version: Option<u64>,
    pub meshes: Option<Vec<Arc<Mesh>>>,
    pub current_timestamp: Timestamp,
    pub target_timestamp: Timestamp,
    pub status: ExecutionStatus,
    pub is_loading: bool,
    pub slide_count: usize,
    pub slide_names: Vec<Option<String>>,
    pub slide_durations: Vec<Option<f64>>,
    pub minimum_slide_durations: Vec<Option<f64>>,
    pub parameters: Option<ParameterSnapshot>,
    pub transcript: Option<Vec<Arc<SectionTranscript>>>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PlaybackMode {
    Presentation,
    Preview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    Playing,
    Paused,
    RuntimeError,
    CompileError,
}

impl PlaybackMode {
    pub fn default_time_interval(&self) -> f64 {
        match self {
            PlaybackMode::Presentation => 1.0 / 120.0,
            PlaybackMode::Preview => 1.0 / 60.0,
        }
    }
}

pub enum RuntimeCommand {
    UpdateBytecode {
        bytecode: Option<Bytecode>,
    },
    SetPlaybackMode(PlaybackMode),
    UpdateParameters {
        updates: HashMap<PresentationUpdateTarget, ParameterValue>,
    },
    TogglePlay,
    SeekTo {
        target: Timestamp,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommandEffect {
    KeepFuture,
    ResetFuture,
}

#[derive(Default)]
pub struct RuntimeIteration {
    pub snapshots: Vec<ExecutionSnapshot>,
    pub next_frame_interval: Option<Duration>,
}

struct SharedRuntimeState {
    target: Cell<Timestamp>,
    current_timestamp: Cell<Timestamp>,
    is_playing: Cell<bool>,
    has_compiler_error: Cell<bool>,
    has_runtime_error: Cell<bool>,
    playback_mode: Cell<PlaybackMode>,
    library_sections: Cell<usize>,
    pending_param_updates: RefCell<Vec<(PresentationUpdateTarget, ParameterValue)>>,
    last_update_at: Cell<f64>,
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
            last_update_at: Cell::new(0.0),
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

#[derive(Clone)]
pub struct RuntimeController {
    executor: Rc<RefCell<Executor>>,
    shared: Rc<SharedRuntimeState>,
}

impl RuntimeController {
    pub fn new() -> Self {
        Self::with_native_funcs(default_bytecode(), registry().func_table())
    }

    pub fn with_native_funcs(bytecode: Bytecode, native_funcs: Vec<StdlibFunc>) -> Self {
        let executor = Executor::new(bytecode, native_funcs);
        let shared = SharedRuntimeState::new(&executor);
        Self {
            executor: Rc::new(RefCell::new(executor)),
            shared: Rc::new(shared),
        }
    }

    pub fn needs_work(&self) -> bool {
        self.shared.needs_work()
    }

    pub fn is_playing(&self) -> bool {
        self.shared.is_playing.get()
    }

    pub fn seek_to_requires_reset(&self, target: Timestamp) -> bool {
        let target = self.shared.user_to_internal_timestamp(target);
        self.shared.seek_requires_reset(target)
    }

    pub fn requires_future_reset(&self, command: &RuntimeCommand) -> bool {
        match command {
            RuntimeCommand::UpdateBytecode { .. } | RuntimeCommand::SetPlaybackMode(_) => true,
            RuntimeCommand::SeekTo { target } => self.seek_to_requires_reset(*target),
            RuntimeCommand::UpdateParameters { .. } | RuntimeCommand::TogglePlay => false,
        }
    }

    pub fn apply_command(&self, command: RuntimeCommand, now_seconds: f64) -> CommandEffect {
        match command {
            RuntimeCommand::UpdateBytecode { bytecode } => {
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
                CommandEffect::ResetFuture
            }
            RuntimeCommand::SetPlaybackMode(playback_mode) => {
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

                CommandEffect::ResetFuture
            }
            RuntimeCommand::SeekTo { target } => {
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
                    CommandEffect::ResetFuture
                } else {
                    CommandEffect::KeepFuture
                }
            }
            RuntimeCommand::TogglePlay => {
                let is_playing = !self.shared.is_playing.get();
                self.shared.is_playing.set(is_playing);
                self.shared.snapshot_requested.set(true);
                if is_playing {
                    self.shared.last_update_at.set(now_seconds);
                }
                CommandEffect::KeepFuture
            }
            RuntimeCommand::UpdateParameters { updates } => {
                self.shared
                    .pending_param_updates
                    .borrow_mut()
                    .extend(updates);
                self.shared.snapshot_requested.set(true);
                CommandEffect::KeepFuture
            }
        }
    }

    pub async fn run_iteration(&self, now_seconds: f64) -> RuntimeIteration {
        let mut executor = self.executor.borrow_mut();
        run_play_session_iteration(&mut executor, &self.shared, now_seconds).await
    }

    pub fn with_executor<R>(&self, f: impl FnOnce(&Executor) -> R) -> R {
        let executor = self.executor.borrow();
        f(&executor)
    }
}

impl Default for RuntimeController {
    fn default() -> Self {
        Self::new()
    }
}

fn default_bytecode() -> Bytecode {
    let mut section = SectionBytecode::new(SectionFlags {
        is_stdlib: true,
        is_library: true,
        is_init: false,
        is_root_module: true,
    });
    section.instructions.push(Instruction::EndOfExecutionHead);
    Bytecode::new(vec![Arc::new(section)])
}

async fn run_play_session_iteration(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    now_seconds: f64,
) -> RuntimeIteration {
    let mut iteration = RuntimeIteration::default();

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
        return iteration;
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
        return iteration;
    }

    let current = executor.state.timestamp;
    let target = shared.target.get();

    if target != current {
        sync_to_target(executor, shared, target, current, &mut iteration).await;
        shared.snapshot_requested.set(false);
        return iteration;
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
        return iteration;
    }

    if shared.is_playing.get() {
        playback_iteration(executor, shared, now_seconds, &mut iteration).await;
    }

    iteration
}

fn apply_pending_parameter_updates(executor: &mut Executor, shared: &SharedRuntimeState) -> bool {
    let updates = shared.pending_param_updates.take();
    let applied_parameters = !updates.is_empty();

    for (target, value) in updates {
        let Some(value) = runtime_value_from_parameter(&value) else {
            log::warn!(
                "parameter update failed for {:?}: unsupported value",
                target
            );
            continue;
        };
        let result = match &target {
            PresentationUpdateTarget::Param { leader_index } => {
                executor.update_parameter_by_leader_index(*leader_index, value)
            }
            PresentationUpdateTarget::MeshAttribute { leader_index, path } => {
                executor.update_mesh_attribute(*leader_index, path, value)
            }
        };
        if let Err(error) = result {
            log::warn!("parameter update failed for {:?}: {}", target, error);
        }
    }

    applied_parameters
}

async fn sync_to_target(
    executor: &mut Executor,
    shared: &SharedRuntimeState,
    target: Timestamp,
    current: Timestamp,
    iteration: &mut RuntimeIteration,
) {
    iteration
        .snapshots
        .push(runtime_snapshot(executor, shared, true, None));

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
) {
    let elapsed = (now_seconds - shared.last_update_at.get()).max(0.0);
    let frame_interval =
        Duration::from_secs_f64(shared.playback_mode.get().default_time_interval());
    let max_slide = max_slide(executor, shared.playback_mode.get());

    shared.last_update_at.set(now_seconds);

    iteration
        .snapshots
        .push(runtime_snapshot(executor, shared, true, None));

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
    match executor.capture_stable_scene_snapshot().await {
        Ok(scene_snapshot) => Ok(Some(scene_snapshot)),
        Err(_) => {
            shared.cancel_runtime_work();
            Err(())
        }
    }
}

fn runtime_snapshot(
    executor: &Executor,
    shared: &SharedRuntimeState,
    is_loading: bool,
    scene_snapshot: Option<SceneSnapshot>,
) -> ExecutionSnapshot {
    let current_timestamp = executor.internal_to_user_timestamp(shared.current_timestamp.get());
    let target_timestamp = executor.internal_to_user_timestamp(shared.target.get());
    let parameters = (shared.playback_mode.get() == PlaybackMode::Presentation)
        .then(|| parameter_snapshot(executor));
    let status = if shared.has_compiler_error.get() {
        ExecutionStatus::CompileError
    } else if executor.state.has_errors() {
        ExecutionStatus::RuntimeError
    } else if shared.is_playing.get() {
        ExecutionStatus::Playing
    } else {
        ExecutionStatus::Paused
    };

    let (background, camera, camera_version, meshes) = match scene_snapshot {
        Some(scene) => (
            Some(scene.background),
            Some(scene.camera),
            Some(scene.camera_version),
            Some(scene.meshes),
        ),
        None => (None, None, None, None),
    };

    let transcript = if is_loading || status == ExecutionStatus::CompileError {
        None
    } else {
        Some(executor.state.transcript.sections.clone())
    };

    ExecutionSnapshot {
        background,
        camera,
        camera_version,
        meshes,
        current_timestamp,
        target_timestamp,
        status,
        is_loading,
        slide_count: executor.real_slide_count(),
        slide_names: executor.real_slide_names(),
        slide_durations: executor.real_slide_durations(),
        minimum_slide_durations: executor.real_minimum_slide_durations(),
        parameters,
        transcript,
    }
}

fn runtime_value_from_parameter(value: &ParameterValue) -> Option<Value> {
    Some(match value {
        ParameterValue::Int(n) => Value::Integer(*n),
        ParameterValue::VectorInt(values) => Value::List(List::new_with(
            values.iter().map(|&value| VRc::new(Value::Integer(value))),
        )),
        ParameterValue::Float(f) => Value::Float(*f),
        ParameterValue::VectorFloat(values) => Value::List(List::new_with(
            values.iter().map(|&value| VRc::new(Value::Float(value))),
        )),
        ParameterValue::Complex { re, im } => Value::Complex { re: *re, im: *im },
        ParameterValue::Camera(camera) => camera_value_from_snapshot(camera),
        ParameterValue::Other => return None,
    })
}

fn parameter_value_from_runtime(value: Value) -> ParameterValue {
    match value {
        Value::Integer(n) => ParameterValue::Int(n),
        Value::Float(f) => ParameterValue::Float(f),
        Value::Complex { re, im } => ParameterValue::Complex { re, im },
        Value::List(list) => {
            let ints = list
                .elements()
                .iter()
                .map(|key| match with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => Some(n),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>();
            if let Some(ints) = ints {
                return ParameterValue::VectorInt(ints);
            }

            let floats = list
                .elements()
                .iter()
                .map(|key| match with_heap(|h| h.get(key.key()).clone()) {
                    Value::Integer(n) => Some(n as f64),
                    Value::Float(f) => Some(f),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>();
            floats.map_or(ParameterValue::Other, ParameterValue::VectorFloat)
        }
        _ => ParameterValue::Other,
    }
}

fn parameter_snapshot(executor: &Executor) -> ParameterSnapshot {
    let mut params = Vec::new();
    let mut meshes = Vec::new();

    for (leader_index, entry) in executor.state.leaders.iter().enumerate() {
        let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
        let locked = matches!(&cell_val, Value::Leader(l) if l.locked_by_anim.is_some());

        match entry.kind {
            LeaderKind::Param => {
                let follower_val = with_heap(|h| h.get(entry.follower_value).clone());
                params.push(ParameterEntrySnapshot {
                    target: PresentationUpdateTarget::Param { leader_index },
                    name: entry.name.clone(),
                    value: parameter_value_from_runtime(follower_val),
                    locked,
                });
            }
            LeaderKind::Mesh => {
                let follower_val = with_heap(|h| h.get(entry.follower_value).clone());
                meshes.push(MeshEntrySnapshot {
                    leader_index,
                    name: entry.name.clone(),
                    locked,
                    attributes: mesh_attributes_from_runtime(leader_index, follower_val, &[]),
                });
            }
        }
    }

    ParameterSnapshot { params, meshes }
}

fn mesh_attributes_from_runtime(
    leader_index: usize,
    value: Value,
    parent_path: &[MeshAttributePathSegment],
) -> Vec<MeshAttributeSnapshot> {
    match value.elide_lvalue() {
        Value::InvokedFunction(inv) => inv
            .body
            .labels
            .iter()
            .filter_map(|(arg_idx, name)| {
                let value = inv.body.arguments.get(*arg_idx)?.clone();
                Some(mesh_labeled_attribute_snapshot(
                    leader_index,
                    parent_path,
                    MeshAttributePathSegment::FunctionArgument(*arg_idx),
                    name.clone(),
                    value,
                ))
            })
            .collect(),
        Value::InvokedOperator(inv) => {
            let mut attributes = Vec::new();
            for (arg_idx, name) in &inv.body.labels {
                let Some(value) = inv.body.arguments.get(*arg_idx).cloned() else {
                    continue;
                };
                attributes.push(mesh_labeled_attribute_snapshot(
                    leader_index,
                    parent_path,
                    MeshAttributePathSegment::OperatorArgument(*arg_idx),
                    name.clone(),
                    value,
                ));
            }

            let mut operand_path = parent_path.to_vec();
            operand_path.push(MeshAttributePathSegment::OperatorOperand);
            let operand_attributes = mesh_attributes_from_runtime(
                leader_index,
                inv.body.operand.as_ref().clone(),
                &operand_path,
            );
            attributes.extend(operand_attributes);
            attributes
        }
        Value::List(list) => list
            .elements()
            .iter()
            .enumerate()
            .filter_map(|(index, element)| {
                let mut item_path = parent_path.to_vec();
                item_path.push(MeshAttributePathSegment::ListIndex(index));
                let attributes = mesh_attributes_from_runtime(
                    leader_index,
                    with_heap(|h| h.get(element.key()).clone()),
                    &item_path,
                );
                (!attributes.is_empty()).then(|| MeshAttributeSnapshot {
                    target: None,
                    name: format!("item {}", index + 1),
                    value: ParameterValue::Other,
                    children: attributes,
                })
            })
            .collect(),
        Value::Stateful(_) => Vec::new(),
        _ => Vec::new(),
    }
}

fn mesh_labeled_attribute_snapshot(
    leader_index: usize,
    parent_path: &[MeshAttributePathSegment],
    segment: MeshAttributePathSegment,
    name: String,
    value: Value,
) -> MeshAttributeSnapshot {
    let mut path = parent_path.to_vec();
    path.push(segment);
    MeshAttributeSnapshot {
        target: Some(PresentationUpdateTarget::MeshAttribute {
            leader_index,
            path: path.clone(),
        }),
        name,
        value: parameter_value_from_runtime(value.clone().elide_cached_wrappers_rec()),
        children: mesh_attributes_from_runtime(leader_index, value, &path),
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};

    use executor::{state::ExecutionState, time::Timestamp, value::Value};

    use super::{
        ParameterValue, PresentationUpdateTarget, RuntimeCommand, RuntimeController,
        default_bytecode,
    };

    #[test]
    fn update_bytecode_restores_live_executor_state_to_cache_point() {
        let runtime = RuntimeController::new();

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

        runtime.apply_command(
            RuntimeCommand::UpdateBytecode {
                bytecode: Some(default_bytecode()),
            },
            0.0,
        );

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
        let runtime = RuntimeController::new();
        let current = runtime
            .shared
            .user_to_internal_timestamp(Timestamp::new(1, 1.0));
        runtime.shared.current_timestamp.set(current);
        runtime.shared.target.set(current);

        assert!(!runtime.requires_future_reset(&RuntimeCommand::SeekTo {
            target: Timestamp::new(1, 2.0),
        }));
    }

    #[test]
    fn backward_seek_resets_play_session_future() {
        let runtime = RuntimeController::new();
        let current = runtime
            .shared
            .user_to_internal_timestamp(Timestamp::new(1, 1.0));
        runtime.shared.current_timestamp.set(current);
        runtime.shared.target.set(current);

        assert!(runtime.requires_future_reset(&RuntimeCommand::SeekTo {
            target: Timestamp::new(1, 0.5),
        }));
    }

    #[test]
    fn parameter_updates_keep_play_session_future() {
        let runtime = RuntimeController::new();
        let message = RuntimeCommand::UpdateParameters {
            updates: HashMap::from([(
                PresentationUpdateTarget::Param { leader_index: 0 },
                ParameterValue::Float(2.0),
            )]),
        };

        assert!(!runtime.requires_future_reset(&message));
    }

    #[test]
    fn runtime_error_recovery_seek_resets_play_session_future() {
        let runtime = RuntimeController::new();
        runtime.shared.has_runtime_error.set(true);

        assert!(runtime.requires_future_reset(&RuntimeCommand::SeekTo {
            target: Timestamp::new(1, 2.0),
        }));
    }

    #[test]
    fn scene_snapshot_capture_keeps_runtime_pollable_until_snapshot_emits() {
        let runtime = RuntimeController::new();
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

    #[test]
    fn custom_native_function_table_is_accepted() {
        let runtime = RuntimeController::with_native_funcs(
            bytecode::Bytecode::new(vec![Arc::new(bytecode::SectionBytecode::new(
                bytecode::SectionFlags {
                    is_stdlib: false,
                    is_library: false,
                    is_init: false,
                    is_root_module: true,
                },
            ))]),
            Vec::new(),
        );

        assert!(!runtime.is_playing());
    }
}
