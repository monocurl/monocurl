use crate::token::Token;
use structs::{
    iterutil::KLookahead,
    text::{Count8, Span8},
};

// upholds the contract that '\n' is a universal separator
pub struct Lexer<I>
where
    I: Iterator<Item = char>,
{
    chars: KLookahead<I, 2>,
    byte_count: Count8,
    // in import lines, "escape" all keywords
    in_import_line: bool,
}

impl<I> Lexer<I>
where
    I: Iterator<Item = char>,
{
    pub fn token_stream(chars: I) -> Vec<(Token, Span8)> {
        let mut utf8 = 0;
        Lexer::new(chars)
            .map(|(token, count)| {
                let start = utf8;
                utf8 += count;
                (token, start..utf8)
            })
            .collect()
    }

    pub fn new(chars: I) -> Self {
        Self {
            chars: KLookahead::new(chars),
            byte_count: 0,
            in_import_line: false,
        }
    }

    fn peek(&mut self) -> Option<char> {
        self.chars.peek(0).cloned()
    }

    fn peek_next(&mut self) -> Option<char> {
        self.chars.peek(1).cloned()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.chars.next()?;
        self.byte_count += ch.len_utf8();
        Some(ch)
    }

    fn advance_if_not_nl(&mut self) -> Option<char> {
        if let Some(ch) = self.peek() {
            if ch != '\n' {
                return self.advance();
            }
        }
        None
    }

    fn advance_if(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn skip_while<F>(&mut self, mut predicate: F) -> Count8
    where
        F: FnMut(char) -> bool,
    {
        let start = self.byte_count;
        while let Some(ch) = self.peek() {
            if predicate(ch) {
                self.advance();
            } else {
                break;
            }
        }
        self.byte_count - start
    }

    fn collect_while<F>(&mut self, mut predicate: F) -> String
    where
        F: FnMut(char) -> bool,
    {
        let mut result = String::new();
        while let Some(ch) = self.peek() {
            if predicate(ch) {
                result.push(ch);
                self.advance();
            } else {
                break;
            }
        }
        result
    }

    fn lex_number(&mut self, first: char) -> (Token, Count8) {
        let start = self.byte_count - first.len_utf8();

        // integer part
        self.skip_while(|ch| ch.is_ascii_digit());

        // decimal point
        let is_float = match (self.peek(), self.peek_next()) {
            (Some('.'), Some(ch)) if ch.is_ascii_digit() => {
                self.advance();
                self.skip_while(|ch| ch.is_ascii_digit());
                true
            }
            _ => false,
        };

        // handle extensions
        match (self.peek(), self.peek_next()) {
            (Some('d'), Some('g')) => {
                self.advance();
                self.advance();
            }
            (
                Some('i') | Some('d') | Some('u') | Some('f') | Some('b') | Some('l') | Some('r'),
                _,
            ) => {
                self.advance();
            }
            _ => {}
        }

        let token = if is_float {
            Token::FloatLiteral
        } else {
            Token::IntegerLiteral
        };

        (token, self.byte_count - start)
    }

    fn lex_identifier_or_keyword(&mut self, first: char) -> (Token, Count8) {
        let start = self.byte_count - first.len_utf8();
        let mut ident = String::from(first);
        ident.push_str(&self.collect_while(|ch| ch.is_alphanumeric() || ch == '_'));
        if self.advance_if(':') {
            return (Token::ArgumentLabel, self.byte_count - start);
        }

        let token = match ident.as_str() {
            _ if self.in_import_line => Token::Identifier,
            "import" => Token::Import,
            "break" => Token::Break,
            "block" => Token::Block,
            "continue" => Token::Continue,
            "return" => Token::Return,
            "if" => Token::If,
            "else" => Token::Else,
            "for" => Token::For,
            "while" => Token::While,
            "operator" => Token::Operator,
            "let" => Token::Let,
            "var" => Token::Var,
            "mesh" => Token::Mesh,
            "param" => Token::Param,
            "anim" => Token::Anim,
            "play" => Token::Play,
            "slide" => Token::Slide,
            "__monocurl__native__" => Token::Native,
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            "in" => Token::In,
            _ => Token::Identifier,
        };

        if token == Token::Import {
            self.in_import_line = true;
        }

        (token, self.byte_count - start)
    }

    fn lex_string(&mut self) -> (Token, Count8) {
        let start = self.byte_count - 1; // -1 for the opening "
        loop {
            match self.peek() {
                Some('"') => {
                    self.advance(); // consume closing "
                    break;
                }
                Some('%') => {
                    self.advance();
                    // skip past escape sequence
                    self.advance_if_not_nl();
                }
                Some('\n') | None => {
                    // malformed string, will be caught later on in parse stage
                    break;
                }
                Some(_) => {
                    self.advance();
                }
            }
        }

        (Token::StringLiteral, self.byte_count - start)
    }
}

impl<I> Iterator for Lexer<I>
where
    I: Iterator<Item = char>,
{
    type Item = (Token, Count8);

    fn next(&mut self) -> Option<Self::Item> {
        let ch = self.advance()?;
        let start = self.byte_count - ch.len_utf8();

        let token = match ch {
            '\n' => {
                self.in_import_line = false;
                Token::Newline
            }
            ' ' | '\t' | '\r' => {
                self.skip_while(|c| matches!(c, ' ' | '\t' | '\r'));
                Token::Whitespace
            }
            '#' => {
                self.skip_while(|c| c != '\n');
                Token::Comment
            }
            '+' => Token::Plus,
            '-' => {
                if self.advance_if('>') {
                    Token::KeyValueMap
                } else {
                    Token::Minus
                }
            }
            '^' => Token::Power,
            '$' => Token::StatefulReference,
            '*' => Token::Multiply,
            '/' => {
                if self.advance_if('/') {
                    Token::IntegerDivide
                } else {
                    Token::Divide
                }
            }
            '=' => {
                if self.advance_if('=') {
                    Token::Eq
                } else {
                    Token::Assign
                }
            }
            '!' => {
                if self.advance_if('=') {
                    Token::Ne
                } else {
                    Token::Illegal
                }
            }
            '<' => {
                if self.advance_if('=') {
                    Token::Le
                } else {
                    Token::Lt
                }
            }
            '>' => {
                if self.advance_if('=') {
                    Token::Ge
                } else {
                    Token::Gt
                }
            }
            '.' => {
                if self.advance_if('=') {
                    Token::DotAssign
                } else if self.advance_if('.') {
                    Token::Append
                } else {
                    Token::Dot
                }
            }
            '|' => Token::Pipe,
            ',' => Token::Comma,
            '&' => Token::Reference,
            '(' => Token::LParen,
            ')' => Token::RParen,
            '[' => Token::LBracket,
            ']' => Token::RBracket,
            '{' => Token::LFlower,
            '}' => Token::RFlower,
            ';' => {
                self.in_import_line = false;
                Token::Semicolon
            }
            '"' => return Some(self.lex_string()),
            _ if ch.is_ascii_digit() => return Some(self.lex_number(ch)),
            _ if ch.is_alphabetic() || ch == '_' => {
                return Some(self.lex_identifier_or_keyword(ch));
            }
            _ => Token::Illegal,
        };

        Some((token, self.byte_count - start))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_counts() {
        let input = "foo bar";
        let tokens: Vec<(Token, Count8)> = Lexer::new(input.chars()).collect();

        assert_eq!(tokens[0], (Token::Identifier, 3)); // "foo"
        assert_eq!(tokens[1], (Token::Whitespace, 1)); // " "
        assert_eq!(tokens[2], (Token::Identifier, 3)); // "bar"
    }

    #[test]
    fn test_utf8_counting() {
        let input = "café"; // é is 2 bytes in UTF-8
        let tokens: Vec<(Token, Count8)> = Lexer::new(input.chars()).collect();

        assert_eq!(tokens[0].1, 5); // c(1) + a(1) + f(1) + é(2) = 5 bytes
    }

    #[test]
    fn test_string_literal() {
        let input = r#""hello%n%"world""#;
        let tokens: Vec<(Token, Count8)> = Lexer::new(input.chars()).collect();

        assert_eq!(tokens[0].0, Token::StringLiteral);
        assert_eq!(tokens[0].1, input.len());
    }
}
