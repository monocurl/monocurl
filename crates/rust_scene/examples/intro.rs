use rust_scene::{
    Color, MeshValue, OperatorEndpoints, Recipe, Result, Vec2, live, mesh_with_follower,
};

live! {
    fn ball(center: Vec2, radius: f64, color: Color) -> Result<MeshValue> {
        Ok(MeshValue::circle(center, radius, color))
    }
}

live! {
    operator fn shift(target: MeshValue, delta: Vec2) -> Result<OperatorEndpoints> {
        Ok(OperatorEndpoints::new(target.clone(), target.translate(delta)))
    }
}

fn main() -> Result<()> {
    use BallNestedExt as _;
    use ShiftMeshExt as _;

    let base = ball(Vec2::ZERO, 0.45, Color::BLUE);
    let follower = base.evaluate()?;
    let scene = mesh_with_follower(shift(Vec2::new(4.0, 0.0), base), follower);
    println!("halfway: {:?}", scene.lerp(0.5)?);
    scene.delta().set(Vec2::new(2.0, 1.0));
    scene.target().radius().set(0.7);
    scene.sync()?;
    println!("edited: {:?}", scene.follower());
    Ok(())
}
