use std::ops::Range;

pub type Count8 = usize;
pub type Count16 = usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location8 {
    pub row: usize,
    pub col: Count8,
}

impl Ord for Location8 {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.row.cmp(&other.row) {
            std::cmp::Ordering::Equal => self.col.cmp(&other.col),
            ord => ord,
        }
    }
}

impl PartialOrd for Location8 {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Location8 {
    pub fn min(&self, other: &Self) -> Self {
        if self <= other {
            *self
        } else {
            *other
        }
    }
    pub fn max(&self, other: &Self) -> Self {
        if self >= other {
            *self
        } else {
            *other
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Location16 {
    pub row: usize,
    pub col: Count16,
}

pub type Span8 = Range<Count8>;
pub type Span16 = Range<Count16>;
