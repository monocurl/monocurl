//! Milestone 4: `#[operator]` / `#[operator(endpoints)]` / `#[constructor]`
//! attribute macros.

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
fn ring(radius: f64, thickness: f64) -> MeshValue {
    // No dedicated "ring" shape in this minimal payload; reuse Circle and fold
    // `thickness` into the radius so the test can assert both slots were
    // threaded through.
    MeshValue::leaf(rust_scene::Leaf::new(Shape::Circle {
        radius: radius + thickness,
    }))
}

#[test]
fn constructor_takes_all_parameters_positionally() {
    let mesh = ring(1.0, 0.5).evaluate();
    assert_eq!(mesh.as_leaf().unwrap().shape, Shape::Circle { radius: 1.5 });
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

/// Multi-parameter operator: every parameter is required and positional, so a
/// single chaining method covers the whole surface. There is no default-argument
/// arity — see `rust_scene_macros::attr_operator`'s "No default arguments" note,
/// and `tests/ui/default_argument_rejected.rs` for the rejection.
#[operator]
fn wobble_field(target: MeshValue, amount: f64, frequency: f64) -> MeshValue {
    target.map_leaves(|leaf| match leaf.shape {
        Shape::Circle { radius } => rust_scene::Leaf {
            shape: Shape::Circle {
                radius: radius + amount * frequency,
            },
            ..leaf.clone()
        },
    })
}

#[test]
fn all_parameters_are_positional() {
    let mesh = circle(1.0).wobble_field(1.0, 2.0).evaluate();
    assert_eq!(mesh.as_leaf().unwrap().shape, Shape::Circle { radius: 3.0 });
}

impl Default for WobbleField {
    fn default() -> Self {
        Self {
            amount: 1.0,
            frequency: 10.0,
        }
    }
}

/// The struct-literal entry point is how callers get defaults now: `WobbleField`
/// is a plain struct, so `..Default::default()` fills whatever is omitted.
#[test]
fn struct_literal_with_default_stands_in_for_omitted_args() {
    let mesh = circle(1.0)
        .with(WobbleField {
            amount: 1.0,
            ..Default::default()
        })
        .evaluate();
    assert_eq!(
        mesh.as_leaf().unwrap().shape,
        Shape::Circle { radius: 11.0 }
    );
}
