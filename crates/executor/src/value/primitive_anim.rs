use crate::value::RcValue;

pub enum PrimitiveAnim {
    Lerp {
        time: f64,
        // some lambda
        progression: Option<RcValue>,
    },
    Set,
}
