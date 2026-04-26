#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use lexer::{lexer::Lexer, token::Token};
    use structs::{rope::Rope, text::Span8};

    use crate::{
        ast::*,
        parser::{Diagnostic, Parser, PreparsedFile, SectionParser},
    };

    fn lex(content: &str) -> Vec<(Token, Span8)> {
        Lexer::token_stream(content.chars())
            .iter()
            .filter(|(tok, _)| tok != &Token::Whitespace && tok != &Token::Comment)
            .cloned()
            .collect()
    }

    fn parse_expr_test(content: &str) -> SpanTagged<Expression> {
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None, None);
        let ret = parser.parse_expr_best_effort();
        if !parser.artifacts.error_diagnostics.is_empty() {
            dbg!(&parser.artifacts.error_diagnostics);
        }
        ret
    }

    fn error_expr_test(content: &str) {
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None, None);
        parser.parse_expr_best_effort();
        assert!(!parser.artifacts.error_diagnostics.is_empty())
    }

    fn parse_stmt_test(content: &str) -> Result<SpanTagged<Statement>, ()> {
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None, None);
        let ret = parser.parse_statement();
        if ret.is_err() {
            dbg!(&parser.artifacts.error_diagnostics);
        }
        ret
    }

    fn parse_root_test(content: &str) -> (Vec<Section>, Vec<Diagnostic>) {
        let (bundle, artifacts) = Parser::parse_file(
            &HashMap::new(),
            PreparsedFile {
                imports: vec![],
                path: None,
                text_rope: Rope::from_str(content),
                root_import_span: None,
                tokens: lex(content),
                is_stdlib: false,
            },
            None,
        );
        (bundle.sections.clone(), artifacts.error_diagnostics)
    }

    // Literal tests
    #[test]
    fn test_integer_literal() {
        let result = parse_expr_test("42");
        let expected = Expression::Literal(Literal::Int(42));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_nil_literal() {
        let result = parse_expr_test("nil");
        let expected = Expression::Literal(Literal::Nil);
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_float_literal() {
        let result = parse_expr_test("3.14");
        if let Expression::Literal(Literal::Float(val)) = result.1 {
            assert!((val - 3.14).abs() < 0.0001);
        } else {
            panic!("Expected float literal");
        }
    }

    #[test]
    fn test_string_literal() {
        let result = parse_expr_test(r#""hello world""#);
        let expected = Expression::Literal(Literal::String("hello world".to_string()));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_string_with_escapes() {
        let result = parse_expr_test(r#""hello%nworld%t%"test%'""#);
        let expected = Expression::Literal(Literal::String("hello\nworld\t\"test'".to_string()));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_root_slide_title_is_parsed_from_same_line_string() {
        let (sections, errors) = parse_root_test("let a = 1\nslide \"Intro\"\nlet b = 2\n");
        assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].name, None);
        assert_eq!(sections[1].name, Some("Intro".to_string()));
        assert_eq!(sections[1].body.len(), 1);
    }

    #[test]
    fn test_root_slide_string_on_next_line_is_not_title() {
        let (sections, errors) = parse_root_test("slide\n\"Intro\"\n");
        assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[1].name, None);
        assert_eq!(
            sections[1].body,
            vec![(
                6..13,
                Statement::Expression(Expression::Literal(Literal::String("Intro".to_string())))
            )]
        );
    }

    #[test]
    fn test_root_slide_title_reports_malformed_string() {
        let (sections, errors) = parse_root_test("slide \"unterminated\nlet x = 1\n");
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[1].name, None);
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].title, "Illegal Slide Title");
        assert_eq!(errors[0].message, "Malformed string literal");
    }

    #[test]
    fn test_directional_literal_left() {
        let result = parse_expr_test("5l");
        let expected = Expression::Literal(Literal::Directional(DirectionalLiteral::Left(5.0)));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_directional_literal_up() {
        let result = parse_expr_test("3.5u");
        let expected = Expression::Literal(Literal::Directional(DirectionalLiteral::Up(3.5)));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_imaginary_literal() {
        let result = parse_expr_test("2i");
        let expected = Expression::Literal(Literal::Imaginary(2.0));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_degrees_literal() {
        let result = parse_expr_test("90dg");
        if let Expression::Literal(Literal::Float(val)) = result.1 {
            assert!((val - std::f64::consts::PI / 2.0).abs() < 0.0001);
        } else {
            panic!("Expected float literal");
        }
    }

    #[test]
    fn test_empty_vector() {
        let result = parse_expr_test("[]");
        let expected = Expression::Literal(Literal::Vector(vec![]));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_vector_literal() {
        let result = parse_expr_test("[1, 2, 3]");
        let expected = Expression::Literal(Literal::Vector(vec![
            (1..2, Expression::Literal(Literal::Int(1))),
            (4..5, Expression::Literal(Literal::Int(2))),
            (7..8, Expression::Literal(Literal::Int(3))),
        ]));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_empty_map() {
        let result = parse_expr_test("[->]");
        let expected = Expression::Literal(Literal::Map(vec![]));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_map_literal() {
        let result = parse_expr_test("[1 -> 2, 3 -> 4]");
        let expected = Expression::Literal(Literal::Map(vec![
            (
                (1..2, Expression::Literal(Literal::Int(1))),
                (6..7, Expression::Literal(Literal::Int(2))),
            ),
            (
                (9..10, Expression::Literal(Literal::Int(3))),
                (14..15, Expression::Literal(Literal::Int(4))),
            ),
        ]));
        assert_eq!(result.1, expected);
    }

    // Identifier tests
    #[test]
    fn test_identifier() {
        let result = parse_expr_test("foo");
        let expected =
            Expression::IdentifierReference(IdentifierReference::Value("foo".to_string()));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_reference() {
        let result = parse_expr_test("&bar");
        let expected =
            Expression::IdentifierReference(IdentifierReference::Reference("bar".to_string()));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_stateful_reference() {
        let result = parse_expr_test("$state_var");
        let expected = Expression::IdentifierReference(IdentifierReference::StatefulReference(
            "state_var".to_string(),
        ));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_dereference() {
        let result = parse_expr_test("*ptr");
        let expected = Expression::IdentifierReference(IdentifierReference::StatefulDereference(
            "ptr".to_string(),
        ));
        assert_eq!(result.1, expected);
    }

    // Binary operator tests
    #[test]
    fn test_addition() {
        let result = parse_expr_test("1 + 2");
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (0..1, Box::new(Expression::Literal(Literal::Int(1)))),
            op_type: BinaryOperatorType::Add,
            rhs: (4..5, Box::new(Expression::Literal(Literal::Int(2)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_append_operator() {
        let result = parse_expr_test("[1, 2] .. 3");
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (
                0..6,
                Box::new(Expression::Literal(Literal::Vector(vec![
                    (1..2, Expression::Literal(Literal::Int(1))),
                    (4..5, Expression::Literal(Literal::Int(2))),
                ]))),
            ),
            op_type: BinaryOperatorType::Append,
            rhs: (10..11, Box::new(Expression::Literal(Literal::Int(3)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_comparison() {
        let result = parse_expr_test("x < y");
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (
                0..1,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "x".to_string(),
                ))),
            ),
            op_type: BinaryOperatorType::Lt,
            rhs: (
                4..5,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "y".to_string(),
                ))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_operator_precedence() {
        let result = parse_expr_test("1 + 2 * 3");
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (0..1, Box::new(Expression::Literal(Literal::Int(1)))),
            op_type: BinaryOperatorType::Add,
            rhs: (
                4..9,
                Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (4..5, Box::new(Expression::Literal(Literal::Int(2)))),
                    op_type: BinaryOperatorType::Multiply,
                    rhs: (8..9, Box::new(Expression::Literal(Literal::Int(3)))),
                })),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_left_associativity_append() {
        let result = parse_expr_test("a .. b .. c");
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (
                0..6,
                Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        0..1,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "a".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Append,
                    rhs: (
                        5..6,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "b".to_string(),
                        ))),
                    ),
                })),
            ),
            op_type: BinaryOperatorType::Append,
            rhs: (
                10..11,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "c".to_string(),
                ))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    // Unary operator tests
    #[test]
    fn test_negation() {
        let result = parse_expr_test("-5");
        let expected = Expression::UnaryPreOperator(UnaryPreOperator {
            op_type: UnaryOperatorType::Negative,
            operand: (1..2, Box::new(Expression::Literal(Literal::Int(5)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_not() {
        let result = parse_expr_test("not x");
        let expected = Expression::UnaryPreOperator(UnaryPreOperator {
            op_type: UnaryOperatorType::Not,
            operand: (
                4..5,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "x".to_string(),
                ))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_binary_operator_allows_newline_before_rhs() {
        let result = parse_expr_test("1 +\n2");
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (0..1, Box::new(Expression::Literal(Literal::Int(1)))),
            op_type: BinaryOperatorType::Add,
            rhs: (4..5, Box::new(Expression::Literal(Literal::Int(2)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_unary_operator_allows_newline_before_operand() {
        let result = parse_expr_test("-\n5");
        let expected = Expression::UnaryPreOperator(UnaryPreOperator {
            op_type: UnaryOperatorType::Negative,
            operand: (2..3, Box::new(Expression::Literal(Literal::Int(5)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_double_negation() {
        let result = parse_expr_test("--5");
        let expected = Expression::UnaryPreOperator(UnaryPreOperator {
            op_type: UnaryOperatorType::Negative,
            operand: (
                1..3,
                Box::new(Expression::UnaryPreOperator(UnaryPreOperator {
                    op_type: UnaryOperatorType::Negative,
                    operand: (2..3, Box::new(Expression::Literal(Literal::Int(5)))),
                })),
            ),
        });
        assert_eq!(result.1, expected);
    }

    // Lambda tests
    #[test]
    fn test_simple_lambda() {
        let result = parse_expr_test("|x| x + 1");
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![LambdaArg {
                identifier: (1..2, IdentifierDeclaration("x".to_string())),
                default_value: None,
                must_be_reference: false,
            }],
            body: (
                4..9,
                LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        4..5,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Add,
                    rhs: (8..9, Box::new(Expression::Literal(Literal::Int(1)))),
                }))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_lambda_with_multiple_args() {
        let result = parse_expr_test("|x, y| x + y");
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![
                LambdaArg {
                    identifier: (1..2, IdentifierDeclaration("x".to_string())),
                    default_value: None,
                    must_be_reference: false,
                },
                LambdaArg {
                    identifier: (4..5, IdentifierDeclaration("y".to_string())),
                    default_value: None,
                    must_be_reference: false,
                },
            ],
            body: (
                7..12,
                LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        7..8,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Add,
                    rhs: (
                        11..12,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "y".to_string(),
                        ))),
                    ),
                }))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_lambda_with_default_arg() {
        let result = parse_expr_test("|x, y = 5| x + y");
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![
                LambdaArg {
                    identifier: (1..2, IdentifierDeclaration("x".to_string())),
                    default_value: None,
                    must_be_reference: false,
                },
                LambdaArg {
                    identifier: (4..5, IdentifierDeclaration("y".to_string())),
                    default_value: Some((8..9, Expression::Literal(Literal::Int(5)))),
                    must_be_reference: false,
                },
            ],
            body: (
                11..16,
                LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        11..12,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Add,
                    rhs: (
                        15..16,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "y".to_string(),
                        ))),
                    ),
                }))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_lambda_block_body() {
        let result = parse_expr_test("|x| { return x + 1 }");
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![LambdaArg {
                identifier: (1..2, IdentifierDeclaration("x".to_string())),
                default_value: None,
                must_be_reference: false,
            }],
            body: (
                4..20,
                LambdaBody::Block(vec![(
                    6..18,
                    Statement::Return(Return {
                        value: (
                            13..18,
                            Expression::BinaryOperator(BinaryOperator {
                                lhs: (
                                    13..14,
                                    Box::new(Expression::IdentifierReference(
                                        IdentifierReference::Value("x".to_string()),
                                    )),
                                ),
                                op_type: BinaryOperatorType::Add,
                                rhs: (17..18, Box::new(Expression::Literal(Literal::Int(1)))),
                            }),
                        ),
                    }),
                )]),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_empty_lambda() {
        let result = parse_expr_test("|| 42");
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![],
            body: (
                3..5,
                LambdaBody::Inline(Box::new(Expression::Literal(Literal::Int(42)))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_multiline_lambda_inline_body() {
        let result = parse_expr_test("|\nx,\ny = 5\n|\nx + y");
        let Expression::LambdaDefinition(LambdaDefinition { args, body }) = result.1 else {
            panic!("expected lambda definition");
        };

        assert_eq!(args.len(), 2);
        assert_eq!(args[0].identifier.1.0, "x");
        assert_eq!(args[0].default_value, None);
        assert_eq!(args[1].identifier.1.0, "y");
        match &args[1].default_value {
            Some((_span, Expression::Literal(Literal::Int(5)))) => {}
            _ => panic!("expected default value 5"),
        }

        match body.1 {
            LambdaBody::Inline(expr) => match *expr {
                Expression::BinaryOperator(BinaryOperator {
                    op_type: BinaryOperatorType::Add,
                    ..
                }) => {}
                other => panic!(
                    "expected inline add body, got {:?}",
                    std::mem::discriminant(&other)
                ),
            },
            _ => panic!("expected inline lambda body"),
        }
    }

    #[test]
    fn test_multiline_lambda_block_body() {
        let result = parse_expr_test("|\n|\n{\n    return 42\n}");
        let Expression::LambdaDefinition(LambdaDefinition { args, body }) = result.1 else {
            panic!("expected lambda definition");
        };

        assert!(args.is_empty());
        match body.1 {
            LambdaBody::Block(statements) => assert_eq!(statements.len(), 1),
            _ => panic!("expected block lambda body"),
        }
    }

    // Function invocation tests
    #[test]
    fn test_function_call_no_args() {
        let result = parse_expr_test("foo()");
        let expected = Expression::LambdaInvocation(LambdaInvocation {
            lambda: (
                0..3,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "foo".to_string(),
                ))),
            ),
            arguments: (3..5, vec![]),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_function_call_with_args() {
        let result = parse_expr_test("add(1, 2)");
        let expected = Expression::LambdaInvocation(LambdaInvocation {
            lambda: (
                0..3,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "add".to_string(),
                ))),
            ),
            arguments: (
                3..9,
                vec![
                    (None, (4..5, Expression::Literal(Literal::Int(1)))),
                    (None, (7..8, Expression::Literal(Literal::Int(2)))),
                ],
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_function_call_with_labeled_args() {
        let result = parse_expr_test("foo(x: 1, y: 2)");
        let expected = Expression::LambdaInvocation(LambdaInvocation {
            lambda: (
                0..3,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "foo".to_string(),
                ))),
            ),
            arguments: (
                3..15,
                vec![
                    (
                        Some((4..6, IdentifierDeclaration("x".to_string()))),
                        (7..8, Expression::Literal(Literal::Int(1))),
                    ),
                    (
                        Some((10..12, IdentifierDeclaration("y".to_string()))),
                        (13..14, Expression::Literal(Literal::Int(2))),
                    ),
                ],
            ),
        });
        assert_eq!(result.1, expected);
    }

    // Operator invocation tests
    #[test]
    fn test_operator_invocation() {
        let result = parse_expr_test("op{x, y} z");
        let expected = Expression::OperatorInvocation(OperatorInvocation {
            operator: (
                0..2,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "op".to_string(),
                ))),
            ),
            arguments: (
                2..8,
                vec![
                    (
                        None,
                        (
                            3..4,
                            Expression::IdentifierReference(IdentifierReference::Value(
                                "x".to_string(),
                            )),
                        ),
                    ),
                    (
                        None,
                        (
                            6..7,
                            Expression::IdentifierReference(IdentifierReference::Value(
                                "y".to_string(),
                            )),
                        ),
                    ),
                ],
            ),
            operand: (
                9..10,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "z".to_string(),
                ))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_operator_invocation_allows_newline_before_operand() {
        let result = parse_expr_test("fill{CLEAR}\nstroke{RED}\nCircle(ORIGIN, radius: 1)");
        assert!(matches!(result.1, Expression::OperatorInvocation(_)));
    }

    #[test]
    fn test_function_invocation_stops_at_newline_before_paren() {
        let result = parse_expr_test("foo\n(1)");
        assert!(matches!(
            result.1,
            Expression::IdentifierReference(IdentifierReference::Value(ref name)) if name == "foo"
        ));
    }

    #[test]
    fn test_operator_invocation_complex() {
        let result = parse_expr_test("derivative{n: 2} (x * x)");
        let expected = Expression::OperatorInvocation(OperatorInvocation {
            operator: (
                0..10,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "derivative".to_string(),
                ))),
            ),
            arguments: (
                10..16,
                vec![(
                    Some((11..13, IdentifierDeclaration("n".to_string()))),
                    (14..15, Expression::Literal(Literal::Int(2))),
                )],
            ),
            operand: (
                17..24,
                Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        18..19,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Multiply,
                    rhs: (
                        22..23,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".to_string(),
                        ))),
                    ),
                })),
            ),
        });
        assert_eq!(result.1, expected);
    }

    // Property access tests
    #[test]
    fn test_property_access() {
        let result = parse_expr_test("obj.field");
        let expected = Expression::Property(Property {
            base: (
                0..3,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "obj".to_string(),
                ))),
            ),
            attribute: (4..9, IdentifierReference::Value("field".to_string())),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_chained_property_access() {
        let result = parse_expr_test("obj.a.b.c");
        let expected = Expression::Property(Property {
            base: (
                0..7,
                Box::new(Expression::Property(Property {
                    base: (
                        0..5,
                        Box::new(Expression::Property(Property {
                            base: (
                                0..3,
                                Box::new(Expression::IdentifierReference(
                                    IdentifierReference::Value("obj".to_string()),
                                )),
                            ),
                            attribute: (4..5, IdentifierReference::Value("a".to_string())),
                        })),
                    ),
                    attribute: (6..7, IdentifierReference::Value("b".to_string())),
                })),
            ),
            attribute: (8..9, IdentifierReference::Value("c".to_string())),
        });
        assert_eq!(result.1, expected);
    }

    // Subscript tests
    #[test]
    fn test_subscript() {
        let result = parse_expr_test("arr[0]");
        let expected = Expression::Subscript(Subscript {
            base: (
                0..3,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "arr".to_string(),
                ))),
            ),
            index: (4..5, Box::new(Expression::Literal(Literal::Int(0)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_chained_subscript() {
        let result = parse_expr_test("matrix[i][j]");
        let expected = Expression::Subscript(Subscript {
            base: (
                0..9,
                Box::new(Expression::Subscript(Subscript {
                    base: (
                        0..6,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "matrix".to_string(),
                        ))),
                    ),
                    index: (
                        7..8,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "i".to_string(),
                        ))),
                    ),
                })),
            ),
            index: (
                10..11,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "j".to_string(),
                ))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    // Parentheses tests
    #[test]
    fn test_parentheses() {
        let result = parse_expr_test("(1 + 2) * 3");
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (
                0..7,
                Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (1..2, Box::new(Expression::Literal(Literal::Int(1)))),
                    op_type: BinaryOperatorType::Add,
                    rhs: (5..6, Box::new(Expression::Literal(Literal::Int(2)))),
                })),
            ),
            op_type: BinaryOperatorType::Multiply,
            rhs: (10..11, Box::new(Expression::Literal(Literal::Int(3)))),
        });
        assert_eq!(result.1, expected);
    }

    // Statement tests
    #[test]
    fn test_let_declaration() {
        let result = parse_stmt_test("let x = 5").unwrap();
        let expected = Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: (4..5, IdentifierDeclaration("x".to_string())),
            value: (8..9, Expression::Literal(Literal::Int(5))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_var_declaration() {
        let result = parse_stmt_test("var y = 10").unwrap();
        let expected = Statement::Declaration(Declaration {
            var_type: VariableType::Var,
            identifier: (4..5, IdentifierDeclaration("y".to_string())),
            value: (8..10, Expression::Literal(Literal::Int(10))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_while_loop() {
        let result = parse_stmt_test("while (x < 10) { x = x + 1 }").unwrap();
        let expected = Statement::While(While {
            condition: (
                7..13,
                Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        7..8,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Lt,
                    rhs: (11..13, Box::new(Expression::Literal(Literal::Int(10)))),
                }),
            ),
            body: (
                15..28,
                vec![(
                    17..26,
                    Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                        lhs: (
                            17..18,
                            Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                                "x".to_string(),
                            ))),
                        ),
                        op_type: BinaryOperatorType::Assign,
                        rhs: (
                            21..26,
                            Box::new(Expression::BinaryOperator(BinaryOperator {
                                lhs: (
                                    21..22,
                                    Box::new(Expression::IdentifierReference(
                                        IdentifierReference::Value("x".to_string()),
                                    )),
                                ),
                                op_type: BinaryOperatorType::Add,
                                rhs: (25..26, Box::new(Expression::Literal(Literal::Int(1)))),
                            })),
                        ),
                    })),
                )],
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_for_loop() {
        let result = parse_stmt_test("for (i in [1, 2, 3]) { print i }").unwrap();
        let expected = Statement::For(For {
            var_name: (5..6, IdentifierDeclaration("i".to_string())),
            container: (
                10..19,
                Expression::Literal(Literal::Vector(vec![
                    (11..12, Expression::Literal(Literal::Int(1))),
                    (14..15, Expression::Literal(Literal::Int(2))),
                    (17..18, Expression::Literal(Literal::Int(3))),
                ])),
            ),
            body: (
                21..32,
                vec![(
                    23..30,
                    Statement::Print(Print {
                        value: (
                            29..30,
                            Expression::IdentifierReference(IdentifierReference::Value(
                                "i".to_string(),
                            )),
                        ),
                    }),
                )],
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_print_statement() {
        let result = parse_stmt_test("print x + 1").unwrap();
        let expected = Statement::Print(Print {
            value: (
                6..11,
                Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        6..7,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Add,
                    rhs: (10..11, Box::new(Expression::Literal(Literal::Int(1)))),
                }),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_if_statement() {
        let result = parse_stmt_test("if (x > 0) { y = 1 }").unwrap();
        let expected = Statement::If(If {
            condition: (
                4..9,
                Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        4..5,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "x".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Gt,
                    rhs: (8..9, Box::new(Expression::Literal(Literal::Int(0)))),
                }),
            ),
            if_block: (
                11..20,
                vec![(
                    13..18,
                    Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                        lhs: (
                            13..14,
                            Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                                "y".to_string(),
                            ))),
                        ),
                        op_type: BinaryOperatorType::Assign,
                        rhs: (17..18, Box::new(Expression::Literal(Literal::Int(1)))),
                    })),
                )],
            ),
            else_block: None,
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_if_else_statement() {
        let result = parse_stmt_test("if (x > 0) { y = 1 } else { y = -1 }").unwrap();
        if let Statement::If(if_stmt) = &result.1 {
            assert!(if_stmt.else_block.is_some());
            if let Some((_, else_stmts)) = &if_stmt.else_block {
                assert_eq!(else_stmts.len(), 1);
            }
        } else {
            panic!("Expected if statement");
        }
    }

    #[test]
    fn test_if_else_if() {
        let result = parse_stmt_test("if (x > 0) { y = 1 } else if (x < 0) { y = -1 }").unwrap();
        if let Statement::If(if_stmt) = &result.1 {
            assert!(if_stmt.else_block.is_some());
            if let Some((_, else_stmts)) = &if_stmt.else_block {
                assert_eq!(else_stmts.len(), 1);
                assert!(matches!(else_stmts[0].1, Statement::If(_)));
            }
        } else {
            panic!("Expected if statement");
        }
    }

    #[test]
    fn test_return_statement() {
        let result = parse_expr_test("|| { return 42 }");
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![],
            body: (
                3..16,
                LambdaBody::Block(vec![(
                    5..14,
                    Statement::Return(Return {
                        value: (12..14, Expression::Literal(Literal::Int(42))),
                    }),
                )]),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_break_statement() {
        let result = parse_stmt_test("while (1) { break }").unwrap();
        if let Statement::While(w) = result.1 {
            assert_eq!(w.body.1.len(), 1);
            assert!(matches!(w.body.1[0].1, Statement::Break));
        } else {
            panic!("Expected while statement");
        }
    }

    #[test]
    fn test_continue_statement() {
        let result = parse_stmt_test("for (i in items) { continue }").unwrap();
        if let Statement::For(f) = result.1 {
            assert_eq!(f.body.1.len(), 1);
            assert!(matches!(f.body.1[0].1, Statement::Continue));
        } else {
            panic!("Expected for statement");
        }
    }

    #[test]
    fn test_dot_assign_statement() {
        let result = parse_expr_test("block {. x + 1}");
        let expected = Expression::Block(Block {
            body: vec![(
                7..14,
                Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        7..8,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "_".into(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::DotAssign,
                    rhs: (
                        9..14,
                        Box::new(Expression::BinaryOperator(BinaryOperator {
                            lhs: (
                                9..10,
                                Box::new(Expression::IdentifierReference(
                                    IdentifierReference::Value("x".into()),
                                )),
                            ),
                            op_type: BinaryOperatorType::Add,
                            rhs: (13..14, Box::new(Expression::Literal(Literal::Int(1)))),
                        })),
                    ),
                })),
            )],
        });
        assert_eq!(result.1, expected);
    }

    // Complex expression tests
    #[test]
    fn test_complex_nested_expression() {
        let result = parse_expr_test("(a + b) * (c - d)");
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (
                0..7,
                Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        1..2,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "a".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Add,
                    rhs: (
                        5..6,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "b".to_string(),
                        ))),
                    ),
                })),
            ),
            op_type: BinaryOperatorType::Multiply,
            rhs: (
                10..17,
                Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (
                        11..12,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "c".to_string(),
                        ))),
                    ),
                    op_type: BinaryOperatorType::Subtract,
                    rhs: (
                        15..16,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "d".to_string(),
                        ))),
                    ),
                })),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_lambda_invocation_chain() {
        let result = parse_expr_test("f(x)(y)(z)");
        let expected = Expression::LambdaInvocation(LambdaInvocation {
            lambda: (
                0..7,
                Box::new(Expression::LambdaInvocation(LambdaInvocation {
                    lambda: (
                        0..4,
                        Box::new(Expression::LambdaInvocation(LambdaInvocation {
                            lambda: (
                                0..1,
                                Box::new(Expression::IdentifierReference(
                                    IdentifierReference::Value("f".to_string()),
                                )),
                            ),
                            arguments: (
                                1..4,
                                vec![(
                                    None,
                                    (
                                        2..3,
                                        Expression::IdentifierReference(
                                            IdentifierReference::Value("x".to_string()),
                                        ),
                                    ),
                                )],
                            ),
                        })),
                    ),
                    arguments: (
                        4..7,
                        vec![(
                            None,
                            (
                                5..6,
                                Expression::IdentifierReference(IdentifierReference::Value(
                                    "y".to_string(),
                                )),
                            ),
                        )],
                    ),
                })),
            ),
            arguments: (
                7..10,
                vec![(
                    None,
                    (
                        8..9,
                        Expression::IdentifierReference(IdentifierReference::Value(
                            "z".to_string(),
                        )),
                    ),
                )],
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_mixed_postfix_operations() {
        let result = parse_expr_test("obj.method()[0].field");
        let expected = Expression::Property(Property {
            base: (
                0..15,
                Box::new(Expression::Subscript(Subscript {
                    base: (
                        0..12,
                        Box::new(Expression::LambdaInvocation(LambdaInvocation {
                            lambda: (
                                0..10,
                                Box::new(Expression::Property(Property {
                                    base: (
                                        0..3,
                                        Box::new(Expression::IdentifierReference(
                                            IdentifierReference::Value("obj".to_string()),
                                        )),
                                    ),
                                    attribute: (
                                        4..10,
                                        IdentifierReference::Value("method".to_string()),
                                    ),
                                })),
                            ),
                            arguments: (10..12, vec![]),
                        })),
                    ),
                    index: (13..14, Box::new(Expression::Literal(Literal::Int(0)))),
                })),
            ),
            attribute: (16..21, IdentifierReference::Value("field".to_string())),
        });
        assert_eq!(result.1, expected);
    }

    // Operator definition test
    #[test]
    fn test_operator_definition() {
        let result = parse_expr_test("operator |x, y| x + y");
        let expected = Expression::OperationDefinition(OperatorDefinition {
            lambda: (
                9..21,
                Box::new(Expression::LambdaDefinition(LambdaDefinition {
                    args: vec![
                        LambdaArg {
                            identifier: (10..11, IdentifierDeclaration("x".to_string())),
                            default_value: None,
                            must_be_reference: false,
                        },
                        LambdaArg {
                            identifier: (13..14, IdentifierDeclaration("y".to_string())),
                            default_value: None,
                            must_be_reference: false,
                        },
                    ],
                    body: (
                        16..21,
                        LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                            lhs: (
                                16..17,
                                Box::new(Expression::IdentifierReference(
                                    IdentifierReference::Value("x".to_string()),
                                )),
                            ),
                            op_type: BinaryOperatorType::Add,
                            rhs: (
                                20..21,
                                Box::new(Expression::IdentifierReference(
                                    IdentifierReference::Value("y".to_string()),
                                )),
                            ),
                        }))),
                    ),
                })),
            ),
        });
        assert_eq!(result.1, expected);
    }

    // Block and Anim tests
    #[test]
    fn test_block_expression() {
        let result = parse_expr_test("block { let x = 5\n x * 2 }");
        let expected = Expression::Block(Block {
            body: vec![
                (
                    8..17,
                    Statement::Declaration(Declaration {
                        var_type: VariableType::Let,
                        identifier: (12..13, IdentifierDeclaration("x".to_string())),
                        value: (16..17, Expression::Literal(Literal::Int(5))),
                    }),
                ),
                (
                    19..24,
                    Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                        lhs: (
                            19..20,
                            Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                                "x".to_string(),
                            ))),
                        ),
                        op_type: BinaryOperatorType::Multiply,
                        rhs: (23..24, Box::new(Expression::Literal(Literal::Int(2)))),
                    })),
                ),
            ],
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_anim_expression() {
        let result = parse_expr_test("anim { play circle }");
        let expected = Expression::Anim(Anim {
            body: vec![(
                7..18,
                Statement::Play(Play {
                    animations: (
                        12..18,
                        Expression::IdentifierReference(IdentifierReference::Value(
                            "circle".to_string(),
                        )),
                    ),
                }),
            )],
        });
        assert_eq!(result.1, expected);
    }

    // Multiline tests
    #[test]
    fn test_multiline_statement_list() {
        let content = "let x = 1\nlet y = 2\nlet z = 3";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None, None);
        let result = parser.parse_statement_list();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_semicolon_separated_statements() {
        let content = "let x = 1; let y = 2; let z = 3";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None, None);
        let result = parser.parse_statement_list();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_multiline_operator_chain_in_declaration() {
        let result =
            parse_stmt_test("mesh x = \n    fill{CLEAR}\n    stroke{RED}\n    Circle(radius: 1)")
                .unwrap();

        let Statement::Declaration(decl) = result.1 else {
            panic!("expected declaration");
        };
        assert_eq!(decl.identifier.1.0, "x");
        assert!(matches!(decl.value.1, Expression::OperatorInvocation(_)));
    }

    #[test]
    fn test_newline_before_dot_starts_new_statement() {
        let content = "block {\n    let x = 1\n    . x\n}";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None, None);
        let result = parser.parse_expr_best_effort();

        let Expression::Block(block) = result.1 else {
            panic!("expected block");
        };

        assert_eq!(block.body.len(), 2);
        assert!(matches!(block.body[0].1, Statement::Declaration(_)));
        assert!(matches!(
            block.body[1].1,
            Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                op_type: BinaryOperatorType::DotAssign,
                ..
            }))
        ));
    }

    #[test]
    fn test_newline_before_dot_after_operator_chain_in_if_starts_new_statement() {
        let content = "block {\n    if (labels) {\n        . w{} centered_at{} Tex(\"A\", 1)\n        . w{} centered_at{} Tex(\"B\", 1)\n    }\n}";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None, None);
        let result = parser.parse_expr_best_effort();

        let Expression::Block(block) = result.1 else {
            panic!("expected block");
        };
        assert_eq!(block.body.len(), 1);

        let Statement::If(if_stmt) = &block.body[0].1 else {
            panic!("expected if statement");
        };
        assert_eq!(if_stmt.if_block.1.len(), 2);
        for statement in &if_stmt.if_block.1 {
            assert!(matches!(
                statement.1,
                Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                    op_type: BinaryOperatorType::DotAssign,
                    ..
                }))
            ));
        }
    }

    #[test]
    fn test_multiline_grouped_expression_allows_newline_before_close_paren() {
        let result = parse_stmt_test("let x = (\n    1\n)").unwrap();

        let Statement::Declaration(decl) = result.1 else {
            panic!("expected declaration");
        };
        assert_eq!(decl.identifier.1.0, "x");
        assert!(matches!(decl.value.1, Expression::Literal(Literal::Int(1))));
    }

    #[test]
    fn test_multiline_grouped_operator_chain_allows_newline_before_close_paren() {
        let result = parse_stmt_test(
            "mesh x = (\n    fill{CLEAR}\n    stroke{RED}\n    Circle(radius: 1)\n)",
        )
        .unwrap();

        let Statement::Declaration(decl) = result.1 else {
            panic!("expected declaration");
        };
        assert_eq!(decl.identifier.1.0, "x");
        assert!(matches!(decl.value.1, Expression::OperatorInvocation(_)));
    }

    // Edge cases
    #[test]
    fn test_nested_vectors() {
        let result = parse_expr_test("[[1, 2], [3, 4]]");
        let expected = Expression::Literal(Literal::Vector(vec![
            (
                1..7,
                Expression::Literal(Literal::Vector(vec![
                    (2..3, Expression::Literal(Literal::Int(1))),
                    (5..6, Expression::Literal(Literal::Int(2))),
                ])),
            ),
            (
                9..15,
                Expression::Literal(Literal::Vector(vec![
                    (10..11, Expression::Literal(Literal::Int(3))),
                    (13..14, Expression::Literal(Literal::Int(4))),
                ])),
            ),
        ]));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_nested_vector_destructure_assignment() {
        let content = "[c, [d, a]] = [a, [b, d]]";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None, None);
        let result = parser.parse_statement().unwrap();
        assert!(
            parser.artifacts.error_diagnostics.is_empty(),
            "{:?}",
            parser.artifacts.error_diagnostics
        );

        let Statement::Expression(Expression::BinaryOperator(op)) = result.1 else {
            panic!("expected assignment expression");
        };
        assert_eq!(op.op_type, BinaryOperatorType::Assign);
        assert!(matches!(
            op.lhs.1.as_ref(),
            Expression::Literal(Literal::Vector(_))
        ));
        assert!(matches!(
            op.rhs.1.as_ref(),
            Expression::Literal(Literal::Vector(_))
        ));
    }

    #[test]
    fn test_multiline_destructure_starts_new_statement() {
        let content = "var d = 1\n[a, b] = [b, a]";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None, None);
        let result = parser.parse_statement_list();

        assert!(
            parser.artifacts.error_diagnostics.is_empty(),
            "{:?}",
            parser.artifacts.error_diagnostics
        );
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0].1, Statement::Declaration(_)));
        assert!(matches!(
            result[1].1,
            Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                op_type: BinaryOperatorType::Assign,
                ..
            }))
        ));
    }

    #[test]
    fn test_map_with_complex_values() {
        let result = parse_expr_test("[1 -> [1, 2], 2 -> [3, 4]]");
        let expected = Expression::Literal(Literal::Map(vec![
            (
                (1..2, Expression::Literal(Literal::Int(1))),
                (
                    6..12,
                    Expression::Literal(Literal::Vector(vec![
                        (7..8, Expression::Literal(Literal::Int(1))),
                        (10..11, Expression::Literal(Literal::Int(2))),
                    ])),
                ),
            ),
            (
                (14..15, Expression::Literal(Literal::Int(2))),
                (
                    19..25,
                    Expression::Literal(Literal::Vector(vec![
                        (20..21, Expression::Literal(Literal::Int(3))),
                        (23..24, Expression::Literal(Literal::Int(4))),
                    ])),
                ),
            ),
        ]));
        assert_eq!(result.1, expected);
    }

    // Complex integration tests
    #[test]
    fn test_complex_lambda_with_nested_blocks() {
        let result = parse_expr_test("|n| anim { for (i in [1, 2, 3]) { play circle } }");
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![LambdaArg {
                identifier: (1..2, IdentifierDeclaration("n".to_string())),
                default_value: None,
                must_be_reference: false,
            }],
            body: (
                4..49,
                LambdaBody::Inline(Box::new(Expression::Anim(Anim {
                    body: vec![(
                        11..47,
                        Statement::For(For {
                            var_name: (16..17, IdentifierDeclaration("i".to_string())),
                            container: (
                                21..30,
                                Expression::Literal(Literal::Vector(vec![
                                    (22..23, Expression::Literal(Literal::Int(1))),
                                    (25..26, Expression::Literal(Literal::Int(2))),
                                    (28..29, Expression::Literal(Literal::Int(3))),
                                ])),
                            ),
                            body: (
                                32..47,
                                vec![(
                                    34..45,
                                    Statement::Play(Play {
                                        animations: (
                                            39..45,
                                            Expression::IdentifierReference(
                                                IdentifierReference::Value("circle".to_string()),
                                            ),
                                        ),
                                    }),
                                )],
                            ),
                        }),
                    )],
                }))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_operator_invocation_with_lambda() {
        let result = parse_expr_test("map{|x| x * 2} [1, 2, 3]");
        let expected = Expression::OperatorInvocation(OperatorInvocation {
            operator: (
                0..3,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "map".to_string(),
                ))),
            ),
            arguments: (
                3..14,
                vec![(
                    None,
                    (
                        4..13,
                        Expression::LambdaDefinition(LambdaDefinition {
                            args: vec![LambdaArg {
                                identifier: (5..6, IdentifierDeclaration("x".to_string())),
                                default_value: None,
                                must_be_reference: false,
                            }],
                            body: (
                                8..13,
                                LambdaBody::Inline(Box::new(Expression::BinaryOperator(
                                    BinaryOperator {
                                        lhs: (
                                            8..9,
                                            Box::new(Expression::IdentifierReference(
                                                IdentifierReference::Value("x".to_string()),
                                            )),
                                        ),
                                        op_type: BinaryOperatorType::Multiply,
                                        rhs: (
                                            12..13,
                                            Box::new(Expression::Literal(Literal::Int(2))),
                                        ),
                                    },
                                ))),
                            ),
                        }),
                    ),
                )],
            ),
            operand: (
                15..24,
                Box::new(Expression::Literal(Literal::Vector(vec![
                    (16..17, Expression::Literal(Literal::Int(1))),
                    (19..20, Expression::Literal(Literal::Int(2))),
                    (22..23, Expression::Literal(Literal::Int(3))),
                ]))),
            ),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_complex_property_chain_with_calls() {
        let result = parse_expr_test("obj.data.process(x).result[0]");
        let expected = Expression::Subscript(Subscript {
            base: (
                0..26,
                Box::new(Expression::Property(Property {
                    base: (
                        0..19,
                        Box::new(Expression::LambdaInvocation(LambdaInvocation {
                            lambda: (
                                0..16,
                                Box::new(Expression::Property(Property {
                                    base: (
                                        0..8,
                                        Box::new(Expression::Property(Property {
                                            base: (
                                                0..3,
                                                Box::new(Expression::IdentifierReference(
                                                    IdentifierReference::Value("obj".to_string()),
                                                )),
                                            ),
                                            attribute: (
                                                4..8,
                                                IdentifierReference::Value("data".to_string()),
                                            ),
                                        })),
                                    ),
                                    attribute: (
                                        9..16,
                                        IdentifierReference::Value("process".to_string()),
                                    ),
                                })),
                            ),
                            arguments: (
                                16..19,
                                vec![(
                                    None,
                                    (
                                        17..18,
                                        Expression::IdentifierReference(
                                            IdentifierReference::Value("x".to_string()),
                                        ),
                                    ),
                                )],
                            ),
                        })),
                    ),
                    attribute: (20..26, IdentifierReference::Value("result".to_string())),
                })),
            ),
            index: (27..28, Box::new(Expression::Literal(Literal::Int(0)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_nested_operator_invocations() {
        let result = parse_expr_test("fold{|a, b| a + b, 0} map{|x| x * 2} data");
        let expected = Expression::OperatorInvocation(OperatorInvocation {
            operator: (
                0..4,
                Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                    "fold".to_string(),
                ))),
            ),
            arguments: (
                4..21,
                vec![
                    (
                        None,
                        (
                            5..17,
                            Expression::LambdaDefinition(LambdaDefinition {
                                args: vec![
                                    LambdaArg {
                                        identifier: (6..7, IdentifierDeclaration("a".to_string())),
                                        default_value: None,
                                        must_be_reference: false,
                                    },
                                    LambdaArg {
                                        identifier: (9..10, IdentifierDeclaration("b".to_string())),
                                        default_value: None,
                                        must_be_reference: false,
                                    },
                                ],
                                body: (
                                    12..17,
                                    LambdaBody::Inline(Box::new(Expression::BinaryOperator(
                                        BinaryOperator {
                                            lhs: (
                                                12..13,
                                                Box::new(Expression::IdentifierReference(
                                                    IdentifierReference::Value("a".to_string()),
                                                )),
                                            ),
                                            op_type: BinaryOperatorType::Add,
                                            rhs: (
                                                16..17,
                                                Box::new(Expression::IdentifierReference(
                                                    IdentifierReference::Value("b".to_string()),
                                                )),
                                            ),
                                        },
                                    ))),
                                ),
                            }),
                        ),
                    ),
                    (None, (19..20, Expression::Literal(Literal::Int(0)))),
                ],
            ),
            operand: (
                22..41,
                Box::new(Expression::OperatorInvocation(OperatorInvocation {
                    operator: (
                        22..25,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "map".to_string(),
                        ))),
                    ),
                    arguments: (
                        25..36,
                        vec![(
                            None,
                            (
                                26..35,
                                Expression::LambdaDefinition(LambdaDefinition {
                                    args: vec![LambdaArg {
                                        identifier: (
                                            27..28,
                                            IdentifierDeclaration("x".to_string()),
                                        ),
                                        default_value: None,
                                        must_be_reference: false,
                                    }],
                                    body: (
                                        30..35,
                                        LambdaBody::Inline(Box::new(Expression::BinaryOperator(
                                            BinaryOperator {
                                                lhs: (
                                                    30..31,
                                                    Box::new(Expression::IdentifierReference(
                                                        IdentifierReference::Value("x".to_string()),
                                                    )),
                                                ),
                                                op_type: BinaryOperatorType::Multiply,
                                                rhs: (
                                                    34..35,
                                                    Box::new(Expression::Literal(Literal::Int(2))),
                                                ),
                                            },
                                        ))),
                                    ),
                                }),
                            ),
                        )],
                    ),
                    operand: (
                        37..41,
                        Box::new(Expression::IdentifierReference(IdentifierReference::Value(
                            "data".to_string(),
                        ))),
                    ),
                })),
            ),
        });
        assert_eq!(result.1, expected);
    }

    // Error cases
    #[test]
    fn test_error_unmatched_paren() {
        error_expr_test("(1 + 2");
    }

    #[test]
    fn test_error_empty_char_literal() {
        error_expr_test("''");
    }

    #[test]
    fn test_error_invalid_escape() {
        error_expr_test(r#""hello%z""#);
    }

    #[test]
    fn test_error_missing_operand() {
        error_expr_test("1 +");
    }

    #[test]
    fn test_error_invalid_identifier_start_with_number() {
        error_expr_test("let 123x = 5");
    }

    #[test]
    fn test_error_break_outside_loop() {
        let content = "break";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Init, None, None);
        let _ = parser.parse_statement();
        assert!(!parser.artifacts.error_diagnostics.is_empty());
    }

    #[test]
    fn test_error_return_outside_function() {
        let content = "return 5";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Init, None, None);
        let _ = parser.parse_statement();
        assert!(!parser.artifacts.error_diagnostics.is_empty());
    }

    #[test]
    fn test_error_play_outside_anim() {
        let content = "play animation";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Init, None, None);
        let _ = parser.parse_statement();
        assert!(!parser.artifacts.error_diagnostics.is_empty());
    }

    #[test]
    fn test_root_statement_autocomplete_includes_slide() {
        let content = "foo";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Init, None, Some(1));
        let _ = parser.parse_statement();
        assert!(parser.autocomplete_possibilities().contains(&Token::Slide));
    }

    #[test]
    fn test_error_ambiguous_vector_map() {
        error_expr_test("[1, 2 -> 3]");
    }

    #[test]
    fn test_error_missing_lambda_argument() {
        error_expr_test("|x,| x + 1");
    }

    #[test]
    fn test_error_weird_lambda_argument() {
        error_expr_test("|x g, y| x + 1");
    }
}
