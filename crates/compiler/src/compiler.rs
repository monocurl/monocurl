use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use bytecode::{
    AnimPrototype, Bytecode, Instruction, InstructionAnnotation, LambdaPrototype, SectionBytecode,
    SectionFlags,
};
use parser::ast::{
    Anim, BinaryOperator, BinaryOperatorType, Block, Declaration, DirectionalLiteral, Expression,
    For, IdentifierDeclaration, IdentifierReference, If, LambdaBody, LambdaDefinition,
    LambdaInvocation, Literal, NativeInvocation, OperatorDefinition, OperatorInvocation, Play,
    Property, Return, Section, SectionBundle, SectionType, SpanTagged, Statement, Subscript,
    UnaryOperatorType, UnaryPreOperator, VariableType as AstVariableType, While,
};
use structs::text::Span8;


pub struct CompileError {
    pub span: Span8,
    pub message: String,
}

pub struct CompileResult {
    pub bytecode: Bytecode,
    pub errors: Vec<CompileError>,
}


#[derive(Clone, Debug, PartialEq, Eq)]
enum VariableType {
    Let,
    Var,
    Reference,
    State,
    Param,
    Mesh,
}

#[derive(Clone, Debug)]
struct Symbol {
    name: String,
    stack_position: usize,
    var_type: VariableType,
    captured: bool,
}

#[derive(Clone)]
struct Scope {
    base_stack_depth: usize,
    symbols: HashMap<String, Symbol>,
}

struct LoopContext {
    // Some(ip) for while loops (known upfront); None for for loops (patched later)
    continue_target: Option<u32>,
    stack_depth_at_loop: usize,
    break_patches: Vec<usize>,
    continue_patches: Vec<usize>,
}

struct CompilerFrame {
    scopes: Vec<Scope>,
    // reference point for how many variables would be on the stack upon executing the current instruction
    stack_depth: usize,
    loop_contexts: Vec<LoopContext>,
}

struct FreeVarCollector {
    defined: HashSet<String>,
    free: Vec<String>,
    seen_free: HashSet<String>,
}

impl FreeVarCollector {
    fn new(predefined: HashSet<String>) -> Self {
        Self { defined: predefined, free: Vec::new(), seen_free: HashSet::new() }
    }

    fn define(&mut self, name: &str) {
        self.defined.insert(name.to_string());
    }

    fn reference(&mut self, name: &str) {
        if !self.defined.contains(name) && self.seen_free.insert(name.to_string()) {
            self.free.push(name.to_string());
        }
    }

    fn into_free(self) -> Vec<String> {
        self.free
    }

    fn visit_stmts(&mut self, stmts: &[SpanTagged<Statement>]) {
        for (_, s) in stmts {
            self.visit_stmt(s);
        }
    }

    fn visit_stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Expression(e) => self.visit_expr(e),
            Statement::Declaration(d) => {
                self.visit_expr(&d.value.1);
                self.define(&d.identifier.1.0);
            }
            Statement::Return(r) => self.visit_expr(&r.value.1),
            Statement::While(w) => {
                self.visit_expr(&w.condition.1);
                self.visit_stmts(&w.body.1);
            }
            Statement::For(f) => {
                self.visit_expr(&f.container.1);
                self.define(&f.var_name.1.0);
                self.visit_stmts(&f.body.1);
            }
            Statement::If(i) => {
                self.visit_expr(&i.condition.1);
                self.visit_stmts(&i.if_block.1);
                if let Some(ref e) = i.else_block {
                    self.visit_stmts(&e.1);
                }
            }
            Statement::Play(p) => self.visit_expr(&p.animations.1),
            Statement::Break | Statement::Continue => {}
        }
    }

    fn visit_expr(&mut self, expr: &Expression) {
        match expr {
            Expression::IdentifierReference(ir) => self.reference(ident_ref_name(ir)),
            Expression::BinaryOperator(b) => {
                self.visit_expr(&b.lhs.1);
                self.visit_expr(&b.rhs.1);
            }
            Expression::UnaryPreOperator(u) => self.visit_expr(&u.operand.1),
            Expression::Literal(l) => match l {
                Literal::Vector(v) => {
                    for e in v { self.visit_expr(&e.1); }
                }
                Literal::Map(m) => {
                    for (k, v) in m { self.visit_expr(&k.1); self.visit_expr(&v.1); }
                }
                _ => {}
            },
            Expression::Subscript(s) => {
                self.visit_expr(&s.base.1);
                self.visit_expr(&s.index.1);
            }
            Expression::Property(p) => self.visit_expr(&p.base.1),
            Expression::LambdaInvocation(l) => {
                self.visit_expr(&l.lambda.1);
                for (_, a) in &l.arguments.1 { self.visit_expr(&a.1); }
            }
            Expression::OperatorInvocation(o) => {
                self.visit_expr(&o.operator.1);
                for (_, a) in &o.arguments.1 { self.visit_expr(&a.1); }
                self.visit_expr(&o.operand.1);
            }
            Expression::NativeInvocation(n) => {
                for a in &n.arguments { self.visit_expr(&a.1); }
            }
            Expression::LambdaDefinition(l) => {
                // default values evaluated in outer scope
                for arg in &l.args {
                    if let Some(ref d) = arg.default_value { self.visit_expr(&d.1); }
                }
                let mut inner_pre: HashSet<String> =
                    l.args.iter().map(|a| a.identifier.1.0.clone()).collect();
                if matches!(l.body.1, LambdaBody::Block(_)) {
                    inner_pre.insert("_".to_string());
                }
                let mut inner = FreeVarCollector::new(inner_pre);
                match &l.body.1 {
                    LambdaBody::Inline(e) => inner.visit_expr(e),
                    LambdaBody::Block(s) => inner.visit_stmts(s),
                }
                for name in inner.into_free() { self.reference(&name); }
            }
            Expression::OperationDefinition(o) => self.visit_expr(&o.lambda.1),
            Expression::Block(b) => {
                let mut inner = FreeVarCollector::new(HashSet::from(["_".to_string()]));
                inner.visit_stmts(&b.body);
                for name in inner.into_free() { self.reference(&name); }
            }
            Expression::Anim(a) => {
                let mut inner = FreeVarCollector::new(HashSet::new());
                inner.visit_stmts(&a.body);
                for name in inner.into_free() { self.reference(&name); }
            }
        }
    }
}

fn ident_ref_name(ir: &IdentifierReference) -> &str {
    match ir {
        IdentifierReference::Value(n)
        | IdentifierReference::Stateful(n)
        | IdentifierReference::Dereference(n) => n,
    }
}

// returns true if evaluates to a stateful expression
fn is_stateful(expr: &Expression) -> bool {
    match expr {
        Expression::IdentifierReference(IdentifierReference::Stateful(_)) => true,
        Expression::IdentifierReference(_) => false,
        Expression::Literal(_) => false,
        Expression::BinaryOperator(b) => is_stateful(&b.lhs.1) || is_stateful(&b.rhs.1),
        Expression::UnaryPreOperator(u) => is_stateful(&u.operand.1),
        Expression::Subscript(s) => is_stateful(&s.base.1) || is_stateful(&s.index.1),
        Expression::Property(p) => is_stateful(&p.base.1),
        Expression::LambdaInvocation(l) => {
            is_stateful(&l.lambda.1)
                || l.arguments.1.iter().any(|(_, a)| is_stateful(&a.1))
        }
        Expression::OperatorInvocation(o) => {
            is_stateful(&o.operator.1)
                || o.arguments.1.iter().any(|(_, a)| is_stateful(&a.1))
                || is_stateful(&o.operand.1)
        }
        Expression::NativeInvocation(n) => n.arguments.iter().any(|a| is_stateful(&a.1)),
        // lambdas/blocks/anims close over their environment at creation; stateful-ness
        // doesn't propagate through them at the call site
        Expression::LambdaDefinition(_)
        | Expression::OperationDefinition(_)
        | Expression::Block(_)
        | Expression::Anim(_) => false,
    }
}

fn free_vars_stmts(stmts: &[SpanTagged<Statement>], pre: HashSet<String>) -> Vec<String> {
    let mut c = FreeVarCollector::new(pre);
    c.visit_stmts(stmts);
    c.into_free()
}

