//! independent tree-walking evaluator for the fuzzed subset, written from the
//! language semantics rather than by calling into the executor. besides
//! evaluating, it is the generator's filter: programs that stray into undefined
//! or platform-sensitive territory (nan, huge magnitudes, runaway loops) or
//! into a known executor bug (see `FUZZ_FINDINGS.md`) are rejected here instead
//! of being compared.

use std::{fmt, rc::Rc};

use super::{BinOp, Body, Expr, Lambda, Native, Program, Stmt};

/// numbers beyond this are rejected: keeps i64 products far from overflow
/// (which panics in debug builds) and floats in a well-printed range
const MAGNITUDE_LIMIT: f64 = 1e9;
const STEP_LIMIT: usize = 200_000;
const CALL_DEPTH_LIMIT: usize = 48;
const RANGE_LIMIT: i64 = 64;

#[derive(Clone, Debug)]
pub enum Val {
    Int(i64),
    Float(f64),
    List(Rc<Vec<Val>>),
    Lambda(Rc<Closure>),
    /// the result of calling a lambda that has default parameters. such a
    /// result remembers its call, so `lerp` between two calls of the same
    /// lambda interpolates their arguments; everywhere else it is just the
    /// wrapped value
    Live(Rc<LiveCall>),
}

#[derive(Clone, Debug)]
pub struct LiveCall {
    value: Val,
    /// the lambda and its full argument list (defaults filled in); `None` for
    /// stdlib calls, which the fuzzer never interpolates
    call: Option<(Rc<Closure>, Vec<Val>)>,
}

impl LiveCall {
    fn wrap(value: Val, call: Option<(Rc<Closure>, Vec<Val>)>) -> Val {
        Val::Live(Rc::new(Self { value, call }))
    }
}

#[derive(Debug)]
pub struct Closure {
    lambda: Rc<Lambda>,
    captured: Vec<(Rc<str>, Val)>,
}

impl Val {
    fn type_name(&self) -> &'static str {
        match self {
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::List(_) => "list",
            Self::Lambda(_) => "lambda",
            Self::Live(_) => "live function",
        }
    }

    fn list(items: Vec<Val>) -> Self {
        Self::List(Rc::new(items))
    }

    /// the plain value, through any live-call wrappers
    fn peel(&self) -> &Val {
        match self {
            Self::Live(live) => live.value.peel(),
            other => other,
        }
    }
}

/// transcript rendering, mirroring what `print` shows
impl fmt::Display for Val {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Int(n) => write!(f, "{n}"),
            Self::Float(x) if x.fract() == 0.0 => write!(f, "{x:.1}"),
            Self::Float(x) => write!(f, "{x}"),
            Self::List(items) => {
                f.write_str("[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{item}")?;
                }
                f.write_str("]")
            }
            Self::Lambda(_) => f.write_str("<lambda>"),
            Self::Live(live) => write!(f, "{}", live.value),
        }
    }
}

/// what a program printed, and the runtime error that stopped it, if any
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Outcome {
    pub transcript: Vec<String>,
    pub error: Option<String>,
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for line in &self.transcript {
            writeln!(f, "{line}")?;
        }
        match &self.error {
            Some(error) => write!(f, "!! {error}"),
            None => write!(f, "(no error)"),
        }
    }
}

enum Stop {
    /// a runtime error the language defines; both sides must agree on it
    Error(String),
    /// the program left the subset whose behaviour we compare
    Reject(&'static str),
}

type Eval<T> = Result<T, Stop>;

fn error<T>(message: impl Into<String>) -> Eval<T> {
    Err(Stop::Error(message.into()))
}

fn reject<T>(reason: &'static str) -> Eval<T> {
    Err(Stop::Reject(reason))
}

fn division_by_zero<T>() -> Eval<T> {
    error("division by zero")
}

/// evaluate `program`; `Err` carries the reason it was rejected
pub fn evaluate(program: &Program) -> Result<Outcome, &'static str> {
    let mut interpreter = Interpreter::default();
    let error = match interpreter.exec_block(&program.stmts) {
        Ok(_) => None,
        Err(Stop::Error(message)) => Some(message),
        Err(Stop::Reject(reason)) => return Err(reason),
    };
    Ok(Outcome {
        transcript: interpreter.transcript,
        error,
    })
}

