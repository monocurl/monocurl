use std::path::PathBuf;

use lexer::token::Token;
use structs::text::Span8;

// context mainly related about finding additional
struct ParserContext {
    working_directory: PathBuf,
}

struct Precomputation {
    slide_boundaries: Vec<Span8>,
    bracket_advance: Vec<i32>
}

pub struct Parser<'a> {
    context: &'a ParserContext,
    tokens: Vec<Token>,
    token_index: usize,
}

impl<'a> Parser<'a> {
    pub fn parse_section() {

    }

    pub fn parse_statement() {

    }

    pub fn parse_statement_list() {

    }

    pub fn parse_while() {

    }

    pub fn parse_for() {

    }

    pub fn parse_if() {

    }

    pub fn parse_expr() {

    }
}

impl Parser<'static> {
    pub fn parse(_tokens: i32) -> i32 {
        // 1. find slide delimetters
        // 2. within each slide find bracket matchings
        0
    }
}
