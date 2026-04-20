mod free_vars;
mod stateful;
mod warnings;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use bytecode::{
    AnimPrototype, Bytecode, Instruction, InstructionAnnotation, LambdaPrototype, SectionBytecode,
    SectionFlags,
};
use free_vars::{free_lvalue_refs_expr, free_vars_expr, free_vars_stmts};
use parser::ast::{
    Anim, BinaryOperator, BinaryOperatorType, Block, Declaration, DirectionalLiteral, Expression,
    For, IdentifierDeclaration, IdentifierReference, If, InvocationArguments, LambdaArg,
    LambdaBody, LambdaDefinition, LambdaInvocation, Literal, NativeInvocation, OperatorDefinition,
    OperatorInvocation, Play, Property, Return, Section, SectionBundle, SectionType, SpanTagged,
    Statement, Subscript, UnaryOperatorType, UnaryPreOperator, VariableType as AstVariableType,
    While,
};
use stateful::is_stateful;
use stdlib::registry::registry;
use structs::text::{Count8, Span8};
use warnings::expression_statement_has_no_effect;

use crate::cache::CompilerCache;

#[derive(Clone)]
pub struct CompileError {
    pub span: Span8,
    pub message: String,
}

#[derive(Clone)]
pub struct CompileWarning {
    pub span: Span8,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CursorIdentifierType {
    Lambda,
    Operator,
    Let,
    Var,
    Mesh,
    Param,
}

// autocomplete suggetion effectively
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CursorIdentifier {
    pub name: String,
    pub identifier_type: CursorIdentifierType,
    stack_position: usize,
    frame_depth: usize,
}

pub struct Reference {
    pub symbol: Arc<Symbol>,
    pub span: Span8,
    // if this is a functional reference, the spans of all arguments and each individual one
    pub invocation_spans: Option<(Span8, Vec<Span8>)>,
}

#[derive(Default)]
pub struct CompileResult {
    pub bytecode: Bytecode,
    pub errors: Vec<CompileError>,
    pub warnings: Vec<CompileWarning>,
    // it is guaranteed these will be emitted in deepest first order
    pub root_references: Vec<Reference>,
    pub possible_cursor_identifiers: Vec<CursorIdentifier>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VariableType {
    Let,
    Var,
    Reference,
    Param,
    Mesh,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionArgInfo {
    pub name: String,
    pub has_default: bool,
    pub is_reference: bool,
}

#[derive(Clone, Debug)]
pub enum SymbolFunctionInfo {
    None,
    Lambda { args: Vec<FunctionArgInfo> },
    Operator { args: Vec<FunctionArgInfo> },
}

impl SymbolFunctionInfo {
    fn arg_info(arg: &LambdaArg) -> FunctionArgInfo {
        FunctionArgInfo {
            name: arg.identifier.1.0.clone(),
            has_default: arg.default_value.is_some(),
            is_reference: arg.must_be_reference,
        }
    }

    fn from(value: &Expression) -> Self {
        match value {
            Expression::LambdaDefinition(l) => SymbolFunctionInfo::Lambda {
                args: l.args.iter().map(Self::arg_info).collect(),
            },
            Expression::OperationDefinition(o) => SymbolFunctionInfo::Operator {
                args: match &*o.lambda.1 {
                    Expression::LambdaDefinition(l) => l.args.iter().map(Self::arg_info).collect(),
                    // difficult to infer in this case
                    _ => Vec::new(),
                },
            },
            _ => SymbolFunctionInfo::None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub name: String,
    imported: bool,
    stack_position: usize,
    // how the symbol looks like to the current bundle
    // may appear as let, even though declared as var
    pub var_type: VariableType,
    pub function_info: SymbolFunctionInfo,
}

#[derive(Clone)]
struct Scope {
    base_stack_depth: usize,
    symbols: HashMap<String, Arc<Symbol>>,
}

struct LoopContext {
    // Some(ip) for while loops (known upfront); None for for loops (patched later)
    continue_target: Option<u32>,
    stack_depth_at_loop: usize,
    break_patches: Vec<usize>,
    continue_patches: Vec<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    /// top-level slide section: play allowed, return not
    Root,
    /// lambda body: return allowed, play not
    Lambda,
    /// block body (immediately invoked): return allowed, play not
    Block,
    /// anim body: play allowed, return not
    Anim,
}

struct CompilerFrame {
    scopes: Vec<Scope>,
    // reference point for how many variables would be on the stack upon executing the current instruction
    stack_depth: usize,
    loop_contexts: Vec<LoopContext>,
    kind: FrameKind,
}

fn ident_ref_name(ir: &IdentifierReference) -> &str {
    match ir {
        IdentifierReference::Value(n)
        | IdentifierReference::StatefulReference(n)
        | IdentifierReference::StatefulDereference(n)
        | IdentifierReference::Reference(n) => n,
    }
}

pub(crate) struct CompileBundle {
    path: Option<PathBuf>,
    is_root_bundle: bool,
    import_display_index: Option<usize>,
    exports: HashMap<String, Arc<Symbol>>,
    errors: Vec<CompileError>,
    warnings: Vec<CompileWarning>,
    bytecode: Vec<Arc<SectionBytecode>>,
    current_bytecode: Option<SectionBytecode>,
    final_stack_depth: usize,
}

struct Compiler {
    // inputs
    cursor_pos: Option<Count8>,
    // compiler state
    frames: Vec<CompilerFrame>,

    current_bundle: Option<CompileBundle>,

    // output
    compile_bundles: Vec<Arc<CompileBundle>>,
    errors: Vec<CompileError>,     // mounted onto root bundle
    warnings: Vec<CompileWarning>, // mounted onto root bundle
    // only applicable to root bundle
    references: Vec<Reference>,
    cursor_identifier_set: HashSet<String>,
    possible_cursor_identifiers: Vec<CursorIdentifier>,

    // of the current bundle
    bundle_root_import_span: Option<Span8>,
}

impl Compiler {
    fn new(cursor_pos: Option<Count8>) -> Self {
        Self {
            frames: vec![CompilerFrame {
                scopes: vec![Scope {
                    base_stack_depth: 0,
                    symbols: HashMap::new(),
                }],
                stack_depth: 0,
                loop_contexts: Vec::new(),
                kind: FrameKind::Root,
            }],
            current_bundle: None,
            compile_bundles: Vec::new(),
            errors: Vec::new(),
            warnings: Vec::new(),
            bundle_root_import_span: None,
            cursor_pos,
            cursor_identifier_set: HashSet::new(),
            possible_cursor_identifiers: Vec::new(),
            references: Vec::new(),
        }
    }

    fn finish(self) -> (CompilerCache, CompileResult) {
        // reorder identifiers
        let mut possible_cursor_identifiers = self.possible_cursor_identifiers;
        possible_cursor_identifiers.sort_by_key(|id| (id.frame_depth, id.stack_position));
        possible_cursor_identifiers.reverse();

        let bytecode_sections = self
            .compile_bundles
            .iter()
            .flat_map(|sec| sec.bytecode.iter().cloned())
            .collect();

        let cache = CompilerCache {
            last_bundles: self.compile_bundles,
        };

        let result = CompileResult {
            bytecode: Bytecode::new(bytecode_sections),
            errors: self.errors,
            warnings: self.warnings,
            possible_cursor_identifiers,
            root_references: self.references,
        };

        (cache, result)
    }
}

pub fn compile(
    compiler_cache: &mut CompilerCache,
    cursor_pos: Option<Count8>,
    bundles: &[Arc<SectionBundle>],
) -> CompileResult {
    let mut c = Compiler::new(cursor_pos);
    c.compile_prelude();

    let mut allowing_caches = true;
    for (ind, bundle) in bundles.iter().enumerate() {
        let can_use_cache = allowing_caches &&
            bundle.was_cached &&
            // skip prelude
            ind + 1 < compiler_cache.last_bundles.len() &&
            compiler_cache.last_bundles[ind + 1].path == bundle.file_path;

        if can_use_cache {
            c.emit_cached_bundle(
                compiler_cache.last_bundles[ind + 1].clone(),
                bundle.root_import_span.clone(),
            );
        } else {
            allowing_caches = false;
            c.compile_bundle(bundle);
        }
    }
    let (cache, result) = c.finish();
    *compiler_cache = cache;
    result
}

impl Compiler {
    fn compile_prelude(&mut self) {
        self.current_bundle = Some(CompileBundle {
            path: None,
            is_root_bundle: true,
            import_display_index: None,
            exports: HashMap::default(),
            errors: Vec::default(),
            warnings: Vec::default(),
            bytecode: vec![],
            final_stack_depth: 0,
            current_bytecode: Some(SectionBytecode::new(SectionFlags {
                is_stdlib: true,
                is_library: true,
                is_init: false,
                is_root_module: true,
            })),
        });

        // define global scene variables
        for (var, init) in [
            ("camera", "initial_camera"),
            ("background", "initial_background"),
        ] {
            self.emit_push(
                Instruction::NativeInvoke {
                    index: registry().index_of(init) as u16,
                    arg_count: 0,
                },
                0..0,
            );
            let name_index = self.intern_string(var);
            self.emit(Instruction::ConvertParam { name_index }, 0..0);
            self.define_symbol(var, VariableType::Param, SymbolFunctionInfo::None);
        }

        self.emit(Instruction::EndOfExecutionHead, 0..0);

        self.emit_current_section();
        self.emit_current_bundle();
    }

    fn emit_current_section(&mut self) {
        let bundle = self.current_bundle.as_mut().unwrap();
        bundle
            .bytecode
            .push(Arc::new(bundle.current_bytecode.take().unwrap()));
    }

    fn emit_current_bundle(&mut self) {
        let mut bundle = self.current_bundle.take().unwrap();
        bundle.final_stack_depth = self.frame().stack_depth;
        bundle.exports = self
            .frame()
            .scopes
            .iter()
            .flat_map(|s| s.symbols.iter().map(|(k, v)| (k.clone(), v.clone())))
            .filter(|sym| !sym.1.imported)
            .collect();
        self.compile_bundles.push(Arc::new(bundle));
    }

    fn emit_cached_bundle(
        &mut self,
        cached_bundle: Arc<CompileBundle>,
        actual_import_span: Option<Span8>,
    ) {
        self.bundle_root_import_span = actual_import_span;

        self.frame_mut().stack_depth = cached_bundle.final_stack_depth;
        for error in &cached_bundle.errors {
            let updated_span = self
                .bundle_root_import_span
                .clone()
                .unwrap_or(error.span.clone());
            self.errors.push(CompileError {
                span: updated_span,
                message: error.message.clone(),
            });
        }
        for warning in &cached_bundle.warnings {
            let updated_span = self
                .bundle_root_import_span
                .clone()
                .unwrap_or(warning.span.clone());
            self.warnings.push(CompileWarning {
                span: updated_span,
                message: warning.message.clone(),
            });
        }

        self.compile_bundles.push(cached_bundle);
    }

    fn compile_bundle(&mut self, bundle: &SectionBundle) {
        self.bundle_root_import_span = bundle.root_import_span.clone();

        let mut base_symbols = HashMap::new();

        // take into account prelude
        let mapped_imports = bundle
            .imported_files
            .iter()
            .map(|x| 1 + *x)
            .chain(std::iter::once(0));
        for p in mapped_imports {
            for sym in self.compile_bundles[p].exports.values() {
                base_symbols.insert(
                    sym.name.clone(),
                    Arc::new(Symbol {
                        // make it constant for future sections (unless from prelude)
                        var_type: if p == 0 {
                            sym.var_type.clone()
                        } else {
                            VariableType::Let
                        },
                        imported: true,
                        ..sym.as_ref().clone()
                    }),
                );
            }
        }

        // we are just hiding the non imported symbols, but the imported symbols take place
        // in the same order as before (by keeping their stack position)
        // the overall stack depth is also the same as of the previous section, irrespective of whether or not it was imported
        self.frame_mut().scopes = vec![Scope {
            base_stack_depth: 0,
            symbols: base_symbols,
        }];

        self.current_bundle = Some(CompileBundle {
            path: bundle.file_path.clone(),
            is_root_bundle: bundle.root_import_span.is_none(),
            import_display_index: bundle.root_import_span.as_ref().map(|_| {
                self.compile_bundles
                    .iter()
                    .filter(|bundle| !bundle.is_root_bundle)
                    .count()
                    + 1
            }),
            exports: HashMap::default(),
            errors: Vec::default(),
            warnings: Vec::default(),
            bytecode: Vec::default(),
            current_bytecode: None,
            final_stack_depth: 0,
        });

        for section in &bundle.sections {
            self.compile_section(section);
        }

        self.emit_current_bundle();
    }

    fn compile_section(&mut self, section: &Section) {
        if self.compile_bundles.len() >= u16::MAX as usize {
            self.error(0..0, "too many sections (limit 65535)");
            return;
        }

        let current_bundle = self.current_bundle.as_ref().unwrap();
        let mut bytecode = SectionBytecode::new(SectionFlags {
            is_stdlib: section.section_type == SectionType::StandardLibrary,
            is_library: matches!(
                section.section_type,
                SectionType::UserLibrary | SectionType::StandardLibrary
            ),
            is_init: section.section_type == SectionType::Init,
            is_root_module: self.bundle_root_import_span.is_none(),
        });
        bytecode.source_file_name = current_bundle.path.as_ref().and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });
        bytecode.import_display_index = current_bundle.import_display_index;
        self.current_bundle.as_mut().unwrap().current_bytecode = Some(bytecode);

        // symbols declared here land in the current top scope (no push/pop)
        self.compile_statements(&section.body);
        if self.current_section().flags.is_init {
            self.emit(Instruction::SyncAllLeaders, 0..0);
        }
        self.emit(Instruction::EndOfExecutionHead, 0..0);

        self.emit_current_section();
    }
}

impl Compiler {
    fn current_section(&self) -> &SectionBytecode {
        self.current_bundle
            .as_ref()
            .unwrap()
            .current_bytecode
            .as_ref()
            .unwrap()
    }

    fn current_section_mut(&mut self) -> &mut SectionBytecode {
        self.current_bundle
            .as_mut()
            .unwrap()
            .current_bytecode
            .as_mut()
            .unwrap()
    }

    fn emit(&mut self, instruction: Instruction, src: Span8) {
        if self.current_section().instructions.len() >= u32::MAX as usize {
            self.error(src.clone(), "too many instructions in section");
        }
        self.current_section_mut().instructions.push(instruction);

        let real_span = self.bundle_root_import_span.clone().unwrap_or(src);
        self.current_section_mut()
            .annotations
            .push(InstructionAnnotation {
                source_loc: real_span,
            });
    }

    fn emit_push(&mut self, instruction: Instruction, src: Span8) {
        self.emit(instruction, src);
        self.inc_stack();
    }

    fn instruction_pointer(&self) -> u32 {
        self.current_section().instructions.len() as u32
    }

    fn section_index(&self) -> u16 {
        let current_bundle_sections = self
            .current_bundle
            .as_ref()
            .map_or(0, |bundle| bundle.bytecode.len());
        (self.compile_bundles.len() + current_bundle_sections) as u16
    }

    fn patch_jump(&mut self, instr_idx: usize, target: u32) {
        match &mut self.current_section_mut().instructions[instr_idx] {
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
        self.emit(
            Instruction::Pop {
                count: count as u32,
            },
            span,
        );
        self.dec_stack(count);
    }

    fn emit_copy(&mut self, stack_delta: i32, span: Span8) {
        self.emit_push(
            Instruction::PushCopy {
                stack_delta,
                pop_tos: false,
                mutable: false,
            },
            span,
        );
    }

    fn emit_copy_ref(&mut self, stack_delta: i32, span: Span8) {
        self.emit_push(
            Instruction::PushCopy {
                stack_delta,
                pop_tos: false,
                mutable: true,
            },
            span,
        );
    }

    fn emit_lvalue(&mut self, stack_delta: i32, span: Span8) {
        self.emit_push(
            Instruction::PushLvalue {
                stack_delta,
                force_ephemeral: false,
            },
            span,
        );
    }

    fn emit_lvalue_ephemeral(&mut self, stack_delta: i32, span: Span8) {
        self.emit_push(
            Instruction::PushLvalue {
                stack_delta,
                force_ephemeral: true,
            },
            span,
        );
    }

    fn emit_push_int(&mut self, val: i64, span: Span8) {
        let idx = self.intern_int(val);
        self.emit_push(Instruction::PushInt { index: idx }, span);
    }

    /// emit a Jump with section set and `to` = 0; returns the instruction index for later patching
    fn emit_jump_patch(&mut self, span: Span8) -> usize {
        let idx = self.instruction_pointer() as usize;
        self.emit(
            Instruction::Jump {
                section: self.section_index(),
                to: 0,
            },
            span,
        );
        idx
    }

    /// emit a ConditionalJump with section set and `to` = 0, consuming the condition (dec_stack);
    /// returns the instruction index for later patching
    fn emit_cond_jump_patch(&mut self, span: Span8) -> usize {
        let idx = self.instruction_pointer() as usize;
        self.emit(
            Instruction::ConditionalJump {
                section: self.section_index(),
                to: 0,
            },
            span,
        );
        self.dec_stack(1);
        idx
    }

    fn emit_jump_to(&mut self, target: u32, span: Span8) {
        self.emit(
            Instruction::Jump {
                section: self.section_index(),
                to: target,
            },
            span,
        );
    }

    fn push_scope(&mut self) {
        let base = self.stack_depth();
        self.frame_mut().scopes.push(Scope {
            base_stack_depth: base,
            symbols: HashMap::new(),
        });
    }

    fn pop_scope(&mut self, span: Span8) {
        let scope = self.frame_mut().scopes.pop().unwrap();
        let to_pop = self.stack_depth() - scope.base_stack_depth;
        self.emit_pops(to_pop, span);
    }

    fn define_symbol(
        &mut self,
        name: &str,
        var_type: VariableType,
        function_info: SymbolFunctionInfo,
    ) {
        let position = self.stack_depth() - 1;
        self.frame_mut().scopes.last_mut().unwrap().symbols.insert(
            name.to_string(),
            Arc::new(Symbol {
                name: name.to_string(),
                stack_position: position,
                var_type,
                function_info,
                imported: false,
            }),
        );
    }

    fn register_symbol(
        &mut self,
        name: &str,
        var_type: VariableType,
        function_info: SymbolFunctionInfo,
        position: usize,
    ) {
        self.frame_mut().scopes.last_mut().unwrap().symbols.insert(
            name.to_string(),
            Arc::new(Symbol {
                name: name.to_string(),
                stack_position: position,
                var_type,
                function_info,
                imported: false,
            }),
        );
    }

    fn lookup(
        &mut self,
        name: &str,
        source_span: Option<Span8>,
        inv_args: Option<&InvocationArguments>,
    ) -> Option<Arc<Symbol>> {
        for scope in self.frame().scopes.iter().rev() {
            if let Some(sym) = scope.symbols.get(name) {
                let clone = sym.clone();

                if let Some(source_span) = source_span
                    && self.bundle_root_import_span.is_none()
                {
                    // we are in root, so this constitutes a root reference
                    self.references.push(Reference {
                        span: source_span,
                        symbol: clone.clone(),
                        invocation_spans: inv_args.map(|inv| {
                            (
                                inv.0.clone(),
                                inv.1.iter().map(|(_, arg)| arg.0.clone()).collect(),
                            )
                        }),
                    });
                }

                return Some(clone);
            }
        }
        None
    }

    fn push_frame(&mut self, kind: FrameKind) {
        self.frames.push(CompilerFrame {
            scopes: vec![Scope {
                base_stack_depth: 0,
                symbols: HashMap::new(),
            }],
            stack_depth: 0,
            loop_contexts: Vec::new(),
            kind,
        });
    }

    fn pop_frame(&mut self) {
        self.frames.pop();
    }

    // -- constant pool helpers --

    fn intern_int(&mut self, val: i64) -> u32 {
        if let Some(idx) = self
            .current_section()
            .int_pool
            .iter()
            .position(|&x| x == val)
        {
            return idx as u32;
        }
        let idx = self.current_section_mut().int_pool.len();
        self.current_section_mut().int_pool.push(val);
        idx as u32
    }

    fn intern_float(&mut self, val: f64) -> u32 {
        if let Some(idx) = self
            .current_section()
            .float_pool
            .iter()
            .position(|x| x.to_bits() == val.to_bits())
        {
            return idx as u32;
        }
        let idx = self.current_section_mut().float_pool.len();
        self.current_section_mut().float_pool.push(val);
        idx as u32
    }

    fn intern_string(&mut self, val: &str) -> u32 {
        if let Some(idx) = self
            .current_section()
            .string_pool
            .iter()
            .position(|x| x == val)
        {
            return idx as u32;
        }
        let idx = self.current_section_mut().string_pool.len();
        self.current_section_mut().string_pool.push(val.to_string());
        idx as u32
    }

    fn error(&mut self, span: Span8, msg: impl Into<String>) {
        let real_span = self.bundle_root_import_span.clone().unwrap_or(span);

        let error = CompileError {
            span: real_span.clone(),
            message: msg.into(),
        };
        self.errors.push(error.clone());
        self.current_bundle.as_mut().unwrap().errors.push(error);
    }

    fn warning(&mut self, span: Span8, msg: impl Into<String>) {
        let real_span = self.bundle_root_import_span.clone().unwrap_or(span);

        let warning = CompileWarning {
            span: real_span.clone(),
            message: msg.into(),
        };
        self.warnings.push(warning.clone());
        self.current_bundle.as_mut().unwrap().warnings.push(warning);
    }
}

impl Compiler {
    // dump all visible symbols at the current positions
    fn infer_possible_cursor_identifiers(&mut self, statement_span: Span8) {
        if self
            .cursor_pos
            .is_none_or(|cp| !statement_span.contains(&cp) && !statement_span.end.eq(&cp))
        {
            return;
        }

        for (frame_depth, frame) in self.frames.iter().enumerate().rev() {
            for scope in frame.scopes.iter().rev() {
                for sym in scope.symbols.values() {
                    if self.cursor_identifier_set.insert(sym.name.clone()) {
                        self.possible_cursor_identifiers.push(CursorIdentifier {
                            name: sym.name.clone(),
                            identifier_type: match sym.function_info {
                                SymbolFunctionInfo::Lambda { .. } => CursorIdentifierType::Lambda,
                                SymbolFunctionInfo::Operator { .. } => {
                                    CursorIdentifierType::Operator
                                }
                                SymbolFunctionInfo::None => {
                                    // fall back to variable type if not a function
                                    match sym.var_type {
                                        VariableType::Let => CursorIdentifierType::Let,
                                        VariableType::Var => CursorIdentifierType::Var,
                                        VariableType::Mesh => CursorIdentifierType::Mesh,
                                        VariableType::Param => CursorIdentifierType::Param,
                                        VariableType::Reference => CursorIdentifierType::Var,
                                    }
                                }
                            },
                            stack_position: sym.stack_position,
                            frame_depth,
                        })
                    }
                }
            }
        }
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
            Statement::Break => {
                // in most of these, technically we might be including identifiers too early
                // in the event that there is a nested lambda definition of something similar
                // but this is just a small performance cost
                self.infer_possible_cursor_identifiers(span.clone());
                self.compile_break(span)
            }
            Statement::Continue => {
                self.infer_possible_cursor_identifiers(span.clone());
                self.compile_continue(span)
            }
            Statement::Return(r) => {
                self.infer_possible_cursor_identifiers(span.clone());
                self.compile_return(r, span)
            }
            Statement::While(w) => self.compile_while(w, span),
            Statement::For(f) => self.compile_for(f, span),
            Statement::If(i) => self.compile_if(i, span),
            Statement::Declaration(d) => {
                self.infer_possible_cursor_identifiers(span.clone());
                self.compile_declaration(d, span)
            }
            Statement::Expression(e) => {
                self.infer_possible_cursor_identifiers(span.clone());
                if expression_statement_has_no_effect(e) {
                    self.warning(span.clone(), "expression statement has no effect");
                }
                self.compile_val(e, span);
                self.emit_pops(1, span.clone());
            }
            Statement::Play(p) => {
                self.infer_possible_cursor_identifiers(span.clone());
                self.compile_play(p, span)
            }
        }
    }

    fn compile_declaration(&mut self, d: &Declaration, span: &Span8) {
        self.compile_val(&d.value.1, &d.value.0);
        let vt = match d.var_type {
            AstVariableType::Let => VariableType::Let,
            AstVariableType::Var => VariableType::Var,
            AstVariableType::Mesh => VariableType::Mesh,
            AstVariableType::Param => VariableType::Param,
        };
        let is_library = self.current_section().flags.is_library;
        match vt {
            VariableType::Param | VariableType::Mesh if is_library => {
                let kind = match vt {
                    VariableType::Param => "param",
                    VariableType::Mesh => "mesh",
                    _ => unreachable!(),
                };
                self.error(
                    span.clone(),
                    &format!("'{kind}' declarations are not allowed in user libraries"),
                );
            }
            VariableType::Param if self.frames.len() != 1 || self.frame().scopes.len() != 1 => {
                self.error(span.clone(), "'param' must be declared at the top level of a section, not inside any nested scope");
            }
            _ => {}
        }
        let name = &d.identifier.1.0;
        let existing_param = self.frame().scopes.iter().any(|s| {
            s.symbols
                .get(name)
                .is_some_and(|sym| sym.var_type == VariableType::Param)
        });
        if existing_param && vt != VariableType::Param {
            self.error(
                span.clone(),
                &format!("cannot shadow 'param' variable '{name}'"),
            );
        } else if vt == VariableType::Param {
            let shadows_existing = self
                .frame()
                .scopes
                .iter()
                .any(|s| s.symbols.contains_key(name));
            if shadows_existing {
                self.error(
                    span.clone(),
                    &format!("'param' variable '{name}' cannot shadow an existing variable"),
                );
            }
        }
        match vt {
            VariableType::Mesh => {
                let ni = self.intern_string(&d.identifier.1.0);
                self.emit(Instruction::ConvertMesh { name_index: ni }, span.clone());
            }
            VariableType::Param => {
                let ni = self.intern_string(&d.identifier.1.0);
                self.emit(Instruction::ConvertParam { name_index: ni }, span.clone());
            }
            VariableType::Let | VariableType::Var | VariableType::Reference => {
                self.emit(
                    Instruction::ConvertVar {
                        allow_stateful: false,
                    },
                    span.clone(),
                );
            }
        }
        self.define_symbol(&d.identifier.1.0, vt, SymbolFunctionInfo::from(&d.value.1));
    }

    fn compile_while(&mut self, w: &While, span: &Span8) {
        let loop_start = self.instruction_pointer();
        self.compile_val(&w.condition.1, &w.condition.0);
        self.emit(Instruction::Not, w.condition.0.clone());
        let exit_jump = self.emit_cond_jump_patch(w.condition.0.clone());

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

        self.emit_jump_to(loop_start, span.clone());

        let loop_end = self.instruction_pointer();
        self.patch_jump(exit_jump, loop_end);
        let ctx = self.frame_mut().loop_contexts.pop().unwrap();
        for patch in ctx.break_patches {
            self.patch_jump(patch, loop_end);
        }
    }

    fn compile_for(&mut self, f: &For, span: &Span8) {
        // desugars for v in container  ->  while idx < len(container)
        self.push_scope();
        let container_span = f.container.0.clone();

        self.compile_val(&f.container.1, &f.container.0);
        let iter_pos = self.stack_depth() - 1;
        // anonymous names (null byte) can't collide with user identifiers
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            container_span.clone(),
        );
        self.define_symbol("\x00iter", VariableType::Let, SymbolFunctionInfo::None);

        self.emit_push_int(0, span.clone());
        let idx_pos = self.stack_depth() - 1;
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            span.clone(),
        );
        self.define_symbol("\x00idx", VariableType::Var, SymbolFunctionInfo::None);

        let condition_ip = self.instruction_pointer();
        let loop_stack = self.stack_depth();

        // condition: idx < len(iter)
        let d = self.stack_delta(idx_pos);
        self.emit_copy(d, span.clone());
        let d = self.stack_delta(iter_pos);
        self.emit_copy(d, container_span.clone());

        let len_idx = registry().index_of("vector_len") as u16;
        self.emit(
            Instruction::NativeInvoke {
                index: len_idx,
                arg_count: 1,
            },
            container_span.clone(),
        );

        self.emit(Instruction::Lt, span.clone());
        self.dec_stack(1);

        self.emit(Instruction::Not, span.clone());
        let exit_jump = self.emit_cond_jump_patch(span.clone()); // depth = loop_stack

        self.frame_mut().loop_contexts.push(LoopContext {
            continue_target: None, // patched below after increment is emitted
            stack_depth_at_loop: loop_stack,
            break_patches: Vec::new(),
            continue_patches: Vec::new(),
        });

        // body scope with the for variable
        self.push_scope();
        let d = self.stack_delta(iter_pos);
        self.emit_copy(d, container_span.clone());
        let d = self.stack_delta(idx_pos);
        self.emit_copy(d, span.clone());
        self.emit(
            Instruction::Subscript { mutable: false },
            container_span.clone(),
        );
        self.dec_stack(1);
        self.define_symbol(&f.var_name.1.0, VariableType::Let, SymbolFunctionInfo::None);
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            span.clone(),
        );

