//! native code for typed kernels: a specialisation whose registers are all
//! ints and floats is compiled with cranelift into one function over the same
//! word frame the typed machine uses, so a scalar op costs an instruction
//! rather than a dispatch.
//!
//! only what is one instruction with the typed machine's exact semantics is
//! inlined (wrapping int arithmetic, ieee float arithmetic, comparisons,
//! conversions, jumps); everything with a rule of its own (floor division,
//! modulo, powers, min/max, sign, the transcendental functions) calls an
//! `extern "C"` shim running the same rust expression or the same `run.rs`
//! helper the typed machine runs, so results and faults cannot diverge.
//! the one boxed shape it models is a list of scalars built and returned by
//! the body (a colour, a point), which lives in the frame. a spec with a boxed
//! argument or capture, any other boxed op, or a call the specialiser did not
//! inline is declined and stays with the typed and lane machines.
//!
//! compiled code is cached by the spec's shape: its entry classes and ops with
//! every float constant replaced by a slot of the frame. the code is a pure
//! function of that key, so a bytecode change can never reuse stale code, and
//! a body whose only change is a float literal or an inlined float capture (a
//! helper capturing the scene's time, say) reuses the code of the last frame

use std::{
    sync::{Arc, OnceLock, atomic::AtomicBool},
    time::Instant,
};

use cranelift_codegen::{
    ir::{
        AbiParam, Block, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, UserFuncName,
        Value, condcodes::FloatCC, condcodes::IntCC, types,
    },
    isa::OwnedTargetIsa,
    settings::{self, Configurable},
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};
use rustc_hash::FxHashMap;

use super::{
    KernelStats,
    ir::{BinKind, Reg},
    run::{self, CALL_OP_BUDGET, Fault},
    typed::{Class, Opnd, Spec, TOp},
    value::{ClosureArena, KVal},
};

type Entry = unsafe extern "C" fn(*mut u64, *mut i64, *const u8) -> i32;

/// compiled bodies kept at once; the cache is cleared wholesale past this
const CACHE_MAX: usize = 256;

/// elements a returned list may have; a longer one faults and the call runs
/// on the typed machine
const LIST_CAP: usize = 16;

/// lists a body may build: one per `EmptyList` op, none of them in a loop
const LIST_SITES: usize = 4;

/// where things live in the frame, in words: the registers, then the result,
/// its kind, the returned list's length, each list's elements and element
/// kinds, and the float constants
#[derive(Clone, Copy)]
struct Layout {
    result: usize,
    kind: usize,
    count: usize,
    items: usize,
    consts: usize,
}

impl Layout {
    fn of(frame_size: u16) -> Self {
        let result = frame_size as usize;
        Self {
            result,
            kind: result + 1,
            count: result + 2,
            items: result + 3,
            consts: result + 3 + 2 * LIST_CAP * LIST_SITES,
        }
    }

    /// the first element of list `site`, relative to `items`
    fn site(site: u8) -> usize {
        site as usize * 2 * LIST_CAP
    }
}

const KIND_INT: i64 = 0;
const KIND_FLOAT: i64 = 1;
/// plus the list's site
const KIND_LIST: i64 = 2;

fn fault_code(fault: Fault) -> i32 {
    match fault {
        Fault::Type => 1,
        Fault::DivisionByZero => 2,
        Fault::Index => 3,
        Fault::Arity => 4,
        Fault::Depth => 5,
        Fault::Budget => 6,
        Fault::Aborted => 7,
        Fault::LengthMismatch => 8,
    }
}

fn fault_of(code: i32) -> Fault {
    match code {
        2 => Fault::DivisionByZero,
        3 => Fault::Index,
        4 => Fault::Arity,
        5 => Fault::Depth,
        6 => Fault::Budget,
        7 => Fault::Aborted,
        8 => Fault::LengthMismatch,
        _ => Fault::Type,
    }
}

const BIN_KINDS: [BinKind; 13] = [
    BinKind::Add,
    BinKind::Sub,
    BinKind::Mul,
    BinKind::Div,
    BinKind::Power,
    BinKind::Lt,
    BinKind::Le,
    BinKind::Gt,
    BinKind::Ge,
    BinKind::Eq,
    BinKind::Ne,
    BinKind::IntDiv,
    BinKind::In,
];

fn bin_code(op: BinKind) -> u64 {
    BIN_KINDS.iter().position(|kind| *kind == op).unwrap() as u64
}

/// the word a scalar result is stored as, the way the typed machine stores it
fn word(value: KVal) -> u64 {
    match value {
        KVal::Int(n) => n as u64,
        KVal::Float(f) => f.to_bits(),
        _ => 0,
    }
}

fn written(out: *mut u64, result: Result<KVal, Fault>) -> i32 {
    match result {
        Ok(value) => {
            // safety: `out` is the generated code's own stack slot
            unsafe { *out = word(value) };
            0
        }
        Err(fault) => fault_code(fault),
    }
}

// the shims: each runs exactly what `typed_run.rs` runs for the op

extern "C" fn shim_int_binary(op: u32, a: i64, b: i64, out: *mut u64) -> i32 {
    written(out, run::int_binary(BIN_KINDS[op as usize], a, b))
}

extern "C" fn shim_float_binary(op: u32, a: f64, b: f64, out: *mut u64) -> i32 {
    written(out, run::float_binary(BIN_KINDS[op as usize], a, b))
}

extern "C" fn shim_unary(f: usize, x: f64) -> f64 {
    // safety: `f` is the `fn(f64) -> f64` of an `FUnary` / `FToI` op, turned
    // into an address when the code was generated
    let f: fn(f64) -> f64 = unsafe { std::mem::transmute(f) };
    f(x)
}

extern "C" fn shim_float_sign(x: f64) -> f64 {
    run::float_sign(x)
}

extern "C" fn shim_int_mod(a: i64, b: i64, out: *mut u64) -> i32 {
    written(
        out,
        if b == 0 {
            Err(Fault::DivisionByZero)
        } else {
            Ok(KVal::Int(a.wrapping_rem_euclid(b)))
        },
    )
}

extern "C" fn shim_float_mod(a: f64, b: f64, out: *mut u64) -> i32 {
    written(
        out,
        if b == 0.0 {
            Err(Fault::DivisionByZero)
        } else {
            Ok(KVal::Float(a.rem_euclid(b)))
        },
    )
}

extern "C" fn shim_float_min(a: f64, b: f64) -> f64 {
    a.min(b)
}

extern "C" fn shim_float_max(a: f64, b: f64) -> f64 {
    a.max(b)
}

extern "C" fn shim_float_pow(a: f64, b: f64) -> f64 {
    a.powf(b)
}

extern "C" fn shim_float_atan2(a: f64, b: f64) -> f64 {
    a.atan2(b)
}

#[derive(Clone, Copy)]
enum Shim {
    IntBinary,
    FloatBinary,
    Unary,
    FloatSign,
    IntMod,
    FloatMod,
    FloatMin,
    FloatMax,
    FloatPow,
    FloatAtan2,
}

impl Shim {
    const ALL: [Shim; 10] = [
        Shim::IntBinary,
        Shim::FloatBinary,
        Shim::Unary,
        Shim::FloatSign,
        Shim::IntMod,
        Shim::FloatMod,
        Shim::FloatMin,
        Shim::FloatMax,
        Shim::FloatPow,
        Shim::FloatAtan2,
    ];

    fn name(self) -> &'static str {
        match self {
            Shim::IntBinary => "mc_int_binary",
            Shim::FloatBinary => "mc_float_binary",
            Shim::Unary => "mc_unary",
            Shim::FloatSign => "mc_float_sign",
            Shim::IntMod => "mc_int_mod",
            Shim::FloatMod => "mc_float_mod",
            Shim::FloatMin => "mc_float_min",
            Shim::FloatMax => "mc_float_max",
            Shim::FloatPow => "mc_float_pow",
            Shim::FloatAtan2 => "mc_float_atan2",
        }
    }

    fn address(self) -> *const u8 {
        match self {
            Shim::IntBinary => shim_int_binary as *const u8,
            Shim::FloatBinary => shim_float_binary as *const u8,
            Shim::Unary => shim_unary as *const u8,
            Shim::FloatSign => shim_float_sign as *const u8,
            Shim::IntMod => shim_int_mod as *const u8,
            Shim::FloatMod => shim_float_mod as *const u8,
            Shim::FloatMin => shim_float_min as *const u8,
            Shim::FloatMax => shim_float_max as *const u8,
            Shim::FloatPow => shim_float_pow as *const u8,
            Shim::FloatAtan2 => shim_float_atan2 as *const u8,
        }
    }

    /// parameter and result types; a shim that can fault takes an out
    /// pointer last and returns a fault code
    fn signature(self, ptr: types::Type) -> (Vec<types::Type>, types::Type) {
        use types::{F64, I32, I64};
        match self {
            Shim::IntBinary => (vec![I32, I64, I64, ptr], I32),
            Shim::FloatBinary => (vec![I32, F64, F64, ptr], I32),
            Shim::Unary => (vec![ptr, F64], F64),
            Shim::FloatSign => (vec![F64], F64),
            Shim::IntMod => (vec![I64, I64, ptr], I32),
            Shim::FloatMod => (vec![F64, F64, ptr], I32),
            Shim::FloatMin | Shim::FloatMax | Shim::FloatPow | Shim::FloatAtan2 => {
                (vec![F64, F64], F64)
            }
        }
    }
}

