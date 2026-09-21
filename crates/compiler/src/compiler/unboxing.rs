//! decides which `let`/`var` locals can live as plain stack values instead of
//! heap slots.
//!
//! a local only needs a slot when something takes a reference into it: a mutable
//! subscript or attribute write, an append-assign, an `&` reference, a stateful
//! reference, a destructuring binding, or -- for `var` only -- being captured by
//! a block or anim, which capture mutable variables by lvalue.
//!
//! the scan is deliberately name-based and covers a whole section including its
//! nested closures, so shadowing can only ever make it box more than strictly
//! necessary. missing a use would be unsound, over-reporting one merely costs a
//! slot.

use std::collections::HashSet;

use parser::ast::{
    BinaryOperator, BinaryOperatorType, BindingPattern, Expression, IdentifierReference, LambdaBody,
    Literal, SpanTagged, Statement,
};

use super::ident_ref_name;

#[derive(Default)]
pub(super) struct SlotRequirements {
    /// names that something takes a reference into, whatever their type
    referenced: HashSet<String>,
    /// names appearing inside a block or anim body, which capture `var` by lvalue
    in_closure: HashSet<String>,
}

impl SlotRequirements {
    /// `let` captures are copied by value, so only a `var` is pinned by a closure
    pub(super) fn needs_slot(&self, name: &str, mutable: bool) -> bool {
        self.referenced.contains(name) || (mutable && self.in_closure.contains(name))
    }
}

pub(super) fn scan(stmts: &[SpanTagged<Statement>]) -> SlotRequirements {
    let mut out = SlotRequirements::default();
    let mut scanner = Scanner {
        out: &mut out,
        closure_depth: 0,
    };
    scanner.stmts(stmts);
    out
}

struct Scanner<'a> {
    out: &'a mut SlotRequirements,
    closure_depth: usize,
}

