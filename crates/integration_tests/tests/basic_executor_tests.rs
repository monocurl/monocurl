// lex → parse → compile → execute

use std::{f64, fs, path::Path, sync::Arc};

use compiler::cache::CompilerCache;
use executor::{
    executor::{Executor, SeekToResult},
    heap::with_heap,
    time::Timestamp,
    value::Value,
};
use lexer::{lexer::Lexer, token::Token};
use parser::{
    ast::{Section, SectionBundle, SectionType},
    parser::SectionParser,
};
use stdlib::registry::registry;
use structs::{
    assets::Assets,
    rope::{Rope, TextAggregate},
    text::Span8,
};

struct ExecResult {
    /// the value captured from the root execution head's TOS, if any
    value: Option<Value>,
    /// compile-time or runtime error messages
    errors: Vec<String>,
    _error_spans: Vec<Span8>,
}

fn cached_value_for_assert(cell: &std::cell::Cell<Option<Box<Value>>>) -> Option<Value> {
    let cached = cell.take();
    let cloned = cached.as_ref().map(|value| (**value).clone());
    cell.set(cached);
    cloned
}

fn elide_value_for_assert(value: &Value) -> Value {
    match value {
        Value::Lvalue(vrc) => {
            let resolved = with_heap(|h| h.get(vrc.key()).clone());
            elide_value_for_assert(&resolved)
        }
        Value::WeakLvalue(vweak) => {
            let resolved = with_heap(|h| h.get(vweak.key()).clone());
            elide_value_for_assert(&resolved)
        }
        Value::Leader(leader) => {
            let resolved = with_heap(|h| h.get(leader.leader_rc.key()).clone());
            elide_value_for_assert(&resolved)
        }
        Value::InvokedFunction(inv) => cached_value_for_assert(&inv.cache.0)
            .map(|resolved| elide_value_for_assert(&resolved))
            .unwrap_or_else(|| value.clone()),
        Value::InvokedOperator(inv) => cached_value_for_assert(&inv.cache.cached_result)
            .map(|resolved| elide_value_for_assert(&resolved))
            .unwrap_or_else(|| value.clone()),
        other => other.clone(),
    }
}

impl ExecResult {
    fn assert_ok(&self) {
        assert!(
            self.errors.is_empty(),
            "expected no errors, got: {:?}",
            self.errors
        );
    }

    fn assert_int(&self, expected: i64) {
        self.assert_ok();
        match &self.value {
            Some(Value::Integer(n)) => assert_eq!(*n, expected, "integer mismatch"),
            other => panic!(
                "expected Integer({}), got {}",
                expected,
                other.as_ref().map(Value::type_name).unwrap_or("(empty)")
            ),
        }
    }

    fn assert_elided_int(&self, expected: i64) {
        self.assert_ok();
        let value = self
            .value
            .as_ref()
            .map(elide_value_for_assert)
            .unwrap_or(Value::Nil);
        match value {
            Value::Integer(n) => assert_eq!(n, expected, "integer mismatch"),
            other => panic!("expected Integer({}), got {}", expected, other.type_name()),
        }
    }

    fn assert_nil(&self) {
        self.assert_ok();
        match &self.value {
            Some(Value::Nil) => {}
            other => panic!(
                "expected Nil, got {}",
                other.as_ref().map(Value::type_name).unwrap_or("(empty)")
            ),
        }
    }

    #[allow(dead_code)]
    fn assert_float(&self, expected: f64) {
        self.assert_ok();
        match &self.value {
            Some(Value::Float(f)) => assert!(
                (f - expected).abs() < 1e-9,
                "float mismatch: expected {}, got {}",
                expected,
                f
            ),
            other => panic!(
                "expected Float({}), got {}",
                expected,
                other.as_ref().map(Value::type_name).unwrap_or("(empty)")
            ),
        }
    }

    fn assert_float_list(&self, expected: &[f64]) {
        self.assert_ok();
        match &self.value {
            Some(Value::List(list)) => {
                assert_eq!(
                    list.elements().len(),
                    expected.len(),
                    "list length mismatch"
                );

                for (actual, expected) in list.elements().iter().zip(expected.iter()) {
                    match with_heap(|h| h.get(actual.key()).clone()) {
                        Value::Float(f) => assert!(
                            (f - *expected).abs() < 1e-9,
                            "float mismatch: expected {}, got {}",
                            expected,
                            f
                        ),
                        other => panic!("expected float list element, got {}", other.type_name()),
                    }
                }
            }
            other => panic!(
                "expected List, got {}",
                other.as_ref().map(Value::type_name).unwrap_or("(empty)")
            ),
        }
    }

