use bytecode::{Bytecode, SectionBytecode};
use parser::{ast::{
    Anim, Block, BinaryOperator, BinaryOperatorType, BoxSpanTagged, Declaration,
    DirectionalLiteral, Expression, For, If, IdentifierDeclaration, IdentifierReference,
    LambdaDefinition, LambdaInvocation, Literal, NativeInvocation, OperatorDefinition,
    OperatorInvocation, Play, Property, Return, Section, SpanTagged, Statement, Subscript,
    UnaryOperatorType, UnaryPreOperator, VariableType as AstVariableType, While,
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
    fn compile_full(&self, ast: Vec<Section>) -> Result<Bytecode> {
        todo!()
    }
}

impl Compiler {
    fn compile_section(&self, section: Section) -> SectionBytecode {
        todo!()
    }
}

impl Compiler {
    fn compile_statement(&self, stmt: Statement) {
        match stmt {
            Statement::Break => self.compile_break(),
            Statement::Continue => self.compile_continue(),
            Statement::Return(r) => self.compile_return(r),
            Statement::While(w) => self.compile_while(w),
            Statement::For(f) => self.compile_for(f),
            Statement::If(i) => self.compile_if(i),
            Statement::Declaration(d) => self.compile_declaration(d),
            Statement::Expression(e) => self.compile_expression(e),
            Statement::Play(p) => self.play(p),
        }
    }

    fn compile_break(&self) {
        todo!()
    }

    fn compile_continue(&self) {
        todo!()
    }

    fn compile_return(&self, r: Return) {
        todo!()
    }

    fn compile_while(&self, w: While) {
        todo!()
    }

    fn compile_for(&self, f: For) {
        todo!()
    }

    fn compile_if(&self, i: If) {
        todo!()
    }

    fn compile_declaration(&self, d: Declaration) {
        match d.var_type {
            AstVariableType::Let => self.compile_let(d.identifier, d.value),
            AstVariableType::Var => self.compile_var(d.identifier, d.value),
            AstVariableType::Mesh => self.compile_mesh(d.identifier, d.value),
            AstVariableType::State => self.compile_state(d.identifier, d.value),
            AstVariableType::Param => self.compile_param(d.identifier, d.value),
        }
    }

    fn compile_let(&self, ident: SpanTagged<IdentifierDeclaration>, value: SpanTagged<Expression>) {
        todo!()
    }

    fn compile_var(&self, ident: SpanTagged<IdentifierDeclaration>, value: SpanTagged<Expression>) {
        todo!()
    }

    fn compile_mesh(&self, ident: SpanTagged<IdentifierDeclaration>, value: SpanTagged<Expression>) {
        todo!()
    }

    fn compile_state(&self, ident: SpanTagged<IdentifierDeclaration>, value: SpanTagged<Expression>) {
        todo!()
    }

    fn compile_param(&self, ident: SpanTagged<IdentifierDeclaration>, value: SpanTagged<Expression>) {
        todo!()
    }

    fn play(&self, p: Play) {
        todo!()
    }
}

impl Compiler {
    fn compile_expression(&self, expr: Expression) {
        match expr {
            Expression::Literal(l) => self.compile_literal(l),
            Expression::LambdaDefinition(l) => self.compile_lambda_definition(l),
            Expression::OperationDefinition(o) => self.compile_operator_definition(o),
            Expression::Block(b) => self.compile_block(b),
            Expression::Anim(a) => self.compile_anim(a),
            Expression::BinaryOperator(b) => self.compile_binary_operator(b),
            Expression::OperatorInvocation(o) => self.compile_operator_invocation(o),
            Expression::UnaryPreOperator(u) => self.compile_unary_pre_operator(u),
            Expression::IdentifierReference(i) => self.compile_identifier_reference(i),
            Expression::Subscript(s) => self.compile_subscript(s),
            Expression::Property(p) => self.compile_property(p),
            Expression::LambdaInvocation(l) => self.compile_lambda_invocation(l),
            Expression::NativeInvocation(n) => self.compile_native_invocation(n),
        }
    }

    fn compile_literal(&self, l: Literal) {
        match l {
            Literal::String(s) => self.compile_string_literal(s),
            Literal::Int(i) => self.compile_int_literal(i),
            Literal::Float(f) => self.compile_float_literal(f),
            Literal::Directional(d) => self.compile_directional_literal(d),
            Literal::Imaginary(f) => self.compile_imaginary_literal(f),
            Literal::Vector(v) => self.compile_vector_literal(v),
            Literal::Map(m) => self.compile_map_literal(m),
        }
    }

    fn compile_string_literal(&self, s: String) {
        todo!()
    }

    fn compile_int_literal(&self, i: i64) {
        todo!()
    }

    fn compile_float_literal(&self, f: f64) {
        todo!()
    }

    fn compile_directional_literal(&self, d: DirectionalLiteral) {
        match d {
            DirectionalLiteral::Up(f) => self.compile_up(f),
            DirectionalLiteral::Down(f) => self.compile_down(f),
            DirectionalLiteral::Left(f) => self.compile_left(f),
            DirectionalLiteral::Right(f) => self.compile_right(f),
            DirectionalLiteral::Forward(f) => self.compile_forward(f),
            DirectionalLiteral::Backward(f) => self.compile_backward(f),
        }
    }

    fn compile_up(&self, magnitude: f64) {
        todo!()
    }

    fn compile_down(&self, magnitude: f64) {
        todo!()
    }

    fn compile_left(&self, magnitude: f64) {
        todo!()
    }

    fn compile_right(&self, magnitude: f64) {
        todo!()
    }

    fn compile_forward(&self, magnitude: f64) {
        todo!()
    }

    fn compile_backward(&self, magnitude: f64) {
        todo!()
    }

    fn compile_imaginary_literal(&self, f: f64) {
        todo!()
    }

    fn compile_vector_literal(&self, v: Vec<SpanTagged<Expression>>) {
        todo!()
    }

    fn compile_map_literal(&self, m: Vec<(SpanTagged<Expression>, SpanTagged<Expression>)>) {
        todo!()
    }

    fn compile_lambda_definition(&self, l: LambdaDefinition) {
        todo!()
    }

    fn compile_operator_definition(&self, o: OperatorDefinition) {
        todo!()
    }

    fn compile_block(&self, b: Block) {
        todo!()
    }

    fn compile_anim(&self, a: Anim) {
        todo!()
    }

    fn compile_binary_operator(&self, b: BinaryOperator) {
        let lhs = b.lhs;
        let rhs = b.rhs;
    }

    fn compile_assign(&self, lhs: BoxSpanTagged<Expression>, rhs: BoxSpanTagged<Expression>) {
        todo!()
    }

    fn compile_dot_assign(&self, lhs: BoxSpanTagged<Expression>, rhs: BoxSpanTagged<Expression>) {
        todo!()
    }

    fn compile_operator_invocation(&self, o: OperatorInvocation) {
        todo!()
    }

    fn compile_unary_pre_operator(&self, u: UnaryPreOperator) {
        match u.op_type {
            UnaryOperatorType::Negative => self.compile_negate(u.operand),
            UnaryOperatorType::Not => self.compile_not(u.operand),
        }
    }

    fn compile_negate(&self, operand: BoxSpanTagged<Expression>) {
        todo!()
    }

    fn compile_not(&self, operand: BoxSpanTagged<Expression>) {
        todo!()
    }

    fn compile_identifier_reference(&self, i: IdentifierReference) {
        match i {
            IdentifierReference::Value(name) => self.compile_value_reference(name),
            IdentifierReference::Reference(name) => self.compile_reference(name),
            IdentifierReference::Stateful(name) => self.compile_stateful_reference(name),
            IdentifierReference::Dereference(name) => self.compile_dereference(name),
        }
    }

    fn compile_value_reference(&self, name: String) {
        todo!()
    }

    fn compile_reference(&self, name: String) {
        todo!()
    }

    fn compile_stateful_reference(&self, name: String) {
        todo!()
    }

    fn compile_dereference(&self, name: String) {
        todo!()
    }

    fn compile_subscript(&self, s: Subscript) {
        todo!()
    }

    fn compile_property(&self, p: Property) {
        todo!()
    }

    fn compile_lambda_invocation(&self, l: LambdaInvocation) {
        todo!()
    }

    fn compile_native_invocation(&self, n: NativeInvocation) {
        todo!()
    }
}
