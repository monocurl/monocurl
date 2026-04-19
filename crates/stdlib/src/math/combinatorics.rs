use executor::{error::ExecutorError, executor::Executor, value::Value};
use stdlib_macros::stdlib_func;

use super::helpers::read_int;

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
    let low = crate::read_float(executor, stack_idx, -2, "low")?;
    let high = crate::read_float(executor, stack_idx, -1, "high")?;
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
