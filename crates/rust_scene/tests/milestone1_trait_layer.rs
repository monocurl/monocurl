//! Milestone 1: trait layer + retained operator nodes + memoized operator
//! variant + `map_leaves`, exercised through the hand-written `Shift`
//! reference operator (no macros).

use rust_scene::{
    Shape, Vec3, circle,
    ops::{Shift, ShiftChainExt},
};

#[test]
fn shift_translates_every_leaf() {
    let mesh = circle(1.0).shift(Vec3::new(2.0, 0.0, 0.0));
    let evaluated = mesh.evaluate();
    let Some(leaf) = evaluated.as_leaf() else {
        panic!("expected a single leaf");
    };
    assert_eq!(leaf.center, Vec3::new(2.0, 0.0, 0.0));
    assert_eq!(leaf.shape, Shape::Circle { radius: 1.0 });
}

#[test]
fn shift_is_retained_and_chainable() {
    // Two shifts compose (each is a distinct retained operator layer, not
    // fused at construction time).
    let mesh = circle(1.0)
        .shift(Vec3::new(1.0, 0.0, 0.0))
        .shift(Vec3::new(0.0, 1.0, 0.0));
    let evaluated = mesh.evaluate();
    let leaf = evaluated.as_leaf().unwrap();
    assert_eq!(leaf.center, Vec3::new(1.0, 1.0, 0.0));
}

#[test]
fn evaluate_is_memoized() {
    // Evaluating twice returns the same (structurally equal) result; this
    // mostly exercises that repeated evaluation doesn't panic/double-apply.
    let mesh = circle(1.0).shift(Vec3::new(1.0, 0.0, 0.0));
    let first = mesh.evaluate();
    let second = mesh.evaluate();
    assert_eq!(first.as_leaf().unwrap(), second.as_leaf().unwrap());
}

#[test]
fn shift_default_identity_is_the_operand() {
    use rust_scene::OperatorNode;
    let base = circle(1.0);
    let node = Shift::new(Vec3::new(3.0, 0.0, 0.0));
    let (identity, modified) = node.endpoints(base.clone());
    assert_eq!(
        identity.evaluate().as_leaf().unwrap(),
        base.evaluate().as_leaf().unwrap()
    );
    assert_eq!(modified.as_leaf().unwrap().center, Vec3::new(3.0, 0.0, 0.0));
}
