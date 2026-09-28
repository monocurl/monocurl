//! differential fuzzing of the numeric / list / control-flow / lambda subset of
//! the language. `gen` builds random programs as an ast, `reference` evaluates
//! that ast directly from the language semantics, and `check_seed` runs the
//! printed source through the real pipeline and compares the two transcripts.
//!
//! batch mode (`check_batch_seed`) also samples the program's lambdas through
//! the constructors the kernel tier runs as batches (`ExplicitFunc`, `Shader`,
//! `point_map`, ..) and runs the scene with the tier off, on and verifying, so
//! the dynamic, typed and lane machines are compared with the interpreter.

pub mod r#gen;
pub mod reference;

use std::{fmt::Write, path::Path, rc::Rc};

use executor::{
    executor::SeekOptions,
    kernel::{KernelMode, KernelStats},
};

use crate::{corpus_dir, run_scene_with_kernels};

pub use r#gen::{Generated, generate, generate_batches};
pub use reference::{Outcome, evaluate};

#[derive(Clone, Debug)]
pub enum Expr {
    Int(i64),
    Float(f64),
    List(Vec<Expr>),
    Name(Rc<str>),
    Neg(Box<Expr>),
    Not(Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    Index(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    Native(Native, Vec<Expr>),
    Lambda(Rc<Lambda>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,
    Pow,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    And,
    Or,
}

impl BinOp {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Add => "+",
            Self::Sub => "-",
            Self::Mul => "*",
            Self::Div => "/",
            Self::IntDiv => "//",
            Self::Pow => "^",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::And => "and",
            Self::Or => "or",
        }
    }
}

/// stdlib functions reachable through `std.math` / `std.util`
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Native {
    Sin,
    Cos,
    Tan,
    Exp,
    Ln,
    Sqrt,
    Abs,
    Sign,
    Floor,
    Ceil,
    Round,
    Trunc,
    Min,
    Max,
    Mod,
    Clamp,
    Arctan2,
    Dot,
    Norm,
    Lerp,
    Len,
    Sum,
    Range,
}

impl Native {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sin => "sin",
            Self::Cos => "cos",
            Self::Tan => "tan",
            Self::Exp => "exp",
            Self::Ln => "ln",
            Self::Sqrt => "sqrt",
            Self::Abs => "abs",
            Self::Sign => "sign",
            Self::Floor => "floor",
            Self::Ceil => "ceil",
            Self::Round => "round",
            Self::Trunc => "trunc",
            Self::Min => "min",
            Self::Max => "max",
            Self::Mod => "mod",
            Self::Clamp => "clamp",
            Self::Arctan2 => "arctan2",
            Self::Dot => "dot",
            Self::Norm => "norm",
            Self::Lerp => "lerp",
            Self::Len => "len",
            Self::Sum => "sum",
            Self::Range => "range",
        }
    }
}

#[derive(Debug)]
pub struct Lambda {
    pub params: Vec<Param>,
    pub body: Body,
}

#[derive(Debug)]
pub struct Param {
    pub name: Rc<str>,
    pub default: Option<Expr>,
}

#[derive(Debug)]
pub enum Body {
    Expr(Expr),
    Block(Vec<Stmt>),
}

#[derive(Clone, Debug)]
pub enum Stmt {
    Let(Rc<str>, Expr),
    Var(Rc<str>, Expr),
    Assign(Rc<str>, Expr),
    AssignIndex(Rc<str>, Expr, Expr),
    Append(Rc<str>, Expr),
    If(Vec<(Expr, Vec<Stmt>)>, Option<Vec<Stmt>>),
    While(Expr, Vec<Stmt>),
    For(Rc<str>, Expr, Vec<Stmt>),
    Return(Expr),
    Print(Expr),
}

#[derive(Debug)]
pub struct Program {
    pub stmts: Vec<Stmt>,
    /// generated in the mode that deliberately triggers a runtime error
    pub expects_error: bool,
    /// source lines after `stmts` that sample its lambdas through batch
    /// constructors; the reference does not model them
    pub batches: Vec<String>,
}

