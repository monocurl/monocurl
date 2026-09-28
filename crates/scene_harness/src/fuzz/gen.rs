//! seeded random program generator. generation is type directed so that most
//! programs are well defined, and every candidate is then run through the
//! reference evaluator: candidates it rejects (nan, huge numbers, runaway
//! loops, known findings) are discarded and the next attempt for the same seed
//! is drawn, so a seed always maps to the same accepted program.
//!
//! the mix is biased towards what the kernel tier compiles: int/float mixing,
//! list arithmetic, loops with accumulators, and nested calls.

use std::rc::Rc;

use super::{BinOp, Body, Expr, Lambda, Native, Param, Program, Stmt, expr_source, reference};

const MAX_ATTEMPTS: u64 = 2_000;

/// small deterministic xorshift64* generator
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        // splitmix64 scrambles nearby seeds into unrelated, nonzero states
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Self((z ^ (z >> 31)) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n.max(1) as u64) as usize
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }

    /// uniform in `lo..=hi`
    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next_u64() % (hi - lo + 1) as u64) as i64
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }

    /// index into `weights`, chosen proportionally
    pub fn weighted(&mut self, weights: &[u32]) -> usize {
        let total: u32 = weights.iter().sum();
        let mut roll = self.next_u64() % u64::from(total.max(1));
        for (i, &weight) in weights.iter().enumerate() {
            if roll < u64::from(weight) {
                return i;
            }
            roll -= u64::from(weight);
        }
        weights.len() - 1
    }
}

/// an accepted program, with the reasons earlier attempts for its seed were discarded
pub struct Generated {
    pub program: Program,
    pub expected: reference::Outcome,
    pub discarded: Vec<&'static str>,
}

/// the program for `seed` and its reference outcome. one seed in five asks for
/// a program that ends in a runtime error, which both sides must then agree on
pub fn generate(seed: u64) -> Generated {
    generate_with(seed, false)
}

/// like `generate`, but the program ends by sampling its lambdas through
/// batch constructors, and never asks for an error: a fault would stop the
/// program before its batches
pub fn generate_batches(seed: u64) -> Generated {
    generate_with(seed, true)
}

fn generate_with(seed: u64, batches: bool) -> Generated {
    let expects_error = !batches && seed % 5 == 4;
    let mut discarded = Vec::new();
    for attempt in 0..MAX_ATTEMPTS {
        let mut rng =
            Rng::new(seed.wrapping_mul(0x1000_0000_01B3) ^ attempt ^ (u64::from(batches) << 40));
        let program = Generator::new(&mut rng, expects_error, batches).program();
        match reference::evaluate(&program) {
            Ok(expected) if expected.error.is_some() == expects_error => {
                return Generated {
                    program,
                    expected,
                    discarded,
                };
            }
            Ok(_) if expects_error => discarded.push("no error raised"),
            Ok(_) => discarded.push("unintended error"),
            Err(reason) => discarded.push(reason),
        }
    }
    panic!("no acceptable program for seed {seed} after {MAX_ATTEMPTS} attempts");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Num {
    Int,
    Float,
    Any,
}

#[derive(Clone, Debug, PartialEq)]
enum Ty {
    Num(Num),
    /// flat list of numbers with a statically known length
    Vec(usize),
    /// flat non-empty list of numbers whose length is only known at runtime
    Seq,
    Mat(usize, usize),
    Func(Rc<Sig>),
    /// the `self` parameter of a self-passing recursive lambda
    Opaque,
}

const ANY: Ty = Ty::Num(Num::Any);
const INT: Ty = Ty::Num(Num::Int);
const FLOAT: Ty = Ty::Num(Num::Float);

#[derive(Debug, PartialEq)]
struct Sig {
    params: Vec<Ty>,
    required: usize,
    ret: Ty,
    /// called as `f(f, depth, ..params)`, recursing at most `max_depth` deep
    recursive: Option<i64>,
}

struct Binding {
    name: Rc<str>,
    ty: Ty,
    mutable: bool,
    /// in `0..=hi` for its whole lifetime (loop counters), so usable as an index
    bound: Option<i64>,
    /// assigning would break a loop's termination or iteration
    frozen: bool,
}

struct Generator<'a> {
    rng: &'a mut Rng,
    bindings: Vec<Binding>,
    /// binding-stack heights at which each enclosing lambda starts; bindings
    /// below the innermost floor are only visible when immutable
    floors: Vec<usize>,
    /// return type of each enclosing lambda
    returns: Vec<Ty>,
    loops: usize,
    names: usize,
    /// statements left before the deliberate fault, in error mode
    fault_in: Option<usize>,
    /// end with a batch section
    batches: bool,
}

impl<'a> Generator<'a> {
    fn new(rng: &'a mut Rng, expects_error: bool, batches: bool) -> Self {
        let fault_in = expects_error.then(|| rng.below(14));
        Self {
            rng,
            bindings: Vec::new(),
            floors: Vec::new(),
            returns: Vec::new(),
            loops: 0,
            names: 0,
            fault_in,
            batches,
        }
    }

    fn program(mut self) -> Program {
        let count = 4 + self.rng.below(9);
        let mut stmts = self.stmts(count);
        let mut batches = Vec::new();
        if self.batches {
            let samplers = 1 + self.rng.below(3);
            for _ in 0..samplers {
                let (setup, lines) = self.batch();
                stmts.extend(setup);
                batches.extend(lines);
            }
        }
        stmts.extend(self.final_prints());
        Program {
            stmts,
            expects_error: self.fault_in.is_some(),
            batches,
        }
    }

    fn name(&mut self, prefix: &str) -> Rc<str> {
        self.names += 1;
        format!("{prefix}{}", self.names).into()
    }

    fn bind(&mut self, name: Rc<str>, ty: Ty, mutable: bool) {
        self.bindings.push(Binding {
            name,
            ty,
            mutable,
            bound: None,
            frozen: false,
        });
    }

    fn floor(&self) -> usize {
        self.floors.last().copied().unwrap_or(0)
    }

    fn visible(&self) -> impl Iterator<Item = &Binding> {
        let floor = self.floor();
        self.bindings
            .iter()
            .enumerate()
            .filter(move |(i, binding)| *i >= floor || !binding.mutable)
            .map(|(_, binding)| binding)
    }

    /// visible bindings whose type can stand in for `ty`
    fn candidates(&self, ty: &Ty) -> Vec<Rc<str>> {
        self.visible()
            .filter(|binding| fits(&binding.ty, ty))
            .map(|binding| binding.name.clone())
            .collect()
    }

    /// mutable bindings in the current lambda (or top level) that may be assigned
    fn assignable(&self, filter: impl Fn(&Ty) -> bool) -> Vec<(Rc<str>, Ty)> {
        self.bindings[self.floor()..]
            .iter()
            .filter(|binding| binding.mutable && !binding.frozen && filter(&binding.ty))
            .map(|binding| (binding.name.clone(), binding.ty.clone()))
            .collect()
    }

    fn scoped<T>(&mut self, body: impl FnOnce(&mut Self) -> T) -> T {
        let mark = self.bindings.len();
        let result = body(self);
        self.bindings.truncate(mark);
        result
    }

    // ---- statements ----

    fn stmts(&mut self, count: usize) -> Vec<Stmt> {
        (0..count).flat_map(|_| self.stmt()).collect()
    }

