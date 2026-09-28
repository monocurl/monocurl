//! the register machine that executes a compiled kernel.
//!
//! every operation mirrors the interpreter's semantics for the values the tier
//! models and faults for everything else. a fault carries no message: the call
//! is simply re-run by the interpreter, which produces the authoritative error
//! with its span and call stack

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use smallvec::SmallVec;

use super::{
    ir::{BinKind, KOp, KernelIntrinsic},
    value::{ClosureArena, ClosureId, KClosure, KList, KVal},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Type,
    DivisionByZero,
    Index,
    Arity,
    Depth,
    Budget,
    /// another call of the same batch faulted, so this one is pointless
    Aborted,
    LengthMismatch,
}

/// nesting the register machine allows before handing the call back to the
/// interpreter, which enforces the real limit
pub(super) const MAX_DEPTH: u32 = 512;

/// operations one call may execute before the tier gives up on it. a call
/// this long belongs in the interpreter, where it yields cooperatively
pub const CALL_OP_BUDGET: u64 = 1 << 26;

/// ops between looks at the abort flag
pub(super) const ABORT_CHECK_MASK: u64 = (1 << 16) - 1;

pub struct Vm {
    regs: Vec<KVal>,
    depth: u32,
    budget: u64,
    /// the closure whose entry frame is still laid out in the register file.
    /// arguments and captures are never written by a body, so a run of calls
    /// to one closure only has to rewrite the arguments
    warm_entry: Option<ClosureId>,
    /// raised by whoever shares this flag (the other workers of a batch) to
    /// stop a call early instead of letting it run out its budget
    abort: Option<Arc<AtomicBool>>,
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

impl Vm {
    pub fn new() -> Self {
        Self {
            regs: Vec::with_capacity(256),
            depth: 0,
            budget: CALL_OP_BUDGET,
            warm_entry: None,
            abort: None,
        }
    }

    pub fn with_abort(abort: Arc<AtomicBool>) -> Self {
        Self {
            abort: Some(abort),
            ..Self::new()
        }
    }

    /// drop the warm entry frame; needed when the arena it was laid out from
    /// is replaced
    pub fn forget_entry(&mut self) {
        self.warm_entry = None;
    }

    /// run the closure `entry` of `arena` on `args`, which must satisfy its
    /// arity exactly (defaults are filled by the caller of a top-level call)
    pub fn call(
        &mut self,
        arena: &ClosureArena,
        entry: ClosureId,
        args: &[KVal],
    ) -> Result<KVal, Fault> {
        let closure = arena.get(entry);
        let kernel = &closure.kernel;
        if args.len() != kernel.total_args as usize {
            return Err(Fault::Arity);
        }
        self.budget = CALL_OP_BUDGET;
        self.depth = 0;

        let frame_size = kernel.frame_size as usize;
        if self.warm_entry != Some(entry) {
            self.regs.clear();
            self.regs.resize(frame_size, KVal::Nil);
            for (slot, capture) in self.regs[args.len()..]
                .iter_mut()
                .zip(closure.captures.iter())
            {
                *slot = capture.clone();
            }
            self.warm_entry = Some(entry);
        } else {
            self.regs.truncate(frame_size);
        }
        for (slot, arg) in self.regs.iter_mut().zip(args) {
            *slot = arg.clone();
        }

        self.depth += 1;
        let result = self.run(arena, closure, 0);
        self.depth -= 1;
        if result.is_err() {
            // a fault may have left the frame half written
            self.warm_entry = None;
        }
        result
    }

    /// lay out a frame for `closure` at `base`, which must be the end of the
    /// register file, let `fill` write its first `provided` argument registers
    /// (it sees the whole file, since arguments come from the caller's frame),
    /// and run it
    fn enter(
        &mut self,
        arena: &ClosureArena,
        closure: &KClosure,
        base: usize,
        provided: usize,
        fill: impl FnOnce(&mut [KVal]),
    ) -> Result<KVal, Fault> {
        let kernel = &closure.kernel;
        let required = kernel.required_args as usize;
        let total = kernel.total_args as usize;
        if provided < required || provided > total {
            return Err(Fault::Arity);
        }
        if self.depth >= MAX_DEPTH {
            return Err(Fault::Depth);
        }

        self.regs
            .resize(base + kernel.frame_size as usize, KVal::Nil);
        fill(&mut self.regs);
        let frame = &mut self.regs[base..];
        let default_start = closure.defaults.len().saturating_sub(total - provided);
        for (slot, default) in frame[provided..total]
            .iter_mut()
            .zip(&closure.defaults[default_start..])
        {
            *slot = default.clone();
        }
        for (slot, capture) in frame[total..].iter_mut().zip(closure.captures.iter()) {
            *slot = capture.clone();
        }

        self.depth += 1;
        let result = self.run(arena, closure, base);
        self.depth -= 1;
        self.regs.truncate(base);
        result
    }

