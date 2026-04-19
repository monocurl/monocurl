use std::sync::Arc;

use structs::text::Span8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LambdaPrototype {
    pub section: u16,
    pub ip: u32,
    pub required_args: u32,
    pub default_arg_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimPrototype {
    pub section: u16,
    pub ip: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    /* push constants */
    PushInt {
        index: u32,
    },
    PushFloat {
        index: u32,
    },
    // pushes complex(0, float_pool[index])
    PushImaginary {
        index: u32,
    },
    PushChar {
        char: char,
    },
    PushString {
        index: u32,
    },
    PushEmptyMap,
    PushEmptyVector,

    // register tos as a leader; name_index into the string pool for debugging
    ConvertParam {
        name_index: u32,
    },
    ConvertMesh {
        name_index: u32,
    },
    ConvertVar {
        allow_stateful: bool,
    },
    // sync all leader followers to their leader values; emitted at end of init section
    SyncAllLeaders,

    // pops old tos if flag is true
    // used for map
    PushCopy {
        pop_tos: bool,
        mutable: bool,
        stack_delta: i32,
    },
    PushLvalue {
        force_ephemeral: bool,
        stack_delta: i32,
    },

    PushDereference {
        stack_delta: i32,
    },
    PushStateful {
        stack_delta: i32,
    },

    // u16::MAX indicates unlabeled
    BufferLabelOrAttribute {
        string_index: u32,
    },

    // pops capture_count lvalues + prototype.default_arg_count values, pushes lambda
    MakeLambda {
        capture_count: u16,
        prototype_index: u32,
    },
    // pops capture_count lvalues, pushes anim
    MakeAnim {
        capture_count: u16,
        prototype_index: u32,
    },
    // pops a lambda, pushes an operator wrapping it
    MakeOperator,

    OperatorInvoke {
        stateful: bool,
        labeled: bool,
        num_args: u32,
    },
    LambdaInvoke {
        stateful: bool,
        labeled: bool,
        num_args: u32,
    },
    // pops the operator lambda result ([initial, modified] list) and pushes the live value.
    // for labeled invocations the InvokedOperator is already on stack; this is a no-op then.
    ConvertToLiveOperator,
    Jump {
        section: u16,
        to: u32,
    },
    // pops TOS; jumps when truthy
    ConditionalJump {
        section: u16,
        to: u32,
    },
    Return {
        stack_delta: i32,
    },
    Pop {
        count: u32,
    },

    NativeInvoke {
        index: u16,
        arg_count: u16,
    },

    Play,

    /* unary */
    Negate,
    Not,

    Subscript {
        mutable: bool,
    },
    Attribute {
        mutable: bool,
        string_index: u32,
    },

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
const _: () = assert!(std::mem::size_of::<Instruction>() == 8);

#[derive(Clone, PartialEq)]
pub struct InstructionAnnotation {
    pub source_loc: Span8,
}

#[derive(Clone, PartialEq)]
pub struct SectionFlags {
    pub is_stdlib: bool,
    pub is_library: bool,
    pub is_init: bool,
    pub is_root_module: bool,
}

#[derive(Clone, PartialEq)]
pub struct SectionBytecode {
    pub flags: SectionFlags,
    pub source_file_name: Option<String>,
    pub import_display_index: Option<usize>,
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
            source_file_name: None,
            import_display_index: None,
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

#[derive(Default, Clone)]
pub struct Bytecode {
    pub sections: Vec<Arc<SectionBytecode>>,
}

impl Bytecode {
    pub fn new(sections: Vec<Arc<SectionBytecode>>) -> Self {
        assert!(!sections.is_empty());
        Self { sections }
    }

    pub fn library_sections(&self) -> usize {
        self.sections
            .iter()
            .take_while(|s| s.flags.is_library)
            .count()
    }

    pub fn non_slide_sections(&self) -> usize {
        self.sections
            .iter()
            .take_while(|s| s.flags.is_library || s.flags.is_init)
            .count()
    }
}
