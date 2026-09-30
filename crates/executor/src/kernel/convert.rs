//! moving values across the boundary between the heap-backed interpreter and
//! the kernel tier

use std::sync::Arc;

use rustc_hash::FxHashMap;

use crate::{
    heap::{VRc, with_heap},
    value::{
        Value,
        container::{HashableKey, List, Map},
        lambda::Lambda,
    },
};

use super::{
    KernelTier,
    value::{ClosureArena, ClosureId, KClosure, KList, KPalette, KVal},
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
                // lists of numbers (points, colors) convert in place
                let scalars: Option<KList> = with_heap(|heap| {
                    list.elements()
                        .iter()
                        .map(|key| match &*heap.get(key.key()) {
                            Value::Nil => Some(KVal::Nil),
                            Value::Integer(n) => Some(KVal::Int(*n)),
                            Value::Float(f) => Some(KVal::Float(*f)),
                            _ => None,
                        })
                        .collect()
                });
                if let Some(scalars) = scalars {
                    return KVal::List(Arc::new(scalars));
                }
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
            Value::Map(map) => palette(map).map_or(KVal::Opaque, |palette| {
                KVal::Palette(self.arena.push_palette(palette))
            }),
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

/// a map `keyframe_lerp` can read without the interpreter: numeric times and
/// values that are numbers or lists of them, sorted the way the stdlib sorts
fn palette(map: &Map) -> Option<KPalette> {
    if map.is_empty() {
        return None;
    }
    let mut keys = map
        .iter()
        .map(|(time, value)| {
            let time = match time {
                HashableKey::Integer(n) => *n as f64,
                HashableKey::Float(bits) => HashableKey::float_value(*bits),
                HashableKey::String(_) | HashableKey::List(_) => return None,
            };
            // named colors and `hex(...)` results reach the map as references
            // and cached calls; read through them outside the heap borrow
            let value = with_heap(|heap| heap.get(value.key()).clone()).elide_cached_wrappers_rec();
            Some((time, numeric(&value)?))
        })
        .collect::<Option<Vec<_>>>()?;
    keys.sort_by(|(a, _), (b, _)| a.total_cmp(b));
    Some(KPalette {
        keys: keys.into_boxed_slice(),
    })
}

fn numeric(value: &Value) -> Option<KVal> {
    Some(match value {
        Value::Integer(n) => KVal::Int(*n),
        Value::Float(f) => KVal::Float(*f),
        Value::List(list) => {
            let elements: Vec<Value> = with_heap(|heap| {
                list.elements()
                    .iter()
                    .map(|key| heap.get(key.key()).clone())
                    .collect()
            });
            KVal::List(Arc::new(
                elements
                    .into_iter()
                    .map(|element| numeric(&element.elide_cached_wrappers_rec()))
                    .collect::<Option<KList>>()?,
            ))
        }
        _ => return None,
    })
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
        KVal::Closure(_) | KVal::Palette(_) | KVal::Opaque => return None,
    })
}