/// a spec's shape: the cache key and the float constants its frame carries
struct Shape {
    key: Box<[u64]>,
    consts: Vec<u64>,
}

fn class_code(class: Class) -> Option<u64> {
    match class {
        Class::Unset | Class::Closure(_) => Some(0),
        Class::Int => Some(1),
        Class::Float => Some(2),
        Class::Boxed => None,
    }
}

fn opnd_code(opnd: Opnd) -> Option<u64> {
    match opnd {
        Opnd::I(reg) => Some(reg as u64),
        Opnd::F(reg) => Some(1 << 16 | reg as u64),
        Opnd::B(reg) => Some(2 << 16 | reg as u64),
        Opnd::C(_) => None,
    }
}

/// the shape of `spec`, or `None` when it has an op the jit does not compile.
/// int constants are part of the code (they are loop bounds and small
/// literals, and fold into the instructions using them); float constants are
/// read from the frame, so a changed literal or captured time reuses the code
fn shape(spec: &Spec) -> Option<Shape> {
    let mut key = Vec::with_capacity(spec.ops.len() * 4 + spec.entry.len() + 2);
    let mut consts = Vec::new();
    key.push(spec.frame_size as u64);
    key.push(spec.entry.len() as u64);
    for class in &spec.entry {
        key.push(class_code(*class)?);
    }
    macro_rules! enc {
        ($tag:expr $(, $field:expr)*) => {
            key.extend([$tag as u64 $(, $field as u64)*])
        };
    }
    for op in spec.ops.iter() {
        match *op {
            TOp::Nop => enc!(0),
            TOp::IConst { dst, value } => enc!(1, dst, value),
            TOp::FConst { dst, value } => {
                enc!(2, dst);
                consts.push(value.to_bits());
            }
            TOp::MoveS { dst, src } => enc!(3, dst, src),
            TOp::IToF { dst, src } => enc!(4, dst, src),
            TOp::IAdd { dst, a, b } => enc!(5, dst, a, b),
            TOp::ISub { dst, a, b } => enc!(6, dst, a, b),
            TOp::IMul { dst, a, b } => enc!(7, dst, a, b),
            TOp::IBin { op, dst, a, b } => enc!(8, bin_code(op), dst, a, b),
            TOp::FAdd { dst, a, b } => enc!(9, dst, a, b),
            TOp::FSub { dst, a, b } => enc!(10, dst, a, b),
            TOp::FMul { dst, a, b } => enc!(11, dst, a, b),
            TOp::FDiv { dst, a, b } => enc!(12, dst, a, b),
            TOp::FBin { op, dst, a, b } => enc!(13, bin_code(op), dst, a, b),
            TOp::INeg { dst, src } => enc!(14, dst, src),
            TOp::FNeg { dst, src } => enc!(15, dst, src),
            TOp::INot { dst, src } => enc!(16, dst, src),
            TOp::FNot { dst, src } => enc!(17, dst, src),
            TOp::Jump { to } => enc!(18, to),
            TOp::JumpIfI { cond, to } => enc!(19, cond, to),
            TOp::JumpIfNotI { cond, to } => enc!(20, cond, to),
            TOp::JumpIfF { cond, to } => enc!(21, cond, to),
            TOp::JumpIfNotF { cond, to } => enc!(22, cond, to),
            TOp::RangeTestI { current, to } => enc!(23, current, to),
            TOp::RangeTestF { current, to } => enc!(24, current, to),
            TOp::IInc { reg } => enc!(25, reg),
            TOp::FInc { reg } => enc!(26, reg),
            TOp::FUnary { f, dst, src } => enc!(27, f as usize, dst, src),
            TOp::FToI { f, dst, src } => enc!(28, f as usize, dst, src),
            TOp::FTrunc { dst, src } => enc!(29, dst, src),
            TOp::IAbs { dst, src } => enc!(30, dst, src),
            TOp::ISign { dst, src } => enc!(31, dst, src),
            TOp::FAbs { dst, src } => enc!(32, dst, src),
            TOp::FSign { dst, src } => enc!(33, dst, src),
            TOp::IMod { dst, a, b } => enc!(34, dst, a, b),
            TOp::FMod { dst, a, b } => enc!(35, dst, a, b),
            TOp::IMin { dst, a, b } => enc!(36, dst, a, b),
            TOp::IMax { dst, a, b } => enc!(37, dst, a, b),
            TOp::FMin { dst, a, b } => enc!(38, dst, a, b),
            TOp::FMax { dst, a, b } => enc!(39, dst, a, b),
            TOp::FPow { dst, a, b } => enc!(40, dst, a, b),
            TOp::FAtan2 { dst, a, b } => enc!(41, dst, a, b),
            TOp::Return { src } => enc!(42, opnd_code(src)?),
            TOp::BoxI { reg } | TOp::BoxF { reg } | TOp::BoxC { reg, .. } => enc!(43, reg),
            TOp::Nil { dst } => enc!(44, dst),
            TOp::EmptyList { dst } => enc!(45, dst),
            TOp::Append { list, value } => match value {
                Opnd::I(_) | Opnd::F(_) => enc!(46, list, opnd_code(value)?),
                Opnd::B(_) | Opnd::C(_) => return None,
            },
            TOp::MoveB { dst, src } => enc!(47, dst, src),
            TOp::Call { .. }
            | TOp::DynBin { .. }
            | TOp::DynNeg { .. }
            | TOp::DynNot { .. }
            | TOp::DynJumpIf { .. }
            | TOp::DynRangeTest { .. }
            | TOp::DynInc { .. }
            | TOp::Index { .. }
            | TOp::Len { .. }
            | TOp::DynNative { .. }
            | TOp::Capture { .. }
            | TOp::Default { .. }
            | TOp::BoxInto { .. } => return None,
        }
    }
    Some(Shape {
        key: key.into_boxed_slice(),
        consts,
    })
}

/// what a register of the word file holds at one point, for the
/// class-agnostic `MoveS`
#[derive(Clone, Copy, PartialEq, Eq)]
enum Word {
    Nothing,
    Int,
    Float,
    Mixed,
}

/// what a register of the boxed file holds at one point. the only boxed values
/// the jit models are lists of scalars a body builds and returns, each living
/// in the frame; scalars boxed at a merge are dead in a body with no boxed
/// ops, so boxing them is a no-op as long as nothing reads them back
#[derive(Clone, Copy, PartialEq, Eq)]
enum Boxed {
    Nothing,
    /// the list built at a site, still growing
    List(u8),
    /// a list after a copy of it was taken: appending would have to copy it,
    /// so it may only be returned
    Shared(u8),
    /// anything else, which nothing may read
    Opaque,
}

macro_rules! join_impl {
    ($ty:ident, $bottom:ident, $top:ident) => {
        impl $ty {
            fn join(self, other: $ty) -> $ty {
                match (self, other) {
                    ($ty::$bottom, x) | (x, $ty::$bottom) => x,
                    (a, b) if a == b => a,
                    _ => $ty::$top,
                }
            }
        }
    };
}
join_impl!(Word, Nothing, Mixed);
join_impl!(Boxed, Nothing, Opaque);

/// the register an op writes in the word file and what it writes there
fn writes(op: &TOp, words: &[Word]) -> Option<(Reg, Word)> {
    use BinKind::*;
    Some(match *op {
        TOp::IConst { dst, .. }
        | TOp::IAdd { dst, .. }
        | TOp::ISub { dst, .. }
        | TOp::IMul { dst, .. }
        | TOp::INeg { dst, .. }
        | TOp::INot { dst, .. }
        | TOp::FNot { dst, .. }
        | TOp::FToI { dst, .. }
        | TOp::FTrunc { dst, .. }
        | TOp::IAbs { dst, .. }
        | TOp::ISign { dst, .. }
        | TOp::IMod { dst, .. }
        | TOp::IMin { dst, .. }
        | TOp::IMax { dst, .. } => (dst, Word::Int),
        TOp::IInc { reg } => (reg, Word::Int),
        TOp::FConst { dst, .. }
        | TOp::IToF { dst, .. }
        | TOp::FAdd { dst, .. }
        | TOp::FSub { dst, .. }
        | TOp::FMul { dst, .. }
        | TOp::FDiv { dst, .. }
        | TOp::FNeg { dst, .. }
        | TOp::FUnary { dst, .. }
        | TOp::FAbs { dst, .. }
        | TOp::FSign { dst, .. }
        | TOp::FMod { dst, .. }
        | TOp::FMin { dst, .. }
        | TOp::FMax { dst, .. }
        | TOp::FPow { dst, .. }
        | TOp::FAtan2 { dst, .. } => (dst, Word::Float),
        TOp::FInc { reg } => (reg, Word::Float),
        TOp::IBin { op, dst, .. } => match op {
            Div | Power => (dst, Word::Float),
            _ => (dst, Word::Int),
        },
        TOp::FBin { op, dst, .. } => match op {
            Add | Sub | Mul | Div | Power => (dst, Word::Float),
            _ => (dst, Word::Int),
        },
        TOp::MoveS { dst, src } => (dst, *words.get(src as usize)?),
        _ => return None,
    })
}

