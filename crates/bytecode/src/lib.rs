use structs::text::Span8;

pub enum Instruction {
    /* memory */
    PushInt { index: u16 },
    PushFloat { index: u16 },
    PushChar { char: char },
    PushString { index: u16 },
    PushEmptyMap,
    PushEmptyVector,

    PushParam,
    PushState ,
    PushMesh,

    PushCopy { stack_delta: i32 },
    PushLvalue { stack_delta: i32, },
    PushMeshLvalue { stack_delta: i32, },
    PushStateLvalue { stack_delta: i32, },
    PushParamLvalue { stack_delta: i32, },

    PushDereference { stack_delta: i32 },
    PushStateful { stack_delta: i32 },

    // u16::max indicates null label
    BufferLabelOrAttribute { string_index: u16 },

    MakeLambda { capture_count: u32 },
    MakeOperator,

    OperatorInvoke { stateful: bool, labeled: bool, num_args: u8 },
    LambdaInvoke { stateful: bool, labeled: bool, num_args: u8 },
    Jump { section: u16, to: u32 },
    ConditionalJump { section: u16, to: u32 },
    Return { stack_delta: i32 },
    Pop { count: u8 },

    NativeInvoke { index: u32 },

    Play,

    /* unary  */
    Negate,
    Not,

    Subscript { mutable: bool },
    Attribute { mutable: bool },

    /* Binary operations */
    Add,
    Sub,
    Mul,
    Div,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    IntDiv,
    Assign,
    AppendAssign,
    Append,

    EndOfExecutionHead,
}

pub struct InstructionAnnotation {
    pub source_loc: Span8,
}

pub struct SectionFlags {
    pub is_stdlib: bool,
    pub is_library: bool,
}

// either a slide or an imported module
pub struct SectionBytecode {
    pub flags: SectionFlags,
    pub instructions: Vec<Instruction>,
    // parallel array
    pub annotations: Vec<InstructionAnnotation>,
    pub int_pool: Vec<i64>,
    pub float_pool: Vec<f64>,
    pub string_pool: Vec<String>,
}

pub struct Bytecode {
    pub sections: Vec<SectionBytecode>,
}

impl Bytecode {
    pub fn new(sections: Vec<SectionBytecode>) -> Self {
        Self { sections }
    }

    // returns the index of the suffix that is invalidated now
    pub fn update(&mut self, sections: Vec<SectionBytecode>) -> usize {
        0
    }
}