        self.compile_statements(&f.body.1);
        self.pop_scope(span.clone()); // depth = loop_stack

        // increment point — patch all pending continues here
        let increment_ip = self.instruction_pointer();
        let cont_patches = std::mem::take(
            &mut self
                .frame_mut()
                .loop_contexts
                .last_mut()
                .unwrap()
                .continue_patches,
        );
        for patch in cont_patches {
            self.patch_jump(patch, increment_ip);
        }

        // idx = idx + 1
        let d = self.stack_delta(idx_pos);
        self.emit_lvalue(d, span.clone());
        let d = self.stack_delta(idx_pos);
        self.emit_copy(d, span.clone());
        self.emit_push_int(1, span.clone());
        self.emit(Instruction::Add, span.clone());
        self.dec_stack(1);
        self.emit(Instruction::Assign, span.clone());
        self.dec_stack(1);
        self.emit_pops(1, span.clone()); // discard assign result, depth = loop_stack

        self.emit_jump_to(condition_ip, span.clone());

        let loop_end = self.instruction_pointer();
        self.patch_jump(exit_jump, loop_end);
        let ctx = self.frame_mut().loop_contexts.pop().unwrap();
        for patch in ctx.break_patches {
            self.patch_jump(patch, loop_end);
        }

        self.pop_scope(span.clone()); // removes iter and idx
    }

