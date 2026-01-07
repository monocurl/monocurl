use crate::token::Token;


pub struct Lexer<I> {
    chars: I,
}

impl<I> Lexer<I> where I: Iterator<Item = char> {
    pub fn new(chars: I) -> Self {
        Self { chars }
    }
}

impl<I> Iterator for Lexer<I> where I: Iterator<Item = char> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        todo!()
    }
}
