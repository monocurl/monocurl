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
//! two boxed shapes are modelled. a list of scalars built and returned by the
//! body (a colour, a point) lives in the frame, one area per site, and a
//! register holding one is its length and site, so the lists of several
//! branches can merge into the register returned. a number, a boxed register
//! that only ever holds an int or a float (`var sum = 0` accumulating floats,
//! where the specialiser boxes at the merge), is a value word plus a kind word,
//! and the dynamic ops on it branch on the kinds the way `run::binary` does, so
//! an int stays an int until a float reaches it. a spec with a boxed argument or
//! capture, any other boxed read, or a call the specialiser did not inline is
//! declined and stays with the typed and lane machines.
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
        AbiParam, Block, InstBuilder, MemFlagsData, StackSlot, StackSlotData, StackSlotKind,
        UserFuncName, Value, condcodes::FloatCC, condcodes::IntCC, types,
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
    ir::{BinKind, KernelIntrinsic, Reg},
    run::{self, CALL_OP_BUDGET, Fault},
    tier::dump_kernels,
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

/// where things live in the frame, in words: the registers, then the result
/// (a returned list's length), its kind, each list's elements and element
/// kinds, and the float constants
#[derive(Clone, Copy)]
struct Layout {
    result: usize,
    kind: usize,
    items: usize,
    consts: usize,
}

