use super::*;

struct RootSectionTokens {
    tokens: Vec<(Token, Span8)>,
    name: Option<String>,
}

impl Parser {
    pub(super) fn import_err(span: Span8, message: &str) -> Diagnostic {
        Diagnostic {
            span,
            title: "Import Error".to_string(),
            message: message.to_string(),
        }
    }

    pub(super) fn dfs(
        &mut self,
        root_span: Option<Span8>,
        external_context: &ParseImportContext,
        file: FileResult,
    ) -> Result<(), ()> {
        if self.preparsed_files.iter().any(|old| old.path == file.path) {
            return Ok(()); // diamond import is fine
        }
        if self.import_stack.iter().any(|old| old == &file.path) {
            self.errors
                .push(Self::import_err(root_span.unwrap(), "Cyclic Import"));
            return Err(());
        }

        self.import_stack.push(file.path.clone());

        let mut imports = vec![];
        let mut token_index = 0;

        loop {
            while token_index < file.tokens.len()
                && matches!(
                    file.tokens[token_index].0,
                    Token::Newline | Token::Semicolon
                )
            {
                token_index += 1;
            }

            if token_index >= file.tokens.len() || file.tokens[token_index].0 != Token::Import {
                break;
            }
            let import_span = file.tokens[token_index].1.clone();
            token_index += 1;

            // parse: identifier (. identifier)*
            if token_index >= file.tokens.len() || file.tokens[token_index].0 != Token::Identifier {
                self.errors
                    .push(Self::import_err(import_span, "Expected module path"));
                return Err(());
            }
            let first_span = file.tokens[token_index].1.clone();
            let mut import_rel_path = PathBuf::from(
                file.text_rope
                    .iterator_range(first_span.clone())
                    .collect::<String>(),
            );
            let mut end = first_span.end;
            token_index += 1;

            while token_index < file.tokens.len() && file.tokens[token_index].0 == Token::Dot {
                token_index += 1;
                if token_index >= file.tokens.len()
                    || file.tokens[token_index].0 != Token::Identifier
                {
                    self.errors.push(Self::import_err(
                        import_span.clone(),
                        "Expected identifier after '.'",
                    ));
                    return Err(());
                }
                let id_span = file.tokens[token_index].1.clone();
                import_rel_path.push(
                    file.text_rope
                        .iterator_range(id_span.clone())
                        .collect::<String>(),
                );
                end = id_span.end;
                token_index += 1;
            }

            let full_span = import_span.start..end;
            let Some(imported_file) = external_context.file_content(
                file.path.as_ref().and_then(|f| f.parent()),
                &import_rel_path,
            ) else {
                self.errors.push(Self::import_err(
                    full_span.clone(),
                    &format!("Cannot find module \"{}\"", import_rel_path.display()),
                ));
                return Err(());
            };
            imports.push(imported_file.path.clone().unwrap());
            self.dfs(
                root_span.clone().or(Some(full_span.clone())),
                external_context,
                imported_file,
            )?;

            if token_index < file.tokens.len()
                && !matches!(
                    file.tokens[token_index].0,
                    Token::Newline | Token::Semicolon
                )
            {
                self.errors.push(Self::import_err(
                    full_span,
                    "Expected <end of line> or semicolon",
                ));
                return Err(());
            }
        }

        self.import_stack.pop();
        self.preparsed_files.push(PreparsedFile {
            imports,
            path: file.path,
            text_rope: file.text_rope,
            root_import_span: root_span,
            tokens: file.tokens.into_iter().skip(token_index).collect(),
            is_stdlib: file.is_stdlib,
        });
        Ok(())
    }

    pub(super) fn parse_section(
        tokens: Vec<(Token, Span8)>,
        text_rope: Rope<TextAggregate>,
        section_type: SectionType,
        cursor: Option<Count8>,
        root_import_span: Option<Span8>,
    ) -> (Section, ParseArtifacts) {
        let mut parser =
            SectionParser::new(tokens, text_rope, section_type, root_import_span, cursor);
        let section = parser.parse_section();
        (section, parser.artifacts)
    }

