use crate::value::Value;

/// leaf animation primitives that the executor knows how to step.
#[derive(Clone)]
pub enum PrimitiveAnim {
    /// interpolate all dirty followers toward their leaders over `time` seconds.
    /// optional `progression` lambda maps t in [0,1] to a modified t (easing).
    Lerp(Box<LerpPrimitiveAnim>),
    /// instantly snap all dirty followers to their leaders.
    Set { candidates: Box<Value> },
    /// do nothing for `time` seconds (keeps the animation running).
    Wait { time: f64 },
}

#[derive(Clone)]
pub struct LerpPrimitiveAnim {
    pub candidates: Box<Value>,
    pub time: f64,
    pub progression: Option<Box<Value>>,
    pub embed: Option<Box<Value>>,
    pub lerp: Option<Box<Value>>,
}

impl PrimitiveAnim {
    pub fn lerp(
        candidates: Box<Value>,
        time: f64,
        progression: Option<Box<Value>>,
        embed: Option<Box<Value>>,
        lerp: Option<Box<Value>>,
    ) -> Self {
        Self::Lerp(Box::new(LerpPrimitiveAnim {
            candidates,
            time,
            progression,
            embed,
            lerp,
        }))
    }
}
