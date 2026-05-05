use super::*;

#[test]
fn test_state_leader_count_prelude_only() {
    // no user-defined leaders → only camera + background from prelude
    let r = run_anim("let x = 1");
    r.assert_ok();
    assert_eq!(
        r.scene_leaders().len(),
        2,
        "expected camera and background from prelude"
    );
    assert_eq!(r.mesh_leaders().len(), 0);
}

#[test]
fn test_mesh_leader_initial_value() {
    let r = run_anim("mesh score = 42");
    r.assert_ok();
    let meshes = r.mesh_leaders();
    let user_leader = meshes.last().expect("no mesh leaders found");
    user_leader.assert_target_int(42);
}

#[test]
fn test_multiple_mesh_leaders() {
    let r = run_anim(
        "
        mesh a = 10
        mesh b = 20
    ",
    );
    r.assert_ok();
    assert_eq!(r.mesh_leaders().len(), 2);
    let meshes = r.mesh_leaders();
    meshes[0].assert_target_int(10);
    meshes[1].assert_target_int(20);
}

#[test]
fn test_scene_leader() {
    let r = run_anim_with_stdlib(
        "
        background = 5
        play Set([&background])
    ",
    );
    r.assert_ok();
    assert_eq!(r.scene_leaders().len(), 2);
    r.scene_leaders()
        .into_iter()
        .find(|leader| matches!(leader.target, Value::Integer(5)))
        .expect("expected updated background leader")
        .assert_target_int(5)
        .assert_current_int(5);
}

// -- set / lerp --
