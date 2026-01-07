use futures::{StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use structs::rope::{RLEAggregate, Rope, TextAggregate};

use crate::{services::{ServiceManagerMessage, execution::ExecutionMessage}, state::{textual_state::{LexData}}};

pub enum CompilationMessage {
    UpdateLexRope {
        lex_rope: Rope<RLEAggregate<LexData>>,
        for_text_rope: Rope<TextAggregate>
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
        log::info!("Starting Compilation Service");
        while let Some(message) = self.rx.next().await {
            match message {
                CompilationMessage::UpdateLexRope { .. } => {
                    // reparse + recompile
                    // let _ = self.execution_tx.send(ExecutionMessage::UpdateBytecode).await;
                },
                CompilationMessage::RecheckDependencies => {
                    // recompile if any new dependencies
                }
            }
        }
        log::info!("Exiting Compilation Service");
    }
}
