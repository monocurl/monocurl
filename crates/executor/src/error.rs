use std::fmt;

#[derive(Debug, Clone)]
pub enum ExecutorError {
    TypeError { expected: &'static str, got: &'static str, target: Option<&'static str> },
    IndexOutOfBounds { index: usize, len: usize },
    DivisionByZero,
    DestructuringError { lhs_size: usize, rhs_size: Option<usize>, rhs_type: &'static str },
    CannotAssignTo(&'static str),
    CannotSubscript(&'static str),
    CannotAttribute(&'static str),
    TooFewArguments { minimum: usize, got: usize, operator: bool },
    TooManyArguments { maximum: usize, got: usize, operator: bool },
    AnimPlayedTwice,
    PlayInLabeledInvocation,
    StackOverflow,
    TooManyActiveAnimations,
    MemoryLimitExceeded { used: u64, limit: u64 },
    NativeFuncError(String),
    MissingArgument(&'static str),
    UnsupportedBinaryOp { op: &'static str, lhs: &'static str, rhs: &'static str },
    UnsupportedNegate(&'static str),
    UnhashableKey(&'static str),
    InvalidCondition(&'static str),
    InvalidArgument { arg: &'static str, message: &'static str },
    Other(String),
}

impl ExecutorError {
    pub fn type_error(expected: &'static str, got: &'static str) -> Self {
        Self::TypeError { expected, got, target: None }
    }

    pub fn type_error_for(expected: &'static str, got: &'static str, target: &'static str) -> Self {
        Self::TypeError { expected, got, target: Some(target) }
    }
}

impl fmt::Display for ExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeError { expected, got, target } => {
                if let Some(target) = target {
                    write!(f, "type error: expected {}, got {} for {}", expected, got, target)
                }
                else {
                    write!(f, "type error: expected {}, got {}", expected, got)
                }
            }
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "index {} out of bounds (len {})", index, len)
            }
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::CannotAssignTo(ty) => write!(f, "cannot assign to {}. You might have forgotten to pass a variable by reference (e.g. &var)", ty),
            Self::CannotSubscript(ty) => write!(f, "cannot subscript {}", ty),
            Self::CannotAttribute(ty) => write!(f, "attribute access on {}", ty),
            Self::TooFewArguments { minimum, got, operator } => {
                if *operator {
                    write!(f, "too few arguments for operator: expected at least {}, got {}", minimum.saturating_sub(1), got.saturating_sub(1))
                }
                else {
                    write!(f, "too few positional arguments: expected at least {}, got {}", minimum, got)
                }
            }
            Self::TooManyArguments { maximum, got, operator } => {
                if *operator {
                    write!(f, "too many arguments for operator: expected at most {}, got {}", maximum.saturating_sub(1), got.saturating_sub(1))
                }
                else {
                    write!(f, "too many positional arguments: expected at most {}, got {}", maximum, got)
                }
            }
            Self::AnimPlayedTwice => write!(f, "animation block was already played"),
            Self::PlayInLabeledInvocation => {
                write!(f, "play inside labeled invocation is not allowed")
            }
            Self::StackOverflow => write!(f, "stack overflow: call depth limit exceeded"),
            Self::MemoryLimitExceeded { used, limit } => {
                write!(
                    f,
                    "memory limit exceeded: process is using {} bytes, limit is {} bytes",
                    used,
                    limit
                )
            }
            Self::NativeFuncError(msg) => write!(f, "{}", msg),
            Self::MissingArgument(name) => write!(f, "{}: missing argument", name),
            Self::UnsupportedBinaryOp { op, lhs, rhs } => {
                write!(f, "unsupported binary op {} on {} and {}", op, lhs, rhs)
            }
            Self::UnsupportedNegate(ty) => write!(f, "cannot negate {}", ty),
            Self::UnhashableKey(ty) => write!(f, "cannot use {} as a map key", ty),
            Self::InvalidCondition(ty) => write!(f, "{} has no truthiness — use a numeric or boolean expression instead", ty),
            Self::DestructuringError { lhs_size, rhs_size, rhs_type } => {
                if let Some(rhs_size) = rhs_size {
                    write!(f, "list destructuring assignment error: cannot assign {} values into a list of length {}", rhs_size, lhs_size)
                }
                else {
                    write!(f, "destructuring assignment error: rhs is not a list, but of type {}", rhs_type)
                }
            },
            Self::TooManyActiveAnimations => write!(f, "too many active animations: limit exceeded"),
            Self::InvalidArgument { arg, message } => {
                write!(f, "invalid argument '{}': {}", arg, message)
            }
            Self::Other(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ExecutorError {}
