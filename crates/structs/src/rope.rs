use arrayvec::ArrayString;
use std::{marker::PhantomData, sync::Arc};

use crate::text::{Count8, Count16};

// Inspired by: https://zed.dev/blog/zed-decoded-rope-sumtree
//
const MAX_LEAF_SIZE: usize = 64;
const DEFAULT_CHILDREN: usize = 8;

pub trait LeafData: Sized {
    fn identity() -> Self;

    fn split(&self, at: Count8) -> (Self, Self);
    // returns the portion that wasn't appended
    fn try_append(&mut self, from: Self) -> Option<Self>;

    fn utf8_chars(&self) -> Count8;
}

pub trait AggregateData: Sized {
    type LeafData: LeafData;

    fn from_leaf(data: Self::LeafData) -> Self;
    fn merge<const N: usize>(children: &[Self; N]) -> Self;

    fn count_utf8(&self) -> Count8;
    fn depth(&self) -> u32;
    fn weight(&self) -> usize;
}

mod internal {
    use std::sync::Arc;

    use crate::{rope::{AggregateData, DEFAULT_CHILDREN, TextAggregate, TextPrefixSummary}, text::{Count8, Count16}};

    // weight is calculated with respect to utf8
    pub(super) enum RopeNode<S: AggregateData, const N: usize = DEFAULT_CHILDREN> {
        Internal {
            agg: S,
            children: [Arc<RopeNode<S, N>>; N],
        },
        Leaf {
            agg: S,
            data: S::LeafData,
        },
    }

    impl<S: AggregateData, const N: usize> RopeNode<S, N> {
        pub(super) fn aggregate(&self) -> &S {
            match self {
                RopeNode::Internal { agg, .. } => agg,
                RopeNode::Leaf { agg, .. } => agg,
            }
        }
    }

    enum ChildIterationResult {
        // use none of child prefix summary (and break)
        AggregateNone,
        // use entire child prefix summary
        AggregateFull,
        // use a prefix of child prefix summary
        AggregatePrefix(TextPrefixSummary)
    }

    impl<const N: usize> RopeNode<TextAggregate, N> {
        /// Helper function to accumulate prefix summaries from children
        fn aggregate_children<F>(
            children: &[Arc<RopeNode<TextAggregate, N>>],
            mut f: F,
        ) -> TextPrefixSummary
        where
            F: FnMut(&TextPrefixSummary, &RopeNode<TextAggregate, N>) -> ChildIterationResult,
        {
            let mut chars_utf8 = 0;
            let mut chars_utf16 = 0;
            let mut newlines = 0;
            let mut chars_utf8_since_newline = 0;

            for child in children.iter() {
                let child_summary = child.aggregate().prefix_summary;

                match f(&child_summary, child) {
                    ChildIterationResult::AggregateNone => break,
                    ChildIterationResult::AggregateFull => {
                        chars_utf8 += child_summary.chars_utf8;
                        chars_utf16 += child_summary.chars_utf16;
                        newlines += child_summary.newlines;
                        chars_utf8_since_newline = child_summary.chars_utf8_since_newline;
                    }
                    ChildIterationResult::AggregatePrefix(summary) => {
                        chars_utf8 += summary.chars_utf8;
                        chars_utf16 += summary.chars_utf16;
                        newlines += summary.newlines;
                        chars_utf8_since_newline = summary.chars_utf8_since_newline;
                        break;
                    }
                }
            }

            TextPrefixSummary {
                chars_utf8,
                chars_utf16,
                newlines,
                chars_utf8_since_newline,
            }
        }

