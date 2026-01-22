use structs::text::Span8;

pub type SpanTagged<T> = (Span8, T);
pub type BoxSpanTagged<T> = (Span8, Box<T>);

pub enum SectionType {
    StandardLibrary,
    UserLibrary,
    Init,
    Slide
}

// a singular slide / init phase / import module
pub struct Section {
    pub body: Vec<SpanTagged<Statement>>,
    pub section_type: SectionType
}

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

pub enum DirectionalLiteral {
    Up(f64),
    Down(f64),
    Left(f64),
    Right(f64),
    Forward(f64),
    Backward(f64)
}

pub enum Literal {
    String(String),
    Char(char),
    Int(i64),
    Double(f64),
    Directional(DirectionalLiteral),
    Imaginary(f64),
    Vector(Vec<SpanTagged<Expression>>),
    Map(Vec<(SpanTagged<Expression>, SpanTagged<Expression>)>)
}

pub struct LambdaDefinition {
    // identifier and default value
    pub args: Vec<(SpanTagged<IdentifierDeclaration>, Option<SpanTagged<Expression>>)>,
    pub body: SpanTagged<LambdaBody>
}

pub enum LambdaBody {
    Inline(Box<Expression>),
    Block(Vec<SpanTagged<Statement>>),
}

pub struct OperatorDefinition {
    pub lambda: BoxSpanTagged<Expression>
}

pub struct Block {
    pub body: Vec<SpanTagged<Statement>>
}

pub struct Anim {
    pub body: Vec<SpanTagged<Statement>>
}

pub type OperatorPriority = usize;

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
    pub fn priority(&self) -> OperatorPriority {
        match self {
            BinaryOperatorType::Add => 1,
            _ => 2
        }
    }

    /// 1 = right associative, 0 = left associative
    pub fn associativity(&self) -> OperatorPriority {
        match self {
            BinaryOperatorType::Append => 1,
            _ => 0
        }
    }
}

pub struct BinaryOperator {
    pub lhs: BoxSpanTagged<Expression>,
    pub op_type: BinaryOperatorType,
    pub rhs: BoxSpanTagged<Expression>,
}

pub enum UnaryOperatorType {
    Negative,
    Not
}

impl UnaryOperatorType {
    pub fn priority(&self) -> OperatorPriority {
        match self {
            UnaryOperatorType::Negative => 10,
            UnaryOperatorType::Not => 10
        }
    }
}

pub struct UnaryPreOperator {
    pub op_type: UnaryOperatorType,
    pub operand: BoxSpanTagged<Expression>,
}

pub struct Subscript {
    pub base: BoxSpanTagged<Expression>,
    pub index: BoxSpanTagged<Expression>
}

pub struct Property {
    pub base: BoxSpanTagged<Expression>,
    pub attribute: SpanTagged<IdentifierReference>
}

pub struct LambdaInvocation {
    pub lambda: BoxSpanTagged<Expression>,
    pub arguments: SpanTagged<Vec<(Option<SpanTagged<IdentifierDeclaration>>, SpanTagged<Expression>)>>,
}

pub struct OperatorInvocation {
    pub operator: BoxSpanTagged<Expression>,
    pub arguments: SpanTagged<Vec<(Option<SpanTagged<IdentifierDeclaration>>, SpanTagged<Expression>)>>,
    pub operand: BoxSpanTagged<Expression>,
}

pub struct NativeInvocation {
    pub function: SpanTagged<IdentifierReference>,
    pub arguments: Vec<SpanTagged<Expression>>,
}

pub struct IdentifierDeclaration(pub String);

pub enum IdentifierReference {
    Value(String),
    Reference(String),
    Stateful(String),
    Dereference(String)
}

pub enum VariableType {
    Let,
    Var,
    Mesh,
    State,
    Param
}

pub struct Declaration {
    pub var_type: VariableType,
    pub identifier: SpanTagged<IdentifierDeclaration>,
    pub value: SpanTagged<Expression>,
}

pub struct Return {
    pub value: SpanTagged<Expression>
}

pub struct While {
    pub condition: SpanTagged<Expression>,
    pub body: SpanTagged<Vec<SpanTagged<Statement>>>
}

pub struct For {
    pub var_name: SpanTagged<IdentifierDeclaration>,
    pub container: SpanTagged<Expression>,
    pub body: SpanTagged<Vec<SpanTagged<Statement>>>
}

pub struct If {
    pub condition: SpanTagged<Expression>,
    pub if_block: SpanTagged<Vec<SpanTagged<Statement>>>,
    pub else_block: Option<SpanTagged<Vec<SpanTagged<Statement>>>>,
}

pub struct Play {
    pub animations: SpanTagged<Expression>
}
