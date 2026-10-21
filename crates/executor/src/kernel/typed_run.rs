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
    typed::{Class, Kind, Opnd, Shape, Spec, SpecId, TOp, TypedProgram, VEC_MAX},
    input::Num,
    value::{ClosureArena, KVal},
};

pub struct TVm {
    words: Vec<u64>,
    boxed: Vec<KVal>,
    vecs: Vec<Lanes>,
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
            vecs: Vec::with_capacity(256),
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
            self.vecs.clear();
            self.words.resize(frame, 0);
            self.boxed.resize(frame, KVal::Nil);
            self.vecs.resize(frame, Lanes::default());
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
            self.vecs.truncate(frame);
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
            (Class::Boxed | Class::Rows(_) | Class::Nums(_), value) => self.boxed[slot] = value,
            (Class::Vec(shape), KVal::List(list)) => self.vecs[slot] = Lanes::unbox(shape, &list)?,
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
        self.vecs.resize(base + frame, Lanes::default());
        for i in 0..provided {
            match spec.sig[i] {
                Class::Int | Class::Float => self.words[base + i] = self.words[caller_args + i],
                Class::Boxed | Class::Rows(_) | Class::Nums(_) => {
                    self.boxed[base + i] =
                        std::mem::replace(&mut self.boxed[caller_args + i], KVal::Nil)
                }
                Class::Vec(_) => self.vecs[base + i] = self.vecs[caller_args + i],
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
        self.vecs.truncate(base);
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
        macro_rules! vec {
            ($r:expr) => {
                self.vecs[base + $r as usize]
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
                    // a boxed list updated in place: the register is taken
                    // out so the machine holds the only reference
                    if let Opnd::B(lhs) = a
                        && lhs == dst
                        && !matches!(b, Opnd::B(other) if other == lhs)
                        && matches!(boxed!(lhs), KVal::List(_))
                    {
                        let mut value = std::mem::replace(&mut boxed!(lhs), KVal::Nil);
                        let rhs = self.load(base, b);
                        if !run::binary_in_place(arena, op, &mut value, &rhs)? {
                            value = run::binary(arena, op, &value, &rhs)?;
                        }
                        self.store(base, dst, ret, value)?;
                        continue;
                    }
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
                    let values: SmallVec<[KVal; 3]> = args[..arity as usize]
                        .iter()
                        .map(|&arg| self.load(base, arg))
                        .collect();
                    let value = run::native_in(arena, intrinsic, &values)?;
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
                TOp::VMove { dst, src } => vec!(dst) = vec!(src),
                TOp::VPush {
                    list,
                    value,
                    at,
                    int,
                } => {
                    let word = self.words[base + value as usize];
                    let lanes = &mut vec!(list);
                    lanes.words[at as usize] = word;
                    lanes.set_int(at as usize, int);
                }
                TOp::VIndex {
                    dst,
                    list,
                    index,
                    len,
                } => {
                    let index = i!(index) as usize;
                    if index >= len as usize {
                        return Err(Fault::Index);
                    }
                    self.words[base + dst as usize] = vec!(list).words[index];
                }
                TOp::VIndexB {
                    dst,
                    list,
                    index,
                    len,
                } => {
                    let index = i!(index) as usize;
                    if index >= len as usize {
                        return Err(Fault::Index);
                    }
                    let value = vec!(list).get(index);
                    boxed!(dst) = value;
                }
                TOp::NIndex { dst, list, index } => {
                    let KVal::List(numbers) = &boxed!(list) else {
                        return Err(Fault::Type);
                    };
                    let word = match numbers.get(i!(index) as usize).ok_or(Fault::Index)? {
                        KVal::Int(n) => *n as u64,
                        KVal::Float(f) => f.to_bits(),
                        _ => return Err(Fault::Type),
                    };
                    self.words[base + dst as usize] = word;
                }
                TOp::VRow {
                    dst,
                    list,
                    index,
                    shape,
                } => {
                    let index = i!(index) as usize;
                    let row = match &boxed!(list) {
                        KVal::List(rows) => match rows.get(index).ok_or(Fault::Index)? {
                            KVal::List(row) => Lanes::unbox(shape, row)?,
                            _ => return Err(Fault::Type),
                        },
                        _ => return Err(Fault::Type),
                    };
                    vec!(dst) = row;
                }
                TOp::VBin {
                    op,
                    dst,
                    a,
                    b,
                    len,
                } => {
                    let (x, y) = (vec!(a), vec!(b));
                    let mut out = Lanes::default();
                    for i in 0..len as usize {
                        out.set(i, run::binary_numbers(op, x.get_num(i), y.get_num(i))?);
                    }
                    vec!(dst) = out;
                }
                TOp::VScale {
                    op,
                    dst,
                    v,
                    s,
                    len,
                    vec_left,
                } => {
                    let scalar = match s {
                        Opnd::I(reg) => Num::Int(self.words[base + reg as usize] as i64),
                        Opnd::F(reg) => Num::Float(f64::from_bits(self.words[base + reg as usize])),
                        Opnd::B(reg) => match boxed!(reg) {
                            KVal::Int(n) => Num::Int(n),
                            KVal::Float(f) => Num::Float(f),
                            _ => return Err(Fault::Type),
                        },
                        Opnd::C(_) => return Err(Fault::Type),
                    };
                    let x = vec!(v);
                    let mut out = Lanes::default();
                    for i in 0..len as usize {
                        let element = x.get_num(i);
                        out.set(
                            i,
                            if vec_left {
                                run::binary_numbers(op, element, scalar)?
                            } else {
                                run::binary_numbers(op, scalar, element)?
                            },
                        );
                    }
                    vec!(dst) = out;
                }
                TOp::VNeg { dst, src, len } => {
                    let x = vec!(src);
                    let mut out = Lanes::default();
                    for i in 0..len as usize {
                        out.set(
                            i,
                            match x.get_num(i) {
                                Num::Int(n) => Num::Int(n.wrapping_neg()),
                                Num::Float(f) => Num::Float(-f),
                            },
                        );
                    }
                    vec!(dst) = out;
                }
                TOp::VDot { dst, a, b, len } => {
                    let (x, y) = (vec!(a), vec!(b));
                    let mut sum = 0.0;
                    for i in 0..len as usize {
                        sum += x.float(i) * y.float(i);
                    }
                    set_f!(dst, sum);
                }
                TOp::VCross { dst, a, b } => {
                    let (x, y) = (vec!(a), vec!(b));
                    let u = [0, 1, 2].map(|i| x.float(i));
                    let v = [0, 1, 2].map(|i| y.float(i));
                    let mut out = Lanes::default();
                    out.set(0, Num::Float(u[1] * v[2] - u[2] * v[1]));
                    out.set(1, Num::Float(u[2] * v[0] - u[0] * v[2]));
                    out.set(2, Num::Float(u[0] * v[1] - u[1] * v[0]));
                    vec!(dst) = out;
                }
                TOp::VLerp { dst, a, b, t, len } => {
                    let (x, y, t) = (vec!(a), vec!(b), f!(t));
                    let mut out = Lanes::default();
                    for i in 0..len as usize {
                        out.set(i, Num::Float(lerp_float(x.float(i), y.float(i), t)));
                    }
                    vec!(dst) = out;
                }
                TOp::FLerp { dst, a, b, t } => set_f!(dst, lerp_float(f!(a), f!(b), f!(t))),
                TOp::BoxV { dst, src, len } => {
                    let value = vec!(src).boxed(len);
                    boxed!(dst) = value;
                }
                TOp::UnboxV { reg, shape } => {
                    let lanes = match &boxed!(reg) {
                        KVal::List(list) => Lanes::unbox(shape, list)?,
                        _ => return Err(Fault::Type),
                    };
                    vec!(reg) = lanes;
                }
                TOp::ReturnV { src, len } => return Ok(vec!(src).boxed(len)),
            }
        }
    }
}

/// `run::lerp` of a float towards a number: equal values keep the first
fn lerp_float(a: f64, b: f64, t: f64) -> f64 {
    if a == b { a } else { (1.0 - t) * a + t * b }
}

/// one register's vector slot: elements as words, and which of them are ints
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Lanes {
    words: [u64; VEC_MAX],
    ints: u8,
}

impl Lanes {
    fn set_int(&mut self, i: usize, int: bool) {
        if int {
            self.ints |= 1 << i;
        } else {
            self.ints &= !(1 << i);
        }
    }

