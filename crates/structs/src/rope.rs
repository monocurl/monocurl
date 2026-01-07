use arrayvec::{ArrayString, ArrayVec};
use std::ops::Range;
use std::{marker::PhantomData, sync::Arc};
use std::iter::Once;
use std::str::Chars;
use crate::rope::internal::RopeNode;
use crate::text::Span8;
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
    fn merge(children: impl DoubleEndedIterator<Item=Self> + Clone) -> Self;

    fn codeunits(&self) -> usize;
    fn height(&self) -> u32;
}

mod internal {
    use std::sync::Arc;

    use arrayvec::ArrayVec;

    use crate::{rope::{AggregateData, DEFAULT_CHILDREN}};

    pub(super) enum RopeNode<S: AggregateData, const N: usize = DEFAULT_CHILDREN> {
        Internal {
            agg: S,
            children: ArrayVec<Arc<RopeNode<S, N>>, N>,
        },
        Leaf {
            agg: S,
            data: S::LeafData,
        },
    }

    impl<S: AggregateData, const N: usize> RopeNode<S, N> {
        pub(super) fn new_leaf(data: S::LeafData) -> Self {
            let agg = S::from_leaf(&data);
            RopeNode::Leaf { agg, data }
        }

        pub(super) fn new_internal(children: ArrayVec<Arc<RopeNode<S, N>>, N>) -> Self {
            let agg = S::merge(children.iter().map(|c| c.aggregate().clone()));
            RopeNode::Internal { agg, children }
        }

        pub(super) fn aggregate(&self) -> &S {
            match self {
                RopeNode::Internal { agg, .. } => agg,
                RopeNode::Leaf { agg, .. } => agg,
            }
        }

        pub(super) fn is_empty(&self) -> bool {
            self.aggregate().codeunits() == 0
        }

        pub(super) fn children_slice(&self) -> &[Arc<RopeNode<S, N>>] {
            match self {
                RopeNode::Internal { children, .. } => &children,
                RopeNode::Leaf { .. } => &[],
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

    pub fn iterator(&self, pos: Count8) -> impl Iterator<Item=<<S::LeafData as LeafData>::Iterator<'_> as Iterator>::Item> {
        RopeIterator::<'_, S, N, true>::new(self, pos)
    }

    pub fn rev_iterator(&self, pos: Count8) -> impl Iterator<Item=<<S::LeafData as LeafData>::Iterator<'_> as Iterator>::Item> {
        RopeIterator::<'_, S, N, false>::new(self, pos)
    }

    pub fn codeunits(&self) -> usize {
        self.root.aggregate().codeunits()
    }
}

impl<const N: usize> Rope<TextAggregate, N> {
    pub fn iterator_range(&self, mut range: Span8) -> impl Iterator<Item=char> + use<'_, N> {
        self.iterator(range.start)
            .take_while(move |c| {
                range.start += c.len_utf8();
                range.start <= range.end
            })
    }
}

impl<T, const N: usize> Rope<RLEAggregate<T>, N> where T: PartialEq + Default + Clone
{
    pub fn iterator_range(&self, mut range: Span8) -> impl Iterator<Item=(Count8, T)> + use<'_, N, T> {
        self.iterator(range.start)
            .map_while(move |(c, item)| {
                if range.start >= range.end {
                    None
                }
                else {
                    let count = c.min(range.end - range.start);
                    range.start += count;
                    Some((count, item))
                }
            })
    }
}

impl<S: AggregateData, const N: usize> Rope<S, N> {
    fn check_invariants_rec(&self, _root: &Arc<RopeNode<S, N>>, ) {
        #[cfg(test)]
        match &**_root {
            RopeNode::Internal { children, .. } => {
                let min_children = N / 2;
                debug_assert!(children.len() >= min_children || Arc::ptr_eq(&self.root, _root));
                // all children same height
                let first_height = children[0].aggregate().height();
                for child in children.iter() {
                    debug_assert_eq!(child.aggregate().height(), first_height);
                    self.check_invariants_rec(child);
                }
            }
            RopeNode::Leaf { .. } => { }
        }
    }

    fn check_invariants(self) -> Self {
        self.check_invariants_rec(&self.root);
        self
    }

