use super::*;

fn cursor_prefers_parameter_hint_up(
    function_start: Location8,
    cursor_pos: Count8,
    text_rope: &Rope<TextAggregate>,
) -> bool {
    text_rope.utf8_prefix_summary(cursor_pos).newlines > function_start.row
}

impl CompilationService {
    pub(super) async fn emit_parameter_hint(
        &mut self,
        latest_cursor: Cursor,
        last_compile_result: &CompileResult,
        latest_text_rope: Rope<TextAggregate>,
        lex_rope: Rope<Attribute<LexData>>,
        latest_version: usize,
    ) {
        let find_active_argument_index = |argument_spans: &[Span8], cursor_pos: Count8| -> usize {
            let last_starting_before = argument_spans
                .iter()
                .rposition(|span| span.start <= cursor_pos);

            let first_ending_after = argument_spans.iter().position(|span| span.end > cursor_pos);

            // if any range contains it, then clearly its that one
            // otherwise, look for comma separating the two
            match (last_starting_before, first_ending_after) {
                (Some(u), Some(v)) => {
                    if u == v {
                        u
                    } else {
                        let mut pos = argument_spans[u].end;
                        let mut comma_pos = argument_spans[v].start;
                        for (chunk, tok) in lex_rope.iterator(pos) {
                            if pos >= argument_spans[v].start {
                                break;
                            }
                            if tok == Token::Comma {
                                comma_pos = pos;
                            }
                            pos += chunk;
                        }
                        if cursor_pos <= comma_pos { u } else { v }
                    }
                }
                (Some(u), None) => u,
                (None, Some(v)) => v,
                (None, None) => 0,
            }
        };

        let hint = self
            .cursor_pos(latest_cursor, &latest_text_rope)
            .and_then(|cursor| {
                last_compile_result
                    .root_references
                    .iter()
                    .find(|reference| {
                        reference
                            .invocation_spans
                            .as_ref()
                            .is_some_and(|inv| inv.0.contains(&cursor))
                    })
                    .and_then(|reference| {
                        let sym = &reference.symbol;
                        match &sym.function_info {
                            SymbolFunctionInfo::Lambda { args }
                            | SymbolFunctionInfo::Operator { args } => {
                                let func_start_loc =
                                    latest_text_rope.utf8_prefix_summary(reference.span.start);

                                let offset = if matches!(
                                    sym.function_info,
                                    SymbolFunctionInfo::Operator { .. }
                                ) {
                                    1
                                } else {
                                    0
                                };
                                let args = if args.len() <= offset {
                                    vec![ParameterHintArg {
                                        name: String::new(),
                                        has_default: false,
                                        is_reference: false,
                                    }]
                                } else {
                                    args[offset..]
                                        .iter()
                                        .map(|arg| ParameterHintArg {
                                            name: arg.name.clone(),
                                            has_default: arg.has_default,
                                            is_reference: arg.is_reference,
                                        })
                                        .collect()
                                };

                                let invoked_args = reference.invocation_spans.as_ref().unwrap();
                                let active_argument_index =
                                    find_active_argument_index(&invoked_args.1, cursor);
                                let active_index = active_argument_index.min(args.len() - 1);

                                let function_start = Location8 {
                                    row: func_start_loc.newlines,
                                    col: func_start_loc.bytes_utf8_since_newline,
                                };
                                Some(ParameterPositionHint {
                                    name: sym.name.clone(),
                                    args,
                                    active_index,
                                    function_start,
                                    is_operator: matches!(
                                        sym.function_info,
                                        SymbolFunctionInfo::Operator { .. }
                                    ),
                                    prefer_up: cursor_prefers_parameter_hint_up(
                                        function_start,
                                        cursor,
                                        &latest_text_rope,
                                    ),
                                })
                            }
                            SymbolFunctionInfo::None => None,
                        }
                    })
            });

        self.sm_tx
            .send(ServiceManagerMessage::UpdateParameterHintPosition {
                hint,
                cursor: latest_cursor,
                version: latest_version,
            })
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_hint_prefers_up_after_function_start_line() {
        let src = "ParametricFunc(\n    |t| t,\n    [0, 1, 64]\n)";
        let rope = Rope::<TextAggregate>::from_str(src);
        let function_start = Location8 { row: 0, col: 0 };
        let same_line_cursor = src.find("(").unwrap();
        let later_line_cursor = src.find("[0").unwrap();

        assert!(!cursor_prefers_parameter_hint_up(
            function_start,
            same_line_cursor,
            &rope
        ));
        assert!(cursor_prefers_parameter_hint_up(
            function_start,
            later_line_cursor,
            &rope
        ));
    }
}
