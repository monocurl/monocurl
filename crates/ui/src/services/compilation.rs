use futures::{SinkExt, StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use lexer::token::Token;
use structs::rope::{RLEAggregate, Rope, TextAggregate};
use structs::text::{Location8, Span8};

use crate::state::diagnostics::{Diagnostic, DiagnosticType};
use crate::state::textual_state::{AutoCompleteItem, AutoCompleteCategory, Cursor};
use crate::{services::{ServiceManagerMessage, execution::ExecutionMessage}, state::{textual_state::{LexData}}};

pub enum CompilationMessage {
    UpdateLexRope {
        lex_rope: Rope<RLEAggregate<LexData>>,
        for_text_rope: Rope<TextAggregate>,
        version: usize,
    },
    UpdateCursor {
        cursor: Cursor
    },
    RecheckDependencies
}

pub struct CompilationService {
    rx: UnboundedReceiver<CompilationMessage>,
    execution_tx: UnboundedSender<ExecutionMessage>,
    sm_tx: UnboundedSender<ServiceManagerMessage>,
}

impl CompilationService {
    pub fn new(rx: UnboundedReceiver<CompilationMessage>, execution_tx: UnboundedSender<ExecutionMessage>, sm_tx: UnboundedSender<ServiceManagerMessage>) -> Self {
        Self {
            rx,
            execution_tx,
            sm_tx,
        }
    }

    pub async fn run(mut self) {
        let mut cursor = Cursor::default();
        while let Some(message) = self.rx.next().await {
            // we should do a select! here for best performance
            match message {
                CompilationMessage::UpdateLexRope { lex_rope, version, for_text_rope  } => {
                    // reparse + recompile
                    let mut diagnostics = vec![];

                    let mut pos = 0;
                    for (len, token) in lex_rope.iterator(0) {
                        let string: String = for_text_rope.iterator_range(pos..pos+len).collect();
                        match token {
                            Token::StringLiteral => {
                                if !string.ends_with('"') || string.len() < 2 {
                                    diagnostics.push(Diagnostic {
                                        dtype: DiagnosticType::CompileTimeError,
                                        span: Span8 {
                                            start: pos,
                                            end: pos + len,
                                        },
                                        title: "Unterminated String Literal".to_string(),
                                        message: "message".to_string()
                                    });
                                }
                            },
                            Token::CharLiteral => {
                                if !string.ends_with('\'') || string.len() < 3 {
                                    diagnostics.push(Diagnostic {
                                        dtype: DiagnosticType::CompileTimeError,
                                        span: Span8 {
                                            start: pos,
                                            end: pos + len,
                                        },
                                        title: "Unterminated Char Literal".to_string(),
                                        message: "message".to_string()
                                    });
                                }
                            },
                            _ => {}
                        }
                        pos += len;
                    }

                    self.sm_tx.send(ServiceManagerMessage::UpdateCompileDiagnostics {
                        diagnostics,
                        version,
                    }).await.unwrap();
                    let item = |s: &str| AutoCompleteItem {
                        head: s.to_string(),
                        replacement: s.to_string() + " ",
                        cursor_anchor_delta: Location8 { row: 0, col: s.len() + 1 },
                        cursor_head_delta: Location8 { row: 0, col: s.len() + 1 },
                        category: AutoCompleteCategory::Keyword
                    };
                    let suggestions = vec![
                        item("fn"),
                        item("let"),
                        item("var"),
                        item("mut"),
                        item("struct"),
                        item("enum"),
                        item("impl"),
                        item("for"),
                        item("while"),
                        item("loop"),
                        item("if"),
                        item("else"),
                        item("match"),
                        item("return"),
                        item("break"),
                        item("continue"),
                        item("use"),
                        item("mod"),
                        item("pub"),
                        item("crate"),
                        item("super"),
                        item("self"),
                        item("Self"),
                        item("const"),
                        item("static"),
                        item("async"),
                        item("await"),
                        item("tree"),
                        item("leaf"),
                        item("mesh"),
                        item("stroke"),
                        item("fill")
                    ];
                    self.sm_tx.send(ServiceManagerMessage::UpdateAutocompleteSuggestions { suggestions, cursor, version }).await.unwrap();
                    // let _ = self.execution_tx.send(ExecutionMessage::UpdateBytecode).await;
                },
                CompilationMessage::UpdateCursor { cursor: c } => {
                    cursor = c;
                }
                CompilationMessage::RecheckDependencies => {
                    // recompile if any new dependencies
                }
            }
        }
    }
}
