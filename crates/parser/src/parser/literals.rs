use super::*;

impl SectionParser {
    pub(super) fn decode_string_literal(string: &str) -> std::result::Result<String, &'static str> {
        let mut build = String::new();
        let mut it = string.chars();
        if it.next() != Some('"') {
            return Err("Malformed string literal");
        }

        while let Some(curr) = it.next() {
            if curr == '%' {
                let next = it.next();
                if let Some(map) = next.and_then(Self::escape_char_literal) {
                    build.push(map);
                } else {
                    return Err("Illegal escape character");
                }
            } else if curr == '"' {
                if it.next().is_some() {
                    return Err("Malformed string literal");
                } else {
                    return Ok(build);
                }
            } else {
                build.push(curr);
            }
        }

        Err("Malformed string literal")
    }

    pub(super) fn parse_basic_literal(
        &mut self,
        tok: Token,
        f: impl FnOnce(&str) -> std::result::Result<Literal, &'static str>,
        default: Literal,
    ) -> SpanTagged<Expression> {
        self.debug_assert_token_eq(tok);
        let span = self.advance_token();
        let content: String = self.text_rope.iterator_range(span.clone()).collect();
        let literal = match f(&content) {
            Ok(literal) => literal,
            Err(message) => {
                self.emit_error("Illegal Literal".into(), message.into(), span.clone());
                return (span, Expression::Literal(default));
            }
        };
        (span, Expression::Literal(literal))
    }

    pub(super) fn escape_char_literal(c: char) -> Option<char> {
        match c {
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            '%' => Some('%'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '\\' => Some('\\'),
            _ => None,
        }
    }

    pub(super) fn parse_string_literal(&mut self) -> SpanTagged<Expression> {
        self.parse_basic_literal(
            Token::StringLiteral,
            |string| Self::decode_string_literal(string).map(Literal::String),
            Literal::String(String::new()),
        )
    }

    pub(super) fn parse_nil_literal(&mut self) -> SpanTagged<Expression> {
        self.parse_basic_literal(
            Token::Nil,
            |string| {
                if string == "nil" {
                    Ok(Literal::Nil)
                } else {
                    Err("Malformed nil literal")
                }
            },
            Literal::Nil,
        )
    }

    pub(super) fn parse_int_literal(&mut self) -> SpanTagged<Expression> {
        self.parse_basic_literal(
            Token::IntegerLiteral,
            |string| {
                Self::parse_numeric_with_suffix(
                    string,
                    |s| {
                        s.parse::<i64>()
                            .map(|v| v as f64)
                            .map_err(|_| "Invalid integer literal")
                    },
                    |s| {
                        Ok(Literal::Int(
                            s.parse::<i64>().map_err(|_| "Invalid integer literal")?,
                        ))
                    },
                )
            },
            Literal::Int(0),
        )
    }

    pub(super) fn parse_float_literal(&mut self) -> SpanTagged<Expression> {
        self.parse_basic_literal(
            Token::FloatLiteral,
            |string| {
                Self::parse_numeric_with_suffix(
                    string,
                    |s| s.parse::<f64>().map_err(|_| "Invalid float literal"),
                    |s| {
                        Ok(Literal::Float(
                            s.parse::<f64>().map_err(|_| "Invalid float literal")?,
                        ))
                    },
                )
            },
            Literal::Float(0.0),
        )
    }

    pub(super) fn parse_numeric_with_suffix(
        string: &str,
        parse_value: impl Fn(&str) -> std::result::Result<f64, &'static str>,
        standard_parse_value: impl Fn(&str) -> std::result::Result<Literal, &'static str>,
    ) -> std::result::Result<Literal, &'static str> {
        // directional suffixes
        if let Some(stripped) = string.strip_suffix('l') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Left(value)));
        }
        if let Some(stripped) = string.strip_suffix('r') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Right(value)));
        }
        if let Some(stripped) = string.strip_suffix('u') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Up(value)));
        }
        if let Some(stripped) = string.strip_suffix('d') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Down(value)));
        }
        if let Some(stripped) = string.strip_suffix('f') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Forward(value)));
        }
        if let Some(stripped) = string.strip_suffix('b') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Backward(value)));
        }
        if let Some(stripped) = string.strip_suffix('i') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Imaginary(value));
        }
        if let Some(stripped) = string.strip_suffix("dg") {
            let value = parse_value(stripped)?;
            let radians = value * std::f64::consts::PI / 180.0;
            return Ok(Literal::Float(radians));
        }

        standard_parse_value(string)
    }

    pub(super) fn parse_map_or_vector_literal(&mut self) -> SpanTagged<Expression> {
        self.debug_assert_token_eq(Token::LBracket);
        let inner_range = self.precomputation.bracket_internal_range(self.token_index);
        let base_span = self.advance_token();

        if inner_range.is_empty() {
            let end_span = self.read_token_best_effort(Token::RBracket);
            return (
                base_span.start..end_span.end,
                Expression::Literal(Literal::Vector(vec![])),
            );
        }

        self.state.push_frame(|frame| {
            frame.operating_range = inner_range.clone();
        });

        self.advance_newlines();

        let literal = if self.read_if_token(Token::KeyValueMap).is_some() {
            Expression::Literal(Literal::Map(vec![]))
        } else {
            let mut vector_entries = Vec::new();
            let mut map_entries = Vec::new();
            let mut emitted_error = false;

            loop {
                let entry_start = self.token_index;
                let (key_value_token, entry_end) =
                    self.find_top_level_map_entry_delimiters(inner_range.end);

                if let Some(key_value_token) = key_value_token {
                    if !vector_entries.is_empty() && !emitted_error {
                        self.emit_error(
                            "Ambiguous Literal".into(),
                            "cannot decide if literal is list or map".into(),
                            base_span.start..self.tokens[key_value_token].1.end,
                        );
                        emitted_error = true;
                    }

                    let entry = self.parse_literal_entry(entry_start..key_value_token);

                    self.token_index = key_value_token;
                    self.read_token_best_effort(Token::KeyValueMap);

                    let value_start = self.token_index;
                    let value = self.parse_literal_entry(value_start..entry_end);

                    map_entries.push((entry, value));
                } else {
                    let entry = self.parse_literal_entry(entry_start..entry_end);

                    if !map_entries.is_empty() && !emitted_error {
                        self.emit_error(
                            "Ambiguous Literal".into(),
                            "cannot decide if literal is list or map".into(),
                            base_span.start..entry.0.end,
                        );
                        emitted_error = true;
                    }
                    vector_entries.push(entry)
                }

                self.advance_newlines();
                if self.peek_token().is_none() {
                    break;
                }

                let fail = try_all!(self, {
                    ExactPred(Token::Comma) => {
                        self.advance_token();
                    },
                    else {
                        Result::<_, ()>::Err(())
                    }
                })
                .is_err();
                if fail {
                    break;
                }
            }

            if !vector_entries.is_empty() {
                Expression::Literal(Literal::Vector(vector_entries))
            } else {
                Expression::Literal(Literal::Map(map_entries))
            }
        };

        self.state.pop_frame();

        self.advance_newlines();
        let end_span = self.read_token_best_effort(Token::RBracket);
        (base_span.start..end_span.end, literal)
    }

    fn parse_literal_entry(&mut self, range: std::ops::Range<usize>) -> SpanTagged<Expression> {
        self.state.push_frame(|frame| {
            frame.operating_range = range.clone();
        });
        let entry = self.parse_expr_best_effort();
        self.advance_newlines();
        if self.token_index < range.end {
            self.emit_error(
                "Invalid Literal Entry".into(),
                "literal entries must be valid expressions separated by ','".into(),
                self.tokens[self.token_index].1.clone(),
            );
        }
        self.state.pop_frame();
        self.token_index = range.end;
        entry
    }

    fn find_top_level_map_entry_delimiters(&self, range_end: usize) -> (Option<usize>, usize) {
        let mut key_value_token = None;
        let mut idx = self.token_index;

        while idx < range_end {
            match self.tokens[idx].0 {
                Token::LParen | Token::LBracket | Token::LFlower => {
                    idx = self
                        .precomputation
                        .bracket_partners
                        .get(&idx)
                        .map(|end| end.saturating_add(1).min(range_end))
                        .unwrap_or(range_end);
                }
                Token::KeyValueMap => {
                    key_value_token = Some(idx);
                    idx += 1;
                }
                Token::Comma => return (key_value_token, idx),
                _ => idx += 1,
            }
        }

        (key_value_token, range_end)
    }
}
