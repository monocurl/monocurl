use super::*;

impl Compiler {
    pub(super) fn compile_expr(
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
                    | Expression::Literal(Literal::List(_))
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

    pub(super) fn compile_val(&mut self, expr: &Expression, span: &Span8) {
        self.compile_expr(false, None, expr, span);
    }
}

impl Compiler {
    pub(super) fn compile_ident_ref(
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
                    // only reachable once an error has already been reported --
                    // a writable unboxed variable is lowered by compile_assign,
                    // and everything else the scan keeps in a slot. emit a read
                    // rather than a reference so the bytecode stays well formed
                    _ if sym.unboxed => self.emit_symbol_copy(&sym, span.clone()),
                    _ => self.emit_lvalue(delta, span.clone()),
                }
            }
            IdentifierReference::Value(_) => {
                if sym.var_type == VariableType::Reference {
                    self.emit_push(
                        Instruction::PushDeepCopy { stack_delta: delta },
                        span.clone(),
                    );
                } else {
                    self.emit_symbol_copy(&sym, span.clone());
                }
            }
            IdentifierReference::StatefulReference(_) => {
                self.emit_push(
                    Instruction::PushStateful { stack_delta: delta },
                    span.clone(),
                );
            }
        }
    }
}

impl Compiler {
    pub(super) fn compile_literal(&mut self, mutable: bool, l: &Literal, span: &Span8) {
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
            Literal::List(elems) => self.compile_list(mutable, elems, span),
            Literal::Map(entries) => self.compile_map(entries, span),
        }
    }

    pub(super) fn compile_directional(&mut self, d: &DirectionalLiteral, span: &Span8) {
        let (x, y, z) = match d {
            DirectionalLiteral::Left(m) => (-m, 0.0, 0.0),
            DirectionalLiteral::Right(m) => (*m, 0.0, 0.0),
            DirectionalLiteral::Up(m) => (0.0, *m, 0.0),
            DirectionalLiteral::Down(m) => (0.0, -m, 0.0),
            DirectionalLiteral::Forward(m) => (0.0, 0.0, -m),
            DirectionalLiteral::Backward(m) => (0.0, 0.0, *m),
        };
        self.emit_push(Instruction::PushEmptyList, span.clone());
        for component in [x, y, z] {
            let idx = self.intern_float(component);
            self.emit_push(Instruction::PushFloat { index: idx }, span.clone());
            self.emit(Instruction::Append, span.clone());
            self.dec_stack(1); // append: pop 2 push 1
        }
    }

    pub(super) fn compile_list(
        &mut self,
        mutable: bool,
        elems: &[SpanTagged<Expression>],
        span: &Span8,
    ) {
        self.emit_push(Instruction::PushEmptyList, span.clone());
        for elem in elems {
            self.compile_expr(mutable, None, &elem.1, &elem.0);
            self.emit(Instruction::Append, span.clone());
            self.dec_stack(1);
        }
    }

    pub(super) fn compile_map(
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
                copy_mode: CopyValueMode::Read,
                pop_tos: true,
                stack_delta: -1,
            },
            span.clone(),
        );
    }
}

impl Compiler {
    pub(super) fn compile_binary(&mut self, b: &BinaryOperator, span: &Span8) {
        match b.op_type {
            BinaryOperatorType::And => self.compile_and(b, span),
            BinaryOperatorType::Or => self.compile_or(b, span),
            BinaryOperatorType::Assign => self.compile_assign(b, span),
            BinaryOperatorType::DotAssign => self.compile_dot_assign(b, span),
            _ => self.compile_simple_binary(b, span),
        }
    }

