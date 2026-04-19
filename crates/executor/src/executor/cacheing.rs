use std::sync::Arc;

use bytecode::{Bytecode, SectionBytecode};

use crate::{executor::Executor, state::ExecutionState, time::Timestamp};

struct CacheEntry {
    state_after: ExecutionState,
}

impl CacheEntry {
    pub fn slide_duration(&self) -> f64 {
        self.state_after.timestamp.time
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
            *minimum = Some(minimum.unwrap_or(0.0).max(timestamp.time));
        }
    }
}

impl Executor {
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
                        self.state = self.cache.entries[j].as_ref().unwrap().state_after.clone()
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
    pub(crate) async fn rebase_at_cache_point(&mut self, target: Timestamp) {
        let valid_state = !self.state.has_errors();
        let in_future = target >= self.state.timestamp;

        if valid_state && in_future {
            // just start from here
            self.state.pending_playback_time = 0.0;
            return;
        } else {
            let latest =
                self.cache.entries.iter().rfind(|en| {
                    en.is_some() && en.as_ref().unwrap().state_after.timestamp <= target
                });

            if let Some(en) = latest
                && false
            {
                self.state = en.as_ref().unwrap().state_after.clone();
            } else {
                self.state = ExecutionState::new();
            }
        }

        self.state.pending_playback_time = 0.0;
    }

    // called right before advance to next section
    pub(crate) fn save_cache(&mut self) {
        assert!(!self.state.has_errors());
        self.cache.note_timestamp(self.state.timestamp);
        self.cache.entries[self.state.timestamp.slide] = Some(CacheEntry {
            state_after: self.state.clone(),
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
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytecode::{Bytecode, SectionBytecode, SectionFlags};

    use super::{CacheEntry, ExecutionCache};
    use crate::{executor::Executor, state::ExecutionState, time::Timestamp};

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
            state_after: cached_state,
        });

        executor.update_bytecode(changed);

        assert_eq!(
            executor.cache.minimum_durations,
            vec![Some(0.5), Some(1.0), None]
        );
        assert!(executor.cache.entries[2].is_none());
    }
}
