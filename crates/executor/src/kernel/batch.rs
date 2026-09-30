//! running a batch of calls: arguments laid flat, a serial probe, then the
//! worker pool once the batch has shown itself to be long

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use smallvec::SmallVec;

use crate::{
    executor::Executor,
    value::{Value, lambda::Lambda},
};

use super::{
    KernelMode,
    convert::Converter,
    jit::{JitEntry, JitVm},
    lanes::{LANES, LVm},
    pool,
    run::{Fault, Vm},
    typed::{SpecId, TypedProgram},
    typed_run::TVm,
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

/// how one batch runs: which specialisation it has, if any, its native code,
/// whether the lane machine is on, and whether every result is cross-checked
#[derive(Clone)]
struct Plan {
    typed: Option<Arc<(TypedProgram, SpecId)>>,
    jit: Option<Arc<JitEntry>>,
    lanes: bool,
    verify: bool,
}

/// the machines one batch runs on: native code for typed calls when the
/// specialisation compiled, else the lane machine for groups of typed calls;
/// the typed one for the rest and for calls those fault, the dynamic one for
/// calls the typed machines decline or fault
struct Machines<'a> {
    arena: &'a ClosureArena,
    entry: ClosureId,
    plan: &'a Plan,
    vm: Vm,
    tvm: TVm,
    lvm: LVm,
    jit: Option<JitVm>,
    typed_calls: usize,
    lane_calls: usize,
    jit_calls: usize,
}

impl<'a> Machines<'a> {
    fn new(
        arena: &'a ClosureArena,
        entry: ClosureId,
        plan: &'a Plan,
        abort: Option<&Arc<AtomicBool>>,
    ) -> Self {
        let (vm, tvm, lvm) = match abort {
            Some(abort) => (
                Vm::with_abort(Arc::clone(abort)),
                TVm::with_abort(Arc::clone(abort)),
                LVm::with_abort(Arc::clone(abort)),
            ),
            None => (Vm::new(), TVm::new(), LVm::new()),
        };
        let jit = plan
            .typed
            .as_deref()
            .zip(plan.jit.as_ref())
            .map(|((program, spec), jit)| {
                JitVm::new(
                    Arc::clone(jit),
                    program.spec(*spec),
                    arena,
                    abort.map(Arc::clone),
                )
            });
        Self {
            arena,
            entry,
            plan,
            vm,
            tvm,
            lvm,
            jit,
            typed_calls: 0,
            lane_calls: 0,
            jit_calls: 0,
        }
    }

    /// run the calls `range` of `args`, appending their results
    fn run(
        &mut self,
        args: &BatchArgs,
        range: std::ops::Range<usize>,
        results: &mut Vec<KVal>,
    ) -> Result<(), Fault> {
        if self.jit.is_some() {
            for index in range {
                results.push(self.jit_call(args.call(index))?);
            }
            return Ok(());
        }
        let Some(typed) = self.plan.typed.as_deref().filter(|_| self.plan.lanes) else {
            for index in range {
                results.push(self.call(args.call(index))?);
            }
            return Ok(());
        };
        let (program, spec) = typed;
        let mut index = range.start;
        while index < range.end {
            let group_end = (index + LANES).min(range.end);
            let group: SmallVec<[&[KVal]; LANES]> =
                (index..group_end).map(|i| args.call(i)).collect();
            let accepted = group_end - index > 1
                && group
                    .iter()
                    .all(|call| TVm::accepts(program.spec(*spec), call));
            if accepted {
                match self.lvm.call(program, self.arena, *spec, &group) {
                    Ok(lane_results) => {
                        self.lane_calls += group.len();
                        self.typed_calls += group.len();
                        if self.plan.verify {
                            for (call, result) in group.iter().zip(&lane_results) {
                                let dynamic = self.vm.call(self.arena, self.entry, call)?;
                                assert!(
                                    KVal::strictly_equal(result, &dynamic),
                                    "lane machine disagrees with the dynamic machine: {result:?} vs {dynamic:?}"
                                );
                            }
                        }
                        results.extend(lane_results);
                        index = group_end;
                        continue;
                    }
                    Err(fault @ (Fault::Budget | Fault::Aborted)) => return Err(fault),
                    // one lane faulted; each call finds out on its own
                    Err(_) => {}
                }
            }
            for call in group {
                results.push(self.call(call)?);
            }
            index = group_end;
        }
        Ok(())
    }