        pub fn utf8_prefix_summary(&self, at: Count8) -> TextPrefixSummary {
            match self {
                RopeNode::Internal { children, .. } => {
                    let mut remaining = at;

                    Self::aggregate_children(children,
                        |child_summary, child| {
                            let child_chars_utf8 = child_summary.chars_utf8;

                            if remaining == 0 {
                                ChildIterationResult::AggregateNone
                            } else if remaining > child_chars_utf8 {
                                remaining -= child_chars_utf8;
                                ChildIterationResult::AggregateFull
                            } else {
                                let summary = child.utf8_prefix_summary(remaining);
                                remaining = 0;
                                ChildIterationResult::AggregatePrefix(summary)
                            }
                        }
                    )
                }
                RopeNode::Leaf { data, .. } => {
                    let hunk = &data.0[..at.min(data.0.len())];
                    let chars_utf8 = hunk.len();
                    let chars_utf16 = hunk.encode_utf16().count();
                    let newlines = hunk.chars().filter(|&c| c == '\n').count();
                    let chars_utf8_since_newline = match hunk.rfind('\n') {
                        Some(pos) => hunk.len() - pos - 1,
                        None => hunk.len(),
                    };

                    TextPrefixSummary {
                        chars_utf8,
                        chars_utf16,
                        newlines,
                        chars_utf8_since_newline,
                    }
                }
            }
        }

        pub fn utf16_prefix_summary(&self, at: Count16) -> TextPrefixSummary {
            match self {
                RopeNode::Internal { children, .. } => {
                    let mut remaining = at;

                    Self::aggregate_children(
                        children,
                        |child_summary, child| {
                            let child_chars_utf16 = child_summary.chars_utf16;

                            if remaining == 0 {
                                ChildIterationResult::AggregateNone
                            } else if remaining > child_chars_utf16 {
                                remaining -= child_chars_utf16;
                                ChildIterationResult::AggregateFull
                            } else {
                                let summary = child.utf16_prefix_summary(remaining);
                                remaining = 0;
                                ChildIterationResult::AggregatePrefix(summary)
                            }
                        }
                    )
                }
                RopeNode::Leaf { data, .. } => {
                    let hunk = &data.0;
                    let mut chars_utf16 = 0;
                    let mut chars_utf8 = 0;
                    let mut newlines = 0;
                    let mut chars_utf8_since_newline = 0;

                    for c in hunk.chars() {
                        let char_len_utf16 = c.len_utf16();
                        if chars_utf16 + char_len_utf16 > at {
                            break;
                        }
                        chars_utf16 += char_len_utf16;
                        chars_utf8 += c.len_utf8();
                        if c == '\n' {
                            newlines += 1;
                            chars_utf8_since_newline = 0;
                        } else {
                            chars_utf8_since_newline += c.len_utf8();
                        }
                    }

                    TextPrefixSummary {
                        chars_utf8,
                        chars_utf16,
                        newlines,
                        chars_utf8_since_newline,
                    }
                }
            }
        }

        pub fn utf8_line_pos_prefix(&self, row: usize, col: Count8) -> TextPrefixSummary {
            match self {
                RopeNode::Internal { children, .. } => {
                    let mut remaining_row = row;
                    let mut remaining_col = col;

                    Self::aggregate_children(
                        children,
                        |child_summary, child| {
                            if remaining_row == 0 && remaining_col == 0 {
                                ChildIterationResult::AggregateNone
                            } else if (remaining_row, remaining_col) > (child_summary.newlines, child_summary.chars_utf8_since_newline) {
                                remaining_row -= child_summary.newlines;
                                if remaining_row == 0 {
                                    remaining_col -= child_summary.chars_utf8_since_newline;
                                }
                                ChildIterationResult::AggregateFull
                            } else {
                                let summary = child.utf8_line_pos_prefix(remaining_row, remaining_col);
                                remaining_row = 0;
                                remaining_col = 0;
                                ChildIterationResult::AggregatePrefix(summary)
                            }
                        }
                    )
                }
                RopeNode::Leaf { data, .. } => {
                    let hunk = &data.0;
                    let mut remaining_row = row;
                    let mut remaining_col = col;
                    let mut chars_utf8 = 0;
                    let mut chars_utf16 = 0;
                    let mut newlines = 0;
                    let mut chars_utf8_since_newline = 0;

                    for c in hunk.chars() {
                        if remaining_row == 0 && remaining_col == 0 {
                            break;
                        }

                        if c == '\n' {
                            if remaining_row == 0 {
                                // We're on the target row, stop at newline
                                break;
                            }
                            chars_utf8 += c.len_utf8();
                            chars_utf16 += c.len_utf16();
                            remaining_row -= 1;
                            newlines += 1;
                            chars_utf8_since_newline = 0;
                        } else {
                            if remaining_row == 0 && remaining_col > 0 {
                                // Only decrement col if we're on target row and haven't reached target col
                                let char_len = c.len_utf8();
                                if remaining_col >= char_len {
                                    remaining_col -= char_len;
                                } else {
                                    // Don't consume partial character
                                    break;
                                }
                            }
                            chars_utf8 += c.len_utf8();
                            chars_utf16 += c.len_utf16();
                            chars_utf8_since_newline += c.len_utf8();
                        }
                    }

                    TextPrefixSummary {
                        chars_utf8,
                        chars_utf16,
                        newlines,
                        chars_utf8_since_newline,
                    }
                }
            }
        }
    }
}

