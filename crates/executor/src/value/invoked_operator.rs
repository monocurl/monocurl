use crate::value::Value;

/// result of applying an operator to an operand, possibly with labeled arguments.
/// stores enough information to recompute when labels change.
#[derive(Clone)]
pub struct InvokedOperator {
    pub operator: Box<Value>,
    pub arguments: Vec<Value>,
    pub operand: Box<Value>,
    /// (argument_index, label_name) pairs
    pub labels: Vec<(usize, String)>,
    pub cached_result: Option<Box<Value>>,
}