fn successors(op: &TOp, pc: u32) -> impl Iterator<Item = u32> {
    let (fall, to) = match *op {
        TOp::Jump { to } => (None, Some(to)),
        TOp::JumpIfI { to, .. }
        | TOp::JumpIfNotI { to, .. }
        | TOp::JumpIfF { to, .. }
        | TOp::JumpIfNotF { to, .. }
        | TOp::RangeTestI { to, .. }
        | TOp::RangeTestF { to, .. } => (Some(pc + 1), Some(to)),
        TOp::Return { .. }
        | TOp::IBin {
            op: BinKind::In, ..
        }
        | TOp::FBin {
            op: BinKind::In, ..
        } => (None, None),
        _ => (Some(pc + 1), None),
    };
    fall.into_iter().chain(to)
}

#[derive(Clone, PartialEq)]
struct State {
    words: Vec<Word>,
    boxed: Vec<Boxed>,
}

impl State {
    /// join `other` in; whether anything changed
    fn absorb(&mut self, other: &State) -> bool {
        let mut changed = false;
        for (old, new) in self.words.iter_mut().zip(&other.words) {
            let joined = old.join(*new);
            changed |= joined != *old;
            *old = joined;
        }
        for (old, new) in self.boxed.iter_mut().zip(&other.boxed) {
            let joined = old.join(*new);
            changed |= joined != *old;
            *old = joined;
        }
        changed
    }
}

/// the effect of `op` on the boxed file, and the list site it touches;
/// `None` when it reads a boxed value the jit does not model
fn step_boxed(op: &TOp, pc: usize, sites: &[usize], boxed: &mut [Boxed]) -> Option<Option<u8>> {
    let slot = |reg: Reg| reg as usize;
    let list = |boxed: &[Boxed], reg: Reg| match boxed.get(slot(reg))? {
        Boxed::List(site) | Boxed::Shared(site) => Some(*site),
        _ => None,
    };
    Some(match *op {
        TOp::BoxI { reg } | TOp::BoxF { reg } | TOp::BoxC { reg, .. } => {
            *boxed.get_mut(slot(reg))? = Boxed::Opaque;
            None
        }
        TOp::Nil { dst } => {
            *boxed.get_mut(slot(dst))? = Boxed::Opaque;
            None
        }
        TOp::EmptyList { dst } => {
            let site = sites.iter().position(|&at| at == pc)? as u8;
            *boxed.get_mut(slot(dst))? = Boxed::List(site);
            Some(site)
        }
        TOp::Append { list, .. } => match boxed.get(slot(list))? {
            Boxed::List(site) => Some(*site),
            _ => return None,
        },
        TOp::MoveB { dst, src } => {
            let site = list(boxed, src)?;
            // both now name the list: neither may grow it
            for reg in boxed.iter_mut() {
                if *reg == Boxed::List(site) {
                    *reg = Boxed::Shared(site);
                }
            }
            *boxed.get_mut(slot(dst))? = Boxed::Shared(site);
            Some(site)
        }
        TOp::Return { src: Opnd::B(reg) } => Some(list(boxed, reg)?),
        _ => None,
    })
}

/// what codegen needs to know about each op beyond the op itself
struct Facts {
    /// for a `MoveS`, whether it copies a float
    float_moves: Vec<bool>,
    /// for an op on a list, the list's site
    sites: Vec<u8>,
}

/// the facts codegen needs, checked along with every boxed read. `None` when
/// a copy's source is not one class on every path, when a boxed value other
/// than a list of scalars is read, or when a list could be built twice in one
/// call (one list per site and call is what lets it live in the frame)
fn analyse(spec: &Spec) -> Option<Facts> {
    let ops = &spec.ops;
    let regs = spec.frame_size as usize;
    let mut entry = State {
        words: vec![Word::Nothing; regs],
        boxed: vec![Boxed::Nothing; regs],
    };
    for (slot, class) in entry.words.iter_mut().zip(&spec.entry) {
        *slot = match class {
            Class::Int => Word::Int,
            Class::Float => Word::Float,
            _ => Word::Nothing,
        };
    }
    let lists: Vec<usize> = (0..ops.len())
        .filter(|&pc| matches!(ops[pc], TOp::EmptyList { .. }))
        .collect();
    if lists.len() > LIST_SITES || lists.iter().any(|&pc| reaches_itself(ops, pc as u32)) {
        return None;
    }

    let mut states: Vec<Option<State>> = vec![None; ops.len()];
    let mut worklist = Vec::new();
    if !ops.is_empty() {
        states[0] = Some(entry);
        worklist.push(0u32);
    }
    while let Some(pc) = worklist.pop() {
        let mut state = states[pc as usize].clone()?;
        let op = &ops[pc as usize];
        if let Some((reg, word)) = writes(op, &state.words) {
            *state.words.get_mut(reg as usize)? = word;
        }
        step_boxed(op, pc as usize, &lists, &mut state.boxed)?;
        for succ in successors(op, pc) {
            let Some(slot) = states.get_mut(succ as usize) else {
                continue;
            };
            match slot {
                None => *slot = Some(state.clone()),
                Some(existing) => {
                    if !existing.absorb(&state) {
                        continue;
                    }
                }
            }
            worklist.push(succ);
        }
    }
    let mut facts = Facts {
        float_moves: vec![false; ops.len()],
        sites: vec![0; ops.len()],
    };
    for (pc, op) in ops.iter().enumerate() {
        let Some(state) = &states[pc] else {
            continue;
        };
        if let TOp::MoveS { src, .. } = op {
            facts.float_moves[pc] = match state.words.get(*src as usize)? {
                Word::Int => false,
                Word::Float => true,
                Word::Nothing | Word::Mixed => return None,
            };
        }
        let mut boxed = state.boxed.clone();
        if let Some(site) = step_boxed(op, pc, &lists, &mut boxed)? {
            facts.sites[pc] = site;
        }
    }
    Some(facts)
}

/// whether control can come back to `start` after leaving it
fn reaches_itself(ops: &[TOp], start: u32) -> bool {
    let mut seen = vec![false; ops.len()];
    let mut stack: Vec<u32> = successors(&ops[start as usize], start).collect();
    while let Some(pc) = stack.pop() {
        if pc == start {
            return true;
        }
        let Some(op) = ops.get(pc as usize) else {
            continue;
        };
        if std::mem::replace(&mut seen[pc as usize], true) {
            continue;
        }
        stack.extend(successors(op, pc));
    }
    false
}

/// a compiled body; the module owns its code and is freed with it
pub struct Compiled {
    module: Option<JITModule>,
    entry: Entry,
}

// safety: the module is only touched again to free it, when the last handle
// is dropped; the code itself is immutable
unsafe impl Send for Compiled {}
unsafe impl Sync for Compiled {}

impl Drop for Compiled {
    fn drop(&mut self) {
        if let Some(module) = self.module.take() {
            // safety: every caller holds an `Arc` to this, so none is running
            unsafe { module.free_memory() };
        }
    }
}

fn host_isa() -> Option<OwnedTargetIsa> {
    static ISA: OnceLock<Option<OwnedTargetIsa>> = OnceLock::new();
    ISA.get_or_init(|| {
        let mut flags = settings::builder();
        flags.set("use_colocated_libcalls", "false").ok()?;
        flags.set("is_pic", "false").ok()?;
        flags.set("opt_level", "speed").ok()?;
        cranelift_native::builder()
            .ok()?
            .finish(settings::Flags::new(flags))
            .ok()
    })
    .clone()
}

/// the per-register variables, one for each class a register is used at
struct Registers {
    ints: Vec<Option<Variable>>,
    floats: Vec<Option<Variable>>,
}

impl Registers {
    fn var(&mut self, builder: &mut FunctionBuilder, reg: Reg, float: bool) -> Variable {
        let (vars, ty) = if float {
            (&mut self.floats, types::F64)
        } else {
            (&mut self.ints, types::I64)
        };
        *vars[reg as usize].get_or_insert_with(|| builder.declare_var(ty))
    }

    fn int(&mut self, builder: &mut FunctionBuilder, reg: Reg) -> Value {
        let var = self.var(builder, reg, false);
        builder.use_var(var)
    }

    fn float(&mut self, builder: &mut FunctionBuilder, reg: Reg) -> Value {
        let var = self.var(builder, reg, true);
        builder.use_var(var)
    }

    fn set_int(&mut self, builder: &mut FunctionBuilder, reg: Reg, value: Value) {
        let var = self.var(builder, reg, false);
        builder.def_var(var, value);
    }

    fn set_float(&mut self, builder: &mut FunctionBuilder, reg: Reg, value: Value) {
        let var = self.var(builder, reg, true);
        builder.def_var(var, value);
    }
}