#[derive(Clone)]
pub struct Rope<S: AggregateData, const N: usize = DEFAULT_CHILDREN> {
    root: Arc<internal::RopeNode<S, N>>,
}

impl<S: AggregateData, const N: usize> Rope<S, N> {

    fn reroot() {

    }

}

impl<S: AggregateData, const N: usize> Rope<S, N> {

    pub fn replace_utf8_range() {

    }

    pub fn iterator_utf8() {

    }

    pub fn rev_iterator_utf8() {

    }

    pub fn count_utf8(&self) -> Count8 {
        self.root.aggregate().count_utf8()
    }

}

// if out of bounds, returns 0 or len
impl<const N: usize> Rope<TextAggregate, N> {

    pub fn utf8_prefix_summary(&self, at: Count8) -> TextPrefixSummary {
        self.root.utf8_prefix_summary(at)
    }

    pub fn utf16_prefix_summary(&self, at: Count16) -> TextPrefixSummary {
        self.root.utf16_prefix_summary(at)
    }

    pub fn utf8_line_pos_prefix(&self, row: usize, col: Count8) -> TextPrefixSummary {
        self.root.utf8_line_pos_prefix(row, col)
    }
}

mod iterator {
    // Iterates on utf8 position len and char
    struct RopeIterator<S: AggregateData, const N: usize> {
        stack: Vec<Arc<internal::RopeNode<S, N>>>,
        local_pos: Count8,
    }

    struct InverseRopeIterator<S: AggregateData, const N: usize> {
        stack: Vec<Arc<internal::RopeNode<S, N>>>,
        local_pos: Count8,
    }
}


#[derive(Debug, Clone)]
pub struct TextData(pub ArrayString<MAX_LEAF_SIZE>);

#[derive(Debug, Copy, Clone)]
pub struct TextAggregate {
    prefix_summary: TextPrefixSummary,
    depth: usize,
    nodes: usize,
}

#[derive(Debug, Copy, Clone)]
pub struct TextPrefixSummary {
    pub chars_utf8: usize,
    pub chars_utf16: usize,
    pub newlines: usize,
    pub chars_utf8_since_newline: usize,
}

impl LeafData for TextData {
    fn identity() -> Self {
        TextData(ArrayString::new_const())
    }

    fn split(&self, at: Count8) -> (Self, Self) {
        let left = ArrayString::from(&self.0[..at]).unwrap();
        let right = ArrayString::from(&self.0[at..]).unwrap();
        (TextData(left), TextData(right))
    }

    fn try_append(&mut self, from: Self) -> Option<Self> {
        if self.0.len() + from.0.len() <= MAX_LEAF_SIZE {
            self.0.push_str(&from.0);
            None
        } else {
            let take = MAX_LEAF_SIZE - self.0.len();
            self.0.push_str(&from.0[..take]);
            let remaining = ArrayString::from(&from.0[take..]).unwrap();
            Some(TextData(remaining))
        }
    }

    fn utf8_chars(&self) -> Count8 {
        self.0.len()
    }
}

impl AggregateData for TextAggregate {
    type LeafData = TextData;