    fn stmt(&mut self) -> Vec<Stmt> {
        if let Some(remaining) = self.fault_in.as_mut() {
            if *remaining == 0 {
                self.fault_in = Some(usize::MAX);
                return vec![self.fault()];
            }
            *remaining -= 1;
        }

        let top_level = self.floors.is_empty();
        let nesting = self.loops + self.floors.len();
        // a lambda created inside a lambda keeps the kernel tier out, so batch
        // mode makes them rare there
        let closures = if self.batches && !top_level { 4 } else { 1 };
        let accumulators = self.assignable(|ty| matches!(ty, Ty::Num(_))).len();
        let lists = self.assignable(|ty| matches!(ty, Ty::Seq)).len();
        let vectors = self
            .assignable(|ty| matches!(ty, Ty::Vec(_) | Ty::Seq))
            .len();

        let weights = [
            14, // let value
            10, // var
            if self.floors.len() < 2 {
                8 / closures
            } else {
                0
            }, // let lambda
            if self.floors.len() < 2 {
                3 / closures
            } else {
                0
            }, // recursive lambda
            if accumulators > 0 { 14 } else { 0 }, // accumulate
            if lists > 0 { 6 } else { 0 }, // append
            if vectors > 0 { 4 } else { 0 }, // indexed store
            if nesting < 3 { 8 } else { 0 }, // if
            if nesting < 3 { 5 } else { 0 }, // while
            if nesting < 3 { 7 } else { 0 }, // for range
            if nesting < 3 { 5 } else { 0 }, // for in
            if top_level { 7 } else { 0 }, // print
        ];
        match self.rng.weighted(&weights) {
            0 => {
                let ty = self.value_ty();
                let value = self.expr(&ty, 3);
                let name = self.name(prefix(&ty));
                self.bind(name.clone(), ty, false);
                vec![Stmt::Let(name, value)]
            }
            1 => {
                let ty = match self.rng.below(10) {
                    0..=5 => Ty::Num(*self.rng.pick(&[Num::Int, Num::Float, Num::Any])),
                    6..=8 => Ty::Seq,
                    _ => Ty::Vec(2 + self.rng.below(2)),
                };
                let value = self.expr(&ty, 2);
                let name = self.name(prefix(&ty));
                self.bind(name.clone(), ty, true);
                vec![Stmt::Var(name, value)]
            }
            2 => {
                let sig = Rc::new(self.sig(true));
                let value = self.lambda(&sig);
                let name = self.name("f");
                self.bind(name.clone(), Ty::Func(sig), false);
                vec![Stmt::Let(name, value)]
            }
            3 => vec![self.recursive_lambda()],
            4 => vec![self.accumulate()],
            5 => {
                let (name, _) = self
                    .rng
                    .pick(&self.assignable(|ty| matches!(ty, Ty::Seq)))
                    .clone();
                vec![Stmt::Append(name, self.expr(&ANY, 2))]
            }
            6 => {
                let (name, ty) = self
                    .rng
                    .pick(&self.assignable(|ty| matches!(ty, Ty::Vec(_) | Ty::Seq)))
                    .clone();
                let index = self.index_into(&name, &ty);
                vec![Stmt::AssignIndex(name, index, self.expr(&ANY, 2))]
            }
            7 => vec![self.if_chain()],
            8 => self.while_loop(),
            9 => vec![self.for_range()],
            10 => vec![self.for_in()],
            _ => vec![Stmt::Print(self.printable())],
        }
    }

    /// in batch mode, whether to steer away from a native the kernel tier
    /// does not model (`lerp`, `range` as a value) inside a lambda. usually
    /// yes, so most sampled bodies compile, but not always, so the fallback
    /// stays covered
    fn kernel_hostile(&mut self) -> bool {
        self.batches && !self.floors.is_empty() && self.rng.chance(80)
    }

    fn value_ty(&mut self) -> Ty {
        match self.rng.below(20) {
            0..=10 => Ty::Num(*self.rng.pick(&[Num::Int, Num::Float, Num::Any, Num::Any])),
            11..=15 => Ty::Vec(2 + self.rng.below(3)),
            16..=17 => Ty::Seq,
            _ => Ty::Mat(2, 2 + self.rng.below(2)),
        }
    }

    /// `x = x op e`, the accumulator shape loops are built around
    fn accumulate(&mut self) -> Stmt {
        let (name, ty) = self
            .rng
            .pick(&self.assignable(|ty| matches!(ty, Ty::Num(_))))
            .clone();
        let Ty::Num(num) = ty else { unreachable!() };
        let value = if self.rng.chance(75) {
            let op = *self
                .rng
                .pick(&[BinOp::Add, BinOp::Add, BinOp::Sub, BinOp::Mul]);
            let operand = match num {
                // int accumulators must stay int for their uses as indices
                Num::Int => self.num(Num::Int, 2),
                _ => self.num(Num::Any, 2),
            };
            let operand = if op == BinOp::Mul {
                self.small_factor(num, operand)
            } else {
                operand
            };
            binary(op, Expr::Name(name.clone()), operand)
        } else {
            self.num(num, 2)
        };
        Stmt::Assign(name, value)
    }

    /// multiplying an accumulator in a loop grows it geometrically; keep the
    /// factor near one so the reference rarely has to reject the program
    fn small_factor(&mut self, num: Num, operand: Expr) -> Expr {
        if num == Num::Int {
            Expr::Int(self.rng.int(-3, 3))
        } else if self.rng.chance(50) {
            self.literal(Num::Any)
        } else {
            Expr::Native(Native::Sin, vec![operand])
        }
    }

    fn if_chain(&mut self) -> Stmt {
        let arms = 1 + self.rng.below(3);
        let branches = (0..arms)
            .map(|_| {
                let condition = self.condition();
                (condition, self.branch_body())
            })
            .collect();
        let otherwise = self.rng.chance(50).then(|| self.branch_body());
        Stmt::If(branches, otherwise)
    }

    /// inside a lambda a branch may return early
    fn branch_body(&mut self) -> Vec<Stmt> {
        self.scoped(|this| {
            let count = 1 + this.rng.below(2);
            let mut body = this.stmts(count);
            if let Some(ret) = this.returns.last().cloned()
                && this.rng.chance(35)
            {
                body.push(Stmt::Return(this.expr(&ret, 2)));
            }
            body
        })
    }

    fn condition(&mut self) -> Expr {
        match self.rng.below(6) {
            0 => {
                let lhs = self.num(Num::Any, 1);
                let rhs = self.num(Num::Any, 1);
                binary(*self.rng.pick(&[BinOp::And, BinOp::Or]), lhs, rhs)
            }
            1 => self.num(Num::Any, 2),
            _ => self.comparison(2),
        }
    }

    fn comparison(&mut self, depth: u32) -> Expr {
        let op = *self.rng.pick(&[
            BinOp::Lt,
            BinOp::Le,
            BinOp::Gt,
            BinOp::Ge,
            BinOp::Eq,
            BinOp::Ne,
        ]);
        if matches!(op, BinOp::Eq | BinOp::Ne) && self.rng.chance(20) {
            let n = 2 + self.rng.below(2);
            return binary(
                op,
                self.expr(&Ty::Vec(n), depth - 1),
                self.expr(&Ty::Vec(n), depth - 1),
            );
        }
        binary(
            op,
            self.num(Num::Any, depth - 1),
            self.num(Num::Any, depth - 1),
        )
    }

    /// `var c = 0; while (c < k) { ..; c = c + 1 }`: terminates by construction
    fn while_loop(&mut self) -> Vec<Stmt> {
        let counter = self.name("c");
        let limit = self.rng.int(1, 6);
        self.bindings.push(Binding {
            name: counter.clone(),
            ty: INT,
            mutable: true,
            bound: Some(limit),
            frozen: true,
        });
        let body = self.loop_body(|_| {});
        let mut body = body;
        body.push(Stmt::Assign(
            counter.clone(),
            binary(BinOp::Add, Expr::Name(counter.clone()), Expr::Int(1)),
        ));
        // after the loop the counter is an ordinary int variable
        if let Some(binding) = self.bindings.iter_mut().rev().find(|b| b.name == counter) {
            binding.frozen = false;
            binding.bound = None;
        }
        vec![
            Stmt::Var(counter.clone(), Expr::Int(0)),
            Stmt::While(
                binary(BinOp::Lt, Expr::Name(counter), Expr::Int(limit)),
                body,
            ),
        ]
    }