    fn compile_if(&mut self, i: &If, span: &Span8) {
        self.compile_val(&i.condition.1, &i.condition.0);
        self.emit(Instruction::Not, i.condition.0.clone());
        let skip_if = self.emit_cond_jump_patch(i.condition.0.clone());

        self.push_scope();
        self.compile_statements(&i.if_block.1);
        self.pop_scope(span.clone());

        if let Some(ref else_block) = i.else_block {
            let skip_else = self.emit_jump_patch(span.clone());
            self.patch_jump(skip_if, self.instruction_pointer());
            self.push_scope();
            self.compile_statements(&else_block.1);
            self.pop_scope(span.clone());
            self.patch_jump(skip_else, self.instruction_pointer());
        } else {
            self.patch_jump(skip_if, self.instruction_pointer());
        }
    }

    fn compile_return(&mut self, r: &Return, span: &Span8) {
        if !matches!(self.frame().kind, FrameKind::Lambda | FrameKind::Block) {
            self.error(
                span.clone(),
                "return is only valid inside a lambda or block",
            );
        }

        if is_stateful(&r.value.1) {
            self.error(span.clone(), "cannot return a stateful value");
        }

        self.compile_val(&r.value.1, &r.value.0);
        let below = self.stack_depth() as i32 - 1;
        self.emit(
            Instruction::Return {
                stack_delta: -below,
            },
            span.clone(),
        );
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

        let patch_idx = self.emit_jump_patch(span.clone());
        self.frame_mut()
            .loop_contexts
            .last_mut()
            .unwrap()
            .break_patches
            .push(patch_idx);
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

        if let Some(to) = target {
            self.emit_jump_to(to, span.clone());
        } else {
            let patch_idx = self.emit_jump_patch(span.clone());
            self.frame_mut()
                .loop_contexts
                .last_mut()
                .unwrap()
                .continue_patches
                .push(patch_idx);
        }
    }