fn free_vars_expr(expr: &Expression, pre: HashSet<String>) -> Vec<String> {
    let mut c = FreeVarCollector::new(pre);
    c.visit_expr(expr);
    c.into_free()
}

struct Compiler {
    frames: Vec<CompilerFrame>,
    sections: Vec<SectionBytecode>,
    current_section: SectionBytecode,
    errors: Vec<CompileError>,
    bundle_exports: Vec<HashMap<String, Symbol>>,
    bundle_stack_end: Vec<usize>,
}

fn default_section() -> SectionBytecode {
    SectionBytecode::new(SectionFlags { is_stdlib: false, is_library: false })
}

impl Compiler {
    fn new(bundle_count: usize) -> Self {
        Self {
            frames: vec![CompilerFrame {
                scopes: vec![Scope { base_stack_depth: 0, symbols: HashMap::new() }],
                stack_depth: 0,
                loop_contexts: Vec::new(),
            }],
            sections: Vec::new(),
            current_section: default_section(),
            errors: Vec::new(),
            bundle_exports: vec![HashMap::new(); bundle_count],
            bundle_stack_end: vec![0; bundle_count],
        }
    }

    fn finish(self) -> CompileResult {
        CompileResult { bytecode: Bytecode::new(self.sections), errors: self.errors }
    }
}

pub fn compile(bundles: &[Rc<SectionBundle>]) -> CompileResult {
    let mut c = Compiler::new(bundles.len());
    for bundle in bundles {
        c.compile_bundle(bundle);
    }
    c.finish()
}

impl Compiler {
    fn compile_bundle(&mut self, bundle: &SectionBundle) {
        // imported symbols are treated as `let` — cross-bundle mutation is not allowed
        let mut base_symbols: HashMap<String, Symbol> = HashMap::new();
        let base_depth = bundle
            .imported_files
            .iter()
            .map(|p| {
                for sym in self.bundle_exports[*p].values() {
                    base_symbols.insert(sym.name.clone(), Symbol {
                        // make it constant for future symbols
                        var_type: VariableType::Let,
                        ..sym.clone()
                    });
                }
                self.bundle_stack_end[*p]
            })
            .max()
            .unwrap_or(0);

        self.frame_mut().scopes = vec![Scope { base_stack_depth: 0, symbols: base_symbols }];
        self.frame_mut().stack_depth = base_depth;

        for section in &bundle.sections {
            self.compile_section(section);
        }

        self.bundle_exports[bundle.file_index] = self
            .frame()
            .scopes
            .iter()
            .flat_map(|s| s.symbols.iter().map(|(k, v)| (k.clone(), v.clone())))
            .collect();
        self.bundle_stack_end[bundle.file_index] = self.stack_depth();
    }

    fn compile_section(&mut self, section: &Section) {
        if self.sections.len() >= u16::MAX as usize {
            self.error(0..0, "too many sections (limit 65535)");
            return;
        }
        self.current_section = SectionBytecode::new(SectionFlags {
            is_stdlib: section.section_type == SectionType::StandardLibrary,
            is_library: matches!(
                section.section_type,
                SectionType::UserLibrary
            ),
        });
        // symbols declared here land in the current top scope (no push/pop)
        self.compile_statements(&section.body);
        self.emit(Instruction::EndOfExecutionHead, 0..0);
        let finished = std::mem::replace(&mut self.current_section, default_section());
        self.sections.push(finished);
    }
}

impl Compiler {
    fn emit(&mut self, instruction: Instruction, src: Span8) {
        if self.current_section.instructions.len() >= u32::MAX as usize {
            self.error(src.clone(), "too many instructions in section");
        }
        self.current_section.instructions.push(instruction);
        self.current_section
            .annotations
            .push(InstructionAnnotation { source_loc: src });
    }

    fn emit_push(&mut self, instruction: Instruction, src: Span8) {
        self.emit(instruction, src);
        self.inc_stack();
    }

    fn instruction_pointer(&self) -> u32 {
        self.current_section.instructions.len() as u32
    }

    fn section_index(&self) -> u16 {
        self.sections.len() as u16
    }

    fn patch_jump(&mut self, instr_idx: usize, target: u32) {
        match &mut self.current_section.instructions[instr_idx] {
            Instruction::Jump { to, .. } | Instruction::ConditionalJump { to, .. } => *to = target,
            _ => panic!("patch_jump on non-jump instruction"),
        }
    }

    fn frame(&self) -> &CompilerFrame {
        self.frames.last().unwrap()
    }

    fn frame_mut(&mut self) -> &mut CompilerFrame {
        self.frames.last_mut().unwrap()
    }

    fn stack_depth(&self) -> usize {
        self.frame().stack_depth
    }

    fn inc_stack(&mut self) {
        self.frame_mut().stack_depth += 1;
    }

    fn dec_stack(&mut self, n: usize) {
        self.frame_mut().stack_depth -= n;
    }

    fn stack_delta(&self, position: usize) -> i32 {
        position as i32 - self.stack_depth() as i32
    }

    fn emit_pops(&mut self, count: usize, span: Span8) {
        if count == 0 {
            return;
        }
        self.emit(Instruction::Pop { count: count as u32 }, span);
        self.dec_stack(count);
    }

    fn push_scope(&mut self) {
        let base = self.stack_depth();
        self.frame_mut().scopes.push(Scope { base_stack_depth: base, symbols: HashMap::new() });
    }

    fn pop_scope(&mut self, span: Span8) {
        let scope = self.frame_mut().scopes.pop().unwrap();
        let to_pop = self.stack_depth() - scope.base_stack_depth;
        self.emit_pops(to_pop, span);
    }

    fn define_symbol(&mut self, name: &str, var_type: VariableType) {
        let position = self.stack_depth() - 1;
        self.frame_mut().scopes.last_mut().unwrap().symbols.insert(
            name.to_string(),
            Symbol { name: name.to_string(), stack_position: position, var_type, captured: false },
        );
    }

    fn register_symbol(&mut self, name: &str, var_type: VariableType, position: usize, captured: bool) {
        self.frame_mut().scopes.last_mut().unwrap().symbols.insert(
            name.to_string(),
            Symbol { name: name.to_string(), stack_position: position, var_type, captured },
        );
    }

    fn lookup(&self, name: &str) -> Option<&Symbol> {
        for scope in self.frame().scopes.iter().rev() {
            if let Some(sym) = scope.symbols.get(name) {
                return Some(sym);
            }
        }
        None
    }

    fn push_frame(&mut self) {
        self.frames.push(CompilerFrame {
            scopes: vec![Scope { base_stack_depth: 0, symbols: HashMap::new() }],
            stack_depth: 0,
            loop_contexts: Vec::new(),
        });
    }

    fn pop_frame(&mut self) {
        self.frames.pop();
    }

    // -- constant pool helpers --

    fn intern_int(&mut self, val: i64, span: &Span8) -> u16 {
        if let Some(idx) = self.current_section.int_pool.iter().position(|&x| x == val) {
            return idx as u16;
        }
        let idx = self.current_section.int_pool.len();
        if idx >= u16::MAX as usize {
            self.error(span.clone(), "int pool overflow");
        }
        self.current_section.int_pool.push(val);
        idx as u16
    }

    fn intern_float(&mut self, val: f64, span: &Span8) -> u16 {
        if let Some(idx) = self
            .current_section
            .float_pool
            .iter()
            .position(|x| x.to_bits() == val.to_bits())
        {
            return idx as u16;
        }
        let idx = self.current_section.float_pool.len();
        if idx >= u16::MAX as usize {
            self.error(span.clone(), "float pool overflow");
        }
        self.current_section.float_pool.push(val);
        idx as u16
    }

    fn intern_string(&mut self, val: &str, span: &Span8) -> u16 {
        if let Some(idx) = self.current_section.string_pool.iter().position(|x| x == val) {
            return idx as u16;
        }
        let idx = self.current_section.string_pool.len();
        if idx >= u16::MAX as usize {
            self.error(span.clone(), "string pool overflow");
        }
        self.current_section.string_pool.push(val.to_string());
        idx as u16
    }

    fn error(&mut self, span: Span8, msg: impl Into<String>) {
        self.errors.push(CompileError { span, message: msg.into() });
    }
}

