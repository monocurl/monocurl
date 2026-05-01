use std::cell::Cell;

use smallvec::SmallVec;

use crate::value::{InstructionPointer, Value};

/// animation block captured at definition time.
/// `Value::AnimBlock` wraps this in `Rc`, so all copies share the same instance
/// (including the `already_played` flag). no inner Rc needed.
#[derive(Clone)]
pub struct AnimBlock {
    pub captures: SmallVec<[Value; 8]>,
    pub ip: InstructionPointer,
    /// interior-mutable flag: set to true when played; any further play is an error.
    /// shared via the outer `Rc<AnimBlock>` in `Value::AnimBlock`.
    pub already_played: Cell<bool>,
}

impl AnimBlock {
    pub fn new(captures: SmallVec<[Value; 8]>, ip: InstructionPointer) -> Self {
        Self {
            captures,
            ip,
            already_played: Cell::new(false),
        }
    }
}
