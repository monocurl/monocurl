use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};

use crate::error::ExecutorError;

pub(super) const EXECUTOR_MEMORY_LIMIT_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub(super) const MEMORY_CHECK_PERIOD: u32 = 8_192;

pub(super) struct PeriodicMemoryChecker {
    count: u32,
    period: u32,
    pid: Option<Pid>,
    system: System,
    limit_bytes: u64,
}

impl PeriodicMemoryChecker {
    pub(super) fn new(limit_bytes: u64, period: u32) -> Self {
        let pid = sysinfo::get_current_pid().ok();
        let refresh_kind =
            RefreshKind::new().with_processes(ProcessRefreshKind::new().with_memory());

        Self {
            count: 0,
            period,
            pid,
            system: System::new_with_specifics(refresh_kind),
            limit_bytes,
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

        Ok(())
    }
}
