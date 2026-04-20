use std::rc::Rc;

use crate::{
    error::ExecutorError,
    heap::with_heap,
    value::{
        Value,
        container::{HashableKey, List},
    },
};

use super::{ExecSingle, Executor};

#[derive(Debug, Clone, Copy)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Power,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    IntDiv,
    In,
}

impl Executor {
    pub(super) async fn exec_binary_op(&mut self, stack_idx: usize, op: BinOp) -> ExecSingle {
        let stack = self.state.stack_mut(stack_idx);
        let rhs = stack.pop();
        let lhs = stack.pop();

        if matches!(lhs, Value::Stateful(_)) || matches!(rhs, Value::Stateful(_)) {
            return ExecSingle::Error(ExecutorError::stateful_binary_op());
        }

        if matches!(op, BinOp::Eq | BinOp::Ne) {
            let result = match op {
                BinOp::Eq => Value::values_equal(&lhs, &rhs),
                _ => !Value::values_equal(&lhs, &rhs),
            };
            self.state
                .stack_mut(stack_idx)
                .push(Value::Integer(result as i64));
            return ExecSingle::Continue;
        }

        let lhs = match lhs.elide_wrappers(self).await {
            Ok(val) => val,
            Err(e) => return ExecSingle::Error(e),
        };
        let rhs = match rhs.elide_wrappers(self).await {
            Ok(val) => val,
            Err(e) => return ExecSingle::Error(e),
        };

        match eval_binary(&lhs, &rhs, op) {
            Ok(val) => {
                self.state.stack_mut(stack_idx).push(val);
                ExecSingle::Continue
            }
            Err(e) => ExecSingle::Error(e),
        }
    }

    pub(super) async fn exec_negate(&mut self, val: Value) -> Result<Value, ExecutorError> {
        if matches!(val, Value::Stateful(_)) {
            return Err(ExecutorError::stateful_unary_op());
        }

        let val = val.elide_wrappers(self).await?;

        match &val {
            Value::Integer(n) => Ok(Value::Integer(-n)),
            Value::Float(f) => Ok(Value::Float(-f)),
            Value::Complex { re, im } => Ok(Value::Complex { re: -re, im: -im }),
            Value::List(list) => negate_list(list),
            _ => Err(ExecutorError::UnsupportedNegate(val.type_name())),
        }
    }

