//! the machine typed kernels run on: a word file for ints and floats and a
//! boxed file for everything else, both indexed by the same register numbers,
//! so a typed op touches one word and a boxed op reuses the dynamic machine's
//! routines unchanged

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use smallvec::SmallVec;

use super::{
    ir::BinKind,
    run::{self, ABORT_CHECK_MASK, CALL_OP_BUDGET, Fault, MAX_DEPTH},
    typed::{Class, Opnd, Spec, SpecId, TOp, TypedProgram},
    value::{ClosureArena, KVal},
};

pub struct TVm {
    words: Vec<u64>,
    boxed: Vec<KVal>,
    depth: u32,
    budget: u64,
    abort: Option<Arc<AtomicBool>>,
    /// the specialisation whose entry frame is laid out at the bottom of the
    /// files; a run of calls to it only rewrites the arguments
    warm_entry: Option<SpecId>,
}

impl Default for TVm {
    fn default() -> Self {
        Self::new()
    }
}

impl TVm {
    pub fn new() -> Self {
        Self {
            words: Vec::with_capacity(256),
            boxed: Vec::with_capacity(256),
            depth: 0,
            budget: CALL_OP_BUDGET,
            abort: None,
            warm_entry: None,
        }
    }

    pub fn with_abort(abort: Arc<AtomicBool>) -> Self {
        Self {
            abort: Some(abort),
            ..Self::new()
        }
    }

    pub fn forget_entry(&mut self) {
        self.warm_entry = None;
    }

    /// run `spec` on `args`, whose classes must be the ones `spec` was made
    /// for (the caller checks with `accepts`)
    pub fn call(
        &mut self,
        program: &TypedProgram,
        arena: &ClosureArena,
        spec_id: SpecId,
        args: &[KVal],
    ) -> Result<KVal, Fault> {
        let spec = program.spec(spec_id);
        if args.len() != spec.sig.len() {
            return Err(Fault::Arity);
        }
        self.budget = CALL_OP_BUDGET;
        self.depth = 0;

        let frame = spec.frame_size as usize;
        if self.warm_entry != Some(spec_id) {
            self.words.clear();
            self.boxed.clear();
            self.words.resize(frame, 0);
            self.boxed.resize(frame, KVal::Nil);
            let closure = arena.get(spec.closure);
            let total = spec.sig.len();
            for (i, capture) in closure.captures.iter().enumerate() {
                self.store(
                    0,
                    (total + i) as u16,
                    spec.entry[total + i],
                    capture.clone(),
                )?;
            }
            self.warm_entry = Some(spec_id);
        } else {
            self.words.truncate(frame);
            self.boxed.truncate(frame);
        }
        for (i, (arg, class)) in args.iter().zip(&spec.sig).enumerate() {
            self.store(0, i as u16, *class, arg.clone())?;
        }

        self.depth += 1;
        let result = self.run(program, arena, spec, 0);
        self.depth -= 1;
        if result.is_err() {
            self.warm_entry = None;
        }
        result
    }

    /// whether `args` have the classes `spec` was specialised for
    pub fn accepts(spec: &Spec, args: &[KVal]) -> bool {
        args.len() == spec.sig.len()
            && args
                .iter()
                .zip(&spec.sig)
                .all(|(arg, class)| Class::of(arg) == *class)
    }

    fn store(&mut self, base: usize, reg: u16, class: Class, value: KVal) -> Result<(), Fault> {
        let slot = base + reg as usize;
        match (class, value) {
            (Class::Int, KVal::Int(n)) => self.words[slot] = n as u64,
            (Class::Float, KVal::Float(f)) => self.words[slot] = f.to_bits(),
            (Class::Closure(_), KVal::Closure(_)) => {}
            (Class::Boxed, value) => self.boxed[slot] = value,
            _ => return Err(Fault::Type),
        }
        Ok(())
    }

    fn load(&self, base: usize, opnd: Opnd) -> KVal {
        match opnd {
            Opnd::I(reg) => KVal::Int(self.words[base + reg as usize] as i64),
            Opnd::F(reg) => KVal::Float(f64::from_bits(self.words[base + reg as usize])),
            Opnd::B(reg) => self.boxed[base + reg as usize].clone(),
            Opnd::C(id) => KVal::Closure(id),
        }
    }

