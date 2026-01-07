
use futures::{SinkExt, StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use lexer::{stateful_lexer::StatefulLexer};
use lexer::token::Token;
use structs::{rope::{RLEAggregate, Rope, TextAggregate}, text::{Count8, Span8}};
use structs::rope::RLEData;
use crate::{services::{ServiceManagerMessage, compilation::CompilationMessage}, state::textual_state::{LexData}};

pub enum LexingMessage {
   UpdateRope {
       old: Span8,
       new: Count8,
       new_rope: Rope<TextAggregate>,
       version: usize,
   },
}

// It may be assumed that the initial text rope is empty
pub struct LexingService {
    lexer: StatefulLexer,

    rx: UnboundedReceiver<LexingMessage>,
    compilation_tx: UnboundedSender<CompilationMessage>,

    sm_tx: UnboundedSender<ServiceManagerMessage>,
}

impl LexingService {
    pub fn new(rx: UnboundedReceiver<LexingMessage>, compilation_tx: UnboundedSender<CompilationMessage>, sm_tx: UnboundedSender<ServiceManagerMessage>) -> Self {
        Self {
            lexer: StatefulLexer::new(),

            rx,
            compilation_tx,

            sm_tx,
        }
    }

    fn process_message(&mut self, lex: Rope<RLEAggregate<LexData>>, old: Span8, new: usize, new_rope: Rope<TextAggregate>, version: usize) -> Rope<RLEAggregate<LexData>> {
        // perform lexing on the new rope
        // let tokens = self.lexer.relex_region(&new_rope, old.start, new);
        // lex.update(old, new, new_rope);
        let ret = lex.replace_range(
            old,
            vec![RLEData { bytes_utf8: new, attribute: Token::Break }]
        );

        ret
        // todo!()
    }

    pub async fn run(mut self) {
        log::info!("Starting Lexing Service");

        let mut lex_rope = Rope::default();
        let mut current_version;
        while let Some(message) = self.rx.next().await {
            match message {
                LexingMessage::UpdateRope { old, new, new_rope, version } => {
                    lex_rope = self.process_message(lex_rope, old, new, new_rope, version);
                    current_version = version;
                }
            }

            while let Ok(Some(message)) = self.rx.try_next() {
                match message {
                    LexingMessage::UpdateRope { old, new, new_rope, version } => {
                        lex_rope = self.process_message(lex_rope, old, new, new_rope, version);
                        current_version = version;
                    }
                }
            }

            // dispatch updates
            self.compilation_tx.send(CompilationMessage::UpdateLexRope {
                lex_rope: lex_rope.clone(),
            }).await.unwrap();

            self.sm_tx.send(ServiceManagerMessage::UpdateLexRope {
                lex_rope: lex_rope.clone(),
                version: current_version,
            }).await.unwrap();
        }

        log::info!("Exiting Lexing Service");
    }
}
