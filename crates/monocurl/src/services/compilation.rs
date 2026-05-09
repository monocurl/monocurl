use std::collections::HashMap;
use std::path::PathBuf;

use compiler::cache::CompilerCache;
use compiler::compiler::{
    CompileResult, CursorIdentifierType, SymbolFunctionInfo, compile, static_analysis_rope,
};
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
    SlideInfo,
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

    fn slide_info_from_parse(
        parse_artifacts: &ParseArtifacts,
        text_rope: &Rope<TextAggregate>,
    ) -> Vec<SlideInfo> {
        fn line_end_location(text_rope: &Rope<TextAggregate>, line: usize) -> Location8 {
            let offset = text_rope
                .utf8_line_pos_prefix(line, usize::MAX)
                .bytes_utf8
                .min(text_rope.codeunits());
            let summary = text_rope.utf8_prefix_summary(offset);
            Location8 {
                row: summary.newlines,
                col: summary.bytes_utf8_since_newline,
            }
        }

        parse_artifacts
            .root_slides
            .iter()
            .map(|slide| {
                let start = slide.keyword_span.start;
                let line = text_rope.utf8_prefix_summary(start).newlines;
                SlideInfo {
                    start_offset: start,
                    source_range: slide.source_range.clone(),
                    line,
                    header_end: line_end_location(text_rope, line),
                }
            })
            .collect()
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
        let slides = Self::slide_info_from_parse(&parse_artifacts, &text_rope);
        let compile_result = compile(compile_state, cursor_pos, &parsed_bundles);
        let analysis_rope = static_analysis_rope(&compile_result, text_rope.codeunits());

        self.sm_tx
            .send(ServiceManagerMessage::UpdateSlideInfo { slides, version })
            .await
            .unwrap();

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

#[cfg(test)]
mod tests {
    use super::*;
    use lexer::lexer::Lexer;
    use structs::rope::RLEData;

    fn lex_rope(src: &str) -> Rope<Attribute<LexData>> {
        Rope::default().replace_range(
            0..0,
            Lexer::new(src.chars()).map(|(attribute, codeunits)| RLEData {
                codeunits,
                attribute,
            }),
        )
    }

    #[test]
    fn slide_info_from_parse_tracks_header_end_and_section_ranges() {
        let src = "mesh c = circle()\nslide \"Intro\"\n  show c\nslide second\n  hide c\n";
        let text_rope = Rope::from_str(src);
        let mut parse_state = ParseImportContext {
            root_file_path: PathBuf::from("scene.mcs"),
            open_tab_ropes: Default::default(),
            cached_parses: Default::default(),
        };
        let (_, parse_artifacts) =
            Parser::parse(&mut parse_state, lex_rope(src), text_rope.clone(), None);
        let slides = CompilationService::slide_info_from_parse(&parse_artifacts, &text_rope);

        assert_eq!(slides.len(), 2);
        assert_eq!(slides[0].line, 1);
        assert_eq!(slides[0].header_end, Location8 { row: 1, col: 13 });
        assert_eq!(slides[0].source_range.start, slides[0].start_offset);
        assert_eq!(slides[0].source_range.end, slides[1].start_offset);
        assert_eq!(slides[1].line, 3);
        assert_eq!(slides[1].header_end, Location8 { row: 3, col: 12 });
        assert_eq!(slides[1].source_range.end, src.len());
    }
}
