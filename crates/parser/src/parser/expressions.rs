use super::*;

impl SectionParser {
    pub(super) fn parse_expr_priority(&mut self, priority: usize) -> SpanTagged<Expression> {
        self.parse_expr_priority_with_leading_newlines(priority, true)
    }

    pub(super) fn parse_expr_priority_without_leading_newlines(
        &mut self,
        priority: usize,
    ) -> SpanTagged<Expression> {
        self.parse_expr_priority_with_leading_newlines(priority, false)
    }

    fn parse_expr_priority_with_leading_newlines(
        &mut self,
        priority: usize,
        allow_leading_newlines: bool,
    ) -> SpanTagged<Expression> {
        let mut expr = Some(self.parse_unary_with_leading_newlines(allow_leading_newlines));
        loop {
            let mut finished = false;
            try_all!(self, {
                op = BinaryOperatorPred => {
                    if (op.priority(), op.associativity()) >= (priority, 1) {
                        // combine right
                        self.advance_token();
                        let right = self.parse_expr_priority(op.priority());
                        let old_expr = expr.take().unwrap();
                        expr = Some((old_expr.0.start .. right.0.end, Expression::BinaryOperator(BinaryOperator {
                            lhs: (old_expr.0.clone(), Box::new(old_expr.1)),
                            op_type: op,
                            rhs: (right.0, Box::new(right.1))
                        })))
                    }
                    else {
                        finished = true;
                    }
                },
                else {
                    finished = true;
                    Result::<(), ()>::Ok(())
                }
            }).unwrap();

            if finished {
                break;
            }
        }

        expr.unwrap()
    }

    pub(super) fn read_pure_identifier(&mut self) -> SpanTagged<String> {
        let span = self.read_token_best_effort(Token::Identifier);
        let str: String = self.text_rope.iterator_range(span.clone()).collect();
        return (span, str);
    }

    pub(super) fn parse_unary(&mut self) -> SpanTagged<Expression> {
        self.parse_unary_with_leading_newlines(true)
    }

    fn parse_unary_with_leading_newlines(
        &mut self,
        allow_leading_newlines: bool,
    ) -> SpanTagged<Expression> {
        if allow_leading_newlines {
            self.advance_newlines();
        }

        // parse base expression
        let expr = try_all!(self, expected = Some("<unary expression>"), {
            /* unary operators */
            op = UnaryOperatorPred => {
                self.parse_unary_preoperator(op)?
            },
            /* entire sub expression */
            ExactPred(Token::LParen) => {
                self.parse_parenthesis_sub_expression()
            },
            /* identifier */
            ExactPred(Token::Identifier) => {
                let span = self.advance_token();
                let str: String = self.text_rope.iterator_range(span.clone()).collect();
                (span, Expression::IdentifierReference(IdentifierReference::Value(str)))
            },
            ExactPred(Token::Reference) => {
                let base_span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier();
                (base_span.start..full_span.end, Expression::IdentifierReference(IdentifierReference::Reference(str)))
            },
            ExactPred(Token::StatefulReference) => {
                let base_span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier();
                (base_span.start..full_span.end, Expression::IdentifierReference(IdentifierReference::StatefulReference(str)))
            },
            /* operator definition */
            ExactPred(Token::Operator) => {
                let base_span = self.advance_token();
                let target = self.parse_unary();
                (base_span.start..target.0.end, Expression::OperationDefinition(OperatorDefinition { lambda: (target.0, Box::new(target.1)) }))
            },
            /* lambda definition */
            ExactPred(Token::Pipe) => {
                self.parse_lambda()
            },
            /* blocks */
            ExactPred(Token::Block) => {
                self.parse_block()
            },
            ExactPred(Token::Anim) => {
                self.parse_anim()
            },
            /* literals */
            ExactPred(Token::Nil) => {
                self.parse_nil_literal()
            },
            ExactPred(Token::IntegerLiteral) => {
                self.parse_int_literal()
            },
            ExactPred(Token::FloatLiteral) => {
                self.parse_float_literal()
            },
            ExactPred(Token::StringLiteral) => {
                self.parse_string_literal()
            },
            ExactPred(Token::LBracket) => {
                self.parse_map_or_list_literal()
            },
            /* monocurl internal */
            if InStdLibPredicate, ExactPred(Token::Native) => {
                let span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier();
                let args = self.parse_invocation(Token::LParen, Token::RParen, false);
                let arguments = (args.0, args.1
                    .into_iter()
                    .map(|(_label, expr)| expr)
                    .collect());
                (span.start..arguments.0.end, Expression::NativeInvocation(NativeInvocation {
                    function: (full_span, IdentifierReference::Value(str)),
                    arguments: arguments.1,
                }))
            },
        });

        match expr {
            Ok(expr) => self.apply_postfixes(expr),
            Err(_) => (self.nil_range(), Expression::default()),
        }
    }

