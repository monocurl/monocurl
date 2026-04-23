use crate::{error::ExecutorError, heap::with_heap};

pub(super) const EXECUTOR_HEAP_SLOT_LIMIT: usize = 1 << 20;
pub(super) const MEMORY_CHECK_PERIOD: u32 = 32_768;

pub(super) struct PeriodicMemoryChecker {
    count: u32,
    period: u32,
    heap_slot_limit: usize,
}

impl PeriodicMemoryChecker {
    pub(super) fn new(heap_slot_limit: usize, period: u32) -> Self {
        Self {
            count: 0,
            period,
            heap_slot_limit,
        }
    }

    pub(super) fn tick(&mut self) -> Result<(), ExecutorError> {
        self.count += 1;
        if self.count < self.period {
            return Ok(());
        }
        self.count = 0;

        let used = with_heap(|heap| heap.slot_count());
        if used > self.heap_slot_limit {
            return Err(ExecutorError::VirtualHeapLimitExceeded {
                used,
                limit: self.heap_slot_limit,
            });
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::PeriodicMemoryChecker;
    use crate::{
        heap::{VRc, with_heap},
        value::Value,
    };

    #[test]
    fn heap_slot_limit_is_enforced() {
        let baseline = with_heap(|heap| heap.slot_count());
        let mut keep_alive = Vec::new();
        while with_heap(|heap| heap.slot_count()) == baseline {
            keep_alive.push(VRc::new(Value::Integer(1)));
        }

        let used = with_heap(|heap| heap.slot_count());
        let mut checker = PeriodicMemoryChecker::new(used - 1, 1);
        assert!(checker.tick().is_err());
    }
}
