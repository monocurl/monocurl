#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct SignedTimestamp {
    pub slide: isize,
    pub time: f64,
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct Timestamp {
    pub slide: usize,
    pub time: f64,
}

impl Default for Timestamp {
    fn default() -> Self {
        Self::right_before_slide(0)
    }
}

impl Timestamp {
    pub fn right_before_slide(slide: usize) -> Self {
        Self {
            slide,
            time: -f64::MIN_POSITIVE,
        }
    }

    pub fn at_end_of_slide(slide: usize) -> Self {
        Self {
            slide,
            time: f64::INFINITY,
        }
    }

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
