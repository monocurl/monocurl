use smallvec::SmallVec;

use crate::value::Value;

/// result of applying an operator to an operand, possibly with labeled arguments.
/// stores enough information to recompute when labels change.
#[derive(Clone)]
pub struct InvokedOperator {
    pub operator: Box<Value>,
    pub arguments: SmallVec<[Value; 8]>,
    pub operand: Box<Value>,
    /// (argument_index, label_name) pairs
    pub labels: SmallVec<[(usize, String); 4]>,
    pub cached_result: Option<Box<Value>>,
}