impl Scanner<'_> {
    fn reference(&mut self, name: &str) {
        self.out.referenced.insert(name.to_string());
    }

    /// the variable an assignment target ultimately writes through
    fn target_root(&mut self, expr: &Expression) {
        match expr {
            Expression::IdentifierReference(ir) => {
                let name = ident_ref_name(ir).to_string();
                self.reference(&name);
            }
            Expression::Subscript(s) => self.target_root(&s.base.1),
            Expression::Property(p) => self.target_root(&p.base.1),
            Expression::Literal(Literal::List(elements)) => {
                for element in elements {
                    self.target_root(&element.1);
                }
            }
            other => self.expr(other),
        }
    }

    fn pattern(&mut self, pattern: &BindingPattern) {
        // a destructuring binding is lowered through nil slots and Assign
        if let BindingPattern::List(elements) = pattern {
            for (_, identifier) in elements {
                self.reference(&identifier.0);
            }
        }
    }

    fn stmts(&mut self, stmts: &[SpanTagged<Statement>]) {
        for (_, stmt) in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Expression(e) => self.expr(e),
            Statement::Declaration(d) => {
                self.expr(&d.value.1);
                self.pattern(&d.pattern.1);
            }
            Statement::Return(r) => self.expr(&r.value.1),
            Statement::While(w) => {
                self.expr(&w.condition.1);
                self.stmts(&w.body.1);
            }
            Statement::For(f) => {
                self.expr(&f.container.1);
                self.pattern(&f.pattern.1);
                self.stmts(&f.body.1);
            }
            Statement::If(i) => {
                self.expr(&i.condition.1);
                self.stmts(&i.if_block.1);
                if let Some(else_block) = &i.else_block {
                    self.stmts(&else_block.1);
                }
            }
            Statement::Play(p) => self.expr(&p.animations.1),
            Statement::Print(p) => self.expr(&p.value.1),
            Statement::Break | Statement::Continue => {}
        }
    }

    fn binary(&mut self, b: &BinaryOperator) {
        match b.op_type {
            // `x = e` stores straight into the variable, but `x[i] = e`,
            // `x.a = e` and `x .= e` all write through a reference to it
            BinaryOperatorType::Assign => {
                if !matches!(*b.lhs.1, Expression::IdentifierReference(_)) {
                    self.target_root(&b.lhs.1);
                }
                self.expr_children(&b.lhs.1);
                self.expr(&b.rhs.1);
            }
            BinaryOperatorType::DotAssign => {
                self.target_root(&b.lhs.1);
                self.expr_children(&b.lhs.1);
                self.expr(&b.rhs.1);
            }
            _ => {
                self.expr(&b.lhs.1);
                self.expr(&b.rhs.1);
            }
        }
    }

    /// visit an assignment target's sub-expressions (indices, bases) as ordinary
    /// reads, without re-reporting the target itself
    fn expr_children(&mut self, expr: &Expression) {
        match expr {
            Expression::IdentifierReference(_) => {}
            Expression::Subscript(s) => {
                self.expr_children(&s.base.1);
                self.expr(&s.index.1);
            }
            Expression::Property(p) => self.expr_children(&p.base.1),
            Expression::Literal(Literal::List(elements)) => {
                for element in elements {
                    self.expr_children(&element.1);
                }
            }
            other => self.expr(other),
        }
    }

    fn closure_body(&mut self, visit: impl FnOnce(&mut Self)) {
        self.closure_depth += 1;
        visit(self);
        self.closure_depth -= 1;
    }

    fn expr(&mut self, expr: &Expression) {
        match expr {
            Expression::IdentifierReference(ir) => {
                let name = ident_ref_name(ir).to_string();
                match ir {
                    // `&x` hands out a reference, and a stateful read resolves
                    // through the slot the variable holds
                    IdentifierReference::Reference(_)
                    | IdentifierReference::StatefulReference(_) => self.reference(&name),
                    IdentifierReference::Value(_) => {}
                }
                if self.closure_depth > 0 {
                    self.out.in_closure.insert(name);
                }
            }
            Expression::BinaryOperator(b) => self.binary(b),
            Expression::UnaryPreOperator(u) => self.expr(&u.operand.1),
            Expression::Literal(l) => match l {
                Literal::List(elements) => {
                    for element in elements {
                        self.expr(&element.1);
                    }
                }
                Literal::Map(entries) => {
                    for (key, value) in entries {
                        self.expr(&key.1);
                        self.expr(&value.1);
                    }
                }
                _ => {}
            },
            Expression::Subscript(s) => {
                self.expr(&s.base.1);
                self.expr(&s.index.1);
            }
            Expression::Property(p) => self.expr(&p.base.1),
            Expression::LambdaInvocation(l) => {
                self.expr(&l.lambda.1);
                for (_, argument) in &l.arguments.1 {
                    self.expr(&argument.1);
                }
            }
            Expression::OperatorInvocation(o) => {
                self.expr(&o.operator.1);
                for (_, argument) in &o.arguments.1 {
                    self.expr(&argument.1);
                }
                self.expr(&o.operand.1);
            }
            Expression::NativeInvocation(n) => {
                for argument in &n.arguments {
                    self.expr(&argument.1);
                }
            }
            Expression::LambdaDefinition(l) => {
                for arg in &l.args {
                    if let Some(default) = &arg.default_value {
                        self.expr(&default.1);
                    }
                }
                // a lambda rejects mutable captures outright, so its body does
                // not pin anything the way a block or anim does
                match &l.body.1 {
                    LambdaBody::Inline(e) => self.expr(e),
                    LambdaBody::Block(stmts) => self.stmts(stmts),
                }
            }
            Expression::OperationDefinition(o) => self.expr(&o.lambda.1),
            Expression::Block(b) => self.closure_body(|s| s.stmts(&b.body)),
            Expression::Anim(a) => self.closure_body(|s| s.stmts(&a.body)),
        }
    }
}
