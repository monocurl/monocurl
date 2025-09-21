use std::ops::Range;

pub type Count8 = usize;
pub type Count16 = usize;

#[derive(Clone, Copy)]
pub struct Location8 {
    pub row: usize,
    pub col: Count8,
}

#[derive(Clone, Copy)]
pub struct Location16 {
    pub row: usize,
    pub col: Count16,
}

pub type Span8 = Range<Count8>;
pub type Span16 = Range<Count16>;