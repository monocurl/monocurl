//! batch arguments handed over as numbers instead of heap values: the kernel
//! machines read them as they are, and heap values are built only for the
//! calls the interpreter runs

use std::{ops::Deref, sync::Arc};

use geo::simd::Float3;
use smallvec::SmallVec;

use crate::{
    heap::VRc,
    value::{Value, container::List},
};

use super::value::KVal;

/// the arguments of every call of a batch, as plain numbers
#[derive(Clone, Copy)]
pub enum BatchInput<'a> {
    /// one argument per call, the list `[x, y, z]`. a whole component is an
    /// int, as the stdlib's `point_value` makes it
    Points(&'a [Float3]),
    /// a point, as in `Points`, and its grid index `[ix, iy]` per call
    IndexedPoints(&'a [(Float3, [usize; 2])]),
    /// `arity` float arguments per call, laid flat
    Floats { values: &'a [f64], arity: usize },
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum Num {
    Int(i64),
    Float(f64),
}

impl Num {
    /// a coordinate as `point_value` makes it
    fn coordinate(value: f32) -> Self {
        let value = value as f64;
        if value.fract() == 0.0 {
            Num::Int(value as i64)
        } else {
            Num::Float(value)
        }
    }

    pub(crate) fn kval(self) -> KVal {
        match self {
            Num::Int(n) => KVal::Int(n),
            Num::Float(f) => KVal::Float(f),
        }
    }

    fn value(self) -> Value {
        match self {
            Num::Int(n) => Value::Integer(n),
            Num::Float(f) => Value::Float(f),
        }
    }
}

/// one argument of one call
#[derive(Clone, Copy)]
pub(crate) enum NumArg<'a> {
    Num(Num),
    List(&'a [Num]),
}

impl NumArg<'_> {
    pub(crate) fn kval(self) -> KVal {
        match self {
            NumArg::Num(num) => num.kval(),
            NumArg::List(nums) => KVal::list(nums.iter().map(|num| num.kval())),
        }
    }

    fn value(self) -> Value {
        match self {
            NumArg::Num(num) => num.value(),
            NumArg::List(nums) => {
                Value::List(List::new_with(nums.iter().map(|num| VRc::new(num.value()))))
            }
        }
    }
}

/// how one argument of every call is laid out
#[derive(Clone, Copy)]
enum Shape {
    Num,
    List(usize),
}

impl Shape {
    fn width(self) -> usize {
        match self {
            Shape::Num => 1,
            Shape::List(len) => len,
        }
    }
}

/// a `BatchInput` owned, so the kernel tier's workers can share it: every
/// call's numbers laid flat with a fixed stride
pub(crate) struct NumArgs {
    shape: SmallVec<[Shape; 2]>,
    stride: usize,
    nums: Vec<Num>,
}

impl NumArgs {
    pub(crate) fn new(input: BatchInput<'_>) -> Self {
        let (shape, nums): (SmallVec<[Shape; 2]>, Vec<Num>) = match input {
            BatchInput::Points(points) => (
                smallvec::smallvec![Shape::List(3)],
                points
                    .iter()
                    .flat_map(|point| point.to_array().map(Num::coordinate))
                    .collect(),
            ),
            BatchInput::IndexedPoints(points) => (
                smallvec::smallvec![Shape::List(3), Shape::List(2)],
                points
                    .iter()
                    .flat_map(|(point, index)| {
                        let [x, y, z] = point.to_array().map(Num::coordinate);
                        let [ix, iy] = index.map(|i| Num::Int(i as i64));
                        [x, y, z, ix, iy]
                    })
                    .collect(),
            ),
            BatchInput::Floats { values, arity } => (
                (0..arity).map(|_| Shape::Num).collect(),
                values.iter().copied().map(Num::Float).collect(),
            ),
        };
        let stride = shape.iter().map(|shape| shape.width()).sum();
        Self {
            shape,
            stride,
            nums,
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.nums.len().checked_div(self.stride).unwrap_or(0)
    }

    pub(crate) fn arity(&self) -> usize {
        self.shape.len()
    }

    /// the arguments of call `index`
    pub(crate) fn call(&self, index: usize) -> impl Iterator<Item = NumArg<'_>> {
        let mut nums = &self.nums[index * self.stride..(index + 1) * self.stride];
        self.shape.iter().map(move |shape| {
            let (arg, rest) = nums.split_at(shape.width());
            nums = rest;
            match shape {
                Shape::Num => NumArg::Num(arg[0]),
                Shape::List(_) => NumArg::List(arg),
            }
        })
    }
}

/// the calls of a batch as the caller handed them
pub(crate) enum Calls<'a, A> {
    Values(&'a [A]),
    Nums(Arc<NumArgs>),
}

impl<A: AsRef<[Value]>> Calls<'_, A> {
    pub(crate) fn len(&self) -> usize {
        match self {
            Calls::Values(args) => args.len(),
            Calls::Nums(nums) => nums.len(),
        }
    }

    /// the heap values of call `index`, built afresh for typed input
    pub(crate) fn values(&self, index: usize) -> CallValues<'_> {
        match self {
            Calls::Values(args) => CallValues::Borrowed(args[index].as_ref()),
            Calls::Nums(nums) => CallValues::Built(nums.call(index).map(NumArg::value).collect()),
        }
    }
}

pub(crate) enum CallValues<'a> {
    Borrowed(&'a [Value]),
    Built(SmallVec<[Value; 4]>),
}

impl Deref for CallValues<'_> {
    type Target = [Value];

    fn deref(&self) -> &[Value] {
        match self {
            CallValues::Borrowed(values) => values,
            CallValues::Built(values) => values,
        }
    }
}