    pub(super) fn apply_postfixes(
        &mut self,
        base: SpanTagged<Expression>,
    ) -> SpanTagged<Expression> {
        let mut expr = Some(base);

        loop {
            if self
                .tokens
                .get(self.token_index)
                .is_some_and(|(tok, _)| *tok == Token::Newline)
            {
                return expr.unwrap();
            }

            let mut is_finished = false;

            let mut take_expr = || expr.take().unwrap();
            let boxify = |raw: SpanTagged<Expression>| (raw.0, Box::new(raw.1));
            let next = match try_all!(self, expected = Some("<postfix operator>"), {
                ExactPredDesc(Token::LParen, "<function call>") => {
                    // lambda invocation
                    let arguments = self.parse_invocation(Token::LParen, Token::RParen, true);
                    let old = boxify(take_expr());
                    (old.0.start..arguments.0.end, Expression::LambdaInvocation(LambdaInvocation {
                        lambda: old,
                        arguments: arguments
                    }))
                },
                ExactPredDesc(Token::LFlower, "<operator invocation>") => {
                    // operator invocation
                    let arguments = self.parse_invocation(Token::LFlower, Token::RFlower, false);
                    let old = boxify(take_expr());
                    let operand = self.parse_unary();

                    (old.0.start..operand.0.end, Expression::OperatorInvocation(OperatorInvocation {
                        operator: old,
                        arguments,
                        operand: boxify(operand)
                    }))
                },
                ExactPredDesc(Token::LBracket, "<subscript>") => {
                    // subscript
                    self.advance_token();
                    let index = self.parse_expr_best_effort();
                    self.advance_newlines();
                    let end_span = self.read_token_best_effort(Token::RBracket);
                    let old = boxify(take_expr());
                    (old.0.start..end_span.end, Expression::Subscript(Subscript {
                        base: old,
                        index: boxify(index)
                    }))
                },
                ExactPredDesc(Token::Dot, "<property access>") => {
                    self.advance_token();
                    let attribute = self.read_pure_identifier();
                    let old = boxify(take_expr());
                    (old.0.start..attribute.0.end, Expression::Property(Property {
                        base: old,
                        attribute: (attribute.0, IdentifierReference::Value(attribute.1))
                    }))
                },
                else {
                    is_finished = true;
                    Result::<_, ()>::Ok(take_expr())
                }
            }) {
                Ok(next) => next,
                Err(_) => {
                    // return whatever we have
                    return take_expr();
                }
            };

            if is_finished {
                return next;
            } else {
                expr = Some(next);
            }
        }
    }

    // (<token_index> a,b,c,d,e)
    pub(super) fn parse_invocation(
        &mut self,
        start: Token,
        end: Token,
        allow_newlines: bool,
    ) -> SpanTagged<
        Vec<(
            Option<SpanTagged<IdentifierDeclaration>>,
            SpanTagged<Expression>,
        )>,
    > {
        self.debug_assert_token_eq(start);
        let range = self.precomputation.bracket_internal_range(self.token_index);
        let base_span = self.advance_token();

        self.state.push_frame(|frame| frame.operating_range = range);

        let mut arguments = Vec::new();
        loop {
            if allow_newlines {
                self.advance_newlines();
            }
            if self.peek_token().is_none() {
                break;
            }
            if matches!(self.peek_token(), Some((token, _)) if *token == end) {
                break;
            }

            let pre_token_index = self.token_index;
            let mut read = || {
                if arguments.len() > 0 {
                    self.read_token_best_effort(Token::Comma);
                }

                if allow_newlines {
                    self.advance_newlines();
                }
                let label = self.peek_token().and_then(|tok| {
                    if !matches!(tok.0, Token::ArgumentLabel) {
                        return None;
                    }
                    let text: String = self.text_rope.iterator_range(tok.1.clone()).collect();
                    if text.ends_with(':') {
                        Some((
                            tok.1.start..tok.1.end,
                            IdentifierDeclaration(text[..text.len() - 1].to_string()),
                        ))
                    } else {
                        None
                    }
                });

                if let Some(label) = label {
                    // consume label
                    self.advance_token();
                    if allow_newlines {
                        self.advance_newlines();
                    }

                    let argument = self.parse_expr_best_effort();

                    (Some(label), argument)
                } else {
                    let argument = self.parse_expr_best_effort();
                    (None, argument)
                }
            };

            let result = read();
            arguments.push(result);
            let post_token_index = self.token_index;

            if pre_token_index == post_token_index {
                // no progress, break to avoid infinite loop
                break;
            }
        }
        self.state.pop_frame();

        if allow_newlines {
            self.advance_newlines();
        }
        let end_span = self.read_token_best_effort(end);

        (base_span.start..end_span.end, arguments)
    }