    fn from_children(mut nodes: Vec<Arc<RopeNode<S, N>>>) -> Self {
        if nodes.is_empty() {
            return Self::default();
        }
        if nodes.len() == 1 {
            return Rope { root: nodes[0].clone() };
        }

        // build tree layer-by-layer
        while nodes.len() > 1 {
            let mut next_level = Vec::new();

            let len = nodes.len();
            let num_blocks = len.div_ceil(N);
            for i in 0..num_blocks {
                let start = i * len / num_blocks;
                let end = (i + 1) * len / num_blocks;
                let chunk = &nodes[start..end];

                let children = ArrayVec::from_iter(chunk.iter().cloned());

                let internal = Arc::new(RopeNode::new_internal(children));
                next_level.push(internal);
            }

            nodes = next_level;
        }

        Rope { root: nodes[0].clone() }.check_invariants()
    }

    #[must_use]
    pub fn replace_range(&self, range: Range<usize>, new_data: impl Into<Vec<S::LeafData>>) -> Self {
        let leaves = new_data
            .into()
            .into_iter().map(|ld| Arc::new(RopeNode::new_leaf(ld)))
            .collect();
        let replacement = Self::from_children(leaves);

        let (left, right) = self.split_at(range.start);
        let (_deleted, right) = right.split_at(range.end - range.start);
        let left_and_replacement = Self::join(left, replacement);

        Self::join(left_and_replacement, right).check_invariants()
    }

    pub fn join(left: Self, right: Self) -> Self {
        // join in a such a way that all leaves are still at the same level
        if left.root.is_empty() {
            return right;
        }
        if right.root.is_empty() {
            return left;
        }

        let (lp, rp) = Self::join_subtree(&left.root, &right.root);
        match (lp, rp) {
            (None, None) => unreachable!(),
            (None, Some(rp)) => Rope { root: rp },
            (Some(lp), None) => Rope { root: lp },
            (Some(lp), Some(rp)) => Rope { root: Arc::new(RopeNode::new_internal(ArrayVec::from_iter([lp, rp].into_iter())) ) }
        }.check_invariants()
    }

    fn group_children_into_two_parents(count: usize, children_iterator: impl Iterator<Item=Arc<RopeNode<S, N>>>) -> (Arc<RopeNode<S, N>>, Arc<RopeNode<S, N>>) {
        debug_assert!(count >= N && count <= 2 * N);
        let split = count / 2;
        let mut left_children = ArrayVec::new();
        let mut right_children = ArrayVec::new();
        for (i, child) in children_iterator.enumerate() {
            if i < split {
                left_children.push(child);
            } else {
                right_children.push(child);
            }
        }

        (
            Arc::new(RopeNode::new_internal(left_children)),
            Arc::new(RopeNode::new_internal(right_children))
        )
    }

    // returns either a single one, or two that cannot be combined
    // any returned value has the same height as the maximum of the two
    fn join_equal_heights(left: &Arc<RopeNode<S, N>>, right: &Arc<RopeNode<S, N>>) -> (Option<Arc<RopeNode<S, N>>>, Option<Arc<RopeNode<S, N>>>) {
        match (&**left, &**right) {
            (RopeNode::Internal { children: left_children, .. } ,
                RopeNode::Internal { children: right_children, .. })  =>
            {
                if left_children.len() + right_children.len() <= N {
                    let mut children = ArrayVec::new();
                    children.extend(left_children.iter().cloned());
                    children.extend(right_children.iter().cloned());

                    (Some(Arc::new(RopeNode::new_internal(children))), None)
                }
                else {
                    // it may be the case that the left is super full and right is unfilled
                    // (or vice versa). In this case, we want to donate some nodes from left to right
                    let (lp, rp) = Self::group_children_into_two_parents(
                        left_children.len() + right_children.len(),
                        left_children.iter().cloned().chain(right_children.iter().cloned())
                    );

                    (Some(lp), Some(rp))
                }
            },
            (RopeNode::Leaf { data: left_data, .. }, RopeNode::Leaf { data: right_data, .. }) => {
                let mut combined_data = left_data.clone();
                if let Some(remaining) = combined_data.try_append(right_data.clone()) {
                    let left_leaf = Arc::new(RopeNode::new_leaf(combined_data));
                    let right_leaf = Arc::new(RopeNode::new_leaf(remaining));
                    return (Some(left_leaf), Some(right_leaf))
                } else {
                    return (Some(Arc::new(RopeNode::new_leaf(combined_data))), None);
                }
            }
            _ => { unreachable!("heights improperly calculated"); }
        }
    }

