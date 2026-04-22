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
            name: None,
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
            name: None,
        }],
        root_import_span: Some(0..0),
        was_cached: false,
    })
}

#[path = "basic_executor_tests/arithmetic.rs"]
mod arithmetic;
#[path = "basic_executor_tests/collections.rs"]
mod collections;
#[path = "basic_executor_tests/control_flow.rs"]
mod control_flow;
#[path = "basic_executor_tests/lambdas.rs"]
mod lambdas;
#[path = "basic_executor_tests/live_values.rs"]
mod live_values;
#[path = "basic_executor_tests/operators.rs"]
mod operators;
#[path = "basic_executor_tests/references.rs"]
mod references;
#[path = "basic_executor_tests/validation.rs"]
mod validation;
