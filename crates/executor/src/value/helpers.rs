use std::rc::Rc;

use crate::error::ExecutorError;
use crate::executor::Executor;

use super::{RcValue, Value, invoked_function::InvokedFunction, invoked_operator::InvokedOperator};

impl Value {
    pub fn check_truthy(&self) -> Result<bool, ExecutorError> {
        match self {
            Value::Integer(n) => Ok(*n != 0),
            Value::Float(f) => Ok(*f != 0.0),
            Value::Complex { re, im } => Ok(*re != 0.0 || *im != 0.0),
            _ => Err(ExecutorError::InvalidCondition(self.type_name())),
        }
    }

    // an element might contain lvalues if it is itself an lvalue or a nested list
    fn may_need_lvalue_elision(&self) -> bool {
        self.is_lvalue() || matches!(self, Value::List(_) | Value::Leader(_))
    }

    // creates owned copy of self which elides all lvalues, recursing on lists
    // once it encounters an lvalue, it elides that, and does not recurse any further
    pub fn elide_lvalue_leader_rec(self) -> Value {
        match self {
            Value::Lvalue(rc) => rc.borrow().clone().elide_lvalue_leader_rec(),
            Value::WeakLvalue(weak) => weak
                .upgrade()
                .map(|rc| rc.borrow().clone().elide_lvalue_leader_rec())
                .unwrap(),
            Value::Leader(leader) => leader.leader_rc.borrow().clone().elide_lvalue_leader_rec(),
            Value::List(mut list) => {
                if !list
                    .elements
                    .iter()
                    .any(|e| e.borrow().may_need_lvalue_elision())
                {
                    return Value::List(list);
                }

                let list_mut = Rc::make_mut(&mut list);
                for elem in &mut list_mut.elements {
                    if !elem.borrow().may_need_lvalue_elision() {
                        continue;
                    }
                    let elided = elem.borrow().clone().elide_lvalue_leader_rec();
                    // reuse the existing allocation when exclusively owned; COW otherwise
                    if Rc::strong_count(elem) == 1 {
                        *elem.borrow_mut() = elided;
                    } else {
                        *elem = super::rc_value(elided);
                    }
                }
                Value::List(list)
            }
            other => other,
        }
    }

    /// read through an lvalue or weak lvalue
    /// if not an lvalue, returns self.
    pub fn elide_lvalue(self) -> Value {
        match self {
            Value::Lvalue(rc) => rc.borrow().clone(),
            Value::WeakLvalue(weak) => weak.upgrade().map(|rc| rc.borrow().clone()).unwrap(),
            other => other,
        }
    }

    pub async fn elide_wrappers(self, executor: &mut Executor) -> Result<Value, ExecutorError> {
        let mut base = self.elide_lvalue();
        loop {
            base = match base {
                Value::Leader(leader) => leader.leader_rc.borrow().clone(),
                Value::InvokedOperator(op) => InvokedOperator::value(&op, executor).await?,
                Value::InvokedFunction(func) => InvokedFunction::value(&func, executor).await?,
                other => return Ok(other),
            };
        }
    }

    pub fn to_follower_stateful(&self) -> Value {
        match self {
            Value::Stateful(stateful) => Value::Stateful(stateful.to_follower_read()),
            other => other.clone(),
        }
    }

    pub fn elide_leader(self) -> Value {
        match self {
            Value::Leader(leader) => leader.leader_rc.borrow().clone(),
            other => other,
        }
    }

    pub fn force_elide_lvalue(&self) -> Value {
        match self {
            Value::Lvalue(rc) => rc.borrow().clone(),
            Value::WeakLvalue(weak) => weak.upgrade().map(|rc| rc.borrow().clone()).unwrap(),
            _ => panic!("Expected Lvalue, got {}", self.type_name()),
        }
    }

    /// try to get the underlying RcValue (upgrading weak refs).
    /// returns None if this isn't an lvalue variant
    pub fn as_lvalue_rc(&self) -> Option<RcValue> {
        match self {
            Value::Lvalue(rc) => Some(rc.clone()),
            Value::WeakLvalue(weak) => Some(weak.upgrade().unwrap()),
            _ => None,
        }
    }

    pub fn is_lvalue(&self) -> bool {
        matches!(self, Value::Lvalue(_) | Value::WeakLvalue(_))
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Nil => "nil",
            Value::Float(_) => "float",
            Value::Integer(_) => "int",
            Value::Complex { .. } => "complex",
            Value::String(_) => "string",
            Value::Mesh(_) => "mesh",
            Value::PrimitiveAnim(_) => "primitive_anim",
            Value::Lambda(_) => "lambda",
            Value::Operator(_) => "operator",
            Value::AnimBlock(_) => "anim_block",
            Value::Map(_) => "map",
            Value::List(_) => "list",
            Value::Stateful(_) => "stateful",
            Value::Leader(_) => "leader",
            Value::InvokedOperator(_) => "live operator",
            Value::InvokedFunction(_) => "live function",
            Value::Lvalue(_) => "lvalue",
            Value::WeakLvalue(_) => "lvalue",
        }
    }
}
