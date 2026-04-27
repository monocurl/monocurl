// lex → parse → compile → execute

use std::f64;

use executor::{
    executor::{Executor, SeekToResult},
    heap::with_heap,
    time::Timestamp,
    value::Value,
};
use integration_tests::{
    compile_bundles, inspect_block, make_section_bundle, parse_section, print_inspection,
    stdlib_bundle, value_summary,
};
use parser::ast::SectionType;
use stdlib::registry::registry;
use structs::text::Span8;

struct ExecResult {
    /// the value captured from the root execution head's TOS, if any
    value: Option<Value>,
    transcript: Vec<String>,
    /// compile-time or runtime error messages
    errors: Vec<String>,
    _error_spans: Vec<Span8>,
}

fn elide_value_for_assert(value: &Value) -> Value {
    value.clone().elide_cached_wrappers_rec()
}

impl ExecResult {
    fn inspection_lines(&self) -> Vec<String> {
        vec![
            format!("value: {}", self.value_summary()),
            format!("transcript: {:?}", self.transcript),
            format!("errors: {:?}", self.errors),
            format!("error spans: {:?}", self._error_spans),
        ]
    }

    fn inspection(&self) -> String {
        inspect_block("exec result", self.inspection_lines())
    }

    #[allow(dead_code)]
    fn inspect(&self, label: &str) -> &Self {
        print_inspection(label, self.inspection_lines());
        self
    }

    fn value_summary(&self) -> String {
        self.value
            .as_ref()
            .map(|value| value_summary(&elide_value_for_assert(value)))
            .unwrap_or_else(|| "(empty)".to_string())
    }

    fn elided_value(&self) -> Option<Value> {
        self.value.as_ref().map(elide_value_for_assert)
    }

    fn unexpected_value(&self, expected: &str, actual: Option<&Value>) -> ! {
        panic!(
            "expected {expected}, got {}\n{}",
            actual.map(Value::type_name).unwrap_or("(empty)"),
            self.inspection()
        )
    }

    fn assert_list_elements<T>(
        &self,
        expected: &[T],
        mut assert_element: impl FnMut(usize, Value, &T),
    ) -> &Self {
        self.assert_ok();
        let value = self.elided_value();
        let Some(Value::List(list)) = &value else {
            self.unexpected_value("List", value.as_ref());
        };

        assert_eq!(
            list.elements().len(),
            expected.len(),
            "list length mismatch\n{}",
            self.inspection()
        );

        for (idx, (actual, expected)) in list.elements().iter().zip(expected.iter()).enumerate() {
            assert_element(
                idx,
                with_heap(|heap| heap.get(actual.key()).clone()),
                expected,
            );
        }

        self
    }

    fn assert_ok(&self) -> &Self {
        assert!(
            self.errors.is_empty(),
            "expected no errors, got: {:?}\n{}",
            self.errors,
            self.inspection()
        );
        self
    }

    fn assert_int(&self, expected: i64) -> &Self {
        self.assert_ok();
        let value = self.elided_value();
        match &value {
            Some(Value::Integer(n)) => {
                assert_eq!(*n, expected, "integer mismatch\n{}", self.inspection())
            }
            other => self.unexpected_value(&format!("Integer({expected})"), other.as_ref()),
        }
        self
    }

    fn assert_nil(&self) -> &Self {
        self.assert_ok();
        let value = self.elided_value();
        match &value {
            Some(Value::Nil) => {}
            other => self.unexpected_value("Nil", other.as_ref()),
        }
        self
    }

    #[allow(dead_code)]
    fn assert_float(&self, expected: f64) -> &Self {
        self.assert_ok();
        let value = self.elided_value();
        match &value {
            Some(Value::Float(f)) => assert!(
                (f - expected).abs() < 1e-9,
                "float mismatch: expected {expected}, got {f}\n{}",
                self.inspection()
            ),
            other => self.unexpected_value(&format!("Float({expected})"), other.as_ref()),
        }
        self
    }

    fn assert_float_list(&self, expected: &[f64]) -> &Self {
        self.assert_list_elements(expected, |idx, actual, expected| match actual {
            Value::Float(f) => assert!(
                (f - *expected).abs() < 1e-9,
                "float mismatch at index {idx}: expected {expected}, got {f}\n{}",
                self.inspection()
            ),
            other => panic!(
                "expected float list element at index {idx}, got {}\n{}",
                other.type_name(),
                self.inspection()
            ),
        })
    }

    fn assert_float_list_approx(&self, expected: &[f64], eps: f64) -> &Self {
        self.assert_list_elements(expected, |idx, actual, expected| match actual {
            Value::Float(f) => assert!(
                (f - *expected).abs() < eps,
                "float mismatch at index {idx}: expected {expected}, got {f}\n{}",
                self.inspection()
            ),
            Value::Integer(n) => assert!(
                (n as f64 - *expected).abs() < eps,
                "float mismatch at index {idx}: expected {expected}, got {n}\n{}",
                self.inspection()
            ),
            other => panic!(
                "expected numeric list element at index {idx}, got {}\n{}",
                other.type_name(),
                self.inspection()
            ),
        })
    }

