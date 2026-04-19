// stateful value tests
// covers: $param creation, *mesh dereference, runtime validation for invalid
// stateful operators/subscripts, caching, error cases, and interaction with animations.

use anim_tests::{LeaderInfo, run_anim, run_anim_with_stdlib, run_anim_with_stdlib_at};
use executor::value::Value;

// anim_tests helpers are in a sibling test file; re-export by including it
#[path = "anim_tests.rs"]
mod anim_tests;

// ── helpers ───────────────────────────────────────────────────────────────────

fn assert_mesh_target_int(leaders: &[LeaderInfo], mesh_idx: usize, expected: i64) {
    use executor::state::LeaderKind;
    let meshes: Vec<&LeaderInfo> = leaders
        .iter()
        .filter(|l| l.kind == LeaderKind::Mesh)
        .collect();
    match &meshes[mesh_idx].target {
        Value::Integer(n) => assert_eq!(*n, expected, "mesh[{}] target int mismatch", mesh_idx),
        other => panic!(
            "mesh[{}]: expected Integer({}), got {}",
            mesh_idx,
            expected,
            other.type_name()
        ),
    }
}

// ── basic stateful creation ───────────────────────────────────────────────────

#[test]
fn test_stateful_stored_in_mesh() {
    // $x captures a stateful ref to param x; stored in mesh m
    let r = run_anim(
        "
        param x = 42
        mesh m = $x
    ",
    );
    r.assert_ok();
}

#[test]
fn test_stateful_dereference_equals_follower() {
    // *m dereferences the stateful expression — with no animations the
    // follower equals the leader value (param initial = 7)
    let r = run_anim(
        "
        param x = 7
        mesh m = $x
        let result = *m
    ",
    );
    r.assert_ok();
}

#[test]
fn test_stateful_dereference_reflects_param_leader_value() {
    // after changing x, *m should evaluate to the new follower.
    // with no animation the follower == leader immediately.
    let r = run_anim(
        "
        param x = 1
        mesh m = $x
        x = 99
    ",
    );
    r.assert_ok();
    // param leader value should be 99
    r.param_leaders()[2].assert_target_int(99);
}

// ── stateful operator runtime errors ─────────────────────────────────────────