    /// one call as native code, falling back to the typed machine when its
    /// classes differ or it faults
    fn jit_call(&mut self, args: &[KVal]) -> Result<KVal, Fault> {
        let (Some((program, spec)), Some(jit)) = (self.plan.typed.as_deref(), &mut self.jit) else {
            return self.call(args);
        };
        let spec = program.spec(*spec);
        if !TVm::accepts(spec, args) {
            return self.call(args);
        }
        match jit.call(spec, self.arena, args) {
            Ok(native) => {
                self.jit_calls += 1;
                self.typed_calls += 1;
                if self.plan.verify {
                    let dynamic = self.vm.call(self.arena, self.entry, args)?;
                    assert!(
                        KVal::strictly_equal(&native, &dynamic),
                        "native kernel disagrees with the dynamic machine: {native:?} vs {dynamic:?}"
                    );
                }
                Ok(native)
            }
            Err(fault @ (Fault::Budget | Fault::Aborted)) => Err(fault),
            Err(_) => self.call(args),
        }
    }

    fn call(&mut self, args: &[KVal]) -> Result<KVal, Fault> {
        let Some((program, spec)) = self.plan.typed.as_deref() else {
            return self.vm.call(self.arena, self.entry, args);
        };
        if !TVm::accepts(program.spec(*spec), args) {
            return self.vm.call(self.arena, self.entry, args);
        }
        match self.tvm.call(program, self.arena, *spec, args) {
            Ok(typed) => {
                self.typed_calls += 1;
                if self.plan.verify {
                    let dynamic = self.vm.call(self.arena, self.entry, args)?;
                    assert!(
                        KVal::strictly_equal(&typed, &dynamic),
                        "typed kernel disagrees with the dynamic machine: {typed:?} vs {dynamic:?}"
                    );
                }
                Ok(typed)
            }
            // these say nothing about the call itself
            Err(fault @ (Fault::Budget | Fault::Aborted)) => Err(fault),
            // a typed fault is either a real error or a shape the typed
            // machine does not handle; the dynamic one decides which
            Err(_) => self.vm.call(self.arena, self.entry, args),
        }
    }
}

/// counts of how a range of calls ran
#[derive(Clone, Copy, Default)]
struct Ran {
    typed: usize,
    lanes: usize,
    jit: usize,
    parallel: bool,
}

