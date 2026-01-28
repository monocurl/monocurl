use crate::value::{InstructionPointer, RcValue};


pub struct Lambda {
    instruction_pointer: InstructionPointer,
    captured_variables: Vec<RcValue>,
    required_arg_count: u8,
    reference_arg_prefix: u8,
    default_args: Vec<RcValue>,
}

pub struct Operator(pub Lambda);
