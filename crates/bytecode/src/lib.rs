use structs::text::Span8;

pub const BYTECODE_VERSION: (usize, usize, usize) = (1, 0, 0);

pub enum Instruction {
    /* memory */
    PushInt { index: u16 },
    PushFloat { index: u16 },
    PushChar { char: char },
    PushString { index: u16 },
    PushEmptyMap,
    PushEmptyVector,
    // push the captured / default variables
    PushLambda,
    // push identifier
    // for assignment
    PushCopy { stack_delta: i32 },
    PushMutableCopy { stack_delta: i32 },
    PushLeaderCopy { stack_delta: i32 },
    PushMutableLeaderCopy { stack_delta: i32 },
    PushDereference { stack_delta: i32 },
    PushStateful { stack_delta: i32 },
    PushParam { },
    PushState { },
    PushMesh { },

    // u16::max indicates null label
    BufferLabelOrAttribute { string_index: u16 },

    OperatorInvoke { stateful: bool, labeled: bool, num_args: u8 },
    LambdaInvoke { stateful: bool, labeled: bool, num_args: u8 },
    Jump { section: u16, to: u32 },
    ConditionalJump { section: u16, to: u32 },
    Return { stack_delta: i32, pop_count: u32 },
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

    EndSection,
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
    pub annotations: Vec<InstructionAnnotation>,
    pub int_pool: Vec<i64>,
    pub float_pool: Vec<f64>,
    pub string_pool: Vec<String>,
}

pub struct Bytecode {
    pub sections: Vec<SectionBytecode>,
    pub bytecode_version: (usize, usize, usize),
}

impl Bytecode {
    pub fn new(sections: Vec<SectionBytecode>) -> Self {
        Self { sections, bytecode_version: BYTECODE_VERSION }
    }

    // returns the index of the suffix that is invalidated now
    pub fn update(&mut self, sections: Vec<SectionBytecode>) -> usize {
        0
    }
}
