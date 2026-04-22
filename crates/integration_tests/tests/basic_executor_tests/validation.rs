use super::*;

#[test]
fn test_exec_block_expression() {
    let r = run("
        let result = block {
            let tmp = 6
            return tmp * 7
        }
    ");
    r.assert_int(42);
}

#[test]
fn test_exec_block_with_intermediate_vars() {
    let r = run("
        let result = block {
            let a = 3
            let b = 4
            let hyp_sq = a * a + b * b
            return hyp_sq
        }
    ");
    r.assert_int(25);
}

// -- error cases --

#[test]
fn test_exec_compile_error_undefined_var() {
    let r = run("let x = undefined_var");
    r.assert_error("undefined");
}

#[test]
fn test_exec_compile_error_mutate_let() {
    let r = run("
        let x = 5
        x = 10
    ");
    r.assert_error("cannot mutate");
}

#[test]
fn test_exec_runtime_error_div_zero() {
    let r = run("let x = 1 / 0");
    r.assert_error("division by zero");
}

#[test]
fn test_exec_runtime_error_float_div_zero() {
    let r = run("let x = 1.0 / 0");
    r.assert_error("division by zero");
}

#[test]
fn test_exec_runtime_error_index_out_of_bounds() {
    let r = run("
        let xs = [1, 2, 3]
        let result = xs[5]
    ");
    r.assert_error("out of bounds");
}

#[test]
fn test_exec_for_non_list_runtime_error_uses_container_span() {
    let src = "
        for (i in 7 + 8) {
            let result = i
        }
    ";
    let r = run(src);
    r.assert_error("list");

    let start = src.find("7 + 8").expect("missing container expression");
    r.assert_first_error_span(start..start + "7 + 8".len());
}

#[test]
fn test_exec_runtime_error_type_in_arithmetic() {
    let r = run(r#"let x = "hello" - 1"#);
    r.assert_error("unsupported");
}

#[test]
fn test_exec_runtime_error_list_add_length_mismatch() {
    let r = run("
        let x = [[1], [2, 3]] + [[4], [5]]
    ");
    r.assert_error("different lengths");
}

#[test]
fn test_exec_runtime_error_list_sub_length_mismatch() {
    let r = run("
        let x = [[1], [2, 3]] - [[4], [5]]
    ");
    r.assert_error("different lengths");
}

#[test]
fn test_exec_runtime_error_list_scalar_multiply_bad_element() {
    let r = run(r#"
        let x = 2 * [1, "hello"]
    "#);
    r.assert_error("list element [1]");
    r.assert_error("unsupported binary op * on int and string");
}

#[test]
fn test_exec_runtime_error_list_negate_bad_element() {
    let r = run(r#"
        let x = -[1, "hello"]
    "#);
    r.assert_error("cannot negate list element [1]");
    r.assert_error("cannot negate string");
}

#[test]
fn test_exec_runtime_error_subscript_non_collection() {
    let r = run("
        let x = 42
        let y = x[0]
    ");
    r.assert_error("subscript");
}

// -- stack overflow --

#[test]
fn test_stack_overflow_infinite_recursion() {
    // inf captures itself via a var lvalue and recurses indefinitely
    let r = run("
        let inf = |inf| inf(inf)
        inf(inf)
    ");
    r.assert_error("stack overflow");
}

#[test]
fn test_stack_overflow_mutual_recursion() {
    // a calls b calls a calls b ...
    let r = run("
        let a = |a, b| b(a, b)
        let b = |a, b| a(a, b)
        a(a, b)
    ");
    r.assert_error("stack overflow");
}

// -- Play / Return context restrictions (compile-time) --

#[test]
fn test_compile_error_play_in_lambda() {
    let r = run("
        let f = |x| {
            play 0
        }
    ");
    r.assert_error("anim body");
}

#[test]
fn test_compile_error_play_in_block() {
    let r = run("
        let result = block {
            play 0
            return 1
        }
    ");
    r.assert_error("anim body");
}

#[test]
fn test_compile_error_return_at_top_level() {
    let r = run("
        let x = 5
        return x
    ");
    r.assert_error("lambda or block");
}

// -- references --
