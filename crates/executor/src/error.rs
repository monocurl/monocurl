use std::fmt;

use structs::text::Span8;

#[derive(Debug, Clone)]
pub enum ExecutorError {
    TypeError {
        expected: &'static str,
        got: &'static str,
        target: Option<&'static str>,
    },
    IndexOutOfBounds {
        index: usize,
        len: usize,
    },
    DivisionByZero,
    DestructuringError {
        lhs_size: usize,
        rhs_size: Option<usize>,
        rhs_type: &'static str,
    },
    CannotAssignTo(&'static str),
    CannotSubscript(&'static str),
    CannotAttribute(&'static str),
    TooFewArguments {
        minimum: usize,
        got: usize,
        operator: bool,
    },
    TooManyArguments {
        maximum: usize,
        got: usize,
        operator: bool,
    },
    AnimPlayedTwice,
    ConcurrentAnimation,
    PlayInLabeledInvocation,
    StackOverflow,
    TooManyActiveAnimations,
    VirtualHeapLimitExceeded {
        used: usize,
        limit: usize,
    },
    StatefulValueError(String),
    InvalidAccess(String),
    InvalidOperation(String),
    InvalidInvocation(String),
    InvalidInterpolation(String),
    InvalidScene(String),
    MissingField {
        target: String,
        field: String,
    },
    UnknownParameter(String),
    Internal(String),
    NativeFuncError(String),
    MissingArgument(&'static str),
    UnsupportedBinaryOp {
        op: &'static str,
        lhs: &'static str,
        rhs: &'static str,
    },
    ListLengthMismatch {
        op: &'static str,
        lhs_len: usize,
        rhs_len: usize,
    },
    UnsupportedNegate(&'static str),
    UnhashableKey(&'static str),
    InvalidCondition(&'static str),
    InvalidArgument {
        arg: &'static str,
        message: &'static str,
    },
}

#[derive(Debug, Clone)]
pub struct RuntimeCallFrame {
    pub section: u16,
    pub span: Span8,
}

#[derive(Debug, Clone)]
pub struct RuntimeError {
    pub error: ExecutorError,
    pub span: Span8,
    pub callstack: Vec<RuntimeCallFrame>,
}

impl ExecutorError {
    pub fn type_error(expected: &'static str, got: &'static str) -> Self {
        Self::TypeError {
            expected,
            got,
            target: None,
        }
    }

    pub fn type_error_for(expected: &'static str, got: &'static str, target: &'static str) -> Self {
        Self::TypeError {
            expected,
            got,
            target: Some(target),
        }
    }

    pub fn stateful_value(message: impl Into<String>) -> Self {
        Self::StatefulValueError(message.into())
    }

    pub fn invalid_access(message: impl Into<String>) -> Self {
        Self::InvalidAccess(message.into())
    }

    pub fn invalid_operation(message: impl Into<String>) -> Self {
        Self::InvalidOperation(message.into())
    }

    pub fn invalid_invocation(message: impl Into<String>) -> Self {
        Self::InvalidInvocation(message.into())
    }

    pub fn invalid_interpolation(message: impl Into<String>) -> Self {
        Self::InvalidInterpolation(message.into())
    }

    pub fn invalid_scene(message: impl Into<String>) -> Self {
        Self::InvalidScene(message.into())
    }

    pub fn missing_field(target: impl Into<String>, field: impl Into<String>) -> Self {
        Self::MissingField {
            target: target.into(),
            field: field.into(),
        }
    }

    pub fn unknown_parameter(name: impl Into<String>) -> Self {
        Self::UnknownParameter(name.into())
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal(message.into())
    }

    pub fn stateful_illegal_assignment() -> Self {
        Self::stateful_value(
            "illegal assignment of stateful value. Stateful values must only be assigned to meshes",
        )
    }

    pub fn stateful_requires_mesh_assignment() -> Self {
        Self::stateful_value("stateful values can only be assigned to mesh variables")
    }

    pub fn stateful_cannot_append() -> Self {
        Self::stateful_value("stateful values cannot be appended to lists")
    }

    pub fn stateful_binary_op() -> Self {
        Self::stateful_value("binary operators cannot be applied to stateful values")
    }

    pub fn stateful_unary_op() -> Self {
        Self::stateful_value("unary operators cannot be applied to stateful values")
    }

    pub fn stateful_operator() -> Self {
        Self::stateful_value("operators cannot be applied to stateful values")
    }

    pub fn stateful_subscript() -> Self {
        Self::stateful_value("subscript cannot be applied to stateful values")
    }

    pub fn direct_stateful_copy() -> Self {
        Self::stateful_value(
            "attempt to copy a stateful value directly. Use $<ident> only where a reactive expression is intended",
        )
    }

    pub fn missing_labeled_argument(name: impl Into<String>) -> Self {
        Self::invalid_access(format!("no labeled argument '{}'", name.into()))
    }

    pub fn invalid_lvalue(context: &'static str) -> Self {
        Self::invalid_access(format!("{context}: lhs is not an lvalue"))
    }
}

impl fmt::Display for ExecutorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeError {
                expected,
                got,
                target,
            } => {
                if let Some(target) = target {
                    write!(
                        f,
                        "type error: expected {}, got {} for {}",
                        expected, got, target
                    )
                } else {
                    write!(f, "type error: expected {}, got {}", expected, got)
                }
            }
            Self::IndexOutOfBounds { index, len } => {
                write!(f, "index {} out of bounds (len {})", index, len)
            }
            Self::DivisionByZero => write!(f, "division by zero"),
            Self::CannotAssignTo(ty) => write!(
                f,
                "cannot assign to {}. You might have forgotten to pass a variable by reference (e.g. &var)",
                ty
            ),
            Self::CannotSubscript(ty) => write!(f, "cannot subscript {}", ty),
            Self::CannotAttribute(ty) => write!(f, "attribute access on {}", ty),
            Self::TooFewArguments {
                minimum,
                got,
                operator,
            } => {
                if *operator {
                    write!(
                        f,
                        "too few arguments for operator: expected at least {}, got {}",
                        minimum.saturating_sub(1),
                        got.saturating_sub(1)
                    )
                } else {
                    write!(
                        f,
                        "too few positional arguments: expected at least {}, got {}",
                        minimum, got
                    )
                }
            }
            Self::TooManyArguments {
                maximum,
                got,
                operator,
            } => {
                if *operator {
                    write!(
                        f,
                        "too many arguments for operator: expected at most {}, got {}",
                        maximum.saturating_sub(1),
                        got.saturating_sub(1)
                    )
                } else {
                    write!(
                        f,
                        "too many positional arguments: expected at most {}, got {}",
                        maximum, got
                    )
                }
            }
            Self::AnimPlayedTwice => write!(f, "animation block was already played"),
            Self::ConcurrentAnimation => write!(
                f,
                "concurrent animation error: variable is already being animated"
            ),
            Self::PlayInLabeledInvocation => {
                write!(f, "play inside labeled invocation is not allowed")
            }
            Self::StackOverflow => write!(f, "stack overflow: call depth limit exceeded"),
            Self::VirtualHeapLimitExceeded { used, limit } => {
                write!(
                    f,
                    "virtual heap limit exceeded: heap is using {} slots, limit is {} slots",
                    used, limit
                )
            }
            Self::StatefulValueError(msg)
            | Self::InvalidAccess(msg)
            | Self::InvalidOperation(msg)
            | Self::InvalidInvocation(msg)
            | Self::InvalidInterpolation(msg)
            | Self::InvalidScene(msg)
            | Self::Internal(msg) => write!(f, "{}", msg),
            Self::MissingField { target, field } => {
                write!(f, "{}: missing '{}' field", target, field)
            }
            Self::UnknownParameter(name) => write!(f, "unknown parameter '{}'", name),
            Self::NativeFuncError(msg) => write!(f, "{}", msg),
            Self::MissingArgument(name) => write!(f, "{}: missing argument", name),
            Self::UnsupportedBinaryOp { op, lhs, rhs } => {
                write!(f, "unsupported binary op {} on {} and {}", op, lhs, rhs)
            }
            Self::ListLengthMismatch {
                op,
                lhs_len,
                rhs_len,
            } => {
                write!(
                    f,
                    "cannot apply {} to lists of different lengths: lhs has length {}, rhs has length {}",
                    op, lhs_len, rhs_len
                )
            }
            Self::UnsupportedNegate(ty) => write!(f, "cannot negate {}", ty),
            Self::UnhashableKey(ty) => write!(f, "cannot use {} as a map key", ty),
            Self::InvalidCondition(ty) => write!(
                f,
                "{} has no truthiness — use a numeric or boolean expression instead",
                ty
            ),
            Self::DestructuringError {
                lhs_size,
                rhs_size,
                rhs_type,
            } => {
                if let Some(rhs_size) = rhs_size {
                    write!(
                        f,
                        "list destructuring assignment error: cannot assign {} values into a list of length {}",
                        rhs_size, lhs_size
                    )
                } else {
                    write!(
                        f,
                        "destructuring assignment error: rhs is not a list, but of type {}",
                        rhs_type
                    )
                }
            }
            Self::TooManyActiveAnimations => {
                write!(f, "too many active animations: limit exceeded")
            }
            Self::InvalidArgument { arg, message } => {
                write!(f, "invalid argument '{}': {}", arg, message)
            }
        }
    }
}

impl std::error::Error for ExecutorError {}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

impl std::error::Error for RuntimeError {}