    fn assert_int_list(&self, expected: &[i64]) -> &Self {
        self.assert_list_elements(expected, |idx, actual, expected| match actual {
            Value::Integer(n) => assert_eq!(
                n,
                *expected,
                "integer mismatch at index {idx}\n{}",
                self.inspection()
            ),
            other => panic!(
                "expected int list element at index {idx}, got {}\n{}",
                other.type_name(),
                self.inspection()
            ),
        })
    }

    fn assert_string(&self, expected: &str) -> &Self {
        self.assert_ok();
        let value = self.elided_value();
        match &value {
            Some(Value::String(s)) => {
                assert_eq!(s, expected, "string mismatch\n{}", self.inspection())
            }
            other => self.unexpected_value(&format!("String({expected:?})"), other.as_ref()),
        }
        self
    }

    fn assert_string_list(&self, expected: &[&str]) -> &Self {
        self.assert_list_elements(expected, |idx, actual, expected| match actual {
            Value::String(s) => assert_eq!(
                s,
                *expected,
                "string mismatch at index {idx}\n{}",
                self.inspection()
            ),
            other => panic!(
                "expected string list element at index {idx}, got {}\n{}",
                other.type_name(),
                self.inspection()
            ),
        })
    }

    fn assert_error(&self, fragment: &str) -> &Self {
        assert!(
            self.errors.iter().any(|e| e.contains(fragment)),
            "expected error containing {:?}, got: {:?}\n{}",
            fragment,
            self.errors,
            self.inspection()
        );
        self
    }

    fn assert_transcript(&self, expected: &[&str]) -> &Self {
        self.assert_ok();
        assert_eq!(
            self.transcript,
            expected
                .iter()
                .map(|entry| entry.to_string())
                .collect::<Vec<_>>(),
            "transcript mismatch\n{}",
            self.inspection()
        );
        self
    }

    fn assert_first_error_span(&self, expected: Span8) -> &Self {
        assert!(
            !self._error_spans.is_empty(),
            "expected at least one runtime error span\n{}",
            self.inspection()
        );
        assert_eq!(
            self._error_spans[0],
            expected,
            "runtime error span mismatch\n{}",
            self.inspection()
        );
        self
    }

    #[allow(dead_code)]
    fn assert_no_value(&self) -> &Self {
        self.assert_ok();
        assert!(
            self.value.is_none(),
            "expected no value, got {}\n{}",
            self.value_summary(),
            self.inspection()
        );
        self
    }
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
    let (section, parse_errors) = parse_section(src, section_type);
    if !parse_errors.is_empty() {
        return ExecResult {
            value: None,
            transcript: Vec::new(),
            errors: parse_errors,
            _error_spans: vec![],
        };
    }

    let stdlib_bundles: Vec<_> = stdlib_names.iter().copied().map(stdlib_bundle).collect();
    let imported_files: Vec<usize> = (0..stdlib_bundles.len()).collect();

    let user_bundle = make_section_bundle("scene.mcs", 0, imported_files, vec![section], None);

    let mut bundles = stdlib_bundles;
    bundles.push(user_bundle);

    let result = compile_bundles(&bundles);

    let compile_errors: Vec<String> = result.errors.iter().map(|e| e.message.clone()).collect();
    if !compile_errors.is_empty() {
        return ExecResult {
            value: None,
            transcript: Vec::new(),
            errors: compile_errors,
            _error_spans: vec![],
        };
    }

    // section 0 is the prelude; section 1 is our slide
    if result.bytecode.sections.len() < 2 {
        return ExecResult {
            value: None,
            transcript: Vec::new(),
            errors: vec!["no user section was compiled".into()],
            _error_spans: vec![],
        };
    }

    // -- execute --
    let mut executor = Executor::new(result.bytecode, registry().func_table());

    let mut runtime_errors: Vec<String> = Vec::new();

    smol::block_on(async {
        let target = executor.user_to_internal_timestamp(Timestamp::at_end_of_slide(1));
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
    let transcript = executor
        .state
        .transcript
        .iter_entries()
        .map(|entry| entry.text().to_string())
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
        transcript,
        errors: runtime_errors,
        _error_spans: error_spans,
    }
}

#[path = "basic_executor_tests/arithmetic.rs"]
mod arithmetic;
#[path = "basic_executor_tests/collections.rs"]
mod collections;
#[path = "basic_executor_tests/control_flow.rs"]
mod control_flow;
#[path = "basic_executor_tests/heap.rs"]
mod heap;
#[path = "basic_executor_tests/lambdas.rs"]
mod lambdas;
#[path = "basic_executor_tests/live_values.rs"]
mod live_values;
#[path = "basic_executor_tests/operators.rs"]
mod operators;
#[path = "basic_executor_tests/references.rs"]
mod references;
#[path = "basic_executor_tests/transcript.rs"]
mod transcript;
#[path = "basic_executor_tests/validation.rs"]
mod validation;
