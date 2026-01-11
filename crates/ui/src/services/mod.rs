
use gpui::{App, AppContext, Context, Entity};
use structs::{rope::{RLEAggregate, Rope, TextAggregate}, text::Span8};
use futures::{SinkExt, StreamExt, channel::mpsc::{UnboundedSender}};
use futures::channel::mpsc::unbounded;
use crate::{services::{compilation::{CompilationMessage, CompilationService}, execution::{ExecutionMessage, ExecutionService}, lexing::{LexingMessage, LexingService}}, state::{diagnostics::Diagnostic, execution_state::ExecutionState, textual_state::{AutoCompleteItem, Cursor, LexData, TextualState}}};

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
        lex_rope: Rope<RLEAggregate<LexData>>,
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
            let wsm = weak_service_manager.clone();
            state.add_text_listener(move |span, new, rope, version, app| {
                weak_service_manager.update(app, |service_manager, _| {
                    service_manager.on_text_rope_updated(span, new, rope.clone(), version);
                }).unwrap();
            });
            state.add_cursor_listener(move |cursor, app| {
                wsm.update(app, |service_manager, _| {
                    service_manager.on_text_cursor_updated(*cursor);
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

    fn on_text_rope_updated(&mut self, old: Span8, new_text: &str, new_rope: Rope<TextAggregate>, version: usize) {
        smol::block_on(self.lexing_tx.send(LexingMessage::UpdateRope {
            old,
            new: new_text.len(),
            new_rope,
            version,
        })).unwrap();
    }

    fn on_text_cursor_updated(&mut self, cursor: Cursor) {
        smol::block_on(self.compilation_tx.send(CompilationMessage::UpdateCursor {
            cursor,
        })).unwrap();
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
            ServiceManagerMessage::UpdateByteCode => {
                // currently no-op
            }
            ServiceManagerMessage::ExecutionStateUpdated => {
                // currently no-op
            }
        }
    }
}
