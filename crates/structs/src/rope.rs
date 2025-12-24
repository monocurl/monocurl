use arrayvec::ArrayString;
use std::ops::Range;
use std::{marker::PhantomData, sync::Arc};
use std::iter::Once;
use std::str::Chars;
use crate::rope::internal::RopeNode;
use crate::{rope::iterator::{RopeIterator}, text::{Count8, Count16}};

// Inspired by: https://zed.dev/blog/zed-decoded-rope-sumtree
//
pub const MAX_LEAF_SIZE: usize = 64;
const DEFAULT_CHILDREN: usize = 8;

pub trait LeafData: Clone + Sized {
    type Iterator<'a>: DoubleEndedIterator where Self: 'a;

    fn identity() -> Self;

    fn subrange(&self, range: Range<usize>) -> Self;
    // returns the portion that wasn't appended
    fn try_append(&mut self, from: Self) -> Option<Self>;

    fn iterator(&self, at: Count8, to: Count8) -> Self::Iterator<'_>;
    fn codeunits(&self) -> usize;
}

pub trait AggregateData: Clone + Sized {
    type LeafData: LeafData;

    fn from_leaf(data: &Self::LeafData) -> Self;
    fn merge(children: &[Self]) -> Self;

    fn codeunits(&self) -> usize;
    fn depth(&self) -> u32;
    fn nodes(&self) -> usize;
}

mod internal {
    use std::sync::Arc;

    use crate::{rope::{AggregateData, DEFAULT_CHILDREN, TextAggregate, TextPrefixSummary}, text::{Count8, Count16}};

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
            let mut bytes_utf8 = 0;
            let mut codeunits_utf16 = 0;
            let mut newlines = 0;
            let mut bytes_utf8_since_newline = 0;

            for child in children.iter() {
                let child_summary = child.aggregate().prefix_summary;

                match f(&child_summary, child) {
                    ChildIterationResult::AggregateNone => break,
                    ChildIterationResult::AggregateFull => {
                        bytes_utf8 += child_summary.bytes_utf8;
                        codeunits_utf16 += child_summary.codeunits_utf16;
                        newlines += child_summary.newlines;
                        bytes_utf8_since_newline = child_summary.bytes_utf8_since_newline;
                    }
                    ChildIterationResult::AggregatePrefix(summary) => {
                        bytes_utf8 += summary.bytes_utf8;
                        codeunits_utf16 += summary.codeunits_utf16;
                        newlines += summary.newlines;
                        bytes_utf8_since_newline = summary.bytes_utf8_since_newline;
                        break;
                    }
                }
            }

