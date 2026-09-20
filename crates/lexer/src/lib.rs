pub mod lexer;
pub mod token;

use structs::rope::{Attribute, RLEData, Rope};

use crate::{lexer::Lexer, token::Token};

/// lex `source` into the run-length encoded token rope the parser consumes
pub fn lex_rope_from_str(source: &str) -> Rope<Attribute<Token>> {
    Rope::default().replace_range(
        0..0,
        Lexer::new(source.chars()).map(|(attribute, codeunits)| RLEData {
            codeunits,
            attribute,
        }),
    )
}
