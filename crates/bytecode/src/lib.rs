use std::{path::PathBuf, sync::Arc};

use structs::text::Span8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LambdaPrototype {
    pub section: u16,
    pub ip: u32,
    pub required_args: u32,
    pub default_arg_count: u32,
    pub reference_args: Vec<bool>,
    pub arg_names: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnimPrototype {
    pub section: u16,
    pub ip: u32,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CopyValueMode {
    Read,
    Reference,
    Raw,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    /* push constants */
    PushNil,
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
    PushEmptyList,

    // register tos as a leader; name_index into the string pool for debugging
    ConvertParam {
        name_index: u32,
    },
    ConvertMesh {
        name_index: u32,
    },
    // a `let`/`var` local that nothing takes a reference into stays in its stack
    // entry instead of a heap slot. BindLocal keeps ConvertVar's rejection of
    // stateful initializers without allocating; StoreLocal writes through to the
    // entry. see the compiler's `unboxing` module for when these are emitted
    BindLocal,
    StoreLocal {
        stack_delta: i32,
    },

    ConvertVar {
        allow_stateful: bool,
    },
    // sync all leader followers to their leader values; emitted at end of init section
    SyncAllLeaders,

    PushDeepCopy {
        stack_delta: i32,
    },
    // pops old tos if flag is true
    // used for map
    PushCopy {
        copy_mode: CopyValueMode,
        pop_tos: bool,
        stack_delta: i32,
    },
    PushLvalue {
        force_ephemeral: bool,
        stack_delta: i32,
    },

    PushStateful {
        stack_delta: i32,
    },

    // u16::MAX indicates unlabeled
    BufferLabelOrAttribute {
        string_index: u32,
    },

    // pops capture_count captured values + prototype.default_arg_count default values, pushes lambda
    MakeLambda {
        capture_count: u16,
        prototype_index: u32,
    },
    // pops capture_count captured values, pushes anim
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
    // pops TOS; jumps when falsy. every loop and conditional branches on the
    // negation of its test, so emitting `Not` first would cost an instruction
    // per iteration
    JumpIfFalse {
        section: u16,
        to: u32,
    },
    // the whole header of a `for i in range(a, b)` loop: reads the counter at
    // `current_delta` and the bound just above it, and jumps when the counter has
    // reached the bound. saves the two copies, the compare and the branch that the
    // header would otherwise run on every iteration. always targets the enclosing
    // section, since a loop never spans two
    RangeLoopTest {
        current_delta: i16,
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

    IncrementByOne {
        stack_delta: i32,
    },

    Play,
    Observe,

    /* unary */
    Negate,
    Not,

    Subscript {
        mutable: bool,
    },
    /// pops `depth` indices (outermost first, innermost on top) and reads
    /// `container[i0][i1]...` in place from the container held at `stack_delta`,
    /// pushing the selected element. unlike a copy followed by `Subscript` this
    /// never duplicates the container, which is what keeps indexing O(1) and
    /// iteration linear. `copy_mode` is how the container would have been
    /// copied, for shapes that fall back to the general path
    SubscriptLocal {
        stack_delta: i32,
        depth: u16,
        copy_mode: CopyValueMode,
    },
    /// pushes the length of the container held at `stack_delta`, reading it in
    /// place
    ContainerLen {
        stack_delta: i32,
    },
    /// pushes `len(...)` of the container held at `stack_delta`, reading it in
    /// place; `copy_mode` is how it would have been copied into the native
    LenLocal {
        stack_delta: i32,
        copy_mode: CopyValueMode,
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

#[derive(Clone, Debug, PartialEq)]
pub struct InstructionAnnotation {
    pub source_loc: Span8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SectionFlags {
    pub is_stdlib: bool,
    pub is_library: bool,
    pub is_init: bool,
    pub is_root_module: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SectionBytecode {
    pub flags: SectionFlags,
    pub name: Option<String>,
    pub source_file_name: Option<String>,
    pub source_file_path: Option<PathBuf>,
    pub import_display_index: Option<usize>,
    /// text of the file this section was compiled from, kept for imported
    /// sections so runtime errors can quote library source lines
    pub source_text: Option<Arc<str>>,
    pub instructions: Vec<Instruction>,
    pub annotations: Vec<InstructionAnnotation>,
    pub int_pool: Vec<i64>,
    pub float_pool: Vec<f64>,
    /// shared so that pushing a constant or naming an argument label costs a
    /// reference count, not a copy
    pub string_pool: Vec<Arc<str>>,
    pub lambda_prototypes: Vec<LambdaPrototype>,
    pub anim_prototypes: Vec<AnimPrototype>,
}

impl SectionBytecode {
    pub fn new(flags: SectionFlags) -> Self {
        Self {
            flags,
            name: None,
            source_file_name: None,
            source_file_path: None,
            import_display_index: None,
            source_text: None,
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

#[derive(Default, Clone, Debug, PartialEq)]
pub struct Bytecode {
    pub sections: Vec<Arc<SectionBytecode>>,
    /// per section, the root `import` statement that pulled it in. annotations
    /// of imported sections hold spans into their own file, so anything that
    /// highlights the root document maps them through this
    pub import_spans: Vec<Option<Span8>>,
}

impl Bytecode {
    pub fn new(sections: Vec<Arc<SectionBytecode>>) -> Self {
        assert!(!sections.is_empty());
        Self {
            sections,
            import_spans: Vec::new(),
        }
    }

    pub fn with_import_spans(mut self, import_spans: Vec<Option<Span8>>) -> Self {
        self.import_spans = import_spans;
        self
    }

    pub fn import_span(&self, section_idx: usize) -> Option<&Span8> {
        self.import_spans.get(section_idx)?.as_ref()
    }

    /// `span` of an instruction in `section_idx`, as a span into the root document
    pub fn root_span(&self, section_idx: usize, span: Span8) -> Span8 {
        match self.sections.get(section_idx) {
            Some(section) if !section.flags.is_root_module => self
                .import_span(section_idx)
                .cloned()
                .unwrap_or(span),
            _ => span,
        }
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