impl Program {
    pub fn source(&self) -> String {
        let mut out = String::from("import std.util\nimport std.math\n");
        if !self.batches.is_empty() {
            out.push_str("import std.mesh\n");
        }
        out.push('\n');
        write_block(&mut out, &self.stmts, 0);
        for line in &self.batches {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

/// `expr` as source, on one line unless it holds a block lambda
pub fn expr_source(expr: &Expr) -> String {
    let mut out = String::new();
    write_expr(&mut out, expr, 0);
    out
}

fn indent(out: &mut String, level: usize) {
    out.extend(std::iter::repeat_n("    ", level));
}

fn write_block(out: &mut String, stmts: &[Stmt], level: usize) {
    for stmt in stmts {
        write_stmt(out, stmt, level);
    }
}

fn write_braced(out: &mut String, stmts: &[Stmt], level: usize) {
    out.push_str("{\n");
    write_block(out, stmts, level + 1);
    indent(out, level);
    out.push('}');
}

fn write_stmt(out: &mut String, stmt: &Stmt, level: usize) {
    indent(out, level);
    match stmt {
        Stmt::Let(name, value) => {
            let _ = write!(out, "let {name} = ");
            write_expr(out, value, level);
        }
        Stmt::Var(name, value) => {
            let _ = write!(out, "var {name} = ");
            write_expr(out, value, level);
        }
        Stmt::Assign(name, value) => {
            let _ = write!(out, "{name} = ");
            write_expr(out, value, level);
        }
        Stmt::AssignIndex(name, index, value) => {
            let _ = write!(out, "{name}[");
            write_expr(out, index, level);
            out.push_str("] = ");
            write_expr(out, value, level);
        }
        Stmt::Append(name, value) => {
            let _ = write!(out, "{name} .= ");
            write_expr(out, value, level);
        }
        Stmt::If(branches, otherwise) => {
            for (i, (condition, body)) in branches.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                    indent(out, level);
                    out.push_str("else ");
                }
                out.push_str("if (");
                write_expr(out, condition, level);
                out.push_str(") ");
                write_braced(out, body, level);
            }
            if let Some(body) = otherwise {
                out.push('\n');
                indent(out, level);
                out.push_str("else ");
                write_braced(out, body, level);
            }
        }
        Stmt::While(condition, body) => {
            out.push_str("while (");
            write_expr(out, condition, level);
            out.push_str(") ");
            write_braced(out, body, level);
        }
        Stmt::For(binder, iterable, body) => {
            let _ = write!(out, "for ({binder} in ");
            write_expr(out, iterable, level);
            out.push_str(") ");
            write_braced(out, body, level);
        }
        Stmt::Return(value) => {
            out.push_str("return ");
            write_expr(out, value, level);
        }
        Stmt::Print(value) => {
            out.push_str("print ");
            write_expr(out, value, level);
        }
    }
    out.push('\n');
}

fn write_list(out: &mut String, items: &[Expr], level: usize) {
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_expr(out, item, level);
    }
}

/// binary and unary operators are always parenthesised, so the printed source
/// never depends on the parser's precedence table
fn write_expr(out: &mut String, expr: &Expr, level: usize) {
    match expr {
        Expr::Int(value) if *value < 0 => {
            let _ = write!(out, "(-{})", value.unsigned_abs());
        }
        Expr::Int(value) => {
            let _ = write!(out, "{value}");
        }
        Expr::Float(value) if value.is_sign_negative() => {
            let _ = write!(out, "(-{:?})", -value);
        }
        Expr::Float(value) => {
            let _ = write!(out, "{value:?}");
        }
        Expr::List(items) => {
            out.push('[');
            write_list(out, items, level);
            out.push(']');
        }
        Expr::Name(name) => out.push_str(name),
        Expr::Neg(operand) => {
            out.push_str("(-");
            write_expr(out, operand, level);
            out.push(')');
        }
        Expr::Not(operand) => {
            out.push_str("(not ");
            write_expr(out, operand, level);
            out.push(')');
        }
        Expr::Binary(op, lhs, rhs) => {
            out.push('(');
            write_expr(out, lhs, level);
            let _ = write!(out, " {} ", op.symbol());
            write_expr(out, rhs, level);
            out.push(')');
        }
        Expr::Index(base, index) => {
            write_expr(out, base, level);
            out.push('[');
            write_expr(out, index, level);
            out.push(']');
        }
        Expr::Call(callee, args) => {
            write_expr(out, callee, level);
            out.push('(');
            write_list(out, args, level);
            out.push(')');
        }
        Expr::Native(native, args) => {
            out.push_str(native.name());
            out.push('(');
            write_list(out, args, level);
            out.push(')');
        }
        Expr::Lambda(lambda) => {
            out.push('|');
            for (i, param) in lambda.params.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                out.push_str(&param.name);
                if let Some(default) = &param.default {
                    out.push_str(" = ");
                    write_expr(out, default, level);
                }
            }
            out.push_str("| ");
            match &lambda.body {
                Body::Expr(body) => write_expr(out, body, level),
                Body::Block(stmts) => write_braced(out, stmts, level),
            }
        }
    }
}

