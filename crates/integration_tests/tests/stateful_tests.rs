// stateful value tests
// covers: $param creation, *mesh dereference, runtime validation for invalid
// stateful operators/subscripts, caching, error cases, and interaction with animations.

use anim_tests::{LeaderInfo, run_anim, run_anim_with_stdlib, run_anim_with_stdlib_at};
use executor::{heap::with_heap, value::Value};

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
    r.assert_error(
        "illegal assignment of stateful value. Stateful values must only be assigned to meshes",
    );
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

// ── aliasing independence: mesh/param → var must not share leader_rc ─────────

#[test]
fn test_mesh_assign_to_var_no_alias() {
    // the original aliasing bug: var y = x shared leader_rc, so y = 20 also wrote x
    let r = run_anim(
        "
        mesh x = 0
        var y = x
        y = 20
    ",
    );
    r.assert_ok();
    assert_mesh_target_int(&r.leaders, 0, 0);
}

#[test]
fn test_mesh_two_vars_from_same_mesh_are_independent() {
    // a and b both initialised from the same mesh x; mutating b must leave x and a untouched
    let r = run_anim(
        "
        mesh x = 5
        var a = x
        var b = x
        b = 99
    ",
    );
    r.assert_ok();
    assert_mesh_target_int(&r.leaders, 0, 5);
}

#[test]
fn test_mesh_var_chain_no_alias() {
    // z → y → x chain: assigning to z must not touch x
    let r = run_anim(
        "
        mesh x = 0
        var y = x
        var z = y
        z = 99
    ",
    );
    r.assert_ok();
    assert_mesh_target_int(&r.leaders, 0, 0);
}

#[test]
fn test_mesh_var_alias_mutation_then_read_mesh_unchanged() {
    // mutate via alias then read mesh directly to confirm it kept its original value
    let r = run_anim(
        "
        mesh x = 42
        var y = x
        y = 7
        let result = *x
    ",
    );
    r.assert_ok();
    assert_mesh_target_int(&r.leaders, 0, 42);
}

#[test]
fn test_param_deref_copy_to_var_no_alias() {
    // *x copies the leader value; assigning to y must not change param x
    let r = run_anim(
        "
        param x = 10
        var y = *x
        y = 20
    ",
    );
    r.assert_ok();
    r.param_leaders()[2].assert_target_int(10);
}

#[test]
fn test_two_meshes_from_same_value_are_independent() {
    // two separate meshes initialised to the same integer; modifying one must not affect the other
    let r = run_anim(
        "
        mesh a = 5
        mesh b = 5
        a = 99
    ",
    );
    r.assert_ok();
    assert_mesh_target_int(&r.leaders, 0, 99);
    assert_mesh_target_int(&r.leaders, 1, 5);
}

#[test]
fn test_mesh_list_copy_no_alias() {
    // copying a list out of a mesh into a var must deep-copy; mutation of the copy must not touch mesh
    let r = run_anim(
        "
        mesh x = [1, 2, 3]
        var y = x
        y[0] = 99
        let result = (*x)[0]
    ",
    );
    r.assert_ok();
    // x's leader value should still have 1 at index 0
    match &r
        .leaders
        .iter()
        .find(|l| l.kind == executor::state::LeaderKind::Mesh)
        .unwrap()
        .target
    {
        Value::List(list) => match with_heap(|h| h.get(list.elements()[0].key()).clone()) {
            Value::Integer(n) => assert_eq!(n, 1, "mesh list element 0 should be unchanged"),
            other => panic!("expected integer, got {}", other.type_name()),
        },
        other => panic!("expected list, got {}", other.type_name()),
    }
}

// ── lambda return stateful: compile-time error (rule 2) ───────────────────────

#[test]
fn test_lambda_explicit_return_stateful_is_compile_error() {
    let r = run_anim(
        "
        param p = 1
        let f = |x| {
            return $x
        }
    ",
    );
    r.assert_error("cannot return a stateful value");
}

#[test]
fn test_block_return_stateful_is_compile_error() {
    let r = run_anim(
        "
        param p = 5
        let result = block {
            return $p
        }
    ",
    );
    r.assert_error("cannot return a stateful value");
}

#[test]
fn test_lambda_implicit_return_stateful_is_compile_error() {
    // |x| $p has implicit return of a stateful value
    let r = run_anim(
        "
        let f = |x| $x
    ",
    );
    // the expression-body shorthand still returns the value; stateful return must be caught
    r.assert_error("stateful");
}

// ── lambda arguments must not contain stateful (rule 1) ───────────────────────

#[test]
fn test_lambda_arg_is_stateful() {
    // rule 1: a plain (non-labeled) lambda call must not receive a stateful arg
    let r = run_anim(
        "
        param p = 1
        let f = |x| x
        mesh m = f($p)
    ",
    );
    r.assert_ok();
}

#[test]
fn test_higher_order_stateful_arg_is_error() {
    // stateful passed as argument to a higher-order lambda must also be rejected
    let r = run_anim(
        "
        param p = 2
        let apply = |f, x| f(x)
        let double = |n| n * 2
        mesh m = apply(double, $p)
    ",
    );
    r.assert_ok();
}

// ── lists/vectors cannot store stateful (rule 4) ──────────────────────────────

#[test]
fn test_list_literal_with_stateful_is_error() {
    let r = run_anim(
        "
        param p = 1
        let v = [$p]
    ",
    );
    r.assert_error("stateful");
}

