use super::run;

#[test]
fn print_statement_records_transcript_entry() {
    let result = run("
        let x = 42
        print x + 1
    ");

    result.assert_ok();
    assert_eq!(result.transcript, vec!["43"]);
}

#[test]
fn print_statement_materializes_nested_values() {
    let result = run(r#"
        print ["value" -> [1, "two"]]
    "#);

    result.assert_ok();
    assert_eq!(result.transcript, vec![r#"{"value" -> [1, "two"]}"#]);
}
