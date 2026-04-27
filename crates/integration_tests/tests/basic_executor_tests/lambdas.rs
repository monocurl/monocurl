use super::*;

// -- lambdas --

#[test]
fn test_exec_lambda_call() {
    let r = run("
        let double = |n| n * 2
        let x = double(21)
    ");
    r.assert_int(42);
}

#[test]
fn test_exec_lambda_multi_arg() {
    let r = run("
        let add = |a, b| a + b
        let result = add(17, 25)
    ");
    r.assert_int(42);
}

#[test]
fn test_exec_lambda_block_body() {
    let r = run("
        let max = |a, b| {
            var result = b
            if (a > b) {
                result = a
            }
            return result
        }
        let result = max(7, 3)
    ");
    r.assert_int(7);
}

#[test]
fn test_exec_lambda_default_arg_omitted() {
    let r = run("
        let add = |x, y = 10| x + y
        let result = add(5)
    ");
    r.assert_int(15);
}

#[test]
fn test_exec_lambda_default_arg_overridden() {
    let r = run("
        let add = |x, y = 10| x + y
        let result = add(5, 20)
    ");
    r.assert_int(25);
}

#[test]
fn test_exec_default_arg_call_is_live_function() {
    let r = run_with_stdlib(
        "
        let func = |x = 1| x
        let result = is_live_function(func())
    ",
        &["util"],
    );
    r.assert_int(1);
}

#[test]
fn test_exec_set_default_updates_unlabeled_default() {
    let r = run_with_stdlib(
        "
        let func = |x = 1| x
        let result = set_default{\"x\", 4} func()
    ",
        &["util"],
    );
    r.assert_int(4);
}

#[test]
fn test_exec_set_defaults_updates_map_values() {
    let r = run_with_stdlib(
        "
        let func = |x = 1, y = 2| x * 10 + y
        let result = set_defaults{[\"x\" -> 4, \"y\" -> 5]} func()
    ",
        &["util"],
    );
    r.assert_int(45);
}

#[test]
fn test_exec_set_default_interpolates_default_value() {
    let r = run_with_stdlib(
        "
        let func = |x = 1| x
        let result = lerp(func(), set_default{\"x\", 5} func(), 0.25) + 0
    ",
        &["util", "math"],
    );
    r.assert_float(2.0);
}

#[test]
fn test_exec_set_defaults_interpolates_map_values() {
    let r = run_with_stdlib(
        "
        let func = |x = 1, y = 10| x * 100 + y
        let result = lerp(func(), set_defaults{[\"x\" -> 5, \"y\" -> 20]} func(), 0.5) + 0
    ",
        &["util", "math"],
    );
    r.assert_float(315.0);
}

#[test]
fn test_exec_set_default_interpolates_through_live_operator_chain() {
    let r = run_with_stdlib(
        "
        let add = operator |target, delta| [target, target + delta]
        let func = |x = 1| x
        let before = add{3} func()
        let after = set_default{\"x\", 5} add{3} func()
        let result = lerp(before, after, 0.5) + 0
    ",
        &["util", "math"],
    );
    r.assert_float(6.0);
}

#[test]
fn test_exec_set_default_traverses_live_operator_chain() {
    let r = run_with_stdlib(
        "
        let add = operator |target, delta| [target, target + delta]
        let func = |x = 1| x
        let result = set_default{\"x\", 4} add{3} func()
    ",
        &["util"],
    );
    r.assert_int(7);
}

#[test]
fn test_exec_get_defaults_lists_default_arg_names() {
    let r = run_with_stdlib(
        "
        let func = |required, x = 1, y = 2| required + x + y
        let result = get_defaults(func(0))
    ",
        &["util"],
    );
    r.assert_string_list(&["x", "y"]);
}

#[test]
fn test_exec_set_default_errors_without_live_function() {
    let r = run_with_stdlib(
        "
        let result = set_default{\"x\", 4} 1
    ",
        &["util"],
    );
    r.assert_error("live function");
}

#[test]
fn test_exec_set_default_errors_for_unknown_default() {
    let r = run_with_stdlib(
        "
        let func = |x = 1| x
        let result = set_default{\"missing\", 4} func()
    ",
        &["util"],
    );
    r.assert_error("no default argument");
}

#[test]
fn test_exec_reference_default_arg_omitted() {
    let r = run("
        let base = 7
        let read = |&x = base| x + 1
        let result = read()
    ");
    r.assert_int(8);
}

#[test]
fn test_exec_reference_default_arg_mutation_is_error() {
    let r = run("
        let base = 7
        let write = |&x = base| {
            x = 1
            return []
        }
        write()
    ");
    r.assert_error("cannot assign");
}

#[test]
fn test_exec_mod_preserves_integer_result_for_integer_inputs() {
    let r = run_section(
        "
        let result = __monocurl__native__ mod_func(7, 3)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_int(1);
}

#[test]
fn test_exec_min_preserves_integer_result_for_integer_inputs() {
    let r = run_section(
        "
        let result = __monocurl__native__ min(7, 3)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_int(3);
}

#[test]
fn test_exec_max_promotes_only_when_needed() {
    let r = run_section(
        "
        let result = __monocurl__native__ max(7, 3.5)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(7.0);
}

#[test]
fn test_exec_assignment_chain_uses_assigned_value() {
    let r = run("
        var a = 0
        var b = 0
        a = b = 3
        let result = [a, b]
    ");
    r.assert_int_list(&[3, 3]);
}

// -- default value free-variable restrictions --

#[test]
fn test_default_lvalue_ref_param_is_error() {
    // &y in a default is always banned, even when y is param
    let r = run("
        param y = 4
        let gamma = |x = &y| x
        let result = gamma()
    ");
    r.assert_error("lvalue reference");
}

#[test]
fn test_default_lvalue_ref_let_is_error() {
    let r = run("
        let base = 10
        let f = |x = &base| x
        let result = f()
    ");
    r.assert_error("lvalue reference");
}

#[test]
fn test_default_references_let_is_error() {
    // plain let variable in a default is not mesh or param
    let r = run("
        let base = 10
        let f = |x = &base| x
        let result = f()
    ");
    r.assert_error("mesh or param");
}

#[test]
fn test_default_references_var_is_error() {
    let r = run("
        var count = 5
        let f = |x = &count| x
        let result = f()
    ");
    r.assert_error("mesh or param");
}

#[test]
fn test_default_references_param_is_ok() {
    let r = run("
        param scale = 3
        let f = |x = scale| x * 2
        let result = f()
    ");
    r.assert_int(6);
}

#[test]
fn test_default_references_mesh_is_ok() {
    let r = run("
        let make = |v| v
        mesh m = make(v: 7)
        let f = |x = m| x + 1
        let result = f()
    ");
    r.assert_int(8);
}

#[test]
fn test_default_literal_only_is_ok() {
    // no free variables at all — always fine
    let r = run("
        let f = |x = 42| x
        let result = f()
    ");
    r.assert_int(42);
}

#[test]
fn test_exec_closure_captures_let() {
    let r = run("
        let base = 100
        let add_base = |n| n + base
        let result = add_base(42)
    ");
    r.assert_int(142);
}

#[test]
fn test_exec_higher_order_function() {
    let r = run("
        let apply = |f, x| f(x)
        let triple = |n| n * 3
        let result = apply(triple, 14)
    ");
    r.assert_int(42);
}

#[test]
fn test_exec_lambda_returns_lambda() {
    let r = run("
        let make_adder = |n| {
            return |x| x + n
        }
        let add5 = make_adder(5)
        let result = add5(37)
    ");
    r.assert_int(42);
}

#[test]
fn test_exec_lambda_by_value_preserves_passed_reference() {
    let r = run("
        param x = 1
        let passthrough = |a| a
        let set = |&y| {
            y = 7
            return []
        }
        set(passthrough(&x))
        let result = x
    ");
    r.assert_int(7);
}

#[test]
fn test_exec_lambda_capture_preserves_passed_reference() {
    let r = run("
        param x = 1
        let passthrough = |a| {
            let inner = || a
            return inner()
        }
        let set = |&y| {
            y = 7
            return []
        }
        set(passthrough(&x))
        let result = x
    ");
    r.assert_int(7);
}

#[test]
fn test_unused_block() {
    let r = run("
        var gamma = |lambda| { return lambda }
        let g = block {
            return 2 + 5
        }
        let g = gamma(|x| x)(7)
    ");
    r.assert_int(7);
}

#[test]
fn test_used_block() {
    let r = run("
        let x = block {
            var a = 2
            var b = 7
            return a + b
        }
    ");
    r.assert_int(9);
}

// -- collections: lists --
