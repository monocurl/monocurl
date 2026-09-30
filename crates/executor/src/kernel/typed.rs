//! typed kernels: a batch's calls all take arguments of the same shapes, so
//! its kernels can be specialised to them. an abstract interpretation over
//! the register ops assigns every register at every point a class (int,
//! float, a statically known closure, or boxed), and ops between scalar
//! registers become ops on plain machine words, with no tag to test and no
//! value to clone. anything else stays a boxed `KVal` handled by the dynamic
//! machine's own routines, so results are identical to the dynamic machine's
//! by construction.
//!
//! specialisation is speculative like the rest of the tier: a body whose
//! registers cannot be given one class (a variable that is an int on one path
//! and a float on another, a call through a closure that is not known
//! statically, recursion) is declined and the batch runs on the dynamic
//! machine as before

use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::{
    ir::{BinKind, KOp, Kernel, KernelIntrinsic, Reg},
    value::{ClosureArena, ClosureId, KVal},
};

/// what a register holds at one point of a specialised body
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Class {
    /// written on no path yet; reading it is a translation error
    Unset,
    Int,
    Float,
    /// a closure known at translation time, so it needs no register at all
    Closure(ClosureId),
    /// anything else, kept as a `KVal`
    Boxed,
}

impl Class {
    pub fn of(value: &KVal) -> Self {
        match value {
            KVal::Int(_) => Class::Int,
            KVal::Float(_) => Class::Float,
            KVal::Closure(id) => Class::Closure(*id),
            _ => Class::Boxed,
        }
    }

    /// the class a register has where two paths meet: paths that disagree
    /// leave it boxed, and the scalar path boxes its value on the way in
    fn join(self, other: Class) -> Class {
        match (self, other) {
            (Class::Unset, x) | (x, Class::Unset) => x,
            (a, b) if a == b => a,
            _ => Class::Boxed,
        }
    }

    fn is_scalar(self) -> bool {
        matches!(self, Class::Int | Class::Float)
    }
}

/// how a typed op reads one of its inputs
#[derive(Clone, Copy, Debug)]
pub enum Opnd {
    I(Reg),
    F(Reg),
    B(Reg),
    C(ClosureId),
}

pub type SpecId = u32;

