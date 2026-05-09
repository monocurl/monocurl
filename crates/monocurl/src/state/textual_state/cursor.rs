use std::ops::Range;

use structs::{
    rope::{Rope, TextAggregate},
    text::{Location8, Span8},
};

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Cursor {
    pub anchor: Location8,
    pub head: Location8,
}

impl Cursor {
    pub fn collapsed(pos: Location8) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn line_range(&self) -> Range<usize> {
        let start_row = self.anchor.min(self.head).row as usize;
        let end_row = self.anchor.max(self.head).row as usize;
        start_row..end_row + 1
    }

    pub fn reversed(&self) -> bool {
        self.head < self.anchor
    }
}

#[derive(Default)]
pub struct TransactionSummary {
    pub text_changes: Vec<(Span8, String, Rope<TextAggregate>, usize)>,
    pub new_cursor: Cursor,
    pub final_version: usize,
}
