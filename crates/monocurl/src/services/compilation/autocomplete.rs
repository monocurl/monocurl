use super::*;

fn token_autocomplete_category(token: &Token) -> AutoCompleteCategory {
    match token {
        Token::StatefulReference
        | Token::Plus
        | Token::Minus
        | Token::Multiply
        | Token::Power
        | Token::Divide
        | Token::IntegerDivide
        | Token::KeyValueMap
        | Token::Assign
        | Token::DotAssign
        | Token::Eq
        | Token::Ne
        | Token::Lt
        | Token::Le
        | Token::Gt
        | Token::Ge
        | Token::And
        | Token::Not
        | Token::Or
        | Token::In
        | Token::Pipe
        | Token::Dot
        | Token::Append
        | Token::Comma
        | Token::Reference
        | Token::LParen
        | Token::RParen
        | Token::LBracket
        | Token::RBracket
        | Token::LFlower
        | Token::RFlower
        | Token::Semicolon => AutoCompleteCategory::Operator,
        Token::Illegal
        | Token::Newline
        | Token::Whitespace
        | Token::Comment
        | Token::Block
        | Token::Import
        | Token::Break
        | Token::Continue
        | Token::Return
        | Token::If
        | Token::Else
        | Token::For
        | Token::While
        | Token::Operator
        | Token::Let
        | Token::Var
        | Token::Mesh
        | Token::Param
        | Token::Anim
        | Token::Play
        | Token::Print
        | Token::Slide
        | Token::Nil
        | Token::Native
        | Token::IntegerLiteral
        | Token::FloatLiteral
        | Token::StringLiteral
        | Token::ArgumentLabel
        | Token::Identifier => AutoCompleteCategory::Keyword,
    }
}

fn identifier_autocomplete_category(
    identifier_type: &CursorIdentifierType,
) -> AutoCompleteCategory {
    match identifier_type {
        CursorIdentifierType::Lambda => AutoCompleteCategory::Function,
        CursorIdentifierType::Operator => AutoCompleteCategory::Operator,
        CursorIdentifierType::Let
        | CursorIdentifierType::Var
        | CursorIdentifierType::Mesh
        | CursorIdentifierType::Param => AutoCompleteCategory::Variable,
    }
}

impl CompilationService {
    pub(super) async fn emit_autocomplete(
        &mut self,
        parse: &ParseArtifacts,
        compile: &CompileResult,
        latest_cursor: Cursor,
        version: usize,
    ) {
        fn suggestion(
            s: impl Into<String>,
            replacement: impl Into<String>,
            cursor_delta: usize,
            category: AutoCompleteCategory,
        ) -> AutoCompleteItem {
            let replacement = replacement.into();
            let rlen = replacement.len();
            AutoCompleteItem {
                head: s.into(),
                replacement: replacement,
                cursor_anchor_delta: Location8 {
                    row: 0,
                    col: rlen - cursor_delta,
                },
                cursor_head_delta: Location8 {
                    row: 0,
                    col: rlen - cursor_delta,
                },
                category,
            }
        }

        let mut suggestions = vec![];
        for token in &parse.cursor_possibilities {
            if let Some(s) = token.autocomplete() {
                let (replacement, delta) = match token {
                    Token::Block | Token::Anim => (s.to_string() + " {}", 1),
                    Token::If | Token::While | Token::For => (s.to_string() + " ()", 1),
                    _ => (s.to_string() + " ", 0),
                };
                suggestions.push(suggestion(
                    s,
                    replacement,
                    delta,
                    token_autocomplete_category(token),
                ));
            }
        }

        if parse.cursor_possibilities.contains(&Token::Identifier) {
            for ident in &compile.possible_cursor_identifiers {
                let (replacement, delta) = match ident.identifier_type {
                    CursorIdentifierType::Lambda => (ident.name.clone() + "()", 1),
                    CursorIdentifierType::Operator => (ident.name.clone() + "{}", 1),
                    _ => (ident.name.clone(), 0),
                };
                suggestions.push(suggestion(
                    &ident.name,
                    replacement,
                    delta,
                    identifier_autocomplete_category(&ident.identifier_type),
                ))
            }
        }

        self.sm_tx
            .send(ServiceManagerMessage::UpdateAutocompleteSuggestions {
                suggestions,
                cursor: latest_cursor,
                version,
            })
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_categories_split_keywords_and_operators() {
        assert_eq!(
            token_autocomplete_category(&Token::If),
            AutoCompleteCategory::Keyword
        );
        assert_eq!(
            token_autocomplete_category(&Token::Plus),
            AutoCompleteCategory::Operator
        );
    }

    #[test]
    fn identifier_categories_split_functions_variables_and_operators() {
        assert_eq!(
            identifier_autocomplete_category(&CursorIdentifierType::Lambda),
            AutoCompleteCategory::Function
        );
        assert_eq!(
            identifier_autocomplete_category(&CursorIdentifierType::Param),
            AutoCompleteCategory::Variable
        );
        assert_eq!(
            identifier_autocomplete_category(&CursorIdentifierType::Operator),
            AutoCompleteCategory::Operator
        );
    }
}
