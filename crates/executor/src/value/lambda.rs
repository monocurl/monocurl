use std::rc::Rc;

use smallvec::SmallVec;

use crate::value::{InstructionPointer, Value};

#[derive(Clone)]
pub struct Lambda {
    pub ip: InstructionPointer,
    /// captured values from the enclosing scope
    pub captures: SmallVec<[Value; 4]>,
    pub required_args: u16,
    /// default values for trailing optional parameters
    pub defaults: SmallVec<[Value; 1]>,
}

#[derive(Clone)]
pub struct Operator(pub Rc<Lambda>);
