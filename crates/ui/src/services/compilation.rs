use std::collections::HashMap;
use std::path::PathBuf;

use compiler::cache::CompilerCache;
use compiler::compiler::{CompileResult, CursorIdentifierType, SymbolFunctionInfo, compile};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::{SinkExt, StreamExt};
use lexer::token::Token;
use parser::import_context::ParseImportContext;
use parser::parser::{ParseArtifacts, Parser};
use structs::rope::{Attribute, Rope, TextAggregate};
use structs::text::{Count8, Location8, Span8};

use crate::state::diagnostics::{Diagnostic, DiagnosticType};
use crate::state::textual_state::{
    AutoCompleteCategory, AutoCompleteItem, Cursor, ParameterHintArg, ParameterPositionHint,
};
use crate::{
    services::{ServiceManagerMessage, execution::ExecutionMessage},
    state::textual_state::LexData,
};

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
    },
}

pub struct CompilationService {
    rx: UnboundedReceiver<CompilationMessage>,
    execution_tx: UnboundedSender<ExecutionMessage>,
    sm_tx: UnboundedSender<ServiceManagerMessage>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BatchCompileAction {
    None,
    Recompile,
}

impl CompilationService {
    pub fn new(
        rx: UnboundedReceiver<CompilationMessage>,
        execution_tx: UnboundedSender<ExecutionMessage>,
        sm_tx: UnboundedSender<ServiceManagerMessage>,
    ) -> Self {
        Self {
            rx,
            execution_tx,
            sm_tx,
        }
    }

    fn cursor_pos(&self, cursor: Cursor, text_rope: &Rope<TextAggregate>) -> Option<Count8> {
        if cursor.is_empty() {
            let l = cursor.head;
            Some(text_rope.utf8_line_pos_prefix(l.row, l.col).bytes_utf8)
        } else {
            None
        }
    }

    async fn emit_parameter_hint(
        &mut self,
        latest_cursor: Cursor,
        last_compile_result: &CompileResult,
        latest_text_rope: Rope<TextAggregate>,
        lex_rope: Rope<Attribute<LexData>>,
        latest_version: usize,
    ) {
        let find_active_index =
            |argument_spans: &[Span8], cursor_pos: Count8, true_arg_count: usize| -> usize {
                // actual index
                let base = {
                    let last_starting_before = argument_spans
                        .iter()
                        .rposition(|span| span.start <= cursor_pos);

                    let first_ending_after =
                        argument_spans.iter().position(|span| span.end > cursor_pos);

                    // if any range contains it, then clearly its that one
                    // otherwise, look for comma separating the two
                    match (last_starting_before, first_ending_after) {
                        (Some(u), Some(v)) => {
                            if u == v {
                                u
                            } else {
                                let mut pos = argument_spans[u].end;
                                let mut comma_pos = argument_spans[v].start;
                                for (chunk, tok) in lex_rope.iterator(pos) {
                                    if pos >= argument_spans[v].start {
                                        break;
                                    }
                                    if tok == Token::Comma {
                                        comma_pos = pos;
                                    }
                                    pos += chunk;
                                }
                                if cursor_pos <= comma_pos { u } else { v }
                            }
                        }
                        (Some(u), None) => u,
                        (None, Some(v)) => v,
                        (None, None) => 0,
                    }
                };

                base.min(true_arg_count - 1)
            };

        let hint = self
            .cursor_pos(latest_cursor, &latest_text_rope)
            .and_then(|cursor| {
                last_compile_result
                    .root_references
                    .iter()
                    .find(|reference| {
                        reference
                            .invocation_spans
                            .as_ref()
                            .is_some_and(|inv| inv.0.contains(&cursor))
                    })
                    .and_then(|reference| {
                        let sym = &reference.symbol;
                        match &sym.function_info {
                            SymbolFunctionInfo::Lambda { args }
                            | SymbolFunctionInfo::Operator { args } => {
                                let func_start_loc =
                                    latest_text_rope.utf8_prefix_summary(reference.span.start);

                                let offset = if matches!(
                                    sym.function_info,
                                    SymbolFunctionInfo::Operator { .. }
                                ) {
                                    1
                                } else {
                                    0
                                };
                                let args = if args.len() <= offset {
                                    vec![ParameterHintArg {
                                        name: String::new(),
                                        has_default: false,
                                        is_reference: false,
                                    }]
                                } else {
                                    args[offset..]
                                        .iter()
                                        .map(|arg| ParameterHintArg {
                                            name: arg.name.clone(),
                                            has_default: arg.has_default,
                                            is_reference: arg.is_reference,
                                        })
                                        .collect()
                                };

                                let invoked_args = reference.invocation_spans.as_ref().unwrap();
                                let active_index =
                                    find_active_index(&invoked_args.1, cursor, args.len());

                                Some(ParameterPositionHint {
                                    name: sym.name.clone(),
                                    args,
                                    active_index,
                                    function_start: Location8 {
                                        row: func_start_loc.newlines,
                                        col: func_start_loc.bytes_utf8_since_newline,
                                    },
                                    is_operator: matches!(
                                        sym.function_info,
                                        SymbolFunctionInfo::Operator { .. }
                                    ),
                                })
                            }
                            SymbolFunctionInfo::None => None,
                        }
                    })
            });

        self.sm_tx
            .send(ServiceManagerMessage::UpdateParameterHintPosition {
                hint,
                cursor: latest_cursor,
                version: latest_version,
            })
            .await
            .unwrap();
    }