impl Compiler {
    fn compile_statements(&mut self, stmts: &[SpanTagged<Statement>]) {
        for (span, stmt) in stmts {
            self.compile_statement(stmt, span);
        }
    }

    fn compile_statement(&mut self, stmt: &Statement, span: &Span8) {
        match stmt {
            Statement::Break => self.compile_break(span),
            Statement::Continue => self.compile_continue(span),
            Statement::Return(r) => self.compile_return(r, span),
            Statement::While(w) => self.compile_while(w, span),
            Statement::For(f) => self.compile_for(f, span),
            Statement::If(i) => self.compile_if(i, span),
            Statement::Declaration(d) => self.compile_declaration(d, span),
            Statement::Expression(e) => {
                self.compile_expr(false, e, span);
                self.emit_pops(1, span.clone());
            }
            Statement::Play(p) => self.compile_play(p, span),
        }
    }

    fn compile_declaration(&mut self, d: &Declaration, span: &Span8) {
        self.compile_expr(false, &d.value.1, &d.value.0);
        let vt = match d.var_type {
            AstVariableType::Let => VariableType::Let,
            AstVariableType::Var => VariableType::Var,
            AstVariableType::Mesh => VariableType::Mesh,
            AstVariableType::State => VariableType::State,
            AstVariableType::Param => VariableType::Param,
        };
        match vt {
            VariableType::Mesh => {
                let ni = self.intern_string(&d.identifier.1.0, span);
                self.emit(Instruction::PushMesh { name_index: ni }, span.clone());
            }
            VariableType::State => {
                let ni = self.intern_string(&d.identifier.1.0, span);
                self.emit(Instruction::PushState { name_index: ni }, span.clone());
            }
            VariableType::Param => {
                let ni = self.intern_string(&d.identifier.1.0, span);
                self.emit(Instruction::PushParam { name_index: ni }, span.clone());
            }
            _ => {}
        }
        self.define_symbol(&d.identifier.1.0, vt);
    }

    fn compile_while(&mut self, w: &While, span: &Span8) {
        let loop_start = self.instruction_pointer();
        self.compile_expr(false, &w.condition.1, &w.condition.0);
        self.emit(Instruction::Not, w.condition.0.clone());
        let exit_jump = self.instruction_pointer();
        // to be patched
        self.emit(
            Instruction::ConditionalJump { section: self.section_index(), to: 0 },
            w.condition.0.clone(),
        );
        self.dec_stack(1);

        let loop_stack = self.stack_depth();
        self.frame_mut().loop_contexts.push(LoopContext {
            continue_target: Some(loop_start),
            stack_depth_at_loop: loop_stack,
            break_patches: Vec::new(),
            continue_patches: Vec::new(),
        });

        self.push_scope();
        self.compile_statements(&w.body.1);
        self.pop_scope(span.clone());

        self.emit(
            Instruction::Jump { section: self.section_index(), to: loop_start },
            span.clone(),
        );

        let loop_end = self.instruction_pointer();
        self.patch_jump(exit_jump as usize, loop_end);
        let ctx = self.frame_mut().loop_contexts.pop().unwrap();
        for patch in ctx.break_patches {
            self.patch_jump(patch, loop_end);
        }
    }

    fn compile_for(&mut self, f: &For, span: &Span8) {
        // desugars for v in container  ->  while idx < len(container)
        self.push_scope();

        self.compile_expr(false, &f.container.1, &f.container.0);
        let iter_pos = self.stack_depth() - 1;
        // anonymous names (null byte) can't collide with user identifiers
        self.define_symbol("\x00iter", VariableType::Let);

        let zero = self.intern_int(0, span);
        self.emit_push(Instruction::PushInt { index: zero }, span.clone());
        let idx_pos = self.stack_depth() - 1;
        self.define_symbol("\x00idx", VariableType::Var);

        let condition_ip = self.instruction_pointer();
        let loop_stack = self.stack_depth();

        // condition: idx < len(iter)
        let d = self.stack_delta(idx_pos);
        self.emit_push(Instruction::PushCopy { stack_delta: d }, span.clone());
        let d = self.stack_delta(iter_pos);
        self.emit_push(Instruction::PushCopy { stack_delta: d }, span.clone());

        // TODO native function lookup
        let len_idx = self.intern_string("len", span) as u32;
        // NativeInvoke pops 1 arg, pushes 1 result — emit_push already counted the slot
        self.emit(Instruction::NativeInvoke { index: len_idx }, span.clone());

        self.emit(Instruction::Lt, span.clone());
        self.dec_stack(1);

        self.emit(Instruction::Not, span.clone());
        let exit_jump = self.instruction_pointer();
        self.emit(
            Instruction::ConditionalJump { section: self.section_index(), to: 0 },
            span.clone(),
        );
        self.dec_stack(1); // depth = loop_stack

        self.frame_mut().loop_contexts.push(LoopContext {
            continue_target: None, // patched below after increment is emitted
            stack_depth_at_loop: loop_stack,
            break_patches: Vec::new(),
            continue_patches: Vec::new(),
        });

        // body scope with the for variable
        self.push_scope();
        let d = self.stack_delta(iter_pos);
        self.emit_push(Instruction::PushCopy { stack_delta: d }, span.clone());
        let d = self.stack_delta(idx_pos);
        self.emit_push(Instruction::PushCopy { stack_delta: d }, span.clone());
        self.emit(Instruction::Subscript { mutable: false }, span.clone());
        self.dec_stack(1);
        self.define_symbol(&f.var_name.1.0, VariableType::Let);

        self.compile_statements(&f.body.1);
        self.pop_scope(span.clone()); // depth = loop_stack

        // increment point — patch all pending continues here
        let increment_ip = self.instruction_pointer();
        let cont_patches =
            std::mem::take(&mut self.frame_mut().loop_contexts.last_mut().unwrap().continue_patches);
        for patch in cont_patches {
            self.patch_jump(patch, increment_ip);
        }

        // idx = idx + 1
        let d = self.stack_delta(idx_pos);
        self.emit_push(Instruction::PushLvalue { stack_delta: d }, span.clone());
        let d = self.stack_delta(idx_pos);
        self.emit_push(Instruction::PushCopy { stack_delta: d }, span.clone());
        let one = self.intern_int(1, span);
        self.emit_push(Instruction::PushInt { index: one }, span.clone());
        self.emit(Instruction::Add, span.clone());
        self.dec_stack(1);
        self.emit(Instruction::Assign, span.clone());
        self.dec_stack(1);
        self.emit_pops(1, span.clone()); // discard assign result, depth = loop_stack

        self.emit(
            Instruction::Jump { section: self.section_index(), to: condition_ip },
            span.clone(),
        );

        let loop_end = self.instruction_pointer();
        self.patch_jump(exit_jump as usize, loop_end);
        let ctx = self.frame_mut().loop_contexts.pop().unwrap();
        for patch in ctx.break_patches {
            self.patch_jump(patch, loop_end);
        }

        self.pop_scope(span.clone()); // removes iter and idx
    }

    fn compile_if(&mut self, i: &If, span: &Span8) {
        self.compile_expr(false, &i.condition.1, &i.condition.0);
        self.emit(Instruction::Not, i.condition.0.clone());
        let skip_if = self.instruction_pointer();
        self.emit(
            Instruction::ConditionalJump { section: self.section_index(), to: 0 },
            i.condition.0.clone(),
        );
        self.dec_stack(1);

        self.push_scope();
        self.compile_statements(&i.if_block.1);
        self.pop_scope(span.clone());

        if let Some(ref else_block) = i.else_block {
            let skip_else = self.instruction_pointer();
            self.emit(
                Instruction::Jump { section: self.section_index(), to: 0 },
                span.clone(),
            );
            self.patch_jump(skip_if as usize, self.instruction_pointer());
            self.push_scope();
            self.compile_statements(&else_block.1);
            self.pop_scope(span.clone());
            self.patch_jump(skip_else as usize, self.instruction_pointer());
        } else {
            self.patch_jump(skip_if as usize, self.instruction_pointer());
        }
    }

