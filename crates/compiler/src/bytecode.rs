pub enum Instruction {
    /* memory */
    PushLiteral { flags: u8, value: u8 },
    PushCopy {},
    Pop(u8),

    /* instruction pointer  */
    Call { num_args: u8, flags: u8 },
    Jump,
    CJump,
    Return,

    Assign,

    /* primitive operations */
    Add,
    Sub,
    Mul,
    Div,
    IntDiv,

    NativeCall { index: u16 },
}

pub struct SectionFlags {
    priviliged: bool,
    is_library: bool,
}

// either a slide or an imported module
pub struct SectionBytecode {
    flags: SectionFlags,
    direct_instructions: Vec<Instruction>,
    literal_pool: Vec<u8>,
}

pub struct Bytecode {
    sections: Vec<SectionBytecode>,
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
