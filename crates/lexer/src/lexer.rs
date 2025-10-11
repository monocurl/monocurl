pub enum Token {
    Newline,
    Whitespace(String),

    Comment,

    Plus,
    Minus,
    Multiply,
    Power,
    Divide,
    IntegerDivide,
    Assign,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Not,
    Or,
    In,
    Range,
    Pipe,
    Comma,
    Reference,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LFlower,
    RFlower,
    Colon,

    Import,
    Break,
    Continue,
    Return,
    If,
    Else,
    For,
    While,
    Func,
    Struct,
    Monad,
    Let,
    Var,
    Mesh,
    State,
    Param,
    Slide,
    Native,

    IntegerLiteral(i64),
    DoubleLiteral(f64),
    // TODO should decide the best way to do this i guess?
    StringLiteralDelimetter,
    StringLiteralChunk(String),
    CharLiteralDelimetter,
    CharLiteral(char),

    Identifier(String),
}

pub struct Lexer {}

impl Lexer {
    // pub fn token_stream(&self) -> &Vec<Lexer> {}

    pub fn clear(&self) {}

    pub fn update() {}

    pub fn update_suffix() {}
}
