use std::fmt;

/// Error type for the structural / lerp layer of `rust_scene`.
///
/// Kept intentionally small (a message string) — this mirrors the executor's
/// `ExecutorError::invalid_interpolation` shape closely enough for messages to
/// read the same way, without dragging in the executor's error type itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    pub fn message(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}
