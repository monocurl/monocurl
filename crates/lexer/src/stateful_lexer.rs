use structs::{rope::{RLEAggregate, Rope, TextAggregate}, text::{Count8, Span8}};

use crate::token::Token;

pub struct StatefulLexer {
    lex_rope: Rope<RLEAggregate<Token>>,

}

impl StatefulLexer {
    pub fn new() -> Self {
        Self {
            lex_rope: Rope::default(),
        }
    }

    pub fn update(&mut self, old: Span8, new: Count8, new_text_rope: Rope<TextAggregate>) {
        // For now, we will just clear the lex rope
        // by lexing rules, only must start updating with the token before
    }

    pub fn lex_rope(&self) -> &Rope<RLEAggregate<Token>> {
        &self.lex_rope
    }
}
