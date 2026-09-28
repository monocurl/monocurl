//! running a batch of calls: arguments laid flat, a serial probe, then the
//! worker pool once the batch has shown itself to be long

use std::{
    sync::{Arc, OnceLock},
    time::Duration,
};

use crate::{
    executor::Executor,
    value::{Value, lambda::Lambda},
};

use super::{
    KernelMode,
    convert::Converter,
    pool,
    run::{Fault, Vm},
    value::{ClosureArena, ClosureId, KVal},
};

/// calls below this count are not worth spreading over threads
const PARALLEL_MIN_CALLS: usize = 64;

/// how many calls to time before deciding whether a batch is long enough to
/// parallelise
const PARALLEL_PROBE_CALLS: usize = 8;

/// projected serial time above which the remaining calls go to worker threads
const PARALLEL_MIN_PROJECTED: Duration = Duration::from_micros(250);

/// the calls of one batch: every argument list laid flat with a fixed stride,
/// so a batch of thousands of calls is one allocation, shared with the workers
struct BatchArgs {
    values: Vec<KVal>,
    arity: usize,
}

impl BatchArgs {
    fn len(&self) -> usize {
        if self.arity == 0 {
            self.values.len()
        } else {
            self.values.len() / self.arity
        }
    }

    fn call(&self, index: usize) -> &[KVal] {
        &self.values[index * self.arity..(index + 1) * self.arity]
    }
}

/// run the calls `range` of `args`, serially at first and on the worker pool
/// once the batch has shown itself to be long. the first fault aborts the
/// batch: the interpreter will find the same error. the flag says whether
/// worker threads were used
fn run_batch(
    arena: &Arc<ClosureArena>,
    entry: ClosureId,
    args: &Arc<BatchArgs>,
    range: std::ops::Range<usize>,
) -> Result<(Vec<KVal>, bool), Fault> {
    let mut results = Vec::with_capacity(range.len());
    let mut vm = Vm::new();

    let probe = range.start + range.len().min(PARALLEL_PROBE_CALLS);
    let started = Stopwatch::start();
    for index in range.start..probe {
        results.push(vm.call(arena, entry, args.call(index))?);
    }
    let remaining = probe..range.end;
    if remaining.is_empty() {
        return Ok((results, false));
    }

    let projected = started.elapsed() * (range.len() / (probe - range.start)) as u32;
    let threads = worker_threads();
    if threads <= 1 || remaining.len() < PARALLEL_MIN_CALLS || projected < PARALLEL_MIN_PROJECTED {
        for index in remaining {
            results.push(vm.call(arena, entry, args.call(index))?);
        }
        return Ok((results, false));
    }

    let pool = pool::pool(threads);
    let chunks = pool.chunk_count();
    let chunk_len = remaining.len().div_ceil(chunks);
    let arena = Arc::clone(arena);
    let args = Arc::clone(args);
    let outcomes = pool.run(chunks, move |chunk| {
        let start = remaining.start + chunk * chunk_len;
        let end = (start + chunk_len).min(remaining.end);
        let mut vm = Vm::new();
        (start..end.max(start))
            .map(|index| vm.call(&arena, entry, args.call(index)))
            .collect::<Result<Vec<KVal>, Fault>>()
    });
    for outcome in outcomes {
        // a worker that never reported back has panicked; the interpreter
        // owns this batch then
        results.extend(outcome.ok_or(Fault::Type)??);
    }
    Ok((results, true))
}

/// wall-clock measurement that degrades to zero on wasm, where `Instant` is
/// not available; the tier then never parallelises and never yields early,
/// which is the right behaviour on a single-threaded target anyway
#[derive(Clone, Copy)]
struct Stopwatch {
    #[cfg(not(target_arch = "wasm32"))]
    started: std::time::Instant,
}

