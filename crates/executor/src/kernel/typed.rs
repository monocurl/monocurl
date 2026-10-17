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
    /// a list of at most `VEC_MAX` numbers whose length and element kinds are
    /// known, held unboxed in the register's vector slot (a point, a colour)
    Vec(Shape),
    /// a boxed list whose every element is a list of this shape (a table of
    /// points or colours captured by a callback); indexing it gives a `Vec`
    Rows(Shape),
    /// a boxed list of numbers of this class (int or float), too long to keep
    /// unboxed (a captured table of samples); indexing it gives that class
    Nums(Scalar),
    /// anything else, kept as a `KVal`
    Boxed,
}

/// the class of every element of a `Nums` list
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Scalar {
    Int,
    Float,
}

/// the longest list a register keeps unboxed
pub const VEC_MAX: usize = 4;

/// what an op knows of one element of an unboxed list
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Int,
    Float,
    /// a number whose kind is only known at run time
    Num,
}

/// the length of an unboxed list and what is known of its elements' kinds:
/// bit i of `ints` marks an int, bit i of `nums` an element of either kind (the
/// vector slot carries the kinds at run time as well)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Shape {
    pub len: u8,
    pub ints: u8,
    pub nums: u8,
}

impl Shape {
    /// the shape of a list of numbers short enough to keep unboxed
    pub fn of(elements: &[KVal]) -> Option<Self> {
        if elements.len() > VEC_MAX {
            return None;
        }
        Self::from_kinds(elements.len(), |i| match elements[i] {
            KVal::Int(_) => Some(Kind::Int),
            KVal::Float(_) => Some(Kind::Float),
            _ => None,
        })
    }

    fn from_kinds(len: usize, kind: impl Fn(usize) -> Option<Kind>) -> Option<Self> {
        let mut shape = Shape {
            len: len as u8,
            ints: 0,
            nums: 0,
        };
        for i in 0..len {
            match kind(i)? {
                Kind::Int => shape.ints |= 1 << i,
                Kind::Float => {}
                Kind::Num => shape.nums |= 1 << i,
            }
        }
        Some(shape)
    }

    pub fn floats(len: u8) -> Self {
        Shape {
            len,
            ints: 0,
            nums: 0,
        }
    }

    pub fn kind(self, i: usize) -> Kind {
        if self.nums & (1 << i) != 0 {
            Kind::Num
        } else if self.ints & (1 << i) != 0 {
            Kind::Int
        } else {
            Kind::Float
        }
    }

    fn all_floats(self) -> bool {
        self.ints == 0 && self.nums == 0
    }

    /// the class every element has, when it is the same statically known one
    fn element(self) -> Option<Class> {
        let all = (1u8 << self.len) - 1;
        match (self.nums, self.ints) {
            (0, 0) => Some(Class::Float),
            (0, ints) if ints == all => Some(Class::Int),
            _ => None,
        }
    }

    fn push(self, class: Class) -> Option<Self> {
        let at = self.len as usize;
        if at >= VEC_MAX {
            return None;
        }
        let ints = match class {
            Class::Int => self.ints | 1 << at,
            Class::Float => self.ints,
            _ => return None,
        };
        Some(Shape {
            len: self.len + 1,
            ints,
            nums: self.nums,
        })
    }

    /// what two shapes of one length have in common: elements whose kinds may
    /// differ become numbers of either kind
    fn union(self, other: Shape) -> Option<Self> {
        (self.len == other.len).then(|| Shape {
            len: self.len,
            ints: self.ints & other.ints,
            nums: self.nums | other.nums | (self.ints ^ other.ints),
        })
    }

    /// each element combined with the matching one of `other` (or a scalar) by
    /// `kind`, the static form of the dynamic machine's promotion rules
    fn map(self, kind: impl Fn(Kind) -> Kind) -> Self {
        Self::from_kinds(self.len as usize, |i| Some(kind(self.kind(i)))).unwrap()
    }

