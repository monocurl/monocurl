use super::*;

impl Compiler {
    // dump all visible symbols at the current positions
    pub(super) fn infer_possible_cursor_identifiers(&mut self, statement_span: Span8) {
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
