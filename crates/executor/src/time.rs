#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Timestamp {
    pub slide: usize,
    pub time: f64,
}

impl Timestamp {
    pub fn new(slide: usize, time: f64) -> Self {
        Self { slide, time }
    }
}
