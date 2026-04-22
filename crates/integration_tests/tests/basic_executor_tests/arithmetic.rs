use super::*;

// -- literals and arithmetic --

#[test]
fn test_exec_literal_int() {
    let r = run("let x = 42");
    r.assert_int(42);
}

#[test]
fn test_exec_literal_nil() {
    let r = run("let x = nil");
    r.assert_nil();
}

#[test]
fn test_block_lambda_fallthrough_is_runtime_error() {
    let src = "
        let f = |x| {
            let y = x + 1
        }
        let z = f(1)
    ";
    let r = run(src);
    let start = src.find("|x|").expect("missing lambda");
    let end = src[start..]
        .find('}')
        .map(|offset| start + offset + 1)
        .expect("missing lambda end");
    r.assert_error("lambda reached end without explicit return");
    r.assert_first_error_span(start..end);
}

#[test]
fn test_empty_block_expression_returns_empty_vector() {
    let r = run("let result = block {}");
    r.assert_int_list(&[]);
}

#[test]
fn test_eager_lambda_too_few_arguments_is_runtime_error() {
    let r = run("
        let f = |x, y| x
        let z = f(1)
        ");
    r.assert_error("too few positional arguments");
}

#[test]
fn test_exec_literal_float() {
    let r = run("let x = 3.14");
    r.assert_float(3.14);
}

#[test]
fn test_exec_literal_negative() {
    let r = run("let x = ---7");
    r.assert_int(-7);
}

#[test]
fn test_exec_arithmetic_precedence() {
    // 2 + 3 * 4 = 14, not 20
    let r = run("let x = 2 + 3 * 4");
    r.assert_int(14);
}

#[test]
fn test_exec_arithmetic_parens() {
    let r = run("let x = (2 + 3) * 4");
    r.assert_int(20);
}

#[test]
fn test_exec_int_division_gives_float() {
    // int / int should give float
    let r = run("let x = 7 / 2");
    r.assert_float(3.5);
}

#[test]
fn test_exec_integer_division() {
    let r = run("let x = 7 // 2");
    r.assert_int(3);
}

#[test]
fn test_exec_float_integer_division_returns_int() {
    let r = run("let x = 7.5 // 2");
    r.assert_int(3);
}

#[test]
fn test_exec_power() {
    let r = run("let x = 2 ^ 10");
    r.assert_float(1024.0);
}

#[test]
fn test_exec_subtraction() {
    let r = run("let x = 100 - 58");
    r.assert_int(42);
}

#[test]
fn test_exec_unary_negate_float() {
    let r = run("let x = -(1.5 + 0.5)");
    r.assert_float(-2.0);
}

#[test]
fn test_exec_scalar_multiply_nested_list() {
    let r = run("
        let rows = 2 * [[1, 2], [3, 4]]
        let result = rows[1]
    ");
    r.assert_int_list(&[6, 8]);
}

#[test]
fn test_exec_nested_list_addition() {
    let r = run("
        let rows = [[1, 2], [3, 4]] + [[10, 20], [30, 40]]
        let result = rows[0][1] + rows[1][0]
    ");
    r.assert_int(55);
}

#[test]
fn test_exec_nested_list_subtraction() {
    let r = run("
        let rows = [[10, 20], [30, 40]] - [[1, 2], [3, 4]]
        let result = rows[0][1] + rows[1][0]
    ");
    r.assert_int(45);
}

#[test]
fn test_exec_negate_nested_list() {
    let r = run("
        let rows = -[[1, 2], [3, 4]]
        let result = rows[0]
    ");
    r.assert_int_list(&[-1, -2]);
}

// -- comparison and equality --

#[test]
fn test_exec_less_than_true() {
    let r = run("let x = 3 < 5");
    r.assert_int(1);
}

#[test]
fn test_exec_less_than_false() {
    let r = run("let x = 5 < 3");
    r.assert_int(0);
}

#[test]
fn test_exec_equal_true() {
    let r = run("let x = 42 == 42");
    r.assert_int(1);
}

#[test]
fn test_exec_equal_false() {
    let r = run("let x = 42 == 43");
    r.assert_int(0);
}

#[test]
fn test_exec_not_equal() {
    let r = run("let x = 1 != 2");
    r.assert_int(1);
}

#[test]
fn test_exec_greater_equal() {
    let r = run("let x = 5 >= 5");
    r.assert_int(1);
}

// -- logical operators --

#[test]
fn test_exec_logical_and_true() {
    let r = run("let x = 1 and 1");
    r.assert_int(1);
}

#[test]
fn test_exec_logical_and_false() {
    let r = run("let x = 1 and 0");
    r.assert_int(0);
}

#[test]
fn test_exec_logical_or_true() {
    let r = run("let x = 0 or 1");
    r.assert_int(1);
}

#[test]
fn test_exec_logical_not() {
    let r = run("let x = not 0");
    r.assert_int(1);
}

#[test]
fn test_exec_short_circuit_and() {
    // right side must not be evaluated when left is falsy; div-by-zero would trigger if it were
    let r = run("let x = 0 and (1 // 0)");
    r.assert_int(0);
}

#[test]
fn test_exec_short_circuit_or() {
    // right side must not be evaluated when left is truthy
    let r = run("let x = 1 or (1 // 0)");
    r.assert_int(1);
}

// -- string --

#[test]
fn test_exec_string_concat() {
    let r = run(r#"let x = "hello" + " " + "world""#);
    r.assert_string("hello world");
}

#[test]
fn test_exec_string_subscript() {
    let r = run(r#"
        let s = "abc"
        let x = s[1]
    "#);
    r.assert_string("b");
}

// -- variables --