/// run the calls `range` of `args`, serially at first and on the worker pool
/// once the batch has shown itself to be long. the first fault aborts the
/// batch: the interpreter will find the same error
fn run_batch(
    arena: &Arc<ClosureArena>,
    entry: ClosureId,
    plan: &Plan,
    args: &Arc<BatchArgs>,
    range: std::ops::Range<usize>,
) -> Result<(Vec<KVal>, Ran), Fault> {
    let mut results = Vec::with_capacity(range.len());
    let mut machines = Machines::new(arena, entry, plan, None);
    let mut ran = Ran::default();

    let probe = range.start + range.len().min(PARALLEL_PROBE_CALLS);
    let started = Stopwatch::start();
    machines.run(args, range.start..probe, &mut results)?;
    let remaining = probe..range.end;
    let projected = started.elapsed() * (range.len() / (probe - range.start)) as u32;
    let threads = worker_threads();
    if remaining.is_empty()
        || threads <= 1
        || remaining.len() < PARALLEL_MIN_CALLS
        || projected < PARALLEL_MIN_PROJECTED
    {
        machines.run(args, remaining, &mut results)?;
        ran.typed = machines.typed_calls;
        ran.lanes = machines.lane_calls;
        ran.jit = machines.jit_calls;
        return Ok((results, ran));
    }

    let pool = pool::pool(threads);
    let chunks = pool.chunk_count();
    let chunk_len = remaining.len().div_ceil(chunks);
    let arena = Arc::clone(arena);
    let args = Arc::clone(args);
    let plan = plan.clone();
    // the first fault raises the flag and every other worker stops at its
    // next look, so an endless loop in user code costs one budget, not one
    // per call
    let aborted = Arc::new(AtomicBool::new(false));
    let outcomes = pool.run(chunks, move |chunk| {
        let start = remaining.start + chunk * chunk_len;
        let end = (start + chunk_len).min(remaining.end).max(start);
        let mut machines = Machines::new(&arena, entry, &plan, Some(&aborted));
        let mut results = Vec::with_capacity(end - start);
        let outcome = if aborted.load(Ordering::Relaxed) {
            Err(Fault::Aborted)
        } else {
            machines.run(&args, start..end, &mut results)
        };
        if outcome.is_err() {
            aborted.store(true, Ordering::Relaxed);
        }
        outcome.map(|()| {
            let counts = Ran {
                typed: machines.typed_calls,
                lanes: machines.lane_calls,
                jit: machines.jit_calls,
                parallel: true,
            };
            (results, counts)
        })
    });
    ran.typed = machines.typed_calls;
    ran.lanes = machines.lane_calls;
    ran.jit = machines.jit_calls;
    ran.parallel = true;
    for outcome in outcomes {
        // a worker that never reported back has panicked; the interpreter
        // owns this batch then
        let (chunk_results, counts) = outcome.ok_or(Fault::Type)??;
        results.extend(chunk_results);
        ran.typed += counts.typed;
        ran.lanes += counts.lanes;
        ran.jit += counts.jit;
    }
    Ok((results, ran))
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
    use std::sync::OnceLock;

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
        // the batch's first call decides the classes the kernels are
        // specialised to; calls with other classes run dynamically
        let typed = (tier.typed && call_count > 0)
            .then(|| TypedProgram::specialise(&arena, entry, batch.call(0)))
            .flatten()
            .map(Arc::new);
        if tier.typed && typed.is_none() {
            tier.stats.typed_declined += 1;
        }
        if super::tier::dump_kernels() {
            match &typed {
                Some(typed) => {
                    for (id, spec) in typed.0.specs.iter().enumerate() {
                        eprintln!(
                            "typed spec {id} of closure {:?} for {:?} -> {:?}, {} registers",
                            spec.closure, spec.sig, spec.ret, spec.frame_size
                        );
                        for (index, op) in spec.ops.iter().enumerate() {
                            eprintln!("{index:6}  {op:?}");
                        }
                    }
                }
                None => eprintln!("typed specialisation declined for {:?}", lambda.ip),
            }
        }
        let jit = typed
            .as_deref()
            .and_then(|(program, spec)| tier.jit.prepare(program.spec(*spec), &mut tier.stats));
        let plan = Plan {
            typed,
            jit,
            lanes: tier.lanes,
            verify: tier.mode == KernelMode::Verify,
        };
        let mut was_parallel = false;
        let mut results = Vec::with_capacity(call_count);
        let mut chunk_start = 0;
        while chunk_start < call_count {
            let chunk_end = (chunk_start + CHUNK_CALLS).min(call_count);
            let run_started = Stopwatch::start();
            let outcome = run_batch(&arena, entry, &plan, &batch, chunk_start..chunk_end);
            chunk_start = chunk_end;
            let run_elapsed = run_started.elapsed();
            let tier = &mut self.kernels;
            tier.stats.run_elapsed += run_elapsed;
            match outcome {
                Ok((chunk_results, ran)) => {
                    was_parallel |= ran.parallel;
                    tier.stats.typed_calls += ran.typed;
                    tier.stats.lane_calls += ran.lanes;
                    tier.stats.jit_calls += ran.jit;
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
