use std::path::PathBuf;

use futures::{SinkExt, StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use lexer::token::Token;
use parser::ast::{BinaryOperator, Declaration, Expression, If, LambdaInvocation, OperatorInvocation, Section, Statement, UnaryPreOperator};
use parser::parser::{Parser, SectionParser};
use structs::rope::{Attribute, RLEData, Rope, TextAggregate};
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
                    let q = p.parse_section();
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

                    let suggestions = cursor_poss.iter()
                        .map(|token| item(token.description()))
                        .collect();

                    let mut should_italicize_rope = Rope::default();
                    should_italicize_rope = should_italicize_rope.replace_range(0..0,
                        [RLEData { codeunits: latest_text_rope.codeunits(), attribute: false }].into_iter()
                    );

                    {
                        fn ast_walk3(q: Expression, mut r: Rope<Attribute<bool>>) -> Rope<Attribute<bool>> {
                            match q {
                                // Expression::OperatorInvocation(OperatorInvocation {
                                //     operator: _,
                                //     arguments,
                                //     operand,
                                // }) => {
                                //     for (span, arg) in arguments.1.iter() {
                                //         if let Some((span, _label)) = span {
                                //             r = r.replace_range(span.clone(), [RLEData { codeunits: span.len(), attribute: true }].into_iter());
                                //         }
                                //     }

                                //     return ast_walk3(*operand.1, r)
                                // },
                                Expression::LambdaInvocation(LambdaInvocation {
                                    lambda: operator,
                                    arguments: _,
                                }) => {
                                    return r.replace_range(operator.0.clone(), [RLEData { codeunits: operator.0.len(), attribute: true }].into_iter())
                                },
                                Expression::BinaryOperator(BinaryOperator {
                                    lhs,
                                    op_type: _,
                                    rhs,
                                }) => {
                                    r = ast_walk3(*lhs.1, r);
                                    return ast_walk3(*rhs.1, r);
                                },
                                Expression::UnaryPreOperator(UnaryPreOperator {
                                    op_type: _,
                                    operand,
                                }) => {
                                    return ast_walk3(*operand.1, r);
                                }
                                _ => return r
                            }
                        }

                        fn ast_walk2(q: Declaration, r: Rope<Attribute<bool>>) -> Rope<Attribute<bool>> {
                            return ast_walk3(q.value.1, r);
                        }

                        fn ast_walk1(q: Statement, r: Rope<Attribute<bool>>) -> Rope<Attribute<bool>> {
                            match q {
                                Statement::Expression(e) => return ast_walk3(e, r),
                                Statement::Declaration(d) => return ast_walk2(d, r),
                                _ => return r
                            }
                        }
                        fn ast_walk(q: Section, mut r: Rope<Attribute<bool>>) -> Rope<Attribute<bool>> {
                            for (_, child) in q.body {
                                r = ast_walk1(child, r);
                            }
                            return r
                        }

                        if let Ok(result) = q {
                            should_italicize_rope = ast_walk(result, should_italicize_rope);
                            self.sm_tx.send(ServiceManagerMessage::UpdateStaticAnalysisRope { analysis_rope: should_italicize_rope, version }).await.unwrap();
                        }
                    }


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
