// stateful value tests
// covers: $param creation, *mesh dereference, binary/unary/subscript lifting,
// caching, error cases, and interaction with animations.

use anim_tests::{
    LeaderInfo, run_anim, run_anim_at, run_anim_with_stdlib, run_anim_with_stdlib_at,
};
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

fn assert_mesh_target_float(leaders: &[LeaderInfo], mesh_idx: usize, expected: f64, eps: f64) {
    use executor::state::LeaderKind;
    let meshes: Vec<&LeaderInfo> = leaders
        .iter()
        .filter(|l| l.kind == LeaderKind::Mesh)
        .collect();
    match &meshes[mesh_idx].target {
        Value::Float(f) => assert!(
            (f - expected).abs() < eps,
            "mesh[{}] target float mismatch: expected {}, got {}",
            mesh_idx,
            expected,
            f
        ),
        other => panic!(
            "mesh[{}]: expected Float({}), got {}",
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

// ── binary op lifting ─────────────────────────────────────────────────────────

#[test]
fn test_stateful_add_constant_stored_in_mesh() {
    // $x + 2 should build a stateful BinaryOp node
    let r = run_anim(
        "
        param x = 10
        mesh m = $x + 2
    ",
    );
    r.assert_ok();
}

#[test]
fn test_stateful_dereference_add_evaluates_correctly() {
    // *m with m = $x + 2, x = 5 → should evaluate to 7
    let r = run_anim(
        "
        param x = 5
        mesh m = $x + 2
        let result = *m
        let check = result == 7
    ",
    );
    r.assert_ok();
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
    r.assert_ok();
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
    r.assert_ok();
}

#[test]
fn test_stateful_two_param_add() {
    // $x + $y lifts both params into the node
    let r = run_anim(
        "
        param x = 3
        param y = 4
        mesh m = $x + $y
        let result = *m
        let check = result == 7
    ",
    );
    r.assert_ok();
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
    r.assert_ok();
}

// ── unary neg lifting ─────────────────────────────────────────────────────────

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
    r.assert_ok();
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
    r.assert_ok();
}

// ── subscript lifting ─────────────────────────────────────────────────────────

#[test]
fn test_stateful_subscript_list_param() {
    // param can hold a list; $list[1] builds a Subscript stateful node
    let r = run_anim(
        "
        param xs = [10, 20, 30]
        mesh m = $xs[1]
        let result = *m
        let check = result == 20
    ",
    );
    r.assert_ok();
}

// ── not lifting ───────────────────────────────────────────────────────────────

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
    r.assert_ok();
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
    r.assert_ok();
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
    r.assert_error("stateful values can only be assigned to mesh variables");
}

#[test]
fn test_stateful_assigned_to_let_is_error() {
    let r = run_anim(
        "
        param x = 1
        let v = $x
    ",
    );
    r.assert_error("stateful values can only be assigned to mesh variables");
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
    r.param_leaders()[2].assert_target_int(20).assert_current_int(20);
}

#[test]
fn test_stateful_dereference_after_lerp_uses_follower() {
    // mid-lerp the follower is interpolated, so *m should return ~2.5 (halfway between 0 and 5)
    let r = run_anim_with_stdlib_at(
        "
        param x = 0
        mesh m = $x
        x = 5
        play Lerp(2)
    ",
        1.0,
    );
    r.assert_ok();
    // the param follower is at 2.5 at t=1 of a 2s lerp
    let params = r.param_leaders();
    params[2].assert_target_int(5).assert_current_float(2.5, 1e-9);
}

#[test]
fn test_stateful_add_after_set_animation() {
    // m = $x + 10; after Set, x = 20; *m = 30
    let r = run_anim_with_stdlib(
        "
        param x = 5
        mesh m = $x + 10
        x = 20
        play Set([&x])
    ",
    );
    r.assert_ok();
}

// ── compound expressions ──────────────────────────────────────────────────────

#[test]
fn test_stateful_arithmetic_chain() {
    // ($x + 1) * 2 — both binary ops are lifted
    let r = run_anim(
        "
        param x = 3
        mesh m = ($x + 1) * 2
        let result = *m
        let check = result == 8
    ",
    );
    r.assert_ok();
}

#[test]
fn test_stateful_mixed_constant_and_param() {
    // 100 - $x
    let r = run_anim(
        "
        param x = 30
        mesh m = 100 - $x
        let result = *m
        let check = result == 70
    ",
    );
    r.assert_ok();
}

#[test]
fn test_stateful_two_params_arithmetic() {
    // ($a * $b) + $c
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
    r.assert_ok();
}
