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

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.slide.cmp(&other.slide) {
            std::cmp::Ordering::Equal => self.time.partial_cmp(&other.time),
            ord => Some(ord),
        }
    }
}
