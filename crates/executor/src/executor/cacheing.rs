use std::sync::Arc;

use bytecode::{Bytecode, SectionBytecode};

use crate::{executor::Executor, state::ExecutionState, time::Timestamp};

struct CacheEntry {
    state_after: ExecutionState,
}

impl CacheEntry {
    pub fn slide_duration(&self) -> f64 {
        self.state_after.timestamp.slide as f64 + 3.0
        // self.state_after.timestamp.time
    }
}

pub(crate) struct ExecutionCache {
    // entries[i] = state right after finishing the ith section in bytecode
    entries: Vec<Option<CacheEntry>>,
}

impl ExecutionCache {
    pub fn new(bytecode: &Bytecode) -> Self {
        let mut entries = Vec::new();
        entries.resize_with(bytecode.sections.len(), || None);

        Self {
            entries
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
            if i >= bytecode.sections.len() || !section_eq(&self.bytecode.sections[i], &bytecode.sections[i]) {
                first_invalid = Some(i);
                break;
            }
        }

        self.bytecode = bytecode;
        self.cache.entries.resize_with(self.bytecode.sections.len(), || None);
        if let Some(i) = first_invalid {
            self.cache.entries[i..]
                .iter_mut()
                .for_each(|entry| *entry = None);

            // possibly go backwards to latest valid state
            if self.state.timestamp.slide >= i {
                let latest = self.cache.entries
                    .iter()
                    .rposition(|en| en.is_some());

                match latest {
                    Some(j) => self.state = self.cache.entries[j].as_ref().unwrap().state_after.clone(),
                    None => self.state = ExecutionState::new(),
                };
            }
        }
    }

    // given a target, find the first cache point that we can base off of
    pub(crate) async fn rebase_at_cache_point(&mut self, target: Timestamp) {
        let valid_state = !self.state.has_errors();
        let in_future = target > self.state.timestamp;

        if valid_state && !in_future {
            // just start from here
            return;
        }
        else {
            let latest = self.cache.entries
                .iter()
                .rfind(|en| en.is_some() && en.as_ref().unwrap().state_after.timestamp <= target);

            if let Some(en) = latest {
                self.state = en.as_ref().unwrap().state_after.clone();
            }
            else {
                self.state = ExecutionState::new();
            }
        }
    }

    // called right before advance to next section
    pub(crate) fn save_cache(&mut self) {
        assert!(!self.state.has_errors());
        self.cache.entries[self.state.timestamp.slide] = Some(CacheEntry {
            state_after: self.state.clone(),
        });
    }

    pub fn real_slide_count(&self) -> usize {
        self.bytecode.sections.len() - self.bytecode.non_slide_sections()
    }

    pub fn real_slide_durations(&self) -> Vec<Option<f64>> {
        self.cache.entries.iter()
            .skip(self.bytecode.non_slide_sections())
            .map(|e| e.as_ref().map(|en| en.slide_duration()))
            .collect()
    }
}
