//! `rust_scene`: the custom-operator / structural layer of Monocurl's native
//! Rust API. See `API_DIRECTION.md` for the three-layer model and
//! `OPERATORS.md` for the operator-authoring spec this crate implements
//! (milestones 1-4; `live!` slot wiring, milestone 5, is not implemented).

extern crate self as rust_scene;

mod builtins;
mod error;
mod keyed;
mod lerp;
mod macro_support;
mod mesh;
mod operator;
pub mod ops;
mod vector;

pub use builtins::circle;
pub use error::{Error, Result};
pub use keyed::{Hold, Keyed};
pub use mesh::{Leaf, MeshValue, Shape};
pub use operator::{
    Apply, ApplyEndpoints, Construct, ConstructorNode, OpId, OperatorNode, OperatorStruct, Slot,
};
pub use vector::{Color, Vec2, Vec3, Vec4};

pub use rust_scene_macros::{Constructor, Operator, constructor, operator};

/// Support code targeted by macro-generated `impl`s. Not part of the stable
/// public API; kept `pub` only because generated code in downstream crates
/// needs to name it.
#[doc(hidden)]
pub mod __macro_support {
    pub use crate::macro_support::*;
}

pub mod prelude {
    pub use crate::{
        Apply, ApplyEndpoints, Color, Construct, Constructor, ConstructorNode, Hold, Keyed, Leaf,
        MeshValue, OpId, Operator, OperatorNode, OperatorStruct, Shape, Slot, Vec2, Vec3, Vec4,
        circle, constructor, operator,
    };
}