fn compile(spec: &Spec) -> Option<Compiled> {
    let facts = analyse(spec)?;
    let isa = host_isa()?;
    let mut jit = JITBuilder::with_isa(isa, default_libcall_names());
    for shim in Shim::ALL {
        jit.symbol(shim.name(), shim.address());
    }
    let mut module = JITModule::new(jit);
    let ptr = module.target_config().pointer_type();

    let mut shims: [Option<FuncId>; Shim::ALL.len()] = [None; Shim::ALL.len()];
    for (slot, shim) in shims.iter_mut().zip(Shim::ALL) {
        let (params, ret) = shim.signature(ptr);
        let mut sig = module.make_signature();
        sig.params.extend(params.into_iter().map(AbiParam::new));
        sig.returns.push(AbiParam::new(ret));
        *slot = Some(
            module
                .declare_function(shim.name(), Linkage::Import, &sig)
                .ok()?,
        );
    }

    let mut ctx = module.make_context();
    ctx.func.signature.params.extend([AbiParam::new(ptr); 3]);
    ctx.func.signature.returns.push(AbiParam::new(types::I32));
    let id = module
        .declare_function("kernel", Linkage::Local, &ctx.func.signature)
        .ok()?;
    ctx.func.name = UserFuncName::user(0, id.as_u32());

    let mut builder_ctx = FunctionBuilderContext::new();
    {
        let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);
        let mut refs = [None; Shim::ALL.len()];
        for (slot, id) in refs.iter_mut().zip(shims) {
            *slot = Some(module.declare_func_in_func(id?, builder.func));
        }
        Codegen {
            spec,
            facts: &facts,
            refs: refs.map(Option::unwrap),
            ptr,
        }
        .emit(&mut builder)?;
        builder.seal_all_blocks();
        builder.finalize(module.target_config());
    }
    module.define_function(id, &mut ctx).ok()?;
    module.clear_context(&mut ctx);
    module.finalize_definitions().ok()?;
    let code = module.get_finalized_function(id);
    Some(Compiled {
        // safety: the function was declared with exactly this signature
        entry: unsafe { std::mem::transmute::<*const u8, Entry>(code) },
        module: Some(module),
    })
}

struct Codegen<'a> {
    spec: &'a Spec,
    facts: &'a Facts,
    refs: [cranelift_codegen::ir::FuncRef; Shim::ALL.len()],
    ptr: types::Type,
}

/// a jump back to `to`, which charges the budget and looks at the abort flag
/// before it lands
struct BackEdge {
    block: Block,
    to: u32,
    weight: i64,
}