    pub(super) fn compile_simple_binary(&mut self, b: &BinaryOperator, span: &Span8) {
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

    pub(super) fn compile_assign(&mut self, b: &BinaryOperator, span: &Span8) {
        // writing a whole unboxed variable needs no reference to it: evaluate the
        // value and store straight into its stack entry. the value stays on the
        // stack, which is the shape Assign leaves behind
        if let Some(position) = self.unboxed_assign_target(&b.lhs.1, &b.lhs.0) {
            self.compile_val(&b.rhs.1, &b.rhs.0);
            let stack_delta = self.stack_delta(position);
            self.emit(Instruction::StoreLocal { stack_delta }, span.clone());
            return;
        }

        self.compile_expr(true, None, &b.lhs.1, &b.lhs.0);
        self.compile_val(&b.rhs.1, &b.rhs.0);
        self.emit(Instruction::Assign, span.clone());
        self.dec_stack(1);
    }

    /// the stack position of a bare identifier that names an unboxed variable and
    /// may be written, if this target is one
    fn unboxed_assign_target(&mut self, lhs: &Expression, span: &Span8) -> Option<usize> {
        let Expression::IdentifierReference(ir) = lhs else {
            return None;
        };
        if !matches!(ir, IdentifierReference::Value(_)) {
            return None;
        }

        let name = ident_ref_name(ir);
        let symbol = self.lookup(name, None, None)?;
        // a `let` is reported as immutable by the general path, which this must
        // not skip past
        if !symbol.unboxed || symbol.var_type != VariableType::Var {
            return None;
        }

        // the general path would have recorded this read for the editor's
        // cross-references; taking the fast path must not lose it
        self.lookup(name, Some(span.clone()), None);
        Some(symbol.stack_position)
    }

    pub(super) fn compile_dot_assign(&mut self, b: &BinaryOperator, span: &Span8) {
        self.compile_expr(true, None, &b.lhs.1, &b.lhs.0);
        self.compile_val(&b.rhs.1, &b.rhs.0);
        self.emit(Instruction::AppendAssign, span.clone());
        self.dec_stack(1);
    }

    // `a && b`: short-circuit; result is 0 if a is falsy, else b
    pub(super) fn compile_and(&mut self, b: &BinaryOperator, span: &Span8) {
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
    pub(super) fn compile_or(&mut self, b: &BinaryOperator, span: &Span8) {
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
    pub(super) fn compile_unary(&mut self, u: &UnaryPreOperator) {
        self.compile_val(&u.operand.1, &u.operand.0);
        let instr = match u.op_type {
            UnaryOperatorType::Negative => Instruction::Negate,
            UnaryOperatorType::Not => Instruction::Not,
        };
        self.emit(instr, u.operand.0.clone());
    }

    pub(super) fn compile_subscript(&mut self, mutable: bool, s: &Subscript) {
        if !mutable && self.compile_local_subscript(s) {
            return;
        }

        self.compile_expr(mutable, None, &s.base.1, &s.base.0);
        self.compile_val(&s.index.1, &s.index.0);
        self.emit(Instruction::Subscript { mutable }, s.base.0.clone());
        self.dec_stack(1);
    }

    /// reads `local[i][j]...` in place, so indexing never copies the container.
    /// the indices are evaluated before the container is read, which is only
    /// unobservable when they cannot write to it
    fn compile_local_subscript(&mut self, s: &Subscript) -> bool {
        let mut indices = vec![&s.index];
        let mut base = &s.base;
        while let Expression::Subscript(inner) = &*base.1 {
            indices.push(&inner.index);
            base = &inner.base;
        }

        let Ok(depth) = u16::try_from(indices.len()) else {
            return false;
        };
        if !indices.iter().all(|index| cannot_write_locals(&index.1)) {
            return false;
        }
        let Some(symbol) = self.in_place_local(&base.1, &base.0) else {
            return false;
        };

        for index in indices.iter().rev() {
            self.compile_val(&index.1, &index.0);
        }
        let stack_delta = self.stack_delta(symbol.stack_position);
        self.emit_push(
            Instruction::SubscriptLocal {
                stack_delta,
                depth,
                copy_mode: symbol.copy_mode(),
            },
            s.base.0.clone(),
        );
        self.dec_stack(depth.into());
        true
    }

    /// the symbol a bare identifier names, when it is a `let` or `var` that can be
    /// read in place instead of copied out first
    pub(super) fn in_place_local(&mut self, expr: &Expression, span: &Span8) -> Option<Arc<Symbol>> {
        let Expression::IdentifierReference(ir @ IdentifierReference::Value(_)) = expr else {
            return None;
        };
        let name = ident_ref_name(ir);
        let symbol = self.lookup(name, None, None)?;
        // a mesh or param can be written through a reference handed to a call,
        // and a reference variable is read by deep copy
        if !matches!(symbol.var_type, VariableType::Let | VariableType::Var) {
            return None;
        }
        // the general path would have recorded this read for the editor's
        // cross-references
        self.lookup(name, Some(span.clone()), None);
        Some(symbol)
    }

    pub(super) fn compile_property(&mut self, mutable: bool, p: &Property) {
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
    fn reference_argument_message() -> &'static str {
        "reference arguments must be explicit &param, &mesh, &reference values, or list literals of references"
    }

    fn is_reference_argument_literal(expr: &Expression) -> bool {
        match expr {
            Expression::IdentifierReference(IdentifierReference::Reference(_)) => true,
            Expression::Literal(Literal::List(elements)) => elements
                .iter()
                .all(|(_, element)| Self::is_reference_argument_literal(element)),
            _ => false,
        }
    }

    fn lambda_arg_info_for_expr(&mut self, expr: &Expression) -> Option<Vec<FunctionArgInfo>> {
        match expr {
            Expression::LambdaDefinition(lambda) => Some(
                lambda
                    .args
                    .iter()
                    .map(SymbolFunctionInfo::arg_info)
                    .collect(),
            ),
            Expression::IdentifierReference(ir) => {
                let name = ident_ref_name(ir);
                let symbol = self.lookup(name, None, None)?;
                match &symbol.function_info {
                    SymbolFunctionInfo::Lambda { args } => Some(args.clone()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn operator_arg_info_for_expr(&mut self, expr: &Expression) -> Option<Vec<FunctionArgInfo>> {
        match expr {
            Expression::OperationDefinition(operator) => match &*operator.lambda.1 {
                Expression::LambdaDefinition(lambda) => Some(
                    lambda
                        .args
                        .iter()
                        .map(SymbolFunctionInfo::arg_info)
                        .collect(),
                ),
                _ => None,
            },
            Expression::IdentifierReference(ir) => {
                let name = ident_ref_name(ir);
                let symbol = self.lookup(name, None, None)?;
                match &symbol.function_info {
                    SymbolFunctionInfo::Operator { args } => Some(args.clone()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn invocation_arg_index(
        args: &[FunctionArgInfo],
        arg_position: usize,
        label: Option<&SpanTagged<IdentifierDeclaration>>,
    ) -> Option<usize> {
        label
            .and_then(|(_, IdentifierDeclaration(label))| {
                args.iter().position(|arg| arg.name == *label)
            })
            .or(Some(arg_position))
    }

    fn validate_reference_argument_syntax(
        &mut self,
        args: &[FunctionArgInfo],
        arg_position: usize,
        label: Option<&SpanTagged<IdentifierDeclaration>>,
        value: &SpanTagged<Expression>,
    ) {
        let Some(arg_idx) = Self::invocation_arg_index(args, arg_position, label) else {
            return;
        };
        if args.get(arg_idx).is_some_and(|arg| arg.is_reference)
            && !Self::is_reference_argument_literal(&value.1)
        {
            self.error(value.0.clone(), Self::reference_argument_message());
        }
    }

    pub(super) fn compile_lambda_invoke(&mut self, l: &LambdaInvocation, span: &Span8) {
        if matches!(
            self.special_function_for_expr(&l.lambda.1),
            Some(SpecialFunction::RootFrameRandom)
        ) && !self.root_frame_special_calls_allowed()
        {
            self.error(
                span.clone(),
                "randomness may only be called from the root frame",
            );
        }

        let labeled = l.arguments.1.iter().any(|(lbl, _)| lbl.is_some());
        let stateful =
            is_stateful(&l.lambda.1) || l.arguments.1.iter().any(|(_, a)| is_stateful(&a.1));
        let num_args = l.arguments.1.len() as u32;

        if let Some(args) = self.lambda_arg_info_for_expr(&l.lambda.1) {
            for (arg_position, (label, arg)) in l.arguments.1.iter().enumerate() {
                self.validate_reference_argument_syntax(&args, arg_position, label.as_ref(), arg);
            }
        }

        // a plain call of a stdlib wrapper such as cos(x) is lowered straight to
        // its native, skipping the lambda value and the call frame
        if !labeled
            && !stateful
            && let Some(alias) = self.native_alias_for_expr(&l.lambda.1)
            && u32::from(alias.arg_count) == num_args
        {
            // `len(local)` reads the length in place rather than copying the
            // container into the native's argument
            if usize::from(alias.index) == registry().index_of("len")
                && let [(_, arg)] = l.arguments.1.as_slice()
                && let Some(symbol) = self.in_place_local(&arg.1, &arg.0)
            {
                let stack_delta = self.stack_delta(symbol.stack_position);
                self.emit_push(
                    Instruction::LenLocal {
                        stack_delta,
                        copy_mode: symbol.copy_mode(),
                    },
                    span.clone(),
                );
                return;
            }

            for (_, arg) in &l.arguments.1 {
                self.compile_val(&arg.1, &arg.0);
            }
            self.emit(
                Instruction::NativeInvoke {
                    index: alias.index,
                    arg_count: alias.arg_count,
                },
                span.clone(),
            );
            self.dec_stack(num_args as usize);
            self.inc_stack();
            return;
        }

        // doing arguments first is useful for stack
        // but it also guarantees deepest first ordering for references
        for (_, arg) in &l.arguments.1 {
            self.compile_val(&arg.1, &arg.0);
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

    pub(super) fn compile_operator_invoke(&mut self, o: &OperatorInvocation, _span: &Span8) {
        let labeled = o.arguments.1.iter().any(|(lbl, _)| lbl.is_some());
        let stateful = is_stateful(&o.operator.1)
            || is_stateful(&o.operand.1)
            || o.arguments.1.iter().any(|(_, a)| is_stateful(&a.1));
        let num_args = o.arguments.1.len() as u32;

        // span covering just `operator{args}`, excluding the operand
        let invoke_span = o.operator.0.start..o.arguments.0.end;

        if let Some(args) = self.operator_arg_info_for_expr(&o.operator.1) {
            if args.first().is_some_and(|arg| arg.is_reference)
                && !Self::is_reference_argument_literal(&o.operand.1)
            {
                self.error(o.operand.0.clone(), Self::reference_argument_message());
            }
            for (arg_position, (label, arg)) in o.arguments.1.iter().enumerate() {
                self.validate_reference_argument_syntax(
                    &args,
                    arg_position + 1,
                    label.as_ref(),
                    arg,
                );
            }
        }

        self.compile_val(&o.operand.1, &o.operand.0);
        for (_, arg) in &o.arguments.1 {
            self.compile_val(&arg.1, &arg.0);
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

    pub(super) fn compile_native_invoke(&mut self, n: &NativeInvocation, span: &Span8) {
        let name = ident_ref_name(&n.function.1);
        for arg in &n.arguments {
            self.compile_val(&arg.1, &arg.0);
        }
        let index = registry().index_of(name) as u16;
        let arg_count = n.arguments.len() as u16;
        self.emit(Instruction::NativeInvoke { index, arg_count }, span.clone());
        self.dec_stack(n.arguments.len());
        self.inc_stack();
    }
}

/// whether evaluating `expr` certainly leaves the current frame's `let`s and
/// `var`s alone: only assignments and blocks write to them, since calls cannot
/// capture a `var` and can only be handed references to meshes and params
fn cannot_write_locals(expr: &Expression) -> bool {
    match expr {
        Expression::IdentifierReference(_) => true,
        Expression::Literal(Literal::List(items)) => {
            none_write_locals(items.iter().map(|item| &item.1))
        }
        Expression::Literal(Literal::Map(entries)) => none_write_locals(
            entries
                .iter()
                .flat_map(|(key, value)| [&key.1, &value.1]),
        ),
        Expression::Literal(_) => true,
        Expression::BinaryOperator(b) => {
            !matches!(
                b.op_type,
                BinaryOperatorType::Assign | BinaryOperatorType::DotAssign
            ) && none_write_locals([&*b.lhs.1, &*b.rhs.1])
        }
        Expression::UnaryPreOperator(u) => cannot_write_locals(&u.operand.1),
        Expression::Subscript(s) => none_write_locals([&*s.base.1, &*s.index.1]),
        Expression::Property(p) => cannot_write_locals(&p.base.1),
        Expression::LambdaInvocation(l) => none_write_locals(
            std::iter::once(&*l.lambda.1).chain(l.arguments.1.iter().map(|arg| &arg.1.1)),
        ),
        Expression::OperatorInvocation(o) => none_write_locals(
            [&*o.operator.1, &*o.operand.1]
                .into_iter()
                .chain(o.arguments.1.iter().map(|arg| &arg.1.1)),
        ),
        Expression::NativeInvocation(n) => none_write_locals(n.arguments.iter().map(|arg| &arg.1)),
        Expression::LambdaDefinition(_)
        | Expression::OperationDefinition(_)
        | Expression::Block(_)
        | Expression::Anim(_) => false,
    }
}

fn none_write_locals<'a>(exprs: impl IntoIterator<Item = &'a Expression>) -> bool {
    exprs.into_iter().all(cannot_write_locals)
}
