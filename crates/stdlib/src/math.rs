use executor::{
    error::ExecutorError,
    executor::Executor,
    heap::{VRc, with_heap},
    value::Value,
};
use stdlib_macros::stdlib_func;

use crate::read_float;

// ── scalar unary helpers ─────────────────────────────────────────────────────

fn unary_f64(
    executor: &Executor,
    stack: usize,
    name: &'static str,
    f: impl Fn(f64) -> f64,
) -> Result<Value, ExecutorError> {
    let x = read_float(executor, stack, -1, name)?;
    Ok(Value::Float(f(x)))
}

fn binary_f64(
    executor: &Executor,
    stack: usize,
    lhs: &'static str,
    rhs: &'static str,
    f: impl Fn(f64, f64) -> f64,
) -> Result<Value, ExecutorError> {
    let a = read_float(executor, stack, -2, lhs)?;
    let b = read_float(executor, stack, -1, rhs)?;
    Ok(Value::Float(f(a, b)))
}

fn read_int(
    executor: &Executor,
    stack: usize,
    index: i32,
    name: &'static str,
) -> Result<i64, ExecutorError> {
    match executor.state.stack(stack).read_at(index) {
        Value::Integer(n) => Ok(*n),
        other => Err(ExecutorError::type_error_for(
            "int",
            other.type_name(),
            name,
        )),
    }
}

fn read_list(
    executor: &Executor,
    stack: usize,
    index: i32,
    name: &'static str,
) -> Result<Vec<f64>, ExecutorError> {
    match executor
        .state
        .stack(stack)
        .read_at(index)
        .clone()
        .elide_lvalue_leader_rec()
    {
        Value::List(list) => list
            .elements
            .iter()
            .map(|key| match with_heap(|h| h.get(key.key()).clone()) {
                Value::Float(f) => Ok(f),
                Value::Integer(n) => Ok(n as f64),
                other => Err(ExecutorError::type_error_for(
                    "number",
                    other.type_name(),
                    name,
                )),
            })
            .collect(),
        other => Err(ExecutorError::type_error_for(
            "list",
            other.type_name(),
            name,
        )),
    }
}

// ── elementary scalars ───────────────────────────────────────────────────────

#[stdlib_func]
pub async fn sqrt(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::sqrt)
}

#[stdlib_func]
pub async fn cbrt(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::cbrt)
}

#[stdlib_func]
pub async fn exp(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::exp)
}

#[stdlib_func]
pub async fn ln(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::ln)
}

#[stdlib_func]
pub async fn pow(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    binary_f64(executor, stack_idx, "base", "exp", f64::powf)
}

// ── trig ─────────────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn sin(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::sin)
}

#[stdlib_func]
pub async fn cos(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::cos)
}

#[stdlib_func]
pub async fn tan(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::tan)
}

#[stdlib_func]
pub async fn arcsin(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::asin)
}

#[stdlib_func]
pub async fn arccos(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::acos)
}

#[stdlib_func]
pub async fn arctan(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::atan)
}

#[stdlib_func]
pub async fn arctan2(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    binary_f64(executor, stack_idx, "y", "x", f64::atan2)
}

#[stdlib_func]
pub async fn sinh(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::sinh)
}

#[stdlib_func]
pub async fn cosh(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::cosh)
}

#[stdlib_func]
pub async fn tanh(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    unary_f64(executor, stack_idx, "x", f64::tanh)
}

// ── rounding / sign ──────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn abs(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor.state.stack(stack_idx).read_at(-1) {
        Value::Integer(n) => Ok(Value::Integer(n.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        other => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            "x",
        )),
    }
}

#[stdlib_func]
pub async fn sign(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    match executor.state.stack(stack_idx).read_at(-1) {
        Value::Integer(n) => Ok(Value::Integer(n.signum())),
        Value::Float(f) => Ok(Value::Float(f.signum())),
        other => Err(ExecutorError::type_error_for(
            "number",
            other.type_name(),
            "x",
        )),
    }
}

#[stdlib_func]
pub async fn floor(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x = read_float(executor, stack_idx, -1, "x")?;
    Ok(Value::Integer(x.floor() as i64))
}

#[stdlib_func]
pub async fn ceil(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x = read_float(executor, stack_idx, -1, "x")?;
    Ok(Value::Integer(x.ceil() as i64))
}

#[stdlib_func]
pub async fn round(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x = read_float(executor, stack_idx, -1, "x")?;
    Ok(Value::Integer(x.round() as i64))
}

#[stdlib_func]
pub async fn trunc(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let x = read_float(executor, stack_idx, -1, "x")?;
    Ok(Value::Integer(x.trunc() as i64))
}

#[stdlib_func]
pub async fn mod_func(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let n = read_float(executor, stack_idx, -2, "n")?;
    let m = read_float(executor, stack_idx, -1, "m")?;
    if m == 0.0 {
        return Err(ExecutorError::DivisionByZero);
    }
    Ok(Value::Float(n.rem_euclid(m)))
}

#[stdlib_func]
pub async fn min(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    binary_f64(executor, stack_idx, "a", "b", f64::min)
}

#[stdlib_func]
pub async fn max(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    binary_f64(executor, stack_idx, "a", "b", f64::max)
}

// ── combinatorics & number theory ────────────────────────────────────────────

#[stdlib_func]
pub async fn factorial(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let n = read_int(executor, stack_idx, -1, "n")?;
    if n < 0 {
        return Err(ExecutorError::InvalidArgument {
            arg: "n",
            message: "must be non-negative",
        });
    }
    Ok(Value::Integer((1..=n).product()))
}

