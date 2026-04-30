use super::*;

impl Compiler {
    pub(super) fn compile_statements(&mut self, stmts: &[SpanTagged<Statement>]) {
        for (span, stmt) in stmts {
            self.compile_statement(stmt, span);
        }
    }

    pub(super) fn compile_statement(&mut self, stmt: &Statement, span: &Span8) {
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
            Statement::Print(p) => {
                self.infer_possible_cursor_identifiers(span.clone());
                self.compile_print(p, span)
            }
        }
    }

    pub(super) fn compile_declaration(&mut self, d: &Declaration, span: &Span8) {
        self.compile_val(&d.value.1, &d.value.0);
        let vt = match d.var_type {
            AstVariableType::Let => VariableType::Let,
            AstVariableType::Var => VariableType::Var,
            AstVariableType::Mesh => VariableType::Mesh,
            AstVariableType::Param => VariableType::Param,
        };
        let is_library = self.current_section().flags.is_library;
        match vt {
            VariableType::Param if is_library => {
                self.error(
                    span.clone(),
                    "'param' declarations are not allowed in user libraries",
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
        self.define_declared_symbol(&d.identifier.1.0, vt, &d.value.1, false);
    }

    pub(super) fn compile_while(&mut self, w: &While, span: &Span8) {
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

    pub(super) fn compile_for(&mut self, f: &For, span: &Span8) {
        if let Some([start, stop]) = self.stdlib_range_bounds(&f.container.1) {
            self.compile_for_stdlib_range(f, &start, &stop, span);
        } else {
            self.compile_generic_for(f, span);
        }
    }

    fn compile_generic_for(&mut self, f: &For, span: &Span8) {
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
        self.define_symbol(
            "\x00iter",
            VariableType::Let,
            SymbolFunctionInfo::None,
            false,
        );

        self.emit_push_int(0, span.clone());
        let idx_pos = self.stack_depth() - 1;
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            span.clone(),
        );
        self.define_symbol(
            "\x00idx",
            VariableType::Var,
            SymbolFunctionInfo::None,
            false,
        );

        let condition_ip = self.instruction_pointer();
        let loop_stack = self.stack_depth();

        // condition: idx < len(iter)
        let d = self.stack_delta(idx_pos);
        self.emit_copy(d, span.clone());
        let d = self.stack_delta(iter_pos);
        self.emit_copy(d, container_span.clone());

        let len_idx = registry().index_of("list_len") as u16;
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
        self.define_symbol(
            &f.var_name.1.0,
            VariableType::Let,
            SymbolFunctionInfo::None,
            false,
        );
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

        // idx += 1
        let d = self.stack_delta(idx_pos);
        self.emit(Instruction::IncrementByOne { stack_delta: d }, span.clone());

        self.emit_jump_to(condition_ip, span.clone());

        let loop_end = self.instruction_pointer();
        self.patch_jump(exit_jump, loop_end);
        let ctx = self.frame_mut().loop_contexts.pop().unwrap();
        for patch in ctx.break_patches {
            self.patch_jump(patch, loop_end);
        }

        self.pop_scope(span.clone()); // removes iter and idx
    }

    fn compile_for_stdlib_range(
        &mut self,
        f: &For,
        start: &SpanTagged<Expression>,
        stop: &SpanTagged<Expression>,
        span: &Span8,
    ) {
        // desugars for i in range(a, b) -> current = a; stop = b; while current < stop { let i = current; ...; current = current + 1 }
        self.push_scope();

        self.compile_val(&start.1, &start.0);
        let current_pos = self.stack_depth() - 1;
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            start.0.clone(),
        );
        self.define_symbol(
            "\x00range_current",
            VariableType::Var,
            SymbolFunctionInfo::None,
            false,
        );

        self.compile_val(&stop.1, &stop.0);
        let stop_pos = self.stack_depth() - 1;
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            stop.0.clone(),
        );
        self.define_symbol(
            "\x00range_stop",
            VariableType::Let,
            SymbolFunctionInfo::None,
            false,
        );

        let condition_ip = self.instruction_pointer();
        let loop_stack = self.stack_depth();

        let d = self.stack_delta(current_pos);
        self.emit_copy(d, start.0.clone());
        let d = self.stack_delta(stop_pos);
        self.emit_copy(d, stop.0.clone());
        self.emit(Instruction::Lt, span.clone());
        self.dec_stack(1);

        self.emit(Instruction::Not, span.clone());
        let exit_jump = self.emit_cond_jump_patch(span.clone());

        self.frame_mut().loop_contexts.push(LoopContext {
            continue_target: None,
            stack_depth_at_loop: loop_stack,
            break_patches: Vec::new(),
            continue_patches: Vec::new(),
        });

        self.push_scope();
        let d = self.stack_delta(current_pos);
        self.emit_copy(d, start.0.clone());
        self.define_symbol(
            &f.var_name.1.0,
            VariableType::Let,
            SymbolFunctionInfo::None,
            false,
        );
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            span.clone(),
        );

        self.compile_statements(&f.body.1);
        self.pop_scope(span.clone());

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

        let d = self.stack_delta(current_pos);
        self.emit(Instruction::IncrementByOne { stack_delta: d }, span.clone());

        self.emit_jump_to(condition_ip, span.clone());

        let loop_end = self.instruction_pointer();
        self.patch_jump(exit_jump, loop_end);
        let ctx = self.frame_mut().loop_contexts.pop().unwrap();
        for patch in ctx.break_patches {
            self.patch_jump(patch, loop_end);
        }

        self.pop_scope(span.clone());
    }

    fn stdlib_range_bounds(
        &mut self,
        container: &Expression,
    ) -> Option<[SpanTagged<Expression>; 2]> {
        let Expression::LambdaInvocation(inv) = container else {
            return None;
        };
        let Expression::IdentifierReference(IdentifierReference::Value(name)) = &*inv.lambda.1
        else {
            return None;
        };
        if name != "range" {
            return None;
        }

        let sym = self.lookup(name, None, None)?;
        if !sym.declared_in_stdlib {
            return None;
        }

        match inv.arguments.1.as_slice() {
            [(None, start), (None, stop)] => Some([start.clone(), stop.clone()]),
            _ => None,
        }
    }

    pub(super) fn compile_if(&mut self, i: &If, span: &Span8) {
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

    pub(super) fn compile_return(&mut self, r: &Return, span: &Span8) {
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

    pub(super) fn compile_break(&mut self, span: &Span8) {
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

    pub(super) fn compile_continue(&mut self, span: &Span8) {
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

    pub(super) fn compile_print(&mut self, p: &Print, span: &Span8) {
        self.compile_val(&p.value.1, &p.value.0);
        self.emit(Instruction::Observe, span.clone());
        self.dec_stack(1);
    }

    pub(super) fn compile_play(&mut self, p: &Play, span: &Span8) {
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
