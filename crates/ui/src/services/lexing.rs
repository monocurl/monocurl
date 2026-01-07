
use futures::{SinkExt, StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use lexer::lexer::Lexer;
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
    rx: UnboundedReceiver<LexingMessage>,
    compilation_tx: UnboundedSender<CompilationMessage>,

    sm_tx: UnboundedSender<ServiceManagerMessage>,
}

impl LexingService {
    pub fn new(rx: UnboundedReceiver<LexingMessage>, compilation_tx: UnboundedSender<CompilationMessage>, sm_tx: UnboundedSender<ServiceManagerMessage>) -> Self {
        Self {
            rx,
            compilation_tx,

            sm_tx,
        }
    }

    fn process_message(&mut self, lex: Rope<RLEAggregate<LexData>>, old: Span8, new: usize, old_rope: Rope<TextAggregate>, new_rope: Rope<TextAggregate>) -> Rope<RLEAggregate<LexData>> {
        // since newline is a universal separator, we only need to relex the associated lines
        let line_start = new_rope.utf8_prefix_summary(old.start).newlines;
        let old_line_end = old_rope.utf8_prefix_summary(old.end).newlines + 1;
        let new_line_end = new_rope.utf8_prefix_summary(old.start + new).newlines + 1;
        let utf8_start = new_rope.utf8_line_pos_prefix(line_start, 0).bytes_utf8;
        let old_utf8_end = old_rope.utf8_line_pos_prefix(old_line_end, 0).bytes_utf8;
        let new_utf8_end = new_rope.utf8_line_pos_prefix(new_line_end, 0).bytes_utf8;

        let relex = Lexer::new(new_rope.iterator_range(utf8_start..new_utf8_end))
            .map(|(attribute, bytes_utf8)| RLEData { bytes_utf8, attribute });

        lex.replace_range(
            utf8_start..old_utf8_end,
            relex
        )
    }

    pub async fn run(mut self) {
        log::info!("Starting Lexing Service");

        let mut lex_rope = Rope::default();
        let mut text_rope = Rope::default();
        while let Some(message) = self.rx.next().await {
            let mut current_version;
            match message {
                LexingMessage::UpdateRope { old, new, new_rope, version } => {
                    lex_rope = self.process_message(lex_rope, old, new, text_rope, new_rope.clone());
                    text_rope = new_rope;
                    current_version = version;
                }
            }

            while let Ok(Some(message)) = self.rx.try_next() {
                match message {
                    LexingMessage::UpdateRope { old, new, new_rope, version } => {
                        lex_rope = self.process_message(lex_rope, old, new, text_rope, new_rope.clone());
                        text_rope = new_rope;
                        current_version = version;
                    }
                }
            }

            // dispatch updates
            self.compilation_tx.send(CompilationMessage::UpdateLexRope {
                lex_rope: lex_rope.clone(),
                for_text_rope: text_rope.clone()

            }).await.unwrap();

            self.sm_tx.send(ServiceManagerMessage::UpdateLexRope {
                lex_rope: lex_rope.clone(),
                version: current_version,
            }).await.unwrap();
        }

        log::info!("Exiting Lexing Service");
    }
}