    fn for_range(&mut self) -> Stmt {
        let start = self.rng.int(-2, 3);
        let stop = start + self.rng.int(0, 6);
        let binder = self.name("i");
        let body = self.loop_body(|this| {
            this.bindings.push(Binding {
                name: binder.clone(),
                ty: INT,
                mutable: false,
                bound: (start >= 0).then_some(stop - 1),
                frozen: false,
            });
        });
        let iterable = Expr::Native(Native::Range, vec![Expr::Int(start), Expr::Int(stop)]);
        Stmt::For(binder, iterable, body)
    }

    fn for_in(&mut self) -> Stmt {
        let (iterable, element) = if self.rng.chance(25) {
            let columns = 2 + self.rng.below(2);
            (self.expr(&Ty::Mat(2, columns), 2), Ty::Vec(columns))
        } else {
            (self.expr(&Ty::Seq, 2), ANY)
        };
        let binder = self.name("x");

        // the loop must not mutate what it iterates
        let mut names = Vec::new();
        names_in(&iterable, &mut names);
        let frozen: Vec<usize> = self
            .bindings
            .iter()
            .enumerate()
            .filter(|(_, binding)| !binding.frozen && names.contains(&binding.name))
            .map(|(i, _)| i)
            .collect();
        frozen.iter().for_each(|&i| self.bindings[i].frozen = true);

        let body = self.loop_body(|this| this.bind(binder.clone(), element, false));

        frozen.iter().for_each(|&i| self.bindings[i].frozen = false);
        Stmt::For(binder, iterable, body)
    }

    fn loop_body(&mut self, bind: impl FnOnce(&mut Self)) -> Vec<Stmt> {
        self.loops += 1;
        let body = self.scoped(|this| {
            bind(this);
            let count = 1 + this.rng.below(3);
            let mut body = this.stmts(count);
            // loops exist to accumulate; make sure most of them do
            if this.rng.chance(60) && !this.assignable(|ty| matches!(ty, Ty::Num(_))).is_empty() {
                body.push(this.accumulate());
            }
            body
        });
        self.loops -= 1;
        body
    }

    fn printable(&mut self) -> Expr {
        let ty = self.value_ty();
        self.expr(&ty, 3)
    }

    /// print whatever the program built, so a wrong value anywhere shows up
    fn final_prints(&mut self) -> Vec<Stmt> {
        let mut prints: Vec<Stmt> = self
            .visible()
            .filter(|binding| matches!(binding.ty, Ty::Num(_) | Ty::Vec(_) | Ty::Seq | Ty::Mat(..)))
            .map(|binding| Stmt::Print(Expr::Name(binding.name.clone())))
            .collect();
        let start = prints.len().saturating_sub(8);
        prints.drain(..start);

        let functions: Vec<(Rc<str>, Rc<Sig>)> = self
            .visible()
            .filter_map(|binding| match &binding.ty {
                Ty::Func(sig) => Some((binding.name.clone(), sig.clone())),
                _ => None,
            })
            .collect();
        for (name, sig) in functions.iter().rev().take(3) {
            let call = self.call(name, sig, 2);
            let call = match &sig.ret {
                // a returned lambda is applied once more so a value gets printed
                Ty::Func(inner) => self.apply(call, inner, 2),
                _ => call,
            };
            prints.push(Stmt::Print(call));
        }
        prints
    }

    // ---- batch constructors ----

    /// one sampler: a `let` binding the sampled lambda and prints of it at
    /// points the constructor also samples (both checked by the reference),
    /// and the source lines that run it as a batch
    fn batch(&mut self) -> (Vec<Stmt>, Vec<String>) {
        let kind = *self.rng.pick(&Sampling::ALL);
        let (params, ret) = kind.shape();
        let targets: Vec<(Rc<str>, Rc<Sig>)> = self
            .visible()
            .filter_map(|binding| match &binding.ty {
                Ty::Func(sig) => Some((binding.name.clone(), sig.clone())),
                _ => None,
            })
            .collect();
        let target =
            (!targets.is_empty() && self.rng.chance(85)).then(|| self.rng.pick(&targets).clone());
        let sampler = self.sampler(&params, &ret, target);

        let name = self.name("w");
        let mut setup = vec![Stmt::Let(name.clone(), sampler.clone())];
        self.bind(
            name.clone(),
            Ty::Func(Rc::new(Sig {
                params: params.clone(),
                required: params.len(),
                ret: ret.clone(),
                recursive: None,
            })),
            false,
        );
        let domain = kind.domain(self.rng);
        for _ in 0..1 + self.rng.below(2) {
            let args = domain.probe(self.rng);
            setup.push(Stmt::Print(Expr::Call(
                Box::new(Expr::Name(name.clone())),
                args,
            )));
        }

        // an expression body can stand inline in the constructor call
        let inline =
            matches!(&sampler, Expr::Lambda(lambda) if matches!(lambda.body, Body::Expr(_)));
        let callee = if inline && self.rng.chance(40) {
            expr_source(&sampler)
        } else {
            name.to_string()
        };
        let mesh = self.name("g");
        let mut lines = vec![format!(
            "mesh {mesh} = {}",
            domain.constructor(&callee, self.rng)
        )];
        lines.push(format!("print {mesh}"));
        // colours only reach the transcript through verify mode, but geometry
        // can be read back
        match &domain {
            Domain::Line { .. } | Domain::Grid { colored: false, .. } | Domain::Points { .. } => {
                lines.push(format!(
                    "print [mesh_center({mesh}), mesh_width({mesh}), mesh_height({mesh})]"
                ));
            }
            _ => {}
        }
        if let Domain::Points { grid: false, .. } = domain {
            lines.push(format!("print mesh_vertex_set({mesh})"));
        }
        // a field prints each sample through the interpreter, whose calls the
        // tier may take one at a time
        if let (Domain::Line { lo, hi, .. }, true) = (&domain, self.rng.chance(30)) {
            let field = self.name("g");
            lines.push(format!(
                "mesh {field} = Field(|pos, idx| block {{ print {name}(pos[0]) }}, [{lo}, {hi}, 5], [0, 1, 1])"
            ));
        }
        (setup, lines)
    }

    /// a lambda over the constructor's sample arguments, usually built around
    /// a call to `target` so the batch reaches the program's own lambdas
    fn sampler(&mut self, params: &[Ty], ret: &Ty, target: Option<(Rc<str>, Rc<Sig>)>) -> Expr {
        let mark = self.bindings.len();
        self.floors.push(mark);
        self.returns.push(ret.clone());
        let saved_loops = std::mem::take(&mut self.loops);

        let params: Vec<Param> = params
            .iter()
            .map(|ty| {
                let name = self.name("s");
                self.bind(name.clone(), ty.clone(), false);
                Param {
                    name,
                    default: None,
                }
            })
            .collect();
        let block = self.rng.chance(50);
        let mut stmts = if block {
            let count = 1 + self.rng.below(3);
            self.stmts(count)
        } else {
            Vec::new()
        };
        let core = target.map(|(name, sig)| self.focused_call(&name, &sig));
        let value = self.sample_value(ret, core);
        let body = if block {
            stmts.push(Stmt::Return(value));
            Body::Block(stmts)
        } else {
            Body::Expr(value)
        };

        self.loops = saved_loops;
        self.returns.pop();
        self.floors.pop();
        self.bindings.truncate(mark);
        Expr::Lambda(Rc::new(Lambda { params, body }))
    }

