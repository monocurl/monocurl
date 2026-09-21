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
    /// the overwhelmingly common case: comparing or combining two values that are
    /// already concrete. handled without suspending so the interpreter's inner
    /// loop does not have to poll a future for ordinary arithmetic
    pub(super) fn try_binary_op(&mut self, stack_idx: usize, op: BinOp) -> Option<ExecSingle> {
        let stack = self.state.stack(stack_idx);
        let rhs = stack.read_at(-1);
        let lhs = stack.read_at(-2);

        if matches!(op, BinOp::Eq | BinOp::Ne) {
            if matches!(lhs, Value::Stateful(_)) || matches!(rhs, Value::Stateful(_)) {
                return None;
            }
            let equal = Value::values_equal(lhs, rhs);
            let result = matches!(op, BinOp::Eq) == equal;

            let stack = self.state.stack_mut(stack_idx);
            stack.pop_n(2);
            stack.push(Value::Integer(result as i64));
            return Some(ExecSingle::Continue);
        }

        let lhs = resolved_numeric(lhs)?;
        let rhs = resolved_numeric(rhs)?;
        let value = eval_binary(&lhs, &rhs, op);

        let stack = self.state.stack_mut(stack_idx);
        stack.pop_n(2);
        Some(match value {
            Ok(value) => {
                stack.push(value);
                ExecSingle::Continue
            }
            Err(error) => ExecSingle::Error(error),
        })
    }

    pub(super) async fn exec_binary_op(&mut self, stack_idx: usize, op: BinOp) -> ExecSingle {
        if let Some(result) = self.try_binary_op(stack_idx, op) {
            return result;
        }

        let stack = self.state.stack_mut(stack_idx);
        let rhs = stack.pop();
        let lhs = stack.pop();

        if matches!(lhs, Value::Stateful(_)) || matches!(rhs, Value::Stateful(_)) {
            return ExecSingle::Error(ExecutorError::stateful_binary_op());
        }

        let lhs = elide_lvalue_leader_shallow(lhs);
        let rhs = elide_lvalue_leader_shallow(rhs);

        let lhs = match lhs.elide_wrappers_rec(self).await {
            Ok(val) => val,
            Err(e) => return ExecSingle::Error(e),
        };
        let rhs = match rhs.elide_wrappers_rec(self).await {
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

    /// sync fast path for unary minus on an already-concrete number
    pub(super) fn try_negate(&mut self, stack_idx: usize) -> Option<ExecSingle> {
        let value = match resolved_numeric(self.state.stack(stack_idx).read_at(-1))? {
            Value::Integer(n) => Value::Integer(-n),
            Value::Float(f) => Value::Float(-f),
            Value::Complex { re, im } => Value::Complex { re: -re, im: -im },
            _ => return None,
        };

        let stack = self.state.stack_mut(stack_idx);
        stack.pop();
        stack.push(value);
        Some(ExecSingle::Continue)
    }

    /// sync fast path for logical negation of an already-concrete number
    pub(super) fn try_not(&mut self, stack_idx: usize) -> Option<ExecSingle> {
        let truthy = resolved_numeric(self.state.stack(stack_idx).read_at(-1))?
            .check_truthy()
            .ok()?;

        let stack = self.state.stack_mut(stack_idx);
        stack.pop();
        stack.push(Value::Integer(!truthy as i64));
        Some(ExecSingle::Continue)
    }

    pub(super) async fn exec_negate(&mut self, val: Value) -> Result<Value, ExecutorError> {
        if matches!(val, Value::Stateful(_)) {
            return Err(ExecutorError::stateful_unary_op());
        }

        let val = val.elide_wrappers_rec(self).await?;

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

        let val = val.elide_wrappers_rec(self).await?;
        val.check_truthy()
            .map(|truthy| Value::Integer(!truthy as i64))
    }
}

/// read through lvalue and leader indirection to a concrete number, without
/// copying anything that is not a number. returns `None` for every other shape,
/// which routes the caller to the general asynchronous path
pub(super) fn resolved_numeric(value: &Value) -> Option<Value> {
    match value {
        // rebuilt rather than cloned: these payloads are Copy, and going through
        // Value::clone means an out-of-line call with a jump table
        Value::Integer(n) => Some(Value::Integer(*n)),
        Value::Float(f) => Some(Value::Float(*f)),
        Value::Complex { re, im } => Some(Value::Complex { re: *re, im: *im }),
        Value::Lvalue(reference) => with_heap(|heap| resolved_numeric(&heap.get(reference.key()))),
        Value::WeakLvalue(reference) => {
            with_heap(|heap| resolved_numeric(&heap.get(reference.key())))
        }
        Value::Leader(leader) => {
            with_heap(|heap| resolved_numeric(&heap.get(leader.leader_rc.key())))
        }
        _ => None,
    }
}

#[inline(always)]
fn elide_lvalue_leader_shallow(value: Value) -> Value {
    value.elide_lvalue().elide_leader()
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
    // matching numeric types need no promotion, so skip the copies it would take
    if matches!(
        (lhs, rhs),
        (Value::Integer(_), Value::Integer(_))
            | (Value::Float(_), Value::Float(_))
            | (Value::Complex { .. }, Value::Complex { .. })
    ) {
        return eval_non_list_binary(lhs, rhs, op);
    }

    match (lhs, rhs, op) {
        (Value::List(lhs_list), Value::List(rhs_list), BinOp::Add) => {
            return combine_lists(lhs_list, rhs_list, BinOp::Add);
        }
        (Value::List(lhs_list), Value::List(rhs_list), BinOp::Sub) => {
            return combine_lists(lhs_list, rhs_list, BinOp::Sub);
        }
        (Value::List(list), rhs, op) if op.is_list_scalar() && !matches!(rhs, Value::List(_)) => {
            return apply_list_scalar(list, rhs, op, false);
        }
        (lhs, Value::List(list), op) if op.is_list_scalar() && !matches!(lhs, Value::List(_)) => {
            return apply_list_scalar(list, lhs, op, true);
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
            Ok(Value::String(format!("{}{}", a, b).into()))
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

    Ok(Value::List(List::new_with(elements)))
}

fn combine_lists(lhs: &List, rhs: &List, op: BinOp) -> Result<Value, ExecutorError> {
    if lhs.len() != rhs.len() {
        return Err(ExecutorError::ListLengthMismatch {
            op: op.name(),
            lhs_len: lhs.len(),
            rhs_len: rhs.len(),
        });
    }

    let mut elements = Vec::with_capacity(lhs.len());
    for (idx, (lhs_key, rhs_key)) in lhs.elements.iter().zip(rhs.elements.iter()).enumerate() {
        let lhs_val = with_heap(|h| h.get(lhs_key.key()).clone());
        let rhs_val = with_heap(|h| h.get(rhs_key.key()).clone());
        let combined = match (lhs_val, rhs_val) {
            (Value::List(lhs_inner), Value::List(rhs_inner)) => {
                combine_lists(&lhs_inner, &rhs_inner, op)
                    .map_err(|err| list_index_err(op.name(), idx, err))?
            }
            (lhs_val, rhs_val) => eval_binary(&lhs_val, &rhs_val, op)
                .map_err(|err| list_index_err(op.name(), idx, err))?,
        };
        elements.push(crate::heap::VRc::new(combined));
    }

    Ok(Value::List(List::new_with(elements)))
}

fn apply_list_scalar(
    list: &List,
    scalar: &Value,
    op: BinOp,
    scalar_on_lhs: bool,
) -> Result<Value, ExecutorError> {
    let mut elements = Vec::with_capacity(list.len());

    for (idx, key) in list.elements.iter().enumerate() {
        let elem_value = with_heap(|h| h.get(key.key()).clone());
        let applied = match elem_value {
            Value::List(inner) => apply_list_scalar(&inner, scalar, op, scalar_on_lhs)
                .map_err(|err| list_index_err(op.name(), idx, err))?,
            other => {
                let (lhs, rhs) = if scalar_on_lhs {
                    (scalar.clone(), other)
                } else {
                    (other, scalar.clone())
                };
                eval_binary(&lhs, &rhs, op).map_err(|err| list_index_err(op.name(), idx, err))?
            }
        };
        elements.push(crate::heap::VRc::new(applied));
    }

    Ok(Value::List(List::new_with(elements)))
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
    fn is_list_scalar(self) -> bool {
        matches!(self, BinOp::Mul | BinOp::Div)
    }

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