    fn compile_play(&mut self, p: &Play, span: &Span8) {
        if !matches!(self.frame().kind, FrameKind::Root | FrameKind::Anim) {
            self.error(
                span.clone(),
                "play is only valid directly inside a slide or anim block",
            );
            return;
        }
        self.compile_val(&p.animations.1, &p.animations.0);
        self.emit(Instruction::Play, span.clone());
        self.dec_stack(1);
    }
}

impl Compiler {
    fn compile_expr(
        &mut self,
        definitely_mutable: bool,
        inv_args: Option<&InvocationArguments>,
        expr: &Expression,
        span: &Span8,
    ) {
        if definitely_mutable
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
            Expression::IdentifierReference(i) => {
                self.compile_ident_ref(definitely_mutable, inv_args, i, span)
            }
            Expression::Subscript(s) => self.compile_subscript(definitely_mutable, s),
            Expression::Property(p) => self.compile_property(definitely_mutable, p),
            Expression::Literal(l) => self.compile_literal(definitely_mutable, l, span),
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

    fn compile_val(&mut self, expr: &Expression, span: &Span8) {
        self.compile_expr(false, None, expr, span);
    }
}

impl Compiler {
    fn compile_ident_ref(
        &mut self,
        mutable: bool,
        inv_args: Option<&InvocationArguments>,
        ir: &IdentifierReference,
        span: &Span8,
    ) {
        let name = ident_ref_name(ir);
        let Some(sym) = self.lookup(name, Some(span.clone()), inv_args) else {
            self.error(span.clone(), format!("undefined variable '{}'", name));
            let idx = self.intern_int(0);
            self.emit_push(Instruction::PushInt { index: idx }, span.clone());
            return;
        };
        let delta = self.stack_delta(sym.stack_position);

        match ir {
            IdentifierReference::Reference(_) => {
                if !matches!(
                    sym.var_type,
                    VariableType::Reference | VariableType::Param | VariableType::Mesh
                ) {
                    self.error(
                        span.clone(),
                        format!(
                            "cannot reference '{}' as it is not a mesh or param variable",
                            name
                        ),
                    );
                }
                // references should be copied to preserve the source reference
                match sym.var_type {
                    VariableType::Reference => self.emit_copy_ref(delta, span.clone()),
                    _ => self.emit_lvalue_ephemeral(delta, span.clone()),
                }
            }
            IdentifierReference::Value(_) if mutable => {
                if sym.var_type == VariableType::Let {
                    self.error(
                        span.clone(),
                        format!("cannot mutate '{}', consider declaring it as a 'var'", name),
                    );
                }
                match sym.var_type {
                    VariableType::Reference => self.emit_copy_ref(delta, span.clone()),
                    _ => self.emit_lvalue(delta, span.clone()),
                }
            }
            IdentifierReference::Value(_) => {
                if sym.var_type == VariableType::Param {
                    self.error(
                        span.clone(),
                        format!(
                            "cannot read param '{}' directly; use '${}' for a stateful reference or '*{}' for the current leader value",
                            name, name, name
                        ),
                    );
                    self.emit_push(
                        Instruction::PushDereference { stack_delta: delta },
                        span.clone(),
                    );
                } else {
                    self.emit_copy(delta, span.clone());
                }
            }
            IdentifierReference::StatefulReference(_) => {
                self.emit_push(
                    Instruction::PushStateful { stack_delta: delta },
                    span.clone(),
                );
            }
            IdentifierReference::StatefulDereference(_) => {
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
            Literal::Nil => {
                self.emit_push(Instruction::PushNil, span.clone());
            }
            Literal::Int(val) => {
                let idx = self.intern_int(*val);
                self.emit_push(Instruction::PushInt { index: idx }, span.clone());
            }
            Literal::Float(val) => {
                let idx = self.intern_float(*val);
                self.emit_push(Instruction::PushFloat { index: idx }, span.clone());
            }
            Literal::Imaginary(val) => {
                let idx = self.intern_float(*val);
                self.emit_push(Instruction::PushImaginary { index: idx }, span.clone());
            }
            Literal::String(s) => {
                let idx = self.intern_string(s);
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
            let idx = self.intern_float(component);
            self.emit_push(Instruction::PushFloat { index: idx }, span.clone());
            self.emit(Instruction::Append, span.clone());
            self.dec_stack(1); // append: pop 2 push 1
        }
    }

    fn compile_vector(&mut self, mutable: bool, elems: &[SpanTagged<Expression>], span: &Span8) {
        self.emit_push(Instruction::PushEmptyVector, span.clone());
        for elem in elems {
            self.compile_expr(mutable, None, &elem.1, &elem.0);
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
        if entries.is_empty() {
            return;
        }
        // needed to subscript lvalue wise
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            span.clone(),
        );
        let map_pos = self.stack_depth() - 1;
        for (key, val) in entries {
            let d = self.stack_delta(map_pos);
            self.emit_lvalue(d, span.clone());
            self.compile_val(&key.1, &key.0);
            self.emit(Instruction::Subscript { mutable: true }, span.clone());
            self.dec_stack(1);
            self.compile_val(&val.1, &val.0);
            self.emit(Instruction::Assign, span.clone());
            self.dec_stack(1);
            self.emit_pops(1, span.clone()); // discard assign result
        }
        self.emit(
            Instruction::PushCopy {
                pop_tos: true,
                stack_delta: -1,
                mutable: false,
            },
            span.clone(),
        );
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
        self.compile_val(&b.lhs.1, &b.lhs.0);
        self.compile_val(&b.rhs.1, &b.rhs.0);
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
        self.compile_expr(true, None, &b.lhs.1, &b.lhs.0);
        self.compile_val(&b.rhs.1, &b.rhs.0);
        self.emit(Instruction::Assign, span.clone());
        self.dec_stack(1);
    }

    fn compile_dot_assign(&mut self, b: &BinaryOperator, span: &Span8) {
        self.compile_expr(true, None, &b.lhs.1, &b.lhs.0);
        self.compile_val(&b.rhs.1, &b.rhs.0);
        self.emit(Instruction::AppendAssign, span.clone());
        self.dec_stack(1);
    }

    // `a && b`: short-circuit; result is 0 if a is falsy, else b
    fn compile_and(&mut self, b: &BinaryOperator, span: &Span8) {
        self.compile_val(&b.lhs.1, &b.lhs.0);
        let jump_rhs = self.emit_cond_jump_patch(span.clone());

        self.emit_push_int(0, span.clone());
        let jump_end = self.emit_jump_patch(span.clone());
        self.dec_stack(1); // undo push for tracking; merge point restores

        self.patch_jump(jump_rhs, self.instruction_pointer());
        self.compile_val(&b.rhs.1, &b.rhs.0);
        self.patch_jump(jump_end, self.instruction_pointer());
    }

    // `a || b`: short-circuit; result is 1 if a is truthy, else b
    fn compile_or(&mut self, b: &BinaryOperator, span: &Span8) {
        self.compile_val(&b.lhs.1, &b.lhs.0);
        let jump_true = self.emit_cond_jump_patch(span.clone());

        self.compile_val(&b.rhs.1, &b.rhs.0);
        let jump_end = self.emit_jump_patch(span.clone());
        self.dec_stack(1);

        self.patch_jump(jump_true, self.instruction_pointer());
        self.emit_push_int(1, span.clone());
        self.patch_jump(jump_end, self.instruction_pointer());
    }
}

impl Compiler {
    fn compile_unary(&mut self, u: &UnaryPreOperator) {
        self.compile_val(&u.operand.1, &u.operand.0);
        let instr = match u.op_type {
            UnaryOperatorType::Negative => Instruction::Negate,
            UnaryOperatorType::Not => Instruction::Not,
        };
        self.emit(instr, u.operand.0.clone());
    }

    fn compile_subscript(&mut self, mutable: bool, s: &Subscript) {
        self.compile_expr(mutable, None, &s.base.1, &s.base.0);
        self.compile_val(&s.index.1, &s.index.0);
        self.emit(Instruction::Subscript { mutable }, s.base.0.clone());
        self.dec_stack(1);
    }

    fn compile_property(&mut self, mutable: bool, p: &Property) {
        self.compile_expr(mutable, None, &p.base.1, &p.base.0);
        let attr = ident_ref_name(&p.attribute.1);
        let si = self.intern_string(attr);
        self.emit(
            Instruction::Attribute {
                mutable,
                string_index: si,
            },
            p.attribute.0.clone(),
        );
    }
}

impl Compiler {
    fn compile_lambda_invoke(&mut self, l: &LambdaInvocation, span: &Span8) {
        let labeled = l.arguments.1.iter().any(|(lbl, _)| lbl.is_some());
        let stateful =
            is_stateful(&l.lambda.1) || l.arguments.1.iter().any(|(_, a)| is_stateful(&a.1));
        let num_args = l.arguments.1.len() as u32;

        // doing arguments first is useful for stack
        // but it also guarantees deepest first ordering for references
        for (_, arg) in &l.arguments.1 {
            self.compile_val(&arg.1, &arg.0);
            self.emit(
                Instruction::ConvertVar {
                    allow_stateful: stateful,
                },
                arg.0.clone(),
            );
        }
        self.compile_expr(false, Some(&l.arguments), &l.lambda.1, &l.lambda.0);

        if labeled {
            for (lbl, _) in &l.arguments.1 {
                let si = match lbl {
                    Some((_, IdentifierDeclaration(name))) => self.intern_string(name),
                    None => u32::MAX,
                };
                self.emit(
                    Instruction::BufferLabelOrAttribute { string_index: si },
                    span.clone(),
                );
            }
        }
        self.emit(
            Instruction::LambdaInvoke {
                stateful,
                labeled,
                num_args,
            },
            span.clone(),
        );
        self.dec_stack(num_args as usize); // goes from +(args+1) to +1
    }

    fn compile_operator_invoke(&mut self, o: &OperatorInvocation, _span: &Span8) {
        let labeled = o.arguments.1.iter().any(|(lbl, _)| lbl.is_some());
        let stateful = is_stateful(&o.operator.1)
            || is_stateful(&o.operand.1)
            || o.arguments.1.iter().any(|(_, a)| is_stateful(&a.1));
        let num_args = o.arguments.1.len() as u32;

        // span covering just `operator{args}`, excluding the operand
        let invoke_span = o.operator.0.start..o.arguments.0.end;

        self.compile_val(&o.operand.1, &o.operand.0);
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: true,
            },
            o.operand.0.clone(),
        );
        for (_, arg) in &o.arguments.1 {
            self.compile_val(&arg.1, &arg.0);
            self.emit(
                Instruction::ConvertVar {
                    allow_stateful: true,
                },
                arg.0.clone(),
            );
        }
        self.compile_expr(false, Some(&o.arguments), &o.operator.1, &o.operator.0);

        if labeled {
            for (lbl, _) in &o.arguments.1 {
                let si = match lbl {
                    Some((_, IdentifierDeclaration(name))) => self.intern_string(name),
                    None => u32::MAX,
                };
                self.emit(
                    Instruction::BufferLabelOrAttribute { string_index: si },
                    invoke_span.clone(),
                );
            }
        }
        self.emit(
            Instruction::OperatorInvoke {
                stateful,
                labeled,
                num_args,
            },
            invoke_span.clone(),
        );
        self.dec_stack(num_args as usize + 1);
        // operator invocations leave an InvokedOperator on stack, so this is
        // effectively a no-op kept for bytecode compatibility.
        self.emit(Instruction::ConvertToLiveOperator, invoke_span);
    }

