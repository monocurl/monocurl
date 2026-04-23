#[cfg(test)]
mod test {
    use std::{path::PathBuf, sync::Arc};

    use bytecode::{CopyValueMode, Instruction};
    use lexer::lexer::Lexer;
    use lexer::token::Token;
    use parser::ast::{
        BinaryOperator, BinaryOperatorType, Declaration, Expression, IdentifierDeclaration,
        IdentifierReference, LambdaArg, LambdaBody, LambdaDefinition, Literal, Section,
        SectionBundle, SectionType, Statement, VariableType,
    };
    use stdlib::registry::registry;
    use structs::rope::Rope;
    use structs::text::Span8;

    use crate::cache::CompilerCache;

    use super::super::{CompileResult, FunctionArgInfo, SymbolFunctionInfo, compile};

    fn empty_span() -> Span8 {
        0..0
    }

    fn s<T>(v: T) -> (Span8, T) {
        (empty_span(), v)
    }

    fn sb<T>(v: T) -> (Span8, Box<T>) {
        (empty_span(), Box::new(v))
    }

    fn make_bundle(
        stmts: Vec<(Span8, Statement)>,
        section_type: SectionType,
    ) -> Arc<SectionBundle> {
        Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section {
                body: stmts,
                section_type,
                name: None,
            }],
            root_import_span: None,
            was_cached: false,
        })
    }

    fn test_compile(sections: &[Arc<SectionBundle>]) -> CompileResult {
        compile(&mut CompilerCache::default(), None, sections)
    }

    fn compile_stmts(stmts: Vec<(Span8, Statement)>) -> CompileResult {
        test_compile(&[make_bundle(stmts, SectionType::Slide)])
    }

    fn lex(src: &str) -> Vec<(Token, Span8)> {
        Lexer::token_stream(src.chars())
            .into_iter()
            .filter(|(t, _)| t != &Token::Whitespace && t != &Token::Comment)
            .collect()
    }

    fn parse_stmts(src: &str) -> Vec<(Span8, Statement)> {
        parse_stmts_as(src, SectionType::Slide)
    }

    fn parse_stmts_as(src: &str, section_type: SectionType) -> Vec<(Span8, Statement)> {
        use parser::parser::SectionParser;

        let tokens = lex(src);
        let text_rope = Rope::from_str(src);
        let mut parser = SectionParser::new(tokens, text_rope, section_type, None, None);
        parser.parse_statement_list()
    }

    fn compile_src(src: &str) -> CompileResult {
        compile_stmts(parse_stmts(src))
    }

    fn compile_src_as_section(src: &str, section_type: SectionType) -> CompileResult {
        test_compile(&[make_bundle(parse_stmts_as(src, section_type), section_type)])
    }

    fn has_error(result: &CompileResult, fragment: &str) -> bool {
        result.errors.iter().any(|e| e.message.contains(fragment))
    }

    fn has_warning(result: &CompileResult, fragment: &str) -> bool {
        result.warnings.iter().any(|w| w.message.contains(fragment))
    }

    fn no_errors(result: &CompileResult) {
        if !result.errors.is_empty() {
            let msgs: Vec<_> = result.errors.iter().map(|e| e.message.as_str()).collect();
            panic!("expected no errors, got: {:?}", msgs);
        }
    }

    #[test]
    fn test_section_name_is_written_to_bytecode() {
        let bundle = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section {
                body: parse_stmts("let x = 1"),
                section_type: SectionType::Slide,
                name: Some("Intro".into()),
            }],
            root_import_span: None,
            was_cached: false,
        });

        let result = test_compile(&[bundle]);
        no_errors(&result);
        assert_eq!(result.bytecode.sections[1].name.as_deref(), Some("Intro"));
    }

    #[test]
    fn test_function_info_emits_reference_and_default_metadata_for_hints() {
        let result = compile_src(
            "
            let a = 1
            let b = 2
            let c = 3
            let f = |&x, y = 1, &z = c| x
            f(a, b, c)
        ",
        );
        no_errors(&result);

        let reference = result
            .root_references
            .iter()
            .find(|reference| reference.symbol.name == "f")
            .expect("expected root reference to f");

        match &reference.symbol.function_info {
            SymbolFunctionInfo::Lambda { args } => {
                assert_eq!(
                    args,
                    &vec![
                        FunctionArgInfo {
                            name: "x".to_string(),
                            has_default: false,
                            is_reference: true,
                        },
                        FunctionArgInfo {
                            name: "y".to_string(),
                            has_default: true,
                            is_reference: false,
                        },
                        FunctionArgInfo {
                            name: "z".to_string(),
                            has_default: true,
                            is_reference: true,
                        },
                    ]
                );
            }
            other => panic!("expected lambda function info, got {other:?}"),
        }
    }

    // -- pure compiler (AST) tests --

    #[test]
    fn test_let_int_decl() {
        let stmts = vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("x".into())),
            value: s(Expression::Literal(Literal::Int(42))),
        }))];
        let result = compile_stmts(stmts);
        no_errors(&result);
        // sections[0] is the prelude; user code is in sections[1]
        let section = &result.bytecode.sections[1];
        assert!(
            section
                .instructions
                .iter()
                .any(|i| matches!(i, Instruction::PushInt { .. }))
        );
    }

    #[test]
    fn test_nil_decl_emits_push_nil() {
        let stmts = vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("x".into())),
            value: s(Expression::Literal(Literal::Nil)),
        }))];
        let result = compile_stmts(stmts);
        no_errors(&result);
        let section = &result.bytecode.sections[1];
        assert!(
            section
                .instructions
                .iter()
                .any(|i| matches!(i, Instruction::PushNil))
        );
    }

    #[test]
    fn test_let_mutation_error() {
        // let x = 1; x = 2  — should error
        let stmts = vec![
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Let,
                identifier: s(IdentifierDeclaration("x".into())),
                value: s(Expression::Literal(Literal::Int(1))),
            })),
            s(Statement::Expression(Expression::BinaryOperator(
                BinaryOperator {
                    op_type: BinaryOperatorType::Assign,
                    lhs: sb(Expression::IdentifierReference(IdentifierReference::Value(
                        "x".into(),
                    ))),
                    rhs: sb(Expression::Literal(Literal::Int(2))),
                },
            ))),
        ];
        let result = compile_stmts(stmts);
        assert!(
            has_error(&result, "cannot mutate 'x'"),
            "expected let mutation error"
        );
    }

    #[test]
    fn test_undefined_variable_error() {
        let stmts = vec![s(Statement::Expression(Expression::IdentifierReference(
            IdentifierReference::Value("notdefined".into()),
        )))];
        let result = compile_stmts(stmts);
        assert!(
            has_error(&result, "undefined variable"),
            "expected undefined variable error"
        );
    }

    #[test]
    fn test_break_outside_loop_error() {
        let result = compile_stmts(vec![s(Statement::Break)]);
        assert!(has_error(&result, "break outside loop"));
    }

    #[test]
    fn test_continue_outside_loop_error() {
        let result = compile_stmts(vec![s(Statement::Continue)]);
        assert!(has_error(&result, "continue outside loop"));
    }

    #[test]
    fn test_lambda_capture_let_ok() {
        // let x = 1; let f = |-> x  — should be fine
        let stmts = vec![
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Let,
                identifier: s(IdentifierDeclaration("x".into())),
                value: s(Expression::Literal(Literal::Int(1))),
            })),
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Let,
                identifier: s(IdentifierDeclaration("f".into())),
                value: s(Expression::LambdaDefinition(LambdaDefinition {
                    args: vec![],
                    body: s(LambdaBody::Inline(Box::new(
                        Expression::IdentifierReference(IdentifierReference::Value("x".into())),
                    ))),
                })),
            })),
        ];
        no_errors(&compile_stmts(stmts));
    }

    #[test]
    fn test_default_args_must_be_suffix() {
        // |a = 1, b| b  — required arg after default arg → error
        let stmts = vec![s(Statement::Expression(Expression::LambdaDefinition(
            LambdaDefinition {
                args: vec![
                    LambdaArg {
                        identifier: s(IdentifierDeclaration("a".into())),
                        default_value: Some(s(Expression::Literal(Literal::Int(1)))),
                        must_be_reference: false,
                    },
                    LambdaArg {
                        identifier: s(IdentifierDeclaration("b".into())),
                        default_value: None,
                        must_be_reference: false,
                    },
                ],
                body: s(LambdaBody::Inline(Box::new(
                    Expression::IdentifierReference(IdentifierReference::Value("b".into())),
                ))),
            },
        )))];
        let result = compile_stmts(stmts);
        assert!(
            has_error(
                &result,
                "required arguments must come before default arguments"
            ),
            "expected default-arg suffix error"
        );
    }

    #[test]
    fn test_default_args_valid_suffix() {
        // |a, b = 1| a  — correct ordering, no error
        let stmts = vec![s(Statement::Expression(Expression::LambdaDefinition(
            LambdaDefinition {
                args: vec![
                    LambdaArg {
                        identifier: s(IdentifierDeclaration("a".into())),
                        default_value: None,
                        must_be_reference: false,
                    },
                    LambdaArg {
                        identifier: s(IdentifierDeclaration("b".into())),
                        default_value: Some(s(Expression::Literal(Literal::Int(1)))),
                        must_be_reference: false,
                    },
                ],
                body: s(LambdaBody::Inline(Box::new(
                    Expression::IdentifierReference(IdentifierReference::Value("a".into())),
                ))),
            },
        )))];
        no_errors(&compile_stmts(stmts));
    }

    // -- cross-bundle import tests --

    #[test]
    fn test_cross_bundle_symbol_visible() {
        // bundle 0 defines `x`; bundle 1 imports it and reads it — no error
        let bundle0 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section {
                body: vec![s(Statement::Declaration(Declaration {
                    var_type: VariableType::Let,
                    identifier: s(IdentifierDeclaration("x".into())),
                    value: s(Expression::Literal(Literal::Int(7))),
                }))],
                section_type: SectionType::UserLibrary,
                name: None,
            }],
            root_import_span: None,
            was_cached: false,
        });
        let bundle1 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 1,
            imported_files: vec![0],
            sections: vec![Section {
                body: vec![s(Statement::Expression(Expression::IdentifierReference(
                    IdentifierReference::Value("x".into()),
                )))],
                section_type: SectionType::Slide,
                name: None,
            }],
            root_import_span: None,
            was_cached: false,
        });
        no_errors(&test_compile(&[bundle0, bundle1]));
    }

    #[test]
    fn test_cross_bundle_symbol_is_let() {
        // bundle 0 defines `var x`; bundle 1 imports it and tries to assign — error
        let bundle0 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section {
                body: vec![s(Statement::Declaration(Declaration {
                    var_type: VariableType::Var,
                    identifier: s(IdentifierDeclaration("x".into())),
                    value: s(Expression::Literal(Literal::Int(0))),
                }))],
                section_type: SectionType::UserLibrary,
                name: None,
            }],
            root_import_span: None,
            was_cached: false,
        });
        let bundle1 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 1,
            imported_files: vec![0],
            sections: vec![Section {
                body: vec![s(Statement::Expression(Expression::BinaryOperator(
                    BinaryOperator {
                        op_type: BinaryOperatorType::Assign,
                        lhs: sb(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".into(),
                        ))),
                        rhs: sb(Expression::Literal(Literal::Int(1))),
                    },
                )))],
                section_type: SectionType::Slide,
                name: None,
            }],
            root_import_span: None,
            was_cached: false,
        });
        let result = test_compile(&[bundle0, bundle1]);
        assert!(
            has_error(&result, "cannot mutate 'x'"),
            "imported var should become let"
        );
    }

    #[test]
    fn test_for_stdlib_range_uses_counted_loop_lowering() {
        let bundle0 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section {
                body: parse_stmts_as(
                    "let range = |start, stop| start",
                    SectionType::StandardLibrary,
                ),
                section_type: SectionType::StandardLibrary,
                name: None,
            }],
            root_import_span: None,
            was_cached: false,
        });
        let bundle1 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 1,
            imported_files: vec![0],
            sections: vec![Section {
                body: parse_stmts(
                    "
                    var sum = 0
                    for (i in range(0, 4)) {
                        sum = sum + i
                    }
                ",
                ),
                section_type: SectionType::Slide,
                name: None,
            }],
            root_import_span: None,
            was_cached: false,
        });

        let result = test_compile(&[bundle0, bundle1]);
        no_errors(&result);

        let vector_len_idx = registry().index_of("vector_len") as u16;
        let section = &result.bytecode.sections[2];
        assert!(
            !section
                .instructions
                .iter()
                .any(|instr| matches!(instr, Instruction::Subscript { .. })),
            "optimized stdlib range loop should not subscript a materialized list"
        );
        assert!(
            !section.instructions.iter().any(|instr| matches!(
                instr,
                Instruction::NativeInvoke { index, .. } if *index == vector_len_idx
            )),
            "optimized stdlib range loop should not call vector_len"
        );
        assert!(
            section
                .instructions
                .iter()
                .any(|instr| matches!(instr, Instruction::Lt)),
            "optimized stdlib range loop should use a counted comparison"
        );
        assert!(
            section
                .instructions
                .iter()
                .any(|instr| matches!(instr, Instruction::IncrementByOne { .. })),
            "optimized stdlib range loop should use the dedicated induction increment opcode"
        );
    }

    #[test]
    fn test_for_shadowed_range_keeps_generic_lowering() {
        let bundle0 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 0,
            imported_files: vec![],
            sections: vec![Section {
                body: parse_stmts_as(
                    "let range = |start, stop| start",
                    SectionType::StandardLibrary,
                ),
                section_type: SectionType::StandardLibrary,
                name: None,
            }],
            root_import_span: None,
            was_cached: false,
        });
        let bundle1 = Arc::new(SectionBundle {
            file_path: Some(PathBuf::new()),
            file_index: 1,
            imported_files: vec![0],
            sections: vec![Section {
                body: parse_stmts(
                    "
                    let range = |start, stop| [start, stop]
                    for (i in range(0, 4)) {
                    }
                ",
                ),
                section_type: SectionType::Slide,
                name: None,
            }],
            root_import_span: None,
            was_cached: false,
        });

        let result = test_compile(&[bundle0, bundle1]);
        no_errors(&result);

        let vector_len_idx = registry().index_of("vector_len") as u16;
        let section = &result.bytecode.sections[2];
        assert!(
            section
                .instructions
                .iter()
                .any(|instr| matches!(instr, Instruction::Subscript { .. })),
            "shadowed range should keep the generic list iteration path"
        );
        assert!(
            section.instructions.iter().any(|instr| matches!(
                instr,
                Instruction::NativeInvoke { index, .. } if *index == vector_len_idx
            )),
            "shadowed range should still measure the produced list"
        );
    }

    #[test]
    fn test_mesh_declaration_allowed_in_user_library() {
        let result = compile_src_as_section("mesh x = 1", SectionType::UserLibrary);
        no_errors(&result);
    }

    #[test]
    fn test_param_declaration_disallowed_in_user_library() {
        let result = compile_src_as_section("param x = 1", SectionType::UserLibrary);
        assert!(has_error(
            &result,
            "'param' declarations are not allowed in user libraries"
        ));
    }

    #[test]
    fn test_integration_arithmetic() {
        no_errors(&compile_src("let x = 1 + 2 * 3"));
    }

    #[test]
    fn test_integration_while_loop() {
        no_errors(&compile_src("var i = 0\nwhile i < 10 {\n    i = i + 1\n}"));
    }

    #[test]
    fn test_integration_for_loop() {
        // vector literal uses [] in Monocurl
        no_errors(&compile_src("let xs = [1, 2, 3]\nfor x in xs {\n}"));
    }

    #[test]
    fn test_integration_if_else() {
        no_errors(&compile_src("let x = 1\nif x == 1 {\n} else {\n}"));
    }

    #[test]
    fn test_integration_lambda_definition_and_call() {
        no_errors(&compile_src("let f = |a, b| a + b\nlet y = f(1, 2)"));
    }

    #[test]
    fn test_integration_short_circuit_and() {
        // compile a && — the result is computed but we just check no errors and
        // that a ConditionalJump was emitted somewhere in the main section
        // Monocurl uses keyword `and` not `&&`
        let result = compile_src("let z = 1 and 0");
        no_errors(&result);
        let has_cj = result.bytecode.sections.iter().any(|sec| {
            sec.instructions
                .iter()
                .any(|i| matches!(i, Instruction::ConditionalJump { .. }))
        });
        assert!(has_cj, "expected ConditionalJump in bytecode for 'and'");
    }

    #[test]
    fn test_integration_short_circuit_or() {
        // Monocurl uses keyword `or` not `||`
        let result = compile_src("let z = 0 or 1");
        no_errors(&result);
        let has_cj = result.bytecode.sections.iter().any(|sec| {
            sec.instructions
                .iter()
                .any(|i| matches!(i, Instruction::ConditionalJump { .. }))
        });
        assert!(has_cj, "expected ConditionalJump in bytecode for 'or'");
    }

    #[test]
    fn test_integration_param_requires_explicit_read() {
        let result = compile_src("param x = 1\nlet y = x");
        assert!(
            has_error(&result, "cannot read param 'x' directly"),
            "expected compile error for naked param read"
        );
    }

    #[test]
    fn test_integration_nested_vector() {
        // [] for vectors in Monocurl
        no_errors(&compile_src("let v = [1, [2, 3], 4]"));
    }

    #[test]
    fn test_integration_map_literal() {
        // [:] map syntax in Monocurl
        no_errors(&compile_src(r#"let m = ["a": 1, "b": 2]"#));
    }

    #[test]
    fn test_integration_default_arg_ordering_error() {
        let result = compile_src("let f = |a = 1, b| b");
        assert!(
            has_error(
                &result,
                "required arguments must come before default arguments"
            ),
            "expected default-arg suffix error from parser-produced AST"
        );
    }

    #[test]
    fn test_warns_for_useless_expression_statement() {
        let result = compile_src("1 + 2");
        assert!(
            has_warning(&result, "expression statement has no effect"),
            "expected useless-expression warning"
        );
    }

    #[test]
    fn test_no_warning_for_expression_statement_with_assignment() {
        let result = compile_src("var x = 0\nx = 1");
        assert!(
            !has_warning(&result, "expression statement has no effect"),
            "did not expect useless-expression warning"
        );
    }

    #[test]
    fn test_no_warning_for_expression_statement_with_lvalue_reference() {
        let result = compile_src("let poke = |slot| slot\npoke(&camera)");
        assert!(
            !has_warning(&result, "expression statement has no effect"),
            "did not expect useless-expression warning"
        );
    }

    // -- bytecode sequence tests --

    // `let x = 42` — PushInt evaluates the value, PushVar creates the variable slot.
    #[test]
    fn test_bytecode_single_let_int() {
        let result = compile_stmts(vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("x".into())),
            value: s(Expression::Literal(Literal::Int(42))),
        }))]);
        no_errors(&result);
        let sec = &result.bytecode.sections[1];
        assert_eq!(
            sec.instructions,
            vec![
                Instruction::PushInt { index: 0 },
                Instruction::ConvertVar {
                    allow_stateful: false
                },
                Instruction::EndOfExecutionHead
            ],
        );
        assert_eq!(sec.int_pool, vec![42i64]);
        assert!(sec.lambda_prototypes.is_empty());
    }

    #[test]
    fn test_bytecode_single_let_nil() {
        let result = compile_stmts(vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("x".into())),
            value: s(Expression::Literal(Literal::Nil)),
        }))]);
        no_errors(&result);
        let sec = &result.bytecode.sections[1];
        assert_eq!(
            sec.instructions,
            vec![
                Instruction::PushNil,
                Instruction::ConvertVar {
                    allow_stateful: false
                },
                Instruction::EndOfExecutionHead
            ],
        );
    }

    // `var x = 0\nx = 1` — covers PushLvalue, Assign, and Pop for expression statements.
    #[test]
    fn test_bytecode_var_assign() {
        let result = compile_stmts(vec![
            s(Statement::Declaration(Declaration {
                var_type: VariableType::Var,
                identifier: s(IdentifierDeclaration("x".into())),
                value: s(Expression::Literal(Literal::Int(0))),
            })),
            s(Statement::Expression(Expression::BinaryOperator(
                BinaryOperator {
                    op_type: BinaryOperatorType::Assign,
                    lhs: sb(Expression::IdentifierReference(IdentifierReference::Value(
                        "x".into(),
                    ))),
                    rhs: sb(Expression::Literal(Literal::Int(1))),
                },
            ))),
        ]);
        no_errors(&result);
        let sec = &result.bytecode.sections[1];
        assert_eq!(
            sec.instructions,
            vec![
                Instruction::PushInt { index: 0 }, // var x = 0
                Instruction::ConvertVar {
                    allow_stateful: false
                }, // create variable slot
                Instruction::PushLvalue {
                    stack_delta: -1,
                    force_ephemeral: false
                }, // lvalue of x (at pos 0, depth 1)
                Instruction::PushInt { index: 1 }, // rhs = 1
                Instruction::Assign,
                Instruction::Pop { count: 1 }, // discard assign result
                Instruction::EndOfExecutionHead,
            ],
        );
        assert_eq!(sec.int_pool, vec![0i64, 1i64]);
    }

    // `let f = |a| a` — verifies lambda body is Jump-over + body + MakeLambda
    // and that the prototype table is populated correctly.
    #[test]
    fn test_bytecode_simple_lambda() {
        let result = compile_stmts(vec![s(Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: s(IdentifierDeclaration("f".into())),
            value: s(Expression::LambdaDefinition(LambdaDefinition {
                args: vec![LambdaArg {
                    identifier: s(IdentifierDeclaration("a".into())),
                    default_value: None,
                    must_be_reference: false,
                }],
                body: s(LambdaBody::Inline(Box::new(
                    Expression::IdentifierReference(IdentifierReference::Value("a".into())),
                ))),
            })),
        }))]);
        no_errors(&result);
        let sec = &result.bytecode.sections[1];
        // [0] Jump{to:3}  [1] PushCopy{-1}  [2] Return{-1}  [3] MakeLambda{proto:0,cap:0}
        // [4] PushVar  [5] EndOfExecutionHead
        assert!(matches!(
            sec.instructions[0],
            Instruction::Jump { to: 3, .. }
        ));
        assert_eq!(
            sec.instructions[1],
            Instruction::PushCopy {
                copy_mode: CopyValueMode::Raw,
                stack_delta: -1,
                pop_tos: false,
            }
        );
        assert_eq!(sec.instructions[2], Instruction::Return { stack_delta: -1 });
        assert_eq!(
            sec.instructions[3],
            Instruction::MakeLambda {
                prototype_index: 0,
                capture_count: 0
            },
        );
        assert_eq!(
            sec.instructions[4],
            Instruction::ConvertVar {
                allow_stateful: false
            }
        );
        assert_eq!(sec.instructions[5], Instruction::EndOfExecutionHead);
        assert_eq!(sec.lambda_prototypes.len(), 1);
        assert_eq!(sec.lambda_prototypes[0].required_args, 1);
        assert_eq!(sec.lambda_prototypes[0].default_arg_count, 0);
        assert_eq!(sec.lambda_prototypes[0].reference_args, vec![false]);
        assert_eq!(sec.lambda_prototypes[0].ip, 1);
    }

    #[test]
    fn test_lambda_prototype_tracks_reference_args() {
        let result = compile_src("let f = |&x, y = 1, &z = 2| x");
        no_errors(&result);

        let sec = &result.bytecode.sections[1];
        assert_eq!(sec.lambda_prototypes.len(), 1);
        assert_eq!(
            sec.lambda_prototypes[0].reference_args,
            vec![true, false, true]
        );
    }

    #[test]
    fn test_lambda_invoke_does_not_box_plain_args() {
        let result = compile_src(
            "
            let f = |a| a
            let y = f(1)
        ",
        );
        no_errors(&result);

        let sec = &result.bytecode.sections[1];
        let convert_vars = sec
            .instructions
            .iter()
            .filter(|instr| matches!(instr, Instruction::ConvertVar { .. }))
            .count();
        assert_eq!(
            convert_vars, 2,
            "only the two let declarations should box values"
        );
    }
}
