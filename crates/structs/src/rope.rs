use arrayvec::ArrayString;
use std::sync::Arc;

use crate::text::Count8;

// Partially inspired by: https://zed.dev/blog/zed-decoded-rope-sumtree
//
pub const MAX_LEAF_SIZE: usize = 64;

pub trait LeafData: Sized {
    fn split(&self, at: Count8) -> (Self, Self);
    fn try_append(&mut self, from: Self) -> Option<Self>;

    fn chars(&self) -> Count8;
}

pub trait AggregateData: Sized {
    type LeafData: LeafData;

    fn from_leaf(data: Self::LeafData) -> Self;
    fn merge(lhs: &Self, rhs: &Self) -> Self;

    fn depth(&self) -> u32;
    fn weight(&self) -> usize;
}

// weight is calculated with respect to utf8
enum RopeNode<S: AggregateData> {
    Internal {
        agg: S,
        lhs: Arc<RopeNode<S>>,
        rhs: Arc<RopeNode<S>>,
    },
    Leaf {
        data: S::LeafData,
        agg: S,
    },
}

pub struct Rope<S: AggregateData> {
    root: Arc<RopeNode<S>>,
}

struct RopeIterator {}

impl<S: AggregateData> Rope<S> {
    pub fn replace_range() {}

    pub fn read_at() {}

    pub fn content_from() {}

    pub fn len() {}
}

pub struct Agg {
    chars_utf8: usize,
    chars_utf16: usize,
    newlines: usize,
    depth: usize,
    nodes: usize,
}

pub struct TextData(pub ArrayString<MAX_LEAF_SIZE>);

// RLE
pub struct RLEAgg {
    chars_utf8: usize,
    depth: usize,
    nodes: usize,
}

pub struct RLEData<T> {
    chars_utf8: usize,
    attribute: T,
}