    fn enter(
        &mut self,
        program: &TypedProgram,
        arena: &ClosureArena,
        spec_id: SpecId,
        base: usize,
        caller_args: usize,
        provided: usize,
    ) -> Result<KVal, Fault> {
        if self.depth >= MAX_DEPTH {
            return Err(Fault::Depth);
        }
        let spec = program.spec(spec_id);
        let closure = arena.get(spec.closure);
        let total = spec.sig.len();
        let frame = spec.frame_size as usize;
        self.words.resize(base + frame, 0);
        self.boxed.resize(base + frame, KVal::Nil);
        for i in 0..provided {
            match spec.sig[i] {
                Class::Int | Class::Float => self.words[base + i] = self.words[caller_args + i],
                Class::Boxed => {
                    self.boxed[base + i] =
                        std::mem::replace(&mut self.boxed[caller_args + i], KVal::Nil)
                }
                Class::Closure(_) | Class::Unset => {}
            }
        }
        let default_start = closure.defaults.len().saturating_sub(total - provided);
        for (i, default) in (provided..total).zip(&closure.defaults[default_start..]) {
            self.store(base, i as u16, spec.sig[i], default.clone())?;
        }
        for (i, capture) in closure.captures.iter().enumerate() {
            self.store(
                base,
                (total + i) as u16,
                spec.entry[total + i],
                capture.clone(),
            )?;
        }

        self.depth += 1;
        let result = self.run(program, arena, spec, base);
        self.depth -= 1;
        self.words.truncate(base);
        self.boxed.truncate(base);
        result
    }

