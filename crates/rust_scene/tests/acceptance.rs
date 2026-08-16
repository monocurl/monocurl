//! The acceptance example from the bottom of `OPERATORS.md`, verbatim (aside
//! from the crate-qualified imports it assumes via `use monocurl::prelude::*`
//! in the doc, which doesn't exist yet — `rust_scene::prelude` stands in).

use rust_scene::prelude::*;

#[operator]
fn squash(target: MeshValue, factor: f64) -> MeshValue {
    target.map_leaves(|m| m.scaled(Vec3::new(1.0, factor, 1.0)))
}

#[test]
fn squash_is_first_class() {
    let a = circle(1.0).squash(1.0);
    let b = circle(2.0).squash(0.5);
    let mid = a.lerp(&b, 0.5).unwrap(); // rule 3: radius 1.5, factor 0.75

    let plain = circle(1.0);
    let popped = plain.lerp(&b, 0.5).unwrap(); // rule 4: pops through identity
    let _ = (mid, popped);
}
