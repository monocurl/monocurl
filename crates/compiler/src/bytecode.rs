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
