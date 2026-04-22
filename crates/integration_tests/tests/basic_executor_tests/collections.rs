use super::*;

// -- collections: lists --

#[test]
fn test_exec_empty_list() {
    let r = run("let xs = []");
    r.assert_ok();
}

#[test]
fn test_exec_list_literal_subscript() {
    let r = run("
        var xs = [10, 20, 30]
        xs[1] = xs[1] + 5
        let result = xs[1]
    ");
    r.assert_int(25);
}

#[test]
fn test_exec_list_append() {
    let r = run("
        var xs = []
        xs .= 1
        xs .= 2
        xs .= 3
        let result = xs[2]
    ");
    r.assert_int(3);
}

#[test]
fn test_exec_list_mutate_element() {
    let r = run("
        var xs = [1, 2, 3]
        xs[0] = 99
        let result = xs[0]
    ");
    r.assert_int(99);
}

#[test]
fn test_exec_list_in_operator_found() {
    let r = run("
        let xs = [10, 20, 30]
        let result = 20 in xs
    ");
    r.assert_int(1);
}

#[test]
fn test_exec_list_in_operator_not_found() {
    let r = run("
        let xs = [10, 20, 30]
        let result = 99 in xs
    ");
    r.assert_int(0);
}

#[test]
fn test_exec_list_build_with_for() {
    // sum of squares: 1 + 4 + 9 + 16 = 30
    let r = run("
        var sum_sq = 0
        for (i in [1, 2, 3, 4]) {
            sum_sq = sum_sq + i * i
        }
        let result = sum_sq
    ");
    r.assert_int(30);
}

// -- operators --

// -- collections: maps --

#[test]
fn test_exec_map_subscript() {
    let r = run(r#"
        var m = ["b" -> 2]
        m["a"] = 1
        let result = m["a"]
    "#);
    r.assert_int(1);
}

#[test]
fn test_exec_map_insert_and_read() {
    let r = run(r#"
        var m = ["key" -> 42]
        let result = m["key"]
    "#);
    r.assert_int(42);
}

#[test]
fn test_exec_map_in_operator() {
    let r = run(r#"
        var m = [->]
        m["x"] = 10
        m["y"] = 20
        let result = "x" in m
    "#);
    r.assert_int(1);
}

// -- block expressions --

// -- map: hashable key validation --

#[test]
fn test_map_integer_key() {
    let r = run("
        var m = [->]
        m[1] = 100
        let result = m[1]
    ");
    r.assert_int(100);
}

#[test]
fn test_map_string_key() {
    let r = run(r#"
        var m = ["hello" -> 42]
        let result = m["hello"]
    "#);
    r.assert_int(42);
}

#[test]
fn test_map_list_key() {
    // vectors of integers are hashable keys
    let r = run("
        var m = [->]
        m[[1, 2]] = 99
        let result = m[[1, 2]]
    ");
    r.assert_int(99);
}

#[test]
fn test_map_unhashable_key_error() {
    // floats cannot be used as map keys
    let r = run("
        var m = [->]
        m[1.5] = 0
    ");
    r.assert_error("cannot use float as a map key");
}

#[test]
fn test_map_in_operator_integer_key() {
    let r = run("
        var m = [->]
        m[7] = 1
        let result = 7 in m
    ");
    r.assert_int(1);
}

#[test]
fn test_map_in_operator_missing_key() {
    let r = run("
        var m = [->]
        m[1] = 1
        let result = 2 in m
    ");
    r.assert_int(0);
}
