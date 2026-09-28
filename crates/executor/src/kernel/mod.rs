//! the kernel tier: a second execution engine for pure numeric lambdas.
//!
//! the interpreter keeps every local in a heap slot and copies tagged values
//! through a stack, which is the right shape for the scene language but far
//! too slow for a function sampled once per vertex or per pixel. a lambda whose
//! body only does arithmetic, list construction, indexing, control flow, calls
//! to other such lambdas and calls to pure natives is translated once into
//! register code (`compile`) and then run by a small machine (`run`) over plain
//! values (`value`) that owe nothing to the thread-local heap, so a batch of
//! calls can be spread over worker threads.
//!
//! the tier is speculative and never the source of truth: anything it cannot
//! model faults, and a fault hands the whole batch back to the interpreter,
//! which produces the real result or the real error. `KernelMode::Verify`
//! runs both and compares, which is how the tier is tested

pub mod compile;
pub mod convert;
pub mod ir;
pub mod pool;
pub mod run;
pub mod value;

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

use rustc_hash::FxHashSet;

use crate::{
    executor::{ExecSingle, Executor, NativeFunction},
    value::{InstructionPointer, Value, lambda::Lambda},
};

pub use self::ir::KernelIntrinsic;
pub use self::convert::to_value as kernel_value_to_value;
pub use self::value::KVal;
use self::{
    compile::{LambdaShape, Reject},
    convert::Converter,
    ir::Kernel,
    run::{Fault, Vm},
    value::{ClosureArena, ClosureId},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelMode {
    /// everything runs in the interpreter
    Off,
    /// batches of pure numeric calls run as kernels
    On,
    /// kernels run and the interpreter runs too; a disagreement panics. for
    /// tests, since it costs more than either engine alone
    Verify,
}

impl KernelMode {
    /// `MONOCURL_KERNELS=0|off` disables the tier, `verify` cross-checks it,
    /// anything else (including unset) enables it
    pub fn from_env() -> Self {
        static MODE: OnceLock<KernelMode> = OnceLock::new();
        *MODE.get_or_init(|| match std::env::var("MONOCURL_KERNELS").as_deref() {
            Ok("0") | Ok("off") | Ok("false") => KernelMode::Off,
            Ok("verify") => KernelMode::Verify,
            _ => KernelMode::On,
        })
    }
}

pub(crate) struct KernelTier {
    pub(crate) mode: KernelMode,
    /// compiled bodies by entry point; `None` records a body the translator
    /// rejected so it is not retried
    kernels: HashMap<InstructionPointer, Option<Arc<Kernel>>>,
    /// entry points whose kernel faulted. the interpreter owns them from then
    /// on: either the fault is a real runtime error, in which case the
    /// bytecode will change before it is worth retrying, or the body touches
    /// something the tier does not model
    disabled: FxHashSet<InstructionPointer>,
    /// the bytecode and natives kernels are compiled against; refreshed by the
    /// executor whenever they change
    sections: Vec<Arc<bytecode::SectionBytecode>>,
    natives: Vec<NativeFunction>,
    stats: KernelStats,
    /// the register machine single calls from the interpreter run on, and the
    /// arena those calls convert into
    vm: Vm,
    arena: ClosureArena,
}

/// counters for benchmarks and tests
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KernelStats {
    pub batches: usize,
    pub calls: usize,
    /// interpreted calls that ran as a kernel on their own
    pub single_calls: usize,
    pub parallel_batches: usize,
    pub faults: usize,
    pub rejected_bodies: usize,
    /// time spent in the tier, conversion included
    pub elapsed: Duration,
    /// the part of `elapsed` spent running kernels
    pub run_elapsed: Duration,
}

/// calls below this count are not worth spreading over threads
const PARALLEL_MIN_CALLS: usize = 64;
/// how many calls to time before deciding whether a batch is long enough to
/// parallelise
const PARALLEL_PROBE_CALLS: usize = 8;
/// projected serial time above which the remaining calls go to worker threads
const PARALLEL_MIN_PROJECTED: Duration = Duration::from_micros(250);

impl KernelTier {
    pub(crate) fn new(
        mode: KernelMode,
        sections: Vec<Arc<bytecode::SectionBytecode>>,
        natives: Vec<NativeFunction>,
    ) -> Self {
        Self {
            mode,
            kernels: HashMap::new(),
            disabled: FxHashSet::default(),
            sections,
            natives,
            stats: KernelStats::default(),
            vm: Vm::new(),
            arena: ClosureArena::default(),
        }
    }

    /// forget every compiled body; called when the bytecode changes
    pub(crate) fn reset(&mut self, sections: Vec<Arc<bytecode::SectionBytecode>>) {
        self.sections = sections;
        self.kernels.clear();
        self.disabled.clear();
    }

    pub(crate) fn stats(&self) -> KernelStats {
        self.stats
    }

    fn kernel_for(&mut self, lambda: &Lambda) -> Option<Arc<Kernel>> {
        if let Some(cached) = self.kernels.get(&lambda.ip) {
            return cached.clone();
        }
        let compiled = self.compile(lambda);
        if compiled.is_none() {
            self.stats.rejected_bodies += 1;
        }
        self.kernels.insert(lambda.ip, compiled.clone());
        compiled
    }

    fn compile(&self, lambda: &Lambda) -> Option<Arc<Kernel>> {
        let section = self.sections.get(lambda.ip.0 as usize)?;
        let shape = LambdaShape {
            ip: lambda.ip,
            required_args: lambda.required_args,
            total_args: lambda.total_args() as u16,
            capture_count: lambda.captures.len() as u16,
            has_reference_args: lambda.reference_args.iter().any(|reference| *reference),
        };
        let compiled = compile::compile(section, &shape, &self.natives);
        if dump_kernels() {
            match &compiled {
                Ok(kernel) => {
                    eprintln!(
                        "kernel {:?}: {} args, {} captures, {} registers",
                        kernel.ip, kernel.total_args, kernel.capture_count, kernel.frame_size
                    );
                    for (index, op) in kernel.ops.iter().enumerate() {
                        eprintln!("  {index:4}  {op:?}");
                    }
                }
                Err(reject) => eprintln!("kernel {:?}: rejected, {reject:?}", shape.ip),
            }
        }
        compiled.ok().map(Arc::new)
    }

    /// the translator's verdict on a body, for tests
    pub(crate) fn reject_reason(&self, lambda: &Lambda) -> Result<(), Reject> {
        let Some(section) = self.sections.get(lambda.ip.0 as usize) else {
            return Err(Reject::NoBodyBounds);
        };
        let shape = LambdaShape {
            ip: lambda.ip,
            required_args: lambda.required_args,
            total_args: lambda.total_args() as u16,
            capture_count: lambda.captures.len() as u16,
            has_reference_args: lambda.reference_args.iter().any(|reference| *reference),
        };
        compile::compile(section, &shape, &self.natives).map(|_| ())
    }
}

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
    if threads <= 1 || remaining.len() < PARALLEL_MIN_CALLS || projected < PARALLEL_MIN_PROJECTED
    {
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

/// `MONOCURL_KERNEL_DUMP=1` prints every body the translator sees, compiled or
/// rejected, to stderr
fn dump_kernels() -> bool {
    static DUMP: OnceLock<bool> = OnceLock::new();
    *DUMP.get_or_init(|| std::env::var_os("MONOCURL_KERNEL_DUMP").is_some())
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

    /// try to run one interpreted call of `lambda` (its `num_args` arguments
    /// on top of the stack, callee already popped) as a kernel. only bodies
    /// that loop, call or are large qualify, and only when the captures are
    /// cheap to convert, since a single call cannot amortise the conversion the
    /// way a batch does. on success the arguments are replaced by the result
    pub(crate) fn try_kernel_call(
        &mut self,
        stack_idx: usize,
        lambda: &Lambda,
        num_args: usize,
    ) -> Option<ExecSingle> {
        let tier = &mut self.kernels;
        if tier.mode != KernelMode::On
            || tier.disabled.contains(&lambda.ip)
            || !cheap_captures(lambda)
        {
            return None;
        }
        let kernel = tier.kernel_for(lambda)?;
        if !kernel.worth_single_call() {
            return None;
        }
        let mut arena = std::mem::take(&mut tier.arena);
        arena.clear();
        let mut converter = Converter::new(tier, &mut arena);
        let Some(entry) = converter.place(lambda) else {
            tier.arena = arena;
            return None;
        };
        let args: Vec<KVal> = {
            let stack = self.state.stack(stack_idx);
            stack
                .top(num_args)
                .iter()
                .map(|arg| converter.convert(arg))
                .collect()
        };
        // the arena is rebuilt per call, so the machine's warm entry frame
        // must not be trusted across calls
        tier.vm.forget_entry();
        tier.stats.single_calls += 1;
        let outcome = tier.vm.call(&arena, entry, &args);
        tier.arena = arena;
        let result = match outcome {
            Ok(result) => result,
            Err(_) => {
                tier.stats.faults += 1;
                tier.disabled.insert(lambda.ip);
                return None;
            }
        };
        let Some(value) = convert::to_value(&result) else {
            tier.disabled.insert(lambda.ip);
            return None;
        };
        let stack = self.state.stack_mut(stack_idx);
        stack.pop_n(num_args);
        stack.push(value);
        Some(ExecSingle::Continue)
    }

    pub fn set_kernel_mode(&mut self, mode: KernelMode) {
        self.kernels.mode = mode;
    }

    pub fn kernel_mode(&self) -> KernelMode {
        self.kernels.mode
    }

    pub fn kernel_stats(&self) -> KernelStats {
        self.kernels.stats()
    }

    /// whether the tier would compile `lambda`, and if not why; for tests
    pub fn kernel_reject_reason(&self, lambda: &Lambda) -> Result<(), Reject> {
        self.kernels.reject_reason(lambda)
    }
}

/// the default arguments of `entry`, in arity order
fn arena_defaults<'a>(converter: &'a Converter<'_>, entry: ClosureId) -> &'a [KVal] {
    &converter.arena().get(entry).defaults
}

/// elements a captured list may hold before converting it on every single
/// call would cost more than the call
const CHEAP_CAPTURE_LEN: usize = 16;

/// whether converting `lambda`'s captures is cheap enough to do per call
fn cheap_captures(lambda: &Lambda) -> bool {
    lambda.captures.iter().all(|capture| match capture {
        Value::List(list) => list.len() <= CHEAP_CAPTURE_LEN,
        Value::Lambda(inner) => cheap_captures(inner),
        _ => true,
    })
}

/// equality that distinguishes `1` from `1.0`, which `values_equal` does not:
/// the tier must reproduce the interpreter's result types exactly
pub fn strictly_equal(a: &Value, b: &Value) -> bool {
    use crate::heap::with_heap;
    match (
        &a.clone().elide_cached_wrappers_rec(),
        &b.clone().elide_cached_wrappers_rec(),
    ) {
        (Value::Nil, Value::Nil) => true,
        (Value::Integer(x), Value::Integer(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()),
        (Value::List(x), Value::List(y)) => {
            x.len() == y.len()
                && x.elements().iter().zip(y.elements()).all(|(a, b)| {
                    with_heap(|heap| strictly_equal(&heap.get(a.key()), &heap.get(b.key())))
                })
        }
        (a, b) => Value::values_equal(a, b),
    }
}