#[derive(Default)]
struct Interpreter {
    /// names are unique per program, so a flat binding stack with scope marks
    /// is enough to model lexical scoping
    bindings: Vec<(Rc<str>, Val)>,
    transcript: Vec<String>,
    steps: usize,
    depth: usize,
}

impl Interpreter {
    fn tick(&mut self) -> Eval<()> {
        self.steps += 1;
        if self.steps > STEP_LIMIT {
            return reject("step limit");
        }
        Ok(())
    }

    fn slot(&mut self, name: &str) -> &mut Val {
        self.bindings
            .iter_mut()
            .rev()
            .find(|(bound, _)| &**bound == name)
            .map(|(_, value)| value)
            .unwrap_or_else(|| panic!("generator referenced unbound name {name}"))
    }

    /// the list a variable holds, for in-place edits
    fn list_slot(&mut self, name: &str) -> Eval<&mut Vec<Val>> {
        let mut slot = self.slot(name);
        while let Val::Live(live) = slot {
            slot = &mut Rc::make_mut(live).value;
        }
        match slot {
            Val::List(items) => Ok(Rc::make_mut(items)),
            _ => reject("list edit on a non-list"),
        }
    }

    /// run statements in a fresh scope; `Some` when a `return` fired
    fn exec_block(&mut self, stmts: &[Stmt]) -> Eval<Option<Val>> {
        let mark = self.bindings.len();
        let result = stmts
            .iter()
            .try_fold(None, |returned, stmt| match returned {
                Some(value) => Ok(Some(value)),
                None => self.exec(stmt),
            });
        self.bindings.truncate(mark);
        result
    }

    fn exec(&mut self, stmt: &Stmt) -> Eval<Option<Val>> {
        self.tick()?;
        match stmt {
            Stmt::Let(name, value) | Stmt::Var(name, value) => {
                let value = self.eval(value)?;
                self.bindings.push((name.clone(), value));
            }
            Stmt::Assign(name, value) => {
                let value = self.eval(value)?;
                *self.slot(name) = value;
            }
            Stmt::AssignIndex(name, index, value) => {
                let index = self.eval(index)?;
                let value = self.eval(value)?;
                let items = self.list_slot(name)?;
                let position = list_position(index.peel(), items.len())?;
                items[position] = value;
            }
            Stmt::Append(name, value) => {
                let value = self.eval(value)?;
                self.list_slot(name)?.push(value);
            }
            Stmt::If(branches, otherwise) => {
                for (condition, body) in branches {
                    if truthy(&self.eval(condition)?)? {
                        return self.exec_block(body);
                    }
                }
                if let Some(body) = otherwise {
                    return self.exec_block(body);
                }
            }
            Stmt::While(condition, body) => {
                while truthy(&self.eval(condition)?)? {
                    if let Some(returned) = self.exec_block(body)? {
                        return Ok(Some(returned));
                    }
                }
            }
            Stmt::For(binder, iterable, body) => {
                let Val::List(items) = self.eval(iterable)?.peel().clone() else {
                    return reject("iterating a non-list");
                };
                for item in items.iter() {
                    // loop binders receive plain values
                    self.bindings.push((binder.clone(), item.peel().clone()));
                    let returned = self.exec_block(body);
                    self.bindings.pop();
                    if let Some(returned) = returned? {
                        return Ok(Some(returned));
                    }
                }
            }
            Stmt::Return(value) => return self.eval(value).map(Some),
            Stmt::Print(value) => {
                let value = self.eval(value)?;
                self.transcript.push(value.to_string());
            }
        }
        Ok(None)
    }