    fn compile_native_invoke(&mut self, n: &NativeInvocation, span: &Span8) {
        let name = ident_ref_name(&n.function.1);
        for arg in &n.arguments {
            self.compile_val(&arg.1, &arg.0);
        }
        let index = registry().index_of(&name) as u16;
        let arg_count = n.arguments.len() as u16;
        self.emit(Instruction::NativeInvoke { index, arg_count }, span.clone());
        self.dec_stack(n.arguments.len());
        self.inc_stack();
    }
}

impl Compiler {
    fn compile_lambda(&mut self, l: &LambdaDefinition, span: &Span8) {
        let captures = self.compute_lambda_captures(l);

        let mut saw_default = false;
        for arg in &l.args {
            if arg.default_value.is_some() {
                saw_default = true;
            } else if saw_default {
                self.error(
                    span.clone(),
                    "required arguments must come before default arguments",
                );
                break;
            }
        }

        for cap in &captures {
            if cap.var_type != VariableType::Let {
                self.error(span.clone(), format!("cannot capture variable '{}' as it is mutable but lambas must be pure functions. Please change the variable type to 'let' ", cap.name));
            }
        }

        let (jump_idx, body_ip) = self.begin_closure_frame(FrameKind::Lambda, span);

        let mut required_args: u32 = 0;
        let mut default_count: u32 = 0;
        for (i, arg) in l.args.iter().enumerate() {
            let vt = if arg.must_be_reference {
                VariableType::Reference
            } else {
                VariableType::Let
            };
            self.register_symbol(&arg.identifier.1.0, vt, SymbolFunctionInfo::None, i);
            if arg.default_value.is_some() {
                default_count += 1;
            } else {
                required_args += 1;
            }
        }
        self.frame_mut().stack_depth = l.args.len();

        self.register_capture_symbols(&captures);

        match &l.body.1 {
            LambdaBody::Inline(expr) => {
                if is_stateful(expr) {
                    self.error(
                        span.clone(),
                        "cannot return a stateful value from an inline lambda",
                    );
                }

                self.compile_val(expr, &l.body.0);
                let below = self.stack_depth() as i32 - 1;
                self.emit(
                    Instruction::Return {
                        stack_delta: -below,
                    },
                    l.body.0.clone(),
                );
            }
            LambdaBody::Block(stmts) => {
                self.compile_statements(stmts);
                self.emit(
                    Instruction::NativeInvoke {
                        index: registry().index_of("lambda_fallthrough_error") as u16,
                        arg_count: 0,
                    },
                    span.clone(),
                );
            }
        }

        self.end_closure_frame(jump_idx);

        self.compile_captures(&captures, false, span);

        for arg in &l.args {
            if let Some(ref default) = arg.default_value {
                let lvalue_refs = free_lvalue_refs_expr(&default.1, HashSet::new());
                for name in &lvalue_refs {
                    self.error(
                        default.0.clone(),
                        format!(
                            "default value for '{}' cannot take a lvalue reference to '{}'",
                            arg.identifier.1.0, name
                        ),
                    );
                }

                self.compile_val(&default.1, &default.0);
            }
        }

        let proto_idx = self.current_section().lambda_prototypes.len() as u32;
        let section = self.section_index();
        self.current_section_mut()
            .lambda_prototypes
            .push(LambdaPrototype {
                section,
                ip: body_ip,
                required_args,
                default_arg_count: default_count,
            });

        let cap16 = captures.len() as u16;
        self.emit(
            Instruction::MakeLambda {
                prototype_index: proto_idx,
                capture_count: cap16,
            },
            span.clone(),
        );
        self.dec_stack(captures.len() + default_count as usize);
        self.inc_stack();
    }