    /// the sample arguments as scalars: float params and the components of
    /// list params
    fn sample_scalars(&self) -> Vec<Expr> {
        self.bindings[self.floor()..]
            .iter()
            .filter(|binding| binding.name.starts_with('s'))
            .flat_map(|binding| match binding.ty {
                Ty::Vec(n) => (0..n)
                    .map(|i| {
                        Expr::Index(
                            Box::new(Expr::Name(binding.name.clone())),
                            Box::new(Expr::Int(i as i64)),
                        )
                    })
                    .collect(),
                _ => vec![Expr::Name(binding.name.clone())],
            })
            .collect()
    }

    /// a number (or a list of numbers) that depends on the sample arguments
    fn sample_arg(&mut self, ty: &Ty) -> Expr {
        let scalars = self.sample_scalars();
        let scalar = |this: &mut Self| this.rng.pick(&scalars).clone();
        match ty {
            Ty::Num(Num::Int) => match self.rng.below(4) {
                0 => self.literal(Num::Int),
                1 | 2 => {
                    let scaled = binary(BinOp::Mul, scalar(self), Expr::Int(self.rng.int(1, 4)));
                    let native = *self.rng.pick(&[Native::Floor, Native::Round, Native::Ceil]);
                    Expr::Native(native, vec![scaled])
                }
                _ => self.num(Num::Int, 1),
            },
            Ty::Num(_) => match self.rng.below(7) {
                0..=2 => scalar(self),
                3 => {
                    let op = *self.rng.pick(&[BinOp::Mul, BinOp::Add, BinOp::Sub]);
                    binary(op, scalar(self), self.literal(Num::Any))
                }
                4 => self.literal(Num::Any),
                _ => self.num(Num::Any, 1),
            },
            Ty::Vec(n) if self.rng.chance(60) => {
                Expr::List((0..*n).map(|_| self.sample_arg(&ANY)).collect())
            }
            other => self.expr(other, 1),
        }
    }

    /// a call to `name` fed from the sample arguments, reduced to a number
    fn focused_call(&mut self, name: &Rc<str>, sig: &Sig) -> Expr {
        let callee = Expr::Name(name.clone());
        let args = match sig.recursive {
            Some(max_depth) => {
                let mut args = vec![callee.clone(), Expr::Int(self.rng.int(0, max_depth))];
                args.extend(sig.params[1..].iter().map(|ty| self.sample_arg(ty)));
                args
            }
            None => {
                let count = self.rng.int(sig.required as i64, sig.params.len() as i64) as usize;
                sig.params[..count]
                    .iter()
                    .map(|ty| self.sample_arg(ty))
                    .collect()
            }
        };
        let call = Expr::Call(Box::new(callee), args);
        match &sig.ret {
            Ty::Vec(n) => {
                let index = self.index(*n, 1);
                Expr::Index(Box::new(call), Box::new(index))
            }
            Ty::Func(inner) => {
                let args = inner.params.iter().map(|ty| self.sample_arg(ty)).collect();
                Expr::Call(Box::new(call), args)
            }
            _ => call,
        }
    }

    /// a value of type `ret` built around `core`
    fn sample_value(&mut self, ret: &Ty, core: Option<Expr>) -> Expr {
        match ret {
            Ty::Vec(n) => {
                let slot = self.rng.below(*n);
                let mut core = core;
                let items = (0..*n)
                    .map(|i| match if i == slot { core.take() } else { None } {
                        Some(core) => self.sample_number(Some(core)),
                        None if self.rng.chance(40) => self.sample_arg(&ANY),
                        None => self.sample_number(None),
                    })
                    .collect();
                Expr::List(items)
            }
            _ => self.sample_number(core),
        }
    }

    fn sample_number(&mut self, core: Option<Expr>) -> Expr {
        let Some(core) = core else {
            return self.num(Num::Any, 3);
        };
        match self.rng.below(5) {
            0 | 1 => core,
            2 => {
                let op =
                    *self
                        .rng
                        .pick(&[BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::And, BinOp::Or]);
                binary(op, core, self.num(Num::Any, 1))
            }
            3 => {
                let native = *self.rng.pick(&[Native::Min, Native::Max]);
                Expr::Native(native, vec![core, self.sample_arg(&ANY)])
            }
            _ => Expr::Native(Native::Sin, vec![core]),
        }
    }

    // ---- deliberate faults ----

    fn fault(&mut self) -> Stmt {
        let expr = match self.rng.below(10) {
            0 => {
                let k = self.rng.int(1, 5);
                binary(
                    BinOp::Div,
                    self.num(Num::Any, 1),
                    binary(BinOp::Sub, Expr::Int(k), Expr::Int(k)),
                )
            }
            1 => binary(BinOp::IntDiv, self.num(Num::Any, 1), Expr::Int(0)),
            2 => Expr::Native(Native::Mod, vec![self.num(Num::Any, 1), Expr::Int(0)]),
            3 => {
                let list = self.expr(&Ty::Seq, 1);
                let past = self.rng.int(0, 3);
                let index = match &list {
                    Expr::Name(_) => binary(
                        BinOp::Add,
                        Expr::Native(Native::Len, vec![list.clone()]),
                        Expr::Int(past),
                    ),
                    _ => Expr::Int(5 + past),
                };
                Expr::Index(Box::new(list), Box::new(index))
            }
            4 => {
                let n = 2 + self.rng.below(2);
                let lhs = self.expr(&Ty::Vec(n), 1);
                let rhs = self.expr(&Ty::Vec(n + 1), 1);
                if self.rng.chance(50) {
                    binary(*self.rng.pick(&[BinOp::Add, BinOp::Sub]), lhs, rhs)
                } else {
                    Expr::Native(Native::Dot, vec![lhs, rhs])
                }
            }
            5 => binary(
                *self.rng.pick(&[BinOp::Add, BinOp::Sub, BinOp::Lt]),
                Expr::List(vec![self.num(Num::Any, 1)]),
                self.num(Num::Any, 1),
            ),
            6 => binary(
                *self.rng.pick(&[BinOp::Mul, BinOp::IntDiv, BinOp::Pow]),
                self.expr(&Ty::Vec(2), 1),
                self.expr(&Ty::Vec(2), 1),
            ),
            7 => {
                let operand = Expr::List(vec![self.num(Num::Any, 1)]);
                match self.rng.below(3) {
                    0 => Expr::Not(Box::new(operand)),
                    1 => binary(BinOp::And, operand, Expr::Int(1)),
                    _ => binary(BinOp::Or, operand, Expr::Int(1)),
                }
            }
            8 => {
                let functions: Vec<(Rc<str>, Rc<Sig>)> = self
                    .visible()
                    .filter_map(|binding| match &binding.ty {
                        Ty::Func(sig) if sig.recursive.is_none() => {
                            Some((binding.name.clone(), sig.clone()))
                        }
                        _ => None,
                    })
                    .collect();
                let numbers = self.candidates(&ANY);
                if let Some((name, sig)) = functions.first().cloned() {
                    let mut args: Vec<Expr> =
                        sig.params.iter().map(|ty| self.expr(ty, 1)).collect();
                    args.push(self.num(Num::Any, 1));
                    Expr::Call(Box::new(Expr::Name(name)), args)
                } else if let Some(name) = numbers.first().cloned() {
                    // calling a number
                    Expr::Call(Box::new(Expr::Name(name)), vec![self.num(Num::Any, 1)])
                } else {
                    binary(BinOp::Div, Expr::Int(1), Expr::Int(0))
                }
            }
            _ => {
                let index = self.literal(Num::Float);
                Expr::Index(Box::new(self.expr(&Ty::Vec(3), 1)), Box::new(index))
            }
        };
        if self.floors.is_empty() {
            Stmt::Print(expr)
        } else {
            let name = self.name("z");
            Stmt::Let(name, expr)
        }
    }

    // ---- lambdas ----

