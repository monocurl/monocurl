use crate::value::Value;

/// result of calling a function, possibly with labeled arguments.
/// labeled invocations store enough information to recompute the result
/// when a labeled argument changes (e.g. `f(a: 2, 1).a = 3`).
#[derive(Clone)]
pub enum InvokedFunction {
    Labeled {
        lambda: Box<Value>,
        arguments: Vec<Value>,
        /// (argument_index, label_name) pairs for labeled args
        labels: Vec<(usize, String)>,
        cached_result: Option<Box<Value>>,
    },
    Unlabeled {
        result: Box<Value>,
    },
}