impl Codegen<'_> {
    fn emit(&self, b: &mut FunctionBuilder) -> Option<()> {
        let ops = &self.spec.ops;
        let len = ops.len();
        let frame_size = self.spec.frame_size as usize;
        let flags = MemFlagsData::trusted();
        // the entry registers and constants are never written by the body,
        // so their loads may be merged and hoisted out of loops
        let fixed = flags.with_readonly().with_can_move();
        let layout = Layout::of(self.spec.frame_size);
        let word = |slot: usize| (slot * 8) as i32;

        let mut leaders = vec![false; len + 1];
        leaders[0] = true;
        leaders[len] = true;
        for (pc, op) in ops.iter().enumerate() {
            let pc = pc as u32;
            // a jump ends its block even when it lands on the next op
            if matches!(op, TOp::Jump { .. }) || successors(op, pc).ne([pc + 1]) {
                leaders[pc as usize + 1] = true;
                for to in successors(op, pc) {
                    *leaders.get_mut(to as usize)? = true;
                }
            }
        }
        let blocks: Vec<Option<Block>> = leaders
            .iter()
            .map(|&leader| leader.then(|| b.create_block()))
            .collect();

        let mut regs = Registers {
            ints: vec![None; frame_size],
            floats: vec![None; frame_size],
        };
        let mut faults: FxHashMap<i32, Block> = FxHashMap::default();
        let mut fault_block = |b: &mut FunctionBuilder, fault: Fault| {
            *faults
                .entry(fault_code(fault))
                .or_insert_with(|| b.create_block())
        };
        let mut back_edges: Vec<BackEdge> = Vec::new();

        // entry: the budget, the entry registers and the constants
        let start = b.create_block();
        b.append_block_params_for_function_params(start);
        b.switch_to_block(start);
        let [frame, budget_ptr, abort] = b.block_params(start) else {
            return None;
        };
        let (frame, budget_ptr, abort) = (*frame, *budget_ptr, *abort);
        let budget = b.declare_var(types::I64);
        let initial = b.ins().load(types::I64, flags, budget_ptr, 0);
        b.def_var(budget, initial);
        for (reg, class) in self.spec.entry.iter().enumerate() {
            let offset = (reg * 8) as i32;
            match class {
                Class::Int => {
                    let value = b.ins().load(types::I64, fixed, frame, offset);
                    regs.set_int(b, reg as Reg, value);
                }
                Class::Float => {
                    let value = b.ins().load(types::F64, fixed, frame, offset);
                    regs.set_float(b, reg as Reg, value);
                }
                _ => {}
            }
        }
        // float constants are read from the frame where they are used
        let mut next_const = layout.consts;
        let mut constant = |b: &mut FunctionBuilder| {
            let value = b.ins().load(types::F64, fixed, frame, word(next_const));
            next_const += 1;
            value
        };
        let counts: [Variable; LIST_SITES] = std::array::from_fn(|_| b.declare_var(types::I64));
        let items = b.ins().iadd_imm_s(frame, word(layout.items) as i64);
        let out = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 8, 3));
        b.ins().jump(blocks[0]?, &[]);

        let mut open = false;
        for (pc, op) in ops.iter().enumerate() {
            if let Some(block) = blocks[pc] {
                if open {
                    b.ins().jump(block, &[]);
                }
                b.switch_to_block(block);
                open = true;
            } else if !open {
                // unreachable ops after a jump or return still need a block
                let block = b.create_block();
                b.switch_to_block(block);
                open = true;
            }
            let pc32 = pc as u32;
            let next = blocks.get(pc + 1).copied().flatten();
            let mut target = |b: &mut FunctionBuilder, to: u32| -> Option<Block> {
                if to <= pc32 {
                    let block = b.create_block();
                    back_edges.push(BackEdge {
                        block,
                        to,
                        weight: (pc32 - to + 1) as i64,
                    });
                    Some(block)
                } else {
                    blocks.get(to as usize).copied().flatten()
                }
            };

            macro_rules! int {
                ($r:expr) => {
                    regs.int(b, $r)
                };
            }
            macro_rules! float {
                ($r:expr) => {
                    regs.float(b, $r)
                };
            }
            macro_rules! set_int {
                ($r:expr, $v:expr) => {{
                    let value = $v;
                    regs.set_int(b, $r, value)
                }};
            }
            macro_rules! set_float {
                ($r:expr, $v:expr) => {{
                    let value = $v;
                    regs.set_float(b, $r, value)
                }};
            }
            macro_rules! flag {
                ($cond:expr) => {{
                    let cond = $cond;
                    b.ins().uextend(types::I64, cond)
                }};
            }
            // a shim that can fault: its code is checked and the result read
            // from the out slot
            macro_rules! fallible {
                ($shim:expr, $args:expr) => {{
                    let out_ptr = b.ins().stack_addr(self.ptr, out, 0);
                    let mut args: Vec<Value> = $args.to_vec();
                    args.push(out_ptr);
                    let call = b.ins().call(self.refs[$shim as usize], &args);
                    let code = b.inst_results(call)[0];
                    let ok = b.create_block();
                    let failed = b.create_block();
                    b.ins().brif(code, failed, &[], ok, &[]);
                    b.switch_to_block(failed);
                    b.ins().return_(&[code]);
                    b.switch_to_block(ok);
                    b.ins().stack_load(self.ptr, types::I64, out, 0)
                }};
            }
            macro_rules! pure {
                ($shim:expr, $args:expr) => {{
                    let call = b.ins().call(self.refs[$shim as usize], &$args);
                    b.inst_results(call)[0]
                }};
            }
            macro_rules! guard_zero {
                ($is_zero:expr) => {{
                    let is_zero = $is_zero;
                    let fault = fault_block(b, Fault::DivisionByZero);
                    let ok = b.create_block();
                    b.ins().brif(is_zero, fault, &[], ok, &[]);
                    b.switch_to_block(ok);
                }};
            }
            macro_rules! branch {
                ($cond:expr, $to:expr) => {{
                    let cond = $cond;
                    let taken = target(b, $to)?;
                    b.ins().brif(cond, taken, &[], next?, &[]);
                    open = false;
                }};
            }

            match *op {
                TOp::Nop => {}
                TOp::IConst { dst, value } => set_int!(dst, b.ins().iconst(types::I64, value)),
                TOp::FConst { dst, .. } => set_float!(dst, constant(b)),
                // boxed scalars are never read back (`analyse` checks), and
                // there is one list per call
                TOp::BoxI { .. }
                | TOp::BoxF { .. }
                | TOp::BoxC { .. }
                | TOp::Nil { .. }
                | TOp::MoveB { .. } => {}
                TOp::EmptyList { .. } => {
                    let zero = b.ins().iconst(types::I64, 0);
                    b.def_var(counts[self.facts.sites[pc] as usize], zero);
                }
                TOp::Append { value, .. } => {
                    let site = self.facts.sites[pc];
                    let count = counts[site as usize];
                    let n = b.use_var(count);
                    let full =
                        b.ins()
                            .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, n, LIST_CAP as i64);
                    let fault = fault_block(b, Fault::Type);
                    let room = b.create_block();
                    b.ins().brif(full, fault, &[], room, &[]);
                    b.switch_to_block(room);
                    let (value, tag) = match value {
                        Opnd::I(reg) => (int!(reg), KIND_INT),
                        Opnd::F(reg) => (float!(reg), KIND_FLOAT),
                        Opnd::B(_) | Opnd::C(_) => return None,
                    };
                    let offset = b.ins().ishl_imm_s(n, 3);
                    let at = b.ins().iadd(items, offset);
                    let first = word(Layout::site(site));
                    b.ins().store(flags, value, at, first);
                    let tag = b.ins().iconst(types::I64, tag);
                    b.ins().store(flags, tag, at, first + word(LIST_CAP));
                    let n = b.ins().iadd_imm_s(n, 1);
                    b.def_var(count, n);
                }
                TOp::MoveS { dst, src } => {
                    if self.facts.float_moves[pc] {
                        set_float!(dst, float!(src))
                    } else {
                        set_int!(dst, int!(src))
                    }
                }
                TOp::IToF { dst, src } => set_float!(dst, {
                    let x = int!(src);
                    b.ins().fcvt_from_sint(types::F64, x)
                }),
                TOp::IAdd { dst, a, b: rhs } => set_int!(dst, {
                    let (x, y) = (int!(a), int!(rhs));
                    b.ins().iadd(x, y)
                }),
                TOp::ISub { dst, a, b: rhs } => set_int!(dst, {
                    let (x, y) = (int!(a), int!(rhs));
                    b.ins().isub(x, y)
                }),
                TOp::IMul { dst, a, b: rhs } => set_int!(dst, {
                    let (x, y) = (int!(a), int!(rhs));
                    b.ins().imul(x, y)
                }),
                TOp::IBin { op, dst, a, b: rhs } => {
                    let (x, y) = (int!(a), int!(rhs));
                    let compare = |b: &mut FunctionBuilder, cc| {
                        let cond = b.ins().icmp(cc, x, y);
                        b.ins().uextend(types::I64, cond)
                    };
                    match op {
                        BinKind::Add => set_int!(dst, b.ins().iadd(x, y)),
                        BinKind::Sub => set_int!(dst, b.ins().isub(x, y)),
                        BinKind::Mul => set_int!(dst, b.ins().imul(x, y)),
                        BinKind::Div => {
                            guard_zero!(b.ins().icmp_imm_s(IntCC::Equal, y, 0));
                            let fx = b.ins().fcvt_from_sint(types::F64, x);
                            let fy = b.ins().fcvt_from_sint(types::F64, y);
                            set_float!(dst, b.ins().fdiv(fx, fy))
                        }
                        BinKind::IntDiv | BinKind::Power => {
                            let code = b.ins().iconst(types::I32, bin_code(op) as i64);
                            let result = fallible!(Shim::IntBinary, [code, x, y]);
                            if op == BinKind::Power {
                                set_float!(
                                    dst,
                                    b.ins().bitcast(types::F64, MemFlagsData::new(), result)
                                )
                            } else {
                                set_int!(dst, result)
                            }
                        }
                        BinKind::Lt => set_int!(dst, compare(b, IntCC::SignedLessThan)),
                        BinKind::Le => set_int!(dst, compare(b, IntCC::SignedLessThanOrEqual)),
                        BinKind::Gt => set_int!(dst, compare(b, IntCC::SignedGreaterThan)),
                        BinKind::Ge => {
                            set_int!(dst, compare(b, IntCC::SignedGreaterThanOrEqual))
                        }
                        BinKind::Eq => set_int!(dst, compare(b, IntCC::Equal)),
                        BinKind::Ne => set_int!(dst, compare(b, IntCC::NotEqual)),
                        BinKind::In => {
                            let fault = fault_block(b, Fault::Type);
                            b.ins().jump(fault, &[]);
                            open = false;
                        }
                    }
                }
                TOp::FAdd { dst, a, b: rhs } => set_float!(dst, {
                    let (x, y) = (float!(a), float!(rhs));
                    b.ins().fadd(x, y)
                }),
                TOp::FSub { dst, a, b: rhs } => set_float!(dst, {
                    let (x, y) = (float!(a), float!(rhs));
                    b.ins().fsub(x, y)
                }),
                TOp::FMul { dst, a, b: rhs } => set_float!(dst, {
                    let (x, y) = (float!(a), float!(rhs));
                    b.ins().fmul(x, y)
                }),
                TOp::FDiv { dst, a, b: rhs } => {
                    let (x, y) = (float!(a), float!(rhs));
                    let zero = b.ins().f64const(0.0);
                    guard_zero!(b.ins().fcmp(FloatCC::Equal, y, zero));
                    set_float!(dst, b.ins().fdiv(x, y))
                }
                TOp::FBin { op, dst, a, b: rhs } => {
                    let (x, y) = (float!(a), float!(rhs));
                    let compare = |b: &mut FunctionBuilder, cc| {
                        let cond = b.ins().fcmp(cc, x, y);
                        b.ins().uextend(types::I64, cond)
                    };
                    match op {
                        BinKind::Add => set_float!(dst, b.ins().fadd(x, y)),
                        BinKind::Sub => set_float!(dst, b.ins().fsub(x, y)),
                        BinKind::Mul => set_float!(dst, b.ins().fmul(x, y)),
                        BinKind::Div => {
                            let zero = b.ins().f64const(0.0);
                            guard_zero!(b.ins().fcmp(FloatCC::Equal, y, zero));
                            set_float!(dst, b.ins().fdiv(x, y))
                        }
                        BinKind::IntDiv | BinKind::Power => {
                            let code = b.ins().iconst(types::I32, bin_code(op) as i64);
                            let result = fallible!(Shim::FloatBinary, [code, x, y]);
                            if op == BinKind::Power {
                                set_float!(
                                    dst,
                                    b.ins().bitcast(types::F64, MemFlagsData::new(), result)
                                )
                            } else {
                                set_int!(dst, result)
                            }
                        }
                        BinKind::Lt => set_int!(dst, compare(b, FloatCC::LessThan)),
                        BinKind::Le => set_int!(dst, compare(b, FloatCC::LessThanOrEqual)),
                        BinKind::Gt => set_int!(dst, compare(b, FloatCC::GreaterThan)),
                        BinKind::Ge => set_int!(dst, compare(b, FloatCC::GreaterThanOrEqual)),
                        BinKind::Eq => set_int!(dst, compare(b, FloatCC::Equal)),
                        BinKind::Ne => set_int!(dst, compare(b, FloatCC::NotEqual)),
                        BinKind::In => {
                            let fault = fault_block(b, Fault::Type);
                            b.ins().jump(fault, &[]);
                            open = false;
                        }
                    }
                }
                TOp::INeg { dst, src } => set_int!(dst, {
                    let x = int!(src);
                    b.ins().ineg(x)
                }),
                TOp::FNeg { dst, src } => set_float!(dst, {
                    let x = float!(src);
                    b.ins().fneg(x)
                }),
                TOp::INot { dst, src } => set_int!(dst, {
                    let x = int!(src);
                    flag!(b.ins().icmp_imm_s(IntCC::Equal, x, 0))
                }),
                TOp::FNot { dst, src } => set_int!(dst, {
                    let x = float!(src);
                    let zero = b.ins().f64const(0.0);
                    flag!(b.ins().fcmp(FloatCC::Equal, x, zero))
                }),
                TOp::Jump { to } => {
                    let to = target(b, to)?;
                    b.ins().jump(to, &[]);
                    open = false;
                }
                TOp::JumpIfI { cond, to } => branch!(
                    {
                        let x = int!(cond);
                        b.ins().icmp_imm_s(IntCC::NotEqual, x, 0)
                    },
                    to
                ),
                TOp::JumpIfNotI { cond, to } => branch!(
                    {
                        let x = int!(cond);
                        b.ins().icmp_imm_s(IntCC::Equal, x, 0)
                    },
                    to
                ),
                TOp::JumpIfF { cond, to } => branch!(
                    {
                        let x = float!(cond);
                        let zero = b.ins().f64const(0.0);
                        b.ins().fcmp(FloatCC::NotEqual, x, zero)
                    },
                    to
                ),
                TOp::JumpIfNotF { cond, to } => branch!(
                    {
                        let x = float!(cond);
                        let zero = b.ins().f64const(0.0);
                        b.ins().fcmp(FloatCC::Equal, x, zero)
                    },
                    to
                ),
                TOp::RangeTestI { current, to } => branch!(
                    {
                        let (x, y) = (int!(current), int!(current + 1));
                        b.ins().icmp(IntCC::SignedGreaterThanOrEqual, x, y)
                    },
                    to
                ),
                TOp::RangeTestF { current, to } => branch!(
                    {
                        // `!(x < y)`, which a nan satisfies
                        let (x, y) = (float!(current), float!(current + 1));
                        b.ins().fcmp(FloatCC::UnorderedOrGreaterThanOrEqual, x, y)
                    },
                    to
                ),
                TOp::IInc { reg } => set_int!(reg, {
                    let x = int!(reg);
                    b.ins().iadd_imm_s(x, 1)
                }),
                TOp::FInc { reg } => set_float!(reg, {
                    let x = float!(reg);
                    let one = b.ins().f64const(1.0);
                    b.ins().fadd(x, one)
                }),
                TOp::FUnary { f, dst, src } => set_float!(dst, {
                    let x = float!(src);
                    let f = b.ins().iconst(self.ptr, f as usize as i64);
                    pure!(Shim::Unary, [f, x])
                }),
                TOp::FToI { f, dst, src } => set_int!(dst, {
                    let x = float!(src);
                    let f = b.ins().iconst(self.ptr, f as usize as i64);
                    let y = pure!(Shim::Unary, [f, x]);
                    b.ins().fcvt_to_sint_sat(types::I64, y)
                }),
                TOp::FTrunc { dst, src } => set_int!(dst, {
                    let x = float!(src);
                    b.ins().fcvt_to_sint_sat(types::I64, x)
                }),
                TOp::IAbs { dst, src } => set_int!(dst, {
                    let x = int!(src);
                    let negative = b.ins().icmp_imm_s(IntCC::SignedLessThan, x, 0);
                    let negated = b.ins().ineg(x);
                    b.ins().select(negative, negated, x)
                }),
                TOp::ISign { dst, src } => set_int!(dst, {
                    let x = int!(src);
                    let positive = flag!(b.ins().icmp_imm_s(IntCC::SignedGreaterThan, x, 0));
                    let negative = flag!(b.ins().icmp_imm_s(IntCC::SignedLessThan, x, 0));
                    b.ins().isub(positive, negative)
                }),
                TOp::FAbs { dst, src } => set_float!(dst, {
                    let x = float!(src);
                    b.ins().fabs(x)
                }),
                TOp::FSign { dst, src } => set_float!(dst, {
                    let x = float!(src);
                    pure!(Shim::FloatSign, [x])
                }),
                TOp::IMod { dst, a, b: rhs } => set_int!(dst, {
                    let (x, y) = (int!(a), int!(rhs));
                    fallible!(Shim::IntMod, [x, y])
                }),
                TOp::FMod { dst, a, b: rhs } => set_float!(dst, {
                    let (x, y) = (float!(a), float!(rhs));
                    let result = fallible!(Shim::FloatMod, [x, y]);
                    b.ins().bitcast(types::F64, MemFlagsData::new(), result)
                }),
                TOp::IMin { dst, a, b: rhs } => set_int!(dst, {
                    let (x, y) = (int!(a), int!(rhs));
                    let le = b.ins().icmp(IntCC::SignedLessThanOrEqual, x, y);
                    b.ins().select(le, x, y)
                }),
                TOp::IMax { dst, a, b: rhs } => set_int!(dst, {
                    let (x, y) = (int!(a), int!(rhs));
                    let ge = b.ins().icmp(IntCC::SignedGreaterThanOrEqual, x, y);
                    b.ins().select(ge, x, y)
                }),
                TOp::FMin { dst, a, b: rhs } => set_float!(dst, {
                    let (x, y) = (float!(a), float!(rhs));
                    pure!(Shim::FloatMin, [x, y])
                }),
                TOp::FMax { dst, a, b: rhs } => set_float!(dst, {
                    let (x, y) = (float!(a), float!(rhs));
                    pure!(Shim::FloatMax, [x, y])
                }),
                TOp::FPow { dst, a, b: rhs } => set_float!(dst, {
                    let (x, y) = (float!(a), float!(rhs));
                    pure!(Shim::FloatPow, [x, y])
                }),
                TOp::FAtan2 { dst, a, b: rhs } => set_float!(dst, {
                    let (x, y) = (float!(a), float!(rhs));
                    pure!(Shim::FloatAtan2, [x, y])
                }),
                TOp::Return { src } => {
                    let (value, kind, slot) = match src {
                        Opnd::I(reg) => (int!(reg), KIND_INT, layout.result),
                        Opnd::F(reg) => (float!(reg), KIND_FLOAT, layout.result),
                        Opnd::B(_) => {
                            let site = self.facts.sites[pc];
                            let count = b.use_var(counts[site as usize]);
                            (count, KIND_LIST + site as i64, layout.count)
                        }
                        Opnd::C(_) => return None,
                    };
                    b.ins().store(flags, value, frame, word(slot));
                    let kind = b.ins().iconst(types::I64, kind);
                    b.ins().store(flags, kind, frame, word(layout.kind));
                    let ok = b.ins().iconst(types::I32, 0);
                    b.ins().return_(&[ok]);
                    open = false;
                }
                _ => return None,
            }
        }

        // falling off the end is not something a body does; fault if it did
        if open {
            b.ins().jump(blocks[len]?, &[]);
        }
        b.switch_to_block(blocks[len]?);
        let fault = fault_block(b, Fault::Type);
        b.ins().jump(fault, &[]);

        for edge in back_edges {
            b.switch_to_block(edge.block);
            let left = b.use_var(budget);
            let left = b.ins().iadd_imm_s(left, -edge.weight);
            b.def_var(budget, left);
            let exhausted = b.ins().icmp_imm_s(IntCC::SignedLessThanOrEqual, left, 0);
            let spent = fault_block(b, Fault::Budget);
            let running = b.create_block();
            b.ins().brif(exhausted, spent, &[], running, &[]);
            b.switch_to_block(running);
            let raised = b.ins().atomic_load(types::I8, flags, abort);
            let aborted = fault_block(b, Fault::Aborted);
            b.ins()
                .brif(raised, aborted, &[], blocks[edge.to as usize]?, &[]);
        }
        for (code, block) in faults {
            b.switch_to_block(block);
            let code = b.ins().iconst(types::I32, code as i64);
            b.ins().return_(&[code]);
        }
        Some(())
    }
}