    fn split_root_sections(
        tokens: Vec<(Token, Span8)>,
        text_rope: &Rope<TextAggregate>,
    ) -> (Vec<RootSectionTokens>, ParseArtifacts) {
        let mut sections = vec![RootSectionTokens {
            tokens: Vec::new(),
            name: None,
        }];
        let mut artifacts = ParseArtifacts::default();
        let mut token_index = 0;

        while token_index < tokens.len() {
            if tokens[token_index].0 != Token::Slide {
                sections
                    .last_mut()
                    .unwrap()
                    .tokens
                    .push(tokens[token_index].clone());
                token_index += 1;
                continue;
            }

            sections.push(RootSectionTokens {
                tokens: Vec::new(),
                name: None,
            });
            token_index += 1;

            if let Some((Token::StringLiteral, span)) = tokens.get(token_index) {
                let raw: String = text_rope.iterator_range(span.clone()).collect();
                match SectionParser::decode_string_literal(&raw) {
                    Ok(name) => sections.last_mut().unwrap().name = Some(name),
                    Err(message) => artifacts.error_diagnostics.push(Diagnostic {
                        span: span.clone(),
                        title: "Illegal Slide Title".into(),
                        message: message.into(),
                    }),
                }
                token_index += 1;
            }
        }

        (sections, artifacts)
    }

    pub(super) fn parse_file(
        currently_parsed: &HashMap<PathBuf, Arc<SectionBundle>>,
        f: PreparsedFile,
        cursor: Option<Count8>,
    ) -> (Arc<SectionBundle>, ParseArtifacts) {
        let file_index = currently_parsed.len();
        let imported_files = f
            .imports
            .iter()
            .map(|path|
            // can be null in the case that the library failed to parse so it wasn't inserted properly
            currently_parsed.get(path).map(|x| x.file_index).unwrap_or_default())
            .collect();

        if f.root_import_span.is_none() {
            let (sections, split_artifacts) = Self::split_root_sections(f.tokens, &f.text_rope);
            let mut artifacts = ParseArtifacts::default();
            artifacts.extend(split_artifacts);
            let mut parsed_sections = vec![];
            for (i, section) in sections.into_iter().enumerate() {
                let stype = if i == 0 {
                    SectionType::Init
                } else {
                    SectionType::Slide
                };
                let (mut parsed_section, sub_artifacts) = Self::parse_section(
                    section.tokens,
                    f.text_rope.clone(),
                    stype,
                    cursor.clone(),
                    f.root_import_span.clone(),
                );
                parsed_section.name = section.name;
                parsed_sections.push(parsed_section);
                artifacts.extend(sub_artifacts);
            }

            let ret = Arc::new(SectionBundle {
                file_path: f.path.clone(),
                file_index,
                imported_files,
                sections: parsed_sections,
                root_import_span: f.root_import_span,
                was_cached: false,
            });
            (ret, artifacts)
        } else {
            let stype = if f.is_stdlib {
                SectionType::StandardLibrary
            } else {
                SectionType::UserLibrary
            };
            let (section, artifacts) = Self::parse_section(
                f.tokens,
                f.text_rope,
                stype,
                None,
                f.root_import_span.clone(),
            );
            (
                Arc::new(SectionBundle {
                    file_path: f.path,
                    file_index,
                    imported_files,
                    sections: vec![section],
                    root_import_span: f.root_import_span,
                    was_cached: false,
                }),
                artifacts,
            )
        }
    }

    pub fn parse(
        external_context: &mut ParseImportContext,
        lex_rope: Rope<Attribute<Token>>,
        text_rope: Rope<TextAggregate>,
        cursor: Option<Count8>,
    ) -> (Vec<Arc<SectionBundle>>, ParseArtifacts) {
        let mut p = Parser {
            preparsed_files: vec![],
            import_stack: vec![],
            errors: vec![],
        };

        let Ok(()) = p.dfs(
            None,
            external_context,
            FileResult {
                path: external_context.root_file_user_path.clone(),
                tokens: flatten_rope(&lex_rope),
                text_rope: Rope::from(text_rope),
                is_stdlib: false,
            },
        ) else {
            return (
                vec![],
                ParseArtifacts {
                    error_diagnostics: p.errors,
                    cursor_possibilities: HashSet::default(),
                },
            );
        };

        let mut bundles = HashMap::new();
        let mut artifacts = ParseArtifacts::default();
        let mut sorted_bundles = Vec::new();
        for file in p.preparsed_files {
            if file.root_import_span.is_some()
                && let Some(result) = external_context.cache_get(&file.path)
            {
                artifacts.extend(result.1);
                sorted_bundles.push(result.0.clone());
                bundles.insert(file.path.unwrap(), result.0);
                continue;
            }

            let key = file.path.clone();
            let is_root = file.root_import_span.is_none();
            let (bundle, sub_artifacts) = Self::parse_file(&bundles, file, cursor.clone());
            if !is_root && let Some(key) = key.clone() {
                external_context.set_cache(key, &bundle, sub_artifacts.clone());
            }

            artifacts.extend(sub_artifacts);
            sorted_bundles.push(bundle.clone());
            if let Some(key) = key {
                bundles.insert(key, bundle);
            }
        }

        (sorted_bundles, artifacts)
    }
}
