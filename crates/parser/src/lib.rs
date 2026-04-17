use std::ops::Range;

use lexer::token::Token;
use structs::{
    rope::{Attribute, Rope},
    text::Count8,
};

pub mod ast;
pub mod import_context;
pub mod parser;

fn flatten_lex_stream(
    i: impl Iterator<Item = (Token, Count8)>,
) -> impl Iterator<Item = (Token, Range<usize>)> {
    let mut utf8 = 0;
    i.map(move |(token, count)| {
        let start = utf8;
        utf8 += count;
        (token, start..utf8)
    })
    .filter(|(tok, _)| tok != &Token::Whitespace && tok != &Token::Comment)
}

fn flatten_rope(r: &Rope<Attribute<Token>>) -> Vec<(Token, Range<usize>)> {
    flatten_lex_stream(r.iterator(0).map(|(x, y)| (y, x))).collect()
}
