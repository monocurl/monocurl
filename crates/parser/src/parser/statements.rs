use super::*;

impl SectionParser {
    pub fn parse_section(&mut self) -> Section {
        let body = self.parse_statement_list();
        Section {
            body,
            section_type: self.state.section_type,
        }
    }

    // best effort parse
    pub fn parse_statement_list(&mut self) -> Vec<SpanTagged<Statement>> {
        let mut statements = Vec::new();
        loop {
            // skip newlines and semicolons
            while matches!(
                self.peek_token(),
                Some((Token::Newline, _)) | Some((Token::Semicolon, _))
            ) {
                self.advance_token();
            }

            if self.peek_token().is_none() {
                break;
            }

            let before = self.token_index;

            let mut read = || -> Result<SpanTagged<Statement>, ()> {
                let ret = self.parse_statement()?;

                // ensure no hanging content
                // when parsing an if, we might actually go through new lines early, so if the most recently consumed token is a newline, no need for this check
                if self.peek_token().is_some()
                    && (self.token_index == 0
                        || !matches!(
                            self.tokens[self.token_index - 1].0,
                            Token::Newline | Token::Semicolon
                        ))
                {
                    try_all!(self, {
                        ExactPred(Token::Newline) => {
                            self.advance_token();
                        },
                        ExactPred(Token::Semicolon) => {
                            self.advance_token();
                        },
                    })?;
                }

                Ok(ret)
            };

            match read() {
                Ok(statement) => statements.push(statement),
                Err(_e) => {
                    // gracefully handle errors
                    while self.peek_token().is_some()
                        && !matches!(
                            self.peek_token().unwrap(),
                            (Token::Newline, _) | (Token::Semicolon, _)
                        )
                    {
                        self.advance_token();
                    }
                }
            };

            // guard against infinite loop: if nothing was consumed, force-advance past the
            // unrecognized token to prevent spinning on keywords in the wrong context
            if self.token_index == before {
                self.advance_token();
            }
        }

        statements
    }

    pub fn parse_statement(&mut self) -> Result<SpanTagged<Statement>, ()> {
        try_all!(self, {
            vtype = VariableDeclarationPred => {
                let (span, statement) = self.parse_declaration(vtype);
                (span, Statement::Declaration(statement))
            },
            ExactPred(Token::For) => {
                let (span, statement) = self.parse_for();
                (span, Statement::For(statement))
            },
            ExactPred(Token::While) => {
                let (span, statement) = self.parse_while();
                (span, Statement::While(statement))
            },
            ExactPred(Token::If) => {
                let (span, statement) = self.parse_if();
                (span, Statement::If(statement))
            },
            if PlayablePredicate, ExactPred(Token::Play) => {
                let (span, statement) = self.parse_play();
                (span, Statement::Play(statement))
            },
            if InLoopPredicate, ExactPred(Token::Break) => {
                let span = self.advance_token();
                (span, Statement::Break)
            },
            if InLoopPredicate, ExactPred(Token::Continue) => {
                let span = self.advance_token();
                (span, Statement::Continue)
            },
            if InLambdaOrBlockPredicate, ExactPred(Token::Return) => {
                let (span, statement) = self.parse_return();
                (span, Statement::Return(statement))
            },
            if InLambdaOrBlockPredicate, ExactPred(Token::Dot) => {
                let base_span = self.advance_token();
                let (span, expr) = self.parse_expr_best_effort();
                let desugared_expression = Statement::Expression(
                    Expression::BinaryOperator(
                        BinaryOperator {
                            lhs: (base_span.clone(), Box::new(Expression::IdentifierReference(
                                IdentifierReference::Value("_".into())
                            ))),
                            op_type: BinaryOperatorType::DotAssign,
                            rhs: (span.clone(), Box::new(expr))
                        }
                    )
                );
                (base_span.start..span.end, desugared_expression)
            },
            else {
                // otherwise must be expression, do not advance
                let (span, statement) = self.parse_expr_best_effort();
                Ok((span, Statement::Expression(statement)))
            }
        })
    }

    pub(super) fn parse_while(&mut self) -> SpanTagged<While> {
        self.debug_assert_token_eq(Token::While);
        let base_span = self.advance_token();
        self.read_token(Token::LParen).ok();
        let condition = self.parse_expr_best_effort();

        self.read_token(Token::RParen).ok();

        let (terminal, body) = self.parse_body(|frame| {
            frame.in_loop = true;
        });

        (
            base_span.start..terminal.end,
            While {
                condition,
                body: (terminal, body),
            },
        )
    }

