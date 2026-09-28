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

    pub(crate) fn materialize_cached_value<'a>(
        &'a mut self,
        val: Value,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, ExecutorError>> + 'a>>
    {
        Box::pin(async move {
            match val {
                Value::Lvalue(vrc) => {
                    let inner = with_heap(|h| h.get(vrc.key()).clone());
                    self.materialize_cached_value(inner).await
                }
                Value::WeakLvalue(vweak) => {
                    let inner = with_heap(|h| h.get(vweak.key()).clone());
                    self.materialize_cached_value(inner).await
                }
                Value::Leader(leader) => {
                    let inner = with_heap(|h| h.get(leader.leader_rc.key()).clone());
                    self.materialize_cached_value(inner).await
                }
                Value::InvokedFunction(inv) => {
                    let inner = InvokedFunction::value(&inv, self).await?;
                    self.materialize_cached_value(inner).await
                }
                Value::InvokedOperator(inv) => {
                    let inner = InvokedOperator::value(&inv, self).await?;
                    self.materialize_cached_value(inner).await
                }
                Value::Stateful(stateful) => {
                    let inner = self.eval_stateful(&stateful).await?;
                    self.materialize_cached_value(inner).await
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
