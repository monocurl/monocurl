use super::{run, run_section};

use parser::ast::SectionType;

#[test]
fn print_statement_records_transcript_entry() {
    let result = run("
        let x = 42
        print x + 1
    ");

    result.assert_transcript(&["43"]);
    result.assert_transcript_root_slide_indexes(&[Some(1)]);
}

#[test]
fn print_statement_in_root_init_uses_initial_root_slide_index() {
    let result = run_section(
        r#"
            print "setup"
        "#,
        SectionType::Init,
    );

    result.assert_transcript(&[r#""setup""#]);
    result.assert_transcript_root_slide_indexes(&[Some(0)]);
}

#[test]
fn print_statement_materializes_nested_values() {
    let result = run(r#"
        print ["value" -> [1, "two"]]
    "#);

    result.assert_transcript(&[r#"{"value" -> [1, "two"]}"#]);
}

#[test]
fn print_statement_elides_mesh_leader_values() {
    let result = run("
        mesh x = [1, 2, 3]
        x[1] = 20
        print x
    ");

    result.assert_transcript(&["[1, 20, 3]"]);
}

#[test]
fn print_statement_captures_nested_mesh_self_assignment() {
    let result = run("
        mesh x = [0, 0, 0]
        x[0] = x
        print x
        x[0] = x
        print x
        x[0][0] = x
        print x
    ");

    result.assert_transcript(&[
        "[[0, 0, 0], 0, 0]",
        "[[[0, 0, 0], 0, 0], 0, 0]",
        "[[[[[0, 0, 0], 0, 0], 0, 0], 0, 0], 0, 0]",
    ]);
}
