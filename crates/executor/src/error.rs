use std::fmt;

#[derive(Debug, Clone)]
pub enum ExecutorError {
    TypeError { expected: &'static str, got: &'static str },
    IndexOutOfBounds { index: usize, len: usize },
    DivisionByZero,
    CannotAssignTo(&'static str),
    CannotSubscript(&'static str),
    CannotAttribute(&'static str),
    AnimPlayedTwice,
    PlayInLabeledInvocation,
    NativeFuncError(String),
    MissingArgument(&'static str),
    UnsupportedBinaryOp { op: &'static str, lhs: &'static str, rhs: &'static str },
    UnsupportedNegate(&'static str),
    Other(String),
}

impl ExecutorError {
    pub fn type_error(expected: &'static str, got: &'static str) -> Self {
        Self::TypeError { expected, got }
    }
}

impl fmt::Display for ExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeError { expected, got } => {
                write!(f, "type error: expected {}, got {}", expected, got)
            }
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "index {} out of bounds (len {})", index, len)
            }
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::CannotAssignTo(ty) => write!(f, "cannot assign to {}", ty),
            Self::CannotSubscript(ty) => write!(f, "cannot subscript {}", ty),
            Self::CannotAttribute(ty) => write!(f, "attribute access on {}", ty),
            Self::AnimPlayedTwice => write!(f, "animation block was already played"),
            Self::PlayInLabeledInvocation => {
                write!(f, "play inside labeled invocation is not allowed")
            }
            Self::NativeFuncError(msg) => write!(f, "{}", msg),
            Self::MissingArgument(name) => write!(f, "{}: missing argument", name),
            Self::UnsupportedBinaryOp { op, lhs, rhs } => {
                write!(f, "unsupported binary op {} on {} and {}", op, lhs, rhs)
            }
            Self::UnsupportedNegate(ty) => write!(f, "cannot negate {}", ty),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ExecutorError {}