    fn eval(&mut self, expr: &Expr) -> Eval<Val> {
        self.tick()?;
        let value = match expr {
            Expr::Int(n) => Val::Int(*n),
            Expr::Float(x) => Val::Float(*x),
            Expr::List(items) => Val::list(
                items
                    .iter()
                    .map(|item| self.eval(item))
                    .collect::<Eval<_>>()?,
            ),
            Expr::Name(name) => self.slot(name).clone(),
            Expr::Neg(operand) => negate(self.eval(operand)?.peel())?,
            Expr::Not(operand) => Val::Int(i64::from(!truthy(&self.eval(operand)?)?)),
            // short circuit: `a and b` is b when a is truthy, else 0; `a or b`
            // is 1 when a is truthy, else b
            Expr::Binary(BinOp::And, lhs, rhs) => match truthy(&self.eval(lhs)?)? {
                true => self.eval(rhs)?,
                false => Val::Int(0),
            },
            Expr::Binary(BinOp::Or, lhs, rhs) => match truthy(&self.eval(lhs)?)? {
                true => Val::Int(1),
                false => self.eval(rhs)?,
            },
            Expr::Binary(op @ (BinOp::Eq | BinOp::Ne), lhs, rhs) => {
                let (lhs, rhs) = (self.eval(lhs)?, self.eval(rhs)?);
                Val::Int(i64::from(equal(&lhs, &rhs) == (*op == BinOp::Eq)))
            }
            Expr::Binary(op, lhs, rhs) => {
                let (lhs, rhs) = (self.eval(lhs)?, self.eval(rhs)?);
                binary(*op, lhs.peel(), rhs.peel())?
            }
            Expr::Index(base, index) => {
                let (base, index) = (self.eval(base)?, self.eval(index)?);
                let Val::List(items) = base.peel() else {
                    return error(format!("cannot subscript {}", base.peel().type_name()));
                };
                items[list_position(index.peel(), items.len())?]
                    .peel()
                    .clone()
            }
            // arguments are evaluated before the callee, which decides which
            // error wins in `make(..)(..)` when both sides fail
            Expr::Call(callee, args) => {
                let args = args
                    .iter()
                    .map(|arg| self.eval(arg))
                    .collect::<Eval<Vec<_>>>()?;
                let callee = self.eval(callee)?;
                self.call(&callee, args)?
            }
            Expr::Native(native, args) => {
                let args = args
                    .iter()
                    .map(|arg| self.eval(arg))
                    .collect::<Eval<Vec<_>>>()?;
                match native {
                    Native::Lerp => self.lerp(&args[0], &args[1], as_float(args[2].peel())?)?,
                    native => native_call(*native, &args)?,
                }
            }
            Expr::Lambda(lambda) => Val::Lambda(Rc::new(Closure {
                lambda: lambda.clone(),
                // lambdas may only capture immutable bindings, so a snapshot of
                // everything visible is indistinguishable from capturing by name
                captured: self.bindings.clone(),
            })),
        };
        check_magnitude(&value)?;
        Ok(value)
    }

    fn call(&mut self, callee: &Val, args: Vec<Val>) -> Eval<Val> {
        let closure = match callee.peel() {
            Val::Lambda(closure) => closure,
            other => {
                return error(format!(
                    "type error: expected lambda, got {}",
                    other.type_name()
                ));
            }
        };
        let params = &closure.lambda.params;
        let required = params
            .iter()
            .filter(|param| param.default.is_none())
            .count();
        if args.len() > params.len() {
            return error(format!(
                "too many positional arguments: expected at most {}, got {}",
                params.len(),
                args.len()
            ));
        }
        if args.len() < required {
            return error(format!(
                "too few positional arguments: expected at least {required}, got {}",
                args.len()
            ));
        }

        self.depth += 1;
        if self.depth > CALL_DEPTH_LIMIT {
            return reject("call depth");
        }

        let saved = std::mem::replace(&mut self.bindings, closure.captured.clone());
        let result = self.bind_and_run(&closure.lambda, args);
        self.bindings = saved;
        self.depth -= 1;

        let (value, full_args) = result?;
        Ok(match required < params.len() {
            true => LiveCall::wrap(value, Some((closure.clone(), full_args))),
            false => value,
        })
    }

