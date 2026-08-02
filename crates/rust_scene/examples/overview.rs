//! translation of the leader/follower core of `(Tutorial) Monocurl Overview.mcs`.
//!
//! `StaticCircle` is ordinary Rust. `live!` is used only for `Ball`, because
//! this scene wants to mutate its named `center`, `radius`, and `color` fields.

use rust_scene::{Color, MeshValue, Recipe, Result, Vec2, live, mesh, mesh_with_follower};

#[derive(Clone)]
struct StaticCircle {
    center: Vec2,
    radius: f64,
    color: Color,
}

impl Recipe for StaticCircle {
    fn evaluate(&self) -> Result<MeshValue> {
        Ok(MeshValue::circle(self.center, self.radius, self.color))
    }
}

fn circle(center: Vec2, radius: f64, color: Color) -> StaticCircle {
    StaticCircle {
        center,
        radius,
        color,
    }
}

live! {
    fn ball(center: Vec2, radius: f64, color: Color) -> Result<MeshValue> {
        Ok(MeshValue::circle(center, radius, color))
    }
}

fn main() -> Result<()> {
    use BallMeshExt as _;

    // ordinary constructor: no macro and no editable labeled fields
    let title_dot = mesh(circle(Vec2::new(-1.6, 0.0), 0.45, Color::BLUE))?;
    println!("init Set: {:?}", title_dot.follower());

    // corresponding to `mesh dot = Ball(center: ..., radius: ..., color: ...)`
    let start = ball(Vec2::new(-1.6, 0.0), 0.45, Color::BLUE);
    let dot = mesh(start)?;

    // corresponding to `dot.center = ...; dot.color = ...; play Lerp(1)`
    dot.center().set(Vec2::new(1.6, 0.0));
    dot.color().set(Color::ORANGE);
    println!("after Lerp(0.5): {:?}", dot.lerp(0.5)?);

    // the typed recipe itself is also usable as a keyframe destination
    let next = ball(Vec2::new(0.0, 1.0), 0.6, Color::ORANGE);
    let follower = dot.follower();
    let next_dot = mesh_with_follower(next, follower);
    println!("next Lerp(0.5): {:?}", next_dot.lerp(0.5)?);
    Ok(())
}