    fn compile_return(&mut self, r: &Return, span: &Span8) {
        self.compile_expr(false, &r.value.1, &r.value.0);
        let below = self.stack_depth() as i32 - 1;
        self.emit(Instruction::Return { stack_delta: -below }, span.clone());
    }

    fn compile_break(&mut self, span: &Span8) {
        let Some(ctx) = self.frame().loop_contexts.last() else {
            self.error(span.clone(), "break outside loop");
            return;
        };
        let pop_count = self.stack_depth() - ctx.stack_depth_at_loop;
        self.emit_pops(pop_count, span.clone());
        // undo tracking so sequential code after the jump sees consistent depth
        self.frame_mut().stack_depth += pop_count;

        let patch_idx = self.instruction_pointer() as usize;
        self.emit(
            Instruction::Jump { section: self.section_index(), to: 0 },
            span.clone(),
        );
        self.frame_mut().loop_contexts.last_mut().unwrap().break_patches.push(patch_idx);
    }

    fn compile_continue(&mut self, span: &Span8) {
        let Some(ctx) = self.frame().loop_contexts.last() else {
            self.error(span.clone(), "continue outside loop");
            return;
        };
        let pop_count = self.stack_depth() - ctx.stack_depth_at_loop;
        let target = ctx.continue_target;
        self.emit_pops(pop_count, span.clone());
        self.frame_mut().stack_depth += pop_count;

        let patch_idx = self.instruction_pointer() as usize;
        let to = target.unwrap_or(0);
        self.emit(Instruction::Jump { section: self.section_index(), to }, span.clone());
        if target.is_none() {
            self.frame_mut()
                .loop_contexts
                .last_mut()
                .unwrap()
                .continue_patches
                .push(patch_idx);
        }
    }

    fn compile_play(&mut self, p: &Play, span: &Span8) {
        self.compile_expr(false, &p.animations.1, &p.animations.0);
        self.emit(Instruction::Play, span.clone());
        self.dec_stack(1);
    }
}

impl Compiler {
    fn compile_expr(&mut self, mutable: bool, expr: &Expression, span: &Span8) {
        if mutable
            && !matches!(
                expr,
                Expression::IdentifierReference(_)
                    | Expression::Subscript(_)
                    | Expression::Property(_)
                    | Expression::Literal(Literal::Vector(_))
            )
        {
            self.error(span.clone(), "expression is not assignable");
        }
        match expr {
            Expression::IdentifierReference(i) => self.compile_ident_ref(mutable, i, span),
            Expression::Subscript(s) => self.compile_subscript(mutable, s),
            Expression::Property(p) => self.compile_property(mutable, p),
            Expression::Literal(l) => self.compile_literal(mutable, l, span),
            Expression::BinaryOperator(b) => self.compile_binary(b, span),
            Expression::UnaryPreOperator(u) => self.compile_unary(u),
            Expression::LambdaDefinition(l) => self.compile_lambda(l, span),
            Expression::OperationDefinition(o) => self.compile_operator_def(o, span),
            Expression::Block(b) => self.compile_block(b, span),
            Expression::Anim(a) => self.compile_anim(a, span),
            Expression::LambdaInvocation(l) => self.compile_lambda_invoke(l, span),
            Expression::OperatorInvocation(o) => self.compile_operator_invoke(o, span),
            Expression::NativeInvocation(n) => self.compile_native_invoke(n, span),
        }
    }
}

impl Compiler {
    fn compile_ident_ref(&mut self, mutable: bool, ir: &IdentifierReference, span: &Span8) {
        let name = ident_ref_name(ir);
        let Some(sym) = self.lookup(name).cloned() else {
            self.error(span.clone(), format!("undefined variable '{}'", name));
            let idx = self.intern_int(0, span);
            self.emit_push(Instruction::PushInt { index: idx }, span.clone());
            return;
        };
        let delta = self.stack_delta(sym.stack_position);

        match ir {
            IdentifierReference::Value(_) => {
                if mutable && sym.var_type == VariableType::Let {
                    self.error(span.clone(), format!("cannot mutate let '{}'", name));
                }

                let instr = match sym.var_type {
                    // try to do lvalue even if non mutable, since it might actually be mutable in the case that we're passing to a reference parameter
                    VariableType::Mesh => Instruction::PushMeshLvalue { stack_delta: delta },
                    VariableType::State => Instruction::PushStateLvalue { stack_delta: delta },
                    VariableType::Param => Instruction::PushParamLvalue { stack_delta: delta },
                    VariableType::Var => Instruction::PushLvalue { stack_delta: delta },
                    // these two are explicitly copy only, irrespective of context. Let and mutable will result in an error
                    VariableType::Reference | VariableType::Let => Instruction::PushCopy { stack_delta: delta }
                };
                self.emit_push(instr, span.clone());
            }
            IdentifierReference::Stateful(_) => {
                self.emit_push(
                    Instruction::PushStateful { stack_delta: delta },
                    span.clone(),
                );
            }
            IdentifierReference::Dereference(_) => {
                self.emit_push(
                    Instruction::PushDereference { stack_delta: delta },
                    span.clone(),
                );
            }
        }
    }
}

impl Compiler {
    fn compile_literal(&mut self, mutable: bool, l: &Literal, span: &Span8) {
        match l {
            Literal::Int(val) => {
                let idx = self.intern_int(*val, span);
                self.emit_push(Instruction::PushInt { index: idx }, span.clone());
            }
            Literal::Float(val) => {
                let idx = self.intern_float(*val, span);
                self.emit_push(Instruction::PushFloat { index: idx }, span.clone());
            }
            Literal::Imaginary(val) => {
                let idx = self.intern_float(*val, span);
                self.emit_push(Instruction::PushImaginary { index: idx }, span.clone());
            }
            Literal::String(s) => {
                let idx = self.intern_string(s, span);
                self.emit_push(Instruction::PushString { index: idx }, span.clone());
            }
            Literal::Directional(d) => self.compile_directional(d, span),
            Literal::Vector(elems) => self.compile_vector(mutable, elems, span),
            Literal::Map(entries) => self.compile_map(entries, span),
        }
    }

    fn compile_directional(&mut self, d: &DirectionalLiteral, span: &Span8) {
        let (x, y, z) = match d {
            DirectionalLiteral::Left(m) => (-m, 0.0, 0.0),
            DirectionalLiteral::Right(m) => (*m, 0.0, 0.0),
            DirectionalLiteral::Up(m) => (0.0, *m, 0.0),
            DirectionalLiteral::Down(m) => (0.0, -m, 0.0),
            DirectionalLiteral::Forward(m) => (0.0, 0.0, *m),
            DirectionalLiteral::Backward(m) => (0.0, 0.0, -m),
        };
        self.emit_push(Instruction::PushEmptyVector, span.clone());
        for component in [x, y, z] {
            let idx = self.intern_float(component, span);
            self.emit_push(Instruction::PushFloat { index: idx }, span.clone());
            self.emit(Instruction::Append, span.clone());
            self.dec_stack(1); // append: pop 2 push 1
        }
    }

    fn compile_vector(&mut self, mutable: bool, elems: &[SpanTagged<Expression>], span: &Span8) {
        self.emit_push(Instruction::PushEmptyVector, span.clone());
        for elem in elems {
            self.compile_expr(mutable, &elem.1, &elem.0);
            self.emit(Instruction::Append, span.clone());
            self.dec_stack(1);
        }
    }

    fn compile_map(
        &mut self,
        entries: &[(SpanTagged<Expression>, SpanTagged<Expression>)],
        span: &Span8,
    ) {
        self.emit_push(Instruction::PushEmptyMap, span.clone());
        let map_pos = self.stack_depth() - 1;
        for (key, val) in entries {
            let d = self.stack_delta(map_pos);
            self.emit_push(Instruction::PushLvalue { stack_delta: d }, span.clone());
            self.compile_expr(false, &key.1, &key.0);
            self.emit(Instruction::Subscript { mutable: true }, span.clone());
            self.dec_stack(1);
            self.compile_expr(false, &val.1, &val.0);
            self.emit(Instruction::Assign, span.clone());
            self.dec_stack(1);
            self.emit_pops(1, span.clone()); // discard assign result
        }
    }
}

