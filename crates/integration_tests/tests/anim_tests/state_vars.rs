use super::*;

#[test]
fn test_state_leader_count_prelude_only() {
    // no user-defined leaders → only camera + background from prelude
    let r = run_anim("let x = 1");
    r.assert_ok();
    assert_eq!(
        r.param_leaders().len(),
        2,
        "expected camera and background from prelude"
    );
    assert_eq!(r.mesh_leaders().len(), 0);
}

#[test]
fn test_state_leader_initial_value() {
    let r = run_anim("param score = 42");
    r.assert_ok();
    let params = r.param_leaders();
    let user_leader = params.last().expect("no param leaders found");
    user_leader.assert_target_int(42).assert_current_int(42);
}

#[test]
fn test_multiple_state_leaders() {
    let r = run_anim(
        "
        param a = 10
        param b = 20
    ",
    );
    r.assert_ok();
    assert_eq!(r.param_leaders().len(), 4);
    let params = r.param_leaders();
    params[2].assert_target_int(10);
    params[3].assert_target_int(20);
}

#[test]
fn test_param_leader() {
    let r = run_anim("param speed = 5");
    r.assert_ok();
    assert_eq!(r.param_leaders().len(), 3);
    r.param_leaders()[2]
        .assert_target_int(5)
        .assert_current_int(5);
}

// -- set / lerp --
