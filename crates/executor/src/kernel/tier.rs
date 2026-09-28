//! the tier itself: compiled bodies by entry point, the mode switch, stats,
//! and the single-call entry point the interpreter's `LambdaInvoke` uses

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

use super::{
    compile::{self, LambdaShape, Reject},
    convert::{self, Converter},
    ir::Kernel,
    run::Vm,
    value::{ClosureArena, KVal},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelMode {
    /// everything runs in the interpreter
    Off,
    /// batches of pure numeric calls run as kernels
    On,
    /// batches only; interpreted single calls stay in the interpreter. for
    /// bisecting a disagreement between the two paths
    Batches,
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
            Ok("batch") | Ok("batches") => KernelMode::Batches,
            _ => KernelMode::On,
        })
    }
}

pub(crate) struct KernelTier {
    pub(crate) mode: KernelMode,
    /// whether batches try the typed machine first; `MONOCURL_TYPED_KERNELS=0`
    /// turns it off for bisecting
    pub(super) typed: bool,
    /// whether typed batches run several calls per op on the lane machine;
    /// `MONOCURL_KERNEL_LANES=0` turns it off
    pub(super) lanes: bool,
    /// compiled bodies by entry point; `None` records a body the translator
    /// rejected so it is not retried
    kernels: HashMap<InstructionPointer, Option<Arc<Kernel>>>,
    /// entry points whose kernel faulted. the interpreter owns them from then
    /// on: either the fault is a real runtime error, in which case the
    /// bytecode will change before it is worth retrying, or the body touches
    /// something the tier does not model
    pub(super) disabled: FxHashSet<InstructionPointer>,
    /// the bytecode and natives kernels are compiled against; refreshed by the
    /// executor whenever they change
    sections: Vec<Arc<bytecode::SectionBytecode>>,
    natives: Vec<NativeFunction>,
    pub(super) stats: KernelStats,
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
    /// batch calls that ran on the typed machine
    pub typed_calls: usize,
    /// batch calls that ran on the lane machine (a subset of `typed_calls`)
    pub lane_calls: usize,
    /// batches whose entry point could not be typed
    pub typed_declined: usize,
    pub parallel_batches: usize,
    pub faults: usize,
    pub rejected_bodies: usize,
    /// time spent in the tier, conversion included
    pub elapsed: Duration,
    /// the part of `elapsed` spent running kernels
    pub run_elapsed: Duration,
}

fn typed_kernels_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| env_switch("MONOCURL_TYPED_KERNELS"))
}

/// an environment switch that is on unless set to `0`, `off` or `false`
fn env_switch(name: &str) -> bool {
    !matches!(
        std::env::var(name).as_deref(),
        Ok("0") | Ok("off") | Ok("false")
    )
}

impl KernelTier {
    pub(crate) fn new(
        mode: KernelMode,
        sections: Vec<Arc<bytecode::SectionBytecode>>,
        natives: Vec<NativeFunction>,
    ) -> Self {
        Self {
            mode,
            typed: typed_kernels_enabled(),
            lanes: env_switch("MONOCURL_KERNEL_LANES"),
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

    pub(super) fn kernel_for(&mut self, lambda: &Lambda) -> Option<Arc<Kernel>> {
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

/// `MONOCURL_KERNEL_DUMP=1` prints every body the translator sees, compiled or
/// rejected, to stderr
pub(super) fn dump_kernels() -> bool {
    static DUMP: OnceLock<bool> = OnceLock::new();
    *DUMP.get_or_init(|| std::env::var_os("MONOCURL_KERNEL_DUMP").is_some())
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

impl Executor {
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
