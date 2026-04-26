use std::sync::Arc;

use bytecode::{Bytecode, SectionBytecode};

use crate::{
    executor::Executor,
    heap::{RawHeapSnapshot, VirtualHeap, restore_heap, snapshot_heap, with_inhibit},
    state::ExecutionState,
    time::Timestamp,
};

struct CacheEntry {
    state_after: RawHeapSnapshot<ExecutionState>,
    heap_snap: RawHeapSnapshot<VirtualHeap>,
}

impl CacheEntry {
    pub fn slide_duration(&self) -> f64 {
        self.state_after.as_ref().timestamp.time.max(0.0)
    }
}

pub(crate) struct ExecutionCache {
    // entries[i] = state right after finishing the ith section in bytecode
    entries: Vec<Option<CacheEntry>>,
    minimum_durations: Vec<Option<f64>>,
}

impl ExecutionCache {
    pub fn new(bytecode: &Bytecode) -> Self {
        let mut entries = Vec::new();
        entries.resize_with(bytecode.sections.len(), || None);
        let mut minimum_durations = Vec::new();
        minimum_durations.resize_with(bytecode.sections.len(), || None);

        Self {
            entries,
            minimum_durations,
        }
    }

    fn note_timestamp(&mut self, timestamp: Timestamp) {
        if let Some(minimum) = self.minimum_durations.get_mut(timestamp.slide) {
            *minimum = Some(minimum.unwrap_or(0.0).max(timestamp.time).max(0.0));
        }
    }
}

impl Executor {
    fn restore_latest_cache_before_or_reset(&mut self, target: Timestamp) {
        let latest = self
            .cache
            .entries
            .iter()
            .enumerate()
            .rev()
            .find_map(|(_, entry)| {
                let entry = entry.as_ref()?;
                (entry.state_after.as_ref().timestamp <= target)
                    .then(|| (entry.state_after.clone(), entry.heap_snap.clone()))
            });

        if let Some((state_after, heap_snap)) = latest {
            self.restore_cached_state(&state_after, &heap_snap);
        } else {
            self.state = ExecutionState::new();
        }

        self.state.pending_playback_time = 0.0;
    }

    fn restore_cached_state(
        &mut self,
        state_after: &RawHeapSnapshot<ExecutionState>,
        heap_snap: &RawHeapSnapshot<VirtualHeap>,
    ) {
        with_inhibit(|| {
            let new_state = state_after.raw_clone();
            restore_heap(heap_snap);
            let old_state = std::mem::replace(&mut self.state, new_state);
            drop(old_state);
        });
    }

    pub fn update_bytecode(&mut self, bytecode: Bytecode) {
        // move to latest position that we can

        let mut first_invalid = None;
        for i in 0..self.cache.entries.len() {
            fn section_eq(a: &Arc<SectionBytecode>, b: &Arc<SectionBytecode>) -> bool {
                Arc::ptr_eq(a, b) || *a == *b
            }
            if i >= bytecode.sections.len()
                || !section_eq(&self.bytecode.sections[i], &bytecode.sections[i])
            {
                first_invalid = Some(i);
                break;
            }
        }

        self.bytecode = bytecode;
        self.cache
            .entries
            .resize_with(self.bytecode.sections.len(), || None);
        self.cache
            .minimum_durations
            .resize_with(self.bytecode.sections.len(), || None);
        if let Some(i) = first_invalid {
            self.cache.entries[i..]
                .iter_mut()
                .for_each(|entry| *entry = None);
            self.cache.minimum_durations[i..]
                .iter_mut()
                .for_each(|entry| *entry = None);

            // possibly go backwards to latest valid state
            if self.state.timestamp.slide >= i {
                let latest = self.cache.entries.iter().rposition(|en| en.is_some());

                match latest {
                    Some(j) => {
                        let entry = self.cache.entries[j].as_ref().unwrap();
                        let state_after = entry.state_after.clone();
                        let heap_snap = entry.heap_snap.clone();
                        self.restore_cached_state(&state_after, &heap_snap);
                    }
                    None => self.state = ExecutionState::new(),
                };
            }
        }
    }

