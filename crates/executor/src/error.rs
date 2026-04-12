use std::fmt;

#[derive(Debug, Clone)]
pub enum ExecutorError {
    TypeError { expected: &'static str, got: &'static str },
    IndexOutOfBounds { index: usize, len: usize },
    DivisionByZero,
    DestructuringError { lhs_size: usize, rhs_size: Option<usize>, rhs_type: &'static str },
    CannotAssignTo(&'static str),
    CannotSubscript(&'static str),
    CannotAttribute(&'static str),
    TooFewArguments { minimum: usize, got: usize },
    TooManyArguments { maximum: usize, got: usize },
    AnimPlayedTwice,
    PlayInLabeledInvocation,
    StackOverflow,
    TooManyActiveAnimations,
    NativeFuncError(String),
    MissingArgument(&'static str),
    UnsupportedBinaryOp { op: &'static str, lhs: &'static str, rhs: &'static str },
    UnsupportedNegate(&'static str),
    UnhashableKey(&'static str),
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
            Self::CannotAssignTo(ty) => write!(f, "cannot assign to {}. You might have forgotten to pass a variable by reference (e.g. &var)", ty),
            Self::CannotSubscript(ty) => write!(f, "cannot subscript {}", ty),
            Self::CannotAttribute(ty) => write!(f, "attribute access on {}", ty),
            Self::TooFewArguments { minimum, got } => {
                write!(f, "too few arguments: expected at least {}, got {}", minimum, got)
            }
            Self::TooManyArguments { maximum, got } => {
                write!(f, "too many arguments: expected at most {}, got {}", maximum, got)
            }
            Self::AnimPlayedTwice => write!(f, "animation block was already played"),
            Self::PlayInLabeledInvocation => {
                write!(f, "play inside labeled invocation is not allowed")
            }
            Self::StackOverflow => write!(f, "stack overflow: call depth limit exceeded"),
            Self::NativeFuncError(msg) => write!(f, "{}", msg),
            Self::MissingArgument(name) => write!(f, "{}: missing argument", name),
            Self::UnsupportedBinaryOp { op, lhs, rhs } => {
                write!(f, "unsupported binary op {} on {} and {}", op, lhs, rhs)
            }
            Self::UnsupportedNegate(ty) => write!(f, "cannot negate {}", ty),
            Self::UnhashableKey(ty) => write!(f, "cannot use {} as a map key", ty),
            Self::DestructuringError { lhs_size, rhs_size, rhs_type } => {
                if let Some(rhs_size) = rhs_size {
                    write!(f, "list destructuring assignment error: cannot assign {} values into a list of length {}", rhs_size, lhs_size)
                }
                else {
                    write!(f, "destructuring assignment error: rhs is not a list, but of type {}", rhs_type)
                }
            },
            Self::TooManyActiveAnimations => write!(f, "too many active animations: limit exceeded"),
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ExecutorError {}