    fn join_subtree(left: &Arc<RopeNode<S, N>>, right: &Arc<RopeNode<S, N>>) -> (Option<Arc<RopeNode<S, N>>>, Option<Arc<RopeNode<S, N>>>) {
        if left.is_empty() {
            return (None, Some(right.clone()));
        }

        if right.is_empty() {
            return (Some(left.clone()), None);
        }

        let lh = left.aggregate().height();
        let rh = right.aggregate().height();
        if lh == rh {
            Self::join_equal_heights(left, right)
        }
        else if lh > rh {
            // descend left
            let (left_child, left_overflow) = Self::join_subtree(
                &left.children_slice()[left.children_slice().len() - 1],
                right,
            );

            let total_c = left.children_slice().len() + if left_overflow.is_some() { 1 } else { 0 };
            let new_children_iterator = left.children_slice()[0..left.children_slice().len() - 1]
                .iter()
                .cloned()
                .chain(left_child.into_iter())
                .chain(left_overflow.into_iter());

            if total_c <= N {
                // if we have space for two, add them both
                let children = ArrayVec::from_iter(
                    new_children_iterator
                );

                (Some(Arc::new(RopeNode::new_internal(children))), None)
            }
            else {
                // otherwise, must split ourselves into two
                let (lp, rp) = Self::group_children_into_two_parents(
                    total_c,
                    new_children_iterator
                );

                (Some(lp), Some(rp))
            }
        }
        else {
            // descend right
            // note that in this case, the right overflow should be the FIRST child
            let (right_overflow, right_child) = Self::join_subtree(
                left,
                &right.children_slice()[0],
            );
            let total_c = right.children_slice().len() + if right_overflow.is_some() { 1 } else { 0 };
            let new_children_iterator = right_overflow.into_iter()
                .chain(right_child.into_iter())
                .chain(right.children_slice()[1..].iter().cloned());

            if total_c <= N {
                let children = ArrayVec::from_iter(new_children_iterator);

                (None, Some(Arc::new(RopeNode::new_internal(children))))
            }
            else {
                // split ourselves into two
                let (lp, rp) = Self::group_children_into_two_parents(
                    total_c,
                    new_children_iterator
                );

                (Some(lp), Some(rp))
            }
        }
    }

    pub fn split_at(&self, pos: usize) -> (Self, Self) {
        if pos == 0 {
            return (Self::default(), self.clone());
        }
        if pos >= self.codeunits() {
            return (self.clone(), Self::default());
        }

        let (left, right) = self.split_node(&self.root, pos);
        (Rope { root: left }.check_invariants(), Rope { root: right }.check_invariants())
    }

    // helper method for splitting
    // cut_child follows the properties specified in split_node
    fn balanced_merge(siblings: &[Arc<RopeNode<S, N>>], cut_child: Arc<RopeNode<S, N>>, merge_front: bool) -> Arc<RopeNode<S, N>> {
        if siblings.is_empty() {
            return cut_child.clone();
        }

        let new_children = if merge_front {
            let (lp, rp) = Self::join_subtree(&cut_child, &siblings[0]);

            ArrayVec::from_iter(
                lp.into_iter()
                    .chain(rp.into_iter())
                    .chain(siblings[1..].iter().cloned())
            )
        } else {
            let (lp, rp) = Self::join_subtree(&siblings[siblings.len() - 1], &cut_child);

            ArrayVec::from_iter(
                siblings[..(siblings.len() - 1)].iter().cloned()
                    .chain(lp.into_iter())
                    .chain(rp.into_iter())
            )
        };

        Arc::new(RopeNode::new_internal(new_children))
    }

