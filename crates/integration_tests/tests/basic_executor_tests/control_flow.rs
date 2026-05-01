use super::*;

#[test]
fn test_exec_var_mutation() {
    let r = run("
        var x = 1
        x = x + 1
        x = x * 3
    ");
    r.assert_int(6);
}

#[test]
fn test_exec_multiple_vars() {
    let r = run("
        let a = 3
        let b = 7
        let result = a * b
    ");
    r.assert_int(21);
}

#[test]
fn test_exec_destructure() {
    let r = run("
        var a = 3
        var b = 7
        var c = 4
        var d = 1
        [a, b] = [b, a] # a = 7, b = 3
        [c, [d, a]] = [a, [b, d]] # c = 7, d = 3, a = 1, b = 3
        let result = a * 1000 + b * 100 + c * 10 + d
    ");
    r.assert_int(1373);
}

// -- if / else --

#[test]
fn test_exec_if_else_true_branch() {
    let r = run("
        var x = 0
        if (1) {
            x = 10
        } else {
            x = 20
        }
    ");
    r.assert_int(10);
}

#[test]
fn test_exec_if_else_false_branch() {
    let r = run("
        var x = 0
        if (0) {
            x = 10
        } else {
            x = 20
        }
    ");
    r.assert_int(20);
}

#[test]
fn test_exec_else_if_chain() {
    let r = run("
        let n = 5
        var result = 0
        if (n < 3) {
            result = 1
        }
        else if (n < 7) {
            result = 2
        }
        else {
            result = 3
        }
    ");
    r.assert_int(2);
}

#[test]
fn test_exec_if_no_else_skipped() {
    let r = run("
        var x = 99
        if (0) {
            x = 0
        }
    ");
    r.assert_int(99);
}

#[test]
fn test_exec_nested_if() {
    let r = run("
        var result = 0
        let a = 1
        let b = 1
        if (a) {
            if (b) {
                result = 42
            }
        }
        let final = result
    ");
    r.assert_int(42);
}

// -- while loop --

#[test]
fn test_exec_while_loop() {
    let r = run("
        var x = 0
        while (x < 5) {
            x = x + 1
        }
    ");
    r.assert_int(5);
}

#[test]
fn test_exec_while_never_entered() {
    let r = run("
        var x = 10
        while (x < 5) {
            x = x + 1
        }
    ");
    r.assert_int(10);
}

#[test]
fn test_exec_while_break() {
    let r = run("
        var x = 0
        while (1) {
            x = x + 1
            if (x >= 3) {
                break
            }
        }
    ");
    r.assert_int(3);
}

#[test]
fn test_exec_while_accumulate() {
    // sum 1..=10
    let r = run("
        var sum = 0
        var i = 1
        while (i <= 10) {
            sum = sum + i
            i = i + 1
        }
        let result = sum
    ");
    r.assert_int(55);
}

// -- for loop --

#[test]
fn test_exec_for_loop_sum() {
    let r = run("
        var sum = 0
        for (i in [1, 2, 3, 4, 5]) {
            sum = sum + i
        }
    ");
    r.assert_int(15);
}

#[test]
fn test_exec_for_loop_empty() {
    let r = run("
        var count = 10
        for (i in []) {
            count = count + 1
        }
    ");
    r.assert_int(10);
}

#[test]
fn test_exec_for_loop_break() {
    let r = run("
        var found = 0
        for (i in [10, 20, 30, 40]) {
            if (i * 2 == 60) {
                found = i
                break
            }
        }
    ");
    r.assert_int(30);
}

#[test]
fn test_exec_for_loop_continue() {
    // sum only even numbers; skip odds via continue
    let r = run("
        var sum = 0
        for (i in [1, 2, 3, 4, 5]) {
            if (i // 2 * 2 != i) {
                continue
            }
            sum = sum + i
        }
    ");
    r.assert_int(6); // 2 + 4
}

#[test]
fn test_exec_nested_loops() {
    let r = run("
        var count = 0
        for (i in [1, 2, 3]) {
            for (j in [1, 2]) {
                count = count + 1
            }
        }
    ");
    r.assert_int(6);
}

#[test]
fn test_exec_for_loop_stdlib_range_sum() {
    let r = run_with_stdlib(
        "
        var sum = 0
        for (i in range(0, 5)) {
            sum = sum + i
        }
        let result = sum
    ",
        &["util"],
    );
    r.assert_int(10);
}

#[test]
fn test_exec_for_loop_shadowed_range_stays_generic() {
    let r = run_with_stdlib(
        "
        let range = |a, b| [10, 20]
        var sum = 0
        for (i in range(0, 5)) {
            sum = sum + i
        }
        let result = sum
    ",
        &["util"],
    );
    r.assert_int(30);
}