/// one generated program and what both evaluators made of it
pub struct Case {
    pub seed: u64,
    /// why earlier candidates for this seed were thrown away
    pub discarded: Vec<&'static str>,
    pub source: String,
    pub expected: Outcome,
    pub actual: Outcome,
}

impl Case {
    pub fn matches(&self) -> bool {
        self.expected.error.as_deref().map(first_line)
            == self.actual.error.as_deref().map(first_line)
            && self.expected.transcript.len() == self.actual.transcript.len()
            && self
                .expected
                .transcript
                .iter()
                .zip(&self.actual.transcript)
                .all(|(expected, actual)| lines_agree(expected, actual))
    }

    pub fn report(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "fuzz mismatch for seed {}", self.seed);
        let _ = writeln!(out, "reproduce with: mcfuzz --seed {} --print", self.seed);
        let _ = writeln!(out, "--- source ---\n{}", self.source);
        let _ = writeln!(out, "--- reference ---\n{}", self.expected);
        let _ = writeln!(out, "--- executor ---\n{}", self.actual);
        out
    }
}

fn first_line(message: &str) -> &str {
    message.lines().next().unwrap_or_default()
}

/// exact match, or equal up to a 1e-9 relative difference in float tokens.
/// integer tokens (no `.`) must match exactly so int/float confusion still fails
fn lines_agree(expected: &str, actual: &str) -> bool {
    if expected == actual {
        return true;
    }
    let tokens = |line: &str| -> Vec<String> {
        line.split(['[', ']', ',', ' '])
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let (expected, actual) = (tokens(expected), tokens(actual));
    expected.len() == actual.len()
        && expected.iter().zip(&actual).all(|(e, a)| {
            if e == a {
                return true;
            }
            let floats = e.contains('.') && a.contains('.');
            match (floats, e.parse::<f64>(), a.parse::<f64>()) {
                (true, Ok(e), Ok(a)) => (e - a).abs() <= 1e-9 * e.abs().max(a.abs()),
                _ => false,
            }
        })
}

/// run a program through lex -> parse -> compile -> execute
pub fn execute_source(source: &str) -> Outcome {
    execute_with_kernels(source, None).outcome
}

/// one run of the real pipeline
pub struct Run {
    pub outcome: Outcome,
    pub stats: KernelStats,
    /// the executor panicked; in verify mode this is an engine disagreement
    pub panic: Option<String>,
}

/// stack for the thread each run gets; the main thread's size, with room to spare
const RUN_STACK_BYTES: usize = 64 << 20;

/// run a program with the kernel tier pinned to `mode` (`None` leaves the
/// environment's choice). each run gets a fresh thread and so a fresh
/// thread-local heap: slots one run leaks would otherwise be copied by every
/// later run's heap snapshots, and a long campaign slows to a crawl
pub fn execute_with_kernels(source: &str, mode: Option<KernelMode>) -> Run {
    let source = source.to_owned();
    std::thread::Builder::new()
        .stack_size(RUN_STACK_BYTES)
        .spawn(move || execute_here(&source, mode))
        .expect("spawn a fuzz run thread")
        .join()
        .expect("fuzz runs catch their own panics")
}

fn execute_here(source: &str, mode: Option<KernelMode>) -> Run {
    let path = corpus_dir().join("fuzz.mcs");
    let run = std::panic::catch_unwind(|| {
        run_scene_with_kernels(source, Path::new(&path), SeekOptions::strict(), mode)
    });
    match run {
        Ok(Ok(run)) => Run {
            outcome: Outcome {
                transcript: run.transcript,
                error: run.runtime_errors.into_iter().next(),
            },
            stats: run.kernel_stats,
            panic: None,
        },
        Ok(Err(error)) => Run {
            outcome: Outcome {
                transcript: Vec::new(),
                error: Some(format!("<pipeline> {error}")),
            },
            stats: KernelStats::default(),
            panic: None,
        },
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "<non-string panic>".into());
            Run {
                outcome: Outcome {
                    transcript: Vec::new(),
                    error: Some("<executor panicked>".into()),
                },
                stats: KernelStats::default(),
                panic: Some(message),
            }
        }
    }
}

