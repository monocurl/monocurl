use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::{error::ExecutorError, heap::with_heap};

pub(super) const EXECUTOR_MEMORY_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(super) const EXECUTOR_HEAP_SLOT_LIMIT: usize = 1 << 20;
pub(super) const MEMORY_CHECK_PERIOD: u32 = 32_768;

pub(super) struct PeriodicMemoryChecker {
    count: u32,
    period: u32,
    pid: Option<Pid>,
    system: System,
    limit_bytes: u64,
    heap_slot_limit: usize,
}

impl PeriodicMemoryChecker {
    pub(super) fn new(limit_bytes: u64, heap_slot_limit: usize, period: u32) -> Self {
        let pid = sysinfo::get_current_pid().ok();
        let refresh_kind =
            RefreshKind::new().with_processes(ProcessRefreshKind::new().with_memory());

        Self {
            count: 0,
            period,
            pid,
            system: System::new_with_specifics(refresh_kind),
            limit_bytes,
            heap_slot_limit,
        }
    }

    pub(super) fn tick(&mut self) -> Result<(), ExecutorError> {
        self.count += 1;
        if self.count < self.period {
            return Ok(());
        }
        self.count = 0;

        let Some(pid) = self.pid else {
            return Ok(());
        };

        self.system.refresh_processes_specifics(
            ProcessesToUpdate::Some(&[pid]),
            ProcessRefreshKind::new().with_memory(),
        );

        let Some(process) = self.system.process(pid) else {
            return Ok(());
        };

        let used = process.memory();
        if used > self.limit_bytes {
            return Err(ExecutorError::MemoryLimitExceeded {
                used,
                limit: self.limit_bytes,
            });
        }

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
        let mut checker = PeriodicMemoryChecker::new(u64::MAX, used - 1, 1);
        assert!(checker.tick().is_err());
    }
}
