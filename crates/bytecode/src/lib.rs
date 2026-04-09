use structs::text::Span8;

pub struct LambdaPrototype {
    pub section: u16,
    pub ip: u32,
    pub required_args: u8,
    pub reference_arg_prefix: u8,
    pub default_arg_count: u8,
}

pub struct AnimPrototype {
    pub section: u16,
    pub ip: u32,
}

pub enum Instruction {
    /* push constants */
    PushInt { index: u16 },
    PushFloat { index: u16 },
    // pushes complex(0, float_pool[index])
    PushImaginary { index: u16 },
    PushChar { char: char },
    PushString { index: u16 },
    PushEmptyMap,
    PushEmptyVector,

    // register TOS as a leader; name_index into the string pool for debugging
    PushParam { name_index: u16 },
    PushState { name_index: u16 },
    PushMesh { name_index: u16 },

    PushCopy { stack_delta: i32 },
    PushLvalue { stack_delta: i32 },
    PushMeshLvalue { stack_delta: i32 },
    PushStateLvalue { stack_delta: i32 },
    PushParamLvalue { stack_delta: i32 },

    // *x: current (leader) value
    PushDereference { stack_delta: i32 },
    // $x: live follower value — TODO: distinguish from PushDereference once VM is ready
    PushStateful { stack_delta: i32 },

    // u16::MAX indicates unlabeled
    BufferLabelOrAttribute { string_index: u16 },

    // pops capture_count lvalues + prototype.default_arg_count values, pushes lambda
    MakeLambda { capture_count: u16, prototype_index: u32 },
    // pops capture_count lvalues, pushes anim
    MakeAnim { capture_count: u16, prototype_index: u32 },
    // pops a lambda, pushes an operator wrapping it
    MakeOperator,

    OperatorInvoke { stateful: bool, labeled: bool, num_args: u8 },
    LambdaInvoke { stateful: bool, labeled: bool, num_args: u8 },
    Jump { section: u16, to: u32 },
    // pops TOS; jumps when truthy
    ConditionalJump { section: u16, to: u32 },
    Return { stack_delta: i32 },
    Pop { count: u32 },

    NativeInvoke { index: u32 },

    Play,

    /* unary */
    Negate,
    Not,

    Subscript { mutable: bool },
    Attribute { mutable: bool, string_index: u16 },

    /* binary (pop 2, push 1) */
    Add,
    Sub,
    Mul,
    Div,
    Power,
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
    IntDiv,
    In,
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

pub struct SectionBytecode {
    pub flags: SectionFlags,
    pub instructions: Vec<Instruction>,
    pub annotations: Vec<InstructionAnnotation>,
    pub int_pool: Vec<i64>,
    pub float_pool: Vec<f64>,
    pub string_pool: Vec<String>,
    pub lambda_prototypes: Vec<LambdaPrototype>,
    pub anim_prototypes: Vec<AnimPrototype>,
}

impl SectionBytecode {
    pub fn new(flags: SectionFlags) -> Self {
        Self {
            flags,
            instructions: Vec::new(),
            annotations: Vec::new(),
            int_pool: Vec::new(),
            float_pool: Vec::new(),
            string_pool: Vec::new(),
            lambda_prototypes: Vec::new(),
            anim_prototypes: Vec::new(),
        }
    }
}

pub struct Bytecode {
    pub sections: Vec<SectionBytecode>,
}

impl Bytecode {
    pub fn new(sections: Vec<SectionBytecode>) -> Self {
        Self { sections }
    }
}
