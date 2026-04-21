use crate::{
    error::ExecutorError,
    executor::Executor,
    heap::{HeapKey, with_heap},
};

use super::{
    Value, invoked_function::InvokedFunction, invoked_operator::InvokedOperator,
    stateful::to_follower_stateful,
};

impl Value {
    pub fn check_truthy(&self) -> Result<bool, ExecutorError> {
        match self {
            Value::Integer(n) => Ok(*n != 0),
            Value::Float(f) => Ok(*f != 0.0),
            Value::Complex { re, im } => Ok(*re != 0.0 || *im != 0.0),
            _ => Err(ExecutorError::InvalidCondition(self.type_name())),
        }
    }

    fn may_need_lvalue_elision(&self) -> bool {
        self.is_lvalue() || matches!(self, Value::List(_) | Value::Leader(_))
    }

    /// creates owned copy of self which elides all lvalues, recursing on lists
    pub fn elide_lvalue_leader_rec(self) -> Value {
        match self {
            Value::Lvalue(vrc) => with_heap(|h| h.get(vrc.key()).clone()).elide_lvalue_leader_rec(),
            Value::WeakLvalue(vweak) => {
                with_heap(|h| h.get(vweak.key()).clone()).elide_lvalue_leader_rec()
            }
            Value::Leader(ref leader) => {
                with_heap(|h| h.get(leader.leader_rc.key()).clone()).elide_lvalue_leader_rec()
            }
            Value::List(mut list) => {
                let needs_work = list
                    .elements
                    .iter()
                    .any(|key| with_heap(|h| h.get(key.key()).may_need_lvalue_elision()));
                if !needs_work {
                    return Value::List(list);
                }

                let list_mut = std::rc::Rc::make_mut(&mut list);
                for i in 0..list_mut.elements.len() {
                    let key = list_mut.elements[i].clone();
                    let val = with_heap(|h| h.get(key.key()).clone());
                    if !val.may_need_lvalue_elision() {
                        continue;
                    }
                    let elided = val.elide_lvalue_leader_rec();
                    if crate::heap::heap_ref_count(key.key()) == 1 {
                        crate::heap::heap_replace(key.key(), elided);
                    } else {
                        let new_key = crate::heap::VRc::new(elided);
                        list_mut.elements[i] = new_key;
                    }
                }
                Value::List(list)
            }
            other => other,
        }
    }

    /// read through an lvalue or weak lvalue
    pub fn elide_lvalue(self) -> Value {
        match self {
            Value::Lvalue(vrc) => with_heap(|h| h.get(vrc.key()).clone()),
            Value::WeakLvalue(vweak) => with_heap(|h| h.get(vweak.key()).clone()),
            other => other,
        }
    }

    pub async fn elide_wrappers(self, executor: &mut Executor) -> Result<Value, ExecutorError> {
        let mut base = self.elide_lvalue();
        loop {
            base = match base {
                Value::Leader(ref leader) => with_heap(|h| h.get(leader.leader_rc.key()).clone()),
                Value::InvokedOperator(ref op) => InvokedOperator::value(op, executor).await?,
                Value::InvokedFunction(ref func) => InvokedFunction::value(func, executor).await?,
                Value::Stateful(ref stateful) => executor.eval_stateful(stateful).await?,
                other => return Ok(other),
            };
        }
    }

    pub fn to_follower_stateful(&self) -> Value {
        match self {
            Value::Stateful(stateful) => Value::Stateful(to_follower_stateful(stateful)),
            other => other.clone(),
        }
    }

    pub fn elide_leader(self) -> Value {
        match self {
            Value::Leader(ref leader) => with_heap(|h| h.get(leader.leader_rc.key()).clone()),
            other => other,
        }
    }

    pub fn force_elide_lvalue(&self) -> Value {
        match self {
            Value::Lvalue(vrc) => with_heap(|h| h.get(vrc.key()).clone()),
            Value::WeakLvalue(vweak) => with_heap(|h| h.get(vweak.key()).clone()),
            _ => panic!("Expected Lvalue, got {}", self.type_name()),
        }
    }

    /// try to get the underlying HeapKey (upgrading weak refs).
    pub fn as_lvalue_key(&self) -> Option<HeapKey> {
        match self {
            Value::Lvalue(vrc) => Some(vrc.key()),
            Value::WeakLvalue(vweak) => Some(vweak.key()),
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