    /// bind parameters and run the body; also returns the full argument list
    fn bind_and_run(&mut self, lambda: &Lambda, args: Vec<Val>) -> Eval<(Val, Vec<Val>)> {
        let mut args = args.into_iter();
        let mut full_args = Vec::with_capacity(lambda.params.len());
        for param in &lambda.params {
            let value = match (args.next(), &param.default) {
                (Some(value), _) => value,
                (None, Some(default)) => self.eval(default)?,
                (None, None) => unreachable!("arity checked by the caller"),
            };
            full_args.push(value.clone());
            self.bindings.push((param.name.clone(), value));
        }
        let value = match &lambda.body {
            Body::Expr(body) => self.eval(body)?,
            Body::Block(stmts) => self
                .exec_block(stmts)?
                .ok_or(Stop::Reject("block lambda without return"))?,
        };
        Ok((value, full_args))
    }

    /// `lerp(a, b, t)`. two calls of the same lambda interpolate their
    /// arguments and call it again; everything else interpolates values
    fn lerp(&mut self, a: &Val, b: &Val, t: f64) -> Eval<Val> {
        let (Val::Live(a_live), Val::Live(b_live)) = (a, b) else {
            return lerp(a, b, t);
        };
        let (Some((closure, a_args)), Some((b_closure, b_args))) = (&a_live.call, &b_live.call)
        else {
            return reject("lerp of stdlib calls");
        };
        if !Rc::ptr_eq(&closure.lambda, &b_closure.lambda) {
            return blend(a.peel(), b.peel(), t);
        }
        if a_args.iter().zip(b_args).all(|(x, y)| equal(x, y)) {
            return Ok(a.clone());
        }
        match a_args
            .iter()
            .zip(b_args)
            .map(|(x, y)| lerp_argument(x, y, t))
            .collect::<Eval<Option<Vec<_>>>>()?
        {
            Some(args) => self.call(&Val::Lambda(closure.clone()), args),
            // an argument that cannot be interpolated falls back to the
            // results, which keep the equal-endpoints rule
            None => lerp(a.peel(), b.peel(), t),
        }
    }
}

/// one argument of a call-to-call lerp; `None` when it cannot be interpolated
fn lerp_argument(a: &Val, b: &Val, t: f64) -> Eval<Option<Val>> {
    if equal(a, b) {
        return Ok(Some(a.clone()));
    }
    Ok(match (a, b) {
        (Val::Int(_) | Val::Float(_), Val::Int(_) | Val::Float(_)) => Some(blend(a, b, t)?),
        (Val::List(x), Val::List(y)) if x.len() == y.len() => x
            .iter()
            .zip(y.iter())
            .map(|(x, y)| lerp_argument(x, y, t))
            .collect::<Eval<Option<Vec<_>>>>()?
            .map(Val::list),
        _ => None,
    })
}

/// `(1 - t) * a + t * b` on two numbers
fn blend(a: &Val, b: &Val, t: f64) -> Eval<Val> {
    match (a, b) {
        (Val::Int(_) | Val::Float(_), Val::Int(_) | Val::Float(_)) => {
            let (x, y) = (as_float(a)?, as_float(b)?);
            Ok(Val::Float((1.0 - t) * x + t * y))
        }
        _ => reject("lerp of non-numbers"),
    }
}

