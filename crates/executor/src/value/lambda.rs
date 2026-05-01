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
    pub reference_args: Vec<bool>,
    pub arg_names: Vec<String>,
}

#[derive(Clone)]
pub struct Operator(pub Rc<Lambda>);

impl Lambda {
    #[inline(always)]
    pub fn arg_is_reference(&self, arg_idx: usize) -> bool {
        self.reference_args.get(arg_idx).copied().unwrap_or(false)
    }

    #[inline(always)]
    pub fn total_args(&self) -> usize {
        self.required_args as usize + self.defaults.len()
    }

    #[inline(always)]
    pub fn default_arg_start(&self) -> usize {
        self.required_args as usize
    }

    pub fn default_arg_index(&self, name: &str) -> Option<usize> {
        self.arg_names
            .iter()
            .enumerate()
            .skip(self.default_arg_start())
            .find_map(|(idx, arg_name)| (arg_name == name).then_some(idx))
    }

    pub fn default_arg_names(&self) -> impl Iterator<Item = &str> {
        self.arg_names
            .iter()
            .skip(self.default_arg_start())
            .map(String::as_str)
    }
}
