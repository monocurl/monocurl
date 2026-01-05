
#[derive(Clone, Debug, PartialEq)]
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
    Conj,
    Let,
    Var,
    Mesh,
    State,
    Param,
    Slide,
    Native,

    IntegerLiteral(i64),
    DoubleLiteral(f64),
    StringLiteralDelimetter,
    StringLiteralChunk(String),
    CharLiteralDelimetter,
    CharLiteral(char),

    Identifier(String),
}

impl Token {
    pub fn category(&self) -> TokenCategory {
        use Token::*;
        match self {
            Newline | Whitespace(_) => TokenCategory::Whitespace,
            Plus | Minus | Multiply | Power | Divide | IntegerDivide | Assign
            | Eq | Ne | Lt | Le | Gt | Ge | And | Not | Or | In | Range
            | Pipe | Comma | Reference | Colon => TokenCategory::Operator,
            LParen | RParen | LBracket | RBracket | LFlower | RFlower => TokenCategory::Parenthesis,
            Import | Break | Continue | Return | If | Else | For | While => TokenCategory::ControlFlow,
            Func | Struct | Conj | Let | Var | Mesh | State | Param | Slide | Native => {
                TokenCategory::NonControlFlowKeyword
            }
            IntegerLiteral(_) | DoubleLiteral(_) => TokenCategory::NumericLiteral,
            StringLiteralDelimetter | StringLiteralChunk(_) |
            CharLiteralDelimetter | CharLiteral(_) => TokenCategory::TextLiteral,
            Comment => TokenCategory::Comment,
            Identifier(_) => TokenCategory::Identifier,
        }
    }
}

#[derive(Default, Clone, Debug, PartialEq)]
pub enum TokenCategory {
    #[default]
    Unknown,
    Whitespace,
    Operator,
    Parenthesis,
    ControlFlow,
    NonControlFlowKeyword,
    Identifier,
    NumericLiteral,
    TextLiteral,
    Comment,
}
