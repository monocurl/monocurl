use smallvec::SmallVec;

use crate::value::RcValue;

pub struct InvokedOperator {
    operator: RcValue,
    arguments: SmallVec<[RcValue; 1]>,
    operand: RcValue,
    labels: Vec<(usize, String)>,
    cached_result: Option<RcValue>,
}
