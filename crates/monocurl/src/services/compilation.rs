use std::collections::HashMap;
use std::path::PathBuf;

use compiler::cache::CompilerCache;
use compiler::compiler::{CompileResult, CursorIdentifierType, SymbolFunctionInfo, compile};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::{SinkExt, StreamExt};
use lexer::token::Token;
use parser::import_context::ParseImportContext;
use parser::parser::{ParseArtifacts, Parser};
use structs::rope::{Attribute, RLEData, Rope, TextAggregate};
use structs::text::{Count8, Location8, Span8};

use crate::state::diagnostics::{Diagnostic, DiagnosticType};
use crate::state::textual_state::{
    AutoCompleteCategory, AutoCompleteItem, Cursor, ParameterHintArg, ParameterPositionHint,
    StaticAnalysisData,
};
use crate::{
    services::{ServiceManagerMessage, execution::ExecutionMessage},
    state::textual_state::LexData,
};

mod autocomplete;
mod parameter_hint;

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
        physical_path: PathBuf,
        open_documents: HashMap<PathBuf, (Rope<Attribute<LexData>>, Rope<TextAggregate>)>,
    },
}

pub struct CompilationService {
    rx: UnboundedReceiver<CompilationMessage>,
    execution_tx: UnboundedSender<ExecutionMessage>,
    sm_tx: UnboundedSender<ServiceManagerMessage>,
    root_path: PathBuf,
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
        root_path: PathBuf,
    ) -> Self {
        Self {
            rx,
            execution_tx,
            sm_tx,
            root_path,
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

    fn static_analysis_rope(
        &self,
        compile: &CompileResult,
        text_rope: &Rope<TextAggregate>,
    ) -> Rope<Attribute<StaticAnalysisData>> {
        let mut rope = Rope::default();

        if text_rope.codeunits() == 0 {
            return rope;
        }

        rope = rope.replace_range(
            0..0,
            std::iter::once(RLEData {
                codeunits: text_rope.codeunits(),
                attribute: StaticAnalysisData::None,
            }),
        );

        for reference in &compile.root_references {
            let analysis = match (&reference.symbol.function_info, &reference.invocation_spans) {
                (compiler::compiler::SymbolFunctionInfo::Lambda { .. }, Some(_)) => {
                    StaticAnalysisData::FunctionInvocation
                }
                (compiler::compiler::SymbolFunctionInfo::Operator { .. }, Some(_)) => {
                    StaticAnalysisData::OperatorInvocation
                }
                _ => continue,
            };

            rope = rope.replace_range(
                reference.span.clone(),
                std::iter::once(RLEData {
                    codeunits: reference.span.len(),
                    attribute: analysis,
                }),
            );
        }

        rope
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
        let analysis_rope = self.static_analysis_rope(&compile_result, &text_rope);

        self.sm_tx
            .send(ServiceManagerMessage::UpdateStaticAnalysisRope {
                analysis_rope,
                version,
            })
            .await
            .unwrap();

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

        let mut parse_state = ParseImportContext {
            root_file_path: self.root_path.clone(),
            open_tab_ropes: Default::default(),
            cached_parses: Default::default(),
        };
        let mut compiler_state = CompilerCache::default();

        let mut last_compile_result = CompileResult::default();

        while let Some(message) = self.rx.next().await {
            let mut compile_action = BatchCompileAction::None;
            let mut emit_parameter_hint = false;

            for message in
                std::iter::once(message).chain(std::iter::from_fn(|| self.rx.try_recv().ok()))
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
                            root_file_path: physical_path,
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
