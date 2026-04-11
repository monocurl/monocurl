use crate::value::{InstructionPointer, Value};

#[derive(Clone)]
pub struct Lambda {
    pub ip: InstructionPointer,
    /// captured values from the enclosing scope (typically Lvalue ptrs for let vars)
    pub captures: Vec<Value>,
    pub required_args: u16,
    /// default values for trailing optional parameters
    pub defaults: Vec<Value>,
}

#[derive(Clone)]
pub struct Operator(pub Lambda);