    // returns a tree that maintains all invariants
    // except that each returned root node (XOR)
    // a. have less height than it originally did
    // b. have less than minimum children
    // c. have proper height and children
    fn split_node(
        &self,
        node: &Arc<RopeNode<S, N>>,
        pos: usize,
    ) -> (Arc<RopeNode<S, N>>, Arc<RopeNode<S, N>>) {
        match &**node {
            RopeNode::Leaf { data, .. } => {
                // split the leaf data
                let left_data = data.subrange(0..pos);
                let right_data = data.subrange(pos..data.codeunits());

                let left_leaf = RopeNode::new_leaf(left_data);

                let right_leaf = RopeNode::new_leaf(right_data);

                (
                    Arc::new(left_leaf),
                    Arc::new(right_leaf),
                )
            }
            RopeNode::Internal { children, .. } => {
                let mut utf8 = 0;
                for (i, child) in children.iter().enumerate() {
                    let child_size = child.aggregate().codeunits();

                    if pos < utf8 + child_size {
                        let local_pos = pos - utf8;

                        let (left_child, right_child) = self.split_node(child, local_pos);

                        let left = Self::balanced_merge(&children[..i], left_child, false);
                        let right = Self::balanced_merge(&children[(i + 1)..], right_child, true);

                        return (left, right);
                    }

                    utf8 += child_size;
                }

                unreachable!("invariants failed")
            }
        }
    }
}

impl<S: AggregateData, const N: usize> Rope<S, N> {
    /// finds the first leaf, such that the prefix aggregate *including* that leaf does not satisfy the (monotonically decreasing) predicate
    pub fn find_leaf(&self, mut lt: impl FnMut(&S) -> bool) -> &S::LeafData {
        let mut agg = S::from_leaf(&S::LeafData::identity());
        self.find_leaf_rec(&mut agg, &self.root, &mut lt)
    }

     fn find_leaf_rec<'a>(
        &'a self,
        agg: &mut S,
        node: &'a Arc<RopeNode<S, N>>,
        lt: &mut impl FnMut(&S) -> bool,
     ) -> &'a S::LeafData {
        match &**node {
            RopeNode::Leaf { data, .. } => {
                data
            }
            RopeNode::Internal { children, .. } => {
                for child in children.iter() {
                    let combined = S::merge([agg.clone(), child.aggregate().clone()].into_iter());
                    if lt(&combined) {
                        *agg = combined;
                    } else {
                        return self.find_leaf_rec(agg, child, lt);
                    }
                }

                panic!("Predicate should have failed before exhausting all children")
            }
        }
     }

    // Find the last leaf where the predicate is true
    // le returns true to fully consume over the current aggregate
    // walk_leaf is called with the prefix aggregate when we find the target leaf
    // NOTE returned value of height is unspecified
    pub fn walk(&self, mut le: impl FnMut(&S) -> bool, walk_leaf: impl FnOnce(&mut S, &S::LeafData)) -> S {
        let mut agg = S::from_leaf(&S::LeafData::identity());
        self.walk_rec(&mut agg, &self.root, &mut le, walk_leaf);
        agg
    }

    fn walk_rec(
        &self,
        agg: &mut S,
        node: &Arc<RopeNode<S, N>>,
        le: &mut impl FnMut(&S) -> bool,
        walk_leaf: impl FnOnce(&mut S, &S::LeafData)
    ) {
        match &**node {
            RopeNode::Leaf { data, .. } => {
                walk_leaf(agg, data);
            }
            RopeNode::Internal { children, .. } => {
                for child in children.iter() {
                    let combined = S::merge([agg.clone(), child.aggregate().clone()].into_iter());
                    if le(&combined) {
                        *agg = combined;
                    } else {
                        self.walk_rec(agg, child, le, walk_leaf);
                        return;
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

                    // overlapping range
                    let start_in_child = local_range.start.saturating_sub(utf8);
                    let end_in_child = (local_range.end - utf8).min(child_size);

                    let child_agg = self.subrange_aggregate_rec(child, start_in_child..end_in_child);
                    collected_aggs.push(child_agg);

                    utf8 += child_size;
                }

                S::merge(collected_aggs.into_iter())
            }
        }
    }
}

