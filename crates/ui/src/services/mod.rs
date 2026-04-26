use std::{collections::HashMap, path::PathBuf};

use crate::{
    services::{
        compilation::{CompilationMessage, CompilationService},
        execution::{ExecutionMessage, ExecutionService},
        lexing::{LexingMessage, LexingService},
    },
    state::{
        diagnostics::Diagnostic,
        execution_state::ExecutionState,
        textual_state::{
            AutoCompleteItem, Cursor, LexData, ParameterPositionHint, StaticAnalysisData,
            TextualState, TransactionSummary,
        },
    },
};
use executor::time::Timestamp;
use futures::channel::mpsc::unbounded;
use futures::{SinkExt, StreamExt, channel::mpsc::UnboundedSender};
use gpui::{App, AppContext, Context, Entity};
use lexer::token::Token;
use structs::rope::{Attribute, Rope, TextAggregate};

mod compilation;
mod execution;
mod lexing;

pub(crate) use execution::{
    ExecutionSnapshot, ExecutionStatus, ParameterSnapshot, ParameterValue, PlaybackMode,
};

pub struct ServiceManager {
    textual_state: Entity<TextualState>,
    execution_state: Entity<ExecutionState>,

    lexing_tx: UnboundedSender<LexingMessage>,
    compilation_tx: UnboundedSender<CompilationMessage>,
    execution_tx: UnboundedSender<ExecutionMessage>,
}

pub enum ServiceManagerMessage {
    UpdateLexRope {
        lex_rope: Rope<Attribute<LexData>>,
        version: usize,
    },
    #[allow(unused)]
    UpdateStaticAnalysisRope {
        analysis_rope: Rope<Attribute<StaticAnalysisData>>,
        version: usize,
    },
    UpdateCompileDiagnostics {
        diagnostics: Vec<Diagnostic>,
        version: usize,
    },
    UpdateRuntimeDiagnostics {
        diagnostics: Vec<Diagnostic>,
        version: usize,
    },
    UpdateAutocompleteSuggestions {
        suggestions: Vec<AutoCompleteItem>,
        cursor: Cursor,
        version: usize,
    },
    UpdateParameterHintPosition {
        hint: Option<ParameterPositionHint>,
        cursor: Cursor,
        version: usize,
    },
    ExecutionStateUpdated {
        snapshot: ExecutionSnapshot,
    },
}

impl ServiceManager {
    pub fn new(
        textual_state: Entity<TextualState>,
        execution_state: Entity<ExecutionState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (sm_tx, mut sm_rx) = unbounded();
        let (lexing_tx, lexing_rx) = unbounded();
        let (compilation_tx, compilation_rx) = unbounded();
        let (execution_tx, execution_rx) = unbounded();

        let lexing = LexingService::new(lexing_rx, compilation_tx.clone(), sm_tx.clone());
        let compilation =
            CompilationService::new(compilation_rx, execution_tx.clone(), sm_tx.clone());
        let execution = ExecutionService::new(execution_rx, sm_tx.clone());

        let weak_service_manager = cx.weak_entity();
        textual_state.update(cx, |state, _| {
            state.add_transaction_listener(move |transaction, cx| {
                weak_service_manager
                    .update(cx, |service_manager, cx| {
                        service_manager.on_transaction(transaction, cx);
                    })
                    .unwrap();
            });
        });

        // the service that actually executes on the actions that the background threads have requested
        // note that they can't make the changes themselves on the other threads
        cx.spawn(async move |sm, cx| {
            log::info!("Starting Service Manager Foreground Handling");
            while let Some(message) = sm_rx.next().await {
                let Some(_) = sm.upgrade().and_then(|sm| {
                    cx.update_entity(&sm, |sm, cx| {
                        sm.on_sm_message_recv(message, cx);

                        while let Ok(Some(sub_message)) = sm_rx.try_next() {
                            sm.on_sm_message_recv(sub_message, cx);
                        }
                    })
                    .ok()
                }) else {
                    break;
                };
            }
            log::info!("Exiting Service Manager Foreground Handling");
        })
        .detach();

        cx.background_spawn(lexing.run()).detach();
        cx.background_spawn(compilation.run()).detach();
        execution.run(); // spawns its own dedicated OS thread (Executor is !Send)

        Self {
            textual_state,
            execution_state,

            lexing_tx,
            compilation_tx,
            execution_tx,
        }
    }

