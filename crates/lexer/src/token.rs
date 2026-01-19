
// might need to add lambdas, labels
#[derive(Default, Clone, Debug, PartialEq)]
pub enum Token {
    #[default]
    Illegal,

    Newline,
    Whitespace,

    Comment,

    StatefulReference,
    Plus,
    Minus,
    Multiply,
    Power,
    Divide,
    IntegerDivide,
    KeyValueMap,
    Assign,
    DotAssign,
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
    Pipe,
    Dot,
    Append,
    Comma,
    Reference,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LFlower,
    RFlower,
    Semicolon,
    Block,

    Import,
    Break,
    Continue,
    Return,
    If,
    Else,
    For,
    While,
    Operator,
    Let,
    Var,
    Mesh,
    State,
    Param,
    Anim,
    Play,
    Slide,
    Native,

    IntegerLiteral,
    DoubleLiteral,
    // this means it looks most like a string literal
    // however it may actually be ilformed
    // the parser / compiler must actually verify the form
    StringLiteral,
    CharLiteral,

    ArgumentLabel,
    Identifier,
}

impl Token {
    pub fn category(&self) -> TokenCategory {
        use Token::*;
        match self {
            Illegal => TokenCategory::Unknown,
            Newline | Whitespace => TokenCategory::Whitespace,
            StatefulReference | Plus | Minus | Multiply | Power | Divide | IntegerDivide | Assign
            | KeyValueMap | Eq | Ne | Lt | Le | Gt | Ge | And | Not | Or | In
            | Pipe | Comma | Dot | DotAssign | Append | Reference | Semicolon => TokenCategory::Operator,
            LParen | RParen | LBracket | RBracket | LFlower | RFlower => TokenCategory::Punctutation,
            Block | Operator | Anim | Play | Break | Continue | Return | If | Else | For | While => TokenCategory::ControlFlow,
            Import | Let | Var | Mesh | State | Param | Slide | Native => {
                TokenCategory::NonControlFlowKeyword
            }
            IntegerLiteral | DoubleLiteral => TokenCategory::NumericLiteral,
            StringLiteral | CharLiteral => TokenCategory::TextLiteral,
            Comment => TokenCategory::Comment,
            Identifier => TokenCategory::Identifier,
            ArgumentLabel => TokenCategory::ArgumentLabel,
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
    ArgumentLabel,
    Identifier,
    NumericLiteral,
    TextLiteral,
    Comment,
}