// if out of bounds, returns 0 or len
impl<const N: usize> Rope<TextAggregate, N> {
    fn walk_chars_until(
        agg: &mut TextAggregate,
        chars: impl Iterator<Item = char>,
        mut should_take: impl FnMut(&TextAggregate) -> bool,
    ) {
        for c in chars {
            let char_utf8_len = c.len_utf8();
            let char_utf16_len = c.len_utf16();

            let new_summary = TextPrefixSummary {
                bytes_utf8: agg.prefix_summary.bytes_utf8 + char_utf8_len,
                codeunits_utf16: agg.prefix_summary.codeunits_utf16 + char_utf16_len,
                newlines: agg.prefix_summary.newlines + if c == '\n' { 1 } else { 0 },
                bytes_utf8_since_newline: if c == '\n' {
                    0
                } else {
                    agg.prefix_summary.bytes_utf8_since_newline + char_utf8_len
                },
            };

            let tentative = TextAggregate {
                prefix_summary: new_summary,
                height: 0,
            };

            if !should_take(&tentative) {
                break;
            }

            agg.prefix_summary = new_summary;
        }
    }

    fn text_walk(&self, le: impl FnMut(&TextAggregate) -> bool + Clone) -> TextPrefixSummary {
        self.walk(le.clone(), |agg, leaf_data| {
            Self::walk_chars_until(
                agg,
                leaf_data.0.chars(),
                le.clone()
            );
        }).prefix_summary
    }

    pub fn utf8_prefix_summary(&self, at: Count8) -> TextPrefixSummary {
        self.text_walk(|agg| agg.prefix_summary.bytes_utf8 <= at)
    }

    pub fn utf16_prefix_summary(&self, at: Count16) -> TextPrefixSummary {
        self.text_walk(|agg| agg.prefix_summary.codeunits_utf16 <= at)
    }

    pub fn utf8_line_pos_prefix(&self, row: usize, col: Count8) -> TextPrefixSummary {
        self.text_walk(|agg| {
            let s = &agg.prefix_summary;
            (s.newlines, s.bytes_utf8_since_newline) <= (row, col)
        })
    }
}

impl<T> Rope<RLEAggregate<T>> where T: PartialEq + Default + Clone
{
    pub fn attribute_at(&self, index: usize) -> &T {
        &self.find_leaf(|agg| agg.codeunits() <= index).attribute
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
    height: usize,
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
            height: 0,
        }
    }

    fn merge(children: impl DoubleEndedIterator<Item=Self> + Clone) -> Self {
        let bytes_utf8 = children.clone().map(|c| c.prefix_summary.bytes_utf8).sum();
        let bytes_utf16 = children.clone().map(|c| c.prefix_summary.codeunits_utf16).sum();
        let newlines = children.clone().map(|c| c.prefix_summary.newlines).sum();


        let mut bytes_utf8_since_newline = 0;
        for child in children.clone().rev() {
            if child.prefix_summary.newlines > 0 {
                bytes_utf8_since_newline += child.prefix_summary.bytes_utf8_since_newline;
                break;
            } else {
                bytes_utf8_since_newline += child.prefix_summary.bytes_utf8;
            }
        }

        let depth = children.clone().map(|c| c.height).max().unwrap_or(0) + 1;

        let prefix_summary = TextPrefixSummary {
            bytes_utf8,
            codeunits_utf16: bytes_utf16,
            newlines,
            bytes_utf8_since_newline,
        };

        TextAggregate {
            prefix_summary,
            height: depth,
        }
    }

    fn codeunits(&self) -> Count8 {
        self.prefix_summary.bytes_utf8
    }

    fn height(&self) -> u32 {
        self.height as u32
    }
}

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

impl<const N: usize> Rope<TextAggregate, N> {
    pub fn from_str(s: &str) -> Self {
        Self::default().replace_range(0..0, leaves_from_str(s))
    }
}

#[derive(Clone)]
pub struct RLEAggregate<T> {
    pub bytes_utf8: usize,
    pub height: usize,
    phantom_t: PhantomData<T>
}

