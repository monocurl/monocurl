use parser::ast::{BinaryOperatorType, Expression, Literal, SpanTagged, Statement};

pub(super) fn expression_statement_has_no_effect(expr: &Expression) -> bool {
    !expression_has_effect(expr)
}

fn statements_have_effect(stmts: &[SpanTagged<Statement>]) -> bool {
    stmts.iter().any(|(_, stmt)| statement_has_effect(stmt))
}

fn statement_has_effect(stmt: &Statement) -> bool {
    match stmt {
        Statement::Break | Statement::Continue => false,
        Statement::Return(ret) => expression_has_effect(&ret.value.1),
        Statement::While(while_stmt) => {
            expression_has_effect(&while_stmt.condition.1)
                || statements_have_effect(&while_stmt.body.1)
        }
        Statement::For(for_stmt) => {
            expression_has_effect(&for_stmt.container.1) || statements_have_effect(&for_stmt.body.1)
        }
        Statement::If(if_stmt) => {
            expression_has_effect(&if_stmt.condition.1)
                || statements_have_effect(&if_stmt.if_block.1)
                || if_stmt
                    .else_block
                    .as_ref()
                    .is_some_and(|else_block| statements_have_effect(&else_block.1))
        }
        Statement::Declaration(decl) => expression_has_effect(&decl.value.1),
        Statement::Expression(expr) => expression_has_effect(expr),
        Statement::Play(_) => true,
        Statement::Print(_) => true,
    }
}

fn expression_has_effect(expr: &Expression) -> bool {
    match expr {
        Expression::Literal(literal) => match literal {
            Literal::Vector(items) => items.iter().any(|(_, expr)| expression_has_effect(expr)),
            Literal::Map(items) => items.iter().any(|((_, key), (_, value))| {
                expression_has_effect(key) || expression_has_effect(value)
            }),
            _ => false,
        },
        Expression::LambdaDefinition(_)
        | Expression::OperationDefinition(_)
        | Expression::Anim(_) => false,
        Expression::Block(block) => statements_have_effect(&block.body),
        Expression::BinaryOperator(binary) => {
            matches!(
                binary.op_type,
                BinaryOperatorType::Assign | BinaryOperatorType::DotAssign
            ) || expression_has_effect(&binary.lhs.1)
                || expression_has_effect(&binary.rhs.1)
        }
        Expression::UnaryPreOperator(unary) => expression_has_effect(&unary.operand.1),
        Expression::IdentifierReference(identifier) => {
            matches!(identifier, parser::ast::IdentifierReference::Reference(_))
        }
        Expression::Subscript(subscript) => {
            expression_has_effect(&subscript.base.1) || expression_has_effect(&subscript.index.1)
        }
        Expression::Property(property) => expression_has_effect(&property.base.1),
        Expression::LambdaInvocation(invocation) => {
            expression_has_effect(&invocation.lambda.1)
                || invocation
                    .arguments
                    .1
                    .iter()
                    .any(|(_, arg)| expression_has_effect(&arg.1))
        }
        Expression::OperatorInvocation(invocation) => {
            expression_has_effect(&invocation.operator.1)
                || invocation
                    .arguments
                    .1
                    .iter()
                    .any(|(_, arg)| expression_has_effect(&arg.1))
                || expression_has_effect(&invocation.operand.1)
        }
        Expression::NativeInvocation(invocation) => invocation
            .arguments
            .iter()
            .any(|(_, expr)| expression_has_effect(expr)),
    }
}
