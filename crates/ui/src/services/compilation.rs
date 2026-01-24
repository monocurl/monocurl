use std::path::PathBuf;

use futures::{SinkExt, StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use lexer::token::Token;
use parser::parser::{Parser, SectionParser};
use structs::rope::{Attribute, Rope, TextAggregate};
use structs::text::{Location8, Span8};

use crate::state::diagnostics::{Diagnostic, DiagnosticType};
use crate::state::textual_state::{AutoCompleteCategory, AutoCompleteItem, Cursor, ParameterPositionHint};
use crate::{services::{ServiceManagerMessage, execution::ExecutionMessage}, state::{textual_state::{LexData}}};

pub enum CompilationMessage {
    UpdateLexRope {
        lex_rope: Rope<Attribute<LexData>>,
        for_text_rope: Rope<TextAggregate>,
        version: usize,
    },
    UpdateCursor {
        cursor: Cursor,
        version: usize,
    },
    RecheckDependencies {
        open_documents: Vec<(PathBuf, Rope<TextAggregate>)>
    }
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
        let mut latest_cursor = Cursor::default();
        let mut latest_text_rope = Rope::default();
        let mut latest_version = 0;

        let mut open_files: Vec<Rope<TextAggregate>> = vec![];

        while let Some(message) = self.rx.next().await {
            // we should do a select! here for best performance
            match message {
                CompilationMessage::UpdateLexRope { lex_rope, version, for_text_rope  } => {
                    latest_text_rope = for_text_rope.clone();
                    latest_version = version;

                    // reparse + recompile
                    let mut utf8 = 0;
                    let tokens = lex_rope.iterator(0)
                        .map(|(len, tok)| {
                            let start = utf8;
                            utf8 += len;
                            (tok, Span8 {
                                start,
                                end: utf8,
                            })
                        })
                        .filter(|(tok, _)| tok != &Token::Whitespace && tok != &Token::Comment)
                        .collect();
                    let mut p = SectionParser::new(
                        tokens,
                        for_text_rope.clone(),
                        parser::ast::SectionType::Slide,
                        if latest_cursor.is_empty() {
                            let l = latest_cursor.head;
                            Some(for_text_rope.utf8_line_pos_prefix(l.row, l.col).bytes_utf8)
                            // None
                        }
                        else {
                            None
                        }
                    );
                    p.parse_section();
                    let artifacts = p.artifacts();
                    let cursor_poss = artifacts.cursor_possibilities;
                    let diags = artifacts.diagnostics;

                    self.sm_tx.send(ServiceManagerMessage::UpdateCompileDiagnostics {
                        diagnostics: diags.into_iter()
                            .map(|d| Diagnostic {
                                message: d.message.clone(),
                                span: d.span,
                                dtype: if d.is_error {
                                    DiagnosticType::CompileTimeError
                                } else {
                                    DiagnosticType::CompileTimeWarning
                                },
                                title: d.title
                            }).collect(),
                        version,
                    }).await.unwrap();
                    let item = |s: &str| AutoCompleteItem {
                        head: s.to_string(),
                        replacement: s.to_string() + " ",
                        cursor_anchor_delta: Location8 { row: 0, col: s.len() + 1 },
                        cursor_head_delta: Location8 { row: 0, col: s.len() + 1 },
                        category: AutoCompleteCategory::Keyword
                    };

                    println!("Sending {:?} autocomplete suggestions",
                        cursor_poss.iter()
                            .map(|token| token.description())
                            .collect::<Vec<_>>()
                    );
                    let suggestions = cursor_poss.iter()
                        .map(|token| item(token.description()))
                        .collect();


                    self.sm_tx.send(ServiceManagerMessage::UpdateAutocompleteSuggestions { suggestions, cursor: latest_cursor, version }).await.unwrap();
                    // let _ = self.execution_tx.send(ExecutionMessage::UpdateBytecode).await;
                },
                CompilationMessage::UpdateCursor { cursor: c, version: _} => {
                    latest_cursor = c;

                    let hint =  {
                        let content: String = latest_text_rope.iterator(0).collect();
                        let cursor_byte_index = latest_text_rope.utf8_line_pos_prefix(latest_cursor.head.row, latest_cursor.head.col).bytes_utf8;

                        // Find the opening parenthesis before cursor
                        let before_cursor = &content[..cursor_byte_index];

                        if let Some(open_paren_pos) = before_cursor.rfind('(') {
                            // Check if there's a closing paren before the next opening paren
                            let after_open = &before_cursor[open_paren_pos + 1..];
                            if after_open.contains(')') {
                                None // We're not in an active function call
                            } else {
                                // Find function name before the opening paren
                                let before_open = &before_cursor[..open_paren_pos];
                                let func_name_start = before_open.rfind(|c: char| !c.is_alphanumeric() && c != '_')
                                    .map(|pos| pos + 1)
                                    .unwrap_or(0);
                                let func_name = before_open[func_name_start..].trim();

                                if func_name.is_empty() {
                                    None
                                } else {
                                    // Count commas to determine active parameter index
                                    let active_index = after_open.chars().filter(|&c| c == ',').count();

                                    // Try to find the closing paren to extract all args
                                    let after_cursor = &content[cursor_byte_index..];
                                    let close_paren_pos = after_cursor.find(')');

                                    let args = if let Some(close_pos) = close_paren_pos {
                                        // Extract the full argument list
                                        let full_args = &content[open_paren_pos + 1..cursor_byte_index + close_pos];
                                        full_args.split(',')
                                            .map(|s| s.trim().to_string())
                                            .filter(|s| !s.is_empty())
                                            .collect::<Vec<_>>()
                                    } else {
                                        // No closing paren yet, create placeholder args based on commas
                                        (0..=active_index).map(|i| format!("arg{}", i)).collect()
                                    };

                                    // Calculate function start position
                                    let func_start_byte = open_paren_pos - func_name.len();
                                    let func_start_loc = latest_text_rope.utf8_prefix_summary(func_start_byte);

                                    Some(ParameterPositionHint {
                                        name: func_name.to_string(),
                                        args: if args.is_empty() { vec!["...".to_string()] } else { args.clone() },
                                        active_index: active_index.min(args.len().saturating_sub(1)),
                                        function_start: Location8 {
                                            row: func_start_loc.newlines,
                                            col: func_start_loc.bytes_utf8_since_newline,
                                        }
                                    })
                                }
                            }
                        } else {
                            None
                        }
                    };

                    self.sm_tx.send(ServiceManagerMessage::UpdateParameterHintPosition {
                        hint,
                        cursor: latest_cursor,
                        version: latest_version
                    }).await.unwrap();
                }
                CompilationMessage::RecheckDependencies { .. } => {
                    // recompile if any new dependencies
                }
            }
        }
    }
}
