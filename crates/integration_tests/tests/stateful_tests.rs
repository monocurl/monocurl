// stateful value tests
// covers: $param creation, plain reads of stateful meshes, runtime validation for invalid
// stateful operators/subscripts, caching, error cases, and interaction with animations.

use anim_tests::{run_anim, run_anim_with_stdlib, run_anim_with_stdlib_at};

// anim_tests helpers are in a sibling test file; re-export by including it
#[path = "anim_tests.rs"]
mod anim_tests;

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
fn test_stateful_plain_read_equals_follower() {
    // m reads the current value of the stateful expression; with no animations the
    // follower equals the leader value (param initial = 7)
    let r = run_anim(
        "
        param x = 7
        mesh m = $x
        let result = m
    ",
    );
    r.assert_ok();
}

#[test]
fn test_stateful_plain_read_reflects_param_leader_value() {
    // after changing x, m should evaluate to the new follower.
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
fn test_stateful_plain_read_add_evaluates_correctly() {
    let r = run_anim(
        "
        param x = 5
        mesh m = $x + 2
        let result = m
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
        let result = m
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
        let result = m
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
        let result = m
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
        let result = m
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
        let result = m
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
        let result = m
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
        let result = m
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
        let result = m
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
        let result = m
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
fn test_stateful_plain_read_after_set_animation() {
    // after a Set animation x follower == x leader == 20;
    // m should evaluate $x and return 20
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
fn test_stateful_plain_read_after_lerp_uses_param_leader() {
    // code-side plain reads should read param leaders, even while the follower is mid-lerp
    let r = run_anim_with_stdlib_at(
        "
        param x = 0
        mesh m = 0
        mesh leader_value = 0
        play Set()
        m = $x
        play Set()
        x = 5
        leader_value = m
        play Lerp(2)
    ",
        1.0,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(5)
        .assert_current_float(2.5, 1e-9);
    r.assert_mesh_target_int(1, 5);
}

#[test]
fn test_stateful_function() {
    // code-side plain reads should read param leaders, even while the follower is mid-lerp
    let r = run_anim_with_stdlib_at(
        "
        param x = 0
        mesh m = $x
        mesh leader_value = 0
        play Set()
        x = 5
        leader_value = m
        play Lerp(2)
    ",
        1.0,
    );
    r.assert_ok();
    let params = r.param_leaders();
    params[2]
        .assert_target_int(5)
        .assert_current_float(2.5, 1e-9);
    r.assert_mesh_target_int(1, 5);
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
    let r = run_anim(
        "
        mesh x = 0
        var y = x
        y = 20
        print x
    ",
    );
    r.assert_transcript(&["0"]);
}

#[test]
fn test_mesh_two_vars_from_same_mesh_are_independent() {
    let r = run_anim(
        "
        mesh x = 5
        var a = x
        var b = x
        b = 99
        print x
        print a
    ",
    );
    r.assert_transcript(&["5", "5"]);
}

#[test]
fn test_mesh_var_chain_no_alias() {
    let r = run_anim(
        "
        mesh x = 0
        var y = x
        var z = y
        z = 99
        print x
        print y
    ",
    );
    r.assert_transcript(&["0", "0"]);
}

#[test]
fn test_mesh_var_alias_mutation_then_read_mesh_unchanged() {
    let r = run_anim(
        "
        mesh x = 42
        var y = x
        y = 7
        print x
    ",
    );
    r.assert_transcript(&["42"]);
}

#[test]
fn test_param_plain_read_copy_to_var_no_alias() {
    let r = run_anim(
        "
        param x = 10
        var y = x
        y = 20
        print x
    ",
    );
    r.assert_transcript(&["10"]);
}

#[test]
fn test_two_meshes_from_same_value_are_independent() {
    let r = run_anim(
        "
        mesh a = 5
        mesh b = 5
        a = 99
        print a
        print b
    ",
    );
    r.assert_transcript(&["99", "5"]);
}

#[test]
fn test_mesh_list_copy_no_alias() {
    let r = run_anim(
        "
        mesh x = [1, 2, 3]
        var y = x
        y[0] = 99
        print x[0]
    ",
    );
    r.assert_transcript(&["1"]);
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

// ── stateful lambda arguments ─────────────────────────────────────────────────

#[test]
fn test_lambda_arg_is_stateful() {
    // a lambda call marked stateful by the compiler may be stored in a mesh
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
fn test_higher_order_stateful_arg_stored_in_mesh() {
    // stateful passed as argument to a higher-order lambda is captured in the mesh recipe
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
    // even if the target is a mesh, a list containing stateful is still illegal (rule 4)
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
    // stateful as the operand to an operator; result stored in mesh
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

#[test]
fn test_operator_stateful_operand_primes_cache_during_seek() {
    let r = run_anim(
        "
        param base = 5
        let bad = operator |target| {
            return [target, target[0]]
        }
        mesh m = bad{} $base
    ",
    );
    r.assert_error("cannot subscript int");
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
fn test_labeled_lambda_stateful_arg_primes_cache_during_seek() {
    let r = run_anim(
        "
        param offset = 3
        let f = |x| x[0]
        mesh m = f(x: $offset)
    ",
    );
    r.assert_error("cannot subscript int");
}

#[test]
fn test_labeled_lambda_stateful_arg_attribute_readable() {
    // labeled-call stateful mesh: attribute read should return the concrete arg value
    let r = run_anim(
        "
        param offset = 7
        let f = |x, y| x + y
        mesh m = f(x: $offset, y: 5)
        let result = (m)
    ",
    );
    r.assert_ok();
}

#[test]
fn test_labeled_lambda_stateful_arg_plain_read_reflects_param() {
    // m evaluated with param = 7 -> leader value should be 7 + 5 = 12
    let r = run_anim(
        "
        param offset = 7
        let f = |x, y| x + y
        mesh m = f(x: $offset, y: 5)
        mesh leader_val = m
    ",
    );
    r.assert_ok();
    r.assert_mesh_target_int(1, 12);
}

#[test]
fn test_stateful_labeled_lambda_param_change_updates_plain_read() {
    // change param then read again; leader_val must reflect new param
    let r = run_anim(
        "
        param offset = 7
        let f = |x, y| x + y
        mesh m = f(x: $offset, y: 5)
        offset = 10
        mesh leader_val = m
    ",
    );
    r.assert_ok();
    r.assert_mesh_target_int(1, 15);
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
        mesh result = m
    ",
    );
    r.assert_ok();
    r.assert_mesh_target_int(1, 5);
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
        mesh updated = m
    ",
    );
    r.assert_ok();
    // after inc, param p leader = 2; m should see 2
    r.assert_mesh_target_int(1, 2);
}

#[test]
fn test_stateful_assignment_through_reference_vector_retained_mesh_lvalue() {
    let r = run_anim(
        "
        param driver = 4
        param scalar = 1
        mesh target = 0

        let bump = |&slot, amount| {
            slot = slot + amount
            return []
        }

        bump(&scalar, 2)
        target = $driver
        driver = 9

        mesh observed = target
        mesh scalar_snapshot = scalar
    ",
    );
    r.assert_ok();
    r.assert_mesh_target_int(1, 9);
    r.assert_mesh_target_int(2, 3);
}

#[test]
fn test_stateful_mesh_plain_read_then_var_copy() {
    // copy a stateful mesh into a var; that copy must be independent of later param changes
    let r = run_anim(
        "
        param offset = 3
        let f = |x, y| x + y
        mesh m = f(x: $offset, y: 10)
        var snap = m
        offset = 99
        let result = snap
        print result
    ",
    );
    r.assert_transcript(&["13"]);
}

#[test]
fn test_stateful_mesh_plain_read_passed_to_non_stateful_lambda() {
    let r = run_anim(
        "
        param value = 4
        let id = |x| x
        mesh m = id(v: $value)
        let result = id(m)
        print result
    ",
    );
    r.assert_transcript(&["4"]);
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
        mesh ra = ma
        mesh rb = mb
    ",
    );
    r.assert_ok();
    r.assert_mesh_target_int(2, 99); // ra = ma = a = 99
    r.assert_mesh_target_int(3, 20); // rb = mb = b = 20
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
        let result = m
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
        mesh result = m
    ",
    );
    r.assert_ok();
    r.assert_mesh_target_int(1, 0);
}

#[test]
fn test_stateful_logical_or() {
    let r = run_anim(
        "
        param x = 0
        param y = 4
        mesh m = $x or $y
        mesh result = m
    ",
    );
    r.assert_ok();
    r.assert_mesh_target_int(1, 4);
}

#[test]
fn test_naked_param_read_uses_current_value() {
    let r = run_anim(
        "
        param x = 1
        let y = x
        print y
    ",
    );
    r.assert_transcript(&["1"]);
}

#[test]
fn test_stateful_mixed_constant_and_param() {
    let r = run_anim(
        "
        param x = 30
        mesh m = 100 - $x
        let result = m
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
        let result = m
        let check = result == 17
    ",
    );
    r.assert_error("operators cannot be applied to stateful values");
}