/// a compiled body plus the constants of the batch that asked for it
pub struct JitEntry {
    compiled: Arc<Compiled>,
    consts: Box<[u64]>,
}

/// compiled bodies by shape, owned by the kernel tier
pub struct JitCache {
    enabled: bool,
    compiled: FxHashMap<Box<[u64]>, Option<Arc<Compiled>>>,
}

impl JitCache {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            compiled: FxHashMap::default(),
        }
    }

    /// native code for `spec`, compiling it the first time its shape is seen;
    /// `None` when the spec is not purely scalar or the jit is off
    pub fn prepare(&mut self, spec: &Spec, stats: &mut KernelStats) -> Option<Arc<JitEntry>> {
        if !self.enabled {
            return None;
        }
        let shape = shape(spec)?;
        let compiled = match self.compiled.get(&shape.key) {
            Some(cached) => cached.clone(),
            None => {
                if self.compiled.len() >= CACHE_MAX {
                    self.compiled.clear();
                }
                let started = Instant::now();
                let compiled = compile(spec).map(Arc::new);
                stats.jit_compile_elapsed += started.elapsed();
                stats.jit_compiles += 1;
                self.compiled.insert(shape.key, compiled.clone());
                compiled
            }
        }?;
        Some(Arc::new(JitEntry {
            compiled,
            consts: shape.consts.into_boxed_slice(),
        }))
    }
}

/// runs one batch's calls on a compiled body: a frame of its own, with the
/// captures and constants written once and the arguments per call
pub struct JitVm {
    entry: Arc<JitEntry>,
    words: Vec<u64>,
    abort: Option<Arc<AtomicBool>>,
    /// whether the captures could be laid out; if not every call faults
    ready: bool,
}

static NEVER_ABORT: AtomicBool = AtomicBool::new(false);

impl JitVm {
    pub fn new(
        entry: Arc<JitEntry>,
        spec: &Spec,
        arena: &ClosureArena,
        abort: Option<Arc<AtomicBool>>,
    ) -> Self {
        let layout = Layout::of(spec.frame_size);
        let mut words = vec![0; layout.consts + entry.consts.len()];
        words[layout.consts..].copy_from_slice(&entry.consts);
        let total = spec.sig.len();
        let ready = arena
            .get(spec.closure)
            .captures
            .iter()
            .enumerate()
            .all(|(i, capture)| store(&mut words, total + i, spec.entry[total + i], capture));
        Self {
            entry,
            words,
            abort,
            ready,
        }
    }

    /// run the body on `args`, whose classes the caller has checked with
    /// `TVm::accepts`
    pub fn call(&mut self, spec: &Spec, args: &[KVal]) -> Result<KVal, Fault> {
        if !self.ready {
            return Err(Fault::Type);
        }
        for (i, (arg, class)) in args.iter().zip(&spec.sig).enumerate() {
            if !store(&mut self.words, i, *class, arg) {
                return Err(Fault::Type);
            }
        }
        let mut budget = CALL_OP_BUDGET as i64;
        let abort = self
            .abort
            .as_deref()
            .unwrap_or(&NEVER_ABORT)
            .as_ptr()
            .cast_const()
            .cast::<u8>();
        let compiled = &self.entry.compiled;
        // safety: the frame has the layout the code was generated for, and
        // the budget and abort flag outlive the call
        let code = unsafe { (compiled.entry)(self.words.as_mut_ptr(), &mut budget, abort) };
        if code != 0 {
            return Err(fault_of(code));
        }
        let layout = Layout::of(spec.frame_size);
        let scalar = |word: u64, kind: u64| match kind as i64 {
            KIND_INT => KVal::Int(word as i64),
            _ => KVal::Float(f64::from_bits(word)),
        };
        let words = &self.words;
        Ok(match words[layout.kind] as i64 {
            kind @ KIND_LIST.. => {
                let items = layout.items + Layout::site((kind - KIND_LIST) as u8);
                let count = words[layout.count] as usize;
                KVal::list(
                    words[items..items + count]
                        .iter()
                        .zip(&words[items + LIST_CAP..])
                        .map(|(word, kind)| scalar(*word, *kind)),
                )
            }
            kind => scalar(words[layout.result], kind as u64),
        })
    }
}

