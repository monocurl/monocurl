//! the lane machine: a typed kernel run on several calls of a batch at once,
//! one op dispatch for `LANES` calls. every scalar op works on an array of
//! words, which the compiler turns into vector instructions on any target;
//! boxed ops and intrinsics run lane by lane.
//!
//! calls of a batch take the same branches most of the time, and when they
//! do not, the group splits: the lanes that jump are parked with a copy of
//! the frame, the lanes that fall through carry on, and a parked group is
//! resumed once the running one has returned. nothing ever needs to
//! reconverge, so a body with loops of differing trip counts (an escape-time
//! fractal) still runs as a group for the iterations its lanes share.
//!
//! calls the specialiser did not inline run lane by lane on the scalar typed
//! machine, and a fault in any lane fails the whole group, which the batch
//! driver then re-runs lane by lane

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use smallvec::SmallVec;

use super::{
    ir::BinKind,
    run::{self, ABORT_CHECK_MASK, CALL_OP_BUDGET, Fault},
    typed::{Class, Opnd, Spec, SpecId, TOp, TypedProgram},
    typed_run::TVm,
    value::{ClosureArena, KVal},
};

pub const LANES: usize = 8;

type Words = [u64; LANES];
type Mask = u8;

const ALL: Mask = ((1u16 << LANES) - 1) as Mask;

/// lanes parked at a branch the running group did not take
struct Parked {
    mask: Mask,
    pc: u32,
    words: Vec<Words>,
    boxed: Vec<KVal>,
}

pub struct LVm {
    /// one array of lane words per register
    words: Vec<Words>,
    /// register-major boxed values: `reg * LANES + lane`
    boxed: Vec<KVal>,
    parked: Vec<Parked>,
    budget: u64,
    abort: Option<Arc<AtomicBool>>,
    /// the specialisation whose captures are laid out in the frame
    warm_entry: Option<SpecId>,
    /// runs the calls the specialiser left as calls
    scalar: TVm,
}

impl Default for LVm {
    fn default() -> Self {
        Self::new()
    }
}

impl LVm {
    pub fn new() -> Self {
        Self {
            words: Vec::new(),
            boxed: Vec::new(),
            parked: Vec::new(),
            budget: CALL_OP_BUDGET,
            abort: None,
            warm_entry: None,
            scalar: TVm::new(),
        }
    }

    pub fn with_abort(abort: Arc<AtomicBool>) -> Self {
        Self {
            abort: Some(Arc::clone(&abort)),
            scalar: TVm::with_abort(abort),
            ..Self::new()
        }
    }

    /// run `spec` on up to `LANES` calls at once; every call's arguments must
    /// have the classes `spec` was made for. one result per call, in order
    pub fn call(
        &mut self,
        program: &TypedProgram,
        arena: &ClosureArena,
        spec_id: SpecId,
        calls: &[&[KVal]],
    ) -> Result<SmallVec<[KVal; LANES]>, Fault> {
        debug_assert!(!calls.is_empty() && calls.len() <= LANES);
        let spec = program.spec(spec_id);
        let frame = spec.frame_size as usize;
        self.budget = CALL_OP_BUDGET;
        self.parked.clear();

        if self.warm_entry != Some(spec_id) {
            self.words.clear();
            self.boxed.clear();
            self.words.resize(frame, [0; LANES]);
            self.boxed.resize(frame * LANES, KVal::Nil);
            let closure = arena.get(spec.closure);
            let total = spec.sig.len();
            for (i, capture) in closure.captures.iter().enumerate() {
                for lane in 0..LANES {
                    self.store(
                        lane,
                        (total + i) as u16,
                        spec.entry[total + i],
                        capture.clone(),
                    )?;
                }
            }
            self.warm_entry = Some(spec_id);
        }
        for (lane, args) in calls.iter().enumerate() {
            if args.len() != spec.sig.len() {
                return Err(Fault::Arity);
            }
            for (i, (arg, class)) in args.iter().zip(&spec.sig).enumerate() {
                self.store(lane, i as u16, *class, arg.clone())?;
            }
        }

        let mask = if calls.len() == LANES {
            ALL
        } else {
            ((1u16 << calls.len()) - 1) as Mask
        };
        let result = self.run(program, arena, spec, mask, calls.len());
        if result.is_err() {
            self.warm_entry = None;
        }
        result
    }

