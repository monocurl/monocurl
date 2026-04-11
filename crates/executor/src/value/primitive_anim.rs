use crate::value::Value;

/// leaf animation primitives that the executor knows how to step.
#[derive(Clone)]
pub enum PrimitiveAnim {
    /// interpolate all dirty followers toward their leaders over `time` seconds.
    /// optional `progression` lambda maps t in [0,1] to a modified t (easing).
    Lerp {
        time: f64,
        progression: Option<Box<Value>>,
    },
    /// instantly snap all dirty followers to their leaders.
    Set,
    /// do nothing for `time` seconds (keeps the animation running).
    Wait { time: f64 },
}