impl Layout {
    fn of(frame_size: u16) -> Self {
        let result = frame_size as usize;
        Self {
            result,
            kind: result + 1,
            items: result + 2,
            consts: result + 2 + 2 * LIST_CAP * LIST_SITES,
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

const INTRINSICS: [KernelIntrinsic; 29] = {
    use KernelIntrinsic::*;
    [
        Sqrt, Cbrt, Exp, Ln, Sin, Cos, Tan, Asin, Acos, Atan, Sinh, Cosh, Tanh, Pow, Atan2, Abs,
        Sign, Floor, Ceil, Round, Trunc, Mod, Min, Max, Dot, Cross, Len, ToInt, ToFloat,
    ]
};

fn intrinsic_code(intrinsic: KernelIntrinsic) -> Option<u64> {
    INTRINSICS
        .iter()
        .position(|&known| known == intrinsic)
        .map(|code| code as u64)
}

/// a number as the frame holds it: its word and its kind
fn number(word: u64, kind: u64) -> KVal {
    match kind as i64 {
        KIND_INT => KVal::Int(word as i64),
        _ => KVal::Float(f64::from_bits(word)),
    }
}

/// an intrinsic on numbers whose kinds are only known at run time; `out`
/// takes the result's word and kind, and a result that is not a number
/// faults so the call reruns on the typed machine
extern "C" fn shim_native(
    code: u32,
    arity: u32,
    a: u64,
    a_kind: u64,
    b: u64,
    b_kind: u64,
    out: *mut u64,
) -> i32 {
    let args = [number(a, a_kind), number(b, b_kind)];
    let (value, kind) = match run::native(INTRINSICS[code as usize], &args[..arity as usize]) {
        Ok(KVal::Int(n)) => (n as u64, KIND_INT),
        Ok(KVal::Float(f)) => (f.to_bits(), KIND_FLOAT),
        Ok(_) => return fault_code(Fault::Type),
        Err(fault) => return fault_code(fault),
    };
    // safety: `out` is the generated code's own two word stack slot
    unsafe {
        *out = value;
        *out.add(1) = kind as u64;
    }
    0
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
    Native,
}

impl Shim {
    const ALL: [Shim; 11] = [
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
        Shim::Native,
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
            Shim::Native => "mc_native",
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
            Shim::Native => shim_native as *const u8,
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
            Shim::Native => (vec![I32, I32, I64, I64, I64, I64, ptr], I32),
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

fn ret_code(class: Class) -> Option<u64> {
    match class {
        Class::Int => Some(1),
        Class::Float => Some(2),
        Class::Boxed => Some(3),
        Class::Unset | Class::Closure(_) => None,
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
fn shape(spec: &Spec) -> Result<Shape, &'static str> {
    let mut key = Vec::with_capacity(spec.ops.len() * 4 + spec.entry.len() + 2);
    let mut consts = Vec::new();
    key.push(spec.frame_size as u64);
    key.push(spec.entry.len() as u64);
    for class in &spec.entry {
        key.push(class_code(*class).ok_or("a boxed argument or capture")?);
    }
    let opnd = |opnd| opnd_code(opnd).ok_or("a closure operand");
    let ret = |class| ret_code(class).ok_or("a closure result");
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
            TOp::Return { src } => enc!(42, opnd(src)?),
            TOp::BoxI { reg } => enc!(43, reg),
            TOp::Nil { dst } => enc!(44, dst),
            TOp::EmptyList { dst } => enc!(45, dst),
            TOp::Append { list, value } => enc!(46, list, opnd(value)?),
            TOp::MoveB { dst, src } => enc!(47, dst, src),
            TOp::DynBin {
                op,
                dst,
                a,
                b,
                ret: class,
            } => {
                enc!(48, bin_code(op), dst, opnd(a)?, opnd(b)?, ret(class)?)
            }
            TOp::DynNeg { dst, src } => enc!(49, dst, opnd(src)?),
            TOp::DynNot { dst, src } => enc!(50, dst, opnd(src)?),
            TOp::DynJumpIf { cond, negate, to } => enc!(51, opnd(cond)?, negate, to),
            TOp::DynRangeTest { current, stop, to } => {
                enc!(52, opnd(current)?, opnd(stop)?, to)
            }
            TOp::DynInc { reg } => enc!(53, reg),
            TOp::BoxInto { dst, src } => enc!(54, dst, opnd(src)?),
            TOp::DynNative {
                intrinsic,
                dst,
                args,
                arity,
                ret: class,
            } => {
                let native = intrinsic_code(intrinsic).ok_or("an unmodelled native")?;
                enc!(55, native, dst, arity, opnd(args[0])?, ret(class)?);
                if arity > 1 {
                    enc!(opnd(args[1])?);
                }
            }
            TOp::BoxF { reg } => enc!(56, reg),
            TOp::BoxC { reg, .. } => enc!(57, reg),
            // a boxed value the body never reads (`analyse` checks)
            TOp::Capture { dst, .. } | TOp::Default { dst, .. } => enc!(58, dst),
            TOp::Call { .. } => return Err("a call that was not inlined"),
            TOp::Index { .. } => return Err("an index"),
            TOp::Len { .. } => return Err("a len"),
        }
    }
    Ok(Shape {
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

/// what a register of the boxed file holds at one point. the boxed values the
/// jit models are numbers and lists of scalars a body builds and returns, each
/// list living in the frame. native code holds either as a word and a kind
/// word: a number's value and kind, or a list's length and site
#[derive(Clone, Copy, PartialEq, Eq)]
enum Boxed {
    Nothing,
    /// the list built at a site, still growing
    List(u8),
    /// a list from one of a set of sites, a bit each, after a copy of it was
    /// taken or where lists of several sites merge: appending would have to
    /// copy it, so it may only be copied and returned
    Shared(u8),
    /// an int or a float, which of the two known only at run time
    Num,
    /// anything else, which nothing may read
    Opaque,
}

impl Boxed {
    fn sites(self) -> Option<u8> {
        match self {
            Boxed::List(site) => Some(1 << site),
            Boxed::Shared(sites) => Some(sites),
            _ => None,
        }
    }

    fn join(self, other: Boxed) -> Boxed {
        match (self, other) {
            (a, b) if a == b => a,
            // the path that never wrote it would read undefined words
            (Boxed::Nothing, _) | (_, Boxed::Nothing) => Boxed::Opaque,
            (a, b) => match a.sites().zip(b.sites()) {
                Some((a, b)) => Boxed::Shared(a | b),
                None => Boxed::Opaque,
            },
        }
    }
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
        TOp::DynNot { dst, .. }
        | TOp::DynBin {
            dst,
            ret: Class::Int,
            ..
        }
        | TOp::DynNative {
            dst,
            ret: Class::Int,
            ..
        } => (dst, Word::Int),
        TOp::DynBin {
            dst,
            ret: Class::Float,
            ..
        }
        | TOp::DynNative {
            dst,
            ret: Class::Float,
            ..
        } => (dst, Word::Float),
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
        | TOp::RangeTestF { to, .. }
        | TOp::DynJumpIf { to, .. }
        | TOp::DynRangeTest { to, .. } => (Some(pc + 1), Some(to)),
        TOp::Return { .. }
        | TOp::IBin {
            op: BinKind::In, ..
        }
        | TOp::FBin {
            op: BinKind::In, ..
        }
        | TOp::DynBin {
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

/// why the jit leaves a spec to the typed machine
type Decline = &'static str;

const OUTSIDE: Decline = "a register outside the frame";

/// the effect of `op` on the boxed file, and the site of the list it builds if
/// it builds one; an error when it reads a boxed value the jit does not model
fn step_boxed(
    op: &TOp,
    pc: usize,
    site_pcs: &[usize],
    boxed: &mut [Boxed],
) -> Result<Option<u8>, Decline> {
    let at = |boxed: &[Boxed], reg: Reg| boxed.get(reg as usize).copied().ok_or(OUTSIDE);
    let numeric = |boxed: &[Boxed], opnd: Opnd| match opnd {
        Opnd::I(_) | Opnd::F(_) => Ok(()),
        Opnd::B(reg) => match at(boxed, reg)? {
            Boxed::Num => Ok(()),
            Boxed::List(_) | Boxed::Shared(_) => Err("a list read as a number"),
            _ => Err("a boxed read it does not model"),
        },
        Opnd::C(_) => Err("a closure operand"),
    };
    let set = |boxed: &mut [Boxed], reg: Reg, value: Boxed| {
        *boxed.get_mut(reg as usize).ok_or(OUTSIDE)? = value;
        Ok(())
    };
    // a copy of a boxed register: a number is copied, a list is shared
    let copy = |boxed: &mut [Boxed], dst: Reg, src: Reg| {
        let value = match at(boxed, src)? {
            Boxed::Num => Boxed::Num,
            held => {
                let sites = held
                    .sites()
                    .ok_or("a copy of a boxed value it does not model")?;
                // both now name the list: neither may grow it
                for reg in boxed.iter_mut() {
                    if let Boxed::List(site) = *reg
                        && sites & 1 << site != 0
                    {
                        *reg = Boxed::Shared(1 << site);
                    }
                }
                Boxed::Shared(sites)
            }
        };
        set(boxed, dst, value)?;
        Ok(None)
    };
    Ok(match *op {
        TOp::BoxI { reg } | TOp::BoxF { reg } => {
            set(boxed, reg, Boxed::Num)?;
            None
        }
        TOp::BoxC { reg: dst, .. }
        | TOp::Nil { dst }
        | TOp::Capture { dst, .. }
        | TOp::Default { dst, .. } => {
            set(boxed, dst, Boxed::Opaque)?;
            None
        }
        TOp::EmptyList { dst } => {
            let site = site_pcs
                .iter()
                .position(|&at| at == pc)
                .ok_or("a list site it did not count")? as u8;
            set(boxed, dst, Boxed::List(site))?;
            Some(site)
        }
        TOp::Append { list, value } => {
            numeric(boxed, value).map_err(|_| "a list element that is not a number")?;
            match at(boxed, list)? {
                Boxed::List(site) => Some(site),
                Boxed::Shared(_) => return Err("an append to a copied or merged list"),
                _ => return Err("an append to a list it does not model"),
            }
        }
        TOp::MoveB { dst, src } => copy(boxed, dst, src)?,
        TOp::BoxInto { dst, src } => match src {
            Opnd::B(src) => copy(boxed, dst, src)?,
            Opnd::I(_) | Opnd::F(_) => {
                set(boxed, dst, Boxed::Num)?;
                None
            }
            Opnd::C(_) => return Err("a closure operand"),
        },
        TOp::Return { src: Opnd::B(reg) } => match at(boxed, reg)? {
            Boxed::Num | Boxed::List(_) | Boxed::Shared(_) => None,
            _ => return Err("a return of a boxed value it does not model"),
        },
        TOp::DynBin { dst, a, b, ret, .. } => {
            numeric(boxed, a)?;
            numeric(boxed, b)?;
            if ret == Class::Boxed {
                set(boxed, dst, Boxed::Num)?;
            }
            None
        }
        TOp::DynNeg { dst, src } => {
            numeric(boxed, src)?;
            set(boxed, dst, Boxed::Num)?;
            None
        }
        TOp::DynNot { src: cond, .. } | TOp::DynJumpIf { cond, .. } => {
            numeric(boxed, cond)?;
            None
        }
        TOp::DynRangeTest { current, stop, .. } => {
            numeric(boxed, current)?;
            numeric(boxed, stop)?;
            None
        }
        TOp::DynInc { reg } => {
            numeric(boxed, Opnd::B(reg))?;
            None
        }
        TOp::DynNative {
            dst,
            args,
            arity,
            ret,
            ..
        } => {
            for arg in &args[..arity as usize] {
                numeric(boxed, *arg)?;
            }
            if ret == Class::Boxed {
                set(boxed, dst, Boxed::Num)?;
            }
            None
        }
        TOp::Call { .. } => return Err("a call that was not inlined"),
        TOp::Index { .. } => return Err("an index"),
        TOp::Len { .. } => return Err("a len"),
        _ => None,
    })
}

/// what codegen needs to know about each op beyond the op itself
struct Facts {
    /// for a `MoveS`, whether it copies a float
    float_moves: Vec<bool>,
    /// for an `EmptyList` or an `Append`, the site of its list
    sites: Vec<Option<u8>>,
}

/// the facts codegen needs, checked along with every boxed read. an error
/// when a copy's source is not one class on every path, when a boxed value
/// other than a list of scalars or a number is read, or when a list could be
/// built twice in one call (one list per site and call is what lets it live in
/// the frame)
fn analyse(spec: &Spec) -> Result<Facts, Decline> {
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
    if lists.len() > LIST_SITES {
        return Err("more list sites than it models");
    }
    if lists.iter().any(|&pc| reaches_itself(ops, pc as u32)) {
        return Err("a list built in a loop");
    }

    let mut states: Vec<Option<State>> = vec![None; ops.len()];
    let mut worklist = Vec::new();
    if !ops.is_empty() {
        states[0] = Some(entry);
        worklist.push(0u32);
    }
    while let Some(pc) = worklist.pop() {
        let Some(mut state) = states[pc as usize].clone() else {
            continue;
        };
        let op = &ops[pc as usize];
        if let Some((reg, word)) = writes(op, &state.words) {
            *state.words.get_mut(reg as usize).ok_or(OUTSIDE)? = word;
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
        sites: vec![None; ops.len()],
    };
    for (pc, op) in ops.iter().enumerate() {
        let Some(state) = &states[pc] else {
            continue;
        };
        if let TOp::MoveS { src, .. } = op {
            facts.float_moves[pc] = match state.words.get(*src as usize).ok_or(OUTSIDE)? {
                Word::Int => false,
                Word::Float => true,
                Word::Nothing | Word::Mixed => {
                    return Err("a copy of a register not one class on every path");
                }
            };
        }
        let mut boxed = state.boxed.clone();
        facts.sites[pc] = step_boxed(op, pc, &lists, &mut boxed)?;
    }
    Ok(facts)
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

/// the per-register variables, one for each class a register is used at, and
/// a word and a kind for a number in the boxed file
struct Registers {
    ints: Vec<Option<Variable>>,
    floats: Vec<Option<Variable>>,
    nums: Vec<Option<(Variable, Variable)>>,
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

    fn num_vars(&mut self, builder: &mut FunctionBuilder, reg: Reg) -> (Variable, Variable) {
        *self.nums[reg as usize].get_or_insert_with(|| {
            (
                builder.declare_var(types::I64),
                builder.declare_var(types::I64),
            )
        })
    }

    fn num(&mut self, builder: &mut FunctionBuilder, reg: Reg) -> (Value, Kind) {
        let (word, kind) = self.num_vars(builder, reg);
        (builder.use_var(word), Kind::Dyn(builder.use_var(kind)))
    }

    fn set_num(&mut self, builder: &mut FunctionBuilder, reg: Reg, (word, kind): (Value, Kind)) {
        let vars = self.num_vars(builder, reg);
        let kind = kind.value(builder);
        builder.def_var(vars.0, word);
        builder.def_var(vars.1, kind);
    }

    /// a number operand as a word and a kind: scalar registers have a kind
    /// known at translation time
    fn operand(&mut self, builder: &mut FunctionBuilder, opnd: Opnd) -> Option<(Value, Kind)> {
        Some(match opnd {
            Opnd::I(reg) => (self.int(builder, reg), Kind::Int),
            Opnd::F(reg) => {
                let x = self.float(builder, reg);
                (bits(builder, x), Kind::Float)
            }
            Opnd::B(reg) => self.num(builder, reg),
            Opnd::C(_) => return None,
        })
    }
}

/// the kind of a number: static for a scalar register, a kind word for a
/// boxed one
#[derive(Clone, Copy)]
enum Kind {
    Int,
    Float,
    Dyn(Value),
}

impl Kind {
    fn of(float: bool) -> Self {
        if float { Kind::Float } else { Kind::Int }
    }

    fn value(self, b: &mut FunctionBuilder) -> Value {
        match self {
            Kind::Int => b.ins().iconst(types::I64, KIND_INT),
            Kind::Float => b.ins().iconst(types::I64, KIND_FLOAT),
            Kind::Dyn(kind) => kind,
        }
    }

    /// whether the number is an int, as a flag, or statically
    fn is_int(self, b: &mut FunctionBuilder) -> Cond {
        match self {
            Kind::Int => Cond::Known(true),
            Kind::Float => Cond::Known(false),
            Kind::Dyn(kind) => Cond::Flag(b.ins().icmp_imm_s(IntCC::Equal, kind, KIND_INT)),
        }
    }
}

/// a condition known at translation time or computed at run time
#[derive(Clone, Copy)]
enum Cond {
    Known(bool),
    Flag(Value),
}

fn bits(b: &mut FunctionBuilder, x: Value) -> Value {
    b.ins().bitcast(types::I64, MemFlagsData::new(), x)
}

fn from_bits(b: &mut FunctionBuilder, x: Value) -> Value {
    b.ins().bitcast(types::F64, MemFlagsData::new(), x)
}

/// a number's value as a float, the promotion `run::binary` makes
fn promoted(b: &mut FunctionBuilder, (x, kind): (Value, Kind)) -> Value {
    match kind.is_int(b) {
        Cond::Known(true) => b.ins().fcvt_from_sint(types::F64, x),
        Cond::Known(false) => from_bits(b, x),
        Cond::Flag(is_int) => {
            let converted = b.ins().fcvt_from_sint(types::F64, x);
            let float = from_bits(b, x);
            b.ins().select(is_int, converted, float)
        }
    }
}

/// a value computed one way for an int and another for a float, which gets
/// its word as a float
fn by_kind(
    b: &mut FunctionBuilder,
    (x, kind): (Value, Kind),
    int: impl FnOnce(&mut FunctionBuilder, Value) -> Value,
    float: impl FnOnce(&mut FunctionBuilder, Value) -> Value,
) -> Value {
    match kind.is_int(b) {
        Cond::Known(true) => int(b, x),
        Cond::Known(false) => {
            let x = from_bits(b, x);
            float(b, x)
        }
        Cond::Flag(is_int) => {
            let as_int = int(b, x);
            let f = from_bits(b, x);
            let as_float = float(b, f);
            b.ins().select(is_int, as_int, as_float)
        }
    }
}

/// the fault blocks, one per fault code
#[derive(Default)]
struct Faults(FxHashMap<i32, Block>);

impl Faults {
    fn block(&mut self, b: &mut FunctionBuilder, fault: Fault) -> Block {
        *self
            .0
            .entry(fault_code(fault))
            .or_insert_with(|| b.create_block())
    }

    /// leave with `fault` when `cond` is set
    fn guard(&mut self, b: &mut FunctionBuilder, cond: Value, fault: Fault) {
        let fault = self.block(b, fault);
        let ok = b.create_block();
        b.ins().brif(cond, fault, &[], ok, &[]);
        b.switch_to_block(ok);
    }
}

fn compile(spec: &Spec, facts: &Facts) -> Option<Compiled> {
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
            facts,
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

/// whether two numbers are both ints, the case `run::binary` keeps as ints
fn both_ints(b: &mut FunctionBuilder, x: Kind, y: Kind) -> Cond {
    match (x.is_int(b), y.is_int(b)) {
        (Cond::Known(false), _) | (_, Cond::Known(false)) => Cond::Known(false),
        (Cond::Known(true), other) | (other, Cond::Known(true)) => other,
        (Cond::Flag(x), Cond::Flag(y)) => Cond::Flag(b.ins().band(x, y)),
    }
}

/// a result as a number's word
fn word_of(b: &mut FunctionBuilder, (value, float): (Value, bool)) -> Value {
    if float { bits(b, value) } else { value }
}

impl Codegen<'_> {
    fn pure(&self, b: &mut FunctionBuilder, shim: Shim, args: &[Value]) -> Value {
        let call = b.ins().call(self.refs[shim as usize], args);
        b.inst_results(call)[0]
    }

    /// a call to a shim that can fault: a fault leaves with its code, and the
    /// result is read from the out slot
    fn fallible(
        &self,
        b: &mut FunctionBuilder,
        out: StackSlot,
        shim: Shim,
        args: &[Value],
    ) -> Value {
        let out_ptr = b.ins().stack_addr(self.ptr, out, 0);
        let mut args = args.to_vec();
        args.push(out_ptr);
        let call = b.ins().call(self.refs[shim as usize], &args);
        let code = b.inst_results(call)[0];
        let ok = b.create_block();
        let failed = b.create_block();
        b.ins().brif(code, failed, &[], ok, &[]);
        b.switch_to_block(failed);
        b.ins().return_(&[code]);
        b.switch_to_block(ok);
        b.ins().stack_load(self.ptr, types::I64, out, 0)
    }

    /// `x op y` on ints as `run::int_binary` computes it: the result and
    /// whether it is a float. `None` for `in`, which always faults
    fn int_binary(
        &self,
        b: &mut FunctionBuilder,
        faults: &mut Faults,
        out: StackSlot,
        op: BinKind,
        x: Value,
        y: Value,
    ) -> Option<(Value, bool)> {
        let compare = |b: &mut FunctionBuilder, cc| {
            let cond = b.ins().icmp(cc, x, y);
            b.ins().uextend(types::I64, cond)
        };
        Some(match op {
            BinKind::Add => (b.ins().iadd(x, y), false),
            BinKind::Sub => (b.ins().isub(x, y), false),
            BinKind::Mul => (b.ins().imul(x, y), false),
            BinKind::Div => {
                let is_zero = b.ins().icmp_imm_s(IntCC::Equal, y, 0);
                faults.guard(b, is_zero, Fault::DivisionByZero);
                let fx = b.ins().fcvt_from_sint(types::F64, x);
                let fy = b.ins().fcvt_from_sint(types::F64, y);
                (b.ins().fdiv(fx, fy), true)
            }
            BinKind::IntDiv => {
                let code = b.ins().iconst(types::I32, bin_code(op) as i64);
                (self.fallible(b, out, Shim::IntBinary, &[code, x, y]), false)
            }
            BinKind::Power => {
                // `(x as f64).powf(y as f64)`, which cannot fault
                let fx = b.ins().fcvt_from_sint(types::F64, x);
                let fy = b.ins().fcvt_from_sint(types::F64, y);
                (self.pure(b, Shim::FloatPow, &[fx, fy]), true)
            }
            BinKind::Lt => (compare(b, IntCC::SignedLessThan), false),
            BinKind::Le => (compare(b, IntCC::SignedLessThanOrEqual), false),
            BinKind::Gt => (compare(b, IntCC::SignedGreaterThan), false),
            BinKind::Ge => (compare(b, IntCC::SignedGreaterThanOrEqual), false),
            BinKind::Eq => (compare(b, IntCC::Equal), false),
            BinKind::Ne => (compare(b, IntCC::NotEqual), false),
            BinKind::In => return None,
        })
    }

    /// `x op y` on floats as `run::float_binary` computes it, like
    /// `int_binary`
    fn float_binary(
        &self,
        b: &mut FunctionBuilder,
        faults: &mut Faults,
        out: StackSlot,
        op: BinKind,
        x: Value,
        y: Value,
    ) -> Option<(Value, bool)> {
        let compare = |b: &mut FunctionBuilder, cc| {
            let cond = b.ins().fcmp(cc, x, y);
            b.ins().uextend(types::I64, cond)
        };
        Some(match op {
            BinKind::Add => (b.ins().fadd(x, y), true),
            BinKind::Sub => (b.ins().fsub(x, y), true),
            BinKind::Mul => (b.ins().fmul(x, y), true),
            BinKind::Div => {
                let zero = b.ins().f64const(0.0);
                let is_zero = b.ins().fcmp(FloatCC::Equal, y, zero);
                faults.guard(b, is_zero, Fault::DivisionByZero);
                (b.ins().fdiv(x, y), true)
            }
            BinKind::IntDiv => {
                let code = b.ins().iconst(types::I32, bin_code(op) as i64);
                (
                    self.fallible(b, out, Shim::FloatBinary, &[code, x, y]),
                    false,
                )
            }
            BinKind::Power => (self.pure(b, Shim::FloatPow, &[x, y]), true),
            BinKind::Lt => (compare(b, FloatCC::LessThan), false),
            BinKind::Le => (compare(b, FloatCC::LessThanOrEqual), false),
            BinKind::Gt => (compare(b, FloatCC::GreaterThan), false),
            BinKind::Ge => (compare(b, FloatCC::GreaterThanOrEqual), false),
            BinKind::Eq => (compare(b, FloatCC::Equal), false),
            BinKind::Ne => (compare(b, FloatCC::NotEqual), false),
            BinKind::In => return None,
        })
    }

    /// `x op y` on numbers as `run::binary` computes it: two ints take the int
    /// path and anything else is promoted to floats, branching on the kinds
    /// only where they are not known. the result is a word and its kind
    fn number_binary(
        &self,
        b: &mut FunctionBuilder,
        faults: &mut Faults,
        out: StackSlot,
        op: BinKind,
        x: (Value, Kind),
        y: (Value, Kind),
    ) -> Option<(Value, Kind)> {
        let ints = |b: &mut FunctionBuilder, faults: &mut Faults| {
            let result = self.int_binary(b, faults, out, op, x.0, y.0)?;
            Some((word_of(b, result), result.1))
        };
        let floats = |b: &mut FunctionBuilder, faults: &mut Faults| {
            let (fx, fy) = (promoted(b, x), promoted(b, y));
            let result = self.float_binary(b, faults, out, op, fx, fy)?;
            Some((word_of(b, result), result.1))
        };
        Some(match both_ints(b, x.1, y.1) {
            Cond::Known(true) => {
                let (value, float) = ints(b, faults)?;
                (value, Kind::of(float))
            }
            Cond::Known(false) => {
                let (value, float) = floats(b, faults)?;
                (value, Kind::of(float))
            }
            Cond::Flag(both) => {
                let result = b.declare_var(types::I64);
                let (int_block, float_block, join) =
                    (b.create_block(), b.create_block(), b.create_block());
                b.ins().brif(both, int_block, &[], float_block, &[]);
                b.switch_to_block(int_block);
                let (value, int_float) = ints(b, faults)?;
                b.def_var(result, value);
                b.ins().jump(join, &[]);
                b.switch_to_block(float_block);
                let (value, float_float) = floats(b, faults)?;
                b.def_var(result, value);
                b.ins().jump(join, &[]);
                b.switch_to_block(join);
                let value = b.use_var(result);
                let kind = if int_float == float_float {
                    Kind::of(int_float)
                } else {
                    let (int, float) = (Kind::of(int_float), Kind::of(float_float));
                    let (int, float) = (int.value(b), float.value(b));
                    Kind::Dyn(b.ins().select(both, int, float))
                };
                (value, kind)
            }
        })
    }

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
            nums: vec![None; frame_size],
        };
        let mut faults = Faults::default();
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
        let items = b.ins().iadd_imm_s(frame, word(layout.items) as i64);
        let out = b.create_sized_stack_slot(StackSlotData::new(StackSlotKind::ExplicitSlot, 16, 3));
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
            macro_rules! fallible {
                ($shim:expr, $args:expr) => {
                    self.fallible(b, out, $shim, &$args)
                };
            }
            macro_rules! pure {
                ($shim:expr, $args:expr) => {
                    self.pure(b, $shim, &$args)
                };
            }
            macro_rules! guard_zero {
                ($is_zero:expr) => {{
                    let is_zero = $is_zero;
                    faults.guard(b, is_zero, Fault::DivisionByZero);
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
                TOp::BoxI { reg } => {
                    let x = int!(reg);
                    regs.set_num(b, reg, (x, Kind::Int))
                }
                TOp::BoxF { reg } => {
                    let x = float!(reg);
                    let x = bits(b, x);
                    regs.set_num(b, reg, (x, Kind::Float))
                }
                // boxed values nothing reads (`analyse` checks)
                TOp::BoxC { .. } | TOp::Nil { .. } | TOp::Capture { .. } | TOp::Default { .. } => {}
                // a copy of a list is its length and site, there being one
                // list per site and call
                TOp::MoveB { dst, src } => {
                    let pair = regs.num(b, src);
                    regs.set_num(b, dst, pair);
                }
                TOp::BoxInto { dst, src } => {
                    let pair = regs.operand(b, src)?;
                    regs.set_num(b, dst, pair);
                }
                TOp::DynBin {
                    op: BinKind::In, ..
                } => {
                    let fault = faults.block(b, Fault::Type);
                    b.ins().jump(fault, &[]);
                    open = false;
                }
                TOp::DynBin {
                    op,
                    dst,
                    a,
                    b: rhs,
                    ret,
                } => {
                    let x = regs.operand(b, a)?;
                    let y = regs.operand(b, rhs)?;
                    let (value, kind) = self.number_binary(b, &mut faults, out, op, x, y)?;
                    match (ret, kind) {
                        (Class::Boxed, _) => regs.set_num(b, dst, (value, kind)),
                        (Class::Int, Kind::Int) => regs.set_int(b, dst, value),
                        (Class::Float, Kind::Float) => {
                            let value = from_bits(b, value);
                            regs.set_float(b, dst, value)
                        }
                        _ => return None,
                    }
                }
                TOp::DynNeg { dst, src } => {
                    let (x, kind) = regs.operand(b, src)?;
                    let negated = by_kind(
                        b,
                        (x, kind),
                        |b, x| b.ins().ineg(x),
                        |b, x| {
                            let negated = b.ins().fneg(x);
                            bits(b, negated)
                        },
                    );
                    regs.set_num(b, dst, (negated, kind))
                }
                TOp::DynNot { dst, src } => {
                    let number = regs.operand(b, src)?;
                    let zero = by_kind(
                        b,
                        number,
                        |b, x| b.ins().icmp_imm_s(IntCC::Equal, x, 0),
                        |b, x| {
                            let zero = b.ins().f64const(0.0);
                            b.ins().fcmp(FloatCC::Equal, x, zero)
                        },
                    );
                    set_int!(dst, flag!(zero))
                }
                TOp::DynJumpIf { cond, negate, to } => {
                    // `truthy`, or its negation: a nan is truthy
                    let (int_cc, float_cc) = if negate {
                        (IntCC::Equal, FloatCC::Equal)
                    } else {
                        (IntCC::NotEqual, FloatCC::NotEqual)
                    };
                    let number = regs.operand(b, cond)?;
                    branch!(
                        by_kind(
                            b,
                            number,
                            |b, x| b.ins().icmp_imm_s(int_cc, x, 0),
                            |b, x| {
                                let zero = b.ins().f64const(0.0);
                                b.ins().fcmp(float_cc, x, zero)
                            },
                        ),
                        to
                    )
                }
                TOp::DynRangeTest { current, stop, to } => {
                    // `!(current < stop)` as `run::binary` compares them
                    let x = regs.operand(b, current)?;
                    let y = regs.operand(b, stop)?;
                    let both = both_ints(b, x.1, y.1);
                    let int_done = |b: &mut FunctionBuilder| {
                        b.ins().icmp(IntCC::SignedGreaterThanOrEqual, x.0, y.0)
                    };
                    let float_done = |b: &mut FunctionBuilder| {
                        let (fx, fy) = (promoted(b, x), promoted(b, y));
                        b.ins().fcmp(FloatCC::UnorderedOrGreaterThanOrEqual, fx, fy)
                    };
                    let done = match both {
                        Cond::Known(true) => int_done(b),
                        Cond::Known(false) => float_done(b),
                        Cond::Flag(both) => {
                            let (int, float) = (int_done(b), float_done(b));
                            b.ins().select(both, int, float)
                        }
                    };
                    branch!(done, to)
                }
                TOp::DynInc { reg } => {
                    let (x, kind) = regs.num(b, reg);
                    let incremented = by_kind(
                        b,
                        (x, kind),
                        |b, x| b.ins().iadd_imm_s(x, 1),
                        |b, x| {
                            let one = b.ins().f64const(1.0);
                            let sum = b.ins().fadd(x, one);
                            bits(b, sum)
                        },
                    );
                    regs.set_num(b, reg, (incremented, kind))
                }
                TOp::DynNative {
                    intrinsic,
                    dst,
                    args,
                    arity,
                    ret,
                } => {
                    let code = b
                        .ins()
                        .iconst(types::I32, intrinsic_code(intrinsic)? as i64);
                    let count = b.ins().iconst(types::I32, arity as i64);
                    let (x, x_kind) = regs.operand(b, args[0])?;
                    let (y, y_kind) = if arity > 1 {
                        regs.operand(b, args[1])?
                    } else {
                        (b.ins().iconst(types::I64, 0), Kind::Int)
                    };
                    let (x_kind, y_kind) = (x_kind.value(b), y_kind.value(b));
                    let value =
                        self.fallible(b, out, Shim::Native, &[code, count, x, x_kind, y, y_kind]);
                    let kind = b.ins().stack_load(self.ptr, types::I64, out, 8);
                    let expected = match ret {
                        Class::Boxed => {
                            regs.set_num(b, dst, (value, Kind::Dyn(kind)));
                            None
                        }
                        Class::Int => Some(KIND_INT),
                        Class::Float => Some(KIND_FLOAT),
                        Class::Unset | Class::Closure(_) => return None,
                    };
                    if let Some(expected) = expected {
                        // the typed machine's `store` into a scalar register
                        let wrong = b.ins().icmp_imm_s(IntCC::NotEqual, kind, expected);
                        faults.guard(b, wrong, Fault::Type);
                        if expected == KIND_INT {
                            regs.set_int(b, dst, value)
                        } else {
                            let value = from_bits(b, value);
                            regs.set_float(b, dst, value)
                        }
                    }
                }
                TOp::EmptyList { dst } => {
                    let site = self.facts.sites[pc]?;
                    let zero = b.ins().iconst(types::I64, 0);
                    let kind = b.ins().iconst(types::I64, KIND_LIST + site as i64);
                    regs.set_num(b, dst, (zero, Kind::Dyn(kind)));
                }
                TOp::Append { list, value } => {
                    let site = self.facts.sites[pc]?;
                    let (count, _) = regs.num_vars(b, list);
                    let n = b.use_var(count);
                    let full =
                        b.ins()
                            .icmp_imm_s(IntCC::SignedGreaterThanOrEqual, n, LIST_CAP as i64);
                    let fault = faults.block(b, Fault::Type);
                    let room = b.create_block();
                    b.ins().brif(full, fault, &[], room, &[]);
                    b.switch_to_block(room);
                    let (value, kind) = regs.operand(b, value)?;
                    let offset = b.ins().ishl_imm_s(n, 3);
                    let at = b.ins().iadd(items, offset);
                    let first = word(Layout::site(site));
                    b.ins().store(flags, value, at, first);
                    let kind = kind.value(b);
                    b.ins().store(flags, kind, at, first + word(LIST_CAP));
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
                    match self.int_binary(b, &mut faults, out, op, x, y) {
                        Some((value, true)) => regs.set_float(b, dst, value),
                        Some((value, false)) => regs.set_int(b, dst, value),
                        None => {
                            let fault = faults.block(b, Fault::Type);
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
                    match self.float_binary(b, &mut faults, out, op, x, y) {
                        Some((value, true)) => regs.set_float(b, dst, value),
                        Some((value, false)) => regs.set_int(b, dst, value),
                        None => {
                            let fault = faults.block(b, Fault::Type);
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
                    let (value, kind) = regs.operand(b, src)?;
                    let kind = kind.value(b);
                    b.ins().store(flags, value, frame, word(layout.result));
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
        let fault = faults.block(b, Fault::Type);
        b.ins().jump(fault, &[]);

        for edge in back_edges {
            b.switch_to_block(edge.block);
            let left = b.use_var(budget);
            let left = b.ins().iadd_imm_s(left, -edge.weight);
            b.def_var(budget, left);
            let exhausted = b.ins().icmp_imm_s(IntCC::SignedLessThanOrEqual, left, 0);
            let spent = faults.block(b, Fault::Budget);
            let running = b.create_block();
            b.ins().brif(exhausted, spent, &[], running, &[]);
            b.switch_to_block(running);
            let raised = b.ins().atomic_load(types::I8, flags, abort);
            let aborted = faults.block(b, Fault::Aborted);
            b.ins()
                .brif(raised, aborted, &[], blocks[edge.to as usize]?, &[]);
        }
        for (code, block) in faults.0 {
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
        let shape = shape(spec)
            .inspect_err(|reason| {
                if dump_kernels() {
                    eprintln!("jit declined: {reason}");
                }
            })
            .ok()?;
        let compiled = match self.compiled.get(&shape.key) {
            Some(cached) => cached.clone(),
            None => {
                if self.compiled.len() >= CACHE_MAX {
                    self.compiled.clear();
                }
                let started = Instant::now();
                let compiled = analyse(spec)
                    .and_then(|facts| compile(spec, &facts).ok_or("code generation failed"))
                    .inspect_err(|reason| {
                        if dump_kernels() {
                            eprintln!("jit declined: {reason}");
                        }
                    })
                    .ok()
                    .map(Arc::new);
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
                let count = words[layout.result] as usize;
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
        run::Vm,
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
        /// the jit, the typed machine and the dynamic machine agree on every
        /// call
        fn agree(&self, calls: impl IntoIterator<Item = Vec<KVal>>) {
            let spec = self.program.spec(self.spec);
            let mut jit = JitVm::new(Arc::clone(&self.entry), spec, &self.arena, None);
            let mut tvm = TVm::new();
            let mut vm = Vm::new();
            for args in calls {
                let native = jit.call(spec, &args);
                let typed = tvm.call(&self.program, &self.arena, self.spec, &args);
                let dynamic = vm.call(&self.arena, spec.closure, &args);
                match (&typed, &dynamic) {
                    (Ok(a), Ok(b)) => assert!(KVal::strictly_equal(a, b), "{args:?}"),
                    (Err(a), Err(b)) => assert_eq!(a, b, "{args:?}"),
                    _ => panic!("{args:?}: typed {typed:?}, dynamic {dynamic:?}"),
                }
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

    fn bin(op: BinKind, dst: u16, a: u16, b: u16) -> KOp {
        KOp::Bin {
            op,
            dst,
            a,
            b,
            take: false,
        }
    }

    impl Compiled {
        fn has_dynamic_ops(&self) -> bool {
            self.program
                .spec(self.spec)
                .ops
                .iter()
                .any(|op| matches!(op, TOp::DynBin { .. }))
        }

        fn call(&self, args: &[KVal]) -> Result<KVal, Fault> {
            let spec = self.program.spec(self.spec);
            JitVm::new(Arc::clone(&self.entry), spec, &self.arena, None).call(spec, args)
        }
    }

    /// `var sum = 0; for i in range(0, n) { if i < k { sum = sum * 3 + i }
    /// else { sum = sum + x } }` over the arguments `n, k, x, d`, with the sum
    /// in register 4 and the next op at 13: an int until the first float
    fn mixed_sum() -> Vec<KOp> {
        vec![
            KOp::Int { dst: 4, value: 0 },
            KOp::Int { dst: 5, value: 0 },
            bin(BinKind::Lt, 6, 5, 0),
            KOp::JumpIfNot { cond: 6, to: 13 },
            bin(BinKind::Lt, 6, 5, 1),
            KOp::JumpIfNot { cond: 6, to: 10 },
            KOp::Int { dst: 7, value: 3 },
            bin(BinKind::Mul, 4, 4, 7),
            bin(BinKind::Add, 4, 4, 5),
            KOp::Jump { to: 11 },
            bin(BinKind::Add, 4, 4, 2),
            KOp::Inc { reg: 5 },
            KOp::Jump { to: 2 },
        ]
    }

    fn mixed_calls(seed: u64, count: usize, max_n: i64) -> impl Iterator<Item = Vec<KVal>> {
        samples(seed, count)
            .zip(samples(seed + 100, count))
            .map(move |((n, x), (k, _))| {
                vec![
                    KVal::Int(n.rem_euclid(max_n)),
                    KVal::Int(k.rem_euclid(max_n + 2)),
                    KVal::Float(x),
                    KVal::Int(k % 4),
                ]
            })
    }

    fn mixed_sample() -> [KVal; 4] {
        [KVal::Int(3), KVal::Int(1), KVal::Float(0.5), KVal::Int(2)]
    }

    #[test]
    fn an_int_accumulator_promoted_by_a_float_compiles_and_keeps_its_kind() {
        // sum = 0; i = 0; while i < n { sum = sum + x; i += 1 }; sum
        let ops = vec![
            KOp::Int { dst: 2, value: 0 },
            KOp::Int { dst: 3, value: 0 },
            bin(BinKind::Lt, 4, 3, 0),
            KOp::JumpIfNot { cond: 4, to: 7 },
            bin(BinKind::Add, 2, 2, 1),
            KOp::Inc { reg: 3 },
            KOp::Jump { to: 2 },
            KOp::Return { src: 2 },
        ];
        let jit = compiled(2, 5, ops, &[KVal::Int(3), KVal::Float(0.5)]);
        assert!(jit.has_dynamic_ops());
        let zero = jit.call(&[KVal::Int(0), KVal::Float(0.5)]).unwrap();
        assert!(KVal::strictly_equal(&zero, &KVal::Int(0)), "{zero:?}");
        let two = jit.call(&[KVal::Int(2), KVal::Float(0.5)]).unwrap();
        assert!(KVal::strictly_equal(&two, &KVal::Float(1.0)), "{two:?}");
        jit.agree(samples(12, 500).map(|(n, x)| vec![KVal::Int(n.rem_euclid(40)), KVal::Float(x)]));
    }

    #[test]
    fn a_boxed_int_stays_an_int_and_wraps_like_the_typed_machine() {
        let mut ops = mixed_sum();
        ops.push(KOp::Return { src: 4 });
        let jit = compiled(4, 8, ops, &mixed_sample());
        assert!(jit.has_dynamic_ops());
        // k >= n keeps every step an int: 3^60 wraps
        let all_ints = jit
            .call(&[KVal::Int(60), KVal::Int(60), KVal::Float(0.5), KVal::Int(0)])
            .unwrap();
        let mut expected = 0i64;
        for i in 0..60 {
            expected = expected.wrapping_mul(3).wrapping_add(i);
        }
        assert!(KVal::strictly_equal(&all_ints, &KVal::Int(expected)));
        jit.agree(mixed_calls(13, 1000, 70));
    }

    #[test]
    fn arithmetic_and_division_by_zero_on_boxed_numbers_match() {
        // (sum / d + sum // d + sum ^ d - sum) with d possibly zero, on an int
        // sum and on a float one
        let mut ops = mixed_sum();
        ops.extend([
            bin(BinKind::Div, 8, 4, 3),
            bin(BinKind::IntDiv, 9, 4, 3),
            bin(BinKind::Add, 8, 8, 9),
            bin(BinKind::Power, 9, 4, 3),
            bin(BinKind::Add, 8, 8, 9),
            bin(BinKind::Sub, 8, 8, 4),
            KOp::Return { src: 8 },
        ]);
        let jit = compiled(4, 10, ops, &mixed_sample());
        assert!(jit.has_dynamic_ops());
        let int_zero = [KVal::Int(3), KVal::Int(5), KVal::Float(0.5), KVal::Int(0)];
        let float_zero = [KVal::Int(3), KVal::Int(0), KVal::Float(0.5), KVal::Int(0)];
        for args in [int_zero, float_zero] {
            assert_eq!(jit.call(&args).err(), Some(Fault::DivisionByZero));
            jit.agree([args.to_vec()]);
        }
        jit.agree(mixed_calls(14, 1000, 12));
    }

    #[test]
    fn comparisons_and_unary_ops_on_boxed_numbers_match() {
        let mut ops = mixed_sum();
        ops.extend([
            bin(BinKind::Lt, 8, 4, 2),
            bin(BinKind::Eq, 9, 4, 3),
            bin(BinKind::Add, 8, 8, 9),
            bin(BinKind::Ge, 9, 3, 4),
            bin(BinKind::Add, 8, 8, 9),
            KOp::Neg { dst: 9, src: 4 },
            bin(BinKind::Add, 8, 8, 9),
            KOp::Not { dst: 9, src: 4 },
            bin(BinKind::Add, 8, 8, 9),
            // 22: skip the 100 when the sum is truthy
            KOp::JumpIf { cond: 4, to: 25 },
            KOp::Int { dst: 9, value: 100 },
            bin(BinKind::Add, 8, 8, 9),
            bin(BinKind::Ne, 9, 4, 2),
            bin(BinKind::Add, 8, 8, 9),
            KOp::Move { dst: 9, src: 4 },
            KOp::Native {
                intrinsic: KernelIntrinsic::Abs,
                arg_start: 9,
                arg_count: 1,
            },
            bin(BinKind::Add, 8, 8, 9),
            KOp::Move { dst: 9, src: 4 },
            KOp::Native {
                intrinsic: KernelIntrinsic::Floor,
                arg_start: 9,
                arg_count: 1,
            },
            bin(BinKind::Add, 8, 8, 9),
            KOp::Move { dst: 9, src: 4 },
            KOp::Move { dst: 10, src: 2 },
            KOp::Native {
                intrinsic: KernelIntrinsic::Min,
                arg_start: 9,
                arg_count: 2,
            },
            bin(BinKind::Add, 8, 8, 9),
            KOp::Inc { reg: 4 },
            bin(BinKind::Add, 8, 8, 4),
            KOp::Return { src: 8 },
        ]);
        let jit = compiled(4, 11, ops, &mixed_sample());
        let spec = jit.program.spec(jit.spec);
        for dynamic in ["DynNeg", "DynNot", "DynJumpIf", "DynNative", "DynInc"] {
            assert!(
                spec.ops
                    .iter()
                    .any(|op| format!("{op:?}").starts_with(dynamic)),
                "{dynamic}"
            );
        }
        jit.agree(mixed_calls(15, 2000, 12));
    }

    #[test]
    fn a_range_over_a_boxed_number_matches() {
        // for j in range(sum, d) { total = total + k }
        let mut ops = mixed_sum();
        ops.extend([
            KOp::Int { dst: 8, value: 0 },
            KOp::Move { dst: 9, src: 4 },
            KOp::Move { dst: 10, src: 3 },
            // 16
            KOp::RangeTest { current: 9, to: 20 },
            bin(BinKind::Add, 8, 8, 1),
            KOp::Inc { reg: 9 },
            KOp::Jump { to: 16 },
            KOp::Return { src: 8 },
        ]);
        let jit = compiled(4, 11, ops, &mixed_sample());
        assert!(
            jit.program
                .spec(jit.spec)
                .ops
                .iter()
                .any(|op| matches!(op, TOp::DynRangeTest { .. }))
        );
        jit.agree(mixed_calls(16, 1000, 6).map(|mut args| {
            args[2] = KVal::Float(match &args[2] {
                KVal::Float(x) if x.is_finite() => x.rem_euclid(6.0) - 3.0,
                _ => 0.25,
            });
            let KVal::Int(k) = args[1] else {
                unreachable!()
            };
            args[3] = KVal::Int(k * 7 - 20);
            args
        }));
    }

    #[test]
    fn a_register_a_list_reaches_is_declined() {
        // sum = 0; if n { sum = [] }; sum + x
        let ops = vec![
            KOp::Int { dst: 2, value: 0 },
            KOp::JumpIfNot { cond: 0, to: 3 },
            KOp::EmptyList { dst: 2 },
            bin(BinKind::Add, 2, 2, 1),
            KOp::Return { src: 2 },
        ];
        let (arena, id) = closure(2, 3, ops);
        let (program, spec) =
            TypedProgram::specialise(&arena, id, &[KVal::Int(1), KVal::Float(0.5)]).unwrap();
        assert!(
            program
                .spec(spec)
                .ops
                .iter()
                .any(|op| matches!(op, TOp::DynBin { .. }))
        );
        let mut cache = JitCache::new(true);
        assert!(
            cache
                .prepare(program.spec(spec), &mut KernelStats::default())
                .is_none()
        );
    }

    /// `r = n > 0 ? [x; floats] : [n; ints]` then `ops` after the merge at
    /// the op they return, over the arguments `n, x`: each branch builds its
    /// list in a register of its own and moves it into register 4, or builds
    /// it in register 4 directly
    fn merged_lists(floats: usize, ints: usize, direct: bool, after: Vec<KOp>) -> Vec<KOp> {
        let (a, b) = if direct { (4, 4) } else { (3, 5) };
        let mut ops = vec![
            KOp::Int { dst: 2, value: 0 },
            bin(BinKind::Gt, 2, 0, 2),
            KOp::JumpIfNot { cond: 2, to: 0 },
            KOp::EmptyList { dst: a },
        ];
        ops.extend((0..floats).map(|_| KOp::Append { list: a, value: 1 }));
        if !direct {
            ops.push(KOp::Move { dst: 4, src: a });
        }
        let jump = ops.len();
        ops.push(KOp::Jump { to: 0 });
        ops[2] = KOp::JumpIfNot {
            cond: 2,
            to: ops.len() as u32,
        };
        ops.push(KOp::EmptyList { dst: b });
        ops.extend((0..ints).map(|_| KOp::Append { list: b, value: 0 }));
        if !direct {
            ops.push(KOp::Move { dst: 4, src: b });
        }
        ops[jump] = KOp::Jump {
            to: ops.len() as u32,
        };
        ops.extend(after);
        ops
    }

    fn analysed(ops: Vec<KOp>) -> Result<(), Decline> {
        let (arena, id) = closure(2, 7, ops);
        let (program, spec) =
            TypedProgram::specialise(&arena, id, &[KVal::Int(3), KVal::Float(0.5)]).unwrap();
        analyse(program.spec(spec)).map(|_| ())
    }

    #[test]
    fn lists_merged_from_two_branches_match_the_typed_machine() {
        for (floats, ints) in [(3, 3), (4, 1), (0, 2), (LIST_CAP, 2)] {
            for direct in [false, true] {
                let ops = merged_lists(floats, ints, direct, vec![KOp::Return { src: 4 }]);
                let jit = compiled(2, 7, ops, &[KVal::Int(3), KVal::Float(0.5)]);
                jit.agree(samples(12, 100).map(|(n, x)| vec![KVal::Int(n), KVal::Float(x)]));
            }
        }
    }

    #[test]
    fn a_merged_list_read_other_than_by_a_return_is_declined() {
        let index = vec![
            KOp::Int { dst: 6, value: 0 },
            KOp::Index {
                dst: 6,
                list: 4,
                index: 6,
            },
            KOp::Return { src: 6 },
        ];
        let append = vec![KOp::Append { list: 4, value: 0 }, KOp::Return { src: 4 }];
        let add = vec![bin(BinKind::Add, 6, 4, 1), KOp::Return { src: 6 }];
        for (after, reason) in [
            (index, "an index"),
            (append, "an append to a copied or merged list"),
            (add, "a list read as a number"),
        ] {
            for direct in [false, true] {
                assert_eq!(
                    analysed(merged_lists(2, 2, direct, after.clone())),
                    Err(reason)
                );
            }
        }
    }

    #[test]
    fn a_list_built_in_a_loop_is_declined() {
        // i = 0; l = [0]; while i < n { l = [x, i]; i += 1 }; l
        let ops = vec![
            KOp::Int { dst: 2, value: 0 },
            KOp::EmptyList { dst: 4 },
            KOp::Append { list: 4, value: 2 },
            bin(BinKind::Lt, 3, 2, 0),
            KOp::JumpIfNot { cond: 3, to: 10 },
            KOp::EmptyList { dst: 4 },
            KOp::Append { list: 4, value: 1 },
            KOp::Append { list: 4, value: 2 },
            KOp::Inc { reg: 2 },
            KOp::Jump { to: 3 },
            KOp::Return { src: 4 },
        ];
        assert_eq!(analysed(ops), Err("a list built in a loop"));
    }
}