    fn compile_operator_def(&mut self, o: &OperatorDefinition, span: &Span8) {
        self.compile_val(&o.lambda.1, &o.lambda.0);
        self.emit(Instruction::MakeOperator, span.clone());
    }

    fn compile_block(&mut self, b: &Block, span: &Span8) {
        let captures = self.compute_block_captures(&b.body);

        let (jump_idx, body_ip) = self.begin_closure_frame(FrameKind::Block, span);
        self.register_capture_symbols(&captures);
        self.compile_block_body(&b.body, span);
        self.end_closure_frame(jump_idx);

        self.compile_captures(&captures, true, span);

        let proto_idx = self.current_section().lambda_prototypes.len() as u32;
        let section = self.section_index();
        self.current_section_mut()
            .lambda_prototypes
            .push(LambdaPrototype {
                section,
                ip: body_ip,
                required_args: 0,
                default_arg_count: 0,
            });

        let cap16 = captures.len() as u16;
        self.emit(
            Instruction::MakeLambda {
                prototype_index: proto_idx,
                capture_count: cap16,
            },
            span.clone(),
        );
        self.dec_stack(captures.len());
        self.inc_stack();

        self.emit(
            Instruction::LambdaInvoke {
                stateful: false,
                labeled: false,
                num_args: 0,
            },
            span.clone(),
        );
    }

    fn compile_anim(&mut self, a: &Anim, span: &Span8) {
        let captures = self.compute_block_captures(&a.body);

        let (jump_idx, body_ip) = self.begin_closure_frame(FrameKind::Anim, span);
        self.register_capture_symbols(&captures);

        {
            self.push_scope();
            self.compile_statements(&a.body);
            self.pop_scope(span.clone());
            self.emit(Instruction::EndOfExecutionHead, span.clone());
        }
        self.end_closure_frame(jump_idx);

        self.compile_captures(&captures, false, span);

        let proto_idx = self.current_section().anim_prototypes.len() as u32;
        let section = self.section_index();
        self.current_section_mut()
            .anim_prototypes
            .push(AnimPrototype {
                section,
                ip: body_ip,
            });

        let cap16 = captures.len() as u16;
        self.emit(
            Instruction::MakeAnim {
                prototype_index: proto_idx,
                capture_count: cap16,
            },
            span.clone(),
        );
        self.dec_stack(captures.len());
        self.inc_stack();
    }

