//! Runtime support code targeted by `rust_scene_macros`-generated
//! implementations. Not part of the crate's stable public API surface (hence
//! `#[doc(hidden)]` throughout) but `pub` because generated code — which
//! lives in downstream crates — needs to name it.
//!
//! ## The `Apply` / `ApplyEndpoints` dispatch trick
//!
//! `OPERATORS.md` describes the derive as emitting `impl OperatorNode for X
//! where X: OperatorBody`, with `Apply` and `ApplyEndpoints` each
//! blanket-impling a shared `OperatorBody` trait so the derive doesn't need to
//! know which one the user picked. That specific shape does not compile in
//! stable Rust: two blanket impls of the *same* trait (`OperatorBody`) for
//! generic `T` bounded on two different traits are conflicting impls as far
//! as coherence is concerned, even though no concrete type is ever meant to
//! implement both `Apply` and `ApplyEndpoints`. Coherence checking is
//! syntactic, not semantic — it can't see that the bounds are meant to be
//! mutually exclusive.
//!
//! What *does* work, and is used here, is the standard "autoref
//! specialization" pattern: two helper traits with the same method name, one
//! implemented for `Probe<T>` and the other for `&Probe<T>`. A call written as
//! `(&Probe(self)).dispatch_endpoints(target)` resolves the `&Probe<T>` impl
//! at method-probe depth 0 (no deref needed) before the `Probe<T>` impl at
//! depth 1, so whichever of `ApplyEndpoints`/`Apply` is actually implemented
//! for the concrete operator type is the one found — and if neither is
//! implemented, the call fails to resolve at all, which is the desired
//! "you must implement exactly one" behavior. The public `Apply` /
//! `ApplyEndpoints` traits are unchanged from the spec; only this internal
//! wiring differs from the doc's literal sketch.

use std::any::Any;

use crate::{
    error::{Error, Result},
    keyed::hold_lerp,
    mesh::MeshValue,
    operator::{Apply, ApplyEndpoints, Construct, ConstructorNode, OperatorNode},
};

#[doc(hidden)]
pub struct Probe<'a, T>(pub &'a T);

#[doc(hidden)]
pub trait ViaApply {
    fn dispatch_endpoints(&self, target: MeshValue) -> (MeshValue, MeshValue);
}

impl<'a, T: Apply> ViaApply for Probe<'a, T> {
    fn dispatch_endpoints(&self, target: MeshValue) -> (MeshValue, MeshValue) {
        let modified = self.0.apply(target.clone());
        (target, modified)
    }
}

#[doc(hidden)]
pub trait ViaApplyEndpoints {
    fn dispatch_endpoints(&self, target: MeshValue) -> (MeshValue, MeshValue);
}

impl<'a, T: ApplyEndpoints> ViaApplyEndpoints for &Probe<'a, T> {
    fn dispatch_endpoints(&self, target: MeshValue) -> (MeshValue, MeshValue) {
        self.0.endpoints(target)
    }
}

/// Downcast an `&dyn OperatorNode` to a concrete operator type, producing a
/// message shaped like the executor's lerp-mismatch errors.
#[doc(hidden)]
pub fn downcast_operator<'a, T: OperatorNode + 'static>(
    other: &'a dyn OperatorNode,
    op_name: &str,
) -> Result<&'a T> {
    other.as_any().downcast_ref::<T>().ok_or_else(|| {
        Error::message(format!(
            "cannot lerp `{op_name}`: operand has a different operator identity"
        ))
    })
}

#[doc(hidden)]
pub fn downcast_constructor<'a, T: ConstructorNode + 'static>(
    other: &'a dyn ConstructorNode,
    op_name: &str,
) -> Result<&'a T> {
    other.as_any().downcast_ref::<T>().ok_or_else(|| {
        Error::message(format!(
            "cannot lerp `{op_name}`: operand has a different constructor identity"
        ))
    })
}

/// Wrap a per-slot lerp failure with the operator name / slot index / field
/// name, mirroring the executor's "cannot lerp ... argument at index N
/// differs" wording.
#[doc(hidden)]
pub fn slot_error(op_name: &str, index: usize, field_name: &str, err: Error) -> Error {
    Error::message(format!(
        "cannot lerp `{op_name}`: argument `{field_name}` at index {index} differs ({err})"
    ))
}

#[doc(hidden)]
pub fn hold_slot_error(op_name: &str, index: usize, field_name: &str) -> Error {
    Error::message(format!(
        "cannot lerp `{op_name}`: held argument `{field_name}` at index {index} differs"
    ))
}

/// Used by generated `#[hold]` slot lerps (fields stored as plain `T`, not
/// `Hold<T>`).
#[doc(hidden)]
pub fn hold_field_lerp<T: Clone + PartialEq>(
    op_name: &str,
    index: usize,
    field_name: &str,
    a: &T,
    b: &T,
) -> Result<T> {
    hold_lerp(a, b).map_err(|_| hold_slot_error(op_name, index, field_name))
}

/// Constructor bodies dispatch through a single trait (`Construct`), so no
/// autoref trick is needed — kept here anyway for symmetry with the operator
/// dispatch helpers and so generated code has one support module to import.
#[doc(hidden)]
pub fn dispatch_construct<T: Construct>(value: &T) -> MeshValue {
    value.construct()
}

#[doc(hidden)]
pub fn any_ref<T: 'static>(value: &T) -> &dyn Any {
    value
}
