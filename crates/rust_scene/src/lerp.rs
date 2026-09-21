//! Structural lerp: the four rules from `API_DIRECTION.md` / `OPERATORS.md`,
//! ported from `crates/executor/src/executor/lerp.rs`'s `Executor::lerp`.
//!
//! Rules (priority order, matching the executor):
//! 1. equal (already-evaluated) values → either side.
//! 2. mismatched operator depth → pop a layer from the deeper side through
//!    the operator's endpoint pair, recursing the popped operand against the
//!    other side first (`lerp_operator_embeds`, mirroring
//!    `lerp_popped_operator_lhs`/`rhs` + `lerp_operator_embeds`).
//! 3. same operator/constructor identity (`op_id`) at equal depth → lerp
//!    slots element-wise via `OperatorNode::lerp_slots` /
//!    `ConstructorNode::lerp_slots`, producing a new retained node (the
//!    result stays unevaluated, just like the executor keeps
//!    `InvokedOperator`/`InvokedFunction` retained after lerping).
//! 4. containers (leaf shape fields, `Group`) recurse elementwise.
//!
//! Two simplifications relative to the executor, both documented here rather
//! than in the four-rule list above since they don't change observable
//! behavior for any case the executor itself treats as valid:
//!
//! - Rule 1's short-circuit only fires when both sides are *already*
//!   evaluated `Leaf`/`Group` nodes (no forced evaluation just to check
//!   equality). This is strictly an optimization in the executor too — a
//!   value that's "equal" but not caught by this check still lerps to the
//!   same result through rules 3/4 (e.g. `f64::lerp(1.0, 1.0, t) == 1.0`), so
//!   skipping it for as-yet-unevaluated retained nodes only costs a cache hit,
//!   never correctness.
//! - Operators require exact `op_id` equality at matched depth (erroring like
//!   the executor's `lerp_invoked_operators` does on operator mismatch).
//!   Constructors, mirroring `InvokedFunction`/`same_lambda_ip`, fall through
//!   to full evaluation on identity mismatch instead of erroring — the
//!   executor's real asymmetry between the two, not a simplification.

use crate::{
    error::{Error, Result},
    keyed::Keyed,
    mesh::MeshValue,
    operator::OperatorNode,
};

impl Keyed for MeshValue {
    fn lerp(&self, other: &Self, t: f64) -> Result<Self> {
        lerp(self, other, t)
    }
}

pub(crate) fn lerp(a: &MeshValue, b: &MeshValue, t: f64) -> Result<MeshValue> {
    // Rule 1.
    if MeshValue::evaluated_eq(a, b) {
        return Ok(a.clone());
    }

    let a_depth = a.operator_depth();
    let b_depth = b.operator_depth();

    // Rule 2 (operator popping).
    if a_depth > b_depth
        && let Some((node, operand)) = a.as_operator()
    {
        let mid = lerp(operand, b, t)?;
        return lerp_operator_embeds(node, &mid, t);
    }
    if b_depth > a_depth
        && let Some((node, operand)) = b.as_operator()
    {
        let mid = lerp(a, operand, t)?;
        return lerp_operator_embeds(node, &mid, t);
    }

    // Rule 3, operators: equal (nonzero) depth, both retained operator layers.
    if a_depth > 0
        && let (Some((a_node, a_operand)), Some((b_node, b_operand))) =
            (a.as_operator(), b.as_operator())
    {
        if a_node.op_id() != b_node.op_id() {
            return Err(Error::message(format!(
                "cannot lerp operators `{}` and `{}`: different operator identities",
                a_node.debug_name(),
                b_node.debug_name()
            )));
        }
        let lerped_operand = lerp(a_operand, b_operand, t)?;
        let lerped_node = a_node.lerp_slots(b_node, t)?;
        return Ok(MeshValue::from_operator_node(lerped_node, lerped_operand));
    }

    // Rule 3, constructors: same identity → lerp slots; mismatched identity
    // falls through to full evaluation (see module doc).
    if let (Some(a_ctor), Some(b_ctor)) = (a.as_constructor(), b.as_constructor())
        && a_ctor.op_id() == b_ctor.op_id()
    {
        let lerped = a_ctor.lerp_slots(b_ctor, t)?;
        return Ok(MeshValue::from_constructor_node(lerped));
    }

    // Rule 4 + fallback: force evaluation and lerp the concrete tree.
    let a_eval = a.evaluate();
    let b_eval = b.evaluate();
    lerp_evaluated(&a_eval, &b_eval, t)
}

fn lerp_operator_embeds(node: &dyn OperatorNode, mid: &MeshValue, t: f64) -> Result<MeshValue> {
    let (embed0, embed1) = node.endpoints(mid.clone());
    lerp(&embed0, &embed1, t)
}

fn lerp_evaluated(a: &MeshValue, b: &MeshValue, t: f64) -> Result<MeshValue> {
    if MeshValue::evaluated_eq(a, b) {
        return Ok(a.clone());
    }
    if let (Some(a_leaf), Some(b_leaf)) = (a.as_leaf(), b.as_leaf()) {
        return Ok(MeshValue::leaf(a_leaf.lerp(b_leaf, t)?));
    }
    if let (Some(a_children), Some(b_children)) = (a.as_group(), b.as_group()) {
        if a_children.len() != b_children.len() {
            return Err(Error::message(format!(
                "cannot lerp mesh groups of different lengths: {} vs {}",
                a_children.len(),
                b_children.len()
            )));
        }
        let children: Result<Vec<_>> = a_children
            .iter()
            .zip(b_children)
            .map(|(x, y)| lerp(x, y, t))
            .collect();
        return Ok(MeshValue::group(children?));
    }
    Err(Error::message(
        "cannot lerp mesh values with incompatible structure",
    ))
}