    fn begin_closure_frame(&mut self, kind: FrameKind, span: &Span8) -> (usize, u32) {
        let jump_idx = self.emit_jump_patch(span.clone());
        let body_ip = self.instruction_pointer();
        self.push_frame(kind);
        (jump_idx, body_ip)
    }

    fn register_capture_symbols(&mut self, captures: &[Arc<Symbol>]) {
        for (i, cap) in captures.iter().enumerate() {
            self.register_symbol(
                &cap.name,
                cap.var_type,
                cap.function_info.clone(),
                self.frame().stack_depth + i,
            );
        }
        self.frame_mut().stack_depth += captures.len();
    }

    fn end_closure_frame(&mut self, jump_idx: usize) {
        self.pop_frame();
        self.patch_jump(jump_idx, self.instruction_pointer());
    }

    fn compile_captures(
        &mut self,
        captures: &[Arc<Symbol>],
        immediately_invoked: bool,
        span: &Span8,
    ) {
        for cap in captures {
            let stack_delta = self.stack_delta(cap.stack_position);
            if !immediately_invoked
                && matches!(cap.var_type, VariableType::Reference | VariableType::Let)
            {
                // optimize out the ephemeral
                self.emit_copy_ref(stack_delta, span.clone());
                self.emit(
                    Instruction::ConvertVar {
                        allow_stateful: false,
                    },
                    span.clone(),
                );
            } else if immediately_invoked {
                self.emit_lvalue(stack_delta, span.clone());
            } else {
                self.emit_lvalue_ephemeral(stack_delta, span.clone());
            }
        }
    }

    // compile a block body: init `_ = []`, compile stmts, implicit `return _`
    fn compile_block_body(&mut self, stmts: &[SpanTagged<Statement>], span: &Span8) {
        self.emit_push(Instruction::PushEmptyVector, span.clone());
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            span.clone(),
        );
        self.define_symbol("_", VariableType::Var, SymbolFunctionInfo::None);
        self.compile_statements(stmts);
        let underscore_pos = self.lookup("_", None, None).unwrap().stack_position;
        let d = self.stack_delta(underscore_pos);
        self.emit_copy(d, span.clone());
        let below = self.stack_depth() as i32 - 1;
        self.emit(
            Instruction::Return {
                stack_delta: -below,
            },
            span.clone(),
        );
    }

    fn resolve_captures(&mut self, free: &[String]) -> Vec<Arc<Symbol>> {
        free.iter()
            .filter_map(|name| self.lookup(name, None, None))
            .collect()
    }

    fn compute_lambda_captures(&mut self, l: &LambdaDefinition) -> Vec<Arc<Symbol>> {
        let pre: HashSet<String> = l.args.iter().map(|a| a.identifier.1.0.clone()).collect();
        let free = match &l.body.1 {
            LambdaBody::Inline(e) => free_vars_expr(e, pre),
            LambdaBody::Block(s) => free_vars_stmts(s, pre),
        };
        self.resolve_captures(&free)
    }

    fn compute_block_captures(&mut self, stmts: &[SpanTagged<Statement>]) -> Vec<Arc<Symbol>> {
        let free = free_vars_stmts(stmts, HashSet::from(["_".to_string()]));
        self.resolve_captures(&free)
    }
}

#[cfg(test)]
mod test {
    use std::{path::PathBuf, sync::Arc};

    use bytecode::Instruction;
    use lexer::lexer::Lexer;
    use lexer::token::Token;
    use parser::ast::{
        BinaryOperator, BinaryOperatorType, Declaration, Expression, IdentifierDeclaration,
        IdentifierReference, LambdaArg, LambdaBody, LambdaDefinition, Literal, Section,
        SectionBundle, SectionType, Statement, VariableType,
    };
    use structs::rope::Rope;
    use structs::text::Span8;

    use crate::cache::CompilerCache;

    use super::{CompileResult, FunctionArgInfo, SymbolFunctionInfo, compile};

    fn empty_span() -> Span8 {
        0..0
    }

    fn s<T>(v: T) -> (Span8, T) {
        (empty_span(), v)
    }

    fn sb<T>(v: T) -> (Span8, Box<T>) {
        (empty_span(), Box::new(v))
    }

