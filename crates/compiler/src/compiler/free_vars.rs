use std::collections::HashSet;

use parser::ast::{Expression, LambdaBody, Literal, SpanTagged, Statement};

pub(super) struct FreeVarCollector {
    defined: HashSet<String>,
    free: Vec<String>,
    seen_free: HashSet<String>,
}

impl FreeVarCollector {
    pub(super) fn new(predefined: HashSet<String>) -> Self {
        Self {
            defined: predefined,
            free: Vec::new(),
            seen_free: HashSet::new(),
        }
    }

    fn define(&mut self, name: &str) {
        self.defined.insert(name.to_string());
    }

    fn reference(&mut self, name: &str) {
        if !self.defined.contains(name) && self.seen_free.insert(name.to_string()) {
            self.free.push(name.to_string());
        }
    }

    pub(super) fn into_free(self) -> Vec<String> {
        self.free
    }

    pub(super) fn visit_stmts(&mut self, stmts: &[SpanTagged<Statement>]) {
        for (_, s) in stmts {
            self.visit_stmt(s);
        }
    }

    fn visit_stmt(&mut self, stmt: &Statement) {
        match stmt {
            Statement::Expression(e) => self.visit_expr(e),
            Statement::Declaration(d) => {
                self.visit_expr(&d.value.1);
                self.define(&d.identifier.1.0);
            }
            Statement::Return(r) => self.visit_expr(&r.value.1),
            Statement::While(w) => {
                self.visit_expr(&w.condition.1);
                self.visit_stmts(&w.body.1);
            }
            Statement::For(f) => {
                self.visit_expr(&f.container.1);
                self.define(&f.var_name.1.0);
                self.visit_stmts(&f.body.1);
            }
            Statement::If(i) => {
                self.visit_expr(&i.condition.1);
                self.visit_stmts(&i.if_block.1);
                if let Some(ref e) = i.else_block {
                    self.visit_stmts(&e.1);
                }
            }
            Statement::Play(p) => self.visit_expr(&p.animations.1),
            Statement::Break | Statement::Continue => {}
        }
    }

    pub(super) fn visit_expr(&mut self, expr: &Expression) {
        match expr {
            Expression::IdentifierReference(ir) => self.reference(super::ident_ref_name(ir)),
            Expression::BinaryOperator(b) => {
                self.visit_expr(&b.lhs.1);
                self.visit_expr(&b.rhs.1);
            }
            Expression::UnaryPreOperator(u) => self.visit_expr(&u.operand.1),
            Expression::Literal(l) => match l {
                Literal::Vector(v) => {
                    for e in v {
                        self.visit_expr(&e.1);
                    }
                }
                Literal::Map(m) => {
                    for (k, v) in m {
                        self.visit_expr(&k.1);
                        self.visit_expr(&v.1);
                    }
                }
                _ => {}
            },
            Expression::Subscript(s) => {
                self.visit_expr(&s.base.1);
                self.visit_expr(&s.index.1);
            }
            Expression::Property(p) => self.visit_expr(&p.base.1),
            Expression::LambdaInvocation(l) => {
                self.visit_expr(&l.lambda.1);
                for (_, a) in &l.arguments.1 {
                    self.visit_expr(&a.1);
                }
            }
            Expression::OperatorInvocation(o) => {
                self.visit_expr(&o.operator.1);
                for (_, a) in &o.arguments.1 {
                    self.visit_expr(&a.1);
                }
                self.visit_expr(&o.operand.1);
            }
            Expression::NativeInvocation(n) => {
                for a in &n.arguments {
                    self.visit_expr(&a.1);
                }
            }
            Expression::LambdaDefinition(l) => {
                // default values evaluated in outer scope
                for arg in &l.args {
                    if let Some(ref d) = arg.default_value {
                        self.visit_expr(&d.1);
                    }
                }
                let mut inner_pre: HashSet<String> =
                    l.args.iter().map(|a| a.identifier.1.0.clone()).collect();
                if matches!(l.body.1, LambdaBody::Block(_)) {
                    inner_pre.insert("_".to_string());
                }
                let mut inner = FreeVarCollector::new(inner_pre);
                match &l.body.1 {
                    LambdaBody::Inline(e) => inner.visit_expr(e),
                    LambdaBody::Block(s) => inner.visit_stmts(s),
                }
                for name in inner.into_free() {
                    self.reference(&name);
                }
            }
            Expression::OperationDefinition(o) => self.visit_expr(&o.lambda.1),
            Expression::Block(b) => {
                let mut inner = FreeVarCollector::new(HashSet::from(["_".to_string()]));
                inner.visit_stmts(&b.body);
                for name in inner.into_free() {
                    self.reference(&name);
                }
            }
            Expression::Anim(a) => {
                let mut inner = FreeVarCollector::new(HashSet::new());
                inner.visit_stmts(&a.body);
                for name in inner.into_free() {
                    self.reference(&name);
                }
            }
        }
    }
}

pub(super) fn free_vars_stmts(
    stmts: &[SpanTagged<Statement>],
    pre: HashSet<String>,
) -> Vec<String> {
    let mut c = FreeVarCollector::new(pre);
    c.visit_stmts(stmts);
    c.into_free()
}

pub(super) fn free_vars_expr(expr: &Expression, pre: HashSet<String>) -> Vec<String> {
    let mut c = FreeVarCollector::new(pre);
    c.visit_expr(expr);
    c.into_free()
}