fn native_call(native: Native, args: &[Val]) -> Eval<Val> {
    let float = |i: usize| as_float(args[i].peel());
    Ok(match native {
        Native::Sin => Val::Float(float(0)?.sin()),
        Native::Cos => Val::Float(float(0)?.cos()),
        Native::Tan => Val::Float(float(0)?.tan()),
        Native::Exp => Val::Float(float(0)?.exp()),
        Native::Ln => Val::Float(float(0)?.ln()),
        Native::Sqrt => Val::Float(float(0)?.sqrt()),
        Native::Arctan2 => Val::Float(float(0)?.atan2(float(1)?)),
        Native::Abs => match args[0].peel() {
            Val::Int(n) => Val::Int(n.abs()),
            Val::Float(x) => Val::Float(x.abs()),
            other => return error(type_error("number", other, "x")),
        },
        Native::Sign => match args[0].peel() {
            Val::Int(n) => Val::Int(n.signum()),
            Val::Float(x) if *x == 0.0 => Val::Float(0.0),
            Val::Float(x) => Val::Float(x.signum()),
            other => return error(type_error("number", other, "x")),
        },
        Native::Floor => to_int(float(0)?.floor())?,
        Native::Ceil => to_int(float(0)?.ceil())?,
        Native::Round => to_int(float(0)?.round())?,
        Native::Trunc => to_int(float(0)?.trunc())?,
        Native::Min => number_pair(args[0].peel(), args[1].peel(), i64::min, f64::min)?,
        Native::Max => number_pair(args[0].peel(), args[1].peel(), i64::max, f64::max)?,
        Native::Mod => match (args[0].peel(), args[1].peel()) {
            (Val::Int(_), Val::Int(0)) => return division_by_zero(),
            (Val::Int(n), Val::Int(m)) => Val::Int(n.rem_euclid(*m)),
            (n, m) => {
                let (n, m) = (number(n, "n")?, number(m, "m")?);
                if m == 0.0 {
                    return division_by_zero();
                }
                Val::Float(n.rem_euclid(m))
            }
        },
        // std.math: clamp = |low, x, high| min(high, max(low, x))
        Native::Clamp => {
            let lower = number_pair(args[0].peel(), args[1].peel(), i64::max, f64::max)?;
            number_pair(args[2].peel(), &lower, i64::min, f64::min)?
        }
        Native::Dot => dot(&args[0], &args[1])?,
        // std.math: norm = |v| sqrt(dot(v, v))
        Native::Norm => Val::Float(as_float(&dot(&args[0], &args[0])?)?.sqrt()),
        Native::Lerp => unreachable!("lerp may call back into the interpreter"),
        Native::Len => match args[0].peel() {
            Val::List(items) => Val::Int(items.len() as i64),
            other => {
                return error(format!(
                    "type error: expected list / map / string, got {}",
                    other.type_name()
                ));
            }
        },
        // std.util: sum folds `+` from the integer 0
        Native::Sum => {
            let Val::List(items) = args[0].peel() else {
                return reject("sum of a non-list");
            };
            items.iter().try_fold(Val::Int(0), |total, item| {
                binary(BinOp::Add, &total, item.peel())
            })?
        }
        // std.util: range = |start, stop, step = 1|, so its result is live too
        Native::Range => {
            let (Val::Int(start), Val::Int(stop)) = (args[0].peel(), args[1].peel()) else {
                return reject("non-integer range");
            };
            if stop - start > RANGE_LIMIT {
                return reject("long range");
            }
            LiveCall::wrap(Val::list((*start..*stop).map(Val::Int).collect()), None)
        }
    })
}

fn type_error(expected: &str, got: &Val, target: &str) -> String {
    format!(
        "type error: expected {expected}, got {} for {target}",
        got.type_name()
    )
}

fn as_float(value: &Val) -> Eval<f64> {
    match value {
        Val::Int(n) => Ok(*n as f64),
        Val::Float(x) => Ok(*x),
        other => error(type_error("float", other, "x")),
    }
}

fn number(value: &Val, target: &str) -> Eval<f64> {
    match value {
        Val::Int(n) => Ok(*n as f64),
        Val::Float(x) => Ok(*x),
        other => error(type_error("number", other, target)),
    }
}

/// int when both sides are ints, float otherwise (`min`, `max`)
fn number_pair(
    a: &Val,
    b: &Val,
    ints: fn(i64, i64) -> i64,
    floats: fn(f64, f64) -> f64,
) -> Eval<Val> {
    Ok(match (a, b) {
        (Val::Int(a), Val::Int(b)) => Val::Int(ints(*a, *b)),
        _ => Val::Float(floats(number(a, "a")?, number(b, "b")?)),
    })
}

fn to_int(x: f64) -> Eval<Val> {
    if x.abs() > MAGNITUDE_LIMIT {
        return reject("huge float to int");
    }
    Ok(Val::Int(x as i64))
}

fn numbers(value: &Val) -> Eval<Vec<f64>> {
    match value.peel() {
        Val::List(items) => items.iter().map(|item| number(item.peel(), "v")).collect(),
        other => error(type_error("list", other, "u")),
    }
}

fn dot(u: &Val, v: &Val) -> Eval<Val> {
    let (u, v) = (numbers(u)?, numbers(v)?);
    if u.len() != v.len() {
        return error(length_mismatch("dot", u.len(), v.len()));
    }
    Ok(Val::Float(u.iter().zip(&v).map(|(a, b)| a * b).sum()))
}