    fn make_bundle(
        stmts: Vec<(Span8, Statement)>,
        section_type: SectionType,
    ) -> Arc<SectionBundle> {
        Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section {
                body: stmts,
                section_type,
            }],
            root_import_span: None,
            was_cached: false,
        })
    }

    fn test_compile(sections: &[Arc<SectionBundle>]) -> CompileResult {
        compile(&mut CompilerCache::default(), None, sections)
    }

    fn compile_stmts(stmts: Vec<(Span8, Statement)>) -> CompileResult {
        test_compile(&[make_bundle(stmts, SectionType::Slide)])
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

    fn has_warning(result: &CompileResult, fragment: &str) -> bool {
        result.warnings.iter().any(|w| w.message.contains(fragment))
    }

    fn no_errors(result: &CompileResult) {
        if !result.errors.is_empty() {
            let msgs: Vec<_> = result.errors.iter().map(|e| e.message.as_str()).collect();
            panic!("expected no errors, got: {:?}", msgs);
        }
    }

    #[test]
    fn test_function_info_emits_reference_and_default_metadata_for_hints() {
        let result = compile_src(
            "
            let a = 1
            let b = 2
            let c = 3
            let f = |&x, y = 1, &z = c| x
            f(a, b, c)
        ",
        );
        no_errors(&result);

        let reference = result
            .root_references
            .iter()
            .find(|reference| reference.symbol.name == "f")
            .expect("expected root reference to f");

        match &reference.symbol.function_info {
            SymbolFunctionInfo::Lambda { args } => {
                assert_eq!(
                    args,
                    &vec![
                        FunctionArgInfo {
                            name: "x".to_string(),
                            has_default: false,
                            is_reference: true,
                        },
                        FunctionArgInfo {
                            name: "y".to_string(),
                            has_default: true,
                            is_reference: false,
                        },
                        FunctionArgInfo {
                            name: "z".to_string(),
                            has_default: true,
                            is_reference: true,
                        },
                    ]
                );
            }
            other => panic!("expected lambda function info, got {other:?}"),
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
        // sections[0] is the prelude; user code is in sections[1]
        let section = &result.bytecode.sections[1];
        assert!(
            section
                .instructions
                .iter()
                .any(|i| matches!(i, Instruction::PushInt { .. }))
        );
    }

    #[test]
    fn test_nil_decl_emits_push_nil() {
        let stmts = vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("x".into())),
            value: s(Expression::Literal(Literal::Nil)),
        }))];
        let result = compile_stmts(stmts);
        no_errors(&result);
        let section = &result.bytecode.sections[1];
        assert!(
            section
                .instructions
                .iter()
                .any(|i| matches!(i, Instruction::PushNil))
        );
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
            s(Statement::Expression(Expression::BinaryOperator(
                BinaryOperator {
                    op_type: BinaryOperatorType::Assign,
                    lhs: sb(Expression::IdentifierReference(IdentifierReference::Value(
                        "x".into(),
                    ))),
                    rhs: sb(Expression::Literal(Literal::Int(2))),
                },
            ))),
        ];
        let result = compile_stmts(stmts);
        assert!(
            has_error(&result, "cannot mutate 'x'"),
            "expected let mutation error"
        );
    }

    #[test]
    fn test_undefined_variable_error() {
        let stmts = vec![s(Statement::Expression(Expression::IdentifierReference(
            IdentifierReference::Value("notdefined".into()),
        )))];
        let result = compile_stmts(stmts);
        assert!(
            has_error(&result, "undefined variable"),
            "expected undefined variable error"
        );
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
                    body: s(LambdaBody::Inline(Box::new(
                        Expression::IdentifierReference(IdentifierReference::Value("x".into())),
                    ))),
                })),
            })),
        ];
        no_errors(&compile_stmts(stmts));
    }

    #[test]
    fn test_default_args_must_be_suffix() {
        // |a = 1, b| b  — required arg after default arg → error
        let stmts = vec![s(Statement::Expression(Expression::LambdaDefinition(
            LambdaDefinition {
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
                body: s(LambdaBody::Inline(Box::new(
                    Expression::IdentifierReference(IdentifierReference::Value("b".into())),
                ))),
            },
        )))];
        let result = compile_stmts(stmts);
        assert!(
            has_error(
                &result,
                "required arguments must come before default arguments"
            ),
            "expected default-arg suffix error"
        );
    }

    #[test]
    fn test_default_args_valid_suffix() {
        // |a, b = 1| a  — correct ordering, no error
        let stmts = vec![s(Statement::Expression(Expression::LambdaDefinition(
            LambdaDefinition {
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
                body: s(LambdaBody::Inline(Box::new(
                    Expression::IdentifierReference(IdentifierReference::Value("a".into())),
                ))),
            },
        )))];
        no_errors(&compile_stmts(stmts));
    }

    // -- cross-bundle import tests --

    #[test]
    fn test_cross_bundle_symbol_visible() {
        // bundle 0 defines `x`; bundle 1 imports it and reads it — no error
        let bundle0 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
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
            root_import_span: None,
            was_cached: false,
        });
        let bundle1 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 1,
            imported_files: vec![0],
            sections: vec![Section {
                body: vec![s(Statement::Expression(Expression::IdentifierReference(
                    IdentifierReference::Value("x".into()),
                )))],
                section_type: SectionType::Slide,
            }],
            root_import_span: None,
            was_cached: false,
        });
        no_errors(&test_compile(&[bundle0, bundle1]));
    }

    #[test]
    fn test_cross_bundle_symbol_is_let() {
        // bundle 0 defines `var x`; bundle 1 imports it and tries to assign — error
        let bundle0 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
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
            root_import_span: None,
            was_cached: false,
        });
        let bundle1 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 1,
            imported_files: vec![0],
            sections: vec![Section {
                body: vec![s(Statement::Expression(Expression::BinaryOperator(
                    BinaryOperator {
                        op_type: BinaryOperatorType::Assign,
                        lhs: sb(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".into(),
                        ))),
                        rhs: sb(Expression::Literal(Literal::Int(1))),
                    },
                )))],
                section_type: SectionType::Slide,
            }],
            root_import_span: None,
            was_cached: false,
        });
        let result = test_compile(&[bundle0, bundle1]);
        assert!(
            has_error(&result, "cannot mutate 'x'"),
            "imported var should become let"
        );
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
        let has_cj = result.bytecode.sections.iter().any(|sec| {
            sec.instructions
                .iter()
                .any(|i| matches!(i, Instruction::ConditionalJump { .. }))
        });
        assert!(has_cj, "expected ConditionalJump in bytecode for 'and'");
    }

    #[test]
    fn test_integration_short_circuit_or() {
        // Monocurl uses keyword `or` not `||`
        let result = compile_src("let z = 0 or 1");
        no_errors(&result);
        let has_cj = result.bytecode.sections.iter().any(|sec| {
            sec.instructions
                .iter()
                .any(|i| matches!(i, Instruction::ConditionalJump { .. }))
        });
        assert!(has_cj, "expected ConditionalJump in bytecode for 'or'");
    }

    #[test]
    fn test_integration_param_requires_explicit_read() {
        let result = compile_src("param x = 1\nlet y = x");
        assert!(
            has_error(&result, "cannot read param 'x' directly"),
            "expected compile error for naked param read"
        );
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
            has_error(
                &result,
                "required arguments must come before default arguments"
            ),
            "expected default-arg suffix error from parser-produced AST"
        );
    }

    #[test]
    fn test_warns_for_useless_expression_statement() {
        let result = compile_src("1 + 2");
        assert!(
            has_warning(&result, "expression statement has no effect"),
            "expected useless-expression warning"
        );
    }

    #[test]
    fn test_no_warning_for_expression_statement_with_assignment() {
        let result = compile_src("var x = 0\nx = 1");
        assert!(
            !has_warning(&result, "expression statement has no effect"),
            "did not expect useless-expression warning"
        );
    }

    #[test]
    fn test_no_warning_for_expression_statement_with_lvalue_reference() {
        let result = compile_src("let poke = |slot| slot\npoke(&camera)");
        assert!(
            !has_warning(&result, "expression statement has no effect"),
            "did not expect useless-expression warning"
        );
    }

    // -- bytecode sequence tests --

    // `let x = 42` — PushInt evaluates the value, PushVar creates the variable slot.
    #[test]
    fn test_bytecode_single_let_int() {
        let result = compile_stmts(vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("x".into())),
            value: s(Expression::Literal(Literal::Int(42))),
        }))]);
        no_errors(&result);
        let sec = &result.bytecode.sections[1];
        assert_eq!(
            sec.instructions,
            vec![
                Instruction::PushInt { index: 0 },
                Instruction::ConvertVar {
                    allow_stateful: false
                },
                Instruction::EndOfExecutionHead
            ],
        );
        assert_eq!(sec.int_pool, vec![42i64]);
        assert!(sec.lambda_prototypes.is_empty());
    }

    #[test]
    fn test_bytecode_single_let_nil() {
        let result = compile_stmts(vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("x".into())),
            value: s(Expression::Literal(Literal::Nil)),
        }))]);
        no_errors(&result);
        let sec = &result.bytecode.sections[1];
        assert_eq!(
            sec.instructions,
            vec![
                Instruction::PushNil,
                Instruction::ConvertVar {
                    allow_stateful: false
                },
                Instruction::EndOfExecutionHead
            ],
        );
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
            s(Statement::Expression(Expression::BinaryOperator(
                BinaryOperator {
                    op_type: BinaryOperatorType::Assign,
                    lhs: sb(Expression::IdentifierReference(IdentifierReference::Value(
                        "x".into(),
                    ))),
                    rhs: sb(Expression::Literal(Literal::Int(1))),
                },
            ))),
        ]);
        no_errors(&result);
        let sec = &result.bytecode.sections[1];
        assert_eq!(
            sec.instructions,
            vec![
                Instruction::PushInt { index: 0 }, // var x = 0
                Instruction::ConvertVar {
                    allow_stateful: false
                }, // create variable slot
                Instruction::PushLvalue {
                    stack_delta: -1,
                    force_ephemeral: false
                }, // lvalue of x (at pos 0, depth 1)
                Instruction::PushInt { index: 1 }, // rhs = 1
                Instruction::Assign,
                Instruction::Pop { count: 1 }, // discard assign result
                Instruction::EndOfExecutionHead,
            ],
        );
        assert_eq!(sec.int_pool, vec![0i64, 1i64]);
    }

    // `let f = |a| a` — verifies lambda body is Jump-over + body + MakeLambda
    // and that the prototype table is populated correctly.
    #[test]
    fn test_bytecode_simple_lambda() {
        let result = compile_stmts(vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("f".into())),
            value: s(Expression::LambdaDefinition(LambdaDefinition {
                args: vec![LambdaArg {
                    identifier: s(IdentifierDeclaration("a".into())),
                    default_value: None,
                    must_be_reference: false,
                }],
                body: s(LambdaBody::Inline(Box::new(
                    Expression::IdentifierReference(IdentifierReference::Value("a".into())),
                ))),
            })),
        }))]);
        no_errors(&result);
        let sec = &result.bytecode.sections[1];
        // [0] Jump{to:3}  [1] PushCopy{-1}  [2] Return{-1}  [3] MakeLambda{proto:0,cap:0}
        // [4] PushVar  [5] EndOfExecutionHead
        assert!(matches!(
            sec.instructions[0],
            Instruction::Jump { to: 3, .. }
        ));
        assert_eq!(
            sec.instructions[1],
            Instruction::PushCopy {
                stack_delta: -1,
                pop_tos: false,
                mutable: false
            }
        );
        assert_eq!(sec.instructions[2], Instruction::Return { stack_delta: -1 });
        assert_eq!(
            sec.instructions[3],
            Instruction::MakeLambda {
                prototype_index: 0,
                capture_count: 0
            },
        );
        assert_eq!(
            sec.instructions[4],
            Instruction::ConvertVar {
                allow_stateful: false
            }
        );
        assert_eq!(sec.instructions[5], Instruction::EndOfExecutionHead);
        assert_eq!(sec.lambda_prototypes.len(), 1);
        assert_eq!(sec.lambda_prototypes[0].required_args, 1);
        assert_eq!(sec.lambda_prototypes[0].default_arg_count, 0);
        assert_eq!(sec.lambda_prototypes[0].ip, 1);
    }
}
