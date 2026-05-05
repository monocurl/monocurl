mod closures;
mod cursor;
mod expressions;
mod free_vars;
mod statements;
#[cfg(test)]
mod tests;
mod warnings;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use bytecode::{
    AnimPrototype, Bytecode, CopyValueMode, Instruction, InstructionAnnotation, LambdaPrototype,
    SectionBytecode, SectionFlags,
};
use free_vars::{free_lvalue_refs_expr, free_vars_expr, free_vars_stmts};
use parser::ast::{
    Anim, BinaryOperator, BinaryOperatorType, Block, Declaration, DirectionalLiteral, Expression,
    For, IdentifierDeclaration, IdentifierReference, If, InvocationArguments, LambdaArg,
    LambdaBody, LambdaDefinition, LambdaInvocation, Literal, NativeInvocation, OperatorDefinition,
    OperatorInvocation, Play, Print, Property, Return, Section, SectionBundle, SectionType,
    SpanTagged, Statement, Subscript, UnaryOperatorType, UnaryPreOperator,
    VariableType as AstVariableType, While,
};
use stdlib::registry::registry;
use structs::{
    rope::{Attribute, RLEData, Rope},
    text::{Count8, Span8},
};
use ui_cli_shared::static_analysis::StaticAnalysisData;
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
    Mesh,
    Scene,
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
    declared_in_stdlib: bool,
    stack_position: usize,
    preserve_lvalues_on_copy: bool,
    special_function: Option<SpecialFunction>,
    // how the symbol looks like to the current bundle
    // may appear as let, even though declared as var
    pub var_type: VariableType,
    pub function_info: SymbolFunctionInfo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpecialFunction {
    RootFrameRandom,
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
        IdentifierReference::Value(n) | IdentifierReference::Reference(n) => n,
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
    deferred_expr_depth: usize,
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
            deferred_expr_depth: 0,
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
        let can_use_cache = allowing_caches
            && bundle.was_cached
            // skip prelude
            && ind + 1 < compiler_cache.last_bundles.len()
            && compiler_cache.last_bundles[ind + 1].path.as_ref() == Some(&bundle.file_path);

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

pub fn static_analysis_rope(
    compile: &CompileResult,
    text_len: Count8,
) -> Rope<Attribute<StaticAnalysisData>> {
    let mut rope = Rope::default();

    if text_len == 0 {
        return rope;
    }

    rope = rope.replace_range(
        0..0,
        std::iter::once(RLEData {
            codeunits: text_len,
            attribute: StaticAnalysisData::None,
        }),
    );

    for reference in &compile.root_references {
        let analysis = match (&reference.symbol.function_info, &reference.invocation_spans) {
            (SymbolFunctionInfo::Lambda { .. }, Some(_)) => StaticAnalysisData::FunctionInvocation,
            (SymbolFunctionInfo::Operator { .. }, Some(_)) => {
                StaticAnalysisData::OperatorInvocation
            }
            _ => continue,
        };

        rope = rope.replace_range(
            reference.span.clone(),
            std::iter::once(RLEData {
                codeunits: reference.span.len(),
                attribute: analysis,
            }),
        );
    }

    rope
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
            self.emit(Instruction::ConvertScene { name_index }, 0..0);
            self.define_symbol(var, VariableType::Scene, SymbolFunctionInfo::None, false);
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
            path: Some(bundle.file_path.clone()),
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

        if bundle
            .sections
            .first()
            .is_some_and(|section| bundle.is_root() && section.section_type == SectionType::Slide)
        {
            self.compile_section(&Section {
                body: Vec::new(),
                section_type: SectionType::Init,
                name: None,
            });
        }

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
        bytecode.name = section.name.clone();
        bytecode.source_file_path = current_bundle.path.clone();
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
                copy_mode: CopyValueMode::Read,
                stack_delta,
                pop_tos: false,
            },
            span,
        );
    }

    fn emit_raw_copy(&mut self, stack_delta: i32, span: Span8) {
        self.emit_push(
            Instruction::PushCopy {
                copy_mode: CopyValueMode::Raw,
                stack_delta,
                pop_tos: false,
            },
            span,
        );
    }

    fn emit_copy_ref(&mut self, stack_delta: i32, span: Span8) {
        self.emit_push(
            Instruction::PushCopy {
                copy_mode: CopyValueMode::Reference,
                stack_delta,
                pop_tos: false,
            },
            span,
        );
    }

    fn emit_symbol_copy(&mut self, symbol: &Symbol, span: Span8) {
        let stack_delta = self.stack_delta(symbol.stack_position);
        if symbol.preserve_lvalues_on_copy {
            self.emit_raw_copy(stack_delta, span);
        } else {
            self.emit_copy(stack_delta, span);
        }
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
        preserve_lvalues_on_copy: bool,
    ) {
        let position = self.stack_depth() - 1;
        let declared_in_stdlib = self.current_section().flags.is_stdlib;
        self.frame_mut().scopes.last_mut().unwrap().symbols.insert(
            name.to_string(),
            Arc::new(Symbol {
                name: name.to_string(),
                declared_in_stdlib,
                stack_position: position,
                preserve_lvalues_on_copy,
                special_function: None,
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
        preserve_lvalues_on_copy: bool,
    ) {
        let declared_in_stdlib = self.current_section().flags.is_stdlib;
        self.frame_mut().scopes.last_mut().unwrap().symbols.insert(
            name.to_string(),
            Arc::new(Symbol {
                name: name.to_string(),
                declared_in_stdlib,
                stack_position: position,
                preserve_lvalues_on_copy,
                special_function: None,
                var_type,
                function_info,
                imported: false,
            }),
        );
    }

    fn register_symbol_like(
        &mut self,
        symbol: &Symbol,
        position: usize,
        preserve_lvalues_on_copy: bool,
    ) {
        self.frame_mut().scopes.last_mut().unwrap().symbols.insert(
            symbol.name.clone(),
            Arc::new(Symbol {
                name: symbol.name.clone(),
                imported: false,
                declared_in_stdlib: symbol.declared_in_stdlib,
                stack_position: position,
                preserve_lvalues_on_copy,
                special_function: symbol.special_function,
                var_type: symbol.var_type,
                function_info: symbol.function_info.clone(),
            }),
        );
    }

    fn define_declared_symbol(
        &mut self,
        name: &str,
        var_type: VariableType,
        value: &Expression,
        preserve_lvalues_on_copy: bool,
    ) {
        let position = self.stack_depth() - 1;
        let declared_in_stdlib = self.current_section().flags.is_stdlib;
        let special_function =
            self.special_function_for_expr(value)
                .or_else(|| match (declared_in_stdlib, name) {
                    (true, "random" | "randint") => Some(SpecialFunction::RootFrameRandom),
                    _ => None,
                });

        self.frame_mut().scopes.last_mut().unwrap().symbols.insert(
            name.to_string(),
            Arc::new(Symbol {
                name: name.to_string(),
                imported: false,
                declared_in_stdlib,
                stack_position: position,
                preserve_lvalues_on_copy,
                special_function,
                var_type,
                function_info: SymbolFunctionInfo::from(value),
            }),
        );
    }

    fn special_function_for_expr(&mut self, expr: &Expression) -> Option<SpecialFunction> {
        match expr {
            Expression::IdentifierReference(ir) => self
                .lookup(ident_ref_name(ir), None, None)
                .and_then(|symbol| symbol.special_function),
            _ => None,
        }
    }

    fn root_frame_special_calls_allowed(&self) -> bool {
        self.frame().kind == FrameKind::Root && self.deferred_expr_depth == 0
    }

    fn with_deferred_expr_context(&mut self, f: impl FnOnce(&mut Self)) {
        self.deferred_expr_depth += 1;
        f(self);
        self.deferred_expr_depth -= 1;
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