    fn run(
        &mut self,
        arena: &ClosureArena,
        closure: &KClosure,
        base: usize,
    ) -> Result<KVal, Fault> {
        let ops = &closure.kernel.ops;
        // registers below this hold arguments and captures, which outlive the
        // call when the frame stays warm, so a return must copy them out
        let entry_end = closure.kernel.total_args + closure.kernel.capture_count;
        let mut pc = 0usize;

        macro_rules! reg {
            ($r:expr) => {
                self.regs[base + $r as usize]
            };
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
                KOp::Nil { dst } => reg!(dst) = KVal::Nil,
                KOp::Int { dst, value } => reg!(dst) = KVal::Int(value),
                KOp::Float { dst, value } => reg!(dst) = KVal::Float(value),
                KOp::Move { dst, src } => {
                    let value = reg!(src).clone();
                    reg!(dst) = value;
                }
                KOp::Bin { op, dst, a, b } => {
                    let value = binary(arena, op, &reg!(a), &reg!(b))?;
                    reg!(dst) = value;
                }
                KOp::Neg { dst, src } => {
                    let value = negate(&reg!(src))?;
                    reg!(dst) = value;
                }
                KOp::Not { dst, src } => {
                    let value = KVal::Int(!truthy(&reg!(src))? as i64);
                    reg!(dst) = value;
                }
                KOp::Jump { to } => pc = to as usize,
                KOp::JumpIf { cond, to } => {
                    if truthy(&reg!(cond))? {
                        pc = to as usize;
                    }
                }
                KOp::JumpIfNot { cond, to } => {
                    if !truthy(&reg!(cond))? {
                        pc = to as usize;
                    }
                }
                KOp::RangeTest { current, to } => {
                    let keep_going = match (&reg!(current), &reg!(current + 1)) {
                        (KVal::Int(current), KVal::Int(stop)) => current < stop,
                        (KVal::Float(current), KVal::Float(stop)) => current < stop,
                        (current, stop) => truthy(&binary(arena, BinKind::Lt, current, stop)?)?,
                    };
                    if !keep_going {
                        pc = to as usize;
                    }
                }
                KOp::Inc { reg } => match &mut reg!(reg) {
                    KVal::Int(n) => *n = n.wrapping_add(1),
                    KVal::Float(f) => *f += 1.0,
                    _ => return Err(Fault::Type),
                },
                KOp::EmptyList { dst } => reg!(dst) = KVal::List(Arc::new(SmallVec::new())),
                KOp::Append { list, value } => {
                    let value = reg!(value).clone();
                    match &mut reg!(list) {
                        KVal::List(list) => Arc::make_mut(list).push(value),
                        _ => return Err(Fault::Type),
                    }
                }
                KOp::Index { dst, list, index } => {
                    let element = match (&reg!(list), &reg!(index)) {
                        (KVal::List(list), KVal::Int(index)) => {
                            list.get(*index as usize).ok_or(Fault::Index)?.clone()
                        }
                        _ => return Err(Fault::Type),
                    };
                    reg!(dst) = element;
                }
                KOp::Len { dst, src } => {
                    let len = match &reg!(src) {
                        KVal::List(list) => list.len() as i64,
                        _ => return Err(Fault::Type),
                    };
                    reg!(dst) = KVal::Int(len);
                }
                KOp::Call {
                    callee,
                    arg_start,
                    arg_count,
                } => {
                    let callee = match &reg!(callee) {
                        KVal::Closure(id) => arena.get(*id),
                        _ => return Err(Fault::Type),
                    };
                    let arg_base = base + arg_start as usize;
                    let new_base = self.regs.len();
                    let result = self.enter(arena, callee, new_base, arg_count as usize, |regs| {
                        // the argument temporaries are dead once the call
                        // returns, so they move rather than copy
                        for i in 0..arg_count as usize {
                            regs[new_base + i] =
                                std::mem::replace(&mut regs[arg_base + i], KVal::Nil);
                        }
                    });
                    reg!(arg_start) = result?;
                }
                KOp::Native {
                    intrinsic,
                    arg_start,
                    arg_count,
                } => {
                    let start = base + arg_start as usize;
                    let value = native(intrinsic, &self.regs[start..start + arg_count as usize])?;
                    reg!(arg_start) = value;
                }
                KOp::Return { src } => {
                    return Ok(if src < entry_end {
                        reg!(src).clone()
                    } else {
                        std::mem::replace(&mut reg!(src), KVal::Nil)
                    });
                }
            }
        }
    }
}