    pub(super) fn parse_for(&mut self) -> SpanTagged<For> {
        self.debug_assert_token_eq(Token::For);
        let base_span = self.advance_token();

        self.read_token_best_effort(Token::LParen);
        let identifier = self.parse_identifier_declaration();
        self.read_token_best_effort(Token::In);

        let container = self.parse_expr_best_effort();
        self.read_token_best_effort(Token::RParen);

        let (terminal, body) = self.parse_body(|frame| {
            frame.in_loop = true;
        });

        (
            base_span.start..terminal.end,
            For {
                var_name: identifier,
                body: (terminal, body),
                container,
            },
        )
    }

    pub(super) fn parse_if(&mut self) -> SpanTagged<If> {
        self.debug_assert_token_eq(Token::If);
        let base_span = self.advance_token();
        self.read_token(Token::LParen).ok();
        let condition = self.parse_expr_best_effort();
        self.read_token(Token::RParen).ok();

        let body = self.parse_body(|_| {});
        self.advance_newlines();
        if self.read_if_token(Token::Else).is_some() {
            // parse else block
            let else_block = try_all!(self, {
                ExactPred(Token::If) => {
                    // else if
                    let (span, statement) = self.parse_if();
                    (span.clone(), vec![(span, Statement::If(statement))])
                },
                else {
                    Result::<_, ()>::Ok(self.parse_body(|_| {}))
                }
            })
            .unwrap();

            (
                base_span.start..else_block.0.end,
                If {
                    condition,
                    if_block: body,
                    else_block: Some(else_block),
                },
            )
        } else {
            (
                base_span.start..body.0.end,
                If {
                    condition,
                    if_block: body,
                    else_block: None,
                },
            )
        }
    }

    pub(super) fn parse_play(&mut self) -> SpanTagged<Play> {
        self.debug_assert_token_eq(Token::Play);
        let base_span = self.advance_token();
        let animations = self.parse_expr_best_effort();
        (base_span.start..animations.0.end, Play { animations })
    }

    pub(super) fn parse_declaration(&mut self, var_type: VariableType) -> SpanTagged<Declaration> {
        let base_span = self.advance_token();
        let identifier = self.parse_identifier_declaration();
        self.read_token(Token::Assign).ok();
        let value = self.parse_expr_best_effort();

        (
            base_span.start..value.0.end,
            Declaration {
                var_type,
                identifier,
                value,
            },
        )
    }

    pub(super) fn parse_return(&mut self) -> SpanTagged<Return> {
        self.debug_assert_token_eq(Token::Return);
        let base_span = self.advance_token();
        let value = self.parse_expr_best_effort();
        (base_span.start..value.0.end, Return { value })
    }

    // best effort
    pub(super) fn parse_identifier_declaration(&mut self) -> SpanTagged<IdentifierDeclaration> {
        let span = self.read_token_best_effort(Token::Identifier);
        let identifier: String = self.text_rope.iterator_range(span.clone()).collect();
        if !identifier.chars().all(|c| c.is_alphanumeric() || c == '_')
            || identifier.chars().next().is_none_or(|c| c.is_numeric())
        {
            self.emit_error(
                "Illegal Identifier".into(),
                format!("\"{}\" is not a valid identifier name", identifier),
                span.clone(),
            );
        }

        (span, IdentifierDeclaration(identifier))
    }

    pub(super) fn parse_body(
        &mut self,
        frame_builder: impl FnOnce(&mut ContextFrame),
    ) -> SpanTagged<Vec<SpanTagged<Statement>>> {
        // continue until we have a lflower
        while self.peek_token().is_some_and(|tok| tok.0 != Token::LFlower) {
            self.advance_token();
        }

        let body_range = self.precomputation.bracket_internal_range(self.token_index);
        let base_span = self.read_token_best_effort(Token::LFlower);

        self.state.push_frame(|frame| {
            frame.operating_range = body_range;
            frame_builder(frame)
        });
        let result = self.parse_statement_list();
        self.state.pop_frame();

        let terminal = self.read_token_best_effort(Token::RFlower);

        (base_span.start..terminal.end, result)
    }

    pub(super) fn parse_expr_best_effort(&mut self) -> SpanTagged<Expression> {
        self.parse_expr_priority(0)
    }
}

// expression parsing
