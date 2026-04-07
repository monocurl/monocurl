use bytecode::{Bytecode, SectionBytecode};
use parser::{ast::{
    Anim, BinaryOperator, BinaryOperatorType, Block, BoxSpanTagged, Declaration, DirectionalLiteral, Expression, For, IdentifierDeclaration, IdentifierReference, If, LambdaDefinition, LambdaInvocation, Literal, NativeInvocation, OperatorDefinition, OperatorInvocation, Play, Property, Return, Section, SectionBundle, SpanTagged, Statement, Subscript, UnaryOperatorType, UnaryPreOperator, VariableType as AstVariableType, While
}, parser::SectionParser};

pub enum CompileError {

}

pub type Result<T> = std::result::Result<T, CompileError>;

pub enum VariableType {
    Let,
    Var,
    State,
    Param,
    Mesh
}

struct Symbol {
    name: String,
}

struct Compiler {
   symbol_stack: Vec<i32>,
}

impl Compiler {
    fn full(&self, ast: &SectionBundle) -> Result<Bytecode> {
        todo!()
    }

    fn section_bundle(&self, ast: &SectionBundle) {

    }
}

impl Compiler {
    fn section(&self, section: &Section) -> SectionBytecode {
        todo!()
    }
}

impl Compiler {
    fn statement(&self, stmt: &Statement) {
        match stmt {
            Statement::Break => self.break_(),
            Statement::Continue => self.continue_(),
            Statement::Return(r) => self.return_(r),
            Statement::While(w) => self.while_(w),
            Statement::For(f) => self.for_(f),
            Statement::If(i) => self.if_(i),
            Statement::Declaration(d) => self.declaration(d),
            Statement::Expression(e) => self.expression(e),
            Statement::Play(p) => self.play(p),
        }
    }

    fn break_(&self) {
        todo!()
    }

    fn continue_(&self) {
        todo!()
    }

    fn return_(&self, r: &Return) {
        todo!()
    }

    fn while_(&self, w: &While) {
        todo!()
    }

    fn for_(&self, f: &For) {
        todo!()
    }

    fn if_(&self, i: &If) {
        todo!()
    }

    fn declaration(&self, d: &Declaration) {
        match &d.var_type {
            AstVariableType::Let => self.let_(&d.identifier, &d.value),
            AstVariableType::Var => self.var(&d.identifier, &d.value),
            AstVariableType::Mesh => self.mesh(&d.identifier, &d.value),
            AstVariableType::State => self.state(&d.identifier, &d.value),
            AstVariableType::Param => self.param(&d.identifier, &d.value),
        }
    }

    fn let_(&self, ident: &SpanTagged<IdentifierDeclaration>, value: &SpanTagged<Expression>) {
        todo!()
    }

    fn var(&self, ident: &SpanTagged<IdentifierDeclaration>, value: &SpanTagged<Expression>) {
        todo!()
    }

    fn mesh(&self, ident: &SpanTagged<IdentifierDeclaration>, value: &SpanTagged<Expression>) {
        todo!()
    }

    fn state(&self, ident: &SpanTagged<IdentifierDeclaration>, value: &SpanTagged<Expression>) {
        todo!()
    }

    fn param(&self, ident: &SpanTagged<IdentifierDeclaration>, value: &SpanTagged<Expression>) {
        todo!()
    }

    fn play(&self, p: &Play) {
        todo!()
    }
}

impl Compiler {
    fn expression(&self, expr: &Expression) {
        match expr {
            Expression::Literal(l) => self.literal(l),
            Expression::LambdaDefinition(l) => self.lambda_definition(l),
            Expression::OperationDefinition(o) => self.operator_definition(o),
            Expression::Block(b) => self.block(b),
            Expression::Anim(a) => self.anim(a),
            Expression::BinaryOperator(b) => self.binary_operator(b),
            Expression::OperatorInvocation(o) => self.operator_invocation(o),
            Expression::UnaryPreOperator(u) => self.unary_pre_operator(u),
            Expression::IdentifierReference(i) => self.identifier_reference(i),
            Expression::Subscript(s) => self.subscript(s),
            Expression::Property(p) => self.property(p),
            Expression::LambdaInvocation(l) => self.lambda_invocation(l),
            Expression::NativeInvocation(n) => self.native_invocation(n),
        }
    }

