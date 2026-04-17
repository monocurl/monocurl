use std::{
    collections::HashMap,
    pin::pin,
    sync::Arc,
    time::{Duration, Instant},
};

use bytecode::{Bytecode, Instruction, SectionBytecode, SectionFlags};
use executor::{
    error::RuntimeError,
    executor::{Executor, SeekPrimitiveAnimSkipResult, SeekToResult},
    time::Timestamp,
    value::Value,
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
    SmallVectorInt(Vec<i64>),
    Float(f64),
    SmallVectorFloat(Vec<f64>),
    Complex { re: f64, im: f64 },
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParameterSnapshot {
    pub parameters: HashMap<String, ParameterValue>,
}

pub struct ExecutionSnapshot {
    pub current_timestamp: Timestamp,
    pub status: ExecutionStatus,
    pub slide_count: usize,
    pub slide_durations: Vec<Option<f64>>,
    pub minimum_slide_durations: Vec<Option<f64>>,

    pub parameters: Option<ParameterSnapshot>,
}

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
                    return ParameterValue::SmallVectorInt(ints);
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
                floats.map_or(ParameterValue::Other, ParameterValue::SmallVectorFloat)
            }
            _ => ParameterValue::Other,
        }
    }

    fn runtime_value_from_parameter(value: &ParameterValue) -> Option<Value> {
        Some(match value {
            ParameterValue::Int(n) => Value::Integer(*n),
            ParameterValue::SmallVectorInt(values) => {
                Value::List(std::rc::Rc::new(executor::value::container::List {
                    elements: values
                        .iter()
                        .map(|value| executor::value::rc_value(Value::Integer(*value)))
                        .collect(),
                }))
            }
            ParameterValue::Float(f) => Value::Float(*f),
            ParameterValue::SmallVectorFloat(values) => {
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

    fn parameter_snapshot(executor: &Executor) -> Option<ParameterSnapshot> {
        let parameters = executor
            .state
            .active_params
            .iter()
            .map(|param| {
                (
                    param.name.clone(),
                    Self::parameter_value_from_runtime(param.follower_value_rc.borrow().clone()),
                )
            })
            .collect::<HashMap<_, _>>();
        (!parameters.is_empty()).then_some(ParameterSnapshot { parameters })
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
                        &executor,
                        &root_text_rope,
                        has_compiler_error,
                        is_playing,
                        true,
                        version,
                    );

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
                        &executor,
                        &root_text_rope,
                        has_compiler_error,
                        is_playing,
                        false,
                        version,
                    );
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
                            Err(_) => is_playing = false,
                        }

                        target = executor.state.timestamp;

                        Self::emit_snapshot(
                            &self.sm_tx,
                            &executor,
                            &root_text_rope,
                            has_compiler_error,
                            is_playing,
                            false,
                            version,
                        );

                        let full_elapsed =
                            Instant::now().duration_since(last_update_at).as_secs_f64();
                        last_update_at = Instant::now();
                        if target_dt > full_elapsed {
                            Timer::after(Duration::from_secs_f64(target_dt - full_elapsed)).await;
                        }
                    } else {
                        Self::emit_snapshot(
                            &self.sm_tx,
                            &executor,
                            &root_text_rope,
                            has_compiler_error,
                            is_playing,
                            false,
                            version,
                        );
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

    fn emit_snapshot(
        sm_tx: &UnboundedSender<ServiceManagerMessage>,
        executor: &Executor,
        root_text_rope: &Rope<TextAggregate>,
        has_compiler_error: bool,
        is_playing: bool,
        is_loading: bool,
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

        let snapshot = ExecutionSnapshot {
            current_timestamp: executor.internal_to_user_timestamp(executor.state.timestamp),
            status,
            slide_count: executor.real_slide_count(),
            slide_durations: executor.real_slide_durations(),
            minimum_slide_durations: executor.real_minimum_slide_durations(),
            parameters: Self::parameter_snapshot(executor),
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
