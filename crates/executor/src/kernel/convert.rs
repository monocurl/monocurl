//! moving values across the boundary between the heap-backed interpreter and
//! the kernel tier

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::{
    heap::{VRc, with_heap},
    value::{Value, container::List, lambda::Lambda},
};

use super::{
    KernelTier,
    value::{ClosureArena, ClosureId, KClosure, KVal},
};

/// one conversion session: values become `KVal`s and every lambda reached on
/// the way lands in the arena once, however many times it is captured
pub(crate) struct Converter<'a> {
    tier: &'a mut KernelTier,
    arena: &'a mut ClosureArena,
    /// lambdas already placed, by identity. the pointers stay valid because the
    /// values being converted keep their lambdas alive for the whole session
    placed: FxHashMap<*const Lambda, ClosureId>,
}

impl<'a> Converter<'a> {
    pub(crate) fn new(tier: &'a mut KernelTier, arena: &'a mut ClosureArena) -> Self {
        Self {
            tier,
            arena,
            placed: FxHashMap::default(),
        }
    }

    pub(crate) fn arena(&self) -> &ClosureArena {
        self.arena
    }

    /// place a lambda in the arena, compiling its body on first sight. `None`
    /// when the body stays in the interpreter
    pub(crate) fn place(&mut self, lambda: &Lambda) -> Option<ClosureId> {
        let identity = lambda as *const Lambda;
        if let Some(id) = self.placed.get(&identity) {
            return Some(*id);
        }
        let kernel = self.tier.kernel_for(lambda)?;
        let captures = lambda
            .captures
            .iter()
            .map(|capture| self.convert(capture))
            .collect();
        let defaults = lambda
            .defaults
            .iter()
            .map(|default| self.convert(default))
            .collect();
        let id = self.arena.push(KClosure {
            ip: lambda.ip,
            kernel,
            captures,
            defaults,
        });
        self.placed.insert(identity, id);
        Some(id)
    }

    /// the tier's view of a value. shapes it does not model become opaque
    /// rather than failing, so a capture the body never touches costs nothing
    pub(crate) fn convert(&mut self, value: &Value) -> KVal {
        match value {
            Value::Nil => KVal::Nil,
            Value::Integer(n) => KVal::Int(*n),
            Value::Float(f) => KVal::Float(*f),
            // elements are cloned out before converting them: converting a
            // live wrapper materialises it, which allocates on the heap, and
            // the heap cannot be borrowed while that happens
            Value::List(list) => {
                let elements: Vec<Value> = with_heap(|heap| {
                    list.elements()
                        .iter()
                        .map(|key| heap.get(key.key()).clone())
                        .collect()
                });
                KVal::List(Arc::new(
                    elements
                        .iter()
                        .map(|element| self.convert(element))
                        .collect(),
                ))
            }
            Value::Lvalue(reference) => {
                let inner = with_heap(|heap| heap.get(reference.key()).clone());
                self.convert(&inner)
            }
            Value::WeakLvalue(reference) => {
                let inner = with_heap(|heap| heap.get(reference.key()).clone());
                self.convert(&inner)
            }
            Value::Lambda(lambda) => self.place(lambda).map_or(KVal::Opaque, KVal::Closure),
            Value::InvokedFunction(_) | Value::InvokedOperator(_) => {
                match value.clone().elide_cached_wrappers_rec() {
                    Value::InvokedFunction(_) | Value::InvokedOperator(_) => KVal::Opaque,
                    concrete => self.convert(&concrete),
                }
            }
            _ => KVal::Opaque,
        }
    }
}

/// bring a kernel result back onto the heap. closures and opaque values have
/// no faithful interpreter form here, so a result holding one is declined and
/// the call is re-run by the interpreter
pub fn to_value(value: &KVal) -> Option<Value> {
    Some(match value {
        KVal::Nil => Value::Nil,
        KVal::Int(n) => Value::Integer(*n),
        KVal::Float(f) => Value::Float(*f),
        KVal::List(list) => {
            let elements = list
                .iter()
                .map(|element| to_value(element).map(VRc::new))
                .collect::<Option<Vec<_>>>()?;
            Value::List(List::new_with(elements))
        }
        KVal::Closure(_) | KVal::Opaque => return None,
    })
}