    fn literal(&self, l: &Literal) {
        match l {
            Literal::String(s) => self.string_literal(s),
            Literal::Int(i) => self.int_literal(*i),
            Literal::Float(f) => self.float_literal(*f),
            Literal::Directional(d) => self.directional_literal(d),
            Literal::Imaginary(f) => self.imaginary_literal(*f),
            Literal::Vector(v) => self.vector_literal(v),
            Literal::Map(m) => self.map_literal(m),
        }
    }

    fn string_literal(&self, s: &str) {
        todo!()
    }

    fn int_literal(&self, i: i64) {
        todo!()
    }

    fn float_literal(&self, f: f64) {
        todo!()
    }

    fn directional_literal(&self, d: &DirectionalLiteral) {
        match d {
            DirectionalLiteral::Up(f) => self.up(*f),
            DirectionalLiteral::Down(f) => self.down(*f),
            DirectionalLiteral::Left(f) => self.left(*f),
            DirectionalLiteral::Right(f) => self.right(*f),
            DirectionalLiteral::Forward(f) => self.forward(*f),
            DirectionalLiteral::Backward(f) => self.backward(*f),
        }
    }

    fn up(&self, magnitude: f64) {
        todo!()
    }

    fn down(&self, magnitude: f64) {
        todo!()
    }

    fn left(&self, magnitude: f64) {
        todo!()
    }

    fn right(&self, magnitude: f64) {
        todo!()
    }

    fn forward(&self, magnitude: f64) {
        todo!()
    }

    fn backward(&self, magnitude: f64) {
        todo!()
    }

    fn imaginary_literal(&self, f: f64) {
        todo!()
    }

    fn vector_literal(&self, v: &[SpanTagged<Expression>]) {
        todo!()
    }

    fn map_literal(&self, m: &[(SpanTagged<Expression>, SpanTagged<Expression>)]) {
        todo!()
    }

    fn lambda_definition(&self, l: &LambdaDefinition) {
        todo!()
    }

    fn operator_definition(&self, o: &OperatorDefinition) {
        todo!()
    }

    fn block(&self, b: &Block) {
        todo!()
    }

    fn anim(&self, a: &Anim) {
        todo!()
    }

    fn binary_operator(&self, b: &BinaryOperator) {
        let lhs = &b.lhs;
        let rhs = &b.rhs;
    }

    fn assign(&self, lhs: &BoxSpanTagged<Expression>, rhs: &BoxSpanTagged<Expression>) {
        todo!()
    }

    fn dot_assign(&self, lhs: &BoxSpanTagged<Expression>, rhs: &BoxSpanTagged<Expression>) {
        todo!()
    }

    fn operator_invocation(&self, o: &OperatorInvocation) {
        todo!()
    }

    fn unary_pre_operator(&self, u: &UnaryPreOperator) {
        match &u.op_type {
            UnaryOperatorType::Negative => self.negate(&u.operand),
            UnaryOperatorType::Not => self.not(&u.operand),
        }
    }

    fn negate(&self, operand: &BoxSpanTagged<Expression>) {
        todo!()
    }

    fn not(&self, operand: &BoxSpanTagged<Expression>) {
        todo!()
    }

    fn identifier_reference(&self, i: &IdentifierReference) {
        match i {
            IdentifierReference::Value(name) => self.value_reference(name),
            IdentifierReference::Reference(name) => self.reference(name),
            IdentifierReference::Stateful(name) => self.stateful_reference(name),
            IdentifierReference::Dereference(name) => self.dereference(name),
        }
    }

    fn value_reference(&self, name: &str) {
        todo!()
    }

    fn reference(&self, name: &str) {
        todo!()
    }

    fn stateful_reference(&self, name: &str) {
        todo!()
    }

    fn dereference(&self, name: &str) {
        todo!()
    }

    fn subscript(&self, s: &Subscript) {
        todo!()
    }

    fn property(&self, p: &Property) {
        todo!()
    }

    fn lambda_invocation(&self, l: &LambdaInvocation) {
        todo!()
    }

    fn native_invocation(&self, n: &NativeInvocation) {
        todo!()
    }
}