impl Compiler {
    fn compile_binary(&mut self, b: &BinaryOperator, span: &Span8) {
        match b.op_type {
            BinaryOperatorType::And => self.compile_and(b, span),
            BinaryOperatorType::Or => self.compile_or(b, span),
            BinaryOperatorType::Assign => self.compile_assign(b, span),
            BinaryOperatorType::DotAssign => self.compile_dot_assign(b, span),
            _ => self.compile_simple_binary(b, span),
        }
    }

    fn compile_simple_binary(&mut self, b: &BinaryOperator, span: &Span8) {
        self.compile_expr(false, &b.lhs.1, &b.lhs.0);
        self.compile_expr(false, &b.rhs.1, &b.rhs.0);
        let instr = match b.op_type {
            BinaryOperatorType::Add => Instruction::Add,
            BinaryOperatorType::Subtract => Instruction::Sub,
            BinaryOperatorType::Multiply => Instruction::Mul,
            BinaryOperatorType::Divide => Instruction::Div,
            BinaryOperatorType::IntegerDivide => Instruction::IntDiv,
            BinaryOperatorType::Power => Instruction::Power,
            BinaryOperatorType::Eq => Instruction::Eq,
            BinaryOperatorType::Ne => Instruction::Ne,
            BinaryOperatorType::Lt => Instruction::Lt,
            BinaryOperatorType::Le => Instruction::Le,
            BinaryOperatorType::Gt => Instruction::Gt,
            BinaryOperatorType::Ge => Instruction::Ge,
            BinaryOperatorType::In => Instruction::In,
            BinaryOperatorType::Append => Instruction::Append,
            _ => unreachable!(),
        };
        self.emit(instr, span.clone());
        self.dec_stack(1);
    }

    fn compile_assign(&mut self, b: &BinaryOperator, span: &Span8) {
        self.compile_expr(true, &b.lhs.1, &b.lhs.0);
        self.compile_expr(false, &b.rhs.1, &b.rhs.0);
        self.emit(Instruction::Assign, span.clone());
        self.dec_stack(1);
    }

    fn compile_dot_assign(&mut self, b: &BinaryOperator, span: &Span8) {
        self.compile_expr(true, &b.lhs.1, &b.lhs.0);
        self.compile_expr(false, &b.rhs.1, &b.rhs.0);
        self.emit(Instruction::AppendAssign, span.clone());
        self.dec_stack(1);
    }

    // `a && b`: short-circuit; result is 0 if a is falsy, else b
    fn compile_and(&mut self, b: &BinaryOperator, span: &Span8) {
        self.compile_expr(false, &b.lhs.1, &b.lhs.0);
        let jump_rhs = self.instruction_pointer();
        self.emit(
            Instruction::ConditionalJump { section: self.section_index(), to: 0 },
            span.clone(),
        );
        self.dec_stack(1);

        let false_idx = self.intern_int(0, span);
        self.emit_push(Instruction::PushInt { index: false_idx }, span.clone());
        let jump_end = self.instruction_pointer();
        self.emit(Instruction::Jump { section: self.section_index(), to: 0 }, span.clone());
        self.dec_stack(1); // undo push for tracking; merge point restores

        self.patch_jump(jump_rhs as usize, self.instruction_pointer());
        self.compile_expr(false, &b.rhs.1, &b.rhs.0);
        self.patch_jump(jump_end as usize, self.instruction_pointer());
    }

    // `a || b`: short-circuit; result is 1 if a is truthy, else b
    fn compile_or(&mut self, b: &BinaryOperator, span: &Span8) {
        self.compile_expr(false, &b.lhs.1, &b.lhs.0);
        let jump_true = self.instruction_pointer();
        self.emit(
            Instruction::ConditionalJump { section: self.section_index(), to: 0 },
            span.clone(),
        );
        self.dec_stack(1);

        self.compile_expr(false, &b.rhs.1, &b.rhs.0);
        let jump_end = self.instruction_pointer();
        self.emit(Instruction::Jump { section: self.section_index(), to: 0 }, span.clone());
        self.dec_stack(1);

        self.patch_jump(jump_true as usize, self.instruction_pointer());
        let true_idx = self.intern_int(1, span);
        self.emit_push(Instruction::PushInt { index: true_idx }, span.clone());
        self.patch_jump(jump_end as usize, self.instruction_pointer());
    }
}

impl Compiler {
    fn compile_unary(&mut self, u: &UnaryPreOperator) {
        self.compile_expr(false, &u.operand.1, &u.operand.0);
        let instr = match u.op_type {
            UnaryOperatorType::Negative => Instruction::Negate,
            UnaryOperatorType::Not => Instruction::Not,
        };
        self.emit(instr, u.operand.0.clone());
    }

    fn compile_subscript(&mut self, mutable: bool, s: &Subscript) {
        self.compile_expr(mutable, &s.base.1, &s.base.0);
        self.compile_expr(false, &s.index.1, &s.index.0);
        self.emit(Instruction::Subscript { mutable }, s.base.0.clone());
        self.dec_stack(1);
    }

    fn compile_property(&mut self, mutable: bool, p: &Property) {
        self.compile_expr(mutable, &p.base.1, &p.base.0);
        let attr = ident_ref_name(&p.attribute.1);
        let si = self.intern_string(attr, &p.attribute.0);
        self.emit(Instruction::Attribute { mutable, string_index: si }, p.attribute.0.clone());
    }
}

impl Compiler {
    fn compile_lambda_invoke(&mut self, l: &LambdaInvocation, span: &Span8) {
        let labeled = l.arguments.1.iter().any(|(lbl, _)| lbl.is_some());
        let stateful = is_stateful(&l.lambda.1)
            || l.arguments.1.iter().any(|(_, a)| is_stateful(&a.1));
        let num_args = l.arguments.1.len() as u8;

        for (_, arg) in &l.arguments.1 {
            self.compile_expr(false, &arg.1, &arg.0);
        }
        self.compile_expr(false, &l.lambda.1, &l.lambda.0);

        if labeled {
            for (lbl, _) in &l.arguments.1 {
                let si = match lbl {
                    Some((_, IdentifierDeclaration(name))) => self.intern_string(name, span),
                    None => u16::MAX,
                };
                self.emit(Instruction::BufferLabelOrAttribute { string_index: si }, span.clone());
            }
        }
        self.emit(Instruction::LambdaInvoke { stateful, labeled, num_args }, span.clone());
        self.dec_stack(num_args as usize); // goes from +(args+1) to +1
    }

    fn compile_operator_invoke(&mut self, o: &OperatorInvocation, span: &Span8) {
        let labeled = o.arguments.1.iter().any(|(lbl, _)| lbl.is_some());
        let stateful = is_stateful(&o.operator.1)
            || is_stateful(&o.operand.1)
            || o.arguments.1.iter().any(|(_, a)| is_stateful(&a.1));
        let num_args = o.arguments.1.len() as u8;

        self.compile_expr(false, &o.operand.1, &o.operand.0);
        for (_, arg) in &o.arguments.1 {
            self.compile_expr(false, &arg.1, &arg.0);
        }
        self.compile_expr(false, &o.operator.1, &o.operator.0);

        if labeled {
            for (lbl, _) in &o.arguments.1 {
                let si = match lbl {
                    Some((_, IdentifierDeclaration(name))) => self.intern_string(name, span),
                    None => u16::MAX,
                };
                self.emit(Instruction::BufferLabelOrAttribute { string_index: si }, span.clone());
            }
        }
        self.emit(Instruction::OperatorInvoke { stateful, labeled, num_args }, span.clone());
        self.dec_stack(num_args as usize + 1);
    }

    fn compile_native_invoke(&mut self, n: &NativeInvocation, span: &Span8) {
        let name = ident_ref_name(&n.function.1);
        for arg in &n.arguments {
            self.compile_expr(false, &arg.1, &arg.0);
        }
        // TODO: replace with a proper native function registry
        let index = self.intern_string(name, span) as u32;
        self.emit(Instruction::NativeInvoke { index }, span.clone());
        self.dec_stack(n.arguments.len());
        self.inc_stack();
    }
}

