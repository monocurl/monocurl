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

#[test]
fn generated_operator_and_nested_fields_are_typed() -> Result<()> {
    use BallNestedExt as _;
    use ShiftMeshExt as _;

    let base = ball(Vec2::ZERO, 1.0, Color::BLUE);
    let follower = base.evaluate()?;
    let scene = mesh_with_follower(shift(Vec2::new(4.0, 0.0), base), follower);

    scene.delta().set(Vec2::new(2.0, 0.0));
    scene.target().radius().set(3.0);
    let MeshValue::Circle { center, radius, .. } = scene.lerp(1.0)? else {
        panic!("expected circle")
    };
    assert_eq!(center, Vec2::new(2.0, 0.0));
    assert_eq!(radius, 3.0);
    Ok(())
}
