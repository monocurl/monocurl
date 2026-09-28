//! differential fuzzing of the numeric / list / control-flow / lambda subset of
//! the language. `gen` builds random programs as an ast, `reference` evaluates
//! that ast directly from the language semantics, and `check_seed` runs the
//! printed source through the real pipeline and compares the two transcripts.

pub mod r#gen;
pub mod reference;

use std::{fmt::Write, path::Path, rc::Rc};

use executor::executor::SeekOptions;

use crate::{corpus_dir, run_scene};

pub use r#gen::{Generated, generate};
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
}

impl Program {
    pub fn source(&self) -> String {
        let mut out = String::from("import std.util\nimport std.math\n\n");
        write_block(&mut out, &self.stmts, 0);
        out
    }
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
    let path = corpus_dir().join("fuzz.mcs");
    let run =
        std::panic::catch_unwind(|| run_scene(source, Path::new(&path), SeekOptions::strict()));
    match run {
        Ok(Ok(run)) => Outcome {
            transcript: run.transcript,
            error: run.runtime_errors.into_iter().next(),
        },
        Ok(Err(error)) => Outcome {
            transcript: Vec::new(),
            error: Some(format!("<pipeline> {error}")),
        },
        Err(_) => Outcome {
            transcript: Vec::new(),
            error: Some("<executor panicked>".into()),
        },
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