    fn assert_float_list_approx(&self, expected: &[f64], eps: f64) {
        self.assert_ok();
        match &self.value {
            Some(Value::List(list)) => {
                assert_eq!(
                    list.elements().len(),
                    expected.len(),
                    "list length mismatch"
                );

                for (actual, expected) in list.elements().iter().zip(expected.iter()) {
                    match with_heap(|h| h.get(actual.key()).clone()) {
                        Value::Float(f) => assert!(
                            (f - *expected).abs() < eps,
                            "float mismatch: expected {}, got {}",
                            expected,
                            f
                        ),
                        Value::Integer(n) => assert!(
                            (n as f64 - *expected).abs() < eps,
                            "float mismatch: expected {}, got {}",
                            expected,
                            n
                        ),
                        other => panic!("expected numeric list element, got {}", other.type_name()),
                    }
                }
            }
            other => panic!(
                "expected List, got {}",
                other.as_ref().map(Value::type_name).unwrap_or("(empty)")
            ),
        }
    }

    fn assert_int_list(&self, expected: &[i64]) {
        self.assert_ok();
        match &self.value {
            Some(Value::List(list)) => {
                assert_eq!(
                    list.elements().len(),
                    expected.len(),
                    "list length mismatch"
                );

                for (actual, expected) in list.elements().iter().zip(expected.iter()) {
                    match with_heap(|h| h.get(actual.key()).clone()) {
                        Value::Integer(n) => {
                            assert_eq!(n, *expected, "integer mismatch");
                        }
                        other => panic!("expected int list element, got {}", other.type_name()),
                    }
                }
            }
            other => panic!(
                "expected List, got {}",
                other.as_ref().map(Value::type_name).unwrap_or("(empty)")
            ),
        }
    }

    fn assert_string(&self, expected: &str) {
        self.assert_ok();
        match &self.value {
            Some(Value::String(s)) => assert_eq!(s, expected, "string mismatch"),
            other => panic!(
                "expected String({:?}), got {}",
                expected,
                other.as_ref().map(Value::type_name).unwrap_or("(empty)")
            ),
        }
    }

    fn assert_error(&self, fragment: &str) {
        assert!(
            self.errors.iter().any(|e| e.contains(fragment)),
            "expected error containing {:?}, got: {:?}",
            fragment,
            self.errors
        );
    }

    fn assert_first_error_span(&self, expected: Span8) {
        assert!(
            !self._error_spans.is_empty(),
            "expected at least one runtime error span"
        );
        assert_eq!(self._error_spans[0], expected);
    }

    #[allow(dead_code)]
    fn assert_no_value(&self) {
        self.assert_ok();
        assert!(
            self.value.is_none(),
            "expected no value, got {}",
            self.value.as_ref().map(Value::type_name).unwrap_or("")
        );
    }
}

fn lex(src: &str) -> Vec<(Token, Span8)> {
    Lexer::token_stream(src.chars())
        .into_iter()
        .filter(|(t, _)| t != &Token::Whitespace && t != &Token::Comment)
        .collect()
}

/// compile and execute a snippet of Monocurl slide code.
/// the source is treated as the body of a single Slide section
fn run(src: &str) -> ExecResult {
    run_with_stdlib(src, &[])
}

fn run_with_stdlib(src: &str, stdlib_names: &[&str]) -> ExecResult {
    run_section_with_stdlib(src, SectionType::Slide, stdlib_names)
}

fn run_section(src: &str, section_type: SectionType) -> ExecResult {
    run_section_with_stdlib(src, section_type, &[])
}

fn run_section_with_stdlib(
    src: &str,
    section_type: SectionType,
    stdlib_names: &[&str],
) -> ExecResult {
    // -- parse --
    let tokens = lex(src);
    let rope: Rope<TextAggregate> = Rope::from_str(src);
    let mut parser = SectionParser::new(tokens, rope, section_type.clone(), None, None);
    let stmts = parser.parse_statement_list();

    let parse_errors: Vec<String> = parser
        .artifacts()
        .error_diagnostics
        .iter()
        .map(|e| e.message.clone())
        .collect();
    if !parse_errors.is_empty() {
        return ExecResult {
            value: None,
            errors: parse_errors,
            _error_spans: vec![],
        };
    }

    let stdlib_bundles: Vec<Arc<SectionBundle>> =
        stdlib_names.iter().copied().map(stdlib_bundle).collect();
    let imported_files: Vec<usize> = (0..stdlib_bundles.len()).collect();

    let user_bundle = Arc::new(SectionBundle {
        file_path: None,
        file_index: 0,
        imported_files,
        sections: vec![Section {
            body: stmts,
            section_type,
        }],
        root_import_span: None,
        was_cached: false,
    });

    let mut bundles = stdlib_bundles;
    bundles.push(user_bundle);

    let mut cache = CompilerCache::default();
    let result = compiler::compiler::compile(&mut cache, None, &bundles);

    let compile_errors: Vec<String> = result.errors.iter().map(|e| e.message.clone()).collect();
    if !compile_errors.is_empty() {
        return ExecResult {
            value: None,
            errors: compile_errors,
            _error_spans: vec![],
        };
    }

    // section 0 is the prelude; section 1 is our slide
    if result.bytecode.sections.len() < 2 {
        return ExecResult {
            value: None,
            errors: vec!["no user section was compiled".into()],
            _error_spans: vec![],
        };
    }

    println!(
        "Bytecode Instructions {:?}",
        result.bytecode.sections[1].instructions
    );

    // -- execute --
    let mut executor = Executor::new(result.bytecode, registry().func_table());

    let mut runtime_errors: Vec<String> = Vec::new();

    smol::block_on(async {
        let target = executor.user_to_internal_timestamp(Timestamp::new(0, f64::INFINITY));
        match executor.seek_to(target).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => {
                runtime_errors.push(e.to_string());
            }
        }
    });

    runtime_errors.extend(
        executor
            .state
            .errors
            .iter()
            .map(|err| err.error.to_string()),
    );
    let error_spans = executor
        .state
        .errors
        .iter()
        .map(|err| err.span.clone())
        .collect();

    let value = executor
        .state
        .captured_output
        .into_iter()
        .last()
        .map(|v| match v {
            Value::Leader(leader) => with_heap(|h| h.get(leader.leader_rc.key()).clone()),
            other => other,
        });

    ExecResult {
        value,
        errors: runtime_errors,
        _error_spans: error_spans,
    }
}