#[derive(Clone, Copy, Debug)]
pub enum TOp {
    Nop,
    Nil {
        dst: Reg,
    },
    /// move a scalar register into the boxed file, where a merge needs it
    BoxI {
        reg: Reg,
    },
    BoxF {
        reg: Reg,
    },
    BoxC {
        reg: Reg,
        id: ClosureId,
    },
    IConst {
        dst: Reg,
        value: i64,
    },
    FConst {
        dst: Reg,
        value: f64,
    },
    /// a copy within the scalar file (int or float, both are one word)
    MoveS {
        dst: Reg,
        src: Reg,
    },
    MoveB {
        dst: Reg,
        src: Reg,
    },
    IToF {
        dst: Reg,
        src: Reg,
    },
    IAdd {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    ISub {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    IMul {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    /// the rest of the int ops: division to float, floor division, power to
    /// float and comparisons
    IBin {
        op: BinKind,
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FAdd {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FSub {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FMul {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FDiv {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FBin {
        op: BinKind,
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    INeg {
        dst: Reg,
        src: Reg,
    },
    FNeg {
        dst: Reg,
        src: Reg,
    },
    INot {
        dst: Reg,
        src: Reg,
    },
    FNot {
        dst: Reg,
        src: Reg,
    },
    Jump {
        to: u32,
    },
    JumpIfI {
        cond: Reg,
        to: u32,
    },
    JumpIfNotI {
        cond: Reg,
        to: u32,
    },
    JumpIfF {
        cond: Reg,
        to: u32,
    },
    JumpIfNotF {
        cond: Reg,
        to: u32,
    },
    RangeTestI {
        current: Reg,
        to: u32,
    },
    RangeTestF {
        current: Reg,
        to: u32,
    },
    IInc {
        reg: Reg,
    },
    FInc {
        reg: Reg,
    },
    FUnary {
        f: fn(f64) -> f64,
        dst: Reg,
        src: Reg,
    },
    /// a float function whose result is truncated to an int (`floor` and co)
    FToI {
        f: fn(f64) -> f64,
        dst: Reg,
        src: Reg,
    },
    /// `to_int` of a float
    FTrunc {
        dst: Reg,
        src: Reg,
    },
    IAbs {
        dst: Reg,
        src: Reg,
    },
    ISign {
        dst: Reg,
        src: Reg,
    },
    FAbs {
        dst: Reg,
        src: Reg,
    },
    FSign {
        dst: Reg,
        src: Reg,
    },
    IMod {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FMod {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    IMin {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    IMax {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FMin {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FMax {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FPow {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    FAtan2 {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    /// a call to another specialisation; arguments sit in
    /// `arg_start..arg_start + arg_count` in whichever file their class says,
    /// the result lands in `arg_start` as `ret`
    Call {
        spec: SpecId,
        arg_start: Reg,
        arg_count: u16,
        ret: Class,
    },
    /// the dynamic machine's routine on boxed (or mixed) operands; the result
    /// is stored by `ret`
    DynBin {
        op: BinKind,
        dst: Reg,
        a: Opnd,
        b: Opnd,
        ret: Class,
    },
    DynNeg {
        dst: Reg,
        src: Opnd,
    },
    DynNot {
        dst: Reg,
        src: Opnd,
    },
    DynJumpIf {
        cond: Opnd,
        negate: bool,
        to: u32,
    },
    DynRangeTest {
        current: Opnd,
        stop: Opnd,
        to: u32,
    },
    DynInc {
        reg: Reg,
    },
    EmptyList {
        dst: Reg,
    },
    Append {
        list: Reg,
        value: Opnd,
    },
    Index {
        dst: Reg,
        list: Reg,
        index: Opnd,
    },
    Len {
        dst: Reg,
        list: Reg,
    },
    /// an intrinsic on at least one boxed operand
    DynNative {
        intrinsic: KernelIntrinsic,
        dst: Reg,
        args: [Opnd; 2],
        arity: u8,
        ret: Class,
    },
    Return {
        src: Opnd,
    },
    /// a boxed capture or default of `closure`, for an inlined body
    Capture {
        dst: Reg,
        closure: ClosureId,
        index: u16,
    },
    Default {
        dst: Reg,
        closure: ClosureId,
        index: u16,
    },
    /// an inlined body's return: its value lands in the caller's register as
    /// a box
    BoxInto {
        dst: Reg,
        src: Opnd,
    },
}

/// bodies up to this many ops are spliced into their callers, so a helper
/// called per sample costs no frame and, on the lane machine, keeps every
/// branch in one frame
const INLINE_MAX_OPS: usize = 256;

/// an inlined body may not push the caller past this many ops
const INLINE_MAX_TOTAL: usize = 1 << 12;

/// the ops of one body as they are emitted
#[derive(Default)]
struct Emitted {
    ops: Vec<TOp>,
    /// indices of jumps whose targets are already final (inlined bodies)
    absolute: Vec<usize>,
    /// registers inlined bodies need beyond the caller's own frame
    extent: u16,
}

pub struct Spec {
    pub closure: ClosureId,
    pub kernel: Arc<Kernel>,
    pub sig: Vec<Class>,
    pub ops: Box<[TOp]>,
    pub ret: Class,
    /// classes of the whole entry frame (arguments then captures)
    pub entry: Vec<Class>,
    /// the kernel's frame plus scratch registers for operand conversions
    pub frame_size: u16,
}

/// scratch registers a typed frame has beyond the kernel's own
const SCRATCH: u16 = 2;

/// every specialisation a batch's entry point reaches
#[derive(Default)]
pub struct TypedProgram {
    pub specs: Vec<Spec>,
    index: FxHashMap<(ClosureId, Vec<Class>), SpecId>,
}

impl TypedProgram {
    /// specialise `entry` to calls whose arguments have the classes of
    /// `args`; `None` when any body it reaches cannot be typed
    pub fn specialise(
        arena: &ClosureArena,
        entry: ClosureId,
        args: &[KVal],
    ) -> Option<(Self, SpecId)> {
        let sig: Vec<Class> = args.iter().map(Class::of).collect();
        let mut program = TypedProgram::default();
        let mut in_progress = Vec::new();
        let id = program.spec_for(arena, entry, sig, &mut in_progress)?;
        Some((program, id))
    }

    pub fn spec(&self, id: SpecId) -> &Spec {
        &self.specs[id as usize]
    }

    fn spec_for(
        &mut self,
        arena: &ClosureArena,
        closure: ClosureId,
        sig: Vec<Class>,
        in_progress: &mut Vec<(ClosureId, Vec<Class>)>,
    ) -> Option<SpecId> {
        let key = (closure, sig);
        if let Some(&id) = self.index.get(&key) {
            return Some(id);
        }
        // recursion would need a return class before the body is typed
        if in_progress.contains(&key) {
            return None;
        }
        in_progress.push(key.clone());
        let spec = specialise_body(self, arena, closure, &key.1, in_progress);
        in_progress.pop();
        let spec = spec?;
        let id = self.specs.len() as SpecId;
        self.specs.push(spec);
        self.index.insert(key, id);
        Some(id)
    }
}

/// what one op does to the class state, and the class its result has
struct Step {
    out: Vec<Class>,
    successors: Vec<u32>,
    ret: Option<Class>,
}

fn specialise_body(
    program: &mut TypedProgram,
    arena: &ClosureArena,
    closure: ClosureId,
    sig: &[Class],
    in_progress: &mut Vec<(ClosureId, Vec<Class>)>,
) -> Option<Spec> {
    let kclosure = arena.get(closure);
    let kernel = Arc::clone(&kclosure.kernel);
    let total = kernel.total_args as usize;
    if sig.len() != total {
        return None;
    }
    let frame = kernel.frame_size as usize;
    let mut entry = vec![Class::Unset; frame];
    entry[..total].copy_from_slice(sig);
    for (slot, capture) in entry[total..].iter_mut().zip(kclosure.captures.iter()) {
        *slot = Class::of(capture);
    }

    // in-state per op, to a fixed point over a worklist
    let ops = &kernel.ops;
    let mut states: Vec<Option<Vec<Class>>> = vec![None; ops.len()];
    states[0] = Some(entry.clone());
    let mut worklist = vec![0u32];
    let mut ret = Class::Unset;
    while let Some(pc) = worklist.pop() {
        let state = states[pc as usize].clone()?;
        let step = transfer(program, arena, &ops[pc as usize], pc, state, in_progress)?;
        if let Some(class) = step.ret {
            // returns are materialised as values anyway, so mixed return
            // classes just mean the caller gets a box
            ret = ret.join(class);
        }
        for succ in step.successors {
            let slot = &mut states[succ as usize];
            let merged = match slot {
                None => step.out.clone(),
                Some(existing) => {
                    let merged: Vec<Class> = existing
                        .iter()
                        .zip(&step.out)
                        .map(|(a, b)| a.join(*b))
                        .collect();
                    if merged == *existing {
                        continue;
                    }
                    merged
                }
            };
            *slot = Some(merged);
            worklist.push(succ);
        }
    }

    let scratch = kernel.frame_size;
    let mut emitted = Emitted::default();
    // the entry edge has no op to carry conversions: a register the first
    // op's merge boxes is boxed up front (never the case for a lambda, whose
    // arguments and captures are never written, but cheap to keep true)
    if let Some(first) = &states[0] {
        emitted.ops.extend(boxing_ops(&entry, first));
    }
    let mut starts = Vec::with_capacity(ops.len() + 1);
    // edges whose jump lands where a scalar register has become boxed go
    // through a trampoline that boxes it first
    let mut trampolines: Vec<(usize, Vec<TOp>, u32)> = Vec::new();
    for (pc, op) in ops.iter().enumerate() {
        starts.push(emitted.ops.len() as u32);
        let Some(state) = &states[pc] else {
            // unreachable, but jumps may still name it
            emitted.ops.push(TOp::Nop);
            continue;
        };
        let step = transfer(program, arena, op, pc as u32, state.clone(), in_progress)?;
        emit(
            program,
            arena,
            op,
            state,
            scratch,
            in_progress,
            &mut emitted,
        )?;
        let typed = &mut emitted.ops;
        let last = typed.len() - 1;
        for succ in step.successors {
            let Some(target) = &states[succ as usize] else {
                continue;
            };
            let boxing = boxing_ops(&step.out, target);
            if boxing.is_empty() {
                continue;
            }
            let falls_through =
                succ == pc as u32 + 1 && jump_target(&mut typed[last]).is_none_or(|to| *to != succ);
            if falls_through {
                typed.extend(boxing);
            } else {
                trampolines.push((last, boxing, succ));
            }
        }
    }
    let Emitted {
        ops: mut typed,
        absolute,
        extent,
    } = emitted;
    starts.push(typed.len() as u32);
    for (index, op) in typed.iter_mut().enumerate() {
        if absolute.contains(&index) {
            continue;
        }
        if let Some(to) = jump_target(op) {
            *to = starts[*to as usize];
        }
    }
    for (jump, boxing, target) in trampolines {
        let entry = typed.len() as u32;
        typed.extend(boxing);
        typed.push(TOp::Jump {
            to: starts[target as usize],
        });
        if let Some(to) = jump_target(&mut typed[jump]) {
            *to = entry;
        }
    }
    Some(Spec {
        closure,
        kernel,
        sig: sig.to_vec(),
        ops: typed.into_boxed_slice(),
        ret,
        entry,
        frame_size: (scratch + SCRATCH).max(extent),
    })
}

/// the boxing a path needs on its way into a state where `after` has boxed
/// the registers that are still scalar in `before`
fn boxing_ops(before: &[Class], after: &[Class]) -> Vec<TOp> {
    before
        .iter()
        .zip(after)
        .enumerate()
        .filter(|(_, (_, after))| **after == Class::Boxed)
        .filter_map(|(reg, (before, _))| {
            let reg = reg as Reg;
            match before {
                Class::Int => Some(TOp::BoxI { reg }),
                Class::Float => Some(TOp::BoxF { reg }),
                Class::Closure(id) => Some(TOp::BoxC { reg, id: *id }),
                Class::Boxed | Class::Unset => None,
            }
        })
        .collect()
}

fn jump_target(op: &mut TOp) -> Option<&mut u32> {
    match op {
        TOp::Jump { to }
        | TOp::JumpIfI { to, .. }
        | TOp::JumpIfNotI { to, .. }
        | TOp::JumpIfF { to, .. }
        | TOp::JumpIfNotF { to, .. }
        | TOp::RangeTestI { to, .. }
        | TOp::RangeTestF { to, .. }
        | TOp::DynJumpIf { to, .. }
        | TOp::DynRangeTest { to, .. } => Some(to),
        _ => None,
    }
}

fn read(state: &[Class], reg: Reg) -> Option<Class> {
    match state[reg as usize] {
        Class::Unset => None,
        class => Some(class),
    }
}

fn bin_class(op: BinKind, a: Class, b: Class) -> Class {
    use BinKind::*;
    match (a, b) {
        (Class::Int, Class::Int) => match op {
            Add | Sub | Mul | IntDiv | Lt | Le | Gt | Ge | Eq | Ne => Class::Int,
            Div | Power => Class::Float,
            In => Class::Boxed,
        },
        (Class::Int | Class::Float, Class::Int | Class::Float) => match op {
            Add | Sub | Mul | Div | Power => Class::Float,
            IntDiv | Lt | Le | Gt | Ge | Eq | Ne => Class::Int,
            In => Class::Boxed,
        },
        _ => match op {
            Eq | Ne | In => Class::Int,
            _ => Class::Boxed,
        },
    }
}

fn native_class(intrinsic: KernelIntrinsic, args: &[Class]) -> Class {
    use KernelIntrinsic::*;
    let pair = || match (args[0], args[1]) {
        (Class::Int, Class::Int) => Class::Int,
        (a, b) if a.is_scalar() && b.is_scalar() => Class::Float,
        _ => Class::Boxed,
    };
    match intrinsic {
        Sqrt | Cbrt | Exp | Ln | Sin | Cos | Tan | Asin | Acos | Atan | Sinh | Cosh | Tanh
        | Pow | Atan2 | ToFloat => Class::Float,
        Floor | Ceil | Round | Trunc | Len => Class::Int,
        Abs | Sign | ToInt => match args[0] {
            Class::Int => Class::Int,
            Class::Float if intrinsic == ToInt => Class::Int,
            Class::Float => Class::Float,
            _ => Class::Boxed,
        },
        Mod | Min | Max => pair(),
        Dot => Class::Float,
        Cross | KeyframeLerp | Fallthrough => Class::Boxed,
    }
}

fn transfer(
    program: &mut TypedProgram,
    arena: &ClosureArena,
    op: &KOp,
    pc: u32,
    mut state: Vec<Class>,
    in_progress: &mut Vec<(ClosureId, Vec<Class>)>,
) -> Option<Step> {
    let next = pc + 1;
    let mut successors = vec![next];
    let mut ret = None;
    match *op {
        KOp::Nil { dst } => state[dst as usize] = Class::Boxed,
        KOp::Int { dst, .. } => state[dst as usize] = Class::Int,
        KOp::Float { dst, .. } => state[dst as usize] = Class::Float,
        KOp::Move { dst, src } | KOp::Take { dst, src } => {
            state[dst as usize] = read(&state, src)?
        }
        KOp::Bin { op, dst, a, b, .. } => {
            state[dst as usize] = bin_class(op, read(&state, a)?, read(&state, b)?)
        }
        KOp::Neg { dst, src } => {
            state[dst as usize] = match read(&state, src)? {
                Class::Closure(_) => return None,
                class => class,
            }
        }
        KOp::Not { dst, src } => {
            read(&state, src)?;
            state[dst as usize] = Class::Int;
        }
        KOp::Jump { to } => successors = vec![to],
        KOp::JumpIf { cond, to } | KOp::JumpIfNot { cond, to } => {
            read(&state, cond)?;
            successors.push(to);
        }
        KOp::RangeTest { current, to } => {
            read(&state, current)?;
            read(&state, current + 1)?;
            successors.push(to);
        }
        KOp::Inc { reg } => {
            if matches!(read(&state, reg)?, Class::Closure(_)) {
                return None;
            }
        }
        KOp::EmptyList { dst } => state[dst as usize] = Class::Boxed,
        KOp::Append { list, value } => {
            read(&state, value)?;
            if read(&state, list)? != Class::Boxed {
                return None;
            }
        }
        KOp::Index { dst, list, index } => {
            read(&state, index)?;
            if read(&state, list)? != Class::Boxed {
                return None;
            }
            state[dst as usize] = Class::Boxed;
        }
        KOp::Len { dst, src } => {
            if read(&state, src)? != Class::Boxed {
                return None;
            }
            state[dst as usize] = Class::Int;
        }
        KOp::Call {
            callee,
            arg_start,
            arg_count,
        } => {
            let Class::Closure(id) = read(&state, callee)? else {
                return None;
            };
            let start = arg_start as usize;
            let count = arg_count as usize;
            let sig = call_signature(arena, id, &state[start..start + count])?;
            let spec = program.spec_for(arena, id, sig, in_progress)?;
            let ret = program.spec(spec).ret;
            if ret == Class::Unset {
                return None;
            }
            state[start] = ret;
            for slot in &mut state[start + 1..start + count] {
                *slot = Class::Unset;
            }
        }
        KOp::Native {
            intrinsic,
            arg_start,
            arg_count,
        } => {
            let start = arg_start as usize;
            let count = arg_count as usize;
            let args: Vec<Class> = (start..start + count)
                .map(|reg| read(&state, reg as Reg))
                .collect::<Option<_>>()?;
            if intrinsic == KernelIntrinsic::Fallthrough {
                return None;
            }
            state[start] = native_class(intrinsic, &args);
            for slot in &mut state[start + 1..start + count] {
                *slot = Class::Unset;
            }
        }
        KOp::Return { src } => {
            ret = Some(read(&state, src)?);
            successors.clear();
        }
        KOp::Exit => return None,
    }
    Some(Step {
        out: state,
        successors,
        ret,
    })
}

/// the full argument classes of a call to `callee` with `provided` explicit
/// arguments: defaults fill the rest, as the dynamic machine does
fn call_signature(
    arena: &ClosureArena,
    callee: ClosureId,
    provided: &[Class],
) -> Option<Vec<Class>> {
    let closure = arena.get(callee);
    let total = closure.kernel.total_args as usize;
    let required = closure.kernel.required_args as usize;
    if provided.len() < required || provided.len() > total {
        return None;
    }
    let mut sig = provided.to_vec();
    let default_start = closure
        .defaults
        .len()
        .saturating_sub(total - provided.len());
    sig.extend(closure.defaults[default_start..].iter().map(Class::of));
    (sig.len() == total).then_some(sig)
}

fn opnd(state: &[Class], reg: Reg) -> Option<Opnd> {
    Some(match read(state, reg)? {
        Class::Int => Opnd::I(reg),
        Class::Float => Opnd::F(reg),
        Class::Closure(id) => Opnd::C(id),
        Class::Boxed => Opnd::B(reg),
        Class::Unset => return None,
    })
}

/// a scalar operand as a float register: an int is converted into a scratch
/// register first
fn as_float(state: &[Class], reg: Reg, scratch: Reg, out: &mut Emitted) -> Option<Reg> {
    match read(state, reg)? {
        Class::Float => Some(reg),
        Class::Int => {
            out.ops.push(TOp::IToF {
                dst: scratch,
                src: reg,
            });
            Some(scratch)
        }
        _ => None,
    }
}

fn emit(
    program: &mut TypedProgram,
    arena: &ClosureArena,
    op: &KOp,
    state: &[Class],
    scratch: Reg,
    in_progress: &mut Vec<(ClosureId, Vec<Class>)>,
    out: &mut Emitted,
) -> Option<()> {
    use BinKind::*;
    let typed = match *op {
        KOp::Nil { dst } => TOp::Nil { dst },
        KOp::Int { dst, value } => TOp::IConst { dst, value },
        KOp::Float { dst, value } => TOp::FConst { dst, value },
        KOp::Move { dst, src } | KOp::Take { dst, src } => match read(state, src)? {
            Class::Int | Class::Float => TOp::MoveS { dst, src },
            Class::Boxed => TOp::MoveB { dst, src },
            // the value is static; nothing to copy
            Class::Closure(_) => TOp::Nop,
            Class::Unset => return None,
        },
        KOp::Bin { op, dst, a, b, .. } => {
            let (ca, cb) = (read(state, a)?, read(state, b)?);
            match (ca, cb) {
                (Class::Int, Class::Int) => match op {
                    Add => TOp::IAdd { dst, a, b },
                    Sub => TOp::ISub { dst, a, b },
                    Mul => TOp::IMul { dst, a, b },
                    In => TOp::DynBin {
                        op,
                        dst,
                        a: Opnd::I(a),
                        b: Opnd::I(b),
                        ret: Class::Boxed,
                    },
                    _ => TOp::IBin { op, dst, a, b },
                },
                (Class::Int | Class::Float, Class::Int | Class::Float) if op != In => {
                    let a = as_float(state, a, scratch, out)?;
                    let b = as_float(state, b, scratch + 1, out)?;
                    match op {
                        Add => TOp::FAdd { dst, a, b },
                        Sub => TOp::FSub { dst, a, b },
                        Mul => TOp::FMul { dst, a, b },
                        Div => TOp::FDiv { dst, a, b },
                        _ => TOp::FBin { op, dst, a, b },
                    }
                }
                _ => TOp::DynBin {
                    op,
                    dst,
                    a: opnd(state, a)?,
                    b: opnd(state, b)?,
                    ret: bin_class(op, ca, cb),
                },
            }
        }
        KOp::Neg { dst, src } => match read(state, src)? {
            Class::Int => TOp::INeg { dst, src },
            Class::Float => TOp::FNeg { dst, src },
            _ => TOp::DynNeg {
                dst,
                src: opnd(state, src)?,
            },
        },
        KOp::Not { dst, src } => match read(state, src)? {
            Class::Int => TOp::INot { dst, src },
            Class::Float => TOp::FNot { dst, src },
            _ => TOp::DynNot {
                dst,
                src: opnd(state, src)?,
            },
        },
        KOp::Jump { to } => TOp::Jump { to },
        KOp::JumpIf { cond, to } => match read(state, cond)? {
            Class::Int => TOp::JumpIfI { cond, to },
            Class::Float => TOp::JumpIfF { cond, to },
            _ => TOp::DynJumpIf {
                cond: opnd(state, cond)?,
                negate: false,
                to,
            },
        },
        KOp::JumpIfNot { cond, to } => match read(state, cond)? {
            Class::Int => TOp::JumpIfNotI { cond, to },
            Class::Float => TOp::JumpIfNotF { cond, to },
            _ => TOp::DynJumpIf {
                cond: opnd(state, cond)?,
                negate: true,
                to,
            },
        },
        KOp::RangeTest { current, to } => {
            match (read(state, current)?, read(state, current + 1)?) {
                (Class::Int, Class::Int) => TOp::RangeTestI { current, to },
                (Class::Float, Class::Float) => TOp::RangeTestF { current, to },
                _ => TOp::DynRangeTest {
                    current: opnd(state, current)?,
                    stop: opnd(state, current + 1)?,
                    to,
                },
            }
        }
        KOp::Inc { reg } => match read(state, reg)? {
            Class::Int => TOp::IInc { reg },
            Class::Float => TOp::FInc { reg },
            _ => TOp::DynInc { reg },
        },
        KOp::EmptyList { dst } => TOp::EmptyList { dst },
        KOp::Append { list, value } => TOp::Append {
            list,
            value: opnd(state, value)?,
        },
        KOp::Index { dst, list, index } => TOp::Index {
            dst,
            list,
            index: opnd(state, index)?,
        },
        KOp::Len { dst, src } => TOp::Len { dst, list: src },
        KOp::Call {
            callee,
            arg_start,
            arg_count,
        } => {
            let Class::Closure(id) = read(state, callee)? else {
                return None;
            };
            let start = arg_start as usize;
            let sig = call_signature(arena, id, &state[start..start + arg_count as usize])?;
            let spec = program.spec_for(arena, id, sig, in_progress)?;
            let callee = program.spec(spec);
            if callee.ops.len() <= INLINE_MAX_OPS
                && out.ops.len() + callee.ops.len() <= INLINE_MAX_TOTAL
            {
                inline_call(arena, callee, scratch + SCRATCH, arg_start, arg_count, out);
                return Some(());
            }
            TOp::Call {
                spec,
                arg_start,
                arg_count,
                ret: callee.ret,
            }
        }
        KOp::Native {
            intrinsic,
            arg_start,
            arg_count,
        } => {
            let start = arg_start as usize;
            let args: Vec<Class> = (start..start + arg_count as usize)
                .map(|reg| read(state, reg as Reg))
                .collect::<Option<_>>()?;
            let ret = native_class(intrinsic, &args);
            match typed_native(intrinsic, arg_start, &args, state, scratch, out) {
                Some(typed) => typed,
                None => {
                    if arg_count > 2 {
                        return None;
                    }
                    let mut operands = [Opnd::I(0); 2];
                    for (slot, reg) in operands.iter_mut().zip(start..start + arg_count as usize) {
                        *slot = opnd(state, reg as Reg)?;
                    }
                    TOp::DynNative {
                        intrinsic,
                        dst: arg_start,
                        args: operands,
                        arity: arg_count as u8,
                        ret,
                    }
                }
            }
        }
        KOp::Exit => return None,
        KOp::Return { src } => TOp::Return {
            src: opnd(state, src)?,
        },
    };
    out.ops.push(typed);
    Some(())
}

/// splice `callee` into the caller at register `offset`: arguments and
/// captures are copied in, every register and jump of the body is shifted,
/// and each return stores into the caller's `arg_start` and jumps past the
/// body
fn inline_call(
    arena: &ClosureArena,
    callee: &Spec,
    offset: Reg,
    arg_start: Reg,
    arg_count: u16,
    out: &mut Emitted,
) {
    let closure = arena.get(callee.closure);
    let total = callee.sig.len();
    let provided = arg_count as usize;
    for i in 0..provided {
        let (dst, src) = (offset + i as Reg, arg_start + i as Reg);
        match callee.sig[i] {
            Class::Int | Class::Float => out.ops.push(TOp::MoveS { dst, src }),
            Class::Boxed => out.ops.push(TOp::MoveB { dst, src }),
            Class::Closure(_) | Class::Unset => {}
        }
    }
    let default_start = closure.defaults.len().saturating_sub(total - provided);
    for (i, default) in (provided..total).zip(&closure.defaults[default_start..]) {
        let dst = offset + i as Reg;
        out.ops.push(match default {
            KVal::Int(value) => TOp::IConst { dst, value: *value },
            KVal::Float(value) => TOp::FConst { dst, value: *value },
            KVal::Closure(_) => TOp::Nop,
            _ => TOp::Default {
                dst,
                closure: callee.closure,
                index: (default_start + i - provided) as u16,
            },
        });
    }
    for (i, capture) in closure.captures.iter().enumerate() {
        let dst = offset + (total + i) as Reg;
        out.ops.push(match capture {
            KVal::Int(value) => TOp::IConst { dst, value: *value },
            KVal::Float(value) => TOp::FConst { dst, value: *value },
            KVal::Closure(_) => TOp::Nop,
            _ => TOp::Capture {
                dst,
                closure: callee.closure,
                index: i as u16,
            },
        });
    }

    // a return becomes two ops, so callee indices are mapped rather than
    // shifted; jumps are patched once every position is known
    let body_start = out.ops.len();
    let mut positions = Vec::with_capacity(callee.ops.len() + 1);
    let mut returns = Vec::new();
    for op in callee.ops.iter() {
        positions.push(out.ops.len() as u32);
        let mut op = *op;
        shift(&mut op, offset);
        if let TOp::Return { src } = op {
            let store = match (callee.ret, src) {
                (Class::Int | Class::Float, Opnd::I(reg) | Opnd::F(reg)) => TOp::MoveS {
                    dst: arg_start,
                    src: reg,
                },
                (Class::Closure(_), _) => TOp::Nop,
                (_, src) => TOp::BoxInto {
                    dst: arg_start,
                    src,
                },
            };
            out.ops.push(store);
            returns.push(out.ops.len());
            out.ops.push(TOp::Jump { to: u32::MAX });
        } else {
            if jump_target(&mut op).is_some() {
                out.absolute.push(out.ops.len());
            }
            out.ops.push(op);
        }
    }
    let end = out.ops.len() as u32;
    positions.push(end);
    for index in returns {
        out.ops[index] = TOp::Jump { to: end };
        out.absolute.push(index);
    }
    for op in &mut out.ops[body_start..] {
        if let Some(to) = jump_target(op)
            && *to != end
        {
            *to = positions[*to as usize];
        }
    }
    out.extent = out.extent.max(offset + callee.frame_size);
}

/// move every register of `op` up by `offset`; jump targets stay callee
/// indices for the caller to map
fn shift(op: &mut TOp, offset: Reg) {
    let shift_opnd = |opnd: &mut Opnd| match opnd {
        Opnd::I(reg) | Opnd::F(reg) | Opnd::B(reg) => *reg += offset,
        Opnd::C(_) => {}
    };
    match op {
        TOp::Nop => {}
        TOp::Nil { dst }
        | TOp::IConst { dst, .. }
        | TOp::FConst { dst, .. }
        | TOp::EmptyList { dst }
        | TOp::Capture { dst, .. }
        | TOp::Default { dst, .. } => *dst += offset,
        TOp::BoxI { reg } | TOp::BoxF { reg } | TOp::BoxC { reg, .. } => *reg += offset,
        TOp::MoveS { dst, src }
        | TOp::MoveB { dst, src }
        | TOp::IToF { dst, src }
        | TOp::INeg { dst, src }
        | TOp::FNeg { dst, src }
        | TOp::INot { dst, src }
        | TOp::FNot { dst, src }
        | TOp::FUnary { dst, src, .. }
        | TOp::FToI { dst, src, .. }
        | TOp::FTrunc { dst, src }
        | TOp::IAbs { dst, src }
        | TOp::ISign { dst, src }
        | TOp::FAbs { dst, src }
        | TOp::FSign { dst, src } => {
            *dst += offset;
            *src += offset;
        }
        TOp::IAdd { dst, a, b }
        | TOp::ISub { dst, a, b }
        | TOp::IMul { dst, a, b }
        | TOp::IBin { dst, a, b, .. }
        | TOp::FAdd { dst, a, b }
        | TOp::FSub { dst, a, b }
        | TOp::FMul { dst, a, b }
        | TOp::FDiv { dst, a, b }
        | TOp::FBin { dst, a, b, .. }
        | TOp::IMod { dst, a, b }
        | TOp::FMod { dst, a, b }
        | TOp::IMin { dst, a, b }
        | TOp::IMax { dst, a, b }
        | TOp::FMin { dst, a, b }
        | TOp::FMax { dst, a, b }
        | TOp::FPow { dst, a, b }
        | TOp::FAtan2 { dst, a, b } => {
            *dst += offset;
            *a += offset;
            *b += offset;
        }
        TOp::Jump { .. } => {}
        TOp::JumpIfI { cond, .. }
        | TOp::JumpIfNotI { cond, .. }
        | TOp::JumpIfF { cond, .. }
        | TOp::JumpIfNotF { cond, .. } => *cond += offset,
        TOp::RangeTestI { current, .. } | TOp::RangeTestF { current, .. } => *current += offset,
        TOp::IInc { reg } | TOp::FInc { reg } | TOp::DynInc { reg } => *reg += offset,
        TOp::Call { arg_start, .. } => *arg_start += offset,
        TOp::DynBin { dst, a, b, .. } => {
            *dst += offset;
            shift_opnd(a);
            shift_opnd(b);
        }
        TOp::DynNeg { dst, src } | TOp::DynNot { dst, src } | TOp::BoxInto { dst, src } => {
            *dst += offset;
            shift_opnd(src);
        }
        TOp::DynJumpIf { cond, .. } => shift_opnd(cond),
        TOp::DynRangeTest { current, stop, .. } => {
            shift_opnd(current);
            shift_opnd(stop);
        }
        TOp::Append { list, value } => {
            *list += offset;
            shift_opnd(value);
        }
        TOp::Index { dst, list, index } => {
            *dst += offset;
            *list += offset;
            shift_opnd(index);
        }
        TOp::Len { dst, list } => {
            *dst += offset;
            *list += offset;
        }
        TOp::DynNative { dst, args, .. } => {
            *dst += offset;
            args.iter_mut().for_each(shift_opnd);
        }
        TOp::Return { src } => shift_opnd(src),
    }
}

/// the scalar form of an intrinsic on scalar arguments, when there is one;
/// int arguments to float functions are converted into scratch registers
fn typed_native(
    intrinsic: KernelIntrinsic,
    start: Reg,
    args: &[Class],
    state: &[Class],
    scratch: Reg,
    out: &mut Emitted,
) -> Option<TOp> {
    use KernelIntrinsic::*;
    if !args.iter().all(|class| class.is_scalar()) {
        return None;
    }
    let dst = start;
    let src = start;
    let both_int = args.len() == 2 && args[0] == Class::Int && args[1] == Class::Int;
    let float_unary = |f: fn(f64) -> f64, out: &mut Emitted| {
        let src = as_float(state, src, scratch, out)?;
        Some(TOp::FUnary { f, dst, src })
    };
    let float_to_int = |f: fn(f64) -> f64, out: &mut Emitted| {
        let src = as_float(state, src, scratch, out)?;
        Some(TOp::FToI { f, dst, src })
    };
    let float_pair = |out: &mut Emitted| {
        let a = as_float(state, start, scratch, out)?;
        let b = as_float(state, start + 1, scratch + 1, out)?;
        Some((a, b))
    };
    let (a, b) = (start, start + 1);
    Some(match intrinsic {
        Sqrt => float_unary(f64::sqrt, out)?,
        Cbrt => float_unary(f64::cbrt, out)?,
        Exp => float_unary(f64::exp, out)?,
        Ln => float_unary(f64::ln, out)?,
        Sin => float_unary(f64::sin, out)?,
        Cos => float_unary(f64::cos, out)?,
        Tan => float_unary(f64::tan, out)?,
        Asin => float_unary(f64::asin, out)?,
        Acos => float_unary(f64::acos, out)?,
        Atan => float_unary(f64::atan, out)?,
        Sinh => float_unary(f64::sinh, out)?,
        Cosh => float_unary(f64::cosh, out)?,
        Tanh => float_unary(f64::tanh, out)?,
        Floor => float_to_int(f64::floor, out)?,
        Ceil => float_to_int(f64::ceil, out)?,
        Round => float_to_int(f64::round, out)?,
        Trunc => float_to_int(f64::trunc, out)?,
        Pow => {
            let (a, b) = float_pair(out)?;
            TOp::FPow { dst, a, b }
        }
        Atan2 => {
            let (a, b) = float_pair(out)?;
            TOp::FAtan2 { dst, a, b }
        }
        Abs if args[0] == Class::Int => TOp::IAbs { dst, src },
        Abs => TOp::FAbs { dst, src },
        Sign if args[0] == Class::Int => TOp::ISign { dst, src },
        Sign => TOp::FSign { dst, src },
        Mod if both_int => TOp::IMod { dst, a, b },
        Mod => {
            let (a, b) = float_pair(out)?;
            TOp::FMod { dst, a, b }
        }
        Min if both_int => TOp::IMin { dst, a, b },
        Min => {
            let (a, b) = float_pair(out)?;
            TOp::FMin { dst, a, b }
        }
        Max if both_int => TOp::IMax { dst, a, b },
        Max => {
            let (a, b) = float_pair(out)?;
            TOp::FMax { dst, a, b }
        }
        ToInt if args[0] == Class::Int => TOp::MoveS { dst, src },
        ToInt => TOp::FTrunc { dst, src },
        ToFloat if args[0] == Class::Int => TOp::IToF { dst, src },
        ToFloat => TOp::MoveS { dst, src },
        Dot | Cross | Len | KeyframeLerp | Fallthrough => return None,
    })
}
