//! the register form a kernel runs. registers are the abstract stack positions
//! of the bytecode, so translation needs no allocation pass: position `p` of the
//! interpreter's frame is register `p`

use crate::value::InstructionPointer;

pub type Reg = u16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinKind {
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
}

/// a native the tier evaluates itself. each mirrors the stdlib function it is
/// registered against, including which argument types it accepts
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KernelIntrinsic {
    Sqrt,
    Cbrt,
    Exp,
    Ln,
    Sin,
    Cos,
    Tan,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Pow,
    Atan2,
    Abs,
    Sign,
    Floor,
    Ceil,
    Round,
    Trunc,
    Mod,
    Min,
    Max,
    Dot,
    Cross,
    /// `len` of a list (maps and strings are not modelled)
    Len,
    ToInt,
    ToFloat,
    /// `keyframe_lerp` over a map the conversion recognised as a palette
    KeyframeLerp,
    /// the native the compiler places after a block body that fell through
    /// without returning; always an error
    Fallthrough,
}

impl KernelIntrinsic {
    pub fn arity(self) -> usize {
        use KernelIntrinsic::*;
        match self {
            Sqrt | Cbrt | Exp | Ln | Sin | Cos | Tan | Asin | Acos | Atan | Sinh | Cosh | Tanh
            | Abs | Sign | Floor | Ceil | Round | Trunc | Len | ToInt | ToFloat => 1,
            Pow | Atan2 | Mod | Min | Max | Dot | Cross | KeyframeLerp => 2,
            Fallthrough => 0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum KOp {
    Nil {
        dst: Reg,
    },
    Int {
        dst: Reg,
        value: i64,
    },
    Float {
        dst: Reg,
        value: f64,
    },
    Move {
        dst: Reg,
        src: Reg,
    },
    /// a move of a register that is dead afterwards: the value changes hands
    /// instead of being copied, so a list moves on with a single holder and
    /// the next arithmetic on it can update it in place
    Take {
        dst: Reg,
        src: Reg,
    },
    Bin {
        op: BinKind,
        dst: Reg,
        a: Reg,
        b: Reg,
        /// `a` is dead after this op, so its value may be consumed
        take: bool,
    },
    Neg {
        dst: Reg,
        src: Reg,
    },
    Not {
        dst: Reg,
        src: Reg,
    },
    Jump {
        to: u32,
    },
    JumpIf {
        cond: Reg,
        to: u32,
    },
    JumpIfNot {
        cond: Reg,
        to: u32,
    },
    /// leaves the loop when `regs[current] < regs[current + 1]` no longer holds
    RangeTest {
        current: Reg,
        to: u32,
    },
    Inc {
        reg: Reg,
    },
    EmptyList {
        dst: Reg,
    },
    /// `regs[list]` gains `regs[value]` as its last element
    Append {
        list: Reg,
        value: Reg,
    },
    Index {
        dst: Reg,
        list: Reg,
        index: Reg,
    },
    Len {
        dst: Reg,
        src: Reg,
    },
    /// arguments sit in `arg_start..arg_start + arg_count`; the result replaces
    /// the first of them, which is where the interpreter leaves it too
    Call {
        callee: Reg,
        arg_start: Reg,
        arg_count: u16,
    },
    Native {
        intrinsic: KernelIntrinsic,
        arg_start: Reg,
        arg_count: u16,
    },
    Return {
        src: Reg,
    },
    /// the end of a region kernel: the frame is the result
    Exit,
}

#[derive(Debug)]
pub struct Kernel {
    pub ip: InstructionPointer,
    pub required_args: u16,
    pub total_args: u16,
    pub capture_count: u16,
    pub frame_size: u16,
    pub ops: Box<[KOp]>,
}

/// ops below which a body is too small for a single call to be worth moving
/// across the value boundary; batches ignore this since they amortise it
const SINGLE_CALL_MIN_OPS: usize = 24;

impl Kernel {
    /// whether one call of this body is likely to run long enough in the
    /// register machine to pay for converting its arguments and result:
    /// anything that loops or calls, or is simply large
    pub fn worth_single_call(&self) -> bool {
        self.ops.len() >= SINGLE_CALL_MIN_OPS
            || self.ops.iter().enumerate().any(|(index, op)| match op {
                KOp::Jump { to } | KOp::JumpIf { to, .. } | KOp::JumpIfNot { to, .. } => {
                    *to as usize <= index
                }
                KOp::RangeTest { .. } | KOp::Call { .. } => true,
                _ => false,
            })
    }
}

const _: () = assert!(std::mem::size_of::<KOp>() <= 16);
