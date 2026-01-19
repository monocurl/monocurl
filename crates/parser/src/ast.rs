use structs::text::Span8;

pub struct SpanTagged<T>(pub T, pub Span8);
pub struct BoxSpanTagged<T>(pub Box<T>, pub Span8);

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
    Assignment(Assignment),
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
    Attribute(Attribute),
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
    Complex(f64),
    Vector(Vec<SpanTagged<Expression>>),
    Map(Vec<(SpanTagged<Expression>, SpanTagged<Expression>)>)
}

pub struct LambdaDefinition {
    pub arg_names: Vec<IdentifierDeclaration>,
    pub body: LambdaBody
}

pub enum LambdaBody {
    Inline(BoxSpanTagged<Expression>),
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

pub struct UnaryPreOperator {
    pub op_type: UnaryOperatorType,
    pub operand: BoxSpanTagged<Expression>,
}

pub struct OperatorInvocation {
    pub operator: SpanTagged<IdentifierReference>,
    pub arguments: Vec<SpanTagged<Expression>>,
    pub operand: BoxSpanTagged<Expression>
}

pub struct Subscript {
    pub base: BoxSpanTagged<Expression>,
    pub index: BoxSpanTagged<Expression>
}

pub struct Attribute {
    pub base: BoxSpanTagged<Expression>,
    pub attribute: SpanTagged<IdentifierReference>
}

pub struct LambdaInvocation {
    pub lambda: BoxSpanTagged<Expression>,
    pub arguments: Vec<SpanTagged<Expression>>,
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

pub enum AssignmentType {
    Normal,
    DotAssignment
}

pub struct Assignment {
    pub lhs: SpanTagged<Expression>,
    pub assignment_type: AssignmentType,
    pub rhs: SpanTagged<Expression>
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
    pub body: Vec<SpanTagged<Statement>>
}

pub struct For {
    pub var_name: SpanTagged<IdentifierDeclaration>,
    pub container: SpanTagged<Expression>,
    pub body: Vec<SpanTagged<Statement>>
}

pub struct If {
    pub condition: SpanTagged<Expression>,
    pub if_block: Vec<SpanTagged<Statement>>,
    pub else_block: Vec<SpanTagged<Statement>>,
}

pub struct Play {
    pub animations: SpanTagged<Expression>
}
