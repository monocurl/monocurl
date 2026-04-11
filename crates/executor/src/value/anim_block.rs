use std::cell::Cell;
use std::rc::Rc;

use crate::value::{InstructionPointer, Value};

#[derive(Clone)]
pub struct AnimBlock {
    pub captures: Vec<Value>,
    pub ip: InstructionPointer,
    /// shared flag — all clones of the same anim block share this.
    /// set to true when played; any further play attempt is an error.
    pub already_played: Rc<Cell<bool>>,
}

impl AnimBlock {
    pub fn new(captures: Vec<Value>, ip: InstructionPointer) -> Self {
        Self {
            captures,
            ip,
            already_played: Rc::new(Cell::new(false)),
        }
    }
}
