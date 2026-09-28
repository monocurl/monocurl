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

pub struct Pool {
    sender: Mutex<Sender<Job>>,
    workers: usize,
}

/// the process-wide pool, sized like `worker_threads` minus the calling
/// thread, which takes a share of every batch itself
pub fn pool(threads: usize) -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    POOL.get_or_init(|| {
        let workers = threads.saturating_sub(1).max(1);
        let (sender, receiver) = mpsc::channel::<Job>();
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..workers {
            let receiver = Arc::clone(&receiver);
            std::thread::Builder::new()
                .name(format!("monocurl-kernel-{index}"))
                .spawn(move || worker(receiver))
                .expect("kernel worker thread should spawn");
        }
        Pool {
            sender: Mutex::new(sender),
            workers,
        }
    })
}

fn worker(receiver: Arc<Mutex<Receiver<Job>>>) {
    loop {
        let job = match receiver.lock() {
            Ok(receiver) => receiver.recv(),
            Err(_) => return,
        };
        let Ok(job) = job else { return };
        // a panicking job must not take the worker with it; the caller notices
        // the missing result
        let _ = catch_unwind(AssertUnwindSafe(job));
    }
}

impl Pool {
    /// the number of chunks a batch should be split into so that every worker
    /// and the calling thread get one
    pub fn chunk_count(&self) -> usize {
        self.workers + 1
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
        {
            let sender = self.sender.lock().expect("kernel pool sender poisoned");
            for index in 1..count {
                let job = Arc::clone(&job);
                let results_tx = results_tx.clone();
                let _ = sender.send(Box::new(move || {
                    let _ = results_tx.send((index, job(index)));
                }));
            }
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