    /// clear all cached state and reset executor to the initial position
    pub fn clear_cache(&mut self) {
        self.cache.entries.iter_mut().for_each(|e| *e = None);
        self.cache
            .minimum_durations
            .iter_mut()
            .for_each(|e| *e = None);
        self.state = ExecutionState::new();
    }

    // given a target, find the first cache point that we can base off of
    pub fn restore_live_state_to_cache_point(&mut self, target: Timestamp) {
        self.restore_latest_cache_before_or_reset(target);
    }

    pub(crate) fn rebase_at_cache_point(&mut self, target: Timestamp) {
        self.restore_live_state_to_cache_point(target);
    }

    // called right before advance to next section
    pub(crate) fn save_cache(&mut self) {
        assert!(!self.state.has_errors());
        self.cache.note_timestamp(self.state.timestamp);
        let heap_snap = snapshot_heap();
        let state_after = RawHeapSnapshot::new(&self.state);
        self.cache.entries[self.state.timestamp.slide] = Some(CacheEntry {
            state_after,
            heap_snap,
        });
    }

    pub(crate) fn note_current_timestamp_in_cache(&mut self) {
        self.cache.note_timestamp(self.state.timestamp);
    }

    pub fn real_slide_count(&self) -> usize {
        self.bytecode.sections.len() - self.bytecode.non_slide_sections()
    }

    pub fn real_slide_durations(&self) -> Vec<Option<f64>> {
        self.cache
            .entries
            .iter()
            .skip(self.bytecode.non_slide_sections())
            .map(|e| e.as_ref().map(|en| en.slide_duration()))
            .collect()
    }

    pub fn real_minimum_slide_durations(&self) -> Vec<Option<f64>> {
        self.cache
            .minimum_durations
            .iter()
            .skip(self.bytecode.non_slide_sections())
            .copied()
            .collect()
    }

    pub fn real_slide_names(&self) -> Vec<Option<String>> {
        self.bytecode
            .sections
            .iter()
            .skip(self.bytecode.non_slide_sections())
            .map(|section| section.name.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytecode::{Bytecode, SectionBytecode, SectionFlags};

    use super::{CacheEntry, ExecutionCache};
    use crate::{
        executor::Executor,
        heap::{RawHeapSnapshot, VRc, heap_replace, snapshot_heap, with_heap},
        state::{ExecutionState, LeaderKind},
        time::Timestamp,
        value::{Value, container::List},
    };
    use smallvec::smallvec;

    fn bytecode_with_sections(flags: &[SectionFlags]) -> Bytecode {
        Bytecode::new(
            flags
                .iter()
                .cloned()
                .map(SectionBytecode::new)
                .map(Arc::new)
                .collect(),
        )
    }

    #[test]
    fn minimum_durations_track_max_timestamp_per_slide() {
        let bytecode = bytecode_with_sections(&[
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: true,
                is_root_module: true,
            },
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: false,
                is_root_module: true,
            },
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: false,
                is_root_module: true,
            },
        ]);
        let mut cache = ExecutionCache::new(&bytecode);

        cache.note_timestamp(Timestamp::new(1, 2.5));
        cache.note_timestamp(Timestamp::new(1, 1.0));
        cache.note_timestamp(Timestamp::new(2, 3.0));