    fn store(&mut self, lane: usize, reg: u16, class: Class, value: KVal) -> Result<(), Fault> {
        match (class, value) {
            (Class::Int, KVal::Int(n)) => self.words[reg as usize][lane] = n as u64,
            (Class::Float, KVal::Float(f)) => self.words[reg as usize][lane] = f.to_bits(),
            (Class::Closure(_), KVal::Closure(_)) => {}
            (Class::Boxed, value) => self.boxed[reg as usize * LANES + lane] = value,
            _ => return Err(Fault::Type),
        }
        Ok(())
    }

    fn load(&self, lane: usize, opnd: Opnd) -> KVal {
        match opnd {
            Opnd::I(reg) => KVal::Int(self.words[reg as usize][lane] as i64),
            Opnd::F(reg) => KVal::Float(f64::from_bits(self.words[reg as usize][lane])),
            Opnd::B(reg) => self.boxed[reg as usize * LANES + lane].clone(),
            Opnd::C(id) => KVal::Closure(id),
        }
    }

    fn run(
        &mut self,
        program: &TypedProgram,
        arena: &ClosureArena,
        spec: &Spec,
        mut mask: Mask,
        count: usize,
    ) -> Result<SmallVec<[KVal; LANES]>, Fault> {
        let ops = &spec.ops;
        let mut results: SmallVec<[KVal; LANES]> = (0..count).map(|_| KVal::Nil).collect();
        let mut pc = 0usize;

        macro_rules! w {
            ($r:expr) => {
                self.words[$r as usize]
            };
        }
        macro_rules! lanes {
            ($dst:expr, |$l:ident| $body:expr) => {{
                let mut out = [0u64; LANES];
                for $l in 0..LANES {
                    out[$l] = $body;
                }
                self.words[$dst as usize] = out;
            }};
        }
        macro_rules! f_lanes {
            ($dst:expr, |$l:ident| $body:expr) => {
                lanes!($dst, |$l| {
                    let value: f64 = $body;
                    value.to_bits()
                })
            };
        }
        macro_rules! i_lanes {
            ($dst:expr, |$l:ident| $body:expr) => {
                lanes!($dst, |$l| {
                    let value: i64 = $body;
                    value as u64
                })
            };
        }
        macro_rules! f {
            ($r:expr, $l:expr) => {
                f64::from_bits(self.words[$r as usize][$l])
            };
        }
        macro_rules! i {
            ($r:expr, $l:expr) => {
                self.words[$r as usize][$l] as i64
            };
        }
        macro_rules! active {
            ($l:expr) => {
                mask & (1 << $l) != 0
            };
        }
        /// fault when any active lane satisfies the condition
        macro_rules! fault_if {
            (|$l:ident| $cond:expr, $fault:expr) => {
                for $l in 0..LANES {
                    if active!($l) && $cond {
                        return Err($fault);
                    }
                }
            };
        }
        /// take a branch for the lanes in `taken`, parking either side
        macro_rules! branch {
            ($taken:expr, $to:expr) => {{
                let taken: Mask = $taken & mask;
                if taken == mask {
                    pc = $to as usize;
                } else if taken != 0 {
                    self.parked.push(Parked {
                        mask: taken,
                        pc: $to,
                        words: self.words.clone(),
                        boxed: self.boxed.clone(),
                    });
                    mask &= !taken;
                }
            }};
        }

        loop {
            let op = ops[pc];
            pc += 1;
            self.budget -= 1;
            if self.budget & ABORT_CHECK_MASK == 0 {
                if self.budget == 0 {
                    return Err(Fault::Budget);
                }
                if self
                    .abort
                    .as_ref()
                    .is_some_and(|abort| abort.load(Ordering::Relaxed))
                {
                    return Err(Fault::Aborted);
                }
            }

            match op {
                TOp::Nop => {}
                TOp::Nil { dst } => {
                    for lane in 0..LANES {
                        self.boxed[dst as usize * LANES + lane] = KVal::Nil;
                    }
                }
                TOp::BoxI { reg } => {
                    for lane in 0..LANES {
                        self.boxed[reg as usize * LANES + lane] = KVal::Int(i!(reg, lane));
                    }
                }
                TOp::BoxF { reg } => {
                    for lane in 0..LANES {
                        self.boxed[reg as usize * LANES + lane] = KVal::Float(f!(reg, lane));
                    }
                }
                TOp::BoxC { reg, id } => {
                    for lane in 0..LANES {
                        self.boxed[reg as usize * LANES + lane] = KVal::Closure(id);
                    }
                }
                TOp::IConst { dst, value } => w!(dst) = [value as u64; LANES],
                TOp::FConst { dst, value } => w!(dst) = [value.to_bits(); LANES],
                TOp::MoveS { dst, src } => w!(dst) = w!(src),
                TOp::MoveB { dst, src } => {
                    for lane in 0..LANES {
                        let value = self.boxed[src as usize * LANES + lane].clone();
                        self.boxed[dst as usize * LANES + lane] = value;
                    }
                }
                TOp::IToF { dst, src } => f_lanes!(dst, |l| i!(src, l) as f64),
                TOp::IAdd { dst, a, b } => i_lanes!(dst, |l| i!(a, l).wrapping_add(i!(b, l))),
                TOp::ISub { dst, a, b } => i_lanes!(dst, |l| i!(a, l).wrapping_sub(i!(b, l))),
                TOp::IMul { dst, a, b } => i_lanes!(dst, |l| i!(a, l).wrapping_mul(i!(b, l))),
                TOp::IBin { op, dst, a, b } => match op {
                    BinKind::Div => {
                        fault_if!(|l| i!(b, l) == 0, Fault::DivisionByZero);
                        f_lanes!(dst, |l| i!(a, l) as f64 / i!(b, l) as f64)
                    }
                    BinKind::IntDiv => {
                        fault_if!(|l| i!(b, l) == 0, Fault::DivisionByZero);
                        i_lanes!(dst, |l| {
                            let y = i!(b, l);
                            if y == 0 {
                                0
                            } else {
                                crate::executor::ops::floor_div(i!(a, l), y)
                            }
                        })
                    }
                    BinKind::Power => f_lanes!(dst, |l| (i!(a, l) as f64).powf(i!(b, l) as f64)),
                    BinKind::Lt => i_lanes!(dst, |l| (i!(a, l) < i!(b, l)) as i64),
                    BinKind::Le => i_lanes!(dst, |l| (i!(a, l) <= i!(b, l)) as i64),
                    BinKind::Gt => i_lanes!(dst, |l| (i!(a, l) > i!(b, l)) as i64),
                    BinKind::Ge => i_lanes!(dst, |l| (i!(a, l) >= i!(b, l)) as i64),
                    BinKind::Eq => i_lanes!(dst, |l| (i!(a, l) == i!(b, l)) as i64),
                    BinKind::Ne => i_lanes!(dst, |l| (i!(a, l) != i!(b, l)) as i64),
                    BinKind::Add => i_lanes!(dst, |l| i!(a, l).wrapping_add(i!(b, l))),
                    BinKind::Sub => i_lanes!(dst, |l| i!(a, l).wrapping_sub(i!(b, l))),
                    BinKind::Mul => i_lanes!(dst, |l| i!(a, l).wrapping_mul(i!(b, l))),
                    BinKind::In => return Err(Fault::Type),
                },
                TOp::FAdd { dst, a, b } => f_lanes!(dst, |l| f!(a, l) + f!(b, l)),
                TOp::FSub { dst, a, b } => f_lanes!(dst, |l| f!(a, l) - f!(b, l)),
                TOp::FMul { dst, a, b } => f_lanes!(dst, |l| f!(a, l) * f!(b, l)),
                TOp::FDiv { dst, a, b } => {
                    fault_if!(|l| f!(b, l) == 0.0, Fault::DivisionByZero);
                    f_lanes!(dst, |l| f!(a, l) / f!(b, l))
                }
                TOp::FBin { op, dst, a, b } => match op {
                    BinKind::IntDiv => {
                        fault_if!(|l| f!(b, l) == 0.0, Fault::DivisionByZero);
                        i_lanes!(dst, |l| (f!(a, l) / f!(b, l)).floor() as i64)
                    }
                    BinKind::Power => f_lanes!(dst, |l| f!(a, l).powf(f!(b, l))),
                    BinKind::Lt => i_lanes!(dst, |l| (f!(a, l) < f!(b, l)) as i64),
                    BinKind::Le => i_lanes!(dst, |l| (f!(a, l) <= f!(b, l)) as i64),
                    BinKind::Gt => i_lanes!(dst, |l| (f!(a, l) > f!(b, l)) as i64),
                    BinKind::Ge => i_lanes!(dst, |l| (f!(a, l) >= f!(b, l)) as i64),
                    BinKind::Eq => i_lanes!(dst, |l| (f!(a, l) == f!(b, l)) as i64),
                    BinKind::Ne => i_lanes!(dst, |l| (f!(a, l) != f!(b, l)) as i64),
                    BinKind::Add => f_lanes!(dst, |l| f!(a, l) + f!(b, l)),
                    BinKind::Sub => f_lanes!(dst, |l| f!(a, l) - f!(b, l)),
                    BinKind::Mul => f_lanes!(dst, |l| f!(a, l) * f!(b, l)),
                    BinKind::Div => {
                        fault_if!(|l| f!(b, l) == 0.0, Fault::DivisionByZero);
                        f_lanes!(dst, |l| f!(a, l) / f!(b, l))
                    }
                    BinKind::In => return Err(Fault::Type),
                },
                TOp::INeg { dst, src } => i_lanes!(dst, |l| i!(src, l).wrapping_neg()),
                TOp::FNeg { dst, src } => f_lanes!(dst, |l| -f!(src, l)),
                TOp::INot { dst, src } => i_lanes!(dst, |l| (i!(src, l) == 0) as i64),
                TOp::FNot { dst, src } => i_lanes!(dst, |l| (f!(src, l) == 0.0) as i64),
                TOp::Jump { to } => pc = to as usize,
                TOp::JumpIfI { cond, to } => {
                    branch!(lane_mask(|l| i!(cond, l) != 0), to)
                }
                TOp::JumpIfNotI { cond, to } => {
                    branch!(lane_mask(|l| i!(cond, l) == 0), to)
                }
                TOp::JumpIfF { cond, to } => {
                    branch!(lane_mask(|l| f!(cond, l) != 0.0), to)
                }
                TOp::JumpIfNotF { cond, to } => {
                    branch!(lane_mask(|l| f!(cond, l) == 0.0), to)
                }
                TOp::RangeTestI { current, to } => {
                    branch!(lane_mask(|l| i!(current, l) >= i!(current + 1, l)), to)
                }
                TOp::RangeTestF { current, to } => {
                    branch!(lane_mask(|l| !(f!(current, l) < f!(current + 1, l))), to)
                }
                TOp::IInc { reg } => i_lanes!(reg, |l| i!(reg, l).wrapping_add(1)),
                TOp::FInc { reg } => f_lanes!(reg, |l| f!(reg, l) + 1.0),
                TOp::FUnary { f, dst, src } => f_lanes!(dst, |l| f(f!(src, l))),
                TOp::FToI { f, dst, src } => i_lanes!(dst, |l| f(f!(src, l)) as i64),
                TOp::FTrunc { dst, src } => i_lanes!(dst, |l| f!(src, l) as i64),
                TOp::IAbs { dst, src } => i_lanes!(dst, |l| i!(src, l).wrapping_abs()),
                TOp::ISign { dst, src } => i_lanes!(dst, |l| i!(src, l).signum()),
                TOp::FAbs { dst, src } => f_lanes!(dst, |l| f!(src, l).abs()),
                TOp::FSign { dst, src } => f_lanes!(dst, |l| run::float_sign(f!(src, l))),
                TOp::IMod { dst, a, b } => {
                    fault_if!(|l| i!(b, l) == 0, Fault::DivisionByZero);
                    i_lanes!(dst, |l| {
                        let y = i!(b, l);
                        if y == 0 {
                            0
                        } else {
                            i!(a, l).wrapping_rem_euclid(y)
                        }
                    })
                }
                TOp::FMod { dst, a, b } => {
                    fault_if!(|l| f!(b, l) == 0.0, Fault::DivisionByZero);
                    f_lanes!(dst, |l| f!(a, l).rem_euclid(f!(b, l)))
                }
                TOp::IMin { dst, a, b } => i_lanes!(dst, |l| i!(a, l).min(i!(b, l))),
                TOp::IMax { dst, a, b } => i_lanes!(dst, |l| i!(a, l).max(i!(b, l))),
                TOp::FMin { dst, a, b } => f_lanes!(dst, |l| f!(a, l).min(f!(b, l))),
                TOp::FMax { dst, a, b } => f_lanes!(dst, |l| f!(a, l).max(f!(b, l))),
                TOp::FPow { dst, a, b } => f_lanes!(dst, |l| f!(a, l).powf(f!(b, l))),
                TOp::FAtan2 { dst, a, b } => f_lanes!(dst, |l| f!(a, l).atan2(f!(b, l))),
                TOp::Call {
                    spec: callee,
                    arg_start,
                    arg_count,
                    ret,
                } => {
                    let sig = &program.spec(callee).sig;
                    for lane in 0..LANES {
                        if !active!(lane) {
                            continue;
                        }
                        let args: SmallVec<[KVal; 4]> = (0..arg_count as usize)
                            .map(|i| {
                                let reg = arg_start + i as u16;
                                self.load(lane, class_opnd(sig[i], reg))
                            })
                            .collect();
                        let result = self.scalar.call(program, arena, callee, &args)?;
                        self.store(lane, arg_start, ret, result)?;
                    }
                }
                TOp::DynBin { op, dst, a, b, ret } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            let value =
                                run::binary(arena, op, &self.load(lane, a), &self.load(lane, b))?;
                            self.store(lane, dst, ret, value)?;
                        }
                    }
                }
                TOp::DynNeg { dst, src } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            let value = run::negate(&self.load(lane, src))?;
                            self.boxed[dst as usize * LANES + lane] = value;
                        }
                    }
                }
                TOp::DynNot { dst, src } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            let value = !run::truthy(&self.load(lane, src))?;
                            self.words[dst as usize][lane] = value as u64;
                        }
                    }
                }
                TOp::DynJumpIf { cond, negate, to } => {
                    let mut taken: Mask = 0;
                    for lane in 0..LANES {
                        if active!(lane) && run::truthy(&self.load(lane, cond))? != negate {
                            taken |= 1 << lane;
                        }
                    }
                    branch!(taken, to)
                }
                TOp::DynRangeTest { current, stop, to } => {
                    let mut taken: Mask = 0;
                    for lane in 0..LANES {
                        if active!(lane) {
                            let below = run::binary(
                                arena,
                                BinKind::Lt,
                                &self.load(lane, current),
                                &self.load(lane, stop),
                            )?;
                            if !run::truthy(&below)? {
                                taken |= 1 << lane;
                            }
                        }
                    }
                    branch!(taken, to)
                }
                TOp::DynInc { reg } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            match &mut self.boxed[reg as usize * LANES + lane] {
                                KVal::Int(n) => *n = n.wrapping_add(1),
                                KVal::Float(f) => *f += 1.0,
                                _ => return Err(Fault::Type),
                            }
                        }
                    }
                }
                TOp::EmptyList { dst } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            self.boxed[dst as usize * LANES + lane] =
                                KVal::List(Arc::new(SmallVec::new()));
                        }
                    }
                }
                TOp::Append { list, value } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            let value = self.load(lane, value);
                            match &mut self.boxed[list as usize * LANES + lane] {
                                KVal::List(list) => Arc::make_mut(list).push(value),
                                _ => return Err(Fault::Type),
                            }
                        }
                    }
                }
                TOp::Index { dst, list, index } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            let element = match (
                                &self.boxed[list as usize * LANES + lane],
                                self.load(lane, index),
                            ) {
                                (KVal::List(list), KVal::Int(index)) => {
                                    list.get(index as usize).ok_or(Fault::Index)?.clone()
                                }
                                _ => return Err(Fault::Type),
                            };
                            self.boxed[dst as usize * LANES + lane] = element;
                        }
                    }
                }
                TOp::Len { dst, list } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            let len = match &self.boxed[list as usize * LANES + lane] {
                                KVal::List(list) => list.len() as i64,
                                _ => return Err(Fault::Type),
                            };
                            self.words[dst as usize][lane] = len as u64;
                        }
                    }
                }
                TOp::DynNative {
                    intrinsic,
                    dst,
                    args,
                    arity,
                    ret,
                } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            let values: SmallVec<[KVal; 2]> = args[..arity as usize]
                                .iter()
                                .map(|&arg| self.load(lane, arg))
                                .collect();
                            let value = run::native_in(arena, intrinsic, &values)?;
                            self.store(lane, dst, ret, value)?;
                        }
                    }
                }
                TOp::Capture {
                    dst,
                    closure,
                    index,
                } => {
                    for lane in 0..LANES {
                        self.boxed[dst as usize * LANES + lane] =
                            arena.get(closure).captures[index as usize].clone();
                    }
                }
                TOp::Default {
                    dst,
                    closure,
                    index,
                } => {
                    for lane in 0..LANES {
                        self.boxed[dst as usize * LANES + lane] =
                            arena.get(closure).defaults[index as usize].clone();
                    }
                }
                TOp::BoxInto { dst, src } => {
                    for lane in 0..LANES {
                        if active!(lane) {
                            let value = self.load(lane, src);
                            self.boxed[dst as usize * LANES + lane] = value;
                        }
                    }
                }
                TOp::Return { src } => {
                    for lane in 0..count {
                        if active!(lane) {
                            results[lane] = self.load(lane, src);
                        }
                    }
                    let Some(parked) = self.parked.pop() else {
                        return Ok(results);
                    };
                    mask = parked.mask;
                    pc = parked.pc as usize;
                    self.words = parked.words;
                    self.boxed = parked.boxed;
                }
            }
        }
    }
}

fn lane_mask(mut test: impl FnMut(usize) -> bool) -> Mask {
    let mut mask: Mask = 0;
    for lane in 0..LANES {
        if test(lane) {
            mask |= 1 << lane;
        }
    }
    mask
}

fn class_opnd(class: Class, reg: u16) -> Opnd {
    match class {
        Class::Int => Opnd::I(reg),
        Class::Float => Opnd::F(reg),
        Class::Closure(id) => Opnd::C(id),
        Class::Boxed | Class::Unset => Opnd::B(reg),
    }
}
