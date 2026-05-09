use structs::text::{Count8, Location8, Span8};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlideInfo {
    /// byte offset of the `slide` keyword start
    pub start_offset: Count8,
    /// byte range covered by this slide section
    pub source_range: Span8,
    /// 0-indexed source row of the `slide` keyword
    pub line: usize,
    /// cursor target at the end of the slide header
    pub header_end: Location8,
}
