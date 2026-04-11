use parser::ast::{Expression, IdentifierReference};

// returns true if evaluates to a stateful expression
pub(super) fn is_stateful(expr: &Expression) -> bool {
    match expr {
        Expression::IdentifierReference(IdentifierReference::StatefulReference(_)) => true,
        Expression::IdentifierReference(_) => false,
        Expression::Literal(_) => false,
        Expression::BinaryOperator(b) => is_stateful(&b.lhs.1) || is_stateful(&b.rhs.1),
        Expression::UnaryPreOperator(u) => is_stateful(&u.operand.1),
        Expression::Subscript(s) => is_stateful(&s.base.1) || is_stateful(&s.index.1),
        Expression::Property(p) => is_stateful(&p.base.1),
        Expression::LambdaInvocation(l) => {
            is_stateful(&l.lambda.1)
                || l.arguments.1.iter().any(|(_, a)| is_stateful(&a.1))
        }
        Expression::OperatorInvocation(o) => {
            is_stateful(&o.operator.1)
                || o.arguments.1.iter().any(|(_, a)| is_stateful(&a.1))
                || is_stateful(&o.operand.1)
        }
        Expression::NativeInvocation(n) => n.arguments.iter().any(|a| is_stateful(&a.1)),
        // lambdas/blocks/anims close over their environment at creation; stateful-ness
        // doesn't propagate through them at the call site
        Expression::LambdaDefinition(_)
        | Expression::OperationDefinition(_)
        | Expression::Block(_)
        | Expression::Anim(_) => false,
    }
}
