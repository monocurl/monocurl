use std::path::PathBuf;

use structs::text::Span8;

pub type SpanTagged<T> = (Span8, T);
pub type BoxSpanTagged<T> = (Span8, Box<T>);

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum SectionType {
    StandardLibrary,
    UserLibrary,
    Init,
    Slide
}

impl SectionType {
    pub fn non_root(&self) -> bool {
        matches!(self, SectionType::UserLibrary | SectionType::Slide)
    }
}

pub struct SectionBundle {
    pub file_path: Option<PathBuf>,
    pub file_index: usize,
    pub imported_files: Vec<usize>,
    pub sections: Vec<Section>,
    pub root_import_span: Option<Span8>,
}

// a singular slide / init phase / import module
#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub body: Vec<SpanTagged<Statement>>,
    pub section_type: SectionType
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    Break,
    Continue,
    Return(Return),
    While(While),
    For(For),
    If(If),
    Declaration(Declaration),
    Expression(Expression),
    Play(Play),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    Literal(Literal),
    LambdaDefinition(LambdaDefinition),
    OperationDefinition(OperatorDefinition),
    Block(Block),
    Anim(Anim),
    BinaryOperator(BinaryOperator),
    OperatorInvocation(OperatorInvocation),
    UnaryPreOperator(UnaryPreOperator),
    IdentifierReference(IdentifierReference),
    Subscript(Subscript),
    Property(Property),
    LambdaInvocation(LambdaInvocation),
    NativeInvocation(NativeInvocation),
}

impl Default for Expression {
    fn default() -> Self {
        Expression::Literal(Literal::Int(0))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DirectionalLiteral {
    Up(f64),
    Down(f64),
    Left(f64),
    Right(f64),
    Forward(f64),
    Backward(f64)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    String(String),
    Int(i64),
    Float(f64),
    Directional(DirectionalLiteral),
    Imaginary(f64),
    Vector(Vec<SpanTagged<Expression>>),
    Map(Vec<(SpanTagged<Expression>, SpanTagged<Expression>)>)
}

#[derive(Debug, Clone, PartialEq)]
pub struct LambdaArg {
    pub identifier: SpanTagged<IdentifierDeclaration>,
    pub default_value: Option<SpanTagged<Expression>>,
    pub must_be_reference: bool
}

#[derive(Debug, Clone, PartialEq)]
pub struct LambdaDefinition {
    // identifier and default value
    pub args: Vec<LambdaArg>,
    pub body: SpanTagged<LambdaBody>
}

#[derive(Debug, Clone, PartialEq)]
pub enum LambdaBody {
    Inline(Box<Expression>),
    Block(Vec<SpanTagged<Statement>>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperatorDefinition {
    pub lambda: BoxSpanTagged<Expression>
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub body: Vec<SpanTagged<Statement>>
}

#[derive(Debug, Clone, PartialEq)]
pub struct Anim {
    pub body: Vec<SpanTagged<Statement>>
}

pub type OperatorPriority = usize;

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperatorType {
    Append,
    And,
    Or,
    Add,
    Subtract,
    Multiply,
    Divide,
    IntegerDivide,
    Power,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    In,
    Assign,
    DotAssign,
}

impl BinaryOperatorType {
    /// higher priorty -> binds tighter
    pub fn priority(&self) -> OperatorPriority {
        match self {
            BinaryOperatorType::Assign
            | BinaryOperatorType::DotAssign => 1,

            BinaryOperatorType::Append => 2,

            BinaryOperatorType::Or => 3,
            BinaryOperatorType::And => 4,

            BinaryOperatorType::Eq
            | BinaryOperatorType::Ne
            | BinaryOperatorType::Lt
            | BinaryOperatorType::Le
            | BinaryOperatorType::Gt
            | BinaryOperatorType::Ge
            | BinaryOperatorType::In => 5,

            BinaryOperatorType::Add
            | BinaryOperatorType::Subtract => 6,

            BinaryOperatorType::Multiply
            | BinaryOperatorType::Divide
            | BinaryOperatorType::IntegerDivide => 7,

            BinaryOperatorType::Power => 9,
        }
    }

    /// 1 = right associative, 0 = left associative
    pub fn associativity(&self) -> OperatorPriority {
        match self {
            BinaryOperatorType::Assign
            | BinaryOperatorType::DotAssign
            | BinaryOperatorType::Power => 1,

            _ => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryOperator {
    pub lhs: BoxSpanTagged<Expression>,
    pub op_type: BinaryOperatorType,
    pub rhs: BoxSpanTagged<Expression>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperatorType {
    Negative,
    Not
}

impl UnaryOperatorType {
    pub fn priority(&self) -> OperatorPriority {
        match self {
            UnaryOperatorType::Negative => 8,
            UnaryOperatorType::Not => 8
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnaryPreOperator {
    pub op_type: UnaryOperatorType,
    pub operand: BoxSpanTagged<Expression>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Subscript {
    pub base: BoxSpanTagged<Expression>,
    pub index: BoxSpanTagged<Expression>
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub base: BoxSpanTagged<Expression>,
    pub attribute: SpanTagged<IdentifierReference>
}

#[derive(Debug, Clone, PartialEq)]
pub struct LambdaInvocation {
    pub lambda: BoxSpanTagged<Expression>,
    pub arguments: SpanTagged<Vec<(Option<SpanTagged<IdentifierDeclaration>>, SpanTagged<Expression>)>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperatorInvocation {
    pub operator: BoxSpanTagged<Expression>,
    pub arguments: SpanTagged<Vec<(Option<SpanTagged<IdentifierDeclaration>>, SpanTagged<Expression>)>>,
    pub operand: BoxSpanTagged<Expression>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeInvocation {
    pub function: SpanTagged<IdentifierReference>,
    pub arguments: Vec<SpanTagged<Expression>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IdentifierDeclaration(pub String);

#[derive(Debug, Clone, PartialEq)]
pub enum IdentifierReference {
    Value(String),
    Reference(String),
    StatefulReference(String),
    StatefulDereference(String)
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariableType {
    Let,
    Var,
    Mesh,
    State,
    Param
}

#[derive(Debug, Clone, PartialEq)]
pub struct Declaration {
    pub var_type: VariableType,
    pub identifier: SpanTagged<IdentifierDeclaration>,
    pub value: SpanTagged<Expression>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Return {
    pub value: SpanTagged<Expression>
}

#[derive(Debug, Clone, PartialEq)]
pub struct While {
    pub condition: SpanTagged<Expression>,
    pub body: SpanTagged<Vec<SpanTagged<Statement>>>
}

#[derive(Debug, Clone, PartialEq)]
pub struct For {
    pub var_name: SpanTagged<IdentifierDeclaration>,
    pub container: SpanTagged<Expression>,
    pub body: SpanTagged<Vec<SpanTagged<Statement>>>
}

#[derive(Debug, Clone, PartialEq)]
pub struct If {
    pub condition: SpanTagged<Expression>,
    pub if_block: SpanTagged<Vec<SpanTagged<Statement>>>,
    pub else_block: Option<SpanTagged<Vec<SpanTagged<Statement>>>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Play {
    pub animations: SpanTagged<Expression>
}
