use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    pin::pin,
    sync::Arc,
    time::{Duration, Instant},
};

use bytecode::{Bytecode, Instruction, SectionBytecode, SectionFlags};
use executor::{
    error::RuntimeError,
    executor::{Executor, SeekPrimitiveAnimSkipResult, SeekToResult},
    state::LeaderKind,
    time::Timestamp,
    value::{Value, container::HashableKey},
};
use futures::{
    StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
    future,
};
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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshDebugSnapshot {
    pub name: String,
    pub leader_value: String,
    pub follower_value: String,
}

pub struct ExecutionSnapshot {
    pub current_timestamp: Timestamp,
    pub status: ExecutionStatus,
    pub slide_count: usize,
    pub slide_durations: Vec<Option<f64>>,
    pub minimum_slide_durations: Vec<Option<f64>>,

    pub parameters: Option<ParameterSnapshot>,
    pub mesh_debug: Vec<MeshDebugSnapshot>,
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
            PlaybackMode::Presentation => 1.0 / 60.0,
            PlaybackMode::Preview => 1.0 / 30.0,
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
    fn format_hashable_key(key: &HashableKey) -> String {
        match key {
            HashableKey::Integer(value) => format!("{value}"),
            HashableKey::String(value) => format!("{value:?}"),
            HashableKey::Vector(values) => {
                let values = values
                    .iter()
                    .map(Self::format_hashable_key)
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{values}]")
            }
        }
    }

    fn parameter_value_from_runtime(value: Value) -> ParameterValue {
        match value {
            Value::Integer(n) => ParameterValue::Int(n),
            Value::Float(f) => ParameterValue::Float(f),
            Value::Complex { re, im } => ParameterValue::Complex { re, im },
            Value::List(list) => {
                let ints = list
                    .elements
                    .iter()
                    .map(|value| match &*value.borrow() {
                        Value::Integer(n) => Some(*n),
                        _ => None,
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(ints) = ints {
                    return ParameterValue::VectorInt(ints);
                }

                let floats = list
                    .elements
                    .iter()
                    .map(|value| match &*value.borrow() {
                        Value::Integer(n) => Some(*n as f64),
                        Value::Float(f) => Some(*f),
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
            ParameterValue::VectorInt(values) => {
                Value::List(std::rc::Rc::new(executor::value::container::List {
                    elements: values
                        .iter()
                        .map(|value| executor::value::rc_value(Value::Integer(*value)))
                        .collect(),
                }))
            }
            ParameterValue::Float(f) => Value::Float(*f),
            ParameterValue::VectorFloat(values) => {
                Value::List(std::rc::Rc::new(executor::value::container::List {
                    elements: values
                        .iter()
                        .map(|value| executor::value::rc_value(Value::Float(*value)))
                        .collect(),
                }))
            }
            ParameterValue::Complex { re, im } => Value::Complex { re: *re, im: *im },
            ParameterValue::Other => return None,
        })
    }

    fn parameter_snapshot(executor: &Executor) -> ParameterSnapshot {
        let mut parameters = HashMap::new();
        let mut locked_params = HashSet::new();
        let mut param_order = Vec::new();
        for param in &executor.state.active_params {
            let value =
                Self::parameter_value_from_runtime(param.follower_value_rc.borrow().clone());
            if matches!(&*param.leader_cell_rc.borrow(), Value::Leader(l) if l.locked_by_anim.is_some())
            {
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

    async fn mesh_debug_snapshot(executor: &mut Executor) -> Vec<MeshDebugSnapshot> {
        let entries: Vec<_> = executor
            .state
            .leaders
            .iter()
            .filter(|entry| entry.kind == LeaderKind::Mesh)
            .map(|entry| {
                (
                    entry.name.clone(),
                    entry.leader_value_rc.borrow().clone(),
                    entry.follower_value_rc.borrow().clone(),
                )
            })
            .collect();

        let mut snapshot = Vec::with_capacity(entries.len());
        for (name, leader_value, follower_value) in entries {
            let leader_value = executor
                .debug_resolve_value(leader_value)
                .await
                .map(|value| Self::format_debug_value(&value, 0))
                .unwrap_or_else(|error| format!("<error: {error}>"));
            let follower_value = executor
                .debug_resolve_value(follower_value)
                .await
                .map(|value| Self::format_debug_value(&value, 0))
                .unwrap_or_else(|error| format!("<error: {error}>"));
            snapshot.push(MeshDebugSnapshot {
                name,
                leader_value,
                follower_value,
            });
        }
        snapshot
    }

    fn format_debug_value(value: &Value, depth: usize) -> String {
        const MAX_DEPTH: usize = 6;

        if depth >= MAX_DEPTH {
            return "...".into();
        }

        match value {
            Value::Nil => "nil".into(),
            Value::Float(value) => format!("{value}"),
            Value::Integer(value) => format!("{value}"),
            Value::Complex { re, im } => format!("{re} + {im}i"),
            Value::String(value) => format!("{value:?}"),
            Value::Mesh(mesh) => format!("{mesh:#?}"),
            Value::PrimitiveAnim(_) => "<primitive_anim>".into(),
            Value::Lambda(_) => "<lambda>".into(),
            Value::Operator(_) => "<operator>".into(),
            Value::AnimBlock(_) => "<anim_block>".into(),
            Value::Map(map) => {
                if map.is_empty() {
                    return "{}".into();
                }

                let indent = "  ".repeat(depth + 1);
                let close_indent = "  ".repeat(depth);
                let entries = map
                    .iter()
                    .map(|(key, value)| {
                        format!(
                            "{indent}{}: {}",
                            Self::format_hashable_key(key),
                            Self::format_debug_value(&value.borrow(), depth + 1)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",\n");
                format!("{{\n{entries}\n{close_indent}}}")
            }
            Value::List(list) => {
                if list.elements.is_empty() {
                    return "[]".into();
                }

                let indent = "  ".repeat(depth + 1);
                let close_indent = "  ".repeat(depth);
                let entries = list
                    .elements
                    .iter()
                    .map(|value| {
                        let mut line = String::new();
                        let _ = write!(
                            &mut line,
                            "{indent}{}",
                            Self::format_debug_value(&value.borrow(), depth + 1)
                        );
                        line
                    })
                    .collect::<Vec<_>>()
                    .join(",\n");
                format!("[\n{entries}\n{close_indent}]")
            }
            Value::Stateful(stateful) => {
                if let Some(cached) = stateful.cache_valid() {
                    return format!(
                        "stateful(roots: {}, cached: {})",
                        stateful.roots.len(),
                        Self::format_debug_value(&cached, depth + 1)
                    );
                }
                format!("stateful(roots: {})", stateful.roots.len())
            }
            Value::Leader(leader) => {
                let leader_value = Self::format_debug_value(&leader.leader_rc.borrow(), depth + 1);
                let follower_value =
                    Self::format_debug_value(&leader.follower_rc.borrow(), depth + 1);
                format!("leader {{ leader: {leader_value}, follower: {follower_value} }}")
            }
            Value::InvokedOperator(_) => "<live operator>".into(),
            Value::InvokedFunction(_) => "<live function>".into(),
            Value::Lvalue(value) => Self::format_debug_value(&value.borrow(), depth),
            Value::WeakLvalue(value) => value.upgrade().map_or_else(
                || "<dangling lvalue>".into(),
                |value| Self::format_debug_value(&value.borrow(), depth),
            ),
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
        let status = if has_compiler_error {
            ExecutionStatus::CompileError
        } else if !executor.state.errors.is_empty() {
            ExecutionStatus::RuntimeError
        } else if is_loading {
            ExecutionStatus::Seeking
        } else if is_playing {
            ExecutionStatus::Playing
        } else {
            ExecutionStatus::Paused
        };

        let parameters = Self::parameter_snapshot(executor);
        let mesh_debug = if playback_mode == PlaybackMode::Presentation {
            Self::mesh_debug_snapshot(executor).await
        } else {
            Vec::new()
        };

        let snapshot = ExecutionSnapshot {
            current_timestamp: executor.internal_to_user_timestamp(executor.state.timestamp),
            status,
            slide_count: executor.real_slide_count(),
            slide_durations: executor.real_slide_durations(),
            minimum_slide_durations: executor.real_minimum_slide_durations(),
            parameters: (playback_mode == PlaybackMode::Presentation).then_some(parameters),
            mesh_debug,
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