pub(super) fn truthy(value: &KVal) -> Result<bool, Fault> {
    match value {
        KVal::Int(n) => Ok(*n != 0),
        KVal::Float(f) => Ok(*f != 0.0),
        _ => Err(Fault::Type),
    }
}

pub(super) fn negate(value: &KVal) -> Result<KVal, Fault> {
    Ok(match value {
        KVal::Int(n) => KVal::Int(n.wrapping_neg()),
        KVal::Float(f) => KVal::Float(-f),
        KVal::List(list) => KVal::List(Arc::new(
            list.iter().map(negate).collect::<Result<KList, _>>()?,
        )),
        _ => return Err(Fault::Type),
    })
}

/// the same promotion and result types as `eval_binary`
pub fn binary(arena: &ClosureArena, op: BinKind, a: &KVal, b: &KVal) -> Result<KVal, Fault> {
    match (a, b) {
        (KVal::Int(x), KVal::Int(y)) => return int_binary(op, *x, *y),
        (KVal::Float(x), KVal::Float(y)) => return float_binary(op, *x, *y),
        (KVal::Int(x), KVal::Float(y)) => return float_binary(op, *x as f64, *y),
        (KVal::Float(x), KVal::Int(y)) => return float_binary(op, *x, *y as f64),
        _ => {}
    }

    match op {
        BinKind::Eq | BinKind::Ne => {
            let equal = KVal::equals(arena, a, b).ok_or(Fault::Type)?;
            return Ok(KVal::Int(((op == BinKind::Eq) == equal) as i64));
        }
        BinKind::In => {
            return match b {
                KVal::List(list) => {
                    let mut found = false;
                    for element in list.iter() {
                        if KVal::equals(arena, a, element).ok_or(Fault::Type)? {
                            found = true;
                            break;
                        }
                    }
                    Ok(KVal::Int(found as i64))
                }
                _ => Err(Fault::Type),
            };
        }
        _ => {}
    }

    match (a, b, op) {
        (KVal::List(x), KVal::List(y), BinKind::Add | BinKind::Sub) => {
            if x.len() != y.len() {
                return Err(Fault::LengthMismatch);
            }
            let combined = x
                .iter()
                .zip(y.iter())
                .map(|(a, b)| binary(arena, op, a, b))
                .collect::<Result<KList, _>>()?;
            Ok(KVal::List(Arc::new(combined)))
        }
        (KVal::List(list), scalar, BinKind::Mul | BinKind::Div)
            if !matches!(scalar, KVal::List(_)) =>
        {
            let applied = list
                .iter()
                .map(|element| binary(arena, op, element, scalar))
                .collect::<Result<KList, _>>()?;
            Ok(KVal::List(Arc::new(applied)))
        }
        (scalar, KVal::List(list), BinKind::Mul | BinKind::Div)
            if !matches!(scalar, KVal::List(_)) =>
        {
            let applied = list
                .iter()
                .map(|element| binary(arena, op, scalar, element))
                .collect::<Result<KList, _>>()?;
            Ok(KVal::List(Arc::new(applied)))
        }
        _ => Err(Fault::Type),
    }
}