#[test]
fn test_stateful_add_constant_stored_in_mesh() {
    let r = run_anim(
        "
        param x = 10
        mesh m = $x + 2
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

#[test]
fn test_stateful_dereference_add_evaluates_correctly() {
    let r = run_anim(
        "
        param x = 5
        mesh m = $x + 2
        let result = *m
        let check = result == 7
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

#[test]
fn test_stateful_sub() {
    let r = run_anim(
        "
        param x = 10
        mesh m = $x - 3
        let result = *m
        let check = result == 7
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

#[test]
fn test_stateful_mul() {
    let r = run_anim(
        "
        param x = 6
        mesh m = $x * 4
        let result = *m
        let check = result == 24
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

#[test]
fn test_stateful_two_param_add() {
    let r = run_anim(
        "
        param x = 3
        param y = 4
        mesh m = $x + $y
        let result = *m
        let check = result == 7
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

#[test]
fn test_stateful_comparison_lt() {
    let r = run_anim(
        "
        param x = 3
        mesh m = $x < 10
        let result = *m
        let check = result == 1
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

// ── stateful unary runtime errors ────────────────────────────────────────────

#[test]
fn test_stateful_negate() {
    let r = run_anim(
        "
        param x = 5
        mesh m = -$x
        let result = *m
        let check = result == -5
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

#[test]
fn test_stateful_double_negate() {
    let r = run_anim(
        "
        param x = 8
        mesh m = -(-$x)
        let result = *m
        let check = result == 8
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

// ── subscript runtime errors ─────────────────────────────────────────────────

#[test]
fn test_stateful_subscript_list_param() {
    let r = run_anim(
        "
        param xs = [10, 20, 30]
        mesh m = $xs[1]
        let result = *m
        let check = result == 20
    ",
    );
    r.assert_error("subscript cannot be applied to stateful values");
}

// ── stateful not runtime errors ──────────────────────────────────────────────

#[test]
fn test_stateful_not_false() {
    let r = run_anim(
        "
        param x = 0
        mesh m =  not $x
        let result = *m
        let check = result == 1
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

#[test]
fn test_stateful_not_truthy() {
    let r = run_anim(
        "
        param x = 5
        mesh m = not $x
        let result = *m
        let check = result == 0
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

// ── error cases ───────────────────────────────────────────────────────────────

#[test]
fn test_stateful_assigned_to_var_is_error() {
    let r = run_anim(
        "
        param x = 1
        var v = $x
    ",
    );
    r.assert_error("illegal assignment of stateful value");
}

#[test]
fn test_stateful_assigned_to_let_is_error() {
    let r = run_anim(
        "
        param x = 1
        let v = $x
    ",
    );
    r.assert_error("illegal assignment of stateful value. Stateful values must only be assigned to meshes");
}

#[test]
fn test_stateful_assigned_to_param_is_error() {
    let r = run_anim(
        "
        param x = 1
        param y = $x
    ",
    );
    r.assert_error("stateful values can only be assigned to mesh variables");
}

#[test]
fn test_dollar_on_mesh_is_error() {
    // $ must be used on param, not mesh
    let r = run_anim(
        "
        mesh m = [1, 2, 3]
        mesh n = $m
    ",
    );
    r.assert_error("'param'");
}

// ── stateful + animation interaction ─────────────────────────────────────────

#[test]
fn test_stateful_dereference_after_set_animation() {
    // after a Set animation x follower == x leader == 20;
    // *m should evaluate $x and return 20
    let r = run_anim_with_stdlib(
        "
        param x = 5
        mesh m = $x
        x = 20
        play Set([&x])
    ",
    );
    r.assert_ok();
    r.param_leaders()[2]
        .assert_target_int(20)
        .assert_current_int(20);
}

#[test]
fn test_stateful_dereference_after_lerp_uses_param_leader() {
    // code-side dereference should read param leaders, even while the follower is mid-lerp
    let r = run_anim_with_stdlib_at(
        "
        param x = 0
        mesh m = 0
        mesh leader_value = 0
        play Set()
        m = $x
        play Set()
        x = 5
        leader_value = *m
        play Lerp(2)
    ",
        1.0,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(5)
        .assert_current_float(2.5, 1e-9);
    assert_mesh_target_int(&r.leaders, 1, 5);
}

#[test]
fn test_stateful_function() {
    // code-side dereference should read param leaders, even while the follower is mid-lerp
    let r = run_anim_with_stdlib_at(
        "
        param x = 0
        mesh m = $x
        mesh leader_value = 0
        play Set()
        x = 5
        leader_value = *m
        play Lerp(2)
    ",
        1.0,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(5)
        .assert_current_float(2.5, 1e-9);
    assert_mesh_target_int(&r.leaders, 1, 5);
}

#[test]
fn test_stateful_add_after_set_animation() {
    let r = run_anim_with_stdlib(
        "
        param x = 5
        mesh m = $x + 10
        x = 20
        play Set([&x])
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

// ── compound stateful runtime errors ─────────────────────────────────────────

#[test]
fn test_stateful_arithmetic_chain() {
    let r = run_anim(
        "
        param x = 3
        mesh m = ($x + 1) * 2
        let result = *m
        let check = result == 8
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

#[test]
fn test_stateful_logical_and() {
    let r = run_anim(
        "
        param x = 1
        param y = 0
        mesh m = $x and $y
        let result = *m
    ",
    );
    r.assert_error("stateful has no truthiness");
}

#[test]
fn test_stateful_logical_or() {
    let r = run_anim(
        "
        param x = 0
        param y = 4
        mesh m = $x or $y
        let result = *m
    ",
    );
    r.assert_error("stateful has no truthiness");
}

#[test]
fn test_naked_param_read_is_compile_error() {
    let r = run_anim(
        "
        param x = 1
        let y = x
    ",
    );
    r.assert_error("cannot read param 'x' directly");
}

#[test]
fn test_stateful_mixed_constant_and_param() {
    let r = run_anim(
        "
        param x = 30
        mesh m = 100 - $x
        let result = *m
        let check = result == 70
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}

#[test]
fn test_stateful_two_params_arithmetic() {
    let r = run_anim(
        "
        param a = 3
        param b = 4
        param c = 5
        mesh m = ($a * $b) + $c
        let result = *m
        let check = result == 17
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}