impl Stopwatch {
    fn start() -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            started: std::time::Instant::now(),
        }
    }

    fn elapsed(self) -> Duration {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started.elapsed()
        }
        #[cfg(target_arch = "wasm32")]
        {
            Duration::ZERO
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn worker_threads() -> usize {
    1
}

#[cfg(not(target_arch = "wasm32"))]
fn worker_threads() -> usize {
    static THREADS: OnceLock<usize> = OnceLock::new();
    *THREADS.get_or_init(|| {
        // `MONOCURL_KERNEL_THREADS` pins the count, for benchmarking the
        // serial path and for keeping test runs deterministic in shape
        if let Some(pinned) = std::env::var("MONOCURL_KERNEL_THREADS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
        {
            return pinned.max(1);
        }
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1)
            .min(16)
    })
}

pub(crate) enum BatchOutcome {
    /// one raw result per call; callers read what they need straight off the
    /// kernel values and only go through the heap for shapes they do not know
    Results(Vec<KVal>),
    /// the tier declined or faulted; the interpreter takes the batch
    Interpreter,
}

/// calls per slice of a batch. the driver yields to the async runtime between
/// slices that take a while, so a huge batch cannot hold the executor's task
/// for longer than one slice at a time
const CHUNK_CALLS: usize = 4096;

const YIELD_AFTER: Duration = Duration::from_millis(4);

/// the default arguments of `entry`, in arity order
fn arena_defaults<'a>(converter: &'a Converter<'_>, entry: ClosureId) -> &'a [KVal] {
    &converter.arena().get(entry).defaults
}

impl Executor {
    /// try to run a batch of calls to `lambda` as a kernel
    pub(crate) async fn kernel_batch<A: AsRef<[Value]>>(
        &mut self,
        lambda: &Lambda,
        args: &[A],
    ) -> BatchOutcome {
        let tier = &mut self.kernels;
        if tier.mode == KernelMode::Off || tier.disabled.contains(&lambda.ip) {
            return BatchOutcome::Interpreter;
        }
        let started = Stopwatch::start();
        let mut arena = ClosureArena::default();
        let mut converter = Converter::new(tier, &mut arena);
        let Some(entry) = converter.place(lambda) else {
            return BatchOutcome::Interpreter;
        };
        let arity = lambda.total_args();
        let mut values = Vec::with_capacity(args.len() * arity);
        for call in args {
            let provided = call.as_ref();
            values.extend(provided.iter().map(|arg| converter.convert(arg)));
            // defaults are filled here so every call has the full arity
            let defaults = arena_defaults(&converter, entry);
            let missing = (arity - provided.len()).min(defaults.len());
            values.extend(defaults[defaults.len() - missing..].iter().cloned());
        }
        let arena = Arc::new(arena);
        let batch = Arc::new(BatchArgs { values, arity });
        let call_count = batch.len();

        tier.stats.batches += 1;
        tier.stats.calls += call_count;
        let mut was_parallel = false;
        let mut results = Vec::with_capacity(call_count);
        let mut chunk_start = 0;
        while chunk_start < call_count {
            let chunk_end = (chunk_start + CHUNK_CALLS).min(call_count);
            let run_started = Stopwatch::start();
            let outcome = run_batch(&arena, entry, &batch, chunk_start..chunk_end);
            chunk_start = chunk_end;
            let run_elapsed = run_started.elapsed();
            let tier = &mut self.kernels;
            tier.stats.run_elapsed += run_elapsed;
            match outcome {
                Ok((chunk_results, parallel)) => {
                    was_parallel |= parallel;
                    results.extend(chunk_results);
                }
                Err(_) => {
                    tier.stats.faults += 1;
                    tier.disabled.insert(lambda.ip);
                    return BatchOutcome::Interpreter;
                }
            }
            if chunk_start < call_count && run_elapsed >= YIELD_AFTER {
                structs::futures::yield_now().await;
            }
        }
        let tier = &mut self.kernels;
        if was_parallel {
            tier.stats.parallel_batches += 1;
        }
        tier.stats.elapsed += started.elapsed();
        BatchOutcome::Results(results)
    }

    /// a batch result the tier could not bring back onto the heap; the entry
    /// point is handed to the interpreter for good
    pub(crate) fn kernel_result_unconvertible(&mut self, lambda: &Lambda) {
        self.kernels.disabled.insert(lambda.ip);
    }
}
