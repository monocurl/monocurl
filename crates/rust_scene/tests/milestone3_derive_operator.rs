//! Milestone 3: `#[derive(Operator)]` — form B — including `#[hold]` and the
//! `mesh.with(Wobble { .. })` struct-literal entry point.

use rust_scene::{Keyed, MeshValue, Operator, Shape, circle};

#[derive(Operator, Clone)]
pub struct Wobble {
    pub amount: f64,
    #[hold]
    pub axis_label: String,
    pub frequency: f64,
}

impl rust_scene::Apply for Wobble {
    fn apply(&self, target: MeshValue) -> MeshValue {
        // Deliberately simple: bump the radius by `amount` so the test can
        // observe the operator actually ran, without needing real geometry.
        target.map_leaves(|leaf| match leaf.shape {
            Shape::Circle { radius } => rust_scene::Leaf {
                shape: Shape::Circle {
                    radius: radius + self.amount,
                },
                ..leaf.clone()
            },
        })
    }
}

#[test]
fn with_struct_literal_entry_point_applies_the_operator() {
    let mesh = circle(1.0).with(Wobble {
        amount: 0.5,
        axis_label: "y".to_string(),
        frequency: 3.0,
    });
    let leaf = mesh.evaluate();
    assert_eq!(leaf.as_leaf().unwrap().shape, Shape::Circle { radius: 1.5 });
}

#[test]
fn generated_chaining_method_matches_with_entry_point() {
    use WobbleChainExt as _;

    let via_with = circle(1.0)
        .with(Wobble {
            amount: 0.5,
            axis_label: "y".to_string(),
            frequency: 3.0,
        })
        .evaluate();
    let via_chain = circle(1.0).wobble(0.5, "y".to_string(), 3.0).evaluate();
    assert_eq!(via_with.as_leaf().unwrap(), via_chain.as_leaf().unwrap());
}

#[test]
fn hold_field_must_match_to_lerp() {
    let a = circle(1.0).with(Wobble {
        amount: 0.0,
        axis_label: "x".to_string(),
        frequency: 1.0,
    });
    let b = circle(1.0).with(Wobble {
        amount: 1.0,
        axis_label: "x".to_string(),
        frequency: 2.0,
    });
    assert!(a.lerp(&b, 0.5).is_ok());

    let c = circle(1.0).with(Wobble {
        amount: 1.0,
        axis_label: "y".to_string(),
        frequency: 2.0,
    });
    let err = a.lerp(&c, 0.5).unwrap_err();
    assert!(err.to_string().contains("differs"));
}