    pub(super) fn parse_unary_preoperator(
        &mut self,
        op: UnaryOperatorType,
    ) -> Result<SpanTagged<Expression>, ()> {
        let base_span = self.advance_token();
        let (next_span, next) = self.parse_expr_priority(op.priority());
        Ok((
            base_span.start..next_span.end,
            Expression::UnaryPreOperator(UnaryPreOperator {
                op_type: op,
                operand: (next_span, Box::new(next)),
            }),
        ))
    }

    pub(super) fn parse_parenthesis_sub_expression(&mut self) -> SpanTagged<Expression> {
        self.debug_assert_token_eq(Token::LParen);
        let inner_range = self.precomputation.bracket_internal_range(self.token_index);
        let base_span = self.advance_token();

        self.state.push_frame(|frame| {
            frame.operating_range = inner_range;
        });
        let result = self.parse_expr_best_effort();
        self.state.pop_frame();

        self.advance_newlines();
        let terminal_span = self.read_token_best_effort(Token::RParen);

        (base_span.start..terminal_span.end, result.1)
    }

    pub(super) fn parse_anim(&mut self) -> SpanTagged<Expression> {
        self.debug_assert_token_eq(Token::Anim);
        let base_span = self.advance_token();

        let body = self.parse_body(|frame| {
            frame.in_playable_block = true;
            frame.in_lambda_or_block = false;
        });

        (
            base_span.start..body.0.end,
            Expression::Anim(Anim { body: body.1 }),
        )
    }

    pub(super) fn parse_lambda(&mut self) -> SpanTagged<Expression> {
        self.debug_assert_token_eq(Token::Pipe);
        let base_span = self.advance_token();
        let mut args = Vec::new();
        self.advance_newlines();
        // parse arguments (and default values)
        if self.read_if_token(Token::Pipe).is_none() {
            let mut is_first = true;
            loop {
                self.advance_newlines();
                if !is_first {
                    let mut done = true;
                    try_all!(self, {
                        ExactPred(Token::Pipe) => {
                            self.advance_token();
                        },
                        ExactPred(Token::Comma) => {
                            self.advance_token();
                            self.advance_newlines();
                            done = false;
                        },
                    })
                    .ok();
                    if done {
                        break;
                    }
                }
                is_first = false;

                let reference = self.read_if_token(Token::Reference).is_some();
                let name = self.parse_identifier_declaration();
                self.advance_newlines();
                let default_value = if self.read_if_token(Token::Assign).is_some() {
                    self.advance_newlines();
                    Some(self.parse_expr_best_effort())
                } else {
                    None
                };
                args.push(LambdaArg {
                    identifier: name,
                    default_value,
                    must_be_reference: reference,
                });
            }
        }

        self.advance_newlines();
        let body = try_all!(self, {
            ExactPred(Token::LFlower) => {
                let body = self.parse_body(|frame| {
                    frame.in_loop = false;
                    frame.in_playable_block = false;
                    frame.in_lambda_or_block = true;
                });
                (body.0, LambdaBody::Block(body.1))
            },
            else {
                let expr = self.parse_expr_best_effort();
                Result::<_, ()>::Ok((expr.0, LambdaBody::Inline(Box::new(expr.1))))
            }
        })
        .unwrap();

        (
            base_span.start..body.0.end,
            Expression::LambdaDefinition(LambdaDefinition { args, body }),
        )
    }

    pub(super) fn parse_block(&mut self) -> SpanTagged<Expression> {
        self.debug_assert_token_eq(Token::Block);
        let base_span = self.advance_token();

        let body = self.parse_body(|frame| {
            frame.in_loop = false;
            frame.in_playable_block = false;
            frame.in_lambda_or_block = true;
        });

        (
            base_span.start..body.0.end,
            Expression::Block(Block { body: body.1 }),
        )
    }
}
