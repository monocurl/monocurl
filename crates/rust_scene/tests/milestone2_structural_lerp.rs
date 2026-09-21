//! Milestone 2: structural lerp rules 1-4, ported from
//! `crates/executor/src/executor/lerp.rs` and
//! `crates/integration_tests/tests/basic_executor_tests/live_values.rs`'s
//! behavioral scenarios (function-identity matching, unequal-depth popping,
//! hold-slot equality), adapted to `MeshValue`/`OperatorNode`.

use std::any::Any;

use rust_scene::{
    Hold, Keyed, MeshValue, OpId, OperatorNode, Result, Shape, Slot, Vec3, circle,
    ops::ShiftChainExt,
};

#[test]
fn rule1_equal_values_short_circuit() {
    let a = circle(1.0).evaluate();
    let b = circle(1.0).evaluate();
    let mid = a.lerp(&b, 0.7).unwrap();
    assert_eq!(mid.as_leaf().unwrap(), a.as_leaf().unwrap());
}

#[test]
fn rule2_numbers_blend_linearly_through_leaf_fields() {
    let a = circle(1.0);
    let b = circle(3.0);
    let mid = a.lerp(&b, 0.25).unwrap().evaluate();
    assert_eq!(mid.as_leaf().unwrap().shape, Shape::Circle { radius: 1.5 });
}

#[test]
fn rule3_same_operator_identity_lerps_slots_and_operand() {
    let a = circle(1.0).shift(Vec3::new(0.0, 0.0, 0.0));
    let b = circle(3.0).shift(Vec3::new(10.0, 0.0, 0.0));
    let mid = a.lerp(&b, 0.5).unwrap().evaluate();
    let leaf = mid.as_leaf().unwrap();
    assert_eq!(leaf.shape, Shape::Circle { radius: 2.0 });
    assert_eq!(leaf.center, Vec3::new(5.0, 0.0, 0.0));
}

#[test]
fn rule3_same_constructor_identity_lerps_slots() {
    // Two `circle(..)` calls are the same retained constructor identity, so
    // an *unlabeled* re-assignment (just a different literal) still
    // animates — this is the "plain unlabeled reassignment still animates
    // when function identities match" guarantee from `API_DIRECTION.md`.
    let a = circle(2.0);
    let b = circle(4.0);
    let mid = a.lerp(&b, 0.5).unwrap().evaluate();
    assert_eq!(mid.as_leaf().unwrap().shape, Shape::Circle { radius: 3.0 });
}

#[test]
fn rule4_mismatched_depth_pops_through_identity_endpoint() {
    // `plain` has 0 operator layers, `shifted` has 1. Popping recurses
    // `lerp(plain, shifted.operand)` first, then applies `shift`'s endpoints
    // to that midpoint and lerps between them — matching
    // `Executor::lerp_popped_operator_rhs` / `lerp_operator_embeds`.
    let plain = circle(1.0);
    let shifted = circle(1.0).shift(Vec3::new(4.0, 0.0, 0.0));

    let mid = plain.lerp(&shifted, 0.5).unwrap();
    let leaf = mid.as_leaf().unwrap();
    // operand side is unchanged (circle(1.0) both sides, depth 0 vs 1 with
    // the same underlying circle), and shift's default identity is the
    // operand untouched — so at t=0.5 we expect the shift to be half-applied.
    assert_eq!(leaf.shape, Shape::Circle { radius: 1.0 });
    assert_eq!(leaf.center, Vec3::new(2.0, 0.0, 0.0));
}

#[test]
fn rule4_pop_also_lerps_a_differing_operand() {
    let plain = circle(1.0);
    let shifted = circle(3.0).shift(Vec3::new(4.0, 0.0, 0.0));

    let mid = plain.lerp(&shifted, 0.5).unwrap();
    let leaf = mid.as_leaf().unwrap();
    assert_eq!(leaf.shape, Shape::Circle { radius: 2.0 });
    assert_eq!(leaf.center, Vec3::new(2.0, 0.0, 0.0));
}

#[test]
fn mismatched_operator_identity_at_equal_depth_errors() {
    #[derive(Clone)]
    struct Noop;
    impl OperatorNode for Noop {
        fn op_id(&self) -> OpId {
            OpId::of::<Noop>()
        }
        fn endpoints(&self, target: MeshValue) -> (MeshValue, MeshValue) {
            (target.clone(), target)
        }
        fn lerp_slots(&self, _other: &dyn OperatorNode, _t: f64) -> Result<Box<dyn OperatorNode>> {
            Ok(Box::new(Noop))
        }
        fn slot(&self, index: usize) -> &dyn Slot {
            panic!("no slots (index {index})")
        }
        fn slot_mut(&mut self, index: usize) -> &mut dyn Slot {
            panic!("no slots (index {index})")
        }
        fn slot_count(&self) -> usize {
            0
        }
        fn clone_node(&self) -> Box<dyn OperatorNode> {
            Box::new(self.clone())
        }
        fn debug_name(&self) -> &'static str {
            "noop"
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    let a = circle(1.0).shift(Vec3::new(1.0, 0.0, 0.0));
    let b = circle(1.0).with(Noop);
    assert!(a.lerp(&b, 0.5).is_err());
}

#[test]
fn hold_slot_requires_equality_to_lerp() {
    let a = Hold::new("left".to_string());
    let b = Hold::new("left".to_string());
    assert!(a.lerp(&b, 0.5).is_ok());

    let c = Hold::new("right".to_string());
    let err = a.lerp(&c, 0.5).unwrap_err();
    assert!(err.to_string().contains("differs"));
}

#[test]
fn mismatched_leaf_shapes_error() {
    // No other builtin shape exists yet to construct a genuinely different
    // shape, so this instead exercises the group-length-mismatch arm of the
    // same fallback path.
    let a = MeshValue::group([circle(1.0).evaluate()]);
    let b = MeshValue::group([circle(1.0).evaluate(), circle(2.0).evaluate()]);
    assert!(a.lerp(&b, 0.5).is_err());
}
