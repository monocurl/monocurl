use std::cell::Cell;

use crate::{
    error::ExecutorError,
    executor::Executor,
    heap::{HeapKey, VRc, with_heap},
};

use super::{
    Value, container::Map, invoked_function::InvokedFunction, invoked_operator::InvokedOperator,
    stateful::to_follower_stateful,
};

fn elided_heap_ref_value(value_ref: &VRc) -> VRc {
    let key = value_ref.key();
    let val = with_heap(|h| h.get(key).clone());
    let value = if val.may_need_lvalue_leader_elision() {
        val.elide_lvalue_leader_rec()
    } else {
        val
    };
    VRc::new(value)
}

fn clone_cached_value(cell: &Cell<Option<Box<Value>>>) -> Option<Value> {
    let cached = cell.take();
    let cloned = cached.as_ref().map(|value| (**value).clone());
    cell.set(cached);
    cloned
}

fn cached_elided_heap_ref_value(value_ref: &VRc) -> VRc {
    let key = value_ref.key();
    let value = with_heap(|h| h.get(key).clone()).elide_cached_wrappers_rec();
    VRc::new(value)
}

impl Value {
    #[inline(always)]
    pub fn check_truthy(&self) -> Result<bool, ExecutorError> {
        match self {
            Value::Integer(n) => Ok(*n != 0),
            Value::Float(f) => Ok(*f != 0.0),
            Value::Complex { re, im } => Ok(*re != 0.0 || *im != 0.0),
            _ => Err(ExecutorError::InvalidCondition(self.type_name())),
        }
    }

    #[inline(always)]
    fn may_need_lvalue_leader_elision(&self) -> bool {
        self.is_lvalue() || matches!(self, Value::List(_) | Value::Map(_) | Value::Leader(_))
    }

    /// creates owned copy of self which elides lvalues and leaders recursively
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
                list.elements = list.elements.iter().map(elided_heap_ref_value).collect();
                Value::List(list)
            }
            Value::Map(map) => {
                let mut out = Map::new();
                for key in &map.insertion_order {
                    let value_ref = map
                        .get(key)
                        .expect("map insertion order points to missing entry");
                    out.insert(key.clone(), elided_heap_ref_value(value_ref));
                }
                Value::Map(out)
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

    /// synchronously read through wrappers that already have a cached concrete value.
    pub fn elide_cached_wrappers_rec(self) -> Value {
        match self.elide_lvalue() {
            Value::Leader(leader) => {
                with_heap(|h| h.get(leader.leader_rc.key()).clone()).elide_cached_wrappers_rec()
            }
            Value::InvokedFunction(inv) => clone_cached_value(&inv.cache.0)
                .map(Value::elide_cached_wrappers_rec)
                .unwrap_or(Value::InvokedFunction(inv)),
            Value::InvokedOperator(inv) => clone_cached_value(&inv.cache.cached_result)
                .map(Value::elide_cached_wrappers_rec)
                .unwrap_or(Value::InvokedOperator(inv)),
            Value::List(mut list) => {
                list.elements = list
                    .elements
                    .iter()
                    .map(cached_elided_heap_ref_value)
                    .collect();
                Value::List(list)
            }
            Value::Map(map) => {
                let mut out = Map::new();
                for key in &map.insertion_order {
                    let value_ref = map
                        .get(key)
                        .expect("map insertion order points to missing entry");
                    out.insert(key.clone(), cached_elided_heap_ref_value(value_ref));
                }
                Value::Map(out)
            }
            other => other,
        }
    }

    pub async fn elide_wrappers_rec(self, executor: &mut Executor) -> Result<Value, ExecutorError> {
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

    #[inline(always)]
    pub fn elide_leader(self) -> Value {
        match self {
            Value::Leader(ref leader) => with_heap(|h| h.get(leader.leader_rc.key()).clone()),
            other => other,
        }
    }

    #[inline(always)]
    pub fn force_elide_lvalue(&self) -> Value {
        match self {
            Value::Lvalue(vrc) => with_heap(|h| h.get(vrc.key()).clone()),
            Value::WeakLvalue(vweak) => with_heap(|h| h.get(vweak.key()).clone()),
            _ => panic!("Expected Lvalue, got {}", self.type_name()),
        }
    }

    /// try to get the underlying HeapKey (upgrading weak refs).
    #[inline(always)]
    pub fn as_lvalue_key(&self) -> Option<HeapKey> {
        match self {
            Value::Lvalue(vrc) => Some(vrc.key()),
            Value::WeakLvalue(vweak) => Some(vweak.key()),
            _ => None,
        }
    }

    pub fn make_mut_lvalue(&mut self) -> HeapKey {
        match self {
            Value::Lvalue(vrc) => vrc.make_mut(),
            Value::WeakLvalue(vweak) => {
                let value = with_heap(|h| h.get(vweak.key()).clone());
                let vrc = VRc::new(value);
                let key = vrc.key();
                *self = Value::Lvalue(vrc);
                key
            }
            _ => {
                let value = std::mem::replace(self, Value::Nil);
                let vrc = VRc::new(value);
                let key = vrc.key();
                *self = Value::Lvalue(vrc);
                key
            }
        }
    }

    #[inline(always)]
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