/// equal endpoints come back unchanged (so ints stay ints); otherwise a
/// float blend
fn lerp(a: &Val, b: &Val, t: f64) -> Eval<Val> {
    let (plain_a, plain_b) = (a.peel(), b.peel());
    if equal(plain_a, plain_b) {
        return Ok(plain_a.clone());
    }
    blend(plain_a, plain_b, t)
}

fn length_mismatch(op: &str, lhs: usize, rhs: usize) -> String {
    format!(
        "cannot apply {op} to lists of different lengths: lhs has length {lhs}, rhs has length {rhs}"
    )
}

fn check_magnitude(value: &Val) -> Eval<()> {
    match value {
        Val::List(items) => items.iter().try_for_each(check_magnitude),
        Val::Live(live) => check_magnitude(&live.value),
        Val::Int(n) if n.unsigned_abs() as f64 > MAGNITUDE_LIMIT => reject("huge int"),
        Val::Float(x) if !x.is_finite() => reject("non-finite float"),
        Val::Float(x) if x.abs() > MAGNITUDE_LIMIT => reject("huge float"),
        _ => Ok(()),
    }
}

fn truthy(value: &Val) -> Eval<bool> {
    match value.peel() {
        Val::Int(n) => Ok(*n != 0),
        Val::Float(x) => Ok(*x != 0.0),
        other => error(format!(
            "{} has no truthiness — use a numeric or boolean expression instead",
            other.type_name()
        )),
    }
}

fn list_position(index: &Val, len: usize) -> Eval<usize> {
    let Val::Int(index) = index else {
        return error(format!(
            "type error: expected int, got {}",
            index.type_name()
        ));
    };
    // negative indices wrap to huge positions, exactly as the message reports them
    let index = *index as usize;
    if index >= len {
        return error(format!("index {index} out of bounds (len {len})"));
    }
    Ok(index)
}

/// structural equality; ints and floats compare by value
fn equal(a: &Val, b: &Val) -> bool {
    match (a.peel(), b.peel()) {
        (Val::Int(a), Val::Int(b)) => a == b,
        (Val::Float(a), Val::Float(b)) => a == b,
        (Val::Int(a), Val::Float(b)) | (Val::Float(b), Val::Int(a)) => *a as f64 == *b,
        (Val::List(a), Val::List(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(a, b)| equal(a, b))
        }
        (Val::Lambda(a), Val::Lambda(b)) => Rc::ptr_eq(&a.lambda, &b.lambda),
        _ => false,
    }
}

/// unary minus; lists negate elementwise, recursing into nested lists
fn negate(value: &Val) -> Eval<Val> {
    Ok(match value {
        Val::Int(n) => Val::Int(-n),
        Val::Float(x) => Val::Float(-x),
        Val::List(items) => Val::list(
            items
                .iter()
                .enumerate()
                .map(|(i, item)| match item.peel() {
                    Val::Lambda(_) => error(format!(
                        "cannot negate list element [{i}]: cannot negate lambda"
                    )),
                    item => negate(item).map_err(|stop| wrap(stop, "negate", i)),
                })
                .collect::<Eval<_>>()?,
        ),
        Val::Lambda(_) => return error("cannot negate lambda"),
        Val::Live(live) => return negate(&live.value),
    })
}

fn wrap(stop: Stop, op: &str, index: usize) -> Stop {
    match stop {
        Stop::Error(message) => Stop::Error(format!(
            "cannot apply {op} to list element [{index}]: {message}"
        )),
        reject => reject,
    }
}

/// `lhs op rhs` on plain operands, rejecting results (including list elements)
/// that leave the compared magnitude range
fn binary(op: BinOp, lhs: &Val, rhs: &Val) -> Eval<Val> {
    let value = binary_unchecked(op, lhs, rhs)?;
    check_magnitude(&value)?;
    Ok(value)
}

/// a list element as the executor's elementwise operators read it: through
/// any live-call wrapper
fn element(value: &Val) -> Eval<&Val> {
    Ok(value.peel())
}

