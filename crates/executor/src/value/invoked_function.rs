use crate::value::RcValue;


pub enum InvokedFunction {
    Labeled {
        lambda: RcValue,
        arguments: Vec<RcValue>,
        labels: Vec<(usize, String)>,
        cached_result: Option<RcValue>,
    },
    Unlabeled {
        result: RcValue
    }
}
