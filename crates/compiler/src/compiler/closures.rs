use super::*;

impl Compiler {
    pub(super) fn compile_lambda(&mut self, l: &LambdaDefinition, span: &Span8) {
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
        let mut reference_args = Vec::with_capacity(l.args.len());
        for (i, arg) in l.args.iter().enumerate() {
            let vt = if arg.must_be_reference {
                VariableType::Reference
            } else {
                VariableType::Let
            };
            reference_args.push(arg.must_be_reference);
            self.register_symbol(
                &arg.identifier.1.0,
                vt,
                SymbolFunctionInfo::None,
                i,
                !arg.must_be_reference,
            );
            if arg.default_value.is_some() {
                default_count += 1;
            } else {
                required_args += 1;
            }
        }
        self.frame_mut().stack_depth = l.args.len();

        self.register_capture_symbols(&captures, true);

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

        self.compile_captures(&captures, false, true, span);

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
                reference_args,
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

    pub(super) fn compile_operator_def(&mut self, o: &OperatorDefinition, span: &Span8) {
        self.compile_val(&o.lambda.1, &o.lambda.0);
        self.emit(Instruction::MakeOperator, span.clone());
    }

    pub(super) fn compile_block(&mut self, b: &Block, span: &Span8) {
        let captures = self.compute_block_captures(&b.body);

        let (jump_idx, body_ip) = self.begin_closure_frame(FrameKind::Block, span);
        self.register_capture_symbols(&captures, true);
        self.compile_block_body(&b.body, span);
        self.end_closure_frame(jump_idx);

        self.compile_captures(&captures, true, false, span);

        let proto_idx = self.current_section().lambda_prototypes.len() as u32;
        let section = self.section_index();
        self.current_section_mut()
            .lambda_prototypes
            .push(LambdaPrototype {
                section,
                ip: body_ip,
                required_args: 0,
                default_arg_count: 0,
                reference_args: Vec::new(),
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

    pub(super) fn compile_anim(&mut self, a: &Anim, span: &Span8) {
        let captures = self.compute_block_captures(&a.body);

        let (jump_idx, body_ip) = self.begin_closure_frame(FrameKind::Anim, span);
        self.register_capture_symbols(&captures, true);

        {
            self.push_scope();
            self.compile_statements(&a.body);
            self.pop_scope(span.clone());
            self.emit(Instruction::EndOfExecutionHead, span.clone());
        }
        self.end_closure_frame(jump_idx);

        self.compile_captures(&captures, false, false, span);

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

    pub(super) fn begin_closure_frame(&mut self, kind: FrameKind, span: &Span8) -> (usize, u32) {
        let jump_idx = self.emit_jump_patch(span.clone());
        let body_ip = self.instruction_pointer();
        self.push_frame(kind);
        (jump_idx, body_ip)
    }

    pub(super) fn register_capture_symbols(
        &mut self,
        captures: &[Arc<Symbol>],
        preserve_let_captures_on_copy: bool,
    ) {
        for (i, cap) in captures.iter().enumerate() {
            self.register_symbol(
                &cap.name,
                cap.var_type,
                cap.function_info.clone(),
                self.frame().stack_depth + i,
                preserve_let_captures_on_copy && cap.var_type == VariableType::Let,
            );
        }
        self.frame_mut().stack_depth += captures.len();
    }

    pub(super) fn end_closure_frame(&mut self, jump_idx: usize) {
        self.pop_frame();
        self.patch_jump(jump_idx, self.instruction_pointer());
    }

    pub(super) fn compile_captures(
        &mut self,
        captures: &[Arc<Symbol>],
        immediately_invoked: bool,
        _capture_let_by_value: bool,
        span: &Span8,
    ) {
        for cap in captures {
            if cap.var_type == VariableType::Let {
                self.emit_symbol_copy(cap, span.clone());
            } else if !immediately_invoked && cap.var_type == VariableType::Reference {
                let stack_delta = self.stack_delta(cap.stack_position);
                self.emit_copy_ref(stack_delta, span.clone());
                self.emit(
                    Instruction::ConvertVar {
                        allow_stateful: false,
                    },
                    span.clone(),
                );
            } else if immediately_invoked {
                let stack_delta = self.stack_delta(cap.stack_position);
                self.emit_lvalue(stack_delta, span.clone());
            } else {
                let stack_delta = self.stack_delta(cap.stack_position);
                self.emit_lvalue_ephemeral(stack_delta, span.clone());
            }
        }
    }

    // compile a block body: init `_ = []`, compile stmts, implicit `return _`
    pub(super) fn compile_block_body(&mut self, stmts: &[SpanTagged<Statement>], span: &Span8) {
        self.emit_push(Instruction::PushEmptyVector, span.clone());
        self.emit(
            Instruction::ConvertVar {
                allow_stateful: false,
            },
            span.clone(),
        );
        self.define_symbol("_", VariableType::Var, SymbolFunctionInfo::None, false);
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

    pub(super) fn resolve_captures(&mut self, free: &[String]) -> Vec<Arc<Symbol>> {
        free.iter()
            .filter_map(|name| self.lookup(name, None, None))
            .collect()
    }

    pub(super) fn compute_lambda_captures(&mut self, l: &LambdaDefinition) -> Vec<Arc<Symbol>> {
        let pre: HashSet<String> = l.args.iter().map(|a| a.identifier.1.0.clone()).collect();
        let free = match &l.body.1 {
            LambdaBody::Inline(e) => free_vars_expr(e, pre),
            LambdaBody::Block(s) => free_vars_stmts(s, pre),
        };
        self.resolve_captures(&free)
    }

    pub(super) fn compute_block_captures(
        &mut self,
        stmts: &[SpanTagged<Statement>],
    ) -> Vec<Arc<Symbol>> {
        let free = free_vars_stmts(stmts, HashSet::from(["_".to_string()]));
        self.resolve_captures(&free)
    }
}