#[derive(Clone)]
pub struct RLEData<T> {
    pub bytes_utf8: usize,
    pub attribute: T,
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
            height: 0,
            phantom_t: PhantomData,
        }
    }

    fn merge(children: impl DoubleEndedIterator<Item=Self> + Clone) -> Self {
        let bytes_utf8 = children.clone().map(|c| c.bytes_utf8).sum();
        let height = children.clone().map(|c| c.height).max().unwrap_or(0) + 1;

        RLEAggregate {
            bytes_utf8,
            height,
            phantom_t: PhantomData,
        }
    }

    fn codeunits(&self) -> Count8 {
        self.bytes_utf8
    }

    fn height(&self) -> u32 {
        self.height as u32
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
        let chars: Vec<char> = rope.iterator(0).collect();
        assert_eq!(chars, vec![]);
    }

    #[test]
    fn test_single_node_forward_iterator() {
        let rope = rope_from_str("hello");
        let chars: String = rope.iterator(0).collect();
        assert_eq!(chars, "hello");
    }

    #[test]
    fn test_single_node_reverse_iterator() {
        let rope = rope_from_str("hello");
        let chars: String = rope.rev_iterator(5).collect();
        assert_eq!(chars, "olleh");
    }

    #[test]
    fn test_forward_iterator_from_middle() {
        let rope = rope_from_str("hello world");
        let chars: String = rope.iterator(6).collect();
        assert_eq!(chars, "world");
    }

    #[test]
    fn test_reverse_iterator_from_middle() {
        let rope = rope_from_str("hello world");
        let chars: String = rope.rev_iterator(5).collect();
        assert_eq!(chars, "olleh");
    }

    #[test]
    fn test_unicode_forward_iterator() {
        let rope = rope_from_str("hello 🦀 world");
        let chars: String = rope.iterator(0).collect();
        assert_eq!(chars, "hello 🦀 world");
    }

    #[test]
    fn test_unicode_reverse_iterator() {
        let rope = rope_from_str("hello 🦀 world");
        let count = rope.codeunits();
        let chars: String = rope.rev_iterator(count).collect();
        assert_eq!(chars, "dlrow 🦀 olleh");
    }

    #[test]
    fn test_iterator_from_end() {
        let rope = rope_from_str("hello");
        let chars: Vec<char> = rope.iterator(5).collect();
        assert_eq!(chars, vec![]);
    }

    #[test]
    fn test_iterator_from_start_reverse() {
        let rope = rope_from_str("hello");
        let chars: Vec<char> = rope.rev_iterator(0).collect();
        assert_eq!(chars, vec![]);
    }

    #[test]
    fn test_multi_node_forward_iterator() {
        // Create rope with multiple nodes (>64 chars per node)
        let text = "a".repeat(100);
        let rope = rope_from_str(&text);
        let chars: String = rope.iterator(0).collect();
        assert_eq!(chars, text);
    }

    #[test]
    fn test_multi_node_forward_iterator2() {
        let text = "a".repeat(1000) + &"b".repeat(150) + &"c".repeat(1);
        let rope = rope_from_str(&text);
        let cands = [0, 500, 512, 1000, 1150, 1151];
        for &cand in &cands {
            let chars: String = rope.iterator(cand).collect();
            let expected = text[cand..].to_string();
            assert_eq!(chars, expected);
        }
    }

    #[test]
    fn test_multi_node_reverse_iterator() {
        let text = "a".repeat(100);
        let rope = rope_from_str(&text);
        let count = rope.codeunits();
        let chars: String = rope.rev_iterator(count).collect();
        assert_eq!(chars, text);
    }

    #[test]
    fn test_multi_node_reverse_iterator2() {
        let text = "a".repeat(1000) + &"b".repeat(150) + &"c".repeat(1);
        let rope = rope_from_str(&text);
        let cands = [0, 500, 512, 1000, 1150, 1151];
        for &cand in &cands {
            let chars: String = rope.rev_iterator(cand).collect();
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
        let rope = rope_from_str("hello\nwrld");
        let summary = rope.utf8_line_pos_prefix(0, 100); // end of "hello"
        assert_eq!(summary.bytes_utf8, 5);
        assert_eq!(summary.newlines, 0);

        let summary = rope.utf8_line_pos_prefix(0, 5); // end of "hello"
        assert_eq!(summary.bytes_utf8, 5);
        assert_eq!(summary.newlines, 0);

        let summary = rope.utf8_line_pos_prefix(0, 4);
        assert_eq!(summary.bytes_utf8, 4);
        assert_eq!(summary.newlines, 0);

        let summary = rope.utf8_line_pos_prefix(1, 100);
        assert_eq!(summary.bytes_utf8, 10);
        assert_eq!(summary.newlines, 1);

        let summary = rope.utf8_line_pos_prefix(1, 0);
        assert_eq!(summary.bytes_utf8, 6);
        assert_eq!(summary.newlines, 1);

        let summary = rope.utf8_line_pos_prefix(1, 4);
        assert_eq!(summary.bytes_utf8, 10);
        assert_eq!(summary.newlines, 1);
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

        let merged = TextAggregate::merge([agg1, agg2].into_iter());
        assert_eq!(merged.codeunits(), 11);
        assert_eq!(merged.prefix_summary.newlines, 1);
        assert_eq!(merged.prefix_summary.bytes_utf8_since_newline, 5); // "world"
    }
}