    fn sig(&mut self, allow_nested: bool) -> Sig {
        let count = 1 + self.rng.below(3);
        let params: Vec<Ty> = (0..count)
            .map(|_| match self.rng.below(20) {
                0..=12 => ANY,
                13..=14 => INT,
                15..=18 => Ty::Vec(2 + self.rng.below(2)),
                _ if allow_nested => Ty::Func(Rc::new(self.simple_sig())),
                _ => ANY,
            })
            .collect();
        // trailing number params may take literal defaults
        let defaults = params
            .iter()
            .rev()
            .take_while(|ty| matches!(ty, Ty::Num(_)))
            .count()
            .min(if self.rng.chance(35) { 2 } else { 0 });
        let ret = match self.rng.below(10) {
            0..=4 => ANY,
            5 => INT,
            6 => FLOAT,
            7..=8 => Ty::Vec(2 + self.rng.below(2)),
            _ if allow_nested => Ty::Func(Rc::new(self.simple_sig())),
            _ => ANY,
        };
        Sig {
            required: params.len() - defaults.min(params.len().saturating_sub(1)),
            params,
            ret,
            recursive: None,
        }
    }

    fn simple_sig(&mut self) -> Sig {
        Sig {
            params: vec![ANY; 1 + self.rng.below(2)],
            required: 0,
            ret: ANY,
            recursive: None,
        }
        .with_all_required()
    }

    fn lambda(&mut self, sig: &Sig) -> Expr {
        self.lambda_with(sig, |_| Vec::new())
    }

    /// a lambda literal for `sig`; `prefix` binds any leading params the
    /// signature does not describe (the self parameter) and returns them
    fn lambda_with(&mut self, sig: &Sig, prefix: impl FnOnce(&mut Self) -> Vec<Param>) -> Expr {
        let mark = self.bindings.len();
        self.floors.push(mark);
        self.returns.push(sig.ret.clone());
        let saved_loops = std::mem::take(&mut self.loops);

        let mut params = prefix(self);
        for (i, ty) in sig.params.iter().enumerate() {
            let name = self.name("p");
            let default = (i >= sig.required).then(|| self.literal(Num::Any));
            self.bind(name.clone(), ty.clone(), false);
            params.push(Param { name, default });
        }

        let body = if sig.recursive.is_some() {
            Body::Block(self.recursive_body(&params, sig))
        } else if self.rng.chance(45) {
            Body::Expr(self.expr(&sig.ret, 3))
        } else {
            let count = 1 + self.rng.below(4);
            let mut stmts = self.stmts(count);
            stmts.push(Stmt::Return(self.expr(&sig.ret, 3)));
            Body::Block(stmts)
        };

        self.loops = saved_loops;
        self.returns.pop();
        self.floors.pop();
        self.bindings.truncate(mark);
        Expr::Lambda(Rc::new(Lambda { params, body }))
    }

    /// `let r = |me, n, ..| { if (n < 1) { return base } return combine(me(me, n - 1, ..)) }`
    fn recursive_lambda(&mut self) -> Stmt {
        let extra = self.rng.below(3);
        let sig = Sig {
            params: std::iter::once(INT).chain(vec![ANY; extra]).collect(),
            required: 1 + extra,
            ret: ANY,
            recursive: Some(if self.rng.chance(40) { 7 } else { 12 }),
        };
        let value = self.lambda_with(&sig, |this| {
            let name = this.name("me");
            this.bind(name.clone(), Ty::Opaque, false);
            vec![Param {
                name,
                default: None,
            }]
        });
        let name = self.name("r");
        self.bind(name.clone(), Ty::Func(Rc::new(sig)), false);
        Stmt::Let(name, value)
    }

    fn recursive_body(&mut self, params: &[Param], sig: &Sig) -> Vec<Stmt> {
        let me = Expr::Name(params[0].name.clone());
        let depth = Expr::Name(params[1].name.clone());
        let base = self.expr(&ANY, 2);
        let recurse = |this: &mut Self| {
            let mut args = vec![me.clone(), binary(BinOp::Sub, depth.clone(), Expr::Int(1))];
            args.extend(sig.params[1..].iter().map(|ty| this.expr(ty, 1)));
            Expr::Call(Box::new(me.clone()), args)
        };
        let first = recurse(self);
        // two recursive calls make a fib-shaped tree, so those stay shallow
        let combined = match (sig.recursive, self.rng.below(3)) {
            (Some(7), _) => binary(BinOp::Add, first, recurse(self)),
            (_, 0) => binary(BinOp::Add, first, self.num(Num::Any, 2)),
            (_, 1) => binary(
                BinOp::Mul,
                Expr::Native(Native::Cos, vec![self.num(Num::Any, 1)]),
                first,
            ),
            _ => Expr::Native(Native::Max, vec![first, self.num(Num::Any, 2)]),
        };
        vec![
            Stmt::If(
                vec![(
                    binary(BinOp::Lt, depth, Expr::Int(1)),
                    vec![Stmt::Return(base)],
                )],
                None,
            ),
            Stmt::Return(combined),
        ]
    }

    fn call(&mut self, name: &Rc<str>, sig: &Sig, depth: u32) -> Expr {
        let callee = Expr::Name(name.clone());
        if let Some(max_depth) = sig.recursive {
            let mut args = vec![callee.clone(), Expr::Int(self.rng.int(0, max_depth))];
            args.extend(
                sig.params[1..]
                    .iter()
                    .map(|ty| self.expr(ty, depth.saturating_sub(1))),
            );
            return Expr::Call(Box::new(callee), args);
        }
        self.apply(callee, sig, depth)
    }

    fn apply(&mut self, callee: Expr, sig: &Sig, depth: u32) -> Expr {
        let count = self.rng.int(sig.required as i64, sig.params.len() as i64) as usize;
        let args = sig.params[..count]
            .iter()
            .map(|ty| self.expr(ty, depth.saturating_sub(1)))
            .collect();
        Expr::Call(Box::new(callee), args)
    }

    /// a call to some visible function returning `ty`, if there is one
    fn call_returning(&mut self, ty: &Ty, depth: u32) -> Option<Expr> {
        let functions: Vec<(Rc<str>, Rc<Sig>)> = self
            .visible()
            .filter_map(|binding| match &binding.ty {
                Ty::Func(sig) if fits(&sig.ret, ty) => Some((binding.name.clone(), sig.clone())),
                _ => None,
            })
            .collect();
        if functions.is_empty() {
            return None;
        }
        let (name, sig) = self.rng.pick(&functions).clone();
        Some(self.call(&name, &sig, depth))
    }

    // ---- expressions ----

    fn expr(&mut self, ty: &Ty, depth: u32) -> Expr {
        match ty {
            Ty::Num(num) => self.num(*num, depth),
            Ty::Vec(n) => self.vector(*n, depth),
            Ty::Seq => self.seq(depth),
            Ty::Mat(rows, columns) => self.matrix(*rows, *columns, depth),
            Ty::Func(sig) => {
                let names = self.candidates(ty);
                if !names.is_empty() && self.rng.chance(40) {
                    return Expr::Name(self.rng.pick(&names).clone());
                }
                // inline lambdas keep expression bodies so they print on one line
                let sig = sig.clone();
                self.inline_lambda(&sig)
            }
            Ty::Opaque => unreachable!("opaque bindings are never generated"),
        }
    }

    fn inline_lambda(&mut self, sig: &Sig) -> Expr {
        let mark = self.bindings.len();
        self.floors.push(mark);
        let params = sig
            .params
            .iter()
            .map(|ty| {
                let name = self.name("p");
                self.bind(name.clone(), ty.clone(), false);
                Param {
                    name,
                    default: None,
                }
            })
            .collect();
        let body = Body::Expr(self.expr(&sig.ret, 2));
        self.floors.pop();
        self.bindings.truncate(mark);
        Expr::Lambda(Rc::new(Lambda { params, body }))
    }

