use crate::value::{InstructionPointer, RcValue};

pub struct AnimBlock {
    pub captured_stack: Vec<RcValue>,
    pub instruction_pointer: InstructionPointer,
}