    fn from_leaf(data: Self::LeafData) -> Self {
        let chars_utf8 = data.utf8_chars().0;
        let chars_utf16 = data.0.encode_utf16().count();
        let newlines = data.0.chars().filter(|&c| c == '\n').count();
        let chars_utf8_since_newline = match data.0.rfind('\n') {
            Some(pos) => data.0.len() - pos - 1,
            None => data.0.len(),
        };
        let prefix_summary = TextPrefixSummary {
            chars_utf8,
            chars_utf16,
            newlines,
            chars_utf8_since_newline,
        };

        TextAggregate {
            prefix_summary,
            depth: 0,
            nodes: 1,
        }
    }

    fn merge<const N: usize>(children: &[Self; N]) -> Self {
        let chars_utf8 = children.iter().map(|c| c.prefix_summary.chars_utf8).sum();
        let chars_utf16 = children.iter().map(|c| c.prefix_summary.chars_utf16).sum();
        let newlines = children.iter().map(|c| c.prefix_summary.newlines).sum();

        let last_newline = children.iter().rposition(|c| c.prefix_summary.newlines > 0);
        let chars_utf8_since_newline = match last_newline {
            Some(idx) => children[idx].prefix_summary.chars_utf8_since_newline + children[idx + 1..].iter().map(|x| x.prefix_summary.chars_utf8).sum::<usize>(),
            None => chars_utf8,
        };

        let depth = children.iter().map(|c| c.depth).max().unwrap_or(0) + 1;
        let nodes = children.iter().map(|c| c.nodes).sum::<usize>() + 1;

        let prefix_summary = TextPrefixSummary {
            chars_utf8,
            chars_utf16,
            newlines,
            chars_utf8_since_newline,
        };

        TextAggregate {
            prefix_summary,
            depth,
            nodes,
        }
    }

    fn depth(&self) -> u32 {
        self.depth as u32
    }

    fn weight(&self) -> usize {
        self.nodes
    }

    fn count_utf8(&self) -> Count8 {
        self.prefix_summary.chars_utf8
    }
}

#[derive(Clone)]
pub struct RLEAggregate<T> {
    chars_utf8: usize,
    depth: usize,
    nodes: usize,
    phantom_t: PhantomData<T>
}

#[derive(Clone)]
pub struct RLEData<T> {
    chars_utf8: usize,
    attribute: T,
}

impl<T> LeafData for RLEData<T>
where
    T: PartialEq + Default + Clone
{
    fn identity() -> Self {
        RLEData {
            chars_utf8: 0,
            attribute: T::default(),
        }
    }

    fn split(&self, at: Count8) -> (Self, Self) {
        let left = RLEData {
            chars_utf8: at,
            attribute: self.attribute.clone(),
        };
        let right = RLEData {
            chars_utf8: self.chars_utf8 - at,
            attribute: self.attribute.clone(),
        };
        (left, right)
    }

    fn try_append(&mut self, from: Self) -> Option<Self> {
        if self.attribute == from.attribute {
            self.chars_utf8 += from.chars_utf8;
            None
        }
        else if self.chars_utf8 == 0 {
            *self = from.clone();
            None
        }
        else if from.chars_utf8 == 0 {
            None
        }
        else {
            Some(from)
        }
    }

    fn utf8_chars(&self) -> Count8 {
        self.chars_utf8
    }
}

impl<T> AggregateData for RLEAggregate<T>
    where T: PartialEq + Default + Clone
{
    type LeafData = RLEData<T>;

    fn from_leaf(data: Self::LeafData) -> Self {
        let chars_utf8 = data.utf8_chars().0;
        RLEAggregate {
            chars_utf8,
            depth: 0,
            nodes: 1,
            phantom_t: PhantomData,
        }
    }

    fn merge<const N: usize>(children: &[Self; N]) -> Self {
        let chars_utf8 = children.iter().map(|c| c.chars_utf8).sum();
        let depth = children.iter().map(|c| c.depth).max().unwrap_or(0) + 1;
        let nodes = children.iter().map(|c| c.nodes).sum::<usize>() + 1;

        RLEAggregate {
            chars_utf8,
            depth,
            nodes,
            phantom_t: PhantomData,
        }
    }

    fn depth(&self) -> u32 {
        self.depth as u32
    }

    fn weight(&self) -> usize {
        self.nodes
    }

    fn count_utf8(&self) -> Count8 {
        self.chars_utf8
    }
}
