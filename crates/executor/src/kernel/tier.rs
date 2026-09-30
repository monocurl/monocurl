//! the tier itself: compiled bodies by entry point, the mode switch, stats,
//! and the single-call entry point the interpreter's `LambdaInvoke` uses

use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::{
    executor::{ExecSingle, Executor, NativeFunction},
    heap::with_heap,
    value::{InstructionPointer, Value, lambda::Lambda},
};

use super::{
    compile::{self, LambdaShape, RegionShape, Reject},
    convert::{self, Converter, to_value},
    ir::Kernel,
    jit,
    run::Vm,
    value::{ClosureArena, KClosure, KVal},
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
    /// native code for scalar typed kernels; `MONOCURL_KERNEL_JIT=0` turns it
    /// off
    pub(super) jit: jit::JitCache,
    /// compiled bodies by entry point; `None` records a body the translator
    /// rejected so it is not retried
    kernels: HashMap<InstructionPointer, Option<Arc<Kernel>>>,
    /// entry points whose kernel faulted. the interpreter owns them from then
    /// on: either the fault is a real runtime error, in which case the
    /// bytecode will change before it is worth retrying, or the body touches
    /// something the tier does not model
    pub(super) disabled: FxHashSet<InstructionPointer>,
    /// loop regions of interpreted frames, by section, the pc of the backward
    /// jump that closes the loop, and the stack depth they were compiled for
    regions: HashMap<(u16, u32, usize), Option<Arc<RegionKernel>>>,
    disabled_regions: FxHashSet<(u16, u32, usize)>,
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
    /// batch calls that ran as native code (a subset of `typed_calls`)
    pub jit_calls: usize,
    /// typed kernels compiled to native code, and the time that took
    pub jit_compiles: usize,
    pub jit_compile_elapsed: Duration,
    /// batches whose entry point could not be typed
    pub typed_declined: usize,
    pub parallel_batches: usize,
    pub faults: usize,
    pub rejected_bodies: usize,
    /// loops of interpreted frames that ran as a region kernel
    pub regions: usize,
    pub region_faults: usize,
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
            jit: jit::JitCache::new(jit::enabled_by_env()),
            kernels: HashMap::new(),
            disabled: FxHashSet::default(),
            regions: HashMap::new(),
            disabled_regions: FxHashSet::default(),
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
        self.regions.clear();
        self.disabled_regions.clear();
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

/// a compiled loop region which stack positions it exchanges with the
/// interpreted frame
pub(super) struct RegionKernel {
    kernel: Arc<Kernel>,
    exit: u32,
    touched: Vec<u16>,
    written: Vec<u16>,
}

impl KernelTier {
    fn region_for(&mut self, key: (u16, u32, usize), start: u32) -> Option<Arc<RegionKernel>> {
        if let Some(cached) = self.regions.get(&key) {
            return cached.clone();
        }
        let (section_index, jump_pc, entry_depth) = key;
        let compiled = self
            .sections
            .get(section_index as usize)
            .and_then(|section| {
                let shape = RegionShape {
                    section: section_index,
                    start,
                    end: jump_pc + 1,
                    entry_depth,
                };
                let compiled = compile::compile_region(section, &shape, &self.natives);
                if dump_kernels() {
                    match &compiled {
                        Ok(kernel) => {
                            eprintln!(
                                "region {:?}..{}: {} registers",
                                kernel.ip, shape.end, kernel.frame_size
                            );
                            for (index, op) in kernel.ops.iter().enumerate() {
                                eprintln!("  {index:4}  {op:?}");
                            }
                        }
                        Err(reject) => {
                            eprintln!("region ({section_index}, {start}): rejected, {reject:?}")
                        }
                    }
                }
                let kernel = compiled.ok()?;
                let registers = compile::region_registers(&kernel, entry_depth);
                Some(Arc::new(RegionKernel {
                    kernel: Arc::new(kernel),
                    exit: jump_pc + 1,
                    touched: registers.touched,
                    written: registers.written,
                }))
            });
        self.regions.insert(key, compiled.clone());
        compiled
    }
}

/// whether a stack slot's value can be given to a region and, when written,
/// taken back: leaders, references and stateful values have interpreter-side
/// meaning a plain value would lose
fn plain_slot(value: &Value) -> bool {
    !matches!(
        value,
        Value::Leader(_) | Value::Lvalue(_) | Value::WeakLvalue(_) | Value::Stateful(_)
    )
}

impl Executor {
    /// run the loop closed by the backward jump at `jump_pc` (to `start`) as a
    /// kernel over the frame's stack. on success the stack holds the loop's
    /// results and the head is at the loop's exit; otherwise nothing changed
    /// and the interpreter runs the loop itself
    pub(crate) fn try_region(
        &mut self,
        stack_idx: usize,
        section_idx: usize,
        jump_pc: u32,
        start: u32,
    ) -> bool {
        let tier = &mut self.kernels;
        if tier.mode != KernelMode::On {
            return false;
        }
        let depth = self.state.stack(stack_idx).var_stack.len();
        let key = (section_idx as u16, jump_pc, depth);
        if tier.disabled_regions.contains(&key) {
            return false;
        }
        let Some(region) = tier.region_for(key, start) else {
            return false;
        };

        let mut arena = std::mem::take(&mut tier.arena);
        arena.clear();
        let mut frame = vec![KVal::Nil; depth];
        let mut lvalues: SmallVec<[(u16, crate::heap::HeapKey); 8]> = SmallVec::new();
        let mut converter = Converter::new(tier, &mut arena);
        let stack = &self.state.stack(stack_idx).var_stack;
        for &reg in &region.touched {
            let slot = &stack[reg as usize];
            let writes = region.written.contains(&reg);
            match slot {
                Value::Lvalue(reference) => {
                    let inner = with_heap(|heap| heap.get(reference.key()).clone());
                    if writes && !plain_slot(&inner) {
                        self.kernels.arena = arena;
                        return false;
                    }
                    frame[reg as usize] = converter.convert(&inner);
                    lvalues.push((reg, reference.key()));
                }
                other => {
                    if writes && !plain_slot(other) {
                        self.kernels.arena = arena;
                        return false;
                    }
                    frame[reg as usize] = converter.convert(other);
                }
            }
        }
        let entry = arena.push(KClosure {
            ip: region.kernel.ip,
            kernel: Arc::clone(&region.kernel),
            captures: Box::new([]),
            defaults: Box::new([]),
        });

        let tier = &mut self.kernels;
        let outcome = tier.vm.run_region(&arena, entry, frame);
        tier.arena = arena;
        let results = match outcome {
            Ok(frame) => frame,
            Err(_) => {
                tier.stats.region_faults += 1;
                tier.disabled_regions.insert(key);
                return false;
            }
        };
        // every written slot must come back onto the heap, or none does and
        // the interpreter runs the loop from the state it left
        let Some(values) = region
            .written
            .iter()
            .map(|&reg| to_value(&results[reg as usize]).map(|value| (reg, value)))
            .collect::<Option<Vec<_>>>()
        else {
            tier.stats.region_faults += 1;
            tier.disabled_regions.insert(key);
            return false;
        };
        tier.stats.regions += 1;
        let stack = self.state.stack_mut(stack_idx);
        for (reg, value) in values {
            match lvalues.iter().find(|(written, _)| *written == reg) {
                Some(&(_, key)) => crate::heap::heap_replace(key, value),
                None => stack.var_stack[reg as usize] = value,
            }
        }
        stack.ip = (section_idx as u16, region.exit);
        true
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
        // the interpreter keeps a call to a lambda with defaults as a live
        // value whose arguments lerp; the machine would hand back its plain
        // result, which the caller could tell apart
        if converter.arena().any_defaults() {
            tier.arena = arena;
            return None;
        }
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

    /// whether the tier can compile `lambda`, which means its body reads
    /// nothing but its arguments and captures: a result cache can key on it
    pub fn lambda_is_pure(&mut self, lambda: &Lambda) -> bool {
        self.kernels.kernel_for(lambda).is_some()
    }
}