impl Compiler {
    fn compile_lambda(&mut self, l: &LambdaDefinition, span: &Span8) {
        let captures = self.compute_lambda_captures(l);

        for cap in &captures {
            if cap.var_type != VariableType::Let {
                self.error(
                    span.clone(),
                    "to make capture semantics clear, lambdas can only reference \"let\" variables",
                );
            }
        }

        let mut saw_default = false;
        for arg in &l.args {
            if arg.default_value.is_some() {
                saw_default = true;
            } else if saw_default {
                self.error(span.clone(), "required arguments must come before default arguments");
                break;
            }
        }

        let jump_idx = self.instruction_pointer();
        self.emit(Instruction::Jump { section: self.section_index(), to: 0 }, span.clone());

        let body_ip = self.instruction_pointer();
        self.push_frame();

        for (i, cap) in captures.iter().enumerate() {
            self.register_symbol(&cap.name, cap.var_type.clone(), i, true);
        }
        let cap_count = captures.len();

        let mut required_args: u8 = 0;
        let mut default_count: u8 = 0;

        for (i, arg) in l.args.iter().enumerate() {
            let vt = if arg.must_be_reference { VariableType::Reference } else { VariableType::Let };
            self.register_symbol(&arg.identifier.1.0, vt, cap_count + i, false);
            if arg.default_value.is_some() {
                default_count += 1;
            } else {
                required_args += 1;
            }
        }
        self.frame_mut().stack_depth = cap_count + l.args.len();

        match &l.body.1 {
            LambdaBody::Inline(expr) => {
                self.compile_expr(false, expr, &l.body.0);
                let below = self.stack_depth() as i32 - 1;
                self.emit(Instruction::Return { stack_delta: -below }, l.body.0.clone());
            }
            LambdaBody::Block(stmts) => {
                self.compile_block_body(stmts, &l.body.0);
            }
        }

        self.pop_frame();
        self.patch_jump(jump_idx as usize, self.instruction_pointer());

        for cap in &captures {
            let d = self.stack_delta(cap.stack_position);
            self.emit_push(Instruction::PushLvalue { stack_delta: d }, span.clone());
        }
        for arg in &l.args {
            if let Some(ref default) = arg.default_value {
                self.compile_expr(false, &default.1, &default.0);
            }
        }

        let proto_idx = self.current_section.lambda_prototypes.len() as u32;
        self.current_section.lambda_prototypes.push(LambdaPrototype {
            section: self.section_index(),
            ip: body_ip,
            required_args,
            default_arg_count: default_count,
        });

        let cap16 = captures.len() as u16;
        self.emit(Instruction::MakeLambda { prototype_index: proto_idx, capture_count: cap16 }, span.clone());
        self.dec_stack(captures.len() + default_count as usize);
        self.inc_stack();
    }

    fn compile_operator_def(&mut self, o: &OperatorDefinition, span: &Span8) {
        self.compile_expr(false, &o.lambda.1, &o.lambda.0);
        self.emit(Instruction::MakeOperator, span.clone());
    }

    fn compile_block(&mut self, b: &Block, span: &Span8) {
        let captures = self.compute_block_captures(&b.body);

        let jump_idx = self.instruction_pointer();
        self.emit(Instruction::Jump { section: self.section_index(), to: 0 }, span.clone());

        let body_ip = self.instruction_pointer();
        self.push_frame();
        for (i, cap) in captures.iter().enumerate() {
            // captured vars are read-only inside a block regardless of their outer type
            self.register_symbol(&cap.name, VariableType::Let, i, true);
        }
        self.frame_mut().stack_depth = captures.len();
        self.compile_block_body(&b.body, span);
        self.pop_frame();

        self.patch_jump(jump_idx as usize, self.instruction_pointer());

        for cap in &captures {
            let d = self.stack_delta(cap.stack_position);
            self.emit_push(Instruction::PushLvalue { stack_delta: d }, span.clone());
        }

        let proto_idx = self.current_section.lambda_prototypes.len() as u32;
        self.current_section.lambda_prototypes.push(LambdaPrototype {
            section: self.section_index(),
            ip: body_ip,
            required_args: 0,
            default_arg_count: 0,
        });

        let cap16 = captures.len() as u16;
        self.emit(
            Instruction::MakeLambda { prototype_index: proto_idx, capture_count: cap16 },
            span.clone(),
        );
        self.dec_stack(captures.len());
        self.inc_stack();

        self.emit(
            Instruction::LambdaInvoke { stateful: false, labeled: false, num_args: 0 },
            span.clone(),
        );
    }

    fn compile_anim(&mut self, a: &Anim, span: &Span8) {
        let captures = self.compute_block_captures(&a.body);

        for cap in &captures {
            if cap.var_type != VariableType::Let {
                self.error(
                    span.clone(),
                    "to make capture semantics clear, lambdas can only reference \"let\" variables",
                );
            }
        }

        let jump_idx = self.instruction_pointer();
        self.emit(Instruction::Jump { section: self.section_index(), to: 0 }, span.clone());

        let body_ip = self.instruction_pointer();
        self.push_frame();
        for (i, cap) in captures.iter().enumerate() {
            self.register_symbol(&cap.name, cap.var_type.clone(), i, true);
        }
        self.frame_mut().stack_depth = captures.len();

        self.push_scope();
        self.compile_statements(&a.body);
        self.pop_scope(span.clone());
        self.emit(Instruction::EndOfExecutionHead, span.clone());

        self.pop_frame();
        self.patch_jump(jump_idx as usize, self.instruction_pointer());

        for cap in &captures {
            let d = self.stack_delta(cap.stack_position);
            self.emit_push(Instruction::PushLvalue { stack_delta: d }, span.clone());
        }

        let proto_idx = self.current_section.anim_prototypes.len() as u32;
        self.current_section.anim_prototypes.push(AnimPrototype {
            section: self.section_index(),
            ip: body_ip,
        });

        let cap16 = captures.len() as u16;
        self.emit(
            Instruction::MakeAnim { prototype_index: proto_idx, capture_count: cap16 },
            span.clone(),
        );
        self.dec_stack(captures.len());
        self.inc_stack();
    }

    // compile a block body: init `_ = []`, compile stmts, implicit `return _`
    fn compile_block_body(&mut self, stmts: &[SpanTagged<Statement>], span: &Span8) {
        self.emit_push(Instruction::PushEmptyVector, span.clone());
        self.define_symbol("_", VariableType::Var);
        self.compile_statements(stmts);
        let underscore_pos = self.lookup("_").unwrap().stack_position;
        let d = self.stack_delta(underscore_pos);
        self.emit_push(Instruction::PushCopy { stack_delta: d }, span.clone());
        let below = self.stack_depth() as i32 - 1;
        self.emit(Instruction::Return { stack_delta: -below }, span.clone());
    }

    fn resolve_captures(&self, free: &[String]) -> Vec<Symbol> {
        free.iter().filter_map(|name| self.lookup(name).cloned()).collect()
    }

    fn compute_lambda_captures(&self, l: &LambdaDefinition) -> Vec<Symbol> {
        let mut pre: HashSet<String> =
            l.args.iter().map(|a| a.identifier.1.0.clone()).collect();
        if matches!(l.body.1, LambdaBody::Block(_)) {
            pre.insert("_".to_string());
        }
        let free = match &l.body.1 {
            LambdaBody::Inline(e) => free_vars_expr(e, pre),
            LambdaBody::Block(s) => free_vars_stmts(s, pre),
        };
        self.resolve_captures(&free)
    }

    fn compute_block_captures(&self, stmts: &[SpanTagged<Statement>]) -> Vec<Symbol> {
        let free = free_vars_stmts(stmts, HashSet::from(["_".to_string()]));
        self.resolve_captures(&free)
    }
}

#[cfg(test)]
mod test {
    use std::path::PathBuf;
    use std::rc::Rc;

    use bytecode::Instruction;
    use lexer::lexer::Lexer;
    use lexer::token::Token;
    use parser::ast::{
        BinaryOperator, BinaryOperatorType, Declaration, Expression, IdentifierDeclaration,
        IdentifierReference, LambdaArg, LambdaBody, LambdaDefinition, Literal, Section, SectionBundle,
        SectionType, Statement, VariableType,
    };
    use structs::rope::Rope;
    use structs::text::Span8;

    use crate::{compile, CompileResult};