    fn run(
        &mut self,
        program: &TypedProgram,
        arena: &ClosureArena,
        spec: &Spec,
        base: usize,
    ) -> Result<KVal, Fault> {
        let ops = &spec.ops;
        let mut pc = 0usize;

        macro_rules! f {
            ($r:expr) => {
                f64::from_bits(self.words[base + $r as usize])
            };
        }
        macro_rules! i {
            ($r:expr) => {
                self.words[base + $r as usize] as i64
            };
        }
        macro_rules! set_f {
            ($r:expr, $v:expr) => {{
                let value: f64 = $v;
                self.words[base + $r as usize] = value.to_bits();
            }};
        }
        macro_rules! set_i {
            ($r:expr, $v:expr) => {{
                let value: i64 = $v;
                self.words[base + $r as usize] = value as u64;
            }};
        }
        macro_rules! boxed {
            ($r:expr) => {
                self.boxed[base + $r as usize]
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
                TOp::Nop => {}
                TOp::Nil { dst } => boxed!(dst) = KVal::Nil,
                TOp::BoxI { reg } => boxed!(reg) = KVal::Int(i!(reg)),
                TOp::BoxF { reg } => boxed!(reg) = KVal::Float(f!(reg)),
                TOp::BoxC { reg, id } => boxed!(reg) = KVal::Closure(id),
                TOp::IConst { dst, value } => set_i!(dst, value),
                TOp::FConst { dst, value } => set_f!(dst, value),
                TOp::MoveS { dst, src } => {
                    self.words[base + dst as usize] = self.words[base + src as usize]
                }
                TOp::MoveB { dst, src } => {
                    let value = boxed!(src).clone();
                    boxed!(dst) = value;
                }
                TOp::IToF { dst, src } => set_f!(dst, i!(src) as f64),
                TOp::IAdd { dst, a, b } => set_i!(dst, i!(a).wrapping_add(i!(b))),
                TOp::ISub { dst, a, b } => set_i!(dst, i!(a).wrapping_sub(i!(b))),
                TOp::IMul { dst, a, b } => set_i!(dst, i!(a).wrapping_mul(i!(b))),
                TOp::IBin { op, dst, a, b } => {
                    let (x, y) = (i!(a), i!(b));
                    match op {
                        BinKind::Div => {
                            if y == 0 {
                                return Err(Fault::DivisionByZero);
                            }
                            set_f!(dst, x as f64 / y as f64)
                        }
                        BinKind::IntDiv => {
                            if y == 0 {
                                return Err(Fault::DivisionByZero);
                            }
                            set_i!(dst, crate::executor::ops::floor_div(x, y))
                        }
                        BinKind::Power => set_f!(dst, (x as f64).powf(y as f64)),
                        BinKind::Lt => set_i!(dst, (x < y) as i64),
                        BinKind::Le => set_i!(dst, (x <= y) as i64),
                        BinKind::Gt => set_i!(dst, (x > y) as i64),
                        BinKind::Ge => set_i!(dst, (x >= y) as i64),
                        BinKind::Eq => set_i!(dst, (x == y) as i64),
                        BinKind::Ne => set_i!(dst, (x != y) as i64),
                        BinKind::Add => set_i!(dst, x.wrapping_add(y)),
                        BinKind::Sub => set_i!(dst, x.wrapping_sub(y)),
                        BinKind::Mul => set_i!(dst, x.wrapping_mul(y)),
                        BinKind::In => return Err(Fault::Type),
                    }
                }
                TOp::FAdd { dst, a, b } => set_f!(dst, f!(a) + f!(b)),
                TOp::FSub { dst, a, b } => set_f!(dst, f!(a) - f!(b)),
                TOp::FMul { dst, a, b } => set_f!(dst, f!(a) * f!(b)),
                TOp::FDiv { dst, a, b } => {
                    let (x, y) = (f!(a), f!(b));
                    if y == 0.0 {
                        return Err(Fault::DivisionByZero);
                    }
                    set_f!(dst, x / y)
                }
                TOp::FBin { op, dst, a, b } => {
                    let (x, y) = (f!(a), f!(b));
                    match op {
                        BinKind::IntDiv => {
                            if y == 0.0 {
                                return Err(Fault::DivisionByZero);
                            }
                            set_i!(dst, (x / y).floor() as i64)
                        }
                        BinKind::Power => set_f!(dst, x.powf(y)),
                        BinKind::Lt => set_i!(dst, (x < y) as i64),
                        BinKind::Le => set_i!(dst, (x <= y) as i64),
                        BinKind::Gt => set_i!(dst, (x > y) as i64),
                        BinKind::Ge => set_i!(dst, (x >= y) as i64),
                        BinKind::Eq => set_i!(dst, (x == y) as i64),
                        BinKind::Ne => set_i!(dst, (x != y) as i64),
                        BinKind::Add => set_f!(dst, x + y),
                        BinKind::Sub => set_f!(dst, x - y),
                        BinKind::Mul => set_f!(dst, x * y),
                        BinKind::Div => {
                            if y == 0.0 {
                                return Err(Fault::DivisionByZero);
                            }
                            set_f!(dst, x / y)
                        }
                        BinKind::In => return Err(Fault::Type),
                    }
                }
                TOp::INeg { dst, src } => set_i!(dst, i!(src).wrapping_neg()),
                TOp::FNeg { dst, src } => set_f!(dst, -f!(src)),
                TOp::INot { dst, src } => set_i!(dst, (i!(src) == 0) as i64),
                TOp::FNot { dst, src } => set_i!(dst, (f!(src) == 0.0) as i64),
                TOp::Jump { to } => pc = to as usize,
                TOp::JumpIfI { cond, to } => {
                    if i!(cond) != 0 {
                        pc = to as usize;
                    }
                }
                TOp::JumpIfNotI { cond, to } => {
                    if i!(cond) == 0 {
                        pc = to as usize;
                    }
                }
                TOp::JumpIfF { cond, to } => {
                    if f!(cond) != 0.0 {
                        pc = to as usize;
                    }
                }
                TOp::JumpIfNotF { cond, to } => {
                    if f!(cond) == 0.0 {
                        pc = to as usize;
                    }
                }
                TOp::RangeTestI { current, to } => {
                    if i!(current) >= i!(current + 1) {
                        pc = to as usize;
                    }
                }
                TOp::RangeTestF { current, to } => {
                    if !(f!(current) < f!(current + 1)) {
                        pc = to as usize;
                    }
                }
                TOp::IInc { reg } => set_i!(reg, i!(reg).wrapping_add(1)),
                TOp::FInc { reg } => set_f!(reg, f!(reg) + 1.0),
                TOp::FUnary { f, dst, src } => set_f!(dst, f(f!(src))),
                TOp::FToI { f, dst, src } => set_i!(dst, f(f!(src)) as i64),
                TOp::FTrunc { dst, src } => set_i!(dst, f!(src) as i64),
                TOp::IAbs { dst, src } => set_i!(dst, i!(src).wrapping_abs()),
                TOp::ISign { dst, src } => set_i!(dst, i!(src).signum()),
                TOp::FAbs { dst, src } => set_f!(dst, f!(src).abs()),
                TOp::FSign { dst, src } => set_f!(dst, run::float_sign(f!(src))),
                TOp::IMod { dst, a, b } => {
                    let (x, y) = (i!(a), i!(b));
                    if y == 0 {
                        return Err(Fault::DivisionByZero);
                    }
                    set_i!(dst, x.wrapping_rem_euclid(y))
                }
                TOp::FMod { dst, a, b } => {
                    let (x, y) = (f!(a), f!(b));
                    if y == 0.0 {
                        return Err(Fault::DivisionByZero);
                    }
                    set_f!(dst, x.rem_euclid(y))
                }
                TOp::IMin { dst, a, b } => set_i!(dst, i!(a).min(i!(b))),
                TOp::IMax { dst, a, b } => set_i!(dst, i!(a).max(i!(b))),
                TOp::FMin { dst, a, b } => set_f!(dst, f!(a).min(f!(b))),
                TOp::FMax { dst, a, b } => set_f!(dst, f!(a).max(f!(b))),
                TOp::FPow { dst, a, b } => set_f!(dst, f!(a).powf(f!(b))),
                TOp::FAtan2 { dst, a, b } => set_f!(dst, f!(a).atan2(f!(b))),
                TOp::Call {
                    spec,
                    arg_start,
                    arg_count,
                    ret,
                } => {
                    let caller_args = base + arg_start as usize;
                    let new_base = self.words.len();
                    let result = self.enter(
                        program,
                        arena,
                        spec,
                        new_base,
                        caller_args,
                        arg_count as usize,
                    )?;
                    self.store(base, arg_start, ret, result)?;
                }
                TOp::DynBin { op, dst, a, b, ret } => {
                    let value = run::binary(arena, op, &self.load(base, a), &self.load(base, b))?;
                    self.store(base, dst, ret, value)?;
                }
                TOp::DynNeg { dst, src } => {
                    let value = run::negate(&self.load(base, src))?;
                    boxed!(dst) = value;
                }
                TOp::DynNot { dst, src } => {
                    let value = !run::truthy(&self.load(base, src))?;
                    set_i!(dst, value as i64)
                }
                TOp::DynJumpIf { cond, negate, to } => {
                    if run::truthy(&self.load(base, cond))? != negate {
                        pc = to as usize;
                    }
                }
                TOp::DynRangeTest { current, stop, to } => {
                    let below = run::binary(
                        arena,
                        BinKind::Lt,
                        &self.load(base, current),
                        &self.load(base, stop),
                    )?;
                    if !run::truthy(&below)? {
                        pc = to as usize;
                    }
                }
                TOp::DynInc { reg } => match &mut boxed!(reg) {
                    KVal::Int(n) => *n = n.wrapping_add(1),
                    KVal::Float(f) => *f += 1.0,
                    _ => return Err(Fault::Type),
                },
                TOp::EmptyList { dst } => boxed!(dst) = KVal::List(Arc::new(SmallVec::new())),
                TOp::Append { list, value } => {
                    let value = self.load(base, value);
                    match &mut boxed!(list) {
                        KVal::List(list) => Arc::make_mut(list).push(value),
                        _ => return Err(Fault::Type),
                    }
                }
                TOp::Index { dst, list, index } => {
                    let element = match (&boxed!(list), self.load(base, index)) {
                        (KVal::List(list), KVal::Int(index)) => {
                            list.get(index as usize).ok_or(Fault::Index)?.clone()
                        }
                        _ => return Err(Fault::Type),
                    };
                    boxed!(dst) = element;
                }
                TOp::Len { dst, list } => {
                    let len = match &boxed!(list) {
                        KVal::List(list) => list.len() as i64,
                        _ => return Err(Fault::Type),
                    };
                    set_i!(dst, len)
                }
                TOp::DynNative {
                    intrinsic,
                    dst,
                    args,
                    arity,
                    ret,
                } => {
                    let values: SmallVec<[KVal; 2]> = args[..arity as usize]
                        .iter()
                        .map(|&arg| self.load(base, arg))
                        .collect();
                    let value = run::native(intrinsic, &values)?;
                    self.store(base, dst, ret, value)?;
                }
                TOp::Capture {
                    dst,
                    closure,
                    index,
                } => boxed!(dst) = arena.get(closure).captures[index as usize].clone(),
                TOp::Default {
                    dst,
                    closure,
                    index,
                } => boxed!(dst) = arena.get(closure).defaults[index as usize].clone(),
                TOp::BoxInto { dst, src } => {
                    let value = self.load(base, src);
                    boxed!(dst) = value;
                }
                TOp::Return { src } => return Ok(self.load(base, src)),
            }
        }
    }
}
