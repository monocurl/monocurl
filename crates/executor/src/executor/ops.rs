use crate::{
    error::ExecutorError,
    value::{Value, container::HashableKey},
};

use super::{BinOp, ExecSingle, Executor};

impl Executor {
    pub(super) async fn exec_binary_op(&mut self, stack_idx: usize, op: BinOp) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let rhs = stack.pop();
        let lhs = stack.pop();

        // equality ops use structural comparison — no wrapper resolution
        match op {
            BinOp::Eq => {
                let result = Value::values_equal(&lhs, &rhs);
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Integer(result as i64));
                return ExecSingle::Continue;
            }
            BinOp::Ne => {
                let result = !Value::values_equal(&lhs, &rhs);
                self.state
                    .stack_mut(stack_idx)
                    .push(Value::Integer(result as i64));
                return ExecSingle::Continue;
            }
            _ => {}
        }

        let lhs = match lhs.elide_wrappers(self).await {
            Ok(val) => val,
            Err(e) => return ExecSingle::Error(e),
        };
        let rhs = match rhs.elide_wrappers(self).await {
            Ok(val) => val,
            Err(e) => return ExecSingle::Error(e),
        };

        // type promotion: int -> float -> complex
        let (lhs, rhs) = promote_pair(lhs, rhs);

        match eval_binary(&lhs, &rhs, op) {
            Ok(val) => {
                self.state.stack_mut(stack_idx).push(val);
                ExecSingle::Continue
            }
            Err(e) => ExecSingle::Error(e),
        }
    }

    pub(super) async fn exec_negate(&mut self, val: Value) -> Result<Value, ExecutorError> {
        let val = match val.elide_wrappers(self).await {
            Ok(val) => val,
            Err(e) => return Err(e),
        };

        match &val {
            Value::Integer(n) => Ok(Value::Integer(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            Value::Complex { re, im } => Ok(Value::Complex { re: -re, im: -im }),
            _ => Err(ExecutorError::UnsupportedNegate(val.type_name())),
        }
    }
}

/// promote a pair of values so mixed int/float/complex operations work.
/// int+float -> both float; float+complex -> both complex; int+complex -> both complex.
fn promote_pair(lhs: Value, rhs: Value) -> (Value, Value) {
    match (&lhs, &rhs) {
        (Value::Integer(a), Value::Float(_)) => (Value::Float(*a as f64), rhs),
        (Value::Float(_), Value::Integer(b)) => (lhs, Value::Float(*b as f64)),
        (Value::Integer(a), Value::Complex { .. }) => (
            Value::Complex {
                re: *a as f64,
                im: 0.0,
            },
            rhs,
        ),
        (Value::Complex { .. }, Value::Integer(b)) => (
            lhs,
            Value::Complex {
                re: *b as f64,
                im: 0.0,
            },
        ),
        (Value::Float(a), Value::Complex { .. }) => (Value::Complex { re: *a, im: 0.0 }, rhs),
        (Value::Complex { .. }, Value::Float(b)) => (lhs, Value::Complex { re: *b, im: 0.0 }),
        _ => (lhs, rhs),
    }
}

fn eval_binary(lhs: &Value, rhs: &Value, op: BinOp) -> Result<Value, ExecutorError> {
    match (lhs, rhs, op) {
        // int x int
        (Value::Integer(a), Value::Integer(b), BinOp::Add) => Ok(Value::Integer(a + b)),
        (Value::Integer(a), Value::Integer(b), BinOp::Sub) => Ok(Value::Integer(a - b)),
        (Value::Integer(a), Value::Integer(b), BinOp::Mul) => Ok(Value::Integer(a * b)),
        (Value::Integer(a), Value::Integer(b), BinOp::Div) => {
            if *b == 0 {
                Err(ExecutorError::DivisionByZero)
            } else {
                Ok(Value::Float(*a as f64 / *b as f64))
            }
        }
        (Value::Integer(a), Value::Integer(b), BinOp::IntDiv) => {
            if *b == 0 {
                Err(ExecutorError::DivisionByZero)
            } else {
                Ok(Value::Integer(a / b))
            }
        }
        (Value::Integer(a), Value::Integer(b), BinOp::Power) => {
            Ok(Value::Float((*a as f64).powf(*b as f64)))
        }
        (Value::Integer(a), Value::Integer(b), BinOp::Lt) => Ok(Value::Integer((a < b) as i64)),
        (Value::Integer(a), Value::Integer(b), BinOp::Le) => Ok(Value::Integer((a <= b) as i64)),
        (Value::Integer(a), Value::Integer(b), BinOp::Gt) => Ok(Value::Integer((a > b) as i64)),
        (Value::Integer(a), Value::Integer(b), BinOp::Ge) => Ok(Value::Integer((a >= b) as i64)),

        // float x float (after promotion, all float pairs land here)
        (Value::Float(a), Value::Float(b), _) => eval_float_binary(*a, *b, op),

        // complex x complex (after promotion)
        (Value::Complex { re: ar, im: ai }, Value::Complex { re: br, im: bi }, BinOp::Add) => {
            Ok(Value::Complex {
                re: ar + br,
                im: ai + bi,
            })
        }
        (Value::Complex { re: ar, im: ai }, Value::Complex { re: br, im: bi }, BinOp::Sub) => {
            Ok(Value::Complex {
                re: ar - br,
                im: ai - bi,
            })
        }
        (Value::Complex { re: ar, im: ai }, Value::Complex { re: br, im: bi }, BinOp::Mul) => {
            Ok(Value::Complex {
                re: ar * br - ai * bi,
                im: ar * bi + ai * br,
            })
        }
        (Value::Complex { re: ar, im: ai }, Value::Complex { re: br, im: bi }, BinOp::Div) => {
            let denom = br * br + bi * bi;
            if denom == 0.0 {
                Err(ExecutorError::DivisionByZero)
            } else {
                Ok(Value::Complex {
                    re: (ar * br + ai * bi) / denom,
                    im: (ai * br - ar * bi) / denom,
                })
            }
        }

        // string concatenation
        (Value::String(a), Value::String(b), BinOp::Add) => {
            Ok(Value::String(format!("{}{}", a, b)))
        }

        // in operator: resolved rhs must be a list or map
        (_, Value::List(list), BinOp::In) => {
            let found = list
                .elements
                .iter()
                .any(|rc| Value::values_equal(lhs, &rc.borrow()));
            Ok(Value::Integer(found as i64))
        }
        (_, Value::Map(map), BinOp::In) => {
            let key = HashableKey::try_from_value(lhs)?;
            Ok(Value::Integer(map.contains_key(&key) as i64))
        }

        _ => Err(ExecutorError::UnsupportedBinaryOp {
            op: op.name(),
            lhs: lhs.type_name(),
            rhs: rhs.type_name(),
        }),
    }
}

fn eval_float_binary(a: f64, b: f64, op: BinOp) -> Result<Value, ExecutorError> {
    Ok(match op {
        BinOp::Add => Value::Float(a + b),
        BinOp::Sub => Value::Float(a - b),
        BinOp::Mul => Value::Float(a * b),
        BinOp::Div => {
            if b == 0.0 {
                return Err(ExecutorError::DivisionByZero);
            }
            Value::Float(a / b)
        }
        BinOp::IntDiv => Value::Float((a / b).floor()),
        BinOp::Power => Value::Float(a.powf(b)),
        BinOp::Lt => Value::Integer((a < b) as i64),
        BinOp::Le => Value::Integer((a <= b) as i64),
        BinOp::Gt => Value::Integer((a > b) as i64),
        BinOp::Ge => Value::Integer((a >= b) as i64),
        BinOp::Eq | BinOp::Ne | BinOp::In => unreachable!("handled before promotion"),
    })
}

impl BinOp {
    fn name(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Power => "**",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::IntDiv => "//",
            BinOp::In => "in",
        }
    }
}
