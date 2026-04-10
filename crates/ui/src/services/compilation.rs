use std::collections::HashMap;
use std::path::PathBuf;
use std::ptr::replace;

use compiler::{CompileResult, CursorIdentifierType, compile};
use futures::{SinkExt, StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use lexer::token::Token;
use parser::import_context::ParseImportContext;
use parser::parser::{ParseArtifacts, Parser};
use structs::rope::{Attribute, Rope, TextAggregate};
use structs::text::{Location8};

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
        _version: usize,
    },
    RecheckDependencies {
        physical_path: Option<PathBuf>,
        open_documents: HashMap<PathBuf, (Rope<Attribute<LexData>>, Rope<TextAggregate>)>,
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

    async fn emit_autocomplete(&mut self, parse: &ParseArtifacts, compile: &CompileResult, latest_cursor: Cursor, version: usize) {
        fn suggestion(s: impl Into<String>, replacement: impl Into<String>, cursor_delta: usize) -> AutoCompleteItem {
            let replacement = replacement.into();
            let rlen = replacement.len();
            AutoCompleteItem {
                head: s.into(),
                replacement: replacement,
                cursor_anchor_delta: Location8 { row: 0, col: rlen - cursor_delta},
                cursor_head_delta: Location8 { row: 0, col: rlen - cursor_delta },
                category: AutoCompleteCategory::Keyword
            }
        }

        let mut suggestions = vec![];
        for token in &parse.cursor_possibilities {
            if let Some(s) = token.autocomplete() {
                let (replacement, delta) = match token {
                    Token::Block | Token::Anim  => (s.to_string() + " {}", 1),
                    Token::If | Token::While | Token::For => (s.to_string() + " ()", 1),
                    _ => (s.to_string() + " ", 0),
                };
                suggestions.push(suggestion(s, replacement, delta));
            }
        }

        if parse.cursor_possibilities.contains(&Token::Identifier) {
            for ident in &compile.possible_cursor_identifiers {
                let (replacement, delta) = match ident.identifier_type {
                    CursorIdentifierType::Lambda => (ident.name.clone() + "()", 1),
                    CursorIdentifierType::Operator => (ident.name.clone() + "{}", 1),
                    _ => (ident.name.clone(), 0),
                };
                suggestions.push(suggestion(&ident.name, replacement, delta))
            }
        }

        self.sm_tx.send(ServiceManagerMessage::UpdateAutocompleteSuggestions { suggestions, cursor: latest_cursor, version }).await.unwrap();
    }

    async fn emit_diagnostics(&mut self, parse: &ParseArtifacts, compile: &CompileResult, version: usize) {
        let mut diagnostics = vec![];

        for parse_error in &parse.error_diagnostics {
            diagnostics.push(Diagnostic {
                message: parse_error.message.clone(),
                span: parse_error.span.clone(),
                dtype: DiagnosticType::CompileTimeError,
                title: parse_error.title.clone()
            });
        }

        for compile_error in &compile.errors {
            diagnostics.push(Diagnostic {
                message: compile_error.message.clone(),
                span: compile_error.span.clone(),
                dtype: DiagnosticType::CompileTimeError,
                title: "Compile Error".into()
            });
        }

        self.sm_tx.send(ServiceManagerMessage::UpdateCompileDiagnostics {
            diagnostics,
            version,
        }).await.unwrap();
    }

    async fn recompile(&mut self, parse_state: &mut ParseImportContext, latest_cursor: Cursor, text_rope: Rope<TextAggregate>, lex_rope: Rope<Attribute<LexData>>,  version: usize) {
        let cursor_pos = if latest_cursor.is_empty() {
            let l = latest_cursor.head;
            Some(text_rope.utf8_line_pos_prefix(l.row, l.col).bytes_utf8)
        }
        else {
            None
        };

        let (parsed_bundles, parse_artifacts) = Parser::parse(parse_state, lex_rope, text_rope.clone(), cursor_pos);
        let compile_result = compile(cursor_pos, &parsed_bundles);

        self.emit_autocomplete(&parse_artifacts, &compile_result, latest_cursor, version).await;
        self.emit_diagnostics(&parse_artifacts, &compile_result, version).await;

        let okay_bytecode = parse_artifacts.error_diagnostics.is_empty() && compile_result.errors.is_empty();
        if okay_bytecode {
            // self.sm_tx.send(ServiceManagerMessage::UpdateBytecode {
            //     bytecode: compile_result.bytecode.clone(),
            //     version,
            // }).await.unwrap();
        }
    }

    pub async fn run(mut self) {
        let mut latest_cursor = Cursor::default();
        let mut latest_text_rope = Rope::default();
        let mut latest_lex_rope = Rope::default();
        let mut latest_version = 0;

        let mut parse_state = ParseImportContext::default();

        while let Some(message) = self.rx.next().await {
            // we should do a select! here for best performance
            match message {
                CompilationMessage::UpdateLexRope { lex_rope, version, for_text_rope  } => {
                    latest_text_rope = for_text_rope.clone();
                    latest_lex_rope = lex_rope.clone();
                    latest_version = version;

                    self.recompile(&mut parse_state, latest_cursor, latest_text_rope.clone(), lex_rope, version).await;
                },
                CompilationMessage::UpdateCursor { cursor: c, _version: _} => {
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
                CompilationMessage::RecheckDependencies { physical_path, open_documents  } => {
                    parse_state = ParseImportContext {
                        root_file_user_path: physical_path,
                        open_tab_ropes: open_documents,
                        cached_parses: Default::default()
                    };

                    self.recompile(&mut parse_state, latest_cursor, latest_text_rope.clone(), latest_lex_rope.clone(), latest_version).await;
                }
            }
        }
    }
}