fn int_binary(op: BinKind, a: i64, b: i64) -> Result<KVal, Fault> {
    Ok(match op {
        BinKind::Add => KVal::Int(a.wrapping_add(b)),
        BinKind::Sub => KVal::Int(a.wrapping_sub(b)),
        BinKind::Mul => KVal::Int(a.wrapping_mul(b)),
        BinKind::Div => {
            if b == 0 {
                return Err(Fault::DivisionByZero);
            }
            KVal::Float(a as f64 / b as f64)
        }
        BinKind::IntDiv => {
            if b == 0 {
                return Err(Fault::DivisionByZero);
            }
            KVal::Int(crate::executor::ops::floor_div(a, b))
        }
        BinKind::Power => KVal::Float((a as f64).powf(b as f64)),
        BinKind::Lt => KVal::Int((a < b) as i64),
        BinKind::Le => KVal::Int((a <= b) as i64),
        BinKind::Gt => KVal::Int((a > b) as i64),
        BinKind::Ge => KVal::Int((a >= b) as i64),
        BinKind::Eq => KVal::Int((a == b) as i64),
        BinKind::Ne => KVal::Int((a != b) as i64),
        BinKind::In => return Err(Fault::Type),
    })
}

fn float_binary(op: BinKind, a: f64, b: f64) -> Result<KVal, Fault> {
    Ok(match op {
        BinKind::Add => KVal::Float(a + b),
        BinKind::Sub => KVal::Float(a - b),
        BinKind::Mul => KVal::Float(a * b),
        BinKind::Div => {
            if b == 0.0 {
                return Err(Fault::DivisionByZero);
            }
            KVal::Float(a / b)
        }
        BinKind::IntDiv => {
            if b == 0.0 {
                return Err(Fault::DivisionByZero);
            }
            KVal::Int((a / b).floor() as i64)
        }
        BinKind::Power => KVal::Float(a.powf(b)),
        BinKind::Lt => KVal::Int((a < b) as i64),
        BinKind::Le => KVal::Int((a <= b) as i64),
        BinKind::Gt => KVal::Int((a > b) as i64),
        BinKind::Ge => KVal::Int((a >= b) as i64),
        BinKind::Eq => KVal::Int((a == b) as i64),
        BinKind::Ne => KVal::Int((a != b) as i64),
        BinKind::In => return Err(Fault::Type),
    })
}

/// `sign` of a float: `signum` would call every zero a one
pub fn float_sign(x: f64) -> f64 {
    if x == 0.0 { 0.0 } else { x.signum() }
}

fn as_f64(value: &KVal) -> Result<f64, Fault> {
    match value {
        KVal::Int(n) => Ok(*n as f64),
        KVal::Float(f) => Ok(*f),
        _ => Err(Fault::Type),
    }
}

/// the stdlib's `read_number_pair`: int stays int only when both are
enum NumberPair {
    Int(i64, i64),
    Float(f64, f64),
}

fn number_pair(a: &KVal, b: &KVal) -> Result<NumberPair, Fault> {
    Ok(match (a, b) {
        (KVal::Int(a), KVal::Int(b)) => NumberPair::Int(*a, *b),
        (KVal::Int(a), KVal::Float(b)) => NumberPair::Float(*a as f64, *b),
        (KVal::Float(a), KVal::Int(b)) => NumberPair::Float(*a, *b as f64),
        (KVal::Float(a), KVal::Float(b)) => NumberPair::Float(*a, *b),
        _ => return Err(Fault::Type),
    })
}

fn float_list(value: &KVal) -> Result<SmallVec<[f64; 4]>, Fault> {
    match value {
        KVal::List(list) => list.iter().map(as_f64).collect(),
        _ => Err(Fault::Type),
    }
}