#[stdlib_func]
pub async fn choose(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let n = read_int(executor, stack_idx, -2, "n")?;
    let r = read_int(executor, stack_idx, -1, "r")?;
    if r < 0 || r > n {
        return Ok(Value::Integer(0));
    }
    let r = r.min(n - r);
    let mut result: i64 = 1;
    for i in 0..r {
        result = result * (n - i) / (i + 1);
    }
    Ok(Value::Integer(result))
}

#[stdlib_func]
pub async fn permute(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let n = read_int(executor, stack_idx, -2, "n")?;
    let r = read_int(executor, stack_idx, -1, "r")?;
    if r < 0 || r > n {
        return Ok(Value::Integer(0));
    }
    Ok(Value::Integer((n - r + 1..=n).product()))
}

#[stdlib_func]
pub async fn gcd(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let a = read_int(executor, stack_idx, -2, "n")?.abs();
    let b = read_int(executor, stack_idx, -1, "m")?.abs();
    let (mut a, mut b) = (a, b);
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    Ok(Value::Integer(a))
}

#[stdlib_func]
pub async fn lcm(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let a = read_int(executor, stack_idx, -2, "n")?;
    let b = read_int(executor, stack_idx, -1, "m")?;
    if a == 0 || b == 0 {
        return Ok(Value::Integer(0));
    }
    let (mut x, mut y) = (a.abs(), b.abs());
    while y != 0 {
        let t = y;
        y = x % y;
        x = t;
    }
    Ok(Value::Integer((a / x).abs() * b.abs()))
}

#[stdlib_func]
pub async fn is_prime(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let n = read_int(executor, stack_idx, -1, "n")?;
    let prime = match n {
        n if n < 2 => false,
        2 | 3 => true,
        n if n % 2 == 0 => false,
        n => {
            let mut i = 3i64;
            loop {
                if i * i > n {
                    break true;
                }
                if n % i == 0 {
                    break false;
                }
                i += 2;
            }
        }
    };
    Ok(Value::Integer(prime as i64))
}

// ── randomness ───────────────────────────────────────────────────────────────

fn rand_f64() -> f64 {
    use std::cell::Cell;
    thread_local!(static STATE: Cell<u64> = const { Cell::new(0x9E37_79B9_7F4A_7C15) });
    STATE.with(|s| {
        let mut x = s.get().wrapping_add(0x9E37_79B9_7F4A_7C15);
        s.set(x);
        x ^= x >> 30;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^= x >> 27;
        x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^= x >> 31;
        (x >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    })
}

#[stdlib_func]
pub async fn random(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let low = read_float(executor, stack_idx, -2, "low")?;
    let high = read_float(executor, stack_idx, -1, "high")?;
    Ok(Value::Float(low + rand_f64() * (high - low)))
}

#[stdlib_func]
pub async fn randint(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let low = read_int(executor, stack_idx, -2, "low")?;
    let high = read_int(executor, stack_idx, -1, "high")?;
    if high <= low {
        return Err(ExecutorError::InvalidArgument {
            arg: "high",
            message: "must be greater than low",
        });
    }
    let span = (high - low) as f64;
    Ok(Value::Integer(low + (rand_f64() * span) as i64))
}

// ── statistics ───────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn mean(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let v = read_list(executor, stack_idx, -1, "v")?;
    if v.is_empty() {
        return Err(ExecutorError::InvalidArgument {
            arg: "v",
            message: "cannot compute mean of empty list",
        });
    }
    Ok(Value::Float(v.iter().sum::<f64>() / v.len() as f64))
}

#[stdlib_func]
pub async fn variance(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let v = read_list(executor, stack_idx, -1, "v")?;
    if v.is_empty() {
        return Err(ExecutorError::InvalidArgument {
            arg: "v",
            message: "cannot compute variance of empty list",
        });
    }
    let mu = v.iter().sum::<f64>() / v.len() as f64;
    Ok(Value::Float(
        v.iter().map(|x| (x - mu).powi(2)).sum::<f64>() / v.len() as f64,
    ))
}

// ── vectors ──────────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn dot(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let u = read_list(executor, stack_idx, -2, "u")?;
    let v = read_list(executor, stack_idx, -1, "v")?;
    if u.len() != v.len() {
        return Err(ExecutorError::ListLengthMismatch {
            op: "dot",
            lhs_len: u.len(),
            rhs_len: v.len(),
        });
    }
    Ok(Value::Float(
        u.iter().zip(v.iter()).map(|(a, b)| a * b).sum(),
    ))
}

#[stdlib_func]
pub async fn cross(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    use executor::value::container::List;
    use smallvec::smallvec;

    let u = read_list(executor, stack_idx, -2, "u")?;
    let v = read_list(executor, stack_idx, -1, "v")?;
    if u.len() != 3 || v.len() != 3 {
        return Err(ExecutorError::InvalidArgument {
            arg: "u",
            message: "cross product requires 3-vectors",
        });
    }
    let out = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    Ok(Value::List(std::rc::Rc::new(List {
        elements: smallvec![
            VRc::new(Value::Float(out[0])),
            VRc::new(Value::Float(out[1])),
            VRc::new(Value::Float(out[2])),
        ],
    })))
}

// ── interpolation ────────────────────────────────────────────────────────────

#[stdlib_func]
pub async fn lerp(executor: &mut Executor, stack_idx: usize) -> Result<Value, ExecutorError> {
    let stack = executor.state.stack(stack_idx);
    let alpha = stack.read_at(-3).clone();
    let beta = stack.read_at(-2).clone();
    let t = read_float(executor, stack_idx, -1, "t")?;
    executor.lerp(alpha, beta, t).await
}

#[stdlib_func]
pub async fn keyframe_lerp(
    _executor: &mut Executor,
    _stack_idx: usize,
) -> Result<Value, ExecutorError> {
    todo!("piecewise lerp over a keyframe list")
}
