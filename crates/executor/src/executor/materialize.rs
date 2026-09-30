//! reading live wrappers down to concrete values: materialising cached
//! results, and warming the caches of wrappers that live in heap slots

use crate::{
    error::ExecutorError,
    heap::{HeapKey, VRc, heap_replace, with_heap},
    value::{
        Value,
        container::{List, Map},
        helpers::has_cached_value,
        invoked_function::InvokedFunction,
        invoked_operator::InvokedOperator,
    },
};

use super::Executor;

impl Executor {
    /// evaluate every live function or operator reachable from the slot at
    /// `key` whose cache is empty, and store the evaluated wrapper back so the
    /// result survives in the heap. mesh followers are read by every frame as
    /// fresh clones, and a clone of an unfilled wrapper recomputes from
    /// scratch; without this an idle frame costs as much as a changing one
    pub(crate) fn warm_live_wrappers_in_slot<'a>(
        &'a mut self,
        key: HeapKey,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), ExecutorError>> + 'a>> {
        Box::pin(async move {
            let value = with_heap(|h| h.get(key).clone());
            match value {
                Value::InvokedFunction(inv) => {
                    if !has_cached_value(&inv.cache.0) {
                        InvokedFunction::value(&inv, self).await?;
                        heap_replace(key, Value::InvokedFunction(inv));
                    }
                }
                Value::InvokedOperator(inv) => {
                    if !has_cached_value(&inv.cache.cached_result) {
                        InvokedOperator::value(&inv, self).await?;
                        heap_replace(key, Value::InvokedOperator(inv));
                    }
                }
                Value::List(list) => {
                    for element in list.elements() {
                        self.warm_live_wrappers_in_slot(element.key()).await?;
                    }
                }
                Value::Lvalue(reference) => {
                    self.warm_live_wrappers_in_slot(reference.key()).await?;
                }
                Value::WeakLvalue(reference) => {
                    self.warm_live_wrappers_in_slot(reference.key()).await?;
                }
                _ => {}
            }
            Ok(())
        })
    }

    /// a value that is already concrete comes back as it is, without boxing a
    /// future for it: most results of a live call are meshes and lists of
    /// meshes, and boxing per node made a deep mesh tree spend its frame in
    /// the allocator
    pub(crate) async fn materialize_cached_value(
        &mut self,
        val: Value,
    ) -> Result<Value, ExecutorError> {
        match materialize_sync(val) {
            Ok(value) => Ok(value),
            Err(val) => self.materialize_cached_value_slow(val).await,
        }
    }

    fn materialize_cached_value_slow<'a>(
        &'a mut self,
        val: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            match val {
                Value::Lvalue(vrc) => {
                    let inner = with_heap(|h| h.get(vrc.key()).clone());
                    self.materialize_cached_value_slow(inner).await
                }
                Value::WeakLvalue(vweak) => {
                    let inner = with_heap(|h| h.get(vweak.key()).clone());
                    self.materialize_cached_value_slow(inner).await
                }
                Value::Leader(leader) => {
                    let inner = with_heap(|h| h.get(leader.leader_rc.key()).clone());
                    self.materialize_cached_value_slow(inner).await
                }
                Value::InvokedFunction(inv) => {
                    let inner = InvokedFunction::value(&inv, self).await?;
                    self.materialize_cached_value_slow(inner).await
                }
                Value::InvokedOperator(inv) => {
                    let inner = InvokedOperator::value(&inv, self).await?;
                    self.materialize_cached_value_slow(inner).await
                }
                Value::Stateful(stateful) => {
                    let inner = self.eval_stateful(&stateful).await?;
                    self.materialize_cached_value_slow(inner).await
                }
                Value::List(list) => {
                    let mut elements = Vec::with_capacity(list.len());
                    for value_ref in list.elements() {
                        let value = with_heap(|h| h.get(value_ref.key()).clone());
                        elements.push(VRc::new(self.materialize_cached_value(value).await?));
                    }
                    Ok(Value::List(List::new_with(elements)))
                }
                Value::Map(map) => {
                    let mut out = Map::new();
                    for key in &map.insertion_order {
                        let value_ref = map
                            .get(key)
                            .expect("map insertion order points to missing entry");
                        let value = with_heap(|h| h.get(value_ref.key()).clone());
                        out.insert(
                            key.clone(),
                            VRc::new(self.materialize_cached_value(value).await?),
                        );
                    }
                    Ok(Value::Map(out))
                }
                other => Ok(other),
            }
        })
    }

    pub(super) async fn resolve_live_value(&mut self, val: Value) -> Result<Value, ExecutorError> {
        self.materialize_cached_value(val).await
    }
}

/// materialise without evaluating anything: heap references are read through
/// and a live wrapper whose cache is filled stands for its cached value, which
/// is stored fully materialised. `Err` hands back the value untouched when
/// something under it would have to run
fn materialize_sync(value: Value) -> Result<Value, Value> {
    if with_heap(|heap| is_materialized(heap, &value)) {
        return Ok(value);
    }
    match &value {
        Value::Lvalue(reference) => {
            materialize_sync(with_heap(|h| h.get(reference.key()).clone())).map_err(|_| value)
        }
        Value::WeakLvalue(reference) => {
            materialize_sync(with_heap(|h| h.get(reference.key()).clone())).map_err(|_| value)
        }
        Value::Leader(leader) => {
            materialize_sync(with_heap(|h| h.get(leader.leader_rc.key()).clone()))
                .map_err(|_| value)
        }
        Value::InvokedFunction(inv) => inv.cache.0.cloned().ok_or(value),
        Value::InvokedOperator(inv) => inv.cache.cached_result.cloned().ok_or(value),
        Value::List(list) => {
            let mut elements = Vec::with_capacity(list.len());
            for value_ref in list.elements() {
                let element = with_heap(|h| h.get(value_ref.key()).clone());
                match materialize_sync(element) {
                    Ok(element) => elements.push(VRc::new(element)),
                    Err(_) => return Err(value),
                }
            }
            Ok(Value::List(List::new_with(elements)))
        }
        _ => Err(value),
    }
}

/// whether nothing under `value` is a wrapper that materialising would read
/// through. runs under a heap borrow, so it neither allocates nor evaluates
fn is_materialized(heap: &crate::heap::VirtualHeap, value: &Value) -> bool {
    match value {
        Value::Lvalue(_)
        | Value::WeakLvalue(_)
        | Value::Leader(_)
        | Value::InvokedFunction(_)
        | Value::InvokedOperator(_)
        | Value::Stateful(_) => false,
        Value::List(list) => list
            .elements()
            .iter()
            .all(|element| is_materialized(heap, &heap.get(element.key()))),
        Value::Map(map) => map
            .insertion_order
            .iter()
            .filter_map(|key| map.get(key))
            .all(|element| is_materialized(heap, &heap.get(element.key()))),
        _ => true,
    }
}
