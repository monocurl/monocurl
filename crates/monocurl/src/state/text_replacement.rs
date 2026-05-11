use structs::text::{Count8, Span8};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TextReplacement {
    old: Span8,
    new_len: Count8,
}

impl TextReplacement {
    pub(crate) fn new(old: Span8, new_len: Count8) -> Self {
        Self { old, new_len }
    }

    pub(crate) fn map_offset(&self, offset: Count8) -> Count8 {
        if offset >= self.old.end {
            offset - self.old.len() + self.new_len
        } else if offset <= self.old.start {
            offset
        } else {
            self.old.start
        }
    }

    pub(crate) fn map_span(&self, span: &Span8) -> Span8 {
        let start = self.map_offset(span.start);
        let end = self.map_offset(span.end);
        start..end.max(start)
    }
}