    fn literal(&mut self, num: Num) -> Expr {
        let float = match num {
            Num::Int => false,
            Num::Float => true,
            Num::Any => self.rng.chance(45),
        };
        if !float {
            return Expr::Int(match self.rng.below(10) {
                0..=7 => self.rng.int(-9, 9),
                _ => self.rng.int(-300, 300),
            });
        }
        let value = match self.rng.below(10) {
            // integral floats are where int/float confusion hides
            0..=1 => self.rng.int(-9, 9) as f64,
            2..=5 => self.rng.int(-80, 80) as f64 / 8.0,
            _ => self.rng.int(-9999, 9999) as f64 / 1000.0,
        };
        Expr::Float(value)
    }

    fn leaf(&mut self, num: Num) -> Expr {
        let names = self.candidates(&Ty::Num(num));
        if !names.is_empty() && self.rng.chance(60) {
            return Expr::Name(self.rng.pick(&names).clone());
        }
        if num == Num::Any && self.rng.chance(20) {
            let lists = self.candidates(&Ty::Seq);
            if !lists.is_empty() {
                let name = self.rng.pick(&lists).clone();
                let ty = self.binding_ty(&name);
                let index = self.index_into(&name, &ty);
                return Expr::Index(Box::new(Expr::Name(name)), Box::new(index));
            }
        }
        self.literal(num)
    }

    fn binding_ty(&self, name: &Rc<str>) -> Ty {
        self.bindings
            .iter()
            .rev()
            .find(|binding| &binding.name == name)
            .map(|binding| binding.ty.clone())
            .expect("binding exists")
    }

    fn num(&mut self, num: Num, depth: u32) -> Expr {
        if depth == 0 || self.rng.chance(22) {
            return self.leaf(num);
        }
        let d = depth - 1;
        match num {
            Num::Int => match self.rng.below(14) {
                0..=3 => {
                    let op = *self.rng.pick(&[BinOp::Add, BinOp::Sub, BinOp::Mul]);
                    binary(op, self.num(Num::Int, d), self.num(Num::Int, d))
                }
                4 => binary(BinOp::IntDiv, self.num(Num::Any, d), self.nonzero(d)),
                5 => {
                    let native = *self.rng.pick(&[
                        Native::Floor,
                        Native::Ceil,
                        Native::Round,
                        Native::Trunc,
                    ]);
                    Expr::Native(native, vec![self.num(Num::Any, d)])
                }
                6 => Expr::Native(Native::Len, vec![self.expr(&Ty::Seq, d)]),
                7 => self.comparison(depth),
                8 => {
                    let native = *self.rng.pick(&[Native::Min, Native::Max, Native::Mod]);
                    let rhs = match native {
                        Native::Mod => {
                            Expr::Int(self.rng.int(1, 7) * if self.rng.chance(20) { -1 } else { 1 })
                        }
                        _ => self.num(Num::Int, d),
                    };
                    Expr::Native(native, vec![self.num(Num::Int, d), rhs])
                }
                9 => Expr::Native(Native::Abs, vec![self.num(Num::Int, d)]),
                10 => Expr::Neg(Box::new(self.num(Num::Int, d))),
                11 => {
                    let op = *self.rng.pick(&[BinOp::And, BinOp::Or]);
                    binary(op, self.num(Num::Any, d), self.num(Num::Int, d))
                }
                12 => Expr::Not(Box::new(self.num(Num::Any, d))),
                _ => self
                    .call_returning(&INT, d)
                    .unwrap_or_else(|| self.num(Num::Int, d)),
            },
            Num::Float => match self.rng.below(13) {
                0..=2 => {
                    let op = *self.rng.pick(&[BinOp::Add, BinOp::Sub, BinOp::Mul]);
                    let (lhs, rhs) = (self.num(Num::Float, d), self.num(Num::Any, d));
                    if self.rng.chance(50) {
                        binary(op, lhs, rhs)
                    } else {
                        binary(op, rhs, lhs)
                    }
                }
                3..=4 => binary(BinOp::Div, self.num(Num::Any, d), self.nonzero(d)),
                5 => self.power(d),
                6..=8 => self.transcendental(d),
                9 => {
                    let n = 2 + self.rng.below(2);
                    Expr::Native(Native::Dot, vec![self.vector(n, d), self.vector(n, d)])
                }
                10 => {
                    let n = 2 + self.rng.below(3);
                    Expr::Native(Native::Norm, vec![self.vector(n, d)])
                }
                11 => Expr::Neg(Box::new(self.num(Num::Float, d))),
                _ => self
                    .call_returning(&FLOAT, d)
                    .unwrap_or_else(|| self.num(Num::Float, d)),
            },
            Num::Any => match self.rng.below(24) {
                0..=4 => {
                    let op = *self
                        .rng
                        .pick(&[BinOp::Add, BinOp::Add, BinOp::Sub, BinOp::Mul]);
                    binary(op, self.num(Num::Any, d), self.num(Num::Any, d))
                }
                5..=6 => self.num(Num::Int, depth),
                7..=8 => self.num(Num::Float, depth),
                9 => {
                    let native =
                        *self
                            .rng
                            .pick(&[Native::Min, Native::Max, Native::Abs, Native::Mod]);
                    match native {
                        Native::Abs => Expr::Native(native, vec![self.num(Num::Any, d)]),
                        Native::Mod => {
                            Expr::Native(native, vec![self.num(Num::Any, d), self.nonzero(d)])
                        }
                        _ => {
                            Expr::Native(native, vec![self.num(Num::Any, d), self.num(Num::Any, d)])
                        }
                    }
                }
                10 => {
                    let low = self.num(Num::Any, d);
                    let x = self.num(Num::Any, d);
                    let high = self.num(Num::Any, d);
                    Expr::Native(Native::Clamp, vec![low, x, high])
                }
                11 if self.kernel_hostile() => self.num(Num::Any, depth),
                11 => {
                    let t = if self.rng.chance(50) {
                        self.literal(Num::Float)
                    } else {
                        self.num(Num::Any, d)
                    };
                    Expr::Native(
                        Native::Lerp,
                        vec![self.num(Num::Any, d), self.num(Num::Any, d), t],
                    )
                }
                12 => Expr::Native(Native::Sum, vec![self.expr(&Ty::Seq, d)]),
                13..=14 => {
                    let n = 2 + self.rng.below(3);
                    let list = self.vector(n, d);
                    let index = self.index(n, d);
                    Expr::Index(Box::new(list), Box::new(index))
                }
                15 => {
                    let columns = 2 + self.rng.below(2);
                    let matrix = self.matrix(2, columns, d);
                    let row = self.index(2, d);
                    let column = self.index(columns, d);
                    Expr::Index(
                        Box::new(Expr::Index(Box::new(matrix), Box::new(row))),
                        Box::new(column),
                    )
                }
                16..=17 => {
                    let op = *self.rng.pick(&[BinOp::And, BinOp::Or]);
                    binary(op, self.num(Num::Any, d), self.num(Num::Any, d))
                }
                18 => Expr::Neg(Box::new(self.num(Num::Any, d))),
                _ => self
                    .call_returning(&ANY, d)
                    .unwrap_or_else(|| self.num(Num::Any, d)),
            },
        }
    }

    /// usually provably nonzero, occasionally anything (the reference then
    /// rejects the rare division by zero)
    fn nonzero(&mut self, depth: u32) -> Expr {
        match self.rng.below(10) {
            0..=3 => {
                let literal = self.literal(Num::Any);
                match literal {
                    Expr::Int(0) => Expr::Int(3),
                    Expr::Float(0.0) => Expr::Float(0.5),
                    other => other,
                }
            }
            4..=6 => binary(
                BinOp::Add,
                Expr::Native(Native::Abs, vec![self.num(Num::Any, depth)]),
                self.literal_positive(),
            ),
            7..=8 => {
                let x = self.num(Num::Any, depth);
                binary(
                    BinOp::Add,
                    binary(BinOp::Mul, x.clone(), x),
                    self.literal_positive(),
                )
            }
            _ => self.num(Num::Any, depth),
        }
    }