/// write `value` into `words[slot]` as the typed machine's `store` would;
/// closures need no word. false where the classes disagree
fn store(words: &mut [u64], slot: usize, class: Class, value: &KVal) -> bool {
    match (class, value) {
        (Class::Int, KVal::Int(n)) => words[slot] = *n as u64,
        (Class::Float, KVal::Float(f)) => words[slot] = f.to_bits(),
        (Class::Closure(_), KVal::Closure(_)) => {}
        _ => return false,
    }
    true
}

/// whether `MONOCURL_KERNEL_JIT` leaves the jit on (it is unless `0`)
pub fn enabled_by_env() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("MONOCURL_KERNEL_JIT").as_deref(),
            Ok("0") | Ok("off") | Ok("false")
        )
    })
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;
    use crate::kernel::{
        ir::{KOp, Kernel, KernelIntrinsic},
        typed::TypedProgram,
        typed_run::TVm,
        value::{ClosureId, KClosure},
    };

    fn closure(total_args: u16, frame_size: u16, ops: Vec<KOp>) -> (ClosureArena, ClosureId) {
        let mut arena = ClosureArena::default();
        let id = arena.push(KClosure {
            ip: Default::default(),
            kernel: Arc::new(Kernel {
                ip: Default::default(),
                required_args: total_args,
                total_args,
                capture_count: 0,
                frame_size,
                ops: ops.into_boxed_slice(),
            }),
            captures: Box::new([]),
            defaults: Box::new([]),
        });
        (arena, id)
    }

    struct Compiled {
        arena: ClosureArena,
        program: TypedProgram,
        spec: u32,
        entry: Arc<JitEntry>,
    }

    fn compiled(total_args: u16, frame_size: u16, ops: Vec<KOp>, sample: &[KVal]) -> Compiled {
        let (arena, id) = closure(total_args, frame_size, ops);
        let (program, spec) = TypedProgram::specialise(&arena, id, sample).expect("typed");
        let entry = JitCache::new(true)
            .prepare(program.spec(spec), &mut KernelStats::default())
            .expect("compiled");
        Compiled {
            arena,
            program,
            spec,
            entry,
        }
    }

    impl Compiled {
        /// the jit and the typed machine agree on every call
        fn agree(&self, calls: impl IntoIterator<Item = Vec<KVal>>) {
            let spec = self.program.spec(self.spec);
            let mut jit = JitVm::new(Arc::clone(&self.entry), spec, &self.arena, None);
            let mut tvm = TVm::new();
            for args in calls {
                let native = jit.call(spec, &args);
                let typed = tvm.call(&self.program, &self.arena, self.spec, &args);
                match (&native, &typed) {
                    (Ok(a), Ok(b)) => assert!(
                        KVal::strictly_equal(a, b),
                        "{args:?}: native {a:?}, typed {b:?}"
                    ),
                    (Err(a), Err(b)) => assert_eq!(a, b, "{args:?}"),
                    _ => panic!("{args:?}: native {native:?}, typed {typed:?}"),
                }
            }
        }
    }

    /// a xorshift over values that include the awkward ones
    fn samples(seed: u64, count: usize) -> impl Iterator<Item = (i64, f64)> {
        let mut state = seed;
        let awkward_ints = [0, 1, -1, 2, -7, i64::MAX, i64::MIN, 1 << 40];
        let awkward_floats = [
            0.0,
            -0.0,
            1.0,
            -2.5,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1e300,
            -1e-300,
            0.5,
        ];
        (0..count).map(move |i| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let int = if i % 3 == 0 {
                awkward_ints[(state % awkward_ints.len() as u64) as usize]
            } else {
                (state as i64) % 1000
            };
            let float = if i % 3 == 1 {
                awkward_floats[(state % awkward_floats.len() as u64) as usize]
            } else {
                (state % 100_000) as f64 / 97.0 - 500.0
            };
            (int, float)
        })
    }

    #[test]
    fn a_loop_over_ints_and_floats_matches_the_typed_machine() {
        // acc = 0.0; i = 0; while i < n { acc = acc + x * i; i += 1 }; acc
        let ops = vec![
            KOp::Float { dst: 2, value: 0.0 },
            KOp::Int { dst: 3, value: 0 },
            KOp::Bin {
                op: BinKind::Lt,
                dst: 4,
                a: 3,
                b: 0,
                take: false,
            },
            KOp::JumpIfNot { cond: 4, to: 8 },
            KOp::Bin {
                op: BinKind::Mul,
                dst: 5,
                a: 1,
                b: 3,
                take: false,
            },
            KOp::Bin {
                op: BinKind::Add,
                dst: 2,
                a: 2,
                b: 5,
                take: false,
            },
            KOp::Inc { reg: 3 },
            KOp::Jump { to: 2 },
            KOp::Return { src: 2 },
        ];
        let jit = compiled(2, 6, ops, &[KVal::Int(3), KVal::Float(0.5)]);
        jit.agree(samples(1, 500).map(|(n, x)| vec![KVal::Int(n.rem_euclid(200)), KVal::Float(x)]));
    }

    #[test]
    fn division_modulo_and_their_faults_match_the_typed_machine() {
        // a // b + mod(a, b) + a / b + a ** b
        let ops = vec![
            KOp::Bin {
                op: BinKind::IntDiv,
                dst: 2,
                a: 0,
                b: 1,
                take: false,
            },
            KOp::Move { dst: 3, src: 0 },
            KOp::Move { dst: 4, src: 1 },
            KOp::Native {
                intrinsic: KernelIntrinsic::Mod,
                arg_start: 3,
                arg_count: 2,
            },
            KOp::Bin {
                op: BinKind::Add,
                dst: 2,
                a: 2,
                b: 3,
                take: false,
            },
            KOp::Bin {
                op: BinKind::Div,
                dst: 5,
                a: 0,
                b: 1,
                take: false,
            },
            KOp::Bin {
                op: BinKind::Add,
                dst: 2,
                a: 2,
                b: 5,
                take: false,
            },
            KOp::Bin {
                op: BinKind::Power,
                dst: 5,
                a: 0,
                b: 1,
                take: false,
            },
            KOp::Bin {
                op: BinKind::Add,
                dst: 2,
                a: 2,
                b: 5,
                take: false,
            },
            KOp::Return { src: 2 },
        ];
        let ints = compiled(2, 6, ops.clone(), &[KVal::Int(7), KVal::Int(2)]);
        ints.agree(
            samples(2, 500)
                .zip(samples(8, 500))
                .map(|((a, _), (b, _))| vec![KVal::Int(a), KVal::Int(b % 5)]),
        );
        let floats = compiled(2, 6, ops, &[KVal::Float(7.0), KVal::Float(2.0)]);
        floats.agree(samples(3, 500).map(|(_, x)| vec![KVal::Float(x), KVal::Float(x % 3.0)]));
        let zero = [vec![KVal::Float(1.0), KVal::Float(0.0)]];
        floats.agree(zero.clone());
        let spec = floats.program.spec(floats.spec);
        let mut jit = JitVm::new(Arc::clone(&floats.entry), spec, &floats.arena, None);
        assert_eq!(jit.call(spec, &zero[0]).err(), Some(Fault::DivisionByZero));
    }

    #[test]
    fn natives_and_comparisons_match_the_typed_machine() {
        // floor(sin(x) * 10) + sign(x) - min(x, y) + max(x, y) + atan2(x, y)
        // + abs(y), plus the comparisons of x and y
        let mut ops = Vec::new();
        let unary = |ops: &mut Vec<KOp>, intrinsic, dst: u16, src: u16| {
            ops.push(KOp::Move { dst, src });
            ops.push(KOp::Native {
                intrinsic,
                arg_start: dst,
                arg_count: 1,
            });
        };
        let binary = |ops: &mut Vec<KOp>, intrinsic, dst: u16| {
            ops.push(KOp::Move { dst, src: 0 });
            ops.push(KOp::Move {
                dst: dst + 1,
                src: 1,
            });
            ops.push(KOp::Native {
                intrinsic,
                arg_start: dst,
                arg_count: 2,
            });
        };
        let add = |ops: &mut Vec<KOp>, op| {
            ops.push(KOp::Bin {
                op,
                dst: 2,
                a: 2,
                b: 3,
                take: false,
            })
        };
        unary(&mut ops, KernelIntrinsic::Sin, 2, 0);
        unary(&mut ops, KernelIntrinsic::Floor, 2, 2);
        unary(&mut ops, KernelIntrinsic::Sign, 3, 0);
        add(&mut ops, BinKind::Add);
        binary(&mut ops, KernelIntrinsic::Min, 3);
        add(&mut ops, BinKind::Sub);
        binary(&mut ops, KernelIntrinsic::Max, 3);
        add(&mut ops, BinKind::Add);
        binary(&mut ops, KernelIntrinsic::Atan2, 3);
        add(&mut ops, BinKind::Add);
        unary(&mut ops, KernelIntrinsic::Abs, 3, 1);
        add(&mut ops, BinKind::Add);
        unary(&mut ops, KernelIntrinsic::ToInt, 3, 0);
        add(&mut ops, BinKind::Add);
        for op in [
            BinKind::Lt,
            BinKind::Le,
            BinKind::Gt,
            BinKind::Ge,
            BinKind::Eq,
            BinKind::Ne,
            BinKind::IntDiv,
        ] {
            ops.push(KOp::Bin {
                op,
                dst: 3,
                a: 0,
                b: 1,
                take: false,
            });
            add(&mut ops, BinKind::Add);
        }
        ops.push(KOp::Return { src: 2 });
        let floats = compiled(2, 6, ops.clone(), &[KVal::Float(0.5), KVal::Float(2.0)]);
        floats.agree(
            samples(4, 2000)
                .zip(samples(5, 2000))
                .map(|((_, x), (_, y))| vec![KVal::Float(x), KVal::Float(y)]),
        );
        let ints = compiled(2, 6, ops, &[KVal::Int(1), KVal::Int(2)]);
        ints.agree(
            samples(6, 2000)
                .zip(samples(7, 2000))
                .map(|((a, _), (b, _))| vec![KVal::Int(a), KVal::Int(b)]),
        );
    }

    #[test]
    fn an_endless_loop_runs_out_of_budget_and_stops_on_abort() {
        // i = 0; while 1 { i += 1 }; i
        let ops = vec![
            KOp::Int { dst: 1, value: 0 },
            KOp::Int { dst: 2, value: 1 },
            KOp::JumpIfNot { cond: 2, to: 5 },
            KOp::Inc { reg: 1 },
            KOp::Jump { to: 2 },
            KOp::Return { src: 1 },
        ];
        let jit = compiled(1, 3, ops, &[KVal::Int(0)]);
        let spec = jit.program.spec(jit.spec);
        let mut vm = JitVm::new(Arc::clone(&jit.entry), spec, &jit.arena, None);
        assert_eq!(vm.call(spec, &[KVal::Int(0)]).err(), Some(Fault::Budget));

        let abort = Arc::new(AtomicBool::new(false));
        let mut vm = JitVm::new(
            Arc::clone(&jit.entry),
            spec,
            &jit.arena,
            Some(Arc::clone(&abort)),
        );
        abort.store(true, Ordering::Relaxed);
        assert_eq!(vm.call(spec, &[KVal::Int(0)]).err(), Some(Fault::Aborted));
    }

    #[test]
    fn boxed_arguments_and_list_reads_are_declined() {
        let ops = vec![KOp::Return { src: 0 }];
        let (arena, id) = closure(1, 1, ops);
        let list = KVal::list([KVal::Int(1)]);
        let (program, spec) = TypedProgram::specialise(&arena, id, &[list]).unwrap();
        let mut cache = JitCache::new(true);
        let mut stats = KernelStats::default();
        assert!(cache.prepare(program.spec(spec), &mut stats).is_none());

        let ops = vec![
            KOp::EmptyList { dst: 1 },
            KOp::Append { list: 1, value: 0 },
            KOp::Len { dst: 2, src: 1 },
            KOp::Return { src: 2 },
        ];
        let (arena, id) = closure(1, 3, ops);
        let (program, spec) = TypedProgram::specialise(&arena, id, &[KVal::Int(0)]).unwrap();
        assert!(cache.prepare(program.spec(spec), &mut stats).is_none());
    }

    #[test]
    fn a_returned_list_of_scalars_matches_the_typed_machine() {
        // [x * 2, n, x + n] when n > 0, else n
        let ops = vec![
            KOp::Int { dst: 2, value: 0 },
            KOp::Bin {
                op: BinKind::Gt,
                dst: 2,
                a: 0,
                b: 2,
                take: false,
            },
            KOp::JumpIfNot { cond: 2, to: 12 },
            KOp::EmptyList { dst: 2 },
            KOp::Int { dst: 3, value: 2 },
            KOp::Bin {
                op: BinKind::Mul,
                dst: 3,
                a: 1,
                b: 3,
                take: false,
            },
            KOp::Append { list: 2, value: 3 },
            KOp::Append { list: 2, value: 0 },
            KOp::Bin {
                op: BinKind::Add,
                dst: 3,
                a: 1,
                b: 0,
                take: false,
            },
            KOp::Append { list: 2, value: 3 },
            KOp::Move { dst: 4, src: 2 },
            KOp::Return { src: 4 },
            KOp::Return { src: 0 },
        ];
        let jit = compiled(2, 5, ops, &[KVal::Int(3), KVal::Float(0.5)]);
        jit.agree(samples(9, 500).map(|(n, x)| vec![KVal::Int(n), KVal::Float(x)]));
    }

    #[test]
    fn lists_from_two_sites_match_the_typed_machine() {
        // an unused [] first, then [x] when n > 0 and [n, n] otherwise
        let ops = vec![
            KOp::EmptyList { dst: 4 },
            KOp::Int { dst: 2, value: 0 },
            KOp::Bin {
                op: BinKind::Gt,
                dst: 2,
                a: 0,
                b: 2,
                take: false,
            },
            KOp::JumpIfNot { cond: 2, to: 7 },
            KOp::EmptyList { dst: 3 },
            KOp::Append { list: 3, value: 1 },
            KOp::Return { src: 3 },
            KOp::EmptyList { dst: 2 },
            KOp::Append { list: 2, value: 0 },
            KOp::Append { list: 2, value: 0 },
            KOp::Return { src: 2 },
        ];
        let jit = compiled(2, 5, ops, &[KVal::Int(3), KVal::Float(0.5)]);
        jit.agree(samples(10, 200).map(|(n, x)| vec![KVal::Int(n), KVal::Float(x)]));
    }

    #[test]
    fn a_jump_to_the_next_op_compiles() {
        let ops = vec![
            KOp::Jump { to: 1 },
            KOp::Bin {
                op: BinKind::Add,
                dst: 1,
                a: 0,
                b: 0,
                take: false,
            },
            KOp::Return { src: 1 },
        ];
        let jit = compiled(1, 2, ops, &[KVal::Int(3)]);
        jit.agree(samples(11, 50).map(|(n, _)| vec![KVal::Int(n)]));
    }

    #[test]
    fn a_list_past_the_frame_faults_to_the_typed_machine() {
        // l = []; i = 0; while i < n { l.append(i); i += 1 }; l
        let ops = vec![
            KOp::EmptyList { dst: 1 },
            KOp::Int { dst: 2, value: 0 },
            KOp::Bin {
                op: BinKind::Lt,
                dst: 3,
                a: 2,
                b: 0,
                take: false,
            },
            KOp::JumpIfNot { cond: 3, to: 7 },
            KOp::Append { list: 1, value: 2 },
            KOp::Inc { reg: 2 },
            KOp::Jump { to: 2 },
            KOp::Return { src: 1 },
        ];
        let jit = compiled(1, 4, ops, &[KVal::Int(3)]);
        jit.agree((0..=LIST_CAP as i64).map(|n| vec![KVal::Int(n)]));
        let spec = jit.program.spec(jit.spec);
        let mut vm = JitVm::new(Arc::clone(&jit.entry), spec, &jit.arena, None);
        let long = [KVal::Int(LIST_CAP as i64 + 1)];
        assert_eq!(vm.call(spec, &long).err(), Some(Fault::Type));
    }

    #[test]
    fn a_changed_constant_reuses_the_compiled_code() {
        let body = |value| {
            vec![
                KOp::Float { dst: 1, value },
                KOp::Bin {
                    op: BinKind::Mul,
                    dst: 1,
                    a: 0,
                    b: 1,
                    take: false,
                },
                KOp::Return { src: 1 },
            ]
        };
        let mut cache = JitCache::new(true);
        let mut stats = KernelStats::default();
        let mut results = Vec::new();
        for value in [2.0, 3.0] {
            let (arena, id) = closure(1, 2, body(value));
            let (program, spec_id) =
                TypedProgram::specialise(&arena, id, &[KVal::Float(1.5)]).unwrap();
            let spec = program.spec(spec_id);
            let entry = cache.prepare(spec, &mut stats).unwrap();
            let mut vm = JitVm::new(entry, spec, &arena, None);
            results.push(vm.call(spec, &[KVal::Float(1.5)]).unwrap());
        }
        assert_eq!(stats.jit_compiles, 1);
        assert!(KVal::strictly_equal(&results[0], &KVal::Float(3.0)));
        assert!(KVal::strictly_equal(&results[1], &KVal::Float(4.5)));
    }
}