    fn zip(self, other: Shape, kind: impl Fn(Kind, Kind) -> Kind) -> Self {
        Self::from_kinds(self.len as usize, |i| Some(kind(self.kind(i), other.kind(i)))).unwrap()
    }
}

/// the kind of `a + b` or `a - b` (and of `*` with an int or float scalar)
fn sum_kind(a: Kind, b: Kind) -> Kind {
    match (a, b) {
        (Kind::Float, _) | (_, Kind::Float) => Kind::Float,
        (Kind::Int, Kind::Int) => Kind::Int,
        _ => Kind::Num,
    }
}

/// what is known of a scalar operand's kind
fn scalar_kind(class: Class) -> Option<Kind> {
    match class {
        Class::Int => Some(Kind::Int),
        Class::Float => Some(Kind::Float),
        // a boxed operand is only accepted at run time when it is a number
        Class::Boxed => Some(Kind::Num),
        _ => None,
    }
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

    /// `of`, refined for a capture: its value is known for the whole batch, so a
    /// short list of numbers or a table of them gets its shape
    fn of_capture(value: &KVal) -> Self {
        let KVal::List(list) = value else {
            return Class::of(value);
        };
        if let Some(shape) = Shape::of(list) {
            return Class::Vec(shape);
        }
        if list.iter().all(|element| matches!(element, KVal::Float(_))) {
            return Class::Nums(Scalar::Float);
        }
        if list.iter().all(|element| matches!(element, KVal::Int(_))) {
            return Class::Nums(Scalar::Int);
        }
        let mut rows = list.iter().map(|row| match row {
            KVal::List(row) => Shape::of(row),
            _ => None,
        });
        let Some(first) = rows.next().flatten() else {
            return Class::Boxed;
        };
        rows.try_fold(first, |shape, row| shape.union(row?))
            .map_or(Class::Boxed, Class::Rows)
    }

    /// whether the register lives in the vector file
    pub fn is_vec(self) -> bool {
        matches!(self, Class::Vec(_))
    }

    /// the class a register has where two paths meet: paths that disagree
    /// leave it boxed, and the scalar path boxes its value on the way in
    fn join(self, other: Class) -> Class {
        match (self, other) {
            (Class::Unset, x) | (x, Class::Unset) => x,
            (a, b) if a == b => a,
            // the slot keeps its kinds at run time, so only what is known shrinks
            (Class::Vec(a), Class::Vec(b)) => a.union(b).map_or(Class::Boxed, Class::Vec),
            (Class::Rows(a), Class::Rows(b)) => a.union(b).map_or(Class::Boxed, Class::Rows),
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
        args: [Opnd; 3],
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
    /// copies between vector slots
    VMove {
        dst: Reg,
        src: Reg,
    },
    /// a list literal's next element, from the word file
    VPush {
        list: Reg,
        value: Reg,
        at: u8,
        int: bool,
    },
    /// an element of a vector whose elements all have the destination's class
    VIndex {
        dst: Reg,
        list: Reg,
        index: Reg,
        len: u8,
    },
    /// an element of a vector as a box
    VIndexB {
        dst: Reg,
        list: Reg,
        index: Reg,
        len: u8,
    },
    /// an element of a boxed list of numbers of the destination's class
    NIndex {
        dst: Reg,
        list: Reg,
        index: Reg,
    },
    /// a row of a boxed table, unboxed into a vector; elements whose kind the
    /// shape knows are checked
    VRow {
        dst: Reg,
        list: Reg,
        index: Reg,
        shape: Shape,
    },
    /// elementwise `+` or `-` of two vectors of one length
    VBin {
        op: BinKind,
        dst: Reg,
        a: Reg,
        b: Reg,
        len: u8,
    },
    /// `*` or `/` of a vector and a number, the vector on the left or the right;
    /// a boxed scalar that is not a number faults
    VScale {
        op: BinKind,
        dst: Reg,
        v: Reg,
        s: Opnd,
        len: u8,
        vec_left: bool,
    },
    VNeg {
        dst: Reg,
        src: Reg,
        len: u8,
    },
    VDot {
        dst: Reg,
        a: Reg,
        b: Reg,
        len: u8,
    },
    VCross {
        dst: Reg,
        a: Reg,
        b: Reg,
    },
    /// `lerp` of a float vector towards a vector of one length, `t` a float
    VLerp {
        dst: Reg,
        a: Reg,
        b: Reg,
        t: Reg,
        len: u8,
    },
    /// `lerp` of floats
    FLerp {
        dst: Reg,
        a: Reg,
        b: Reg,
        t: Reg,
    },
    /// a vector moved into the boxed file as a list
    BoxV {
        dst: Reg,
        src: Reg,
        len: u8,
    },
    /// a boxed list of this shape moved into the register's vector slot
    UnboxV {
        reg: Reg,
        shape: Shape,
    },
    ReturnV {
        src: Reg,
        len: u8,
    },
}

impl TOp {
    /// whether only the typed machine runs this op
    pub fn is_vector(&self) -> bool {
        matches!(
            self,
            TOp::VMove { .. }
                | TOp::VPush { .. }
                | TOp::VIndex { .. }
                | TOp::VIndexB { .. }
                | TOp::NIndex { .. }
                | TOp::VRow { .. }
                | TOp::VBin { .. }
                | TOp::VScale { .. }
                | TOp::VNeg { .. }
                | TOp::VDot { .. }
                | TOp::VCross { .. }
                | TOp::VLerp { .. }
                | TOp::FLerp { .. }
                | TOp::BoxV { .. }
                | TOp::UnboxV { .. }
                | TOp::ReturnV { .. }
        )
    }
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
    /// registers hold unboxed vectors, which only the typed machine runs
    pub vectors: bool,
}

/// scratch registers a typed frame has beyond the kernel's own: up to three
/// operands of one op converted, plus one
const SCRATCH: u16 = 4;

/// every specialisation a batch's entry point reaches
#[derive(Default)]
pub struct TypedProgram {
    pub specs: Vec<Spec>,
    index: FxHashMap<(ClosureId, Vec<Class>), SpecId>,
    /// short lists of numbers are kept unboxed
    vectors: bool,
}

impl TypedProgram {
    /// specialise `entry` to calls whose arguments have the classes of
    /// `args`; `None` when any body it reaches cannot be typed
    pub fn specialise(
        arena: &ClosureArena,
        entry: ClosureId,
        args: &[KVal],
    ) -> Option<(Self, SpecId)> {
        Self::specialise_with(arena, entry, args, false)
    }

    /// `specialise`, keeping short lists of numbers unboxed: list arithmetic,
    /// `dot` and `lerp` on points and colours then run without allocating, on
    /// the typed machine only
    pub fn specialise_vectors(
        arena: &ClosureArena,
        entry: ClosureId,
        args: &[KVal],
    ) -> Option<(Self, SpecId)> {
        Self::specialise_with(arena, entry, args, true)
    }

    fn specialise_with(
        arena: &ClosureArena,
        entry: ClosureId,
        args: &[KVal],
        vectors: bool,
    ) -> Option<(Self, SpecId)> {
        let sig: Vec<Class> = args.iter().map(Class::of).collect();
        let mut program = TypedProgram {
            vectors,
            ..TypedProgram::default()
        };
        let mut in_progress = Vec::new();
        let id = program.spec_for(arena, entry, sig, &mut in_progress)?;
        Some((program, id))
    }

    /// whether any specialisation keeps vectors unboxed
    pub fn uses_vectors(&self) -> bool {
        self.specs.iter().any(|spec| spec.vectors)
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
        *slot = if program.vectors {
            Class::of_capture(capture)
        } else {
            Class::of(capture)
        };
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
            ret,
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
    let vectors = typed.iter().any(TOp::is_vector)
        || entry
            .iter()
            .chain(sig)
            .chain([&ret])
            .any(|class| matches!(class, Class::Vec(_) | Class::Rows(_) | Class::Nums(_)));
    Some(Spec {
        closure,
        kernel,
        sig: sig.to_vec(),
        ops: typed.into_boxed_slice(),
        ret,
        entry,
        frame_size: (scratch + SCRATCH).max(extent),
        vectors,
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
                Class::Vec(shape) => Some(TOp::BoxV {
                    dst: reg,
                    src: reg,
                    len: shape.len,
                }),
                // already a box
                Class::Rows(_) | Class::Nums(_) | Class::Boxed | Class::Unset => None,
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

/// `bin_class`, with list arithmetic on vectors kept unboxed
fn vec_bin_class(op: BinKind, a: Class, b: Class) -> Class {
    use BinKind::*;
    match (op, a, b) {
        (Add | Sub, Class::Vec(sa), Class::Vec(sb)) if sa.len == sb.len => {
            Class::Vec(sa.zip(sb, sum_kind))
        }
        (Mul | Div, Class::Vec(shape), scalar) | (Mul | Div, scalar, Class::Vec(shape))
            if scalar_kind(scalar).is_some() =>
        {
            Class::Vec(match op {
                // int over int is a float too
                Div => Shape::floats(shape.len),
                _ => {
                    let s = scalar_kind(scalar).unwrap();
                    shape.map(|kind| sum_kind(kind, s))
                }
            })
        }
        _ => bin_class(op, a, b),
    }
}

/// `native_class`, with `cross` and `lerp` of vectors kept unboxed
fn vec_native_class(intrinsic: KernelIntrinsic, args: &[Class]) -> Class {
    match (intrinsic, args) {
        (KernelIntrinsic::Cross, [Class::Vec(a), Class::Vec(b)]) if a.len == 3 && b.len == 3 => {
            Class::Vec(Shape::floats(3))
        }
        (KernelIntrinsic::Lerp, [Class::Float, b, t]) if b.is_scalar() && t.is_scalar() => {
            Class::Float
        }
        (KernelIntrinsic::Lerp, [Class::Vec(a), Class::Vec(b), t])
            if a.all_floats() && a.len == b.len && t.is_scalar() =>
        {
            Class::Vec(Shape::floats(a.len))
        }
        _ => native_class(intrinsic, args),
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
        Cross | KeyframeLerp | Lerp | Fallthrough => Class::Boxed,
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
            state[dst as usize] = vec_bin_class(op, read(&state, a)?, read(&state, b)?)
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
        KOp::EmptyList { dst } => {
            state[dst as usize] = if program.vectors {
                Class::Vec(Shape::floats(0))
            } else {
                Class::Boxed
            }
        }
        KOp::Append { list, value } => {
            let value = read(&state, value)?;
            state[list as usize] = match read(&state, list)? {
                Class::Vec(shape) => shape.push(value).map_or(Class::Boxed, Class::Vec),
                Class::Boxed | Class::Rows(_) | Class::Nums(_) => Class::Boxed,
                _ => return None,
            };
        }
        KOp::Index { dst, list, index } => {
            let index = read(&state, index)?;
            state[dst as usize] = match (read(&state, list)?, index) {
                (Class::Vec(shape), Class::Int) => shape.element().unwrap_or(Class::Boxed),
                (Class::Rows(shape), Class::Int) => Class::Vec(shape),
                (Class::Nums(Scalar::Int), Class::Int) => Class::Int,
                (Class::Nums(Scalar::Float), Class::Int) => Class::Float,
                (Class::Vec(_) | Class::Rows(_) | Class::Nums(_) | Class::Boxed, _) => Class::Boxed,
                _ => return None,
            };
        }
        KOp::Len { dst, src } => {
            if !matches!(
                read(&state, src)?,
                Class::Boxed | Class::Vec(_) | Class::Rows(_) | Class::Nums(_)
            ) {
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
            state[start] = vec_native_class(intrinsic, &args);
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

/// how an op reads a register; a vector has to be boxed first (`operand`)
fn opnd(state: &[Class], reg: Reg) -> Option<Opnd> {
    Some(match read(state, reg)? {
        Class::Int => Opnd::I(reg),
        Class::Float => Opnd::F(reg),
        Class::Closure(id) => Opnd::C(id),
        Class::Boxed | Class::Rows(_) | Class::Nums(_) => Opnd::B(reg),
        Class::Vec(_) | Class::Unset => return None,
    })
}

/// `opnd`, boxing a vector into the scratch register `slot` for an op that
/// only reads boxes
fn operand(state: &[Class], reg: Reg, slot: Reg, out: &mut Emitted) -> Option<Opnd> {
    match read(state, reg)? {
        Class::Vec(shape) => {
            out.ops.push(TOp::BoxV {
                dst: slot,
                src: reg,
                len: shape.len,
            });
            Some(Opnd::B(slot))
        }
        _ => opnd(state, reg),
    }
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

#[allow(clippy::too_many_arguments)]
fn emit(
    program: &mut TypedProgram,
    arena: &ClosureArena,
    op: &KOp,
    state: &[Class],
    scratch: Reg,
    ret: Class,
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
            Class::Boxed | Class::Rows(_) | Class::Nums(_) => TOp::MoveB { dst, src },
            Class::Vec(_) => TOp::VMove { dst, src },
            // the value is static; nothing to copy
            Class::Closure(_) => TOp::Nop,
            Class::Unset => return None,
        },
        KOp::Bin { op, dst, a, b, .. } => {
            let (ca, cb) = (read(state, a)?, read(state, b)?);
            match (ca, cb) {
                (Class::Vec(sa), Class::Vec(sb)) if matches!(op, Add | Sub) && sa.len == sb.len => {
                    TOp::VBin {
                        op,
                        dst,
                        a,
                        b,
                        len: sa.len,
                    }
                }
                (Class::Vec(shape), scalar)
                    if matches!(op, Mul | Div) && scalar_kind(scalar).is_some() =>
                {
                    TOp::VScale {
                        op,
                        dst,
                        v: a,
                        s: opnd(state, b)?,
                        len: shape.len,
                        vec_left: true,
                    }
                }
                (scalar, Class::Vec(shape))
                    if matches!(op, Mul | Div) && scalar_kind(scalar).is_some() =>
                {
                    TOp::VScale {
                        op,
                        dst,
                        v: b,
                        s: opnd(state, a)?,
                        len: shape.len,
                        vec_left: false,
                    }
                }
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
                    a: operand(state, a, scratch, out)?,
                    b: operand(state, b, scratch + 1, out)?,
                    ret: bin_class(op, ca, cb),
                },
            }
        }
        KOp::Neg { dst, src } => match read(state, src)? {
            Class::Int => TOp::INeg { dst, src },
            Class::Float => TOp::FNeg { dst, src },
            Class::Vec(shape) => TOp::VNeg {
                dst,
                src,
                len: shape.len,
            },
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
                src: operand(state, src, scratch, out)?,
            },
        },
        KOp::Jump { to } => TOp::Jump { to },
        KOp::JumpIf { cond, to } => match read(state, cond)? {
            Class::Int => TOp::JumpIfI { cond, to },
            Class::Float => TOp::JumpIfF { cond, to },
            _ => TOp::DynJumpIf {
                cond: operand(state, cond, scratch, out)?,
                negate: false,
                to,
            },
        },
        KOp::JumpIfNot { cond, to } => match read(state, cond)? {
            Class::Int => TOp::JumpIfNotI { cond, to },
            Class::Float => TOp::JumpIfNotF { cond, to },
            _ => TOp::DynJumpIf {
                cond: operand(state, cond, scratch, out)?,
                negate: true,
                to,
            },
        },
        KOp::RangeTest { current, to } => {
            match (read(state, current)?, read(state, current + 1)?) {
                (Class::Int, Class::Int) => TOp::RangeTestI { current, to },
                (Class::Float, Class::Float) => TOp::RangeTestF { current, to },
                _ => TOp::DynRangeTest {
                    current: operand(state, current, scratch, out)?,
                    stop: operand(state, current + 1, scratch + 1, out)?,
                    to,
                },
            }
        }
        KOp::Inc { reg } => match read(state, reg)? {
            Class::Int => TOp::IInc { reg },
            Class::Float => TOp::FInc { reg },
            // a vector is never a loop counter: incrementing it faults, as a list does
            Class::Vec(_) => return None,
            _ => TOp::DynInc { reg },
        },
        // a vector slot needs no clearing: its length lives in the class
        KOp::EmptyList { .. } if program.vectors => TOp::Nop,
        KOp::EmptyList { dst } => TOp::EmptyList { dst },
        KOp::Append { list, value } => match (read(state, list)?, read(state, value)?) {
            (Class::Vec(shape), value_class) if shape.push(value_class).is_some() => TOp::VPush {
                list,
                value,
                at: shape.len,
                int: value_class == Class::Int,
            },
            (Class::Vec(shape), _) => {
                // the list outgrows a vector: it continues as a box
                out.ops.push(TOp::BoxV {
                    dst: list,
                    src: list,
                    len: shape.len,
                });
                TOp::Append {
                    list,
                    value: operand(state, value, scratch, out)?,
                }
            }
            _ => TOp::Append {
                list,
                value: operand(state, value, scratch, out)?,
            },
        },
        KOp::Index { dst, list, index } => match (read(state, list)?, read(state, index)?) {
            (Class::Vec(shape), Class::Int) if shape.element().is_some() => TOp::VIndex {
                dst,
                list,
                index,
                len: shape.len,
            },
            (Class::Nums(_), Class::Int) => TOp::NIndex { dst, list, index },
            (Class::Rows(shape), Class::Int) => TOp::VRow {
                dst,
                list,
                index,
                shape,
            },
            (Class::Vec(shape), Class::Int) => TOp::VIndexB {
                dst,
                list,
                index,
                len: shape.len,
            },
            (Class::Vec(shape), _) => {
                out.ops.push(TOp::BoxV {
                    dst: scratch,
                    src: list,
                    len: shape.len,
                });
                TOp::Index {
                    dst,
                    list: scratch,
                    index: operand(state, index, scratch + 1, out)?,
                }
            }
            _ => TOp::Index {
                dst,
                list,
                index: operand(state, index, scratch, out)?,
            },
        },
        KOp::Len { dst, src } => match read(state, src)? {
            Class::Vec(shape) => TOp::IConst {
                dst,
                value: shape.len as i64,
            },
            _ => TOp::Len { dst, list: src },
        },
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
            let (dst, a, b, c) = (arg_start, arg_start, arg_start + 1, arg_start + 2);
            match (intrinsic, args.as_slice()) {
                (KernelIntrinsic::Dot, [Class::Vec(sa), Class::Vec(sb)]) if sa.len == sb.len => {
                    TOp::VDot {
                        dst,
                        a,
                        b,
                        len: sa.len,
                    }
                }
                (KernelIntrinsic::Cross, [Class::Vec(sa), Class::Vec(sb)])
                    if sa.len == 3 && sb.len == 3 =>
                {
                    TOp::VCross { dst, a, b }
                }
                (KernelIntrinsic::Len, [Class::Vec(shape)]) => TOp::IConst {
                    dst,
                    value: shape.len as i64,
                },
                (KernelIntrinsic::Lerp, [Class::Float, bc, tc])
                    if bc.is_scalar() && tc.is_scalar() =>
                {
                    let b = as_float(state, b, scratch, out)?;
                    let t = as_float(state, c, scratch + 1, out)?;
                    TOp::FLerp { dst, a, b, t }
                }
                (KernelIntrinsic::Lerp, [Class::Vec(sa), Class::Vec(sb), tc])
                    if sa.all_floats() && sa.len == sb.len && tc.is_scalar() =>
                {
                    let t = as_float(state, c, scratch, out)?;
                    TOp::VLerp {
                        dst,
                        a,
                        b,
                        t,
                        len: sa.len,
                    }
                }
                _ => match typed_native(intrinsic, arg_start, &args, state, scratch, out) {
                    Some(typed) => typed,
                    None => {
                        if arg_count > 3 {
                            return None;
                        }
                        let mut operands = [Opnd::I(0); 3];
                        for (i, (slot, reg)) in operands
                            .iter_mut()
                            .zip(start..start + arg_count as usize)
                            .enumerate()
                        {
                            *slot = operand(state, reg as Reg, scratch + i as Reg, out)?;
                        }
                        TOp::DynNative {
                            intrinsic,
                            dst: arg_start,
                            args: operands,
                            arity: arg_count as u8,
                            ret,
                        }
                    }
                },
            }
        }
        KOp::Exit => return None,
        KOp::Return { src } => match (read(state, src)?, ret) {
            (Class::Vec(shape), Class::Vec(returned)) if shape.len == returned.len => {
                TOp::ReturnV {
                    src,
                    len: shape.len,
                }
            }
            _ => TOp::Return {
                src: operand(state, src, scratch, out)?,
            },
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
            Class::Boxed | Class::Rows(_) | Class::Nums(_) => out.ops.push(TOp::MoveB { dst, src }),
            Class::Vec(_) => out.ops.push(TOp::VMove { dst, src }),
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
        if let Class::Vec(shape) = callee.entry[total + i] {
            out.ops.push(TOp::UnboxV { reg: dst, shape });
        }
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
        if let TOp::ReturnV { src, .. } = op {
            out.ops.push(TOp::VMove {
                dst: arg_start,
                src,
            });
            returns.push(out.ops.len());
            out.ops.push(TOp::Jump { to: u32::MAX });
        } else if let TOp::Return { src } = op {
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
        TOp::VMove { dst, src }
        | TOp::VNeg { dst, src, .. }
        | TOp::BoxV { dst, src, .. } => {
            *dst += offset;
            *src += offset;
        }
        TOp::VPush { list, value, .. } => {
            *list += offset;
            *value += offset;
        }
        TOp::NIndex { dst, list, index }
        | TOp::VIndexB {
            dst, list, index, ..
        }
        | TOp::VIndex {
            dst, list, index, ..
        }
        | TOp::VRow {
            dst, list, index, ..
        } => {
            *dst += offset;
            *list += offset;
            *index += offset;
        }
        TOp::VBin { dst, a, b, .. }
        | TOp::VDot { dst, a, b, .. }
        | TOp::VCross { dst, a, b, .. } => {
            *dst += offset;
            *a += offset;
            *b += offset;
        }
        TOp::VScale { dst, v, s, .. } => {
            *dst += offset;
            *v += offset;
            shift_opnd(s);
        }
        TOp::VLerp { dst, a, b, t, .. } | TOp::FLerp { dst, a, b, t } => {
            *dst += offset;
            *a += offset;
            *b += offset;
            *t += offset;
        }
        TOp::UnboxV { reg, .. } => *reg += offset,
        TOp::ReturnV { src, .. } => *src += offset,
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
        Dot | Cross | Len | KeyframeLerp | Lerp | Fallthrough => return None,
    })
}