            TextPrefixSummary {
                bytes_utf8,
                codeunits_utf16,
                newlines,
                bytes_utf8_since_newline,
            }
        }

        pub fn utf8_prefix_summary(&self, at: Count8) -> TextPrefixSummary {
            match self {
                RopeNode::Internal { children, .. } => {
                    let mut remaining = at;

                    Self::aggregate_children(children,
                        |child_summary, child| {
                            let child_bytes_utf8 = child_summary.bytes_utf8;

                            if remaining == 0 {
                                ChildIterationResult::AggregateNone
                            } else if remaining > child_bytes_utf8 {
                                remaining -= child_bytes_utf8;
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
                    let bytes_utf8 = hunk.len();
                    let codeunits_utf16 = hunk.encode_utf16().count();
                    let newlines = hunk.chars().filter(|&c| c == '\n').count();
                    let bytes_utf8_since_newline = match hunk.rfind('\n') {
                        Some(pos) => hunk.len() - pos - 1,
                        None => hunk.len(),
                    };

                    TextPrefixSummary {
                        bytes_utf8,
                        codeunits_utf16,
                        newlines,
                        bytes_utf8_since_newline,
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
                            let child_codeunits_utf16 = child_summary.codeunits_utf16;

                            if remaining == 0 {
                                ChildIterationResult::AggregateNone
                            } else if remaining > child_codeunits_utf16 {
                                remaining -= child_codeunits_utf16;
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
                    let mut codeunits_utf16 = 0;
                    let mut bytes_utf8 = 0;
                    let mut newlines = 0;
                    let mut bytes_utf8_since_newline = 0;

                    for c in hunk.chars() {
                        let byte_len_utf16 = c.len_utf16();
                        if codeunits_utf16 + byte_len_utf16 > at {
                            break;
                        }
                        codeunits_utf16 += byte_len_utf16;
                        bytes_utf8 += c.len_utf8();
                        if c == '\n' {
                            newlines += 1;
                            bytes_utf8_since_newline = 0;
                        } else {
                            bytes_utf8_since_newline += c.len_utf8();
                        }
                    }

                    TextPrefixSummary {
                        bytes_utf8,
                        codeunits_utf16,
                        newlines,
                        bytes_utf8_since_newline,
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
                            } else if (remaining_row, remaining_col) > (child_summary.newlines, child_summary.bytes_utf8_since_newline) {
                                remaining_row -= child_summary.newlines;
                                if remaining_row == 0 {
                                    remaining_col -= child_summary.bytes_utf8_since_newline;
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
                    let mut bytes_utf8 = 0;
                    let mut codeunits_utf16 = 0;
                    let mut newlines = 0;
                    let mut bytes_utf8_since_newline = 0;

                    for c in hunk.chars() {
                        if remaining_row == 0 && remaining_col == 0 {
                            break;
                        }

                        if c == '\n' {
                            if remaining_row == 0 {
                                // We're on the target row, stop at newline
                                break;
                            }
                            bytes_utf8 += c.len_utf8();
                            codeunits_utf16 += c.len_utf16();
                            remaining_row -= 1;
                            newlines += 1;
                            bytes_utf8_since_newline = 0;
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
                            bytes_utf8 += c.len_utf8();
                            codeunits_utf16 += c.len_utf16();
                            bytes_utf8_since_newline += c.len_utf8();
                        }
                    }

                    TextPrefixSummary {
                        bytes_utf8,
                        codeunits_utf16,
                        newlines,
                        bytes_utf8_since_newline,
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

impl<S: AggregateData, const N: usize> Default for Rope<S, N> {
    fn default() -> Self {
        let null_leaf = RopeNode::<S, N>::Leaf {
            agg: S::from_leaf(&S::LeafData::identity()),
            data: S::LeafData::identity(),
        };

        Rope {
            root: Arc::new(null_leaf),
        }
    }
}

impl<S: AggregateData, const N: usize> Rope<S, N> {

    pub fn iterator_utf8(&self, pos: Count8) -> impl Iterator<Item=<<S::LeafData as LeafData>::Iterator<'_> as Iterator>::Item> {
        RopeIterator::<'_, S, N, true>::new(self, pos)
    }

    pub fn rev_iterator_utf8(&self, pos: Count8) -> impl Iterator<Item=<<S::LeafData as LeafData>::Iterator<'_> as Iterator>::Item> {
        RopeIterator::<'_, S, N, false>::new(self, pos)
    }

    pub fn codeunits(&self) -> usize {
        self.root.aggregate().codeunits()
    }
}

impl<S: AggregateData, const N: usize> Rope<S, N> {
    /// returns a new rope with the modification applied (persistent structure).
    pub fn replace_range(&self, range: Range<usize>, new_data: impl Into<Vec<S::LeafData>>) -> Self {
        let leaves = new_data.into();

        let (before, rest) = self.split_at(range.start);
        let (_deleted, after) = rest.split_at(range.end - range.start);

        let middle = Self::from_leaves(leaves);
        Self::concat([before, middle, after]).rebalance_if_needed()
    }

    /// split the rope at a given position into [0, pos) and [pos, end)
    pub fn split_at(&self, pos: usize) -> (Self, Self) {
        if pos == 0 {
            return (Self::empty(), self.clone());
        }
        if pos >= self.codeunits() {
            return (self.clone(), Self::empty());
        }

        let (left, right) = self.split_node(&self.root, pos);
        (left, right)
    }

    fn split_node(
        &self,
        node: &Arc<RopeNode<S, N>>,
        pos: usize,
    ) -> (Self, Self) {
        match &**node {
            RopeNode::Leaf { data, .. } => {
                // Split the leaf data
                let left_data = data.subrange(0..pos);
                let right_data = data.subrange(pos..data.codeunits());

                let left_leaf = RopeNode::Leaf {
                    agg: S::from_leaf(&left_data),
                    data: left_data,
                };

                let right_leaf = RopeNode::Leaf {
                    agg: S::from_leaf(&right_data),
                    data: right_data,
                };

                (
                    Rope { root: Arc::new(left_leaf) },
                    Rope { root: Arc::new(right_leaf) },
                )
            }
            RopeNode::Internal { children, .. } => {
                let mut utf8 = 0;
                for (i, child) in children.iter().enumerate() {
                    let child_size = child.aggregate().codeunits();

                    if pos < utf8 + child_size {
                        let local_pos = pos - utf8;
                        let (left_child, right_child) = self.split_node(child, local_pos);

                        let mut left_children = Vec::new();
                        for j in 0..i {
                            left_children.push(children[j].clone());
                        }
                        if left_child.codeunits() > 0 {
                            left_children.push(left_child.root.clone());
                        }

                        let mut right_children = Vec::new();
                        if right_child.codeunits() > 0 {
                            right_children.push(right_child.root.clone());
                        }
                        for j in (i + 1)..children.len() {
                            right_children.push(children[j].clone());
                        }

                        return (
                            Self::from_children(left_children).rebalance_if_needed(),
                            Self::from_children(right_children).rebalance_if_needed(),
                        );
                    }

                    utf8 += child_size;
                }

                unreachable!("invariants failed")
            }
        }
    }

    pub fn concat<const U: usize>(ropes: [Self; U]) -> Self {
        let children = ropes
            .into_iter()
            .filter(|r| r.codeunits() > 0)
            .map(|r| r.root)
            .collect();

        Self::from_children(children)
    }

    pub fn from_leaves(leaves: Vec<S::LeafData>) -> Self {
        let leaf_nodes: Vec<_> = leaves
            .into_iter()
            .filter(|leaf| leaf.codeunits() > 0) // Skip empty leaves
            .map(|data| {
                Arc::new(RopeNode::Leaf {
                    agg: S::from_leaf(&data),
                    data,
                })
            })
            .collect();

        Self::from_children(leaf_nodes)
    }

    fn from_children(mut nodes: Vec<Arc<RopeNode<S, N>>>) -> Self {
        if nodes.is_empty() {
            return Self::empty();
        }
        if nodes.len() == 1 {
            return Rope { root: nodes[0].clone() };
        }

        // build tree layer-by-layer
        while nodes.len() > 1 {
            let mut next_level = Vec::new();

            for chunk in nodes.chunks(N) {
                if chunk.len() == 1 {
                    next_level.push(chunk[0].clone());
                } else {
                    let children_array: [Arc<RopeNode<S, N>>; N] =
                        std::array::from_fn(|i| {
                            if i < chunk.len() {
                                chunk[i].clone()
                            } else {
                                Arc::new(Self::empty_leaf())
                            }
                        });

                    let agg = Self::aggregate_children(&children_array);

                    let internal = Arc::new(RopeNode::Internal {
                        agg,
                        children: children_array,
                    });

                    next_level.push(internal);
                }
            }

            nodes = next_level;
        }

        Rope { root: nodes[0].clone() }
    }

    fn empty_leaf() -> RopeNode<S, N> {
        let data = S::LeafData::identity();
        RopeNode::Leaf {
            agg: S::from_leaf(&data),
            data,
        }
    }

    fn empty() -> Self {
        Self::default()
    }

    fn aggregate_children(children: &[Arc<RopeNode<S, N>>]) -> S {
        let aggs: Vec<_> = children
            .iter()
            .map(|c| c.aggregate().clone())
            .collect();

        S::merge(&aggs)
    }

    fn rebalance_if_needed(self) -> Self {
        let depth = self.root.aggregate().depth();
        let size = self.root.aggregate().nodes();

        // Rebalance if depth is more than 2 * log2(weight)
        let max_depth = if size > 0 {
            (64 - (size as u64).leading_zeros()) * 2
        } else {
            1
        };

        if depth > max_depth {
            self.rebalance()
        } else {
            self
        }
    }

    fn rebalance(self) -> Self {
        let merged_leaves = self.collect_and_merge_leaves();
        Self::from_leaves(merged_leaves)
    }

    fn collect_and_merge_leaves(&self) -> Vec<S::LeafData> {
        let mut result = Vec::new();
        self.collect_and_merge_rec(&self.root, &mut result);
        result
    }

    fn collect_and_merge_rec(&self, node: &Arc<RopeNode<S, N>>, result: &mut Vec<S::LeafData>) {
        match &**node {
            RopeNode::Leaf { data, .. } => {
                if data.codeunits() == 0 {
                    return;
                }

                // try to merge with the last leaf in result
                if let Some(last) = result.last_mut() {
                    if let Some(remainder) = last.try_append(data.clone()) {
                        // couldn't merge completely, push the remainder
                        result.push(remainder);
                    }
                } else {
                    result.push(data.clone());
                }
            }
            RopeNode::Internal { children, .. } => {
                for child in children.iter() {
                    if child.aggregate().codeunits() > 0 {
                        self.collect_and_merge_rec(child, result);
                    }
                }
            }
        }
    }

    pub fn subrange_aggregate(&self, range: Range<usize>) -> S {
        self.subrange_aggregate_rec(&self.root, range)
    }

    fn subrange_aggregate_rec(&self, node: &Arc<RopeNode<S, N>>, local_range: Range<usize>) -> S {
        if local_range.start == 0 && local_range.end >= node.aggregate().codeunits() {
            return node.aggregate().clone();
        }

        match &**node {
            RopeNode::Leaf { data, .. } => {
                let sub_data = data.subrange(local_range);
                S::from_leaf(&sub_data)
            }
            RopeNode::Internal { children, .. } => {
                let mut collected_aggs = Vec::new();

                let mut utf8 = 0;
                for child in children.iter() {
                    let child_size = child.aggregate().codeunits();

                    if local_range.start >= utf8 + child_size {
                        utf8 += child_size;
                        continue;
                    }

                    if local_range.end <= utf8 {
                        break;
                    }

                    // Overlapping range
                    let start_in_child = local_range.start.saturating_sub(utf8);
                    let end_in_child = (local_range.end - utf8).min(child_size);

                    let child_agg = self.subrange_aggregate_rec(child, start_in_child..end_in_child);
                    collected_aggs.push(child_agg);

                    utf8 += child_size;
                }

                S::merge(&collected_aggs)
            }
        }
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
    use std::sync::Arc;

    use crate::{rope::{AggregateData, Rope}, text::Count8};
    use crate::rope::internal::RopeNode;
    use crate::rope::LeafData;

    pub(crate) struct RopeIterator<'a, S: AggregateData, const N: usize, const F: bool> {
        // (node, index in parent)
        stack: Vec<(&'a Arc<RopeNode<S, N>>, usize)>,
        sub_iterator: Option<<S::LeafData as LeafData>::Iterator<'a>>,
    }

    impl<'a, S: AggregateData, const N: usize, const F: bool> RopeIterator<'a, S, N, F> {

        fn build_stack(node: &'a Arc<RopeNode<S, N>>, local_pos: Count8, stack: &mut Vec<(&'a Arc<RopeNode<S, N>>, usize)>) -> <S::LeafData as LeafData>::Iterator<'a> {
            match &**node {
                RopeNode::Leaf { data, .. } => {
                    if F {
                        data.iterator(local_pos, data.codeunits())
                    } else {
                        data.iterator(0, local_pos)
                    }
                }
                RopeNode::Internal { children, .. } => {
                    let mut utf8 = 0usize;
                    for (index, child) in children.iter().enumerate() {
                        let end = utf8 + child.aggregate().codeunits();
                        let in_child = if F {
                            local_pos < end
                        } else {
                            local_pos <= end
                        };
                        if in_child {
                            stack.push((child, index));
                            return Self::build_stack(child, local_pos - utf8, stack)
                        }
                        utf8 = end;
                    }
                    unreachable!("Invariants not withheld")
                }
            }
        }

        pub fn new(rope: &'a Rope<S, N>, pos: Count8) -> Self {
            assert!(pos <= rope.codeunits());
            let end = if F {
                pos == rope.codeunits()
            } else {
                pos == 0
            };

            if end {
                Self {
                    stack: vec![],
                    sub_iterator: None,
                }
            }
            else {
                let mut stack = Vec::new();
                stack.push((&rope.root, 0));
                let sub_iterator = Self::build_stack(&rope.root, pos, &mut stack);
                Self {
                    stack,
                    sub_iterator: Some(sub_iterator)
                }
            }
        }

        fn next_node(&mut self) -> Option<()> {
            let Some((_leaf, mut last_index)) = self.stack.pop() else {
                return None;
            };

            loop {
                // if not exist, then we unravelled stack and there is no next
                let (node_arc, _) = self.stack.last()?;

                match &***node_arc {
                    RopeNode::Internal { children, .. } => {
                        let not_last = if F {
                            last_index < children.len() - 1
                        } else {
                            last_index > 0
                        };

                        if not_last {
                            let (index, pos) = if F {
                                (last_index + 1, 0)
                            } else {
                                (last_index - 1, children[last_index - 1].aggregate().codeunits())
                            };

                            self.stack.push((&children[index], index));
                            self.sub_iterator = Some(Self::build_stack(&children[index], pos, &mut self.stack));
                            break
                        }
                        else {
                            // climb up the stack
                            last_index = self.stack.pop().unwrap().1;
                        }
                    }
                    RopeNode::Leaf { .. } => unreachable!()
                }
            }

            Some(())
        }

        fn advance(&mut self) -> Option<()> {
            let result = self.next_node();

            self.sub_iterator = self.stack.last().map(|(node, _)| {
                let RopeNode::Leaf { data, agg } = &***node else {
                    unreachable!();
                };

                data.iterator(0,agg.codeunits())
            });

            result
        }

        fn sub_advance(&mut self) -> Option<<<S::LeafData as LeafData>::Iterator<'a> as Iterator>::Item> {
            if F {
                self.sub_iterator.as_mut().and_then(|it| it.next())
            } else {
                self.sub_iterator.as_mut().and_then(|it| it.next_back())
            }
        }
    }

    impl<'a, S: AggregateData, const N: usize, const F: bool> Iterator for RopeIterator<'a, S, N, F> {
        type Item = <<S::LeafData as LeafData>::Iterator<'a> as Iterator>::Item;

        fn next(&mut self) -> Option<Self::Item> {
            loop {
                if let Some(result) = self.sub_advance() {
                    return Some(result);
                }

                self.advance()?
            }
        }
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
    pub bytes_utf8: usize,
    pub codeunits_utf16: usize,
    pub newlines: usize,
    pub bytes_utf8_since_newline: usize,
}

impl LeafData for TextData {
    type Iterator<'a> = Chars<'a>;

    fn identity() -> Self {
        TextData(ArrayString::new_const())
    }

    fn subrange(&self, range: Range<usize>) -> Self {
        TextData(ArrayString::from(&self.0[range]).unwrap())
    }

    fn try_append(&mut self, from: Self) -> Option<Self> {
        let remaining_capacity = MAX_LEAF_SIZE - self.0.len();

        let mut end = 0;
        for c in from.0.chars() {
            let char_len = c.len_utf8();
            if end + char_len > remaining_capacity {
                break;
            }
            end += char_len;
        }

        self.0.push_str(&from.0[..end]);

        if end < from.0.len() {
            let remaining = ArrayString::from(&from.0[end..]).unwrap();
            Some(TextData(remaining))
        } else {
            None
        }
    }

    fn iterator(&self, at: Count8, to: Count8) -> Self::Iterator<'_> {
        self.0[at..to].chars()
    }

    fn codeunits(&self) -> Count8 {
        self.0.len()
    }

}

impl AggregateData for TextAggregate {
    type LeafData = TextData;

    fn from_leaf(data: &Self::LeafData) -> Self {
        let bytes_utf8 = data.codeunits();
        let bytes_utf16 = data.0.encode_utf16().count();
        let newlines = data.0.chars().filter(|&c| c == '\n').count();
        let bytes_utf8_since_newline = match data.0.rfind('\n') {
            Some(pos) => data.0.len() - pos - 1,
            None => data.0.len(),
        };
        let prefix_summary = TextPrefixSummary {
            bytes_utf8,
            codeunits_utf16: bytes_utf16,
            newlines,
            bytes_utf8_since_newline,
        };

        TextAggregate {
            prefix_summary,
            depth: 0,
            nodes: 1,
        }
    }

    fn merge(children: &[Self]) -> Self {
        let bytes_utf8 = children.iter().map(|c| c.prefix_summary.bytes_utf8).sum();
        let bytes_utf16 = children.iter().map(|c| c.prefix_summary.codeunits_utf16).sum();
        let newlines = children.iter().map(|c| c.prefix_summary.newlines).sum();

        let last_newline = children.iter().rposition(|c| c.prefix_summary.newlines > 0);
        let bytes_utf8_since_newline = match last_newline {
            Some(idx) => children[idx].prefix_summary.bytes_utf8_since_newline + children[idx + 1..].iter().map(|x| x.prefix_summary.bytes_utf8).sum::<usize>(),
            None => bytes_utf8,
        };

        let depth = children.iter().map(|c| c.depth).max().unwrap_or(0) + 1;
        let nodes = children.iter().map(|c| c.nodes).sum::<usize>() + 1;

        let prefix_summary = TextPrefixSummary {
            bytes_utf8,
            codeunits_utf16: bytes_utf16,
            newlines,
            bytes_utf8_since_newline,
        };

        TextAggregate {
            prefix_summary,
            depth,
            nodes,
        }
    }

    fn codeunits(&self) -> Count8 {
        self.prefix_summary.bytes_utf8
    }

    fn depth(&self) -> u32 {
        self.depth as u32
    }

    fn nodes(&self) -> usize {
        self.nodes
    }
}

impl<const N: usize> Rope<TextAggregate, N> {
    pub fn leaves_from_str(text: &str) -> Vec<TextData> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut leaves = Vec::new();
        let mut start = 0;

        while start < text.len() {
            let end = (start + MAX_LEAF_SIZE).min(text.len());

            // Find UTF-8 boundary
            let mut adjusted_end = end;
            while adjusted_end > start && !text.is_char_boundary(adjusted_end) {
                adjusted_end -= 1;
            }

            if adjusted_end == start {
                // This should never happen with valid UTF-8
                unreachable!("Invalid UTF-8 or character larger than MAX_LEAF_SIZE");
            }

            let slice = &text[start..adjusted_end];
            leaves.push(TextData(ArrayString::from(slice).unwrap()));
            start = adjusted_end;
        }

        leaves
    }

    pub fn from_str(s: &str) -> Self {
        Self::default().replace_range(0..0, Self::leaves_from_str(s))
    }
}

#[derive(Clone)]
pub struct RLEAggregate<T> {
    bytes_utf8: usize,
    depth: usize,
    nodes: usize,
    phantom_t: PhantomData<T>
}

#[derive(Clone)]
pub struct RLEData<T> {
    bytes_utf8: usize,
    attribute: T,
}

impl<T> LeafData for RLEData<T>
where
    T: PartialEq + Default + Clone
{
    type Iterator<'a> = Once<(Count8, T)> where T: 'a;

    fn identity() -> Self {
        RLEData {
            bytes_utf8: 0,
            attribute: T::default(),
        }
    }

    fn subrange(&self, range: Range<usize>) -> Self {
        RLEData {
            bytes_utf8: range.len(),
            attribute: self.attribute.clone(),
        }
    }

    fn try_append(&mut self, from: Self) -> Option<Self> {
        if self.attribute == from.attribute {
            self.bytes_utf8 += from.bytes_utf8;
            None
        }
        else if self.bytes_utf8 == 0 {
            *self = from.clone();
            None
        }
        else if from.bytes_utf8 == 0 {
            None
        }
        else {
            Some(from)
        }
    }

    fn iterator(&self, at: Count8, to: Count8) -> Self::Iterator<'_> {
        debug_assert!(at <= to && to <= self.bytes_utf8);
        std::iter::once((to - at, self.attribute.clone()))
    }

    fn codeunits(&self) -> Count8 {
        self.bytes_utf8
    }
}

impl<T> AggregateData for RLEAggregate<T>
    where T: PartialEq + Default + Clone
{
    type LeafData = RLEData<T>;

    fn from_leaf(data: &Self::LeafData) -> Self {
        let bytes_utf8 = data.codeunits();
        RLEAggregate {
            bytes_utf8,
            depth: 0,
            nodes: 1,
            phantom_t: PhantomData,
        }
    }

    fn merge(children: &[Self]) -> Self {
        let bytes_utf8 = children.iter().map(|c| c.bytes_utf8).sum();
        let depth = children.iter().map(|c| c.depth).max().unwrap_or(0) + 1;
        let nodes = children.iter().map(|c| c.nodes).sum::<usize>() + 1;

        RLEAggregate {
            bytes_utf8,
            depth,
            nodes,
            phantom_t: PhantomData,
        }
    }

    fn codeunits(&self) -> Count8 {
        self.bytes_utf8
    }

    fn depth(&self) -> u32 {
        self.depth as u32
    }

    fn nodes(&self) -> usize {
        self.nodes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create a simple rope from text
    fn rope_from_str(s: &str) -> Rope<TextAggregate, 8> {
        Rope::from_str(s)
    }

    #[test]
    fn test_empty_rope_iterator() {
        let rope = rope_from_str("");
        let chars: Vec<char> = rope.iterator_utf8(0).collect();
        assert_eq!(chars, vec![]);
    }

    #[test]
    fn test_single_node_forward_iterator() {
        let rope = rope_from_str("hello");
        let chars: String = rope.iterator_utf8(0).collect();
        assert_eq!(chars, "hello");
    }

    #[test]
    fn test_single_node_reverse_iterator() {
        let rope = rope_from_str("hello");
        let chars: String = rope.rev_iterator_utf8(5).collect();
        assert_eq!(chars, "olleh");
    }

    #[test]
    fn test_forward_iterator_from_middle() {
        let rope = rope_from_str("hello world");
        let chars: String = rope.iterator_utf8(6).collect();
        assert_eq!(chars, "world");
    }

    #[test]
    fn test_reverse_iterator_from_middle() {
        let rope = rope_from_str("hello world");
        let chars: String = rope.rev_iterator_utf8(5).collect();
        assert_eq!(chars, "olleh");
    }

    #[test]
    fn test_unicode_forward_iterator() {
        let rope = rope_from_str("hello 🦀 world");
        let chars: String = rope.iterator_utf8(0).collect();
        assert_eq!(chars, "hello 🦀 world");
    }

    #[test]
    fn test_unicode_reverse_iterator() {
        let rope = rope_from_str("hello 🦀 world");
        let count = rope.codeunits();
        let chars: String = rope.rev_iterator_utf8(count).collect();
        assert_eq!(chars, "dlrow 🦀 olleh");
    }

    #[test]
    fn test_iterator_from_end() {
        let rope = rope_from_str("hello");
        let chars: Vec<char> = rope.iterator_utf8(5).collect();
        assert_eq!(chars, vec![]);
    }

    #[test]
    fn test_iterator_from_start_reverse() {
        let rope = rope_from_str("hello");
        let chars: Vec<char> = rope.rev_iterator_utf8(0).collect();
        assert_eq!(chars, vec![]);
    }

    #[test]
    fn test_multi_node_forward_iterator() {
        // Create rope with multiple nodes (>64 chars per node)
        let text = "a".repeat(100);
        let rope = rope_from_str(&text);
        let chars: String = rope.iterator_utf8(0).collect();
        assert_eq!(chars, text);
    }

    #[test]
    fn test_multi_node_forward_iterator2() {
        let text = "a".repeat(1000) + &"b".repeat(150) + &"c".repeat(1);
        let rope = rope_from_str(&text);
        let cands = [0, 500, 512, 1000, 1150, 1151];
        for &cand in &cands {
            let chars: String = rope.iterator_utf8(cand).collect();
            let expected = text[cand..].to_string();
            assert_eq!(chars, expected);
        }
    }

    #[test]
    fn test_multi_node_reverse_iterator() {
        let text = "a".repeat(100);
        let rope = rope_from_str(&text);
        let count = rope.codeunits();
        let chars: String = rope.rev_iterator_utf8(count).collect();
        assert_eq!(chars, text);
    }

    #[test]
    fn test_multi_node_reverse_iterator2() {
        let text = "a".repeat(1000) + &"b".repeat(150) + &"c".repeat(1);
        let rope = rope_from_str(&text);
        let cands = [0, 500, 512, 1000, 1150, 1151];
        for &cand in &cands {
            let chars: String = rope.rev_iterator_utf8(cand).collect();
            let expected: String = text[..cand].chars().rev().collect();
            assert_eq!(chars, expected);
        }
    }

    #[test]
    fn test_utf8_prefix_summary_empty() {
        let rope = rope_from_str("");
        let summary = rope.utf8_prefix_summary(0);
        assert_eq!(summary.bytes_utf8, 0);
        assert_eq!(summary.codeunits_utf16, 0);
        assert_eq!(summary.newlines, 0);
    }

    #[test]
    fn test_utf8_prefix_summary_simple() {
        let rope = rope_from_str("hello");
        let summary = rope.utf8_prefix_summary(5);
        assert_eq!(summary.bytes_utf8, 5);
        assert_eq!(summary.codeunits_utf16, 5);
        assert_eq!(summary.newlines, 0);
        assert_eq!(summary.bytes_utf8_since_newline, 5);
    }

    #[test]
    fn test_utf8_prefix_summary_with_newlines() {
        let rope = rope_from_str("hello\nworld\n!");
        let summary = rope.utf8_prefix_summary(13);
        assert_eq!(summary.bytes_utf8, 13);
        assert_eq!(summary.newlines, 2);
        assert_eq!(summary.bytes_utf8_since_newline, 1); // "!"
    }

    #[test]
    fn test_utf8_prefix_summary_partial() {
        let rope = rope_from_str("hello\nworld");
        let summary = rope.utf8_prefix_summary(8); // "hello\nwo"
        assert_eq!(summary.bytes_utf8, 8);
        assert_eq!(summary.newlines, 1);
        assert_eq!(summary.bytes_utf8_since_newline, 2); // "wo"
    }

    #[test]
    fn test_utf16_prefix_summary_ascii() {
        let rope = rope_from_str("hello");
        let summary = rope.utf16_prefix_summary(5);
        assert_eq!(summary.bytes_utf8, 5);
        assert_eq!(summary.codeunits_utf16, 5);
    }

    #[test]
    fn test_utf16_prefix_summary_unicode() {
        let rope = rope_from_str("🦀"); // 4 bytes UTF-8, 2 units UTF-16
        let summary = rope.utf16_prefix_summary(2);
        assert_eq!(summary.bytes_utf8, 4);
        assert_eq!(summary.codeunits_utf16, 2);
    }

    #[test]
    fn test_utf16_prefix_summary_mixed() {
        let rope = rope_from_str("hi🦀"); // "hi" = 2+2, "🦀" = 4+2
        let summary = rope.utf16_prefix_summary(4);
        assert_eq!(summary.bytes_utf8, 6);
        assert_eq!(summary.codeunits_utf16, 4);
    }

    #[test]
    fn test_utf8_line_pos_prefix_start() {
        let rope = rope_from_str("hello\nworld\ntest");
        let summary = rope.utf8_line_pos_prefix(0, 0);
        assert_eq!(summary.bytes_utf8, 0);
        assert_eq!(summary.newlines, 0);
    }

    #[test]
    fn test_utf8_line_pos_prefix_first_line() {
        let rope = rope_from_str("hello\nworld\ntest");
        let summary = rope.utf8_line_pos_prefix(0, 3); // "hel"
        assert_eq!(summary.bytes_utf8, 3);
        assert_eq!(summary.newlines, 0);
        assert_eq!(summary.bytes_utf8_since_newline, 3);
    }

    #[test]
    fn test_utf8_line_pos_prefix_second_line() {
        let rope = rope_from_str("hello\nworld\ntest");
        let summary = rope.utf8_line_pos_prefix(1, 2); // to "wo" in "world"
        assert_eq!(summary.bytes_utf8, 8); // "hello\nwo"
        assert_eq!(summary.newlines, 1);
        assert_eq!(summary.bytes_utf8_since_newline, 2);
    }

    #[test]
    fn test_utf8_line_pos_prefix_third_line() {
        let rope = rope_from_str("hello\nworld\ntest");
        let summary = rope.utf8_line_pos_prefix(2, 4); // to "test"
        assert_eq!(summary.bytes_utf8, 16); // "hello\nworld\ntest"
        assert_eq!(summary.newlines, 2);
        assert_eq!(summary.bytes_utf8_since_newline, 4);
    }

    #[test]
    fn test_utf8_line_pos_prefix_end_of_line() {
        let rope = rope_from_str("hello\nworld");
        let summary = rope.utf8_line_pos_prefix(0, 5); // end of "hello"
        assert_eq!(summary.bytes_utf8, 5);
        assert_eq!(summary.newlines, 0);
    }

    #[test]
    fn test_text_data_try_append_fits() {
        let mut data1 = TextData(ArrayString::from("hello").unwrap());
        let data2 = TextData(ArrayString::from(" world").unwrap());
        let remaining = data1.try_append(data2);
        assert_eq!(data1.0.as_str(), "hello world");
        assert!(remaining.is_none());
    }

    #[test]
    fn test_text_data_try_append_overflow() {
        let mut data1 = TextData(ArrayString::from("a".repeat(60).as_str()).unwrap());
        let data2 = TextData(ArrayString::from("b".repeat(10).as_str()).unwrap());
        let remaining = data1.try_append(data2);
        assert_eq!(data1.0.len(), MAX_LEAF_SIZE);
        assert!(remaining.is_some());
        assert_eq!(remaining.unwrap().0.as_str(), "bbbbbb");
    }

    #[test]
    fn test_text_data_try_append_unicode_boundary() {
        let mut data1 = TextData(ArrayString::from("a".repeat(MAX_LEAF_SIZE - 1).as_str()).unwrap());
        let data2 = TextData(ArrayString::from("🦀").unwrap()); // 4 bytes
        let remaining = data1.try_append(data2);
        assert_eq!(data1.0.len(), MAX_LEAF_SIZE - 1);
        assert!(remaining.is_some());
        assert_eq!(remaining.unwrap().0.as_str(), "🦀");
    }

    #[test]
    fn test_text_data_split() {
        let data = TextData(ArrayString::from("hello world").unwrap());
        let left = data.subrange(0..6);
        let right = data.subrange(6..11);
        assert_eq!(left.0.as_str(), "hello ");
        assert_eq!(right.0.as_str(), "world");
    }

    #[test]
    fn test_rle_try_append_same_attribute() {
        let mut rle1 = RLEData {
            bytes_utf8: 10,
            attribute: 5,
        };
        let rle2 = RLEData {
            bytes_utf8: 5,
            attribute: 5,
        };
        let remaining = rle1.try_append(rle2);
        assert_eq!(rle1.bytes_utf8, 15);
        assert!(remaining.is_none());
    }

    #[test]
    fn test_rle_try_append_different_attribute() {
        let mut rle1 = RLEData {
            bytes_utf8: 10,
            attribute: 5,
        };
        let rle2 = RLEData {
            bytes_utf8: 5,
            attribute: 7,
        };
        let remaining = rle1.try_append(rle2);
        assert_eq!(rle1.bytes_utf8, 10); // Unchanged
        assert!(remaining.is_some());
        assert_eq!(remaining.unwrap().attribute, 7);
    }

    #[test]
    fn test_rle_split() {
        let rle = RLEData {
            bytes_utf8: 20,
            attribute: 42,
        };
        let left = rle.subrange(0..12);
        let right = rle.subrange(12..20);
        assert_eq!(left.bytes_utf8, 12);
        assert_eq!(left.attribute, 42);
        assert_eq!(right.bytes_utf8, 8);
        assert_eq!(right.attribute, 42);
    }

    #[test]
    fn test_aggregate_merge() {
        let agg1 = TextAggregate::from_leaf(&TextData(ArrayString::from("hello\n").unwrap()));
        let agg2 = TextAggregate::from_leaf(&TextData(ArrayString::from("world").unwrap()));

        let merged = TextAggregate::merge(&[agg1, agg2]);
        assert_eq!(merged.codeunits(), 11);
        assert_eq!(merged.prefix_summary.newlines, 1);
        assert_eq!(merged.prefix_summary.bytes_utf8_since_newline, 5); // "world"
    }
}

#[cfg(test)]
mod replace_tests {
    use super::*;

    #[test]
    fn test_split_at_simple() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello world");
        let (left, right) = rope.split_at(6);

        let left_str: String = left.iterator_utf8(0).collect();
        let right_str: String = right.iterator_utf8(0).collect();

        assert_eq!(left_str, "hello ");
        assert_eq!(right_str, "world");
    }

    #[test]
    fn test_split_at_boundaries() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello");

        let (left, right) = rope.split_at(0);
        assert_eq!(left.codeunits(), 0);
        assert_eq!(right.codeunits(), 5);

        let (left, right) = rope.split_at(5);
        assert_eq!(left.codeunits(), 5);
        assert_eq!(right.codeunits(), 0);
    }

    #[test]
    fn test_concat_simple() {
        let rope1 = Rope::<TextAggregate, 8>::from_str("hello");
        let rope2 = Rope::<TextAggregate, 8>::from_str(" world");

        let result = Rope::concat([rope1, rope2]);
        let result_str: String = result.iterator_utf8(0).collect();

        assert_eq!(result_str, "hello world");
    }

    #[test]
    fn test_replace_range_simple() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello world");
        let new_data = vec![TextData(ArrayString::from("beautiful ").unwrap())];

        let result = rope.replace_range(6..11, new_data);
        let result_str: String = result.iterator_utf8(0).collect();

        assert_eq!(result_str, "hello beautiful ");
    }

    #[test]
    fn test_replace_range_delete() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello world");
        let result = rope.replace_range(5..11, vec![]);
        let result_str: String = result.iterator_utf8(0).collect();

        assert_eq!(result_str, "hello");
    }

    #[test]
    fn test_replace_range_insert() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello world");
        let new_data = vec![TextData(ArrayString::from(" beautiful").unwrap())];

        let result = rope.replace_range(5..5, new_data);
        let result_str: String = result.iterator_utf8(0).collect();

        assert_eq!(result_str, "hello beautiful world");
    }

    #[test]
    fn test_replace_range_unicode() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello 🦀 world");
        let crab_pos = "hello ".len();
        let after_crab = crab_pos + "🦀".len();

        let new_data = vec![TextData(ArrayString::from("🎉").unwrap())];
        let result = rope.replace_range(crab_pos..after_crab, new_data);
        let result_str: String = result.iterator_utf8(0).collect();

        assert_eq!(result_str, "hello 🎉 world");
    }

    #[test]
    fn test_persistence() {
        let rope1 = Rope::<TextAggregate, 8>::from_str("hello world");
        let new_data = vec![TextData(ArrayString::from("REPLACED").unwrap())];

        let rope2 = rope1.replace_range(6..11, new_data);

        // Original rope should be unchanged
        let rope1_str: String = rope1.iterator_utf8(0).collect();
        let rope2_str: String = rope2.iterator_utf8(0).collect();

        assert_eq!(rope1_str, "hello world");
        assert_eq!(rope2_str, "hello REPLACED");
    }

    #[test]
    fn test_large_replacement() {
        let original = "a".repeat(200);
        let rope = Rope::<TextAggregate, 8>::from_str(&original);

        let replacement = "b".repeat(100);
        let leaves: Vec<_> = replacement
            .as_bytes()
            .chunks(MAX_LEAF_SIZE)
            .map(|chunk| {
                TextData(ArrayString::from(std::str::from_utf8(chunk).unwrap()).unwrap())
            })
            .collect();

        let result = rope.replace_range(50..150, leaves);
        let result_str: String = result.iterator_utf8(0).collect();

        let expected = format!("{}{}{}",
            "a".repeat(50),
            "b".repeat(100),
            "a".repeat(50)
        );

        assert_eq!(result_str, expected);
    }
}