    fn literal_positive(&mut self) -> Expr {
        if self.rng.chance(50) {
            Expr::Int(self.rng.int(1, 5))
        } else {
            Expr::Float(self.rng.int(1, 40) as f64 / 8.0)
        }
    }

    fn power(&mut self, depth: u32) -> Expr {
        match self.rng.below(3) {
            0 => {
                let exponent = Expr::Int(self.rng.int(0, 3));
                binary(BinOp::Pow, self.num(Num::Any, depth), exponent)
            }
            1 => {
                let base = Expr::Native(Native::Abs, vec![self.num(Num::Any, depth)]);
                let exponent = Expr::Float(*self.rng.pick(&[0.5, 1.5, 0.25, 2.0]));
                binary(BinOp::Pow, base, exponent)
            }
            _ => {
                let base = self.literal_positive();
                let exponent = Expr::Native(Native::Sin, vec![self.num(Num::Any, depth)]);
                binary(BinOp::Pow, base, exponent)
            }
        }
    }

    fn transcendental(&mut self, depth: u32) -> Expr {
        let x = self.num(Num::Any, depth);
        match self.rng.below(9) {
            0 => Expr::Native(Native::Sin, vec![x]),
            1 => Expr::Native(Native::Cos, vec![x]),
            2 => Expr::Native(Native::Tan, vec![Expr::Native(Native::Sin, vec![x])]),
            3 => Expr::Native(Native::Exp, vec![Expr::Native(Native::Cos, vec![x])]),
            4 => Expr::Native(
                Native::Ln,
                vec![binary(
                    BinOp::Add,
                    Expr::Native(Native::Abs, vec![x]),
                    self.literal_positive(),
                )],
            ),
            5 => Expr::Native(Native::Sqrt, vec![Expr::Native(Native::Abs, vec![x])]),
            6 => Expr::Native(Native::Arctan2, vec![x, self.num(Num::Any, depth)]),
            7 => Expr::Native(Native::Exp, vec![x]),
            _ => Expr::Native(
                Native::Sin,
                vec![binary(BinOp::Mul, x, self.literal(Num::Float))],
            ),
        }
    }

    /// an int expression within `0..len`
    fn index(&mut self, len: usize, depth: u32) -> Expr {
        let bounded: Vec<Rc<str>> = self
            .visible()
            .filter(|binding| binding.bound.is_some_and(|hi| hi < len as i64))
            .map(|binding| binding.name.clone())
            .collect();
        match self.rng.below(10) {
            0..=4 => Expr::Int(self.rng.below(len) as i64),
            5..=6 if !bounded.is_empty() => Expr::Name(self.rng.pick(&bounded).clone()),
            _ => Expr::Native(
                Native::Mod,
                vec![self.num(Num::Int, depth), Expr::Int(len as i64)],
            ),
        }
    }

    fn index_into(&mut self, name: &Rc<str>, ty: &Ty) -> Expr {
        match ty {
            Ty::Vec(n) => self.index(*n, 1),
            _ => match self.rng.below(3) {
                0 => Expr::Int(0),
                _ => Expr::Native(
                    Native::Mod,
                    vec![
                        self.num(Num::Int, 1),
                        Expr::Native(Native::Len, vec![Expr::Name(name.clone())]),
                    ],
                ),
            },
        }
    }

    fn vector(&mut self, n: usize, depth: u32) -> Expr {
        let names = self.candidates(&Ty::Vec(n));
        if depth == 0 || self.rng.chance(25) {
            if !names.is_empty() && self.rng.chance(55) {
                return Expr::Name(self.rng.pick(&names).clone());
            }
            return Expr::List((0..n).map(|_| self.num(Num::Any, depth.min(1))).collect());
        }
        let d = depth - 1;
        match self.rng.below(12) {
            0..=2 => Expr::List((0..n).map(|_| self.num(Num::Any, d)).collect()),
            3..=4 => {
                let op = *self.rng.pick(&[BinOp::Add, BinOp::Sub]);
                binary(op, self.vector(n, d), self.vector(n, d))
            }
            5..=6 => {
                let (list, scalar) = (self.vector(n, d), self.num(Num::Any, d));
                if self.rng.chance(50) {
                    binary(BinOp::Mul, list, scalar)
                } else {
                    binary(BinOp::Mul, scalar, list)
                }
            }
            7 => binary(BinOp::Div, self.vector(n, d), self.nonzero(d)),
            8 => Expr::Neg(Box::new(self.vector(n, d))),
            9 => {
                let matrix = self.matrix(2, n, d);
                let row = self.index(2, d);
                Expr::Index(Box::new(matrix), Box::new(row))
            }
            _ => self
                .call_returning(&Ty::Vec(n), d)
                .unwrap_or_else(|| self.vector(n, d)),
        }
    }

    fn seq(&mut self, depth: u32) -> Expr {
        let names = self.candidates(&Ty::Seq);
        match self.rng.below(10) {
            0..=4 if !names.is_empty() => Expr::Name(self.rng.pick(&names).clone()),
            0..=5 if !self.kernel_hostile() => {
                let start = self.rng.int(-2, 3);
                let stop = start + self.rng.int(1, 6);
                Expr::Native(Native::Range, vec![Expr::Int(start), Expr::Int(stop)])
            }
            0..=7 => {
                let n = 1 + self.rng.below(5);
                Expr::List((0..n).map(|_| self.num(Num::Any, depth.min(1))).collect())
            }
            _ => {
                let n = 2 + self.rng.below(3);
                self.vector(n, depth)
            }
        }
    }

    fn matrix(&mut self, rows: usize, columns: usize, depth: u32) -> Expr {
        let ty = Ty::Mat(rows, columns);
        let names = self.candidates(&ty);
        if depth == 0 || self.rng.chance(30) {
            if !names.is_empty() && self.rng.chance(60) {
                return Expr::Name(self.rng.pick(&names).clone());
            }
            return Expr::List(
                (0..rows)
                    .map(|_| self.vector(columns, depth.min(1)))
                    .collect(),
            );
        }
        let d = depth - 1;
        match self.rng.below(6) {
            0..=1 => Expr::List((0..rows).map(|_| self.vector(columns, d)).collect()),
            2 => {
                let op = *self.rng.pick(&[BinOp::Add, BinOp::Sub]);
                binary(
                    op,
                    self.matrix(rows, columns, d),
                    self.matrix(rows, columns, d),
                )
            }
            3 => {
                let (matrix, scalar) = (self.matrix(rows, columns, d), self.num(Num::Any, d));
                if self.rng.chance(50) {
                    binary(BinOp::Mul, matrix, scalar)
                } else {
                    binary(BinOp::Mul, scalar, matrix)
                }
            }
            4 => binary(BinOp::Div, self.matrix(rows, columns, d), self.nonzero(d)),
            _ => Expr::Neg(Box::new(self.matrix(rows, columns, d))),
        }
    }
}

impl Sig {
    fn with_all_required(mut self) -> Self {
        self.required = self.params.len();
        self
    }
}

fn prefix(ty: &Ty) -> &'static str {
    match ty {
        Ty::Num(_) => "n",
        Ty::Vec(_) | Ty::Seq => "v",
        Ty::Mat(..) => "m",
        Ty::Func(_) | Ty::Opaque => "f",
    }
}

