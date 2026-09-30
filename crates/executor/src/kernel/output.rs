//! batch results handed back as numbers instead of kernel values: native code
//! writes a returned list of the width the caller asked for straight into its
//! numbers, so a sampled vertex or pixel costs no list allocation

use geo::simd::{Float2, Float3, Float4};

use super::value::KVal;

/// the widest result read as numbers
pub const FLAT_MAX: usize = 4;

/// a result of `WIDTH` numbers, as the stdlib's samplers reduce a returned
/// `[x, y, ...]`: every element an int or a float, narrowed to `f32`
pub trait FlatOutput: Sized {
    const WIDTH: usize;

    /// from the first `WIDTH` numbers
    fn from_flat(values: [f32; FLAT_MAX]) -> Self;

    fn from_kernel(value: &KVal) -> Option<Self> {
        flat_of(value, Self::WIDTH).map(Self::from_flat)
    }
}

impl FlatOutput for Float2 {
    const WIDTH: usize = 2;

    fn from_flat([x, y, ..]: [f32; FLAT_MAX]) -> Self {
        Float2::from_array([x, y])
    }
}

impl FlatOutput for Float3 {
    const WIDTH: usize = 3;

    fn from_flat([x, y, z, _]: [f32; FLAT_MAX]) -> Self {
        Float3::from_array([x, y, z])
    }
}

impl FlatOutput for Float4 {
    const WIDTH: usize = 4;

    fn from_flat(values: [f32; FLAT_MAX]) -> Self {
        Float4::from_array(values)
    }
}

/// a number as the samplers narrow it
pub(crate) fn narrow(value: &KVal) -> Option<f32> {
    match *value {
        KVal::Int(n) => Some(n as f32),
        KVal::Float(f) => Some(f as f32),
        _ => None,
    }
}

/// `value` as a list of `width` numbers
pub(crate) fn flat_of(value: &KVal, width: usize) -> Option<[f32; FLAT_MAX]> {
    let KVal::List(list) = value else { return None };
    if list.len() != width || width > FLAT_MAX {
        return None;
    }
    let mut out = [0.0; FLAT_MAX];
    for (slot, element) in out.iter_mut().zip(list.iter()) {
        *slot = narrow(element)?;
    }
    Some(out)
}

/// the result of one call of a batch
#[derive(Debug)]
pub(crate) enum Output {
    /// the numbers of a list of the width the caller asked for
    Flat([f32; FLAT_MAX]),
    Value(KVal),
}