fn stdlib_path(name: &str) -> std::path::PathBuf {
    Assets::std_lib().join(format!("std/{name}.mcl"))
}

fn stdlib_bundle(name: &str) -> Arc<SectionBundle> {
    let src = fs::read_to_string(stdlib_path(name)).expect("failed to read stdlib file");
    let tokens = lex(&src);
    let rope: Rope<TextAggregate> = Rope::from_str(&src);
    let mut parser = SectionParser::new(tokens, rope, SectionType::StandardLibrary, None, None);
    let stmts = parser.parse_statement_list();
    let errors: Vec<String> = parser
        .artifacts()
        .error_diagnostics
        .iter()
        .map(|e| e.message.clone())
        .collect();
    assert!(errors.is_empty(), "stdlib parse errors: {:?}", errors);

    Arc::new(SectionBundle {
        file_path: Some(Path::new(&format!("std/{name}.mcl")).to_path_buf()),
        file_index: 0,
        imported_files: vec![],
        sections: vec![Section {
            body: stmts,
            section_type: SectionType::StandardLibrary,
        }],
        root_import_span: Some(0..0),
        was_cached: false,
    })
}

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
        let f = |x = *scale| x * 2
        let result = f()
    ");
    r.assert_int(6);
}

#[test]
fn test_default_references_mesh_is_ok() {
    let r = run("
        let make = |v| v
        mesh m = make(v: 7)
        let f = |x = m.v| x + 1
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

#[test]
fn test_exec_operator_creation_and_invocation() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let x = 40
        let result = add{2} x
    ");
    r.assert_elided_int(42);
}

#[test]
fn test_exec_operator_creation() {
    let r = run("
        let result = operator |target, amount| {
            return [target, target + amount]
        }
    ");
    r.assert_ok();
    match &r.value {
        Some(Value::Operator(_)) => {}
        other => panic!(
            "expected operator, got {}",
            other.as_ref().map(Value::type_name).unwrap_or("(empty)")
        ),
    }
}

#[test]
fn test_exec_operator_chain_invocation() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        let x = 10
        let result = add{2} mul{3} x
    ");
    r.assert_elided_int(32);
}

#[test]
fn test_exec_operator_chain_with_aliases() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        let outer = add
        let inner = mul
        let x = 10
        let result = outer{2} inner{3} x
    ");
    r.assert_elided_int(32);
}

#[test]
fn test_exec_operator_chain_same_operator_multiple_times() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let x = 10
        let result = add{2} add{3} add{4} x
    ");
    r.assert_elided_int(19);
}

#[test]
fn test_exec_labeled_operator_arg_readable() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let inv = add{amount: 2} 40
        let result = inv.amount
    ");
    r.assert_int(2);
}

#[test]
fn test_exec_labeled_operator_arg_mutable() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        var inv = add{amount: 2} 40
        inv.amount = 5
        let result = inv.amount
    ");
    r.assert_int(5);
}

#[test]
fn test_exec_mesh_leader_labeled_attribute_mutable() {
    let r = run_section(
        "
        let hello = |origin, radius| origin + radius
        mesh base = hello(origin: 10, radius: 2)
        base.origin = 45
        let result = base.origin
    ",
        SectionType::Slide,
    );
    r.assert_int(45);
}

#[test]
fn test_exec_mesh_leader_labeled_attribute_binary_ops_elide_leader() {
    let r = run_section(
        "
        let hello = |origin, radius| origin + radius
        mesh base = hello(origin: 10, radius: 2)
        let result = base.origin + 5
    ",
        SectionType::Slide,
    );
    r.assert_int(15);
}

