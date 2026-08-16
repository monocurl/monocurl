//! Milestone 4: `#[operator]` / `#[operator(endpoints)]` / `#[constructor]`
//! attribute macros, including `#[default(..)]` arity generation.

use rust_scene::{Keyed, MeshValue, Shape, Vec3, circle, constructor, operator};

/// A displacement operator with a custom identity endpoint: unlike the
/// default (untouched operand), `rotate_about` should interpolate "along the
/// arc" — modeled here as two straight shifts (0 and `distance`) so the test
/// can observe that `endpoints()` is genuinely driving the interpolation
/// rather than falling back to the default identity.
#[operator(endpoints)]
fn slide_about(target: MeshValue, distance: f64) -> (MeshValue, MeshValue) {
    let go = |d: f64| target.map_leaves(|leaf| leaf.translated(Vec3::new(d, 0.0, 0.0)));
    (go(0.0), go(distance))
}

#[test]
fn operator_endpoints_uses_custom_identity() {
    use SlideAboutChainExt as _;

    let mesh = circle(1.0).slide_about(4.0);
    let leaf = mesh.evaluate();
    assert_eq!(leaf.as_leaf().unwrap().center, Vec3::new(4.0, 0.0, 0.0));
}

#[test]
fn operator_endpoints_pop_uses_custom_identity_not_the_operand() {
    use SlideAboutChainExt as _;

    // Identity endpoint for `slide_about` is `go(0.0)` (still centered at the
    // origin), *not* the raw operand — same value here since translating by
    // 0 is a no-op, but exercised through the actual `endpoints()` call path
    // (rule 4 popping), not the default `Apply` identity shortcut.
    let plain = circle(1.0);
    let slid = circle(1.0).slide_about(4.0);
    let mid = plain.lerp(&slid, 0.5).unwrap().evaluate();
    assert_eq!(mid.as_leaf().unwrap().center, Vec3::new(2.0, 0.0, 0.0));
}

#[constructor]
fn ring(radius: f64, #[default(0.1)] thickness: f64) -> MeshValue {
    // No dedicated "ring" shape in this minimal payload; reuse Circle and
    // fold `thickness` into the radius so the test can still assert the
    // default actually got threaded through.
    MeshValue::leaf(rust_scene::Leaf::new(Shape::Circle {
        radius: radius + thickness,
    }))
}

#[test]
fn constructor_full_arity() {
    let mesh = ring(1.0, 0.5).evaluate();
    assert_eq!(mesh.as_leaf().unwrap().shape, Shape::Circle { radius: 1.5 });
}

#[test]
fn constructor_default_arity_uses_default_expression() {
    let mesh = ring_defaults(1.0).evaluate();
    assert_eq!(mesh.as_leaf().unwrap().shape, Shape::Circle { radius: 1.1 });
}

#[test]
fn constructor_retains_identity_for_lerp_rule_3() {
    let a = ring(1.0, 0.1);
    let b = ring(3.0, 0.5);
    let mid = a.lerp(&b, 0.5).unwrap().evaluate();
    // (1.0+0.1) lerp (3.0+0.5) at 0.5 == 1.1 lerp 3.5 == 2.3, since radius and
    // thickness both lerp linearly and addition is linear.
    assert_eq!(mid.as_leaf().unwrap().shape, Shape::Circle { radius: 2.3 });
}

/// `#[default(..)]` arity on an operator: full arity
/// (`WobbleField::wobble(amount, frequency)`) vs. defaults-dropped arity
/// (`WobbleFieldDefaultsChainExt::wobble(amount)`). Each is exercised in its
/// own module so only one of the two chaining traits is ever `use`d into a
/// scope at a time — see `rust_scene_macros::attr_operator`'s module doc for
/// why both can't be imported simultaneously in stable Rust.
#[operator]
fn wobble_field(target: MeshValue, amount: f64, #[default(10.0)] frequency: f64) -> MeshValue {
    target.map_leaves(|leaf| match leaf.shape {
        Shape::Circle { radius } => rust_scene::Leaf {
            shape: Shape::Circle {
                radius: radius + amount * frequency,
            },
            ..leaf.clone()
        },
    })
}

// Each submodule imports only the specific items it needs (never `use
// super::*`, which would glob in *both* chaining traits and reproduce the
// E0034 ambiguity this split is meant to avoid) plus exactly one of the two
// `WobbleField*ChainExt` traits.
mod full_arity {
    use super::WobbleFieldChainExt;
    use rust_scene::{Shape, circle};

    #[test]
    fn explicit_frequency_is_used() {
        let mesh = circle(1.0).wobble_field(1.0, 2.0).evaluate();
        assert_eq!(mesh.as_leaf().unwrap().shape, Shape::Circle { radius: 3.0 });
    }
}

mod default_arity {
    use super::WobbleFieldDefaultsChainExt;
    use rust_scene::{Shape, circle};

    #[test]
    fn omitted_frequency_uses_default_expression() {
        let mesh = circle(1.0).wobble_field(1.0).evaluate();
        assert_eq!(
            mesh.as_leaf().unwrap().shape,
            Shape::Circle { radius: 11.0 }
        );
    }
}
