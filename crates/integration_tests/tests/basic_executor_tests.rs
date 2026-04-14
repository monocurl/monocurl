// lex → parse → compile → execute

use std::{f64, sync::Arc};

use compiler::cache::CompilerCache;
use executor::{
    executor::{Executor, SeekToResult},
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
    rope::{Rope, TextAggregate},
    text::Span8,
};

struct ExecResult {
    /// the value captured from the root execution head's TOS, if any
    value: Option<Value>,
    /// compile-time or runtime error messages
    errors: Vec<String>,
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
    // -- parse --
    let tokens = lex(src);
    let rope: Rope<TextAggregate> = Rope::from_str(src);
    let mut parser = SectionParser::new(tokens, rope, SectionType::Slide, None, None);
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
        };
    }

    let bundle = Arc::new(SectionBundle {
        file_path: None,
        file_index: 0,
        imported_files: vec![],
        sections: vec![Section {
            body: stmts,
            section_type: SectionType::Slide,
        }],
        root_import_span: None,
        was_cached: false,
    });

    let mut cache = CompilerCache::default();
    let result = compiler::compiler::compile(&mut cache, None, &[bundle]);

    let compile_errors: Vec<String> = result.errors.iter().map(|e| e.message.clone()).collect();
    if !compile_errors.is_empty() {
        return ExecResult {
            value: None,
            errors: compile_errors,
        };
    }

    // section 0 is the prelude; section 1 is our slide
    if result.bytecode.sections.len() < 2 {
        return ExecResult {
            value: None,
            errors: vec!["no user section was compiled".into()],
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
        match executor.seek_to(Timestamp::new(1, f64::INFINITY)).await {
            SeekToResult::SeekedTo(_) => {}
            SeekToResult::Error(e) => {
                runtime_errors.push(e.to_string());
            }
        }
    });

    runtime_errors.extend(executor.state.errors.iter().cloned());

    let value = executor.state.captured_output.into_iter().last();
    ExecResult {
        value,
        errors: runtime_errors,
    }
}

// -- literals and arithmetic --

#[test]
fn test_exec_literal_int() {
    let r = run("let x = 42");
    r.assert_int(42);
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
    r.assert_int(42);
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
    r.assert_int(32);
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
    r.assert_int(32);
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
    r.assert_int(19);
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
    r.assert_int(90);
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
fn test_exec_runtime_error_index_out_of_bounds() {
    let r = run("
        let xs = [1, 2, 3]
        let result = xs[5]
    ");
    r.assert_error("out of bounds");
}

#[test]
fn test_exec_runtime_error_type_in_arithmetic() {
    let r = run(r#"let x = "hello" - 1"#);
    r.assert_error("unsupported");
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
        var x = 0
        let mutate = |&y| {
            y = y + 1
        }
        mutate(&x)
        let result = x
    ");
    r.assert_int(1);
}

#[test]
fn test_ref_mutation_does_not_affect_unrelated_var() {
    let r = run("
        var x = 10
        var z = 99
        let inc = |&y| {
            y = y + 1
        }
        inc(&x)
        let result = z
    ");
    r.assert_int(99);
}

#[test]
fn test_ref_called_multiple_times() {
    let r = run("
        var x = 0
        let inc = |&y| {
            y = y + 1
        }
        inc(&x)
        inc(&x)
        inc(&x)
        let result = x
    ");
    r.assert_int(3);
}

#[test]
fn test_ref_chain_of_lambdas() {
    // inner passes its reference argument straight through to another lambda
    let r = run("
        var x = 0
        let add_two = |&y| {
            y = y + 2
        }
        let double_add = |&z| {
            add_two(&z)
            add_two(&z)
        }
        double_add(&x)
        let result = x
    ");
    r.assert_int(4);
}

#[test]
fn test_ref_two_distinct_references() {
    let r = run("
        var a = 1
        var b = 10
        let modify_both = |&x, &y| {
            x = x + 1
            y = y + 1
        }
        modify_both(&a, &b)
        let result = a + b
    ");
    // a=2, b=11, result=13
    r.assert_int(13);
}

#[test]
fn test_ref_reference_to_list_via_ref() {
    // pass the whole list by reference; subscript-assign inside the lambda
    let r = run("
        var arr = [0, 0, 0]
        let set_first = |&a| {
            a[0] = 42
        }
        set_first(&arr)
        let result = arr[0]
    ");
    r.assert_int(42);
}

#[test]
fn test_ref_destructure_list_references() {
    // pass a list of references using list destructure assignment inside the lambda
    let r = run("
        var a = 0
        var b = 0
        let set_both = |&x, &y| {
            x = 7
            y = 13
        }
        set_both(&a, &b)
        let result = a + b
    ");
    r.assert_int(20);
}

#[test]
fn test_ref_reference_in_closure_capture() {
    // lambda captures a var by value; separate reference arg must not alias the capture
    let r = run("
        let captured = 5
        var target = 0
        let f = |&r| {
            r = captured + 1
        }
        f(&target)
        let result = target
    ");
    r.assert_int(6);
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