#[test]
fn test_exec_labeled_operator_mutation_updates_downstream_value() {
    let r = run("
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        var inv = add{amount: 2} 40
        inv.amount = 5
        let result = mul{2} inv
    ");
    r.assert_elided_int(90);
}

#[test]
fn test_exec_labeled_operator_error_on_unknown_label() {
    let r = run("
        let f = |x, y| x + y
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        let inv = passthrough{amount: 2} f(lbl: 40, 2)
        let result = inv.unknown_label
    ");
    r.assert_error("no labeled argument");
}

#[test]
fn test_exec_labeled_operator_delegates_read_to_operand_attribute() {
    let r = run("
        let f = |x, y| x + y
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        let inv = passthrough{amount: 2} f(lbl: 40, 2)
        let result = inv.lbl
    ");
    r.assert_int(40);
}

#[test]
fn test_exec_unlabeled_operator_delegates_read_to_operand_attribute() {
    let r = run("
        let f = |origin = 10, radius = 2| origin + radius
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        let inv = passthrough{2} f(origin: 40, radius: 2)
        let result = inv.origin
    ");
    r.assert_int(40);
}

#[test]
fn test_exec_labeled_operator_delegates_mutation_to_operand_attribute() {
    let r = run("
        let f = |x, y| x + y
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        var inv = passthrough{amount: 2} f(lbl: 40, 2)
        inv.lbl = 50
        let result = inv.lbl
    ");
    r.assert_int(50);
}

#[test]
fn test_exec_labeled_operator_operand_mutation_invalidates_cache() {
    let r = run("
        let f = |x, y| x + y
        let passthrough = operator |target, amount| {
            return [target, target]
        }
        var inv = passthrough{amount: 2} f(lbl: 40, 2)
        inv.lbl = 50
        let result = inv + 0
    ");
    r.assert_int(52);
}

#[test]
fn test_exec_native_lerp_numbers() {
    let r = run_section(
        "
        let result = __monocurl__native__ lerp(10, 20, 0.25)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(12.5);
}

#[test]
fn test_exec_native_lerp_list_element() {
    let r = run_section(
        "
        let xs = __monocurl__native__ lerp([0, 10], [10, 20], 0.5)
        let result = xs[1]
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(15.0);
}

#[test]
fn test_exec_native_lerp_vector() {
    let r = run_section(
        "
        let result = __monocurl__native__ lerp([0, 10, 20], [10, 20, 30], 0.25)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float_list(&[2.5, 12.5, 22.5]);
}

#[test]
fn test_exec_native_lerp_nested_vector() {
    let r = run_section(
        "
        let rows = __monocurl__native__ lerp([[0, 10], [20, 30]], [[10, 20], [30, 40]], 0.5)
        let result = rows[1]
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float_list(&[25.0, 35.0]);
}

#[test]
fn test_exec_native_lerp_labeled_function_result_value() {
    let r = run_section(
        "
        let f = |x, y| x + y
        let result = __monocurl__native__ lerp(f(lbl: 0, 10), f(lbl: 8, 10), 0.25) + 0
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(12.0);
}

#[test]
fn test_exec_native_lerp_labeled_function_preserves_label() {
    let r = run_section(
        "
        let f = |x, y| x + y
        let result = (__monocurl__native__ lerp(f(lbl: 0, 10), f(lbl: 8, 10), 0.25)).lbl
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(2.0);
}

#[test]
fn test_exec_native_lerp_labeled_function_rejects_unlabeled_difference() {
    let r = run_section(
        "
        let f = |x, y| x + y
        let result = __monocurl__native__ lerp(f(1, lbl: 10), f(2, lbl: 20), 0.5)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_error("unlabeled argument at index 0 differs");
}

#[test]
fn test_exec_native_lerp_operator_rhs_uses_operand() {
    let r = run_section(
        "
        let shift = operator |target, delta| {
            return [target + 100, target + delta]
        }
        let result = __monocurl__native__ lerp(10, shift{delta: 4} 20, 0.5)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(67.0);
}

#[test]
fn test_exec_native_lerp_labeled_operator_rhs() {
    let r = run_section(
        "
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let result = __monocurl__native__ lerp(10, add{amount: 8} 20, 0.25)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(14.5);
}

#[test]
fn test_exec_native_lerp_copied_labeled_operator_preserves_label() {
    let r = run_section(
        "
        let shift = operator |target, lbl| {
            return [target, target + lbl]
        }
        var x = shift{lbl: 1} 10
        var y = x
        y.lbl = 10
        let result = (__monocurl__native__ lerp(x, y, 0.5)).lbl
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(5.5);
}

#[test]
fn test_exec_native_lerp_copied_labeled_operator_value() {
    let r = run_section(
        "
        let shift = operator |target, lbl| {
            return [target, target + lbl]
        }
        var x = shift{lbl: 1} 10
        var y = x
        y.lbl = 10
        let result = (__monocurl__native__ lerp(x, y, 0.5)) + 0
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(15.5);
}

#[test]
fn test_exec_native_lerp_nested_labeled_operator_rhs() {
    let r = run_section(
        "
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        let result = __monocurl__native__ lerp(10, add{amount: 2} mul{factor: 3} 4, 0.5)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(15.0);
}

#[test]
fn test_exec_native_lerp_copied_nested_labeled_operator() {
    let r = run_section(
        "
        let add = operator |target, amount| {
            return [target, target + amount]
        }
        let mul = operator |target, factor| {
            return [target, target * factor]
        }
        var x = add{amount: 2} mul{factor: 3} 4
        var y = x
        y.amount = 6
        y.factor = 5
        let z = __monocurl__native__ lerp(x, y, 0.5)
        let result = z.amount + z.factor + (z + 0)
    ",
        SectionType::StandardLibrary,
    );
    r.assert_float(28.0);
}

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

#[test]
fn test_exec_runtime_error_call_non_lambda() {
    let r = run("
        let x = 42
        let y = x(1)
    ");
    r.assert_error("lambda");
}

// -- COW: list element independence after aliasing --

#[test]
fn test_cow_list_mutation_doesnt_affect_alias() {
    // a[0] = 99 must not bleed into b; they share Rc elements until the write triggers COW
    let r = run("
        var a = [1, 2, 3]
        var b = a
        a[0] = 99
        let result = b[0]
    ");
    r.assert_int(1);
}

#[test]
fn test_cow_list_alias_mutation_doesnt_affect_original() {
    let r = run("
        var a = [10, 20, 30]
        var b = a
        b[2] = 77
        let result = a[2]
    ");
    r.assert_int(30);
}

#[test]
fn test_cow_list_both_aliases_mutate_independently() {
    let r = run("
        var a = [1, 2, 3]
        var b = a
        a[0] = 100
        b[0] = 200
        let result = a[0] + b[0]
    ");
    r.assert_int(300);
}

#[test]
fn test_cow_list_nested_alias_chain() {
    // a → b → c all start sharing element Rcs; mutation to c must not affect a
    let r = run("
        var a = [5, 6, 7]
        var b = a
        var c = b
        c[1] = 99
        let result = a[1]
    ");
    r.assert_int(6);
}

// -- labeled function invocations --
#[test]
fn test_labeled_elide() {
    let r = run("
        let f = |x, y| x + y
        let inv = f(myarg: 10, 30)
        let result = inv + 10
    ");
    r.assert_int(50);
}

#[test]
fn test_labeled_recompute() {
    let r = run("
        let f = |x, y| x + y
        var inv = f(myarg: 10, 30)
        let org = 0 + inv
        inv.myarg = 30
        let full = org + inv
    ");
    r.assert_int(100);
}

#[test]
fn test_labeled_read_first_arg() {
    let r = run("
        let f = |x, y| x + y
        let inv = f(myarg: 10, 30)
        let result = inv.myarg
    ");
    r.assert_int(10);
}

#[test]
fn test_labeled_read_second_arg() {
    let r = run("
        let f = |x, y| x + y
        let inv = f(10, second: 30)
        let result = inv.second
    ");
    r.assert_int(30);
}

#[test]
fn test_labeled_both_args_readable() {
    let r = run("
        let f = |a, b| a - b
        let inv = f(lhs: 50, rhs: 8)
        let result = inv.lhs - inv.rhs
    ");
    r.assert_int(42);
}

#[test]
fn test_labeled_mutate_arg() {
    let r = run("
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        inv.lbl = 5
        let result = inv.lbl
    ");
    r.assert_int(5);
}

#[test]
fn test_labeled_default_arg_is_readable() {
    let r = run("
        let f = |x, y = 100| x + y
        let inv = f(lbl: 7)
        let result = inv.lbl
    ");
    r.assert_int(7);
}

#[test]
fn test_labeled_error_on_unknown_label() {
    let r = run("
        let f = |x, y| x + y
        let inv = f(known: 1, 2)
        let result = inv.unknown_label
    ");
    r.assert_error("no labeled argument");
}

// -- COW on InvokedFunction: mutating one copy must not affect the other --

#[test]
fn test_cow_invoked_function_mutation_leaves_alias_intact() {
    // alias and inv start sharing the same Rc<InvokedFunction>;
    // mutating inv.lbl triggers Rc::make_mut (COW) so alias is unchanged
    let r = run("
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        let alias = inv
        inv.lbl = 99
        let result = alias.lbl
    ");
    r.assert_int(10);
}

#[test]
fn test_cow_invoked_function_mutated_copy_has_new_value() {
    let r = run("
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        let _alias = inv
        inv.lbl = 77
        let result = inv.lbl
    ");
    r.assert_int(77);
}

#[test]
fn test_labeled_nested_live_elision_in_arithmetic() {
    let r = run("
        let inner = |x, y| x + y
        let outer = |seed| inner(lhs: seed * 2, rhs: 5)
        let result = outer(seed: 7) + 3
    ");
    r.assert_int(22);
}

#[test]
fn test_labeled_nested_mutation_recomputes_live_value() {
    let r = run("
        let inner = |x, y| x + y
        let outer = |seed| inner(lhs: seed * 2, rhs: 5)
        var inv = outer(seed: 7)
        inv.seed = 10
        let result = inv + 3
    ");
    r.assert_int(28);
}

#[test]
fn test_labeled_aliases_keep_independent_live_results() {
    let r = run("
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        let alias = inv
        inv.lbl = 99
        let result = alias + inv
    ");
    r.assert_int(169);
}

#[test]
fn test_live_function_structural_equality() {
    // same labeled invocation is structurally equal
    let r = run("
        let f = |x, y| x + y
        let result = f(lhs: 8, rhs: 4) == f(lhs: 8, rhs: 4)
    ");
    r.assert_int(1);
}

#[test]
fn test_live_function_structural_inequality() {
    // different args → not equal, even if computed result would be the same
    let r = run("
        let f = |x| x * 2
        let result = f(a: 3) == f(a: 6)
    ");
    r.assert_int(0);
}

#[test]
fn test_live_function_not_equal_to_primitive() {
    // a live function invocation is structurally different from a plain integer
    let r = run("
        let f = |x, y| x + y
        let result = f(lhs: 8, rhs: 4) == 12
    ");
    r.assert_int(0);
}

#[test]
fn test_live_elision_supports_negation() {
    let r = run("
        let f = |x, y| x - y
        let result = -f(lhs: 5, rhs: 8)
    ");
    r.assert_int(3);
}

#[test]
fn test_live_elision_recomputes_defaulted_labeled_invocation() {
    let r = run("
        let f = |x, y = 100| x + y
        var inv = f(lbl: 7)
        inv.lbl = 20
        let result = inv + inv.lbl
    ");
    r.assert_int(140);
}

#[test]
fn test_util_attr_helpers_on_live_function() {
    let r = run_with_stdlib(
        "
        let f = |x, y| x + y
        var inv = f(lbl: 10, 30)
        inv = set_attr(inv, \"lbl\", 25)
        let result = has_attr(inv, \"lbl\") * 100 + has_attr(inv, \"missing\") * 10 + get_attr(inv, \"lbl\")
    ",
        &["util"],
    );
    r.assert_int(125);
}

#[test]
fn test_util_attr_helpers_on_live_operator_delegate_to_operand() {
    let r = run_with_stdlib(
        "
        let f = |x, y| x + y
        let passthrough = operator |target, amount| [target, target]
        let inv = passthrough{amount: 2} f(lbl: 40, 2)
        let updated = set_attr(inv, \"lbl\", 50)
        let result = has_attr(updated, \"lbl\") * 100 + get_attr(updated, \"lbl\")
    ",
        &["util"],
    );
    r.assert_int(150);
}

#[test]
fn test_util_type_predicates_cover_callable_variants() {
    let r = run_with_stdlib(
        "
        let f = |x, y| x + y
        let op = operator |target| [target, target]
        let live_f = f(arg: 1, 2)
        let live_op = op{} 1
        let result = is_float(1.5)
            + is_number(2)
            + is_list([1, 2])
            + is_function(f)
            + is_function(live_f)
            + is_operator(op)
            + is_operator(live_op)
            + is_callable(f)
            + is_callable(live_op)
            + is_live_function(live_f)
            + is_live_operator(live_op)
    ",
        &["util"],
    );
    r.assert_int(11);
}

#[test]
fn test_util_type_predicates_cover_mesh_and_primitive_anim() {
    let r = run_with_stdlib(
        "
        let result = is_mesh(Dot()) + is_primitive_anim(PrimitiveAnim())
    ",
        &["util", "mesh", "anim"],
    );
    r.assert_int(2);
}

#[test]
fn test_util_type_of_and_runtime_error() {
    let r = run_with_stdlib(
        "
        let f = |x, y| x + y
        let result = type_of(f(lbl: 1, 2))
    ",
        &["util"],
    );
    r.assert_string("live function");

    let err = run_with_stdlib("runtime_error(\"boom\")", &["util"]);
    err.assert_error("boom");
}

#[test]
fn test_mesh_stdlib_reports_named_bad_list_argument() {
    let r = run_with_stdlib(
        "
        let result = ColorGrid(|pos| [1, 0, 0, 1], 5)
    ",
        &["mesh"],
    );
    r.assert_error("invalid argument 'x_min_max_samples'");
    r.assert_error("expected list of length 3");
    r.assert_error("got int");
}

#[test]
fn test_mesh_operator_filter_applies_predicate_to_subset() {
    let r = run_with_stdlib(
        "
        let scene = [
            retag{1} Circle([0, 0, 0], 1),
            retag{2} Circle([4, 0, 0], 1)
        ]
        let shifted = shift{delta: 10 * 1r, filter: |tag| tag == 1} scene
        let x1 = mesh_center(tag_filter(shifted, 1))[0]
        let x2 = mesh_center(tag_filter(shifted, 2))[0]
        let result = (abs(x1 - 10) < 0.001) + (abs(x2 - 4) < 0.001)
    ",
        &["mesh", "math"],
    );
    r.assert_int(2);
}

#[test]
fn test_on_side_and_on_corner_smoke() {
    let r = run_with_stdlib(
        "
        let cam = Camera([0, 0, -10], 1f, 1u)
        let side = mesh_center(on_side{dir: 1r, camera: cam} Circle())
        let corner = mesh_center(on_corner{dir: [1, 1, 0], camera: cam, buffer: 0.1} Circle())
        let result = (side[0] > 0) + (corner[0] > 0) + (corner[1] > 0)
    ",
        &["mesh", "scene"],
    );
    r.assert_int(3);
}

#[test]
fn test_capsule_accepts_scalar_and_equal_pair_radii() {
    let r = run_with_stdlib(
        "
        let scalar = len(mesh_triangle_set(Capsule([0, 0, 0], [2, 0, 0], 0.4)))
        let pair = len(mesh_triangle_set(Capsule([0, 0, 0], [2, 0, 0], [0.4, 0.4])))
        let result = (scalar > 0) + (pair > 0)
    ",
        &["mesh", "util"],
    );
    r.assert_int(2);
}

#[test]
fn test_explicit_func_diff_accepts_custom_tags() {
    let r = run_with_stdlib(
        "
        let f = |x| 1
        let g = |x| 0
        let fill0 = [0.3, 0.8, 0.3, 0.5]
        let fill1 = [0.8, 0.3, 0.3, 0.5]
        let fills = [fill0, fill1]
        let custom_tags = [7, 9]
        let diff = ExplicitFuncDiff(f, g, [-1, 1, 16], fills, custom_tags)
        let tags = sort(mesh_tags(diff))
        let result = (len(tags) == 2) + (tags[0] == 7) + (tags[1] == 9)
    ",
        &["mesh", "util"],
    );
    r.assert_int(3);
}

#[test]
fn test_parametric_func_reports_named_bad_sample_range_argument() {
    let r = run_with_stdlib(
        "
        let result = ParametricFunc(|t| [t, 0, 0], 5)
    ",
        &["mesh"],
    );
    r.assert_error("invalid argument 't_min_max_samples'");
    r.assert_error("expected list of length 3");
    r.assert_error("got int");
}

#[test]
fn test_explicit_func_reports_named_bad_sample_range_argument() {
    let r = run_with_stdlib(
        "
        let result = ExplicitFunc(|x| x, 5)
    ",
        &["mesh"],
    );
    r.assert_error("invalid argument 'x_min_max_samples'");
    r.assert_error("expected list of length 3");
    r.assert_error("got int");
}

#[test]
fn test_mesh_stdlib_reports_named_bad_list_length() {
    let r = run_with_stdlib(
        "
        let result = Rect([0, 0, 0], [1, 2, 3])
    ",
        &["mesh"],
    );
    r.assert_error("invalid argument 'size'");
    r.assert_error("expected list of length 2");
    r.assert_error("got list of length 3");
}

#[test]
fn test_color_stdlib_reports_named_bad_color_argument() {
    let r = run_with_stdlib(
        "
        let result = with_alpha(7, 0.5)
    ",
        &["color"],
    );
    r.assert_error("invalid argument 'color'");
    r.assert_error("expected list of length 4");
    r.assert_error("got int");
}

#[test]
fn test_field_uses_sample_counts_and_index_callback() {
    let r = run_with_stdlib(
        "
        let result = Field(|pos, idx| idx[0] * 10 + idx[1], [0, 1, 3], [0, 1, 2])
    ",
        &["mesh"],
    );
    r.assert_int_list(&[0, 1, 10, 11, 20, 21]);
}

#[test]
fn test_color_grid_uses_sample_counts() {
    let r = run_with_stdlib(
        "
        let result = len(mesh_triangle_set(ColorGrid(|pos, idx| [1, 0, 0, 1], [0, 1, 3], [0, 1, 4])))
    ",
        &["mesh", "util"],
    );
    r.assert_int(12);
}

#[test]
fn test_parametric_func_sample_limit_is_reported() {
    let r = run_with_stdlib(
        "
        let result = ParametricFunc(|t| [t, 0, 0], [0, 1, 20000])
    ",
        &["mesh"],
    );
    r.assert_error("parametric samples is too large");
}

#[test]
fn test_mesh_collapse_flattens_tree_into_one_mesh() {
    let r = run_with_stdlib(
        "
        let result = mesh_center(mesh_collapse([Line([0, 0, 0], [1, 0, 0]), Line([2, 0, 0], [3, 0, 0])]))
    ",
        &["mesh"],
    );
    r.assert_float_list_approx(&[1.5, 0.0, 0.0], 1e-9);
}

#[test]
fn test_mesh_trans_helper_interpolates_without_animation_context() {
    let r = run_with_stdlib(
        "
        let result = mesh_center(trans(Dot([0, 0, 0]), Dot([2, 0, 0]), 0.5))
    ",
        &["mesh"],
    );
    r.assert_float_list_approx(&[1.0, 0.0, 0.0], 1e-9);
}

#[test]
fn test_rotate_operator_uses_angle_axis_and_optional_pivot() {
    let r = run_with_stdlib(
        "
        let result = mesh_center(rotate{1.5707963267948966, 1f, [0, 0, 0]} Dot([1, 0, 0]))
    ",
        &["mesh"],
    );
    r.assert_float_list_approx(&[0.0, 1.0, 0.0], 1e-5);
}

#[test]
fn test_camera_stdlib_uses_forward_vector_surface() {
    let r = run_with_stdlib(
        "
        let cam = Camera([1, 2, 3], [0, 0, 2], [0, 1, 0], 0.2, 50)
        let result = [cam[\"position\"], cam[\"forward\"], cam[\"near\"], cam[\"far\"]]
    ",
        &["scene"],
    );
    r.assert_ok();
    match &r.value {
        Some(Value::List(list)) => {
            let elems = list.elements();
            match with_heap(|h| h.get(elems[0].key()).clone()) {
                Value::List(position) => {
                    let coords: Vec<_> = position
                        .elements()
                        .iter()
                        .map(|elem| with_heap(|h| h.get(elem.key()).clone()))
                        .collect();
                    assert!(matches!(coords[0], Value::Integer(1)));
                    assert!(matches!(coords[1], Value::Integer(2)));
                    assert!(matches!(coords[2], Value::Integer(3)));
                }
                other => panic!("expected camera position list, got {}", other.type_name()),
            }
            match with_heap(|h| h.get(elems[1].key()).clone()) {
                Value::List(forward) => {
                    let coords: Vec<_> = forward
                        .elements()
                        .iter()
                        .map(|elem| with_heap(|h| h.get(elem.key()).clone()))
                        .collect();
                    assert!(matches!(coords[0], Value::Integer(0)));
                    assert!(matches!(coords[1], Value::Integer(0)));
                    assert!(matches!(coords[2], Value::Integer(2)));
                }
                other => panic!("expected camera forward list, got {}", other.type_name()),
            }
            assert!(matches!(
                with_heap(|h| h.get(elems[2].key()).clone()),
                Value::Float(f) if (f - 0.2).abs() < 1e-9
            ));
            assert!(matches!(
                with_heap(|h| h.get(elems[3].key()).clone()),
                Value::Float(f) if (f - 50.0).abs() < 1e-9
            ));
        }
        other => panic!(
            "expected camera surface list, got {}",
            other.as_ref().map(Value::type_name).unwrap_or("(empty)")
        ),
    }
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

#[test]
fn test_ref_basic_mutation() {
    // mutate increments its reference argument; x should be 1 after the call
    let r = run("
        param x = 0
        let mutate = |&y| {
            y = y + 1
            return []
        }
        mutate(&x)
        let result = *x
    ");
    r.assert_int(1);
}

#[test]
fn test_ref_mutation_does_not_affect_unrelated_var() {
    let r = run("
        param x = 10
        param z = 99
        let inc = |&y| {
            y = y + 1
            return []
        }
        inc(&x)
        let result = *z
    ");
    r.assert_int(99);
}

#[test]
fn test_ref_called_multiple_times() {
    let r = run("
        param x = 0
        let inc = |&y| {
            y = y + 1
            return []
        }
        inc(&x)
        inc(&x)
        inc(&x)
        let result = *x
    ");
    r.assert_int(3);
}

#[test]
fn test_ref_chain_of_lambdas() {
    // inner passes its reference argument straight through to another lambda
    let r = run("
        param x = 0
        let add_two = |&y| {
            y = y + 2
            return []
        }
        let double_add = |&z| {
            add_two(&z)
            add_two(&z)
            return []
        }
        double_add(&x)
        let result = *x
    ");
    r.assert_int(4);
}

#[test]
fn test_ref_two_distinct_references() {
    let r = run("
        param a = 1
        param b = 10
        let modify_both = |&x, &y| {
            x = x + 1
            y = y + 1
            return []
        }
        modify_both(&a, &b)
        let result = *a + *b
    ");
    // a=2, b=11, result=13
    r.assert_int(13);
}

#[test]
fn test_ref_reference_to_list_via_ref() {
    // pass the whole list by reference; subscript-assign inside the lambda
    let r = run("
        param arr = [0, 0, 0]
        let set_first = |&a| {
            a[0] = 42
            return []
        }
        set_first(&arr)
        let result = (*arr)[0]
    ");
    r.assert_int(42);
}

#[test]
fn test_ref_destructure_list_references() {
    // pass a list of references using list destructure assignment inside the lambda
    let r = run("
        param a = 0
        param b = 0
        let set_both = |&x, &y| {
            x = 7
            y = 13
            return []
        }
        set_both(&a, &b)
        let result = *a + *b
    ");
    r.assert_int(20);
}

#[test]
fn test_ref_reference_in_closure_capture() {
    // lambda captures a var by value; separate reference arg must not alias the capture
    let r = run("
        let captured = 5
        param target = 0
        let f = |&r| {
            r = captured + 1
            return []
        }
        f(&target)
        let result = *target
    ");
    r.assert_int(6);
}

#[test]
fn test_ref_lambda_called_with_value_reports_runtime_error_instead_of_panicking() {
    let r = run("
        let overwrite = |&y| {
            y = 2
            return []
        }
        overwrite(1)
    ");
    r.assert_error("cannot assign");
}

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