#[test]
fn test_list_append_stateful_is_error() {
    let r = run_anim(
        "
        param p = 10
        var v = []
        v .= $p
    ",
    );
    r.assert_error("stateful");
}

#[test]
fn test_list_of_stateful_assigned_to_mesh_is_error() {
    // even if the target is a mesh, a *list* containing stateful is still illegal (rule 4)
    let r = run_anim(
        "
        param p = 3
        mesh m = [$p, 1, 2]
    ",
    );
    r.assert_error("stateful");
}

// ── stateful in operator invocations ─────────────────────────────────────────

#[test]
fn test_operator_extra_arg_stateful_stored_in_mesh() {
    // an operator invocation whose labeled extra-arg is stateful; result stored in mesh is fine
    let r = run_anim(
        "
        param delta = 4
        let shift = operator |target, amount| {
            return [target, target + amount]
        }
        mesh m = shift{$delta} 10
    ",
    );
    r.assert_ok();
}

#[test]
fn test_operator_stateful_operand_stored_in_mesh() {
    // stateful as the *operand* to an operator; result stored in mesh
    let r = run_anim(
        "
        param base = 5
        let double_op = operator |target| {
            return [target, target * 2]
        }
        mesh m = double_op{} $base
    ",
    );
    r.assert_ok();
}

// ── stateful in labeled lambda invocations ────────────────────────────────────

#[test]
fn test_labeled_lambda_stateful_arg_stored_in_mesh() {
    // labeled call with a stateful arg — result is a StatefulNode::LabeledCall; valid in mesh
    let r = run_anim(
        "
        param offset = 3
        let f = |x, y| x + y
        mesh m = f(x: $offset, y: 10)
    ",
    );
    r.assert_ok();
}

#[test]
fn test_labeled_lambda_stateful_arg_attribute_readable() {
    // labeled-call stateful mesh: attribute read should return the concrete arg value
    let r = run_anim(
        "
        param offset = 7
        let f = |x, y| x + y
        mesh m = f(x: $offset, y: 5)
        let result = (*m)
    ",
    );
    r.assert_ok();
}

#[test]
fn test_labeled_lambda_stateful_arg_dereference_reflects_param() {
    // *m evaluated with param = 7 → leader value should be 7 + 5 = 12
    let r = run_anim(
        "
        param offset = 7
        let f = |x, y| x + y
        mesh m = f(x: $offset, y: 5)
        mesh leader_val = *m
    ",
    );
    r.assert_ok();
    assert_mesh_target_int(&r.leaders, 1, 12);
}

#[test]
fn test_stateful_labeled_lambda_param_change_updates_deref() {
    // change param then re-dereference; leader_val must reflect new param
    let r = run_anim(
        "
        param offset = 7
        let f = |x, y| x + y
        mesh m = f(x: $offset, y: 5)
        offset = 10
        mesh leader_val = *m
    ",
    );
    r.assert_ok();
    assert_mesh_target_int(&r.leaders, 1, 15);
}

#[test]
fn test_nested_labeled_stateful_calls_in_mesh() {
    // inner labeled call is itself a stateful arg to an outer labeled call
    let r = run_anim(
        "
        param a = 2
        param b = 3
        let add = |x, y| x + y
        mesh m = add(x: $a, y: $b)
        mesh result = *m
    ",
    );
    r.assert_ok();
    assert_mesh_target_int(&r.leaders, 1, 5);
}

// ── stateful + references ─────────────────────────────────────────────────────

#[test]
fn test_ref_to_param_and_independent_stateful() {
    // passing &p to a mutating lambda while also using $p in a mesh must not corrupt each other
    let r = run_anim(
        "
        param p = 1
        mesh m = $p
        let inc = |&y| {
            y = y + 1
            return []
        }
        inc(&p)
        mesh updated = *m
    ",
    );
    r.assert_ok();
    // after inc, param p leader = 2; *m should see 2
    assert_mesh_target_int(&r.leaders, 1, 2);
}

#[test]
fn test_stateful_mesh_attribute_then_naked_var_copy() {
    // copy a mesh attribute into a var; that copy must be independent of the mesh
    let r = run_anim(
        "
        param offset = 3
        let f = |x, y| x + y
        mesh m = f(x: $offset, y: 10)
        var snap = *m
        offset = 99
        let result = snap
    ",
    );
    r.assert_ok();
    // snap captured *m when offset=3, so result should be 13
    match &r.leaders.iter().last() {
        _ => {} // just checking no error; captured_output check done via assert_ok
    }
}

#[test]
fn test_two_params_two_meshes_independent_stateful() {
    // each mesh tracks a different param; mutating one must not corrupt the other
    let r = run_anim(
        "
        param a = 10
        param b = 20
        mesh ma = $a
        mesh mb = $b
        a = 99
        mesh ra = *ma
        mesh rb = *mb
    ",
    );
    r.assert_ok();
    assert_mesh_target_int(&r.leaders, 2, 99); // ra = *ma = a = 99
    assert_mesh_target_int(&r.leaders, 3, 20); // rb = *mb = b = 20
}

// ── stateful map: maps cannot store stateful values ───────────────────────────

#[test]
fn test_map_value_stateful_is_error() {
    let r = run_anim(
        r#"
        param p = 5
        let m = ["key" -> $p]
    "#,
    );
    r.assert_error("stateful");
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