    fn empty_span() -> Span8 {
        0..0
    }

    fn s<T>(v: T) -> (Span8, T) {
        (empty_span(), v)
    }

    fn sb<T>(v: T) -> (Span8, Box<T>) {
        (empty_span(), Box::new(v))
    }

    fn make_bundle(stmts: Vec<(Span8, Statement)>, section_type: SectionType) -> Rc<SectionBundle> {
        Rc::new(SectionBundle {
            file_path: PathBuf::new(),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section { body: stmts, section_type }],
        })
    }

    fn compile_stmts(stmts: Vec<(Span8, Statement)>) -> CompileResult {
        compile(&[make_bundle(stmts, SectionType::Slide)])
    }

    fn lex(src: &str) -> Vec<(Token, Span8)> {
        Lexer::token_stream(src.chars())
            .into_iter()
            .filter(|(t, _)| t != &Token::Whitespace && t != &Token::Comment)
            .collect()
    }

    fn parse_stmts(src: &str) -> Vec<(Span8, Statement)> {
        use parser::parser::SectionParser;

        let tokens = lex(src);
        let text_rope = Rope::from_str(src);
        let mut parser = SectionParser::new(tokens, text_rope, SectionType::Slide, None, None);
        parser.parse_statement_list()
    }

    fn compile_src(src: &str) -> CompileResult {
        compile_stmts(parse_stmts(src))
    }

    fn has_error(result: &CompileResult, fragment: &str) -> bool {
        result.errors.iter().any(|e| e.message.contains(fragment))
    }

    fn no_errors(result: &CompileResult) {
        if !result.errors.is_empty() {
            let msgs: Vec<_> = result.errors.iter().map(|e| e.message.as_str()).collect();
            panic!("expected no errors, got: {:?}", msgs);
        }
    }

    // -- pure compiler (AST) tests --

    #[test]
    fn test_let_int_decl() {
        let stmts = vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("x".into())),
            value: s(Expression::Literal(Literal::Int(42))),
        }))];
        let result = compile_stmts(stmts);
        no_errors(&result);
        let section = &result.bytecode.sections[0];
        assert!(section.instructions.iter().any(|i| matches!(i, Instruction::PushInt { .. })));
    }

    #[test]
    fn test_let_mutation_error() {
        // let x = 1; x = 2  — should error
        let stmts = vec![
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Let,
                identifier: s(IdentifierDeclaration("x".into())),
                value: s(Expression::Literal(Literal::Int(1))),
            })),
            s(Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                op_type: BinaryOperatorType::Assign,
                lhs: sb(Expression::IdentifierReference(IdentifierReference::Value("x".into()))),
                rhs: sb(Expression::Literal(Literal::Int(2))),
            }))),
        ];
        let result = compile_stmts(stmts);
        assert!(has_error(&result, "cannot mutate let"), "expected let mutation error");
    }

    #[test]
    fn test_undefined_variable_error() {
        let stmts = vec![s(Statement::Expression(Expression::IdentifierReference(
            IdentifierReference::Value("notdefined".into()),
        )))];
        let result = compile_stmts(stmts);
        assert!(has_error(&result, "undefined variable"), "expected undefined variable error");
    }

    #[test]
    fn test_break_outside_loop_error() {
        let result = compile_stmts(vec![s(Statement::Break)]);
        assert!(has_error(&result, "break outside loop"));
    }

    #[test]
    fn test_continue_outside_loop_error() {
        let result = compile_stmts(vec![s(Statement::Continue)]);
        assert!(has_error(&result, "continue outside loop"));
    }

    #[test]
    fn test_lambda_capture_non_let_error() {
        // var x = 1; let f = |-> x  — lambda captures non-let var
        let stmts = vec![
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Var,
                identifier: s(IdentifierDeclaration("x".into())),
                value: s(Expression::Literal(Literal::Int(1))),
            })),
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Let,
                identifier: s(IdentifierDeclaration("f".into())),
                value: s(Expression::LambdaDefinition(LambdaDefinition {
                    args: vec![],
                    body: s(LambdaBody::Inline(Box::new(Expression::IdentifierReference(
                        IdentifierReference::Value("x".into()),
                    )))),
                })),
            })),
        ];
        let result = compile_stmts(stmts);
        assert!(has_error(&result, "lambdas can only reference"), "expected capture-must-be-let error");
    }

    #[test]
    fn test_lambda_capture_let_ok() {
        // let x = 1; let f = |-> x  — should be fine
        let stmts = vec![
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Let,
                identifier: s(IdentifierDeclaration("x".into())),
                value: s(Expression::Literal(Literal::Int(1))),
            })),
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Let,
                identifier: s(IdentifierDeclaration("f".into())),
                value: s(Expression::LambdaDefinition(LambdaDefinition {
                    args: vec![],
                    body: s(LambdaBody::Inline(Box::new(Expression::IdentifierReference(
                        IdentifierReference::Value("x".into()),
                    )))),
                })),
            })),
        ];
        no_errors(&compile_stmts(stmts));
    }

    #[test]
    fn test_default_args_must_be_suffix() {
        // |a = 1, b| b  — required arg after default arg → error
        let stmts = vec![s(Statement::Expression(Expression::LambdaDefinition(LambdaDefinition {
            args: vec![
                LambdaArg {
                    identifier: s(IdentifierDeclaration("a".into())),
                    default_value: Some(s(Expression::Literal(Literal::Int(1)))),
                    must_be_reference: false,
                },
                LambdaArg {
                    identifier: s(IdentifierDeclaration("b".into())),
                    default_value: None,
                    must_be_reference: false,
                },
            ],
            body: s(LambdaBody::Inline(Box::new(Expression::IdentifierReference(
                IdentifierReference::Value("b".into()),
            )))),
        })))];
        let result = compile_stmts(stmts);
        assert!(
            has_error(&result, "required arguments must come before default arguments"),
            "expected default-arg suffix error"
        );
    }

    #[test]
    fn test_default_args_valid_suffix() {
        // |a, b = 1| a  — correct ordering, no error
        let stmts = vec![s(Statement::Expression(Expression::LambdaDefinition(LambdaDefinition {
            args: vec![
                LambdaArg {
                    identifier: s(IdentifierDeclaration("a".into())),
                    default_value: None,
                    must_be_reference: false,
                },
                LambdaArg {
                    identifier: s(IdentifierDeclaration("b".into())),
                    default_value: Some(s(Expression::Literal(Literal::Int(1)))),
                    must_be_reference: false,
                },
            ],
            body: s(LambdaBody::Inline(Box::new(Expression::IdentifierReference(
                IdentifierReference::Value("a".into()),
            )))),
        })))];
        no_errors(&compile_stmts(stmts));
    }

    // -- cross-bundle import tests --

    #[test]
    fn test_cross_bundle_symbol_visible() {
        // bundle 0 defines `x`; bundle 1 imports it and reads it — no error
        let bundle0 = Rc::new(SectionBundle {
            file_path: PathBuf::new(),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section {
                body: vec![s(Statement::Declaration(Declaration {
                    var_type: VariableType::Let,
                    identifier: s(IdentifierDeclaration("x".into())),
                    value: s(Expression::Literal(Literal::Int(7))),
                }))],
                section_type: SectionType::UserLibrary,
            }],
        });
        let bundle1 = Rc::new(SectionBundle {
            file_path: PathBuf::new(),
            file_index: 1,
            imported_files: vec![0],
            sections: vec![Section {
                body: vec![s(Statement::Expression(Expression::IdentifierReference(
                    IdentifierReference::Value("x".into()),
                )))],
                section_type: SectionType::Slide,
            }],
        });
        no_errors(&compile(&[bundle0, bundle1]));
    }

    #[test]
    fn test_cross_bundle_symbol_is_let() {
        // bundle 0 defines `var x`; bundle 1 imports it and tries to assign — error
        let bundle0 = Rc::new(SectionBundle {
            file_path: PathBuf::new(),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section {
                body: vec![s(Statement::Declaration(Declaration {
                    var_type: VariableType::Var,
                    identifier: s(IdentifierDeclaration("x".into())),
                    value: s(Expression::Literal(Literal::Int(0))),
                }))],
                section_type: SectionType::UserLibrary,
            }],
        });
        let bundle1 = Rc::new(SectionBundle {
            file_path: PathBuf::new(),
            file_index: 1,
            imported_files: vec![0],
            sections: vec![Section {
                body: vec![s(Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                    op_type: BinaryOperatorType::Assign,
                    lhs: sb(Expression::IdentifierReference(IdentifierReference::Value("x".into()))),
                    rhs: sb(Expression::Literal(Literal::Int(1))),
                })))],
                section_type: SectionType::Slide,
            }],
        });
        let result = compile(&[bundle0, bundle1]);
        assert!(has_error(&result, "cannot mutate let"), "imported var should become let");
    }

    #[test]
    fn test_integration_arithmetic() {
        no_errors(&compile_src("let x = 1 + 2 * 3"));
    }

    #[test]
    fn test_integration_while_loop() {
        no_errors(&compile_src("var i = 0\nwhile i < 10 {\n    i = i + 1\n}"));
    }

    #[test]
    fn test_integration_for_loop() {
        // vector literal uses [] in Monocurl
        no_errors(&compile_src("let xs = [1, 2, 3]\nfor x in xs {\n}"));
    }

    #[test]
    fn test_integration_if_else() {
        no_errors(&compile_src("let x = 1\nif x == 1 {\n} else {\n}"));
    }

    #[test]
    fn test_integration_lambda_definition_and_call() {
        no_errors(&compile_src("let f = |a, b| a + b\nlet y = f(1, 2)"));
    }

    #[test]
    fn test_integration_short_circuit_and() {
        // compile a && — the result is computed but we just check no errors and
        // that a ConditionalJump was emitted somewhere in the main section
        // Monocurl uses keyword `and` not `&&`
        let result = compile_src("let z = 1 and 0");
        no_errors(&result);
        let has_cj = result
            .bytecode
            .sections
            .iter()
            .any(|sec| sec.instructions.iter().any(|i| matches!(i, Instruction::ConditionalJump { .. })));
        assert!(has_cj, "expected ConditionalJump in bytecode for 'and'");
    }

    #[test]
    fn test_integration_short_circuit_or() {
        // Monocurl uses keyword `or` not `||`
        let result = compile_src("let z = 0 or 1");
        no_errors(&result);
        let has_cj = result
            .bytecode
            .sections
            .iter()
            .any(|sec| sec.instructions.iter().any(|i| matches!(i, Instruction::ConditionalJump { .. })));
        assert!(has_cj, "expected ConditionalJump in bytecode for 'or'");
    }

    #[test]
    fn test_integration_nested_vector() {
        // [] for vectors in Monocurl
        no_errors(&compile_src("let v = [1, [2, 3], 4]"));
    }

    #[test]
    fn test_integration_map_literal() {
        // [:] map syntax in Monocurl
        no_errors(&compile_src(r#"let m = ["a": 1, "b": 2]"#));
    }

    #[test]
    fn test_integration_default_arg_ordering_error() {
        let result = compile_src("let f = |a = 1, b| b");
        assert!(
            has_error(&result, "required arguments must come before default arguments"),
            "expected default-arg suffix error from parser-produced AST"
        );
    }

    // -- bytecode sequence tests --

    // `let x = 42` should produce exactly PushInt + EndOfExecutionHead
    // with 42 in the int pool.
    #[test]
    fn test_bytecode_single_let_int() {
        use bytecode::LambdaPrototype;
        let result = compile_stmts(vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("x".into())),
            value: s(Expression::Literal(Literal::Int(42))),
        }))]);
        no_errors(&result);
        let sec = &result.bytecode.sections[0];
        assert_eq!(
            sec.instructions,
            vec![Instruction::PushInt { index: 0 }, Instruction::EndOfExecutionHead],
        );
        assert_eq!(sec.int_pool, vec![42i64]);
        assert!(sec.lambda_prototypes.is_empty());
    }

    // `var x = 0\nx = 1` — covers PushLvalue, Assign, and Pop for expression statements.
    #[test]
    fn test_bytecode_var_assign() {
        let result = compile_stmts(vec![
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Var,
                identifier: s(IdentifierDeclaration("x".into())),
                value: s(Expression::Literal(Literal::Int(0))),
            })),
            s(Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                op_type: BinaryOperatorType::Assign,
                lhs: sb(Expression::IdentifierReference(IdentifierReference::Value("x".into()))),
                rhs: sb(Expression::Literal(Literal::Int(1))),
            }))),
        ]);
        no_errors(&result);
        let sec = &result.bytecode.sections[0];
        assert_eq!(
            sec.instructions,
            vec![
                Instruction::PushInt { index: 0 },          // var x = 0
                Instruction::PushLvalue { stack_delta: -1 }, // lvalue of x (at pos 0, depth 1)
                Instruction::PushInt { index: 1 },           // rhs = 1
                Instruction::Assign,
                Instruction::Pop { count: 1 },               // discard assign result
                Instruction::EndOfExecutionHead,
            ],
        );
        assert_eq!(sec.int_pool, vec![0i64, 1i64]);
    }

    // `let z = 1 and 0` — verifies short-circuit ConditionalJump structure.
    #[test]
    fn test_bytecode_and_short_circuit() {
        let result = compile_stmts(vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("z".into())),
            value: s(Expression::BinaryOperator(BinaryOperator {
                op_type: BinaryOperatorType::And,
                lhs: sb(Expression::Literal(Literal::Int(1))),
                rhs: sb(Expression::Literal(Literal::Int(0))),
            })),
        }))]);
        no_errors(&result);
        let sec = &result.bytecode.sections[0];
        // [0] PushInt(1)  [1] ConditionalJump→4  [2] PushInt(0)  [3] Jump→5
        // [4] PushInt(0)  [5] EndOfExecutionHead
        assert_eq!(sec.instructions[0], Instruction::PushInt { index: 0 }); // lhs = 1
        assert!(matches!(sec.instructions[1], Instruction::ConditionalJump { to: 4, .. }));
        assert_eq!(sec.instructions[2], Instruction::PushInt { index: 1 }); // false literal
        assert!(matches!(sec.instructions[3], Instruction::Jump { to: 5, .. }));
        assert_eq!(sec.instructions[4], Instruction::PushInt { index: 1 }); // rhs = 0 (same pool slot)
        assert_eq!(sec.instructions[5], Instruction::EndOfExecutionHead);
        assert_eq!(sec.int_pool[0], 1i64);
        assert_eq!(sec.int_pool[1], 0i64);
    }

    // `let f = |a| a` — verifies lambda body is Jump-over + body + MakeLambda
    // and that the prototype table is populated correctly.
    #[test]
    fn test_bytecode_simple_lambda() {
        use bytecode::LambdaPrototype;
        let result = compile_stmts(vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("f".into())),
            value: s(Expression::LambdaDefinition(LambdaDefinition {
                args: vec![LambdaArg {
                    identifier: s(IdentifierDeclaration("a".into())),
                    default_value: None,
                    must_be_reference: false,
                }],
                body: s(LambdaBody::Inline(Box::new(Expression::IdentifierReference(
                    IdentifierReference::Value("a".into()),
                )))),
            })),
        }))]);
        no_errors(&result);
        let sec = &result.bytecode.sections[0];
        // [0] Jump{to:3}  [1] PushCopy{-1}  [2] Return{-1}  [3] MakeLambda{proto:0,cap:0}
        // [4] EndOfExecutionHead
        assert!(matches!(sec.instructions[0], Instruction::Jump { to: 3, .. }));
        assert_eq!(sec.instructions[1], Instruction::PushCopy { stack_delta: -1 });
        assert_eq!(sec.instructions[2], Instruction::Return { stack_delta: -1 });
        assert_eq!(
            sec.instructions[3],
            Instruction::MakeLambda { prototype_index: 0, capture_count: 0 },
        );
        assert_eq!(sec.instructions[4], Instruction::EndOfExecutionHead);
        assert_eq!(sec.lambda_prototypes.len(), 1);
        assert_eq!(sec.lambda_prototypes[0].required_args, 1);
        assert_eq!(sec.lambda_prototypes[0].default_arg_count, 0);
        assert_eq!(sec.lambda_prototypes[0].ip, 1);
    }

}
