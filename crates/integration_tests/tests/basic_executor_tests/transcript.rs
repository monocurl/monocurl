use super::run;

#[test]
fn print_statement_records_transcript_entry() {
    let result = run("
        let x = 42
        print x + 1
    ");

    result.assert_transcript(&["43"]);
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
