
// might need to add lambdas, labels
#[derive(Default, Clone, Debug, PartialEq)]
pub enum Token {
    #[default]
    Illegal,

    Newline,
    Whitespace,

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
    // this means it looks most like a string literal
    // however it may actually be ilformed
    // the parser / compiler must actually verify the form
    StringLiteral,
    CharLiteral,

    Identifier,
}

impl Token {
    pub fn category(&self) -> TokenCategory {
        use Token::*;
        match self {
            Illegal => TokenCategory::Unknown,
            Newline | Whitespace => TokenCategory::Whitespace,
            Plus | Minus | Multiply | Power | Divide | IntegerDivide | Assign
            | Eq | Ne | Lt | Le | Gt | Ge | And | Not | Or | In | Range
            | Pipe | Comma | Reference | Colon => TokenCategory::Operator,
            LParen | RParen | LBracket | RBracket | LFlower | RFlower => TokenCategory::Punctutation,
            Import | Break | Continue | Return | If | Else | For | While => TokenCategory::ControlFlow,
            Func | Struct | Conj | Let | Var | Mesh | State | Param | Slide | Native => {
                TokenCategory::NonControlFlowKeyword
            }
            IntegerLiteral(_) | DoubleLiteral(_) => TokenCategory::NumericLiteral,
            StringLiteral | CharLiteral => TokenCategory::TextLiteral,
            Comment => TokenCategory::Comment,
            Identifier => TokenCategory::Identifier
        }
    }
}

#[derive(Default, Copy, Clone, Debug, PartialEq)]
pub enum TokenCategory {
    #[default]
    Unknown,
    Whitespace,
    Operator,
    // as in parentheses, brackets, ::, etc
    Punctutation,
    ControlFlow,
    NonControlFlowKeyword,
    Identifier,
    NumericLiteral,
    TextLiteral,
    Comment,
}