    fn set(&mut self, i: usize, value: Num) {
        match value {
            Num::Int(n) => {
                self.words[i] = n as u64;
                self.set_int(i, true);
            }
            Num::Float(f) => {
                self.words[i] = f.to_bits();
                self.set_int(i, false);
            }
        }
    }

    fn get_num(&self, i: usize) -> Num {
        if self.ints & (1 << i) != 0 {
            Num::Int(self.words[i] as i64)
        } else {
            Num::Float(f64::from_bits(self.words[i]))
        }
    }

    /// as the dynamic machine reads a number into float arithmetic
    fn float(&self, i: usize) -> f64 {
        match self.get_num(i) {
            Num::Int(n) => n as f64,
            Num::Float(f) => f,
        }
    }

    fn get(&self, i: usize) -> KVal {
        match self.get_num(i) {
            Num::Int(n) => KVal::Int(n),
            Num::Float(f) => KVal::Float(f),
        }
    }

    fn boxed(&self, len: u8) -> KVal {
        KVal::List(Arc::new((0..len as usize).map(|i| self.get(i)).collect()))
    }

    /// a list of `shape`'s length; elements whose kind the shape knows must have it
    fn unbox(shape: Shape, list: &[KVal]) -> Result<Self, Fault> {
        if list.len() != shape.len as usize {
            return Err(Fault::Type);
        }
        let mut lanes = Lanes::default();
        for (i, element) in list.iter().enumerate() {
            let value = match element {
                KVal::Int(n) => Num::Int(*n),
                KVal::Float(f) => Num::Float(*f),
                _ => return Err(Fault::Type),
            };
            match (shape.kind(i), value) {
                (Kind::Num, _) | (Kind::Int, Num::Int(_)) | (Kind::Float, Num::Float(_)) => {}
                _ => return Err(Fault::Type),
            }
            lanes.set(i, value);
        }
        Ok(lanes)
    }
}