/// whether a binding of type `have` can be used where `want` is expected
fn fits(have: &Ty, want: &Ty) -> bool {
    match (have, want) {
        (Ty::Num(_), Ty::Num(Num::Any)) => true,
        (Ty::Vec(_), Ty::Seq) => true,
        (Ty::Func(have), Ty::Func(want)) => have.recursive.is_none() && have == want,
        (have, want) => have == want,
    }
}

fn binary(op: BinOp, lhs: Expr, rhs: Expr) -> Expr {
    Expr::Binary(op, Box::new(lhs), Box::new(rhs))
}

fn names_in(expr: &Expr, names: &mut Vec<Rc<str>>) {
    match expr {
        Expr::Name(name) => names.push(name.clone()),
        Expr::Int(_) | Expr::Float(_) | Expr::Lambda(_) => {}
        Expr::List(items) | Expr::Native(_, items) => {
            items.iter().for_each(|item| names_in(item, names))
        }
        Expr::Neg(operand) | Expr::Not(operand) => names_in(operand, names),
        Expr::Binary(_, lhs, rhs) | Expr::Index(lhs, rhs) => {
            names_in(lhs, names);
            names_in(rhs, names);
        }
        Expr::Call(callee, args) => {
            names_in(callee, names);
            args.iter().for_each(|arg| names_in(arg, names));
        }
    }
}

/// the batch constructors a sampler is written for
#[derive(Clone, Copy, Debug)]
enum Sampling {
    Explicit,
    Explicit2d,
    SurfaceColor,
    Shader,
    PointMap,
    ColorMap,
}

impl Sampling {
    const ALL: [Self; 6] = [
        Self::Explicit,
        Self::Explicit2d,
        Self::SurfaceColor,
        Self::Shader,
        Self::PointMap,
        Self::ColorMap,
    ];

    /// the sampled lambda's parameter and return types
    fn shape(self) -> (Vec<Ty>, Ty) {
        match self {
            Self::Explicit => (vec![FLOAT], ANY),
            Self::Explicit2d => (vec![FLOAT; 2], ANY),
            Self::SurfaceColor => (vec![FLOAT; 3], Ty::Vec(4)),
            Self::Shader => (vec![FLOAT; 2], Ty::Vec(4)),
            Self::PointMap => (vec![Ty::Vec(3)], Ty::Vec(3)),
            // colours are mapped from positions
            Self::ColorMap => (vec![Ty::Vec(3)], Ty::Vec(4)),
        }
    }

    /// dyadic domains, so every sample point is an exact float the probes
    /// can name
    fn domain(self, rng: &mut Rng) -> Domain {
        match self {
            Self::Explicit => {
                let (lo, hi) = *rng.pick(&[(-2, 2), (-1, 1), (0, 4), (-3, 1)]);
                Domain::Line {
                    lo,
                    hi,
                    samples: *rng.pick(&[5, 9, 17, 33, 65]),
                }
            }
            Self::Explicit2d | Self::SurfaceColor => Domain::Grid {
                samples: [*rng.pick(&[3, 5, 9]), *rng.pick(&[3, 5, 9])],
                colored: matches!(self, Self::SurfaceColor),
            },
            Self::Shader => Domain::Pixels {
                wide: rng.chance(50),
                resolution: *rng.pick(&[2, 4, 8, 16]),
            },
            Self::PointMap => Domain::Points {
                size: *rng.pick(&[[2, 1], [1, 1], [4, 2]]),
                grid: rng.chance(30),
            },
            Self::ColorMap => Domain::Colors {
                grid: rng.chance(30),
            },
        }
    }
}

/// where a constructor samples its lambda
enum Domain {
    Line {
        lo: i64,
        hi: i64,
        samples: i64,
    },
    /// `ExplicitFunc2d` over `[-1, 1]` squared; `colored` samples the colour
    /// callback, with a fixed surface
    Grid {
        samples: [i64; 2],
        colored: bool,
    },
    Pixels {
        wide: bool,
        resolution: i64,
    },
    Points {
        size: [i64; 2],
        grid: bool,
    },
    Colors {
        grid: bool,
    },
}

fn float_list(values: &[f64]) -> Expr {
    Expr::List(values.iter().copied().map(Expr::Float).collect())
}

impl Domain {
    /// arguments the constructor passes on some call
    fn probe(&self, rng: &mut Rng) -> Vec<Expr> {
        let along = |rng: &mut Rng, lo: f64, hi: f64, samples: i64| {
            lo + (hi - lo) * rng.int(0, samples - 1) as f64 / (samples - 1) as f64
        };
        match *self {
            Self::Line { lo, hi, samples } => {
                vec![Expr::Float(along(rng, lo as f64, hi as f64, samples))]
            }
            Self::Grid { samples, colored } => {
                let x = along(rng, -1.0, 1.0, samples[0]);
                let y = along(rng, -1.0, 1.0, samples[1]);
                let mut args = vec![Expr::Float(x), Expr::Float(y)];
                if colored {
                    args.push(Expr::Float(0.5 * (x * x + y * y)));
                }
                args
            }
            Self::Pixels { wide, resolution } => {
                let (columns, rows) = if wide {
                    (resolution, (resolution / 2).max(1))
                } else {
                    (resolution, resolution)
                };
                let width = if wide { 4.0 } else { 2.0 };
                let column = rng.int(0, columns - 1) as f64;
                let row = rng.int(0, rows - 1) as f64;
                vec![
                    Expr::Float(-width / 2.0 + width * (column + 0.5) / columns as f64),
                    Expr::Float(1.0 - 2.0 * (row + 0.5) / rows as f64),
                ]
            }
            Self::Points { size, .. } => corner(rng, size),
            Self::Colors { .. } => corner(rng, [2, 1]),
        }
    }

    fn constructor(&self, callee: &str, rng: &mut Rng) -> String {
        match *self {
            Self::Line { lo, hi, samples } => match rng.below(6) {
                0 => format!("ExplicitFunc({callee}, [{lo}, {hi}, {samples}], 1)"),
                1 => format!(
                    "ExplicitFunc({callee}, [{lo}.0, {hi}.0, {samples}], 0, [0.2, 0.6, 0.9, 0.4])"
                ),
                _ => format!("ExplicitFunc({callee}, [{lo}, {hi}, {samples}])"),
            },
            Self::Grid {
                samples: [nx, ny],
                colored: false,
            } => format!("ExplicitFunc2d({callee}, [-1, 1, {nx}], [-1, 1, {ny}])"),
            Self::Grid {
                samples: [nx, ny],
                colored: true,
            } => format!(
                "ExplicitFunc2d(|x, y| 0.5 * (x * x + y * y), [-1, 1, {nx}], [-1, 1, {ny}], {callee})"
            ),
            Self::Pixels { wide, resolution } => {
                let x = if wide { "[-2, 2]" } else { "[-1, 1]" };
                format!("Shader({callee}, {x}, [-1, 1], {resolution})")
            }
            Self::Points { size: [w, h], grid } => {
                format!("point_map{{{callee}}} {}", target_mesh(grid, w, h))
            }
            Self::Colors { grid } => format!("color_map{{{callee}}} {}", target_mesh(grid, 2, 1)),
        }
    }
}

/// a corner of the `Rect` of `size`, one of the points a map samples
fn corner(rng: &mut Rng, size: [i64; 2]) -> Vec<Expr> {
    let x = size[0] as f64 / 2.0 * if rng.chance(50) { 1.0 } else { -1.0 };
    let y = size[1] as f64 / 2.0 * if rng.chance(50) { 1.0 } else { -1.0 };
    vec![float_list(&[x, y, 0.0])]
}

fn target_mesh(grid: bool, w: i64, h: i64) -> String {
    if grid {
        "LineGrid([-1, 1, 5], [-1, 1, 3], 1)".into()
    } else {
        format!("Rect([{w}, {h}])")
    }
}