pub fn native(intrinsic: KernelIntrinsic, args: &[KVal]) -> Result<KVal, Fault> {
    use KernelIntrinsic::*;

    let unary = |f: fn(f64) -> f64| as_f64(&args[0]).map(|x| KVal::Float(f(x)));
    let to_int = |f: fn(f64) -> f64| as_f64(&args[0]).map(|x| KVal::Int(f(x) as i64));

    match intrinsic {
        Sqrt => unary(f64::sqrt),
        Cbrt => unary(f64::cbrt),
        Exp => unary(f64::exp),
        Ln => unary(f64::ln),
        Sin => unary(f64::sin),
        Cos => unary(f64::cos),
        Tan => unary(f64::tan),
        Asin => unary(f64::asin),
        Acos => unary(f64::acos),
        Atan => unary(f64::atan),
        Sinh => unary(f64::sinh),
        Cosh => unary(f64::cosh),
        Tanh => unary(f64::tanh),
        Pow => Ok(KVal::Float(as_f64(&args[0])?.powf(as_f64(&args[1])?))),
        Atan2 => Ok(KVal::Float(as_f64(&args[0])?.atan2(as_f64(&args[1])?))),
        Abs => match &args[0] {
            KVal::Int(n) => Ok(KVal::Int(n.wrapping_abs())),
            KVal::Float(f) => Ok(KVal::Float(f.abs())),
            _ => Err(Fault::Type),
        },
        Sign => match &args[0] {
            KVal::Int(n) => Ok(KVal::Int(n.signum())),
            KVal::Float(f) => Ok(KVal::Float(float_sign(*f))),
            _ => Err(Fault::Type),
        },
        Floor => to_int(f64::floor),
        Ceil => to_int(f64::ceil),
        Round => to_int(f64::round),
        Trunc => to_int(f64::trunc),
        Mod => match number_pair(&args[0], &args[1])? {
            NumberPair::Int(n, m) => {
                if m == 0 {
                    return Err(Fault::DivisionByZero);
                }
                Ok(KVal::Int(n.wrapping_rem_euclid(m)))
            }
            NumberPair::Float(n, m) => {
                if m == 0.0 {
                    return Err(Fault::DivisionByZero);
                }
                Ok(KVal::Float(n.rem_euclid(m)))
            }
        },
        Min => Ok(match number_pair(&args[0], &args[1])? {
            NumberPair::Int(a, b) => KVal::Int(a.min(b)),
            NumberPair::Float(a, b) => KVal::Float(a.min(b)),
        }),
        Max => Ok(match number_pair(&args[0], &args[1])? {
            NumberPair::Int(a, b) => KVal::Int(a.max(b)),
            NumberPair::Float(a, b) => KVal::Float(a.max(b)),
        }),
        Dot => {
            let u = float_list(&args[0])?;
            let v = float_list(&args[1])?;
            if u.len() != v.len() {
                return Err(Fault::LengthMismatch);
            }
            Ok(KVal::Float(
                u.iter().zip(v.iter()).fold(0.0, |acc, (a, b)| acc + a * b),
            ))
        }
        Cross => {
            let u = float_list(&args[0])?;
            let v = float_list(&args[1])?;
            if u.len() != 3 || v.len() != 3 {
                return Err(Fault::Type);
            }
            Ok(KVal::list([
                KVal::Float(u[1] * v[2] - u[2] * v[1]),
                KVal::Float(u[2] * v[0] - u[0] * v[2]),
                KVal::Float(u[0] * v[1] - u[1] * v[0]),
            ]))
        }
        Len => match &args[0] {
            KVal::List(list) => Ok(KVal::Int(list.len() as i64)),
            _ => Err(Fault::Type),
        },
        ToInt => match &args[0] {
            KVal::Int(n) => Ok(KVal::Int(*n)),
            KVal::Float(f) => Ok(KVal::Int(*f as i64)),
            _ => Err(Fault::Type),
        },
        ToFloat => match &args[0] {
            KVal::Int(n) => Ok(KVal::Float(*n as f64)),
            KVal::Float(f) => Ok(KVal::Float(*f)),
            _ => Err(Fault::Type),
        },
        Fallthrough => Err(Fault::Type),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::ir::Kernel;

    fn endless_loop() -> ClosureArena {
        let mut arena = ClosureArena::default();
        arena.push(KClosure {
            ip: Default::default(),
            kernel: Arc::new(Kernel {
                ip: Default::default(),
                required_args: 0,
                total_args: 0,
                capture_count: 0,
                frame_size: 0,
                ops: Box::new([KOp::Jump { to: 0 }]),
            }),
            captures: Box::new([]),
            defaults: Box::new([]),
        });
        arena
    }

    #[test]
    fn an_endless_loop_runs_out_of_budget() {
        let arena = endless_loop();
        let mut vm = Vm::new();
        assert!(matches!(
            vm.call(&arena, ClosureId(0), &[]),
            Err(Fault::Budget)
        ));
    }

    #[test]
    fn a_raised_abort_flag_stops_a_call_early() {
        let arena = endless_loop();
        let abort = Arc::new(AtomicBool::new(false));
        let mut vm = Vm::with_abort(Arc::clone(&abort));
        let started = std::time::Instant::now();
        let worker = std::thread::spawn(move || vm.call(&arena, ClosureId(0), &[]));
        abort.store(true, Ordering::Relaxed);
        assert!(matches!(worker.join().unwrap(), Err(Fault::Aborted)));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }
}