#[cfg(test)]
mod replace_tests {
    use super::*;


    fn assert_invariants<S: AggregateData, const N: usize>(rope: &Rope<S, N>) {
        rope.clone().check_invariants();
    }

    fn collect_string(rope: &Rope<TextAggregate, 8>) -> String {
        rope.iterator(0).collect()
    }

    #[test]
    fn test_split_at_simple() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello world");
        let (left, right) = rope.split_at(6);

        let left_str: String = left.iterator(0).collect();
        let right_str: String = right.iterator(0).collect();

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
    fn test_join_simple() {
        let rope1 = Rope::<TextAggregate, 8>::from_str("hello");
        let rope2 = Rope::<TextAggregate, 8>::from_str(" world");

        let result = Rope::join(rope1, rope2);
        let result_str: String = result.iterator(0).collect();

        assert_eq!(result_str, "hello world");
    }

    #[test]
    fn test_replace_range_simple() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello world");
        let new_data = vec![TextData(ArrayString::from("beautiful ").unwrap())];

        let result = rope.replace_range(6..11, new_data);
        let result_str: String = result.iterator(0).collect();

        assert_eq!(result_str, "hello beautiful ");
    }

    #[test]
    fn test_replace_range_delete() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello world");
        let result = rope.replace_range(5..11, vec![]);
        let result_str: String = result.iterator(0).collect();

        assert_eq!(result_str, "hello");
    }

    #[test]
    fn test_replace_range_insert() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello world");
        let new_data = vec![TextData(ArrayString::from(" beautiful").unwrap())];

        let result = rope.replace_range(5..5, new_data);
        let result_str: String = result.iterator(0).collect();

        assert_eq!(result_str, "hello beautiful world");
    }

    #[test]
    fn test_replace_range_unicode() {
        let rope = Rope::<TextAggregate, 8>::from_str("hello 🦀 world");
        let crab_pos = "hello ".len();
        let after_crab = crab_pos + "🦀".len();

        let new_data = vec![TextData(ArrayString::from("🎉").unwrap())];
        let result = rope.replace_range(crab_pos..after_crab, new_data);
        let result_str: String = result.iterator(0).collect();

        assert_eq!(result_str, "hello 🎉 world");
    }

    #[test]
    fn test_persistence() {
        let rope1 = Rope::<TextAggregate, 8>::from_str("hello world");
        let new_data = vec![TextData(ArrayString::from("REPLACED").unwrap())];

        let rope2 = rope1.replace_range(6..11, new_data);

        // Original rope should be unchanged
        let rope1_str: String = rope1.iterator(0).collect();
        let rope2_str: String = rope2.iterator(0).collect();

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
        let result_str: String = result.iterator(0).collect();

        let expected = format!("{}{}{}",
            "a".repeat(50),
            "b".repeat(100),
            "a".repeat(50)
        );

        assert_eq!(result_str, expected);
    }

    #[test]
    fn split_at_every_position_small() {
        let text = "abcdefghijklmnopqrstuvwxyz";
        let rope = Rope::<TextAggregate, 8>::from_str(text);

        for i in 0..=text.len() {
            let (l, r) = rope.split_at(i);
            assert_eq!(
                collect_string(&l) + &collect_string(&r),
                text
            );
            assert_invariants(&l);
            assert_invariants(&r);
        }
    }

    #[test]
    fn split_at_every_position_large() {
        let text = "a".repeat(2000);
        let rope = Rope::<TextAggregate, 8>::from_str(&text);

        for i in (0..=text.len()).step_by(17) {
            let (l, r) = rope.split_at(i);
            assert_eq!(
                collect_string(&l) + &collect_string(&r),
                text
            );
            assert_invariants(&l);
            assert_invariants(&r);
        }
    }

    #[test]
    fn repeated_split_cascade() {
        let mut rope = Rope::<TextAggregate, 8>::from_str(&"x".repeat(1500));

        for _ in 0..20 {
            let mid = rope.codeunits() / 2;
            let (l, r) = rope.split_at(mid);
            assert_invariants(&l);
            assert_invariants(&r);
            rope = Rope::join(l, r);
            assert_invariants(&rope);
        }
    }

    #[test]
    fn join_different_heights() {
        let small = Rope::<TextAggregate, 8>::from_str("small");
        let large = Rope::<TextAggregate, 8>::from_str(&"L".repeat(2000));

        let joined1 = Rope::join(small.clone(), large.clone());
        let joined2 = Rope::join(large, small);

        assert_eq!(
            collect_string(&joined1),
            "small".to_string() + &"L".repeat(2000)
        );
        assert_eq!(
            collect_string(&joined2),
            "L".repeat(2000) + "small"
        );

        assert_invariants(&joined1);
        assert_invariants(&joined2);
    }

    #[test]
    fn join_many_small_roots() {
        let mut rope = Rope::<TextAggregate, 8>::default();

        for _ in 0..100 {
            let leaf = Rope::<TextAggregate, 8>::from_str("abc");
            rope = Rope::join(rope, leaf);
            assert_invariants(&rope);
        }

        assert_eq!(collect_string(&rope), "abc".repeat(100));
    }

    #[test]
    fn join_after_many_splits() {
        let rope = Rope::<TextAggregate, 8>::from_str(&"x".repeat(1000));
        let mut pieces = Vec::new();

        let mut current = rope;
        for _ in 0..10 {
            let (l, r) = current.split_at(current.codeunits() / 2);
            pieces.push(l);
            current = r;
        }
        pieces.push(current);

        let rebuilt = pieces.into_iter().reduce(Rope::join).unwrap();
        assert_eq!(collect_string(&rebuilt), "x".repeat(1000));
        assert_invariants(&rebuilt);
    }

    #[test]
    fn delete_entire_rope_incrementally() {
        let mut rope = Rope::<TextAggregate, 8>::from_str(&"a".repeat(1024));

        while rope.codeunits() > 0 {
            let len = rope.codeunits();
            rope = rope.replace_range(0..(len / 2).max(1), vec![]);
            assert_invariants(&rope);
        }

        assert_eq!(rope.codeunits(), 0);
    }
    #[test]
    fn alternating_delete_ranges() {
        let mut rope = Rope::<TextAggregate, 8>::from_str(&"0123456789".repeat(100));

        for i in 0..50 {
            let start = (i * 10) % rope.codeunits();
            let end = (start + 5).min(rope.codeunits());
            rope = rope.replace_range(start..end, vec![]);
            assert_invariants(&rope);
        }
    }

    #[test]
    fn split_join_roundtrip_identity() {
        let text = "roundtrip test ".repeat(100);
        let rope = Rope::<TextAggregate, 8>::from_str(&text);

        for i in (0..text.len()).step_by(13) {
            let (l, r) = rope.split_at(i);
            let rebuilt = Rope::join(l, r);
            assert_eq!(collect_string(&rebuilt), text);
            assert_invariants(&rebuilt);
        }
    }

    #[test]
    fn height_shrinks_after_mass_deletion() {
        let rope = Rope::<TextAggregate, 8>::from_str(&"x".repeat(3000));
        let h1 = rope.root.aggregate().height();

        let rope2 = rope.replace_range(0..2900, vec![]);
        let h2 = rope2.root.aggregate().height();

        assert!(h2 <= h1);
        assert_invariants(&rope2);
    }

    #[test]
    fn deterministic_fuzz_split_join() {
        let mut rope = Rope::<TextAggregate, 8>::from_str(&"abcdefghijklmnopqrstuvwxyz".repeat(50));

        for i in 0..100 {
            let pos = (i * 37) % rope.codeunits();
            let (l, r) = rope.split_at(pos);
            rope = Rope::join(l, r);
            assert_invariants(&rope);
        }
    }

}