fn binary_unchecked(op: BinOp, lhs: &Val, rhs: &Val) -> Eval<Val> {
    use Val::{Float, Int, List};

    match (op, lhs, rhs) {
        (BinOp::Eq, _, _) => return Ok(Int(i64::from(equal(lhs, rhs)))),
        (BinOp::Ne, _, _) => return Ok(Int(i64::from(!equal(lhs, rhs)))),
        // lists combine elementwise under + and -, recursing into nested lists
        (BinOp::Add | BinOp::Sub, List(a), List(b)) => {
            if a.len() != b.len() {
                return error(length_mismatch(op.symbol(), a.len(), b.len()));
            }
            return Ok(Val::list(
                a.iter()
                    .zip(b.iter())
                    .enumerate()
                    .map(|(i, (a, b))| {
                        binary(op, element(a)?, element(b)?)
                            .map_err(|stop| wrap(stop, op.symbol(), i))
                    })
                    .collect::<Eval<_>>()?,
            ));
        }
        // * and / broadcast a scalar over a (possibly nested) list, either side
        (BinOp::Mul | BinOp::Div, List(items), scalar) if !matches!(scalar, List(_)) => {
            return broadcast(op, items, scalar, false);
        }
        (BinOp::Mul | BinOp::Div, scalar, List(items)) if !matches!(scalar, List(_)) => {
            return broadcast(op, items, scalar, true);
        }
        _ => {}
    }

    match (lhs, rhs) {
        (Int(a), Int(b)) => Ok(match op {
            BinOp::Add => Int(a + b),
            BinOp::Sub => Int(a - b),
            BinOp::Mul => Int(a.checked_mul(*b).ok_or(Stop::Reject("int overflow"))?),
            BinOp::Div if *b == 0 => return division_by_zero(),
            BinOp::Div => Float(*a as f64 / *b as f64),
            BinOp::IntDiv if *b == 0 => return division_by_zero(),
            // floor division, like the float form
            BinOp::IntDiv => Int(a.div_euclid(*b) - i64::from(*b < 0 && a.rem_euclid(*b) != 0)),
            BinOp::Pow => Float((*a as f64).powf(*b as f64)),
            BinOp::Lt => Int(i64::from(a < b)),
            BinOp::Le => Int(i64::from(a <= b)),
            BinOp::Gt => Int(i64::from(a > b)),
            BinOp::Ge => Int(i64::from(a >= b)),
            BinOp::Eq | BinOp::Ne | BinOp::And | BinOp::Or => unreachable!(),
        }),
        (Int(_) | Float(_), Int(_) | Float(_)) => {
            let (a, b) = (as_float(lhs)?, as_float(rhs)?);
            Ok(match op {
                BinOp::Add => Float(a + b),
                BinOp::Sub => Float(a - b),
                BinOp::Mul => Float(a * b),
                BinOp::Div if b == 0.0 => return division_by_zero(),
                BinOp::Div => Float(a / b),
                BinOp::IntDiv if b == 0.0 => return division_by_zero(),
                BinOp::IntDiv => to_int((a / b).floor())?,
                BinOp::Pow => Float(a.powf(b)),
                BinOp::Lt => Int(i64::from(a < b)),
                BinOp::Le => Int(i64::from(a <= b)),
                BinOp::Gt => Int(i64::from(a > b)),
                BinOp::Ge => Int(i64::from(a >= b)),
                BinOp::Eq | BinOp::Ne | BinOp::And | BinOp::Or => unreachable!(),
            })
        }
        _ => error(format!(
            "unsupported binary op {} on {} and {}",
            // the executor names power `**` in its messages
            if op == BinOp::Pow { "**" } else { op.symbol() },
            lhs.type_name(),
            rhs.type_name()
        )),
    }
}

fn broadcast(op: BinOp, items: &[Val], scalar: &Val, scalar_on_lhs: bool) -> Eval<Val> {
    Ok(Val::list(
        items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let result = match element(item)? {
                    Val::List(inner) => broadcast(op, inner, scalar, scalar_on_lhs),
                    item if scalar_on_lhs => binary(op, scalar, item),
                    item => binary(op, item, scalar),
                };
                result.map_err(|stop| wrap(stop, op.symbol(), i))
            })
            .collect::<Eval<_>>()?,
    ))
}
