
use std::{collections::HashMap, path::PathBuf};

use gpui::{App, AppContext, Context, Entity};
use futures::{SinkExt, StreamExt, channel::mpsc::{UnboundedSender}};
use futures::channel::mpsc::unbounded;
use lexer::token::Token;
use structs::rope::{Attribute, Rope, TextAggregate};
use crate::{services::{compilation::{CompilationMessage, CompilationService}, execution::{ExecutionMessage, ExecutionService}, lexing::{LexingMessage, LexingService}}, state::{diagnostics::Diagnostic, execution_state::ExecutionState, textual_state::{AutoCompleteItem, Cursor, LexData, ParameterPositionHint, StaticAnalysisData, TextualState, TransactionSummary}, window_state::WindowState}};

mod lexing;
mod compilation;
mod execution;

pub struct ServiceManager {
    textual_state: Entity<TextualState>,
    execution_state: Entity<ExecutionState>,

    sm_tx: UnboundedSender<ServiceManagerMessage>,
    lexing_tx: UnboundedSender<LexingMessage>,
    compilation_tx: UnboundedSender<CompilationMessage>,
    execution_tx: UnboundedSender<ExecutionMessage>,
}

pub enum ServiceManagerMessage {
    UpdateLexRope {
        lex_rope: Rope<Attribute<LexData>>,
        version: usize,
    },
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
    UpdateByteCode,
    ExecutionStateUpdated,
}

impl ServiceManager {
    pub fn new(textual_state: Entity<TextualState>, execution_state: Entity<ExecutionState>, cx: &mut Context<Self>) -> Self {
        let (sm_tx, mut sm_rx) = unbounded();
        let (lexing_tx, lexing_rx) = unbounded();
        let (compilation_tx, compilation_rx) = unbounded();
        let (execution_tx, execution_rx) = unbounded();

        let lexing = LexingService::new(lexing_rx, compilation_tx.clone(), sm_tx.clone());
        let compilation = CompilationService::new(compilation_rx, execution_tx.clone(), sm_tx.clone());
        let execution = ExecutionService::new(execution_rx, sm_tx.clone());

        let weak_service_manager = cx.weak_entity();
        textual_state.update(cx, |state, _| {
            state.add_transaction_listener(move |transaction, cx| {
                weak_service_manager.update(cx, |service_manager, cx| {
                    service_manager.on_transaction(transaction, cx);
                }).unwrap();
            });
        });

        // the service that actually executes on the actions that the background threads have requested
        // note that they can't make the changes themselves on the other threads
        cx.spawn(async move |sm, cx| {
            log::info!("Starting Service Manager Foreground Handling");
            while let Some(message) = sm_rx.next().await {
                let Some(_) = sm.upgrade()
                    .and_then(|sm| {
                        cx.update_entity(&sm, |sm, cx| {
                            sm.on_sm_message_recv(message, cx);
                        }).ok()
                    })
                else {
                    break;
                };
            }
            log::info!("Exiting Service Manager Foreground Handling");
        }).detach();

        cx.background_spawn(lexing.run()).detach();
        cx.background_spawn(compilation.run()).detach();
        cx.background_spawn(execution.run()).detach();

        Self {
            textual_state,
            execution_state,

            lexing_tx,
            compilation_tx,
            execution_tx,
            sm_tx,
        }
    }

    fn on_transaction(&mut self, transaction: &TransactionSummary, _cx: &mut App) {
        smol::block_on(async {
            // because cursor is sent first, it must be the case that
            // compilation thread will receive cursor messages before any rope updates
            self.compilation_tx.send(
                CompilationMessage::UpdateCursor {
                    cursor: transaction.new_cursor,
                    version: transaction.final_version
                }
            ).await.unwrap();

            self.lexing_tx.send_all(&mut futures::stream::iter(
                transaction.text_changes.iter()
                    .map(|(old, new_text, new_rope, version)| Ok(LexingMessage::UpdateRope {
                        old: old.clone(),
                        new: new_text.len(),
                        new_rope: new_rope.clone(),
                        version: *version
                    }))
            )).await.unwrap();
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
            },
            ServiceManagerMessage::UpdateStaticAnalysisRope { analysis_rope, version } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_static_analysis_rope(analysis_rope, version) {
                        cx.notify();
                    }
                });
            },
            ServiceManagerMessage::UpdateCompileDiagnostics { diagnostics, version } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_compile_diagnostics(diagnostics, version) {
                        cx.notify();
                    }
                });
            },
            ServiceManagerMessage::UpdateRuntimeDiagnostics { diagnostics, version } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_runtime_diagnostics(diagnostics, version) {
                        cx.notify();
                    }
                });
            }
            ServiceManagerMessage::UpdateAutocompleteSuggestions { suggestions, cursor, version } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_autocomplete_state(suggestions, version, cursor) {
                        cx.notify();
                    }
                });
            }
            ServiceManagerMessage::UpdateParameterHintPosition { hint, cursor, version } => {
                self.textual_state.update(cx, |state, cx| {
                    if state.set_parameter_position_state(hint, version, cursor) {
                        cx.notify();
                    }
                });
            }
            ServiceManagerMessage::UpdateByteCode => {
                // currently no-op
            }
            ServiceManagerMessage::ExecutionStateUpdated => {
                // currently no-op
            }
        }
    }

    pub fn invalidate_dependencies(&mut self, physical_path: Option<PathBuf>, live_ropes: HashMap<PathBuf, (Rope<Attribute<Token>>, Rope<TextAggregate>)>) {
        smol::block_on(async {
            self.compilation_tx.send(
                CompilationMessage::RecheckDependencies {
                    physical_path,
                    open_documents: live_ropes
                }
            ).await.unwrap();
        })
    }
}
