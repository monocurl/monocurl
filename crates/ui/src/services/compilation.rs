use futures::{SinkExt, StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use lexer::token::Token;
use structs::rope::{RLEAggregate, Rope, TextAggregate};
use structs::text::Span8;

use crate::state::diagnostics::{Diagnostic, DiagnosticType};
use crate::{services::{ServiceManagerMessage, execution::ExecutionMessage}, state::{textual_state::{LexData}}};

pub enum CompilationMessage {
    UpdateLexRope {
        lex_rope: Rope<RLEAggregate<LexData>>,
        for_text_rope: Rope<TextAggregate>,
        version: usize,
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
        while let Some(message) = self.rx.next().await {
            match message {
                CompilationMessage::UpdateLexRope { lex_rope, version, for_text_rope  } => {
                    // reparse + recompile
                    let mut diagnostics = vec![];

                    let mut pos = 0;
                    let mut token_count = 0;
                    for (len, token) in lex_rope.iterator(0) {
                        token_count += 1;
                        match token {
                            Token::StringLiteral => {
                                let string: String = for_text_rope.iterator_range(pos..pos+len).collect();
                                if !string.ends_with('"') || string.len() < 2 {
                                    diagnostics.push(Diagnostic {
                                        dtype: DiagnosticType::CompileTimeError,
                                        span: Span8 {
                                            start: pos,
                                            end: pos + len,
                                        },
                                        title: "Unterminated String Literal".to_string(),
                                        message: "".to_string()
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
                    // let _ = self.execution_tx.send(ExecutionMessage::UpdateBytecode).await;
                },
                CompilationMessage::RecheckDependencies => {
                    // recompile if any new dependencies
                }
            }
        }
    }
}