    pub(super) async fn exec_not(&mut self, val: Value) -> Result<Value, ExecutorError> {
        if matches!(val, Value::Stateful(_)) {
            return Err(ExecutorError::stateful_operator());
        }

        let val = val.elide_wrappers(self).await?;
        val.check_truthy()
            .map(|truthy| Value::Integer(!truthy as i64))
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

pub(crate) fn eval_binary(lhs: &Value, rhs: &Value, op: BinOp) -> Result<Value, ExecutorError> {
    match (lhs, rhs, op) {
        (Value::List(lhs_list), Value::List(rhs_list), BinOp::Add) => {
            return add_lists(lhs_list, rhs_list);
        }
        (Value::List(list), rhs, BinOp::Mul) if !matches!(rhs, Value::List(_)) => {
            return multiply_list(list, rhs, false);
        }
        (lhs, Value::List(list), BinOp::Mul) if !matches!(lhs, Value::List(_)) => {
            return multiply_list(list, lhs, true);
        }
        _ => {}
    }

    let (lhs, rhs) = promote_pair(lhs.clone(), rhs.clone());

    eval_non_list_binary(&lhs, &rhs, op)
}

fn eval_non_list_binary(lhs: &Value, rhs: &Value, op: BinOp) -> Result<Value, ExecutorError> {
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
            let found = list.elements.iter().any(|key| {
                let elem = with_heap(|h| h.get(key.key()).clone());
                Value::values_equal(lhs, &elem)
            });
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

fn negate_list(list: &List) -> Result<Value, ExecutorError> {
    let mut elements = Vec::with_capacity(list.elements.len());

    for (idx, key) in list.elements.iter().enumerate() {
        let value = with_heap(|h| h.get(key.key()).clone());
        let negated = match value {
            Value::Integer(n) => Value::Integer(-n),
            Value::Float(f) => Value::Float(-f),
            Value::Complex { re, im } => Value::Complex { re: -re, im: -im },
            Value::List(inner) => {
                negate_list(&inner).map_err(|err| list_index_err("negate", idx, err))?
            }
            other => {
                return Err(list_element_err(
                    "negate",
                    idx,
                    ExecutorError::UnsupportedNegate(other.type_name()),
                ));
            }
        };
        elements.push(crate::heap::VRc::new(negated));
    }

    Ok(Value::List(Rc::new(List {
        elements: elements.into(),
    })))
}

fn add_lists(lhs: &List, rhs: &List) -> Result<Value, ExecutorError> {
    if lhs.len() != rhs.len() {
        return Err(ExecutorError::ListLengthMismatch {
            op: BinOp::Add.name(),
            lhs_len: lhs.len(),
            rhs_len: rhs.len(),
        });
    }

    let mut elements = Vec::with_capacity(lhs.len());
    for (idx, (lhs_key, rhs_key)) in lhs.elements.iter().zip(rhs.elements.iter()).enumerate() {
        let lhs_val = with_heap(|h| h.get(lhs_key.key()).clone());
        let rhs_val = with_heap(|h| h.get(rhs_key.key()).clone());
        let sum = match (lhs_val, rhs_val) {
            (Value::List(lhs_inner), Value::List(rhs_inner)) => {
                add_lists(&lhs_inner, &rhs_inner)
                    .map_err(|err| list_index_err(BinOp::Add.name(), idx, err))?
            }
            (lhs_val, rhs_val) => eval_binary(&lhs_val, &rhs_val, BinOp::Add)
                .map_err(|err| list_index_err(BinOp::Add.name(), idx, err))?,
        };
        elements.push(crate::heap::VRc::new(sum));
    }

    Ok(Value::List(Rc::new(List {
        elements: elements.into(),
    })))
}

fn multiply_list(list: &List, scalar: &Value, scalar_on_lhs: bool) -> Result<Value, ExecutorError> {
    let mut elements = Vec::with_capacity(list.len());

    for (idx, key) in list.elements.iter().enumerate() {
        let elem_value = with_heap(|h| h.get(key.key()).clone());
        let product = match elem_value {
            Value::List(inner) => multiply_list(&inner, scalar, scalar_on_lhs)
                .map_err(|err| list_index_err(BinOp::Mul.name(), idx, err))?,
            other => {
                let (lhs, rhs) = if scalar_on_lhs {
                    (scalar.clone(), other)
                } else {
                    (other, scalar.clone())
                };
                eval_binary(&lhs, &rhs, BinOp::Mul)
                    .map_err(|err| list_index_err(BinOp::Mul.name(), idx, err))?
            }
        };
        elements.push(crate::heap::VRc::new(product));
    }

    Ok(Value::List(Rc::new(List {
        elements: elements.into(),
    })))
}

fn list_index_err(op: &'static str, idx: usize, err: ExecutorError) -> ExecutorError {
    ExecutorError::invalid_operation(format!(
        "cannot apply {} to list element [{}]: {}",
        op, idx, err
    ))
}

fn list_element_err(op: &'static str, idx: usize, err: ExecutorError) -> ExecutorError {
    ExecutorError::invalid_operation(format!("cannot {} list element [{}]: {}", op, idx, err))
}

fn eval_float_binary(a: f64, b: f64, op: BinOp) -> Result<Value, ExecutorError> {
    match op {
        BinOp::Add => Ok(Value::Float(a + b)),
        BinOp::Sub => Ok(Value::Float(a - b)),
        BinOp::Mul => Ok(Value::Float(a * b)),
        BinOp::Div => {
            if b == 0.0 {
                Err(ExecutorError::DivisionByZero)
            } else {
                Ok(Value::Float(a / b))
            }
        }
        BinOp::IntDiv => {
            if b == 0.0 {
                Err(ExecutorError::DivisionByZero)
            } else {
                Ok(Value::Integer((a / b).floor() as i64))
            }
        }
        BinOp::Power => Ok(Value::Float(a.powf(b))),
        BinOp::Lt => Ok(Value::Integer((a < b) as i64)),
        BinOp::Le => Ok(Value::Integer((a <= b) as i64)),
        BinOp::Gt => Ok(Value::Integer((a > b) as i64)),
        BinOp::Ge => Ok(Value::Integer((a >= b) as i64)),
        BinOp::Eq | BinOp::Ne | BinOp::In => {
            unreachable!("handled before promotion")
        }
    }
}

impl BinOp {
    pub(crate) fn name(self) -> &'static str {
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