        assert_eq!(cache.minimum_durations, vec![None, Some(2.5), Some(3.0)]);
    }

    #[test]
    fn update_bytecode_clears_minimum_durations_from_first_invalid_section() {
        let original = bytecode_with_sections(&[
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: true,
                is_root_module: true,
            },
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: false,
                is_root_module: true,
            },
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: false,
                is_root_module: true,
            },
        ]);
        let mut changed_sections = original.sections.clone();
        let mut changed_slide = (*changed_sections[2]).clone();
        changed_slide.string_pool.push("changed".into());
        changed_sections[2] = Arc::new(changed_slide);
        let changed = Bytecode::new(changed_sections);

        let mut executor = Executor::new(original, Vec::new());
        executor.cache.minimum_durations = vec![Some(0.5), Some(1.0), Some(2.0)];

        let mut cached_state = ExecutionState::new();
        cached_state.timestamp = Timestamp::new(1, 1.0);
        executor.cache.entries[1] = Some(CacheEntry {
            state_after: RawHeapSnapshot::new(&cached_state),
            heap_snap: snapshot_heap(),
        });

        executor.update_bytecode(changed);

        assert_eq!(
            executor.cache.minimum_durations,
            vec![Some(0.5), Some(1.0), None]
        );
        assert!(executor.cache.entries[2].is_none());
    }

    #[test]
    fn rebase_restores_cached_plain_list_without_aliasing() {
        let bytecode = bytecode_with_sections(&[
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: true,
                is_root_module: true,
            },
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: false,
                is_root_module: true,
            },
        ]);

        let mut executor = Executor::new(bytecode, Vec::new());
        executor.state.timestamp = Timestamp::new(1, 0.0);
        executor
            .state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .push(Value::List(List {
                elements: smallvec![VRc::new(Value::Integer(1)), VRc::new(Value::Integer(2))],
            }));
        executor
            .state
            .promote_to_var(ExecutionState::ROOT_STACK_IDX);
        executor.save_cache();

        let list_key = executor
            .state
            .stack(ExecutionState::ROOT_STACK_IDX)
            .peek()
            .as_lvalue_key()
            .unwrap();
        heap_replace(
            list_key,
            Value::List(List {
                elements: smallvec![VRc::new(Value::Integer(99)), VRc::new(Value::Integer(2))],
            }),
        );
        executor.state.timestamp = Timestamp::new(1, 1.0);

        executor.rebase_at_cache_point(Timestamp::new(1, 0.0));

        let restored = with_heap(|h| h.get(list_key).clone());
        let Value::List(restored) = restored else {
            panic!("expected list after restore");
        };
        match with_heap(|h| h.get(restored.elements[0].key()).clone()).elide_lvalue() {
            Value::Integer(1) => {}
            other => panic!(
                "expected restored first element to be 1, got {}",
                other.type_name()
            ),
        }
    }

    #[test]
    fn rebase_restores_cached_leader_inner_values_without_aliasing() {
        let bytecode = bytecode_with_sections(&[
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: true,
                is_root_module: true,
            },
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: false,
                is_root_module: true,
            },
        ]);

        let mut executor = Executor::new(bytecode, Vec::new());
        executor.state.timestamp = Timestamp::new(1, 0.0);
        executor
            .state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .push(Value::Integer(10));
        executor.state.promote_to_leader(
            ExecutionState::ROOT_STACK_IDX,
            LeaderKind::Param,
            "speed".into(),
        );
        executor.save_cache();

        let cell_key = executor
            .state
            .stack(ExecutionState::ROOT_STACK_IDX)
            .peek()
            .as_lvalue_key()
            .unwrap();
        let (leader_key, follower_key) = match with_heap(|h| h.get(cell_key).clone()) {
            Value::Leader(leader) => (leader.leader_rc.key(), leader.follower_rc.key()),
            other => panic!("expected leader, got {}", other.type_name()),
        };
        heap_replace(leader_key, Value::Integer(55));
        heap_replace(follower_key, Value::Integer(55));
        executor.state.timestamp = Timestamp::new(1, 1.0);

        executor.rebase_at_cache_point(Timestamp::new(1, 0.0));

        let restored = match with_heap(|h| h.get(cell_key).clone()) {
            Value::Leader(leader) => leader,
            other => panic!("expected leader after restore, got {}", other.type_name()),
        };
        match with_heap(|h| h.get(restored.leader_rc.key()).clone()).elide_lvalue() {
            Value::Integer(10) => {}
            other => panic!(
                "expected restored leader to be 10, got {}",
                other.type_name()
            ),
        }
        match with_heap(|h| h.get(restored.follower_rc.key()).clone()).elide_lvalue() {
            Value::Integer(10) => {}
            other => panic!(
                "expected restored follower to be 10, got {}",
                other.type_name()
            ),
        }
    }

    #[test]
    fn rebase_restores_cached_nested_lists_without_aliasing() {
        let bytecode = bytecode_with_sections(&[
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: true,
                is_root_module: true,
            },
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: false,
                is_root_module: true,
            },
        ]);

        let mut executor = Executor::new(bytecode, Vec::new());
        executor.state.timestamp = Timestamp::new(1, 0.0);
        executor
            .state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .push(Value::List(List {
                elements: smallvec![
                    VRc::new(Value::List(List {
                        elements: smallvec![VRc::new(Value::Integer(3))],
                    })),
                    VRc::new(Value::Integer(4)),
                ],
            }));
        executor
            .state
            .promote_to_var(ExecutionState::ROOT_STACK_IDX);
        executor.save_cache();

        let live_key = executor
            .state
            .stack(ExecutionState::ROOT_STACK_IDX)
            .peek()
            .as_lvalue_key()
            .unwrap();
        heap_replace(
            live_key,
            Value::List(List {
                elements: smallvec![
                    VRc::new(Value::List(List {
                        elements: smallvec![VRc::new(Value::Integer(30))],
                    })),
                    VRc::new(Value::Integer(4)),
                ],
            }),
        );
        executor.state.timestamp = Timestamp::new(1, 1.0);

        executor.rebase_at_cache_point(Timestamp::new(1, 0.0));

        let restored = with_heap(|h| h.get(live_key).clone());
        let Value::List(restored) = restored else {
            panic!("expected restored outer list");
        };
        let nested = with_heap(|h| h.get(restored.elements[0].key()).clone());
        let Value::List(nested) = nested else {
            panic!("expected restored nested list");
        };
        match with_heap(|h| h.get(nested.elements[0].key()).clone()).elide_lvalue() {
            Value::Integer(3) => {}
            other => panic!(
                "expected restored nested element to remain 3, got {}",
                other.type_name()
            ),
        }
    }

    #[test]
    fn restore_live_state_to_cache_point_discards_transient_live_state_only() {
        let bytecode = bytecode_with_sections(&[
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: true,
                is_root_module: true,
            },
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: false,
                is_root_module: true,
            },
        ]);

        let mut executor = Executor::new(bytecode, Vec::new());
        executor.state.timestamp = Timestamp::new(1, 0.0);
        executor
            .state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .push(Value::Integer(5));
        executor.save_cache();

        let child = executor
            .state
            .alloc_stack((0, 0), Some(ExecutionState::ROOT_STACK_IDX), None)
            .expect("child stack");
        executor.state.execution_heads.insert(child);
        executor.state.stack_mut(child).push(Value::Integer(9));
        executor
            .state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .pop();
        executor
            .state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .push(Value::Integer(11));

        executor.restore_live_state_to_cache_point(Timestamp::new(1, 0.5));

        assert!(executor.cache.entries[1].is_some());
        assert_eq!(executor.state.alive_stack_count, 1);
        assert_eq!(
            executor
                .state
                .execution_heads
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![ExecutionState::ROOT_STACK_IDX]
        );
        assert!(matches!(
            executor.state.stack(ExecutionState::ROOT_STACK_IDX).peek(),
            Value::Integer(5)
        ));
    }

    #[test]
    fn rebase_at_cache_point_discards_transient_live_state_even_for_future_target() {
        let bytecode = bytecode_with_sections(&[
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: true,
                is_root_module: true,
            },
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: false,
                is_root_module: true,
            },
        ]);

        let mut executor = Executor::new(bytecode, Vec::new());
        executor.state.timestamp = Timestamp::new(1, 0.0);
        executor
            .state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .push(Value::Integer(5));
        executor.save_cache();

        let child = executor
            .state
            .alloc_stack((0, 0), Some(ExecutionState::ROOT_STACK_IDX), None)
            .expect("child stack");
        executor.state.execution_heads.insert(child);
        executor.state.stack_mut(child).push(Value::Integer(9));
        executor
            .state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .pop();
        executor
            .state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .push(Value::Integer(11));
        executor.state.timestamp = Timestamp::new(1, 0.5);

        executor.rebase_at_cache_point(Timestamp::new(1, 0.75));

        assert_eq!(executor.state.alive_stack_count, 1);
        assert_eq!(
            executor
                .state
                .execution_heads
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![ExecutionState::ROOT_STACK_IDX]
        );
        assert!(matches!(
            executor.state.stack(ExecutionState::ROOT_STACK_IDX).peek(),
            Value::Integer(5)
        ));
    }
}
