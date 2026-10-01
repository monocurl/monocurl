use super::{run, run_section, run_with_stdlib};

use parser::ast::SectionType;

#[test]
fn labeled_arguments_out_of_order_bind_by_name() {
    let r = run("
        let f = |a, b, c| [a, b, c]
        print f(c: 3, a: 1, b: 2)
    ");
    r.assert_transcript(&["[1, 2, 3]"]);
}

#[test]
fn labeled_arguments_skip_a_defaulted_parameter() {
    let r = run("
        let f = |a = 1, b = 2, c = nil, d = [1, 1, 1]| [a, b, c, d]
        print f(a: 5, b: 6, d: [9, 9, 9])
        print f(5, 6, d: [9, 9, 9])
    ");
    r.assert_transcript(&["[5, 6, nil, [9, 9, 9]]", "[5, 6, nil, [9, 9, 9]]"]);
}

#[test]
fn positional_then_labeled_arguments() {
    let r = run("
        let f = |a, b, c = 3, d = 4, e = 5| [a, b, c, d, e]
        print f(1, 2, e: 50)
        print f(1, 2, 30, e: 50)
    ");
    r.assert_transcript(&["[1, 2, 3, 4, 50]", "[1, 2, 30, 4, 50]"]);
}

#[test]
fn labeled_required_argument_out_of_order() {
    let r = run("
        let f = |a, b, c = 0| [a, b, c]
        print f(b: 2, 1)
        print f(c: 3, b: 2, a: 1)
    ");
    r.assert_transcript(&["[1, 2, 0]", "[1, 2, 3]"]);
}

#[test]
fn labeled_argument_skipping_a_required_parameter_errors() {
    let r = run("
        let f = |a, b, c = 0| [a, b, c]
        print f(a: 1, c: 3)
    ");
    r.assert_error("too few");
}

#[test]
fn label_naming_no_parameter_is_a_positional_alias() {
    let r = run("
        let f = |a, b = 2, c = 3, d = 4| [a, b, c, d]
        var call = f(1, z: 5, d: 9)
        print call
        call.z = 6
        print call
    ");
    r.assert_transcript(&["[1, 5, 3, 9]", "[1, 6, 3, 9]"]);
}

#[test]
fn label_given_twice_errors() {
    let r = run("
        let f = |a = 1, b = 2| [a, b]
        print f(b: 3, b: 4)
    ");
    r.assert_error("given more than once");
}

#[test]
fn labeled_operator_call_skips_a_default() {
    let r = run("
        let op = operator |target, a = 1, b = 2, c = 3| [target, [target, a, b, c]]
        let inv = op{c: 30, a: 10} 0
        print inv
        print inv.c
        print inv.a
    ");
    r.assert_transcript(&["[0, 10, 2, 30]", "30", "10"]);
}

#[test]
fn editing_live_call_by_name_after_skipped_default() {
    let r = run("
        let f = |a = 1, b = 2, c = nil, d = [1, 1, 1]| [a, b, c, d]
        var call = f(a: 5, d: [9, 9, 9])
        call.d = [7]
        print call
        print call.d
        call.a = 6
        print call
    ");
    r.assert_transcript(&["[5, 2, nil, [7]]", "[7]", "[6, 2, nil, [7]]"]);
}

#[test]
fn editing_live_operator_call_by_name_after_skipped_default() {
    let r = run("
        let op = operator |target, a = 1, b = 2, c = 3| [target, [target, a, b, c]]
        var inv = op{c: 30} 0
        inv.c = 40
        print inv
    ");
    r.assert_transcript(&["[0, 1, 2, 40]"]);
}

#[test]
fn lerp_matches_live_calls_labeled_in_different_orders() {
    let r = run_section(
        "
        let f = |a = 0, b = 7, c = 0| a + 10 * b + 100 * c
        let result = __monocurl__native__ lerp(f(c: 0, a: 0), f(a: 2, c: 4), 0.5) + 0
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(271.0);
}

#[test]
fn lerp_matches_live_operator_calls_labeled_in_different_orders() {
    let r = run_section(
        "
        let op = operator |target, a = 0, b = 7, c = 0| [target, target + a + 10 * b + 100 * c]
        let result = __monocurl__native__ lerp(op{c: 0, a: 0} 0, op{a: 2, c: 4} 0, 0.5) + 0
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(271.0);
}

#[test]
fn axis3d_label_up_skips_grid_color() {
    let r = run_with_stdlib(
        "
        var axis = Axis3d(basis: [1r, 1u, 1b], color: GRAY, label_up: [2r, 2u, 0.8b])
        let result = (axis.label_up == [2r, 2u, 0.8b]) + (len(axis) > 0)
    ",
        &["mesh", "util", "color", "math"],
    );
    r.assert_int(2);
}