    async fn emit_autocomplete(
        &mut self,
        parse: &ParseArtifacts,
        compile: &CompileResult,
        latest_cursor: Cursor,
        version: usize,
    ) {
        fn suggestion(
            s: impl Into<String>,
            replacement: impl Into<String>,
            cursor_delta: usize,
        ) -> AutoCompleteItem {
            let replacement = replacement.into();
            let rlen = replacement.len();
            AutoCompleteItem {
                head: s.into(),
                replacement: replacement,
                cursor_anchor_delta: Location8 {
                    row: 0,
                    col: rlen - cursor_delta,
                },
                cursor_head_delta: Location8 {
                    row: 0,
                    col: rlen - cursor_delta,
                },
                category: AutoCompleteCategory::Keyword,
            }
        }

        let mut suggestions = vec![];
        for token in &parse.cursor_possibilities {
            if let Some(s) = token.autocomplete() {
                let (replacement, delta) = match token {
                    Token::Block | Token::Anim => (s.to_string() + " {}", 1),
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

        self.sm_tx
            .send(ServiceManagerMessage::UpdateAutocompleteSuggestions {
                suggestions,
                cursor: latest_cursor,
                version,
            })
            .await
            .unwrap();
    }

    async fn emit_diagnostics(
        &mut self,
        parse: &ParseArtifacts,
        compile: &CompileResult,
        version: usize,
    ) {
        let mut diagnostics = vec![];

        for parse_error in &parse.error_diagnostics {
            diagnostics.push(Diagnostic {
                message: parse_error.message.clone(),
                span: parse_error.span.clone(),
                dtype: DiagnosticType::CompileTimeError,
                title: parse_error.title.clone(),
            });
        }

        for compile_error in &compile.errors {
            diagnostics.push(Diagnostic {
                message: compile_error.message.clone(),
                span: compile_error.span.clone(),
                dtype: DiagnosticType::CompileTimeError,
                title: "Compile Error".into(),
            });
        }

        for compile_warning in &compile.warnings {
            diagnostics.push(Diagnostic {
                message: compile_warning.message.clone(),
                span: compile_warning.span.clone(),
                dtype: DiagnosticType::CompileTimeWarning,
                title: "Compile Warning".into(),
            });
        }

        self.sm_tx
            .send(ServiceManagerMessage::UpdateCompileDiagnostics {
                diagnostics,
                version,
            })
            .await
            .unwrap();
    }

    #[must_use]
    async fn recompile(
        &mut self,
        parse_state: &mut ParseImportContext,
        compile_state: &mut CompilerCache,
        latest_cursor: Cursor,
        text_rope: Rope<TextAggregate>,
        lex_rope: Rope<Attribute<LexData>>,
        version: usize,
    ) -> CompileResult {
        let cursor_pos = self.cursor_pos(latest_cursor, &text_rope);

        let (parsed_bundles, parse_artifacts) =
            Parser::parse(parse_state, lex_rope.clone(), text_rope.clone(), cursor_pos);
        let compile_result = compile(compile_state, cursor_pos, &parsed_bundles);

        self.emit_autocomplete(&parse_artifacts, &compile_result, latest_cursor, version)
            .await;
        self.emit_diagnostics(&parse_artifacts, &compile_result, version)
            .await;
        self.emit_parameter_hint(
            latest_cursor,
            &compile_result,
            text_rope.clone(),
            lex_rope,
            version,
        )
        .await;

        let okay_bytecode =
            parse_artifacts.error_diagnostics.is_empty() && compile_result.errors.is_empty();
        self.execution_tx
            .send(ExecutionMessage::UpdateBytecode {
                bytecode: okay_bytecode.then_some(compile_result.bytecode.clone()),
                root_text_rope: text_rope.clone(),
                version,
            })
            .await
            .unwrap();

        return compile_result;
    }

    pub async fn run(mut self) {
        let mut latest_cursor = Cursor::default();
        let mut latest_text_rope = Rope::default();
        let mut latest_lex_rope = Rope::default();
        let mut latest_version = 0;

        let mut parse_state = ParseImportContext::default();
        let mut compiler_state = CompilerCache::default();

        let mut last_compile_result = CompileResult::default();

        while let Some(message) = self.rx.next().await {
            let mut compile_action = BatchCompileAction::None;
            let mut emit_parameter_hint = false;

            for message in std::iter::once(message)
                .chain(std::iter::from_fn(|| self.rx.try_next().ok().flatten()))
            {
                match message {
                    CompilationMessage::UpdateLexRope {
                        lex_rope,
                        version,
                        for_text_rope,
                    } => {
                        latest_text_rope = for_text_rope;
                        latest_lex_rope = lex_rope;
                        latest_version = version;
                        compile_action = BatchCompileAction::Recompile;
                    }
                    CompilationMessage::UpdateCursor {
                        cursor: c,
                        _version: _,
                    } => {
                        latest_cursor = c;
                        emit_parameter_hint = true;
                    }
                    CompilationMessage::RecheckDependencies {
                        physical_path,
                        open_documents,
                    } => {
                        parse_state = ParseImportContext {
                            root_file_user_path: physical_path,
                            open_tab_ropes: open_documents,
                            cached_parses: Default::default(),
                        };
                        compile_action = BatchCompileAction::Recompile;
                    }
                }
            }

            if compile_action == BatchCompileAction::Recompile {
                last_compile_result = self
                    .recompile(
                        &mut parse_state,
                        &mut compiler_state,
                        latest_cursor,
                        latest_text_rope.clone(),
                        latest_lex_rope.clone(),
                        latest_version,
                    )
                    .await;
                continue;
            }

            if emit_parameter_hint {
                self.emit_parameter_hint(
                    latest_cursor,
                    &last_compile_result,
                    latest_text_rope.clone(),
                    latest_lex_rope.clone(),
                    latest_version,
                )
                .await;
            }
        }
    }
}
