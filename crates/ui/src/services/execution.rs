use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::pin,
    sync::Arc,
    time::{Duration, Instant},
};

use bytecode::{Bytecode, Instruction, SectionBytecode, SectionFlags};
use executor::{
    error::{ExecutorError, RuntimeError},
    executor::{Executor, SeekPrimitiveAnimSkipResult, SeekToResult},
    heap::{VRc, with_heap},
    state::LeaderKind,
    time::Timestamp,
    value::{
        Value,
        container::{HashableKey, List, Map},
    },
};
use futures::{
    StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
    future,
};
use geo::{mesh::Mesh, simd::Float3};
use smol::Timer;
use stdlib::registry::registry;
use structs::rope::{Rope, TextAggregate};

use crate::{services::ServiceManagerMessage, state::diagnostics::Diagnostic};

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterValue {
    Int(i64),
    VectorInt(Vec<i64>),
    Float(f64),
    VectorFloat(Vec<f64>),
    Complex { re: f64, im: f64 },
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParameterSnapshot {
    pub parameters: HashMap<String, ParameterValue>,
    /// parameter names currently locked by an active animation
    pub locked_params: HashSet<String>,
    /// names in order of first registration (oldest first)
    pub param_order: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewportCameraSnapshot {
    pub position: Float3,
    pub look_at: Float3,
    pub up: Float3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub ortho: bool,
}

impl Default for ViewportCameraSnapshot {
    fn default() -> Self {
        Self {
            position: Float3::new(0.0, 0.0, -10.0),
            look_at: Float3::ZERO,
            up: Float3::Y,
            fov: 0.698_131_7,
            near: 0.1,
            far: 100.0,
            ortho: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportBackgroundSnapshot {
    pub color: (f32, f32, f32, f32),
}

impl Default for ViewportBackgroundSnapshot {
    fn default() -> Self {
        Self {
            color: (0.0, 0.0, 0.0, 1.0),
        }
    }
}

pub struct ExecutionSnapshot {
    pub background: Option<ViewportBackgroundSnapshot>,
    pub camera: Option<ViewportCameraSnapshot>,
    pub meshes: Option<Vec<Arc<Mesh>>>,
    pub current_timestamp: Timestamp,
    pub status: ExecutionStatus,
    pub slide_count: usize,
    pub slide_durations: Vec<Option<f64>>,
    pub minimum_slide_durations: Vec<Option<f64>>,

    pub parameters: Option<ParameterSnapshot>,
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
    Seeking,
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

pub enum ExecutionMessage {
    UpdateBytecode {
        bytecode: Option<Bytecode>,
        root_text_rope: Rope<TextAggregate>,
        version: usize,
    },
    SetPlaybackMode(PlaybackMode),
    // doesn't have to be all parameters
    UpdateParameters {
        updates: HashMap<String, ParameterValue>,
    },
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
    let mut section = SectionBytecode::new(SectionFlags {
        is_stdlib: true,
        is_library: true,
        is_init: false,
        is_root_module: true,
    });
    section.instructions.push(Instruction::EndOfExecutionHead);
    Bytecode::new(vec![Arc::new(section)])
}

impl ExecutionService {
    fn collect_scene_meshes<'a>(
        executor: &'a mut Executor,
        value: Value,
        target_name: &'a str,
        out: &'a mut Vec<Arc<Mesh>>,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            let value = value.elide_wrappers(executor).await?;
            match value {
                Value::Mesh(mesh) => {
                    out.push(mesh);
                    Ok(())
                }
                Value::List(list) => {
                    for item in list.elements() {
                        let item = with_heap(|h| h.get(item.key()).clone());
                        Self::collect_scene_meshes(executor, item, target_name, out).await?;
                    }
                    Ok(())
                }
                other => Err(ExecutorError::Other(format!(
                    "on-screen mesh '{}' must resolve to a mesh tree, got {}",
                    target_name,
                    other.type_name()
                ))),
            }
        })
    }

    async fn scene_meshes(executor: &mut Executor) -> Result<Vec<Arc<Mesh>>, ExecutorError> {
        let mut meshes = Vec::new();
        let leaders = executor
            .state
            .leaders
            .iter()
            .filter(|entry| entry.kind == LeaderKind::Mesh)
            .map(|entry| (entry.name.clone(), entry.follower_value))
            .collect::<Vec<_>>();

        for (name, follower_value) in leaders {
            let follower = with_heap(|h| h.get(follower_value).clone());
            Self::collect_scene_meshes(executor, follower, &name, &mut meshes).await?;
        }
        Ok(meshes)
    }

    fn map_field_value(map: &Map, name: &str) -> Option<Value> {
        map.get(&HashableKey::String(name.to_string()))
            .map(|value| with_heap(|h| h.get(value.key()).clone()).elide_lvalue_leader_rec())
    }

    fn read_f32(value: &Value) -> Option<f32> {
        match value {
            Value::Integer(n) => Some(*n as f32),
            Value::Float(f) => Some(*f as f32),
            _ => None,
        }
    }

    fn read_bool_flag(value: &Value) -> Option<bool> {
        match value {
            Value::Integer(n) => Some(*n != 0),
            Value::Float(f) => Some(*f != 0.0),
            _ => None,
        }
    }

    fn read_float3(value: &Value) -> Option<Float3> {
        let Value::List(list) = value else {
            return None;
        };
        if list.len() != 3 {
            return None;
        }

        let mut components = [0.0; 3];
        for (slot, component) in components.iter_mut().zip(list.elements()) {
            *slot = Self::read_f32(&with_heap(|h| h.get(component.key()).clone()))?;
        }
        Some(Float3::from_array(components))
    }

    fn read_float4(value: &Value) -> Option<(f32, f32, f32, f32)> {
        let Value::List(list) = value else {
            return None;
        };
        if list.len() != 4 {
            return None;
        }

        let mut components = [0.0; 4];
        for (slot, component) in components.iter_mut().zip(list.elements()) {
            *slot = Self::read_f32(&with_heap(|h| h.get(component.key()).clone()))?;
        }
        Some((
            components[0],
            components[1],
            components[2],
            components[3],
        ))
    }

    fn camera_snapshot_from_value(value: &Value) -> Option<ViewportCameraSnapshot> {
        let Value::Map(map) = value else {
            return None;
        };
        let kind = Self::map_field_value(map, "kind")?;
        if !matches!(kind, Value::String(ref kind) if kind == "camera") {
            return None;
        }

        Some(ViewportCameraSnapshot {
            position: Self::read_float3(&Self::map_field_value(map, "position")?)?,
            look_at: Self::read_float3(&Self::map_field_value(map, "look_at")?)?,
            up: Self::read_float3(&Self::map_field_value(map, "up")?)?,
            fov: Self::read_f32(&Self::map_field_value(map, "fov")?)?,
            near: Self::read_f32(&Self::map_field_value(map, "near")?)?,
            far: Self::read_f32(&Self::map_field_value(map, "far")?)?,
            ortho: Self::read_bool_flag(&Self::map_field_value(map, "ortho")?)?,
        })
    }

    fn camera_snapshot(executor: &Executor) -> ViewportCameraSnapshot {
        executor
            .state
            .leaders
            .iter()
            .rev()
            .find_map(|entry| {
                (entry.name == "camera")
                    .then(|| {
                        with_heap(|h| h.get(entry.follower_value).clone()).elide_lvalue_leader_rec()
                    })
                    .and_then(|value| Self::camera_snapshot_from_value(&value))
            })
            .unwrap_or_default()
    }

    fn background_snapshot_from_value(value: &Value) -> Option<ViewportBackgroundSnapshot> {
        let Value::Map(map) = value else {
            return None;
        };
        let kind = Self::map_field_value(map, "kind")?;
        if !matches!(kind, Value::String(ref kind) if kind == "solid_background") {
            return None;
        }

        Some(ViewportBackgroundSnapshot {
            color: Self::read_float4(&Self::map_field_value(map, "color")?)?,
        })
    }

    fn background_snapshot(executor: &Executor) -> ViewportBackgroundSnapshot {
        executor
            .state
            .leaders
            .iter()
            .rev()
            .find_map(|entry| {
                (entry.name == "background")
                    .then(|| {
                        with_heap(|h| h.get(entry.follower_value).clone()).elide_lvalue_leader_rec()
                    })
                    .and_then(|value| Self::background_snapshot_from_value(&value))
            })
            .unwrap_or_default()
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

    fn runtime_value_from_parameter(value: &ParameterValue) -> Option<Value> {
        Some(match value {
            ParameterValue::Int(n) => Value::Integer(*n),
            ParameterValue::VectorInt(values) => Value::List(std::rc::Rc::new(List::new_with(
                values
                    .iter()
                    .map(|&value| VRc::new(Value::Integer(value)))
                    .collect(),
            ))),
            ParameterValue::Float(f) => Value::Float(*f),
            ParameterValue::VectorFloat(values) => Value::List(std::rc::Rc::new(List::new_with(
                values
                    .iter()
                    .map(|&value| VRc::new(Value::Float(value)))
                    .collect(),
            ))),
            ParameterValue::Complex { re, im } => Value::Complex { re: *re, im: *im },
            ParameterValue::Other => return None,
        })
    }

    fn parameter_snapshot(executor: &Executor) -> ParameterSnapshot {
        let mut parameters = HashMap::new();
        let mut locked_params = HashSet::new();
        let mut param_order = Vec::new();
        for param in &executor.state.active_params {
            let follower_val = with_heap(|h| h.get(param.follower_value).clone());
            let value = Self::parameter_value_from_runtime(follower_val);
            let cell_val = with_heap(|h| h.get(param.leader_cell.key()).clone());
            if matches!(&cell_val, Value::Leader(l) if l.locked_by_anim.is_some()) {
                locked_params.insert(param.name.clone());
            }
            parameters.insert(param.name.clone(), value);
            param_order.push(param.name.clone());
        }
        ParameterSnapshot {
            parameters,
            locked_params,
            param_order,
        }
    }

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
        let mut root_text_rope = Rope::default();

        let mut last_update_at = Instant::now();

        let Some(mut message) = self.rx.next().await else {
            return;
        };

        loop {
            match message {
                ExecutionMessage::UpdateBytecode {
                    bytecode,
                    root_text_rope: new_root_text_rope,
                    version: nversion,
                } => {
                    version = nversion;
                    is_playing = false;
                    root_text_rope = new_root_text_rope;
                    if let Some(bytecode) = bytecode {
                        let old_user_timestamp = executor.internal_to_user_timestamp(target);
                        executor.update_bytecode(bytecode);
                        target = executor.user_to_internal_timestamp(old_user_timestamp);
                        has_compiler_error = false;
                    } else {
                        has_compiler_error = true;
                    }
                }
                ExecutionMessage::SetPlaybackMode(ctx) => {
                    log::info!(
                        "playback mode -> {}",
                        match ctx {
                            PlaybackMode::Presentation => "presentation",
                            PlaybackMode::Preview => "preview",
                        }
                    );
                    is_playing = false;
                    playback_mode = ctx;
                    executor.clear_cache();
                    target = executor.user_to_internal_timestamp(Timestamp::default());
                    has_seeked_for_play = false;
                }
                ExecutionMessage::SeekTo { target: t } => {
                    log::info!("seek_to {:?}", t);
                    is_playing = false;
                    target = executor.user_to_internal_timestamp(t);
                }
                ExecutionMessage::TogglePlay => {
                    is_playing = !is_playing;
                    log::info!(
                        "playback toggled -> {}",
                        if is_playing { "playing" } else { "paused" }
                    );
                    if is_playing {
                        last_update_at = Instant::now();
                        has_seeked_for_play = false;
                    }
                }
                ExecutionMessage::UpdateParameters { ref updates } => {
                    for (name, value) in updates {
                        let Some(value) = Self::runtime_value_from_parameter(&value) else {
                            log::warn!("parameter update failed for {}: unsupported value", name);
                            continue;
                        };
                        if let Err(error) = executor.update_parameter(name, value) {
                            log::warn!("parameter update failed for {}: {}", name, error);
                        }
                    }
                }
            }

            let state_update = async {
                if !is_playing || !has_seeked_for_play {
                    Self::emit_snapshot(
                        &self.sm_tx,
                        &mut executor,
                        &root_text_rope,
                        has_compiler_error,
                        is_playing,
                        true,
                        playback_mode,
                        version,
                    )
                    .await;

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

                    Self::emit_snapshot(
                        &self.sm_tx,
                        &mut executor,
                        &root_text_rope,
                        has_compiler_error,
                        is_playing,
                        false,
                        playback_mode,
                        version,
                    )
                    .await;
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
                        SeekPrimitiveAnimSkipResult::PrimitiveAnim => {}
                        SeekPrimitiveAnimSkipResult::NoAnimsLeft => {
                            // even in presentation mode, actually advance
                            if executor.state.timestamp.slide + 1 < executor.total_sections() {
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
                                    if executor.state.timestamp.slide + 1
                                        < executor.total_sections()
                                    {
                                        executor.advance_section().await;
                                        // this brings parameters into scope
                                        let _ = executor.seek_primitive_anim_skip(max_slide).await;
                                    }
                                    is_playing = false;
                                }
                            }
                            Err(_) => is_playing = false,
                        }

                        target = executor.state.timestamp;

                        Self::emit_snapshot(
                            &self.sm_tx,
                            &mut executor,
                            &root_text_rope,
                            has_compiler_error,
                            is_playing,
                            false,
                            playback_mode,
                            version,
                        )
                        .await;

                        let full_elapsed =
                            Instant::now().duration_since(last_update_at).as_secs_f64();
                        last_update_at = Instant::now();
                        if target_dt > full_elapsed {
                            Timer::after(Duration::from_secs_f64(target_dt - full_elapsed)).await;
                        }
                    } else {
                        Self::emit_snapshot(
                            &self.sm_tx,
                            &mut executor,
                            &root_text_rope,
                            has_compiler_error,
                            is_playing,
                            false,
                            playback_mode,
                            version,
                        )
                        .await;
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

    async fn emit_snapshot(
        sm_tx: &UnboundedSender<ServiceManagerMessage>,
        executor: &mut Executor,
        root_text_rope: &Rope<TextAggregate>,
        has_compiler_error: bool,
        is_playing: bool,
        is_loading: bool,
        playback_mode: PlaybackMode,
        version: usize,
    ) {
        let parameters = Self::parameter_snapshot(executor);
        let mut status = if has_compiler_error {
            ExecutionStatus::CompileError
        } else if executor.state.has_errors() {
            ExecutionStatus::RuntimeError
        } else if is_loading {
            ExecutionStatus::Seeking
        } else if is_playing {
            ExecutionStatus::Playing
        } else {
            ExecutionStatus::Paused
        };

        let (background, camera, meshes) =
            if matches!(status, ExecutionStatus::Playing | ExecutionStatus::Paused) {
                match Self::scene_meshes(executor).await {
                    Ok(meshes) => (
                        Some(Self::background_snapshot(executor)),
                        Some(Self::camera_snapshot(executor)),
                        Some(meshes),
                    ),
                    Err(error) => {
                        executor.record_runtime_error(error);
                        status = ExecutionStatus::RuntimeError;
                        (None, None, None)
                    }
                }
            } else {
                (None, None, None)
            };

        let snapshot = ExecutionSnapshot {
            background,
            camera,
            meshes,
            current_timestamp: executor.internal_to_user_timestamp(executor.state.timestamp),
            status,
            slide_count: executor.real_slide_count(),
            slide_durations: executor.real_slide_durations(),
            minimum_slide_durations: executor.real_minimum_slide_durations(),
            parameters: (playback_mode == PlaybackMode::Presentation).then_some(parameters),
        };

        sm_tx
            .unbounded_send(ServiceManagerMessage::ExecutionStateUpdated { snapshot })
            .ok();

        let diagnostics = executor
            .state
            .errors
            .iter()
            .map(|runtime_error| Diagnostic {
                dtype: crate::state::diagnostics::DiagnosticType::RuntimeError,
                span: runtime_error.span.clone(),
                title: "Runtime Error".into(),
                message: format_runtime_error_message(executor, root_text_rope, runtime_error),
            })
            .collect();

        if has_compiler_error {
            sm_tx
                .unbounded_send(ServiceManagerMessage::UpdateRuntimeDiagnostics {
                    diagnostics: Vec::new(),
                    version,
                })
                .ok();
        } else {
            sm_tx
                .unbounded_send(ServiceManagerMessage::UpdateRuntimeDiagnostics {
                    diagnostics,
                    version,
                })
                .ok();
        }
    }
}

fn format_runtime_error_message(
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
    runtime_error: &RuntimeError,
) -> String {
    let mut message = runtime_error.error.to_string();
    if runtime_error.callstack.is_empty() {
        return message;
    }

    let formatted_callstack = runtime_error
        .callstack
        .iter()
        .map(|frame| format_runtime_call_frame(executor, root_text_rope, frame))
        .collect::<Vec<_>>()
        .join("\n");

    message.push_str("\n\ntop of callstack:\n");
    message.push_str(&formatted_callstack);
    message
}

fn format_runtime_call_frame(
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
    frame: &executor::error::RuntimeCallFrame,
) -> String {
    let section_idx = frame.section as usize;
    let section = executor.section_bytecode(section_idx);

    if section.flags.is_root_module {
        let line = root_text_rope
            .utf8_prefix_summary(frame.span.start)
            .newlines
            + 1;
        format!("{}:{}", root_section_label(executor, section_idx), line)
    } else if let Some(name) = &section.source_file_name {
        format!("<{}>", name)
    } else if let Some(index) = section.import_display_index {
        format!("<imported library {}>", index)
    } else {
        "<imported library>".into()
    }
}

fn root_section_label(executor: &Executor, section_idx: usize) -> String {
    let section = executor.section_bytecode(section_idx);
    if section.flags.is_init {
        let ordinal = executor.sections()[..=section_idx]
            .iter()
            .filter(|section| section.flags.is_root_module && section.flags.is_init)
            .count();
        if ordinal <= 1 {
            "<init>".into()
        } else {
            format!("<init {}>", ordinal)
        }
    } else if section.flags.is_library {
        "<prelude>".into()
    } else {
        let ordinal = executor.sections()[..=section_idx]
            .iter()
            .filter(|section| {
                section.flags.is_root_module && !section.flags.is_library && !section.flags.is_init
            })
            .count();
        format!("<slide {}>", ordinal)
    }
}
