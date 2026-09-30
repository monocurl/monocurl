//! a small persistent worker pool for kernel batches. spawning threads per
//! batch cost about a tenth of a per-pixel frame, and every frame runs a few
//! batches, so the workers stay parked between them

use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Mutex, OnceLock,
        mpsc::{self, Receiver, Sender},
    },
};

type Job = Box<dyn FnOnce() + Send>;

/// every worker owns its channel: a run hands chunk `k` to worker `k - 1`
/// directly. sharing one receiver behind a mutex made the workers take jobs
/// one at a time, each pickup a mutex handoff and a kernel wake-up, which
/// cost a per-pixel frame more than the kernels themselves
pub struct Pool {
    senders: Vec<Mutex<Sender<Job>>>,
}

/// the process-wide pool, sized like `worker_threads` minus the calling
/// thread, which takes a share of every batch itself
pub fn pool(threads: usize) -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let workers = threads.saturating_sub(1).max(1);
        let senders = (0..workers)
            .map(|index| {
                let (sender, receiver) = mpsc::channel::<Job>();
                std::thread::Builder::new()
                    .name(format!("monocurl-kernel-{index}"))
                    .spawn(move || worker(receiver))
                    .expect("kernel worker thread should spawn");
                Mutex::new(sender)
            })
            .collect();
        Pool { senders }
    })
}

fn worker(receiver: Receiver<Job>) {
    while let Ok(job) = receiver.recv() {
        // a panicking job must not take the worker with it; the caller notices
        // the missing result
        let _ = catch_unwind(AssertUnwindSafe(job));
    }
}

impl Pool {
    /// the number of chunks a batch should be split into so that every worker
    /// and the calling thread get one
    pub fn chunk_count(&self) -> usize {
        self.senders.len() + 1
    }

    /// run `job(0..count)` across the workers and the calling thread, which
    /// takes chunk 0, and return the results in chunk order. `None` for a
    /// chunk whose worker did not report back
    pub fn run<R: Send + 'static>(
        &self,
        count: usize,
        job: impl Fn(usize) -> R + Send + Sync + 'static,
    ) -> Vec<Option<R>> {
        let job = Arc::new(job);
        let (results_tx, results_rx) = mpsc::channel::<(usize, R)>();
        for index in 1..count {
            let job = Arc::clone(&job);
            let results_tx = results_tx.clone();
            let sender = self.senders[(index - 1) % self.senders.len()]
                .lock()
                .expect("kernel pool sender poisoned");
            let _ = sender.send(Box::new(move || {
                let _ = results_tx.send((index, job(index)));
            }));
        }
        drop(results_tx);

        let mut results: Vec<Option<R>> = (0..count).map(|_| None).collect();
        if count > 0 {
            results[0] = Some(job(0));
        }
        for (index, result) in results_rx {
            results[index] = Some(result);
        }
        results
    }
}