/// generate the program for `seed`, evaluate it both ways, and compare
pub fn check_seed(seed: u64) -> Case {
    let generated = generate(seed);
    let source = generated.program.source();
    let actual = execute_source(&source);
    Case {
        seed,
        discarded: generated.discarded,
        source,
        expected: generated.expected,
        actual,
    }
}

/// keep every batch on the calling thread. a worker's panic is swallowed and
/// the batch handed back to the interpreter, which would hide verify mode's
/// disagreements. call before any executor runs
pub fn use_serial_kernels() {
    // SAFETY: called before any worker threads touch the environment
    unsafe { std::env::set_var("MONOCURL_KERNEL_THREADS", "1") };
}

/// a batch-mode program run with the kernel tier off, on and verifying
pub struct BatchCase {
    pub seed: u64,
    pub discarded: Vec<&'static str>,
    pub source: String,
    /// the reference's view of the statements before the batch section
    pub expected: Outcome,
    pub off: Run,
    pub on: Run,
    pub verify: Run,
}

impl BatchCase {
    /// everything that went wrong, empty when the case agrees
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        for (label, run) in [
            ("off", &self.off),
            ("on", &self.on),
            ("verify", &self.verify),
        ] {
            if let Some(message) = &run.panic {
                problems.push(format!("kernels {label} panicked: {message}"));
            }
        }
        if self.on.outcome != self.off.outcome {
            problems.push("kernels on changes the transcript or error".into());
        }
        if self.verify.panic.is_none() && self.verify.outcome != self.off.outcome {
            problems.push("kernels verify changes the transcript or error".into());
        }
        // the batch section only appends, so the reference's transcript is a
        // prefix of the interpreter's. an error is the batch section's own
        // once every reference line is out
        let off = &self.off.outcome.transcript;
        let prefix_agrees = off.len() >= self.expected.transcript.len()
            && self
                .expected
                .transcript
                .iter()
                .zip(off)
                .all(|(expected, actual)| lines_agree(expected, actual));
        if self.off.panic.is_none() && !prefix_agrees {
            problems.push("the interpreter disagrees with the reference".into());
        }
        problems
    }

    pub fn matches(&self) -> bool {
        self.problems().is_empty()
    }

    pub fn report(&self) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "batch fuzz mismatch for seed {}", self.seed);
        let _ = writeln!(
            out,
            "reproduce with: mcfuzz --batches --seed {} --print",
            self.seed
        );
        for problem in self.problems() {
            let _ = writeln!(out, "  {problem}");
        }
        let _ = writeln!(out, "--- source ---\n{}", self.source);
        let _ = writeln!(
            out,
            "--- reference (before the batches) ---\n{}",
            self.expected
        );
        let _ = writeln!(out, "--- kernels off ---\n{}", self.off.outcome);
        let _ = writeln!(out, "--- kernels on ---\n{}", self.on.outcome);
        let _ = writeln!(out, "--- kernels verify ---\n{}", self.verify.outcome);
        out
    }
}

/// generate the batch-mode program for `seed` and run it under every kernel mode
pub fn check_batch_seed(seed: u64) -> BatchCase {
    let generated = generate_batches(seed);
    let source = generated.program.source();
    let [off, on, verify] = [KernelMode::Off, KernelMode::On, KernelMode::Verify]
        .map(|mode| execute_with_kernels(&source, Some(mode)));
    BatchCase {
        seed,
        discarded: generated.discarded,
        source,
        expected: generated.expected,
        off,
        on,
        verify,
    }
}