    fn on_transaction(&mut self, transaction: &TransactionSummary, _cx: &mut App) {
        smol::block_on(async {
            // because cursor is sent first, it must be the case that
            // compilation thread will receive cursor messages before any rope updates
            self.compilation_tx
                .send(CompilationMessage::UpdateCursor {
                    cursor: transaction.new_cursor,
                    _version: transaction.final_version,
                })
                .await
                .unwrap();

            self.lexing_tx
                .send_all(&mut futures::stream::iter(
                    transaction
                        .text_changes
                        .iter()
                        .map(|(old, new_text, new_rope, version)| {
                            Ok(LexingMessage::UpdateRope {
                                old: old.clone(),
                                new: new_text.len(),
                                new_rope: new_rope.clone(),
                                version: *version,
                            })
                        }),
                ))
                .await
                .unwrap();
        })
    }

    fn on_sm_message_recv(&mut self, msg: ServiceManagerMessage, cx: &mut App) {
        match msg {
            ServiceManagerMessage::UpdateLexRope { lex_rope, version } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_lex_rope(lex_rope, version) {
                        cx.notify();
                    }
                });
            }
            ServiceManagerMessage::UpdateStaticAnalysisRope {
                analysis_rope,
                version,
            } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_static_analysis_rope(analysis_rope, version) {
                        cx.notify();
                    }
                });
            }
            ServiceManagerMessage::UpdateCompileDiagnostics {
                diagnostics,
                version,
            } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_compile_diagnostics(diagnostics, version) {
                        cx.notify();
                    }
                });
            }
            ServiceManagerMessage::UpdateRuntimeDiagnostics {
                diagnostics,
                version,
            } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_runtime_diagnostics(diagnostics, version) {
                        cx.notify();
                    }
                });
            }
            ServiceManagerMessage::UpdateAutocompleteSuggestions {
                suggestions,
                cursor,
                version,
            } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_autocomplete_state(suggestions, version, cursor) {
                        cx.notify();
                    }
                });
            }
            ServiceManagerMessage::UpdateParameterHintPosition {
                hint,
                cursor,
                version,
            } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_parameter_position_state(hint, version, cursor) {
                        cx.notify();
                    }
                });
            }
            ServiceManagerMessage::ExecutionStateUpdated { snapshot } => {
                self.execution_state.update(cx, |state, cx| {
                    state.apply_snapshot(snapshot);
                    cx.notify();
                });
            }
        }
    }

    pub fn invalidate_dependencies(
        &mut self,
        physical_path: Option<PathBuf>,
        live_ropes: HashMap<PathBuf, (Rope<Attribute<Token>>, Rope<TextAggregate>)>,
    ) {
        smol::block_on(async {
            self.compilation_tx
                .send(CompilationMessage::RecheckDependencies {
                    physical_path,
                    open_documents: live_ropes,
                })
                .await
                .unwrap();
        })
    }

    pub fn timestamp(&self, cx: &App) -> Timestamp {
        self.execution_state.read(cx).current_timestamp
    }

    pub fn seek_to(&mut self, target: Timestamp) {
        smol::block_on(async {
            self.execution_tx
                .send(ExecutionMessage::SeekTo { target })
                .await
                .unwrap();
        })
    }

    pub fn prev_slide(&mut self, cx: &App) {
        let ts = self.timestamp(cx);
        let next = if ts.time > 1e-3 {
            Timestamp::new(ts.slide, -f64::MIN_POSITIVE)
        } else if ts.slide == 0 {
            Timestamp::new(0, -f64::MIN_POSITIVE)
        } else {
            Timestamp::new(ts.slide - 1, -f64::MIN_POSITIVE)
        };
        self.seek_to(next);
    }

    pub fn next_slide(&mut self, cx: &App) {
        let ts = self.timestamp(cx);
        self.seek_to(Timestamp::new(ts.slide + 1, -f64::MIN_POSITIVE));
    }

    pub fn scene_start(&mut self) {
        self.seek_to(Timestamp::new(0, -f64::MIN_POSITIVE));
    }

    pub fn scene_end(&mut self, cx: &App) {
        let slide_count = self.execution_state.read(cx).slide_count;
        self.seek_to(Timestamp::new(slide_count, -f64::MIN_POSITIVE));
    }

    pub fn toggle_play(&mut self) {
        smol::block_on(async {
            self.execution_tx
                .send(ExecutionMessage::TogglePlay)
                .await
                .unwrap();
        })
    }

    pub fn set_playback_mode(&mut self, ctx: PlaybackMode) {
        smol::block_on(async {
            self.execution_tx
                .send(ExecutionMessage::SetPlaybackMode(ctx))
                .await
                .unwrap();
        })
    }

    pub fn update_parameters(&mut self, updates: HashMap<String, ParameterValue>) {
        smol::block_on(async {
            self.execution_tx
                .send(ExecutionMessage::UpdateParameters { updates })
                .await
                .unwrap();
        })
    }

    pub fn execution_state(&self) -> &Entity<ExecutionState> {
        &self.execution_state
    }
}
