//! translation of a lambda body from stack bytecode to kernel registers.
//!
//! the compiler emits balanced stack code, so every instruction sees a fixed
//! stack depth and stack position `p` can simply become register `p`. the
//! translation walks the body once to establish those depths (rejecting any
//! instruction the tier does not model, and any merge where two paths disagree
//! on the stack shape) and once more to emit ops. a small peephole folds the
//! copies the stack discipline forces around every operation.
//!
//! a `block { ... }` expression compiles to a zero-argument closure that is
//! created and called on the spot. its body is translated inline: the
//! captures already sit in the registers the closure would copy them into,
//! so the block's registers are simply offset by where its captures start,
//! and its `return` becomes a move into the result register plus a jump to
//! the end of the block

use bytecode::{Instruction, SectionBytecode};

use crate::executor::NativeFunction;
use crate::value::InstructionPointer;

use super::ir::{BinKind, KOp, Kernel, KernelIntrinsic, Reg};

/// why a lambda body stays in the interpreter
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    /// the body is not bracketed by the jump the compiler places before it
    NoBodyBounds,
    /// reference parameters alias the caller's heap slots
    ReferenceArgs,
    Unsupported(&'static str),
    /// a native without a kernel intrinsic, or called with the wrong arity
    Native,
    /// two paths reach the same instruction with different stack shapes
    InconsistentStack,
    /// a jump leaves the body
    JumpOutOfBody,
    TooManyRegisters,
    StackUnderflow,
}

pub struct LambdaShape {
    pub ip: InstructionPointer,
    pub required_args: u16,
    pub total_args: u16,
    pub capture_count: u16,
    pub has_reference_args: bool,
}

/// what an abstract stack position holds: a value in its own register, or a
/// reference the compiler pushed to write through to another register
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Slot {
    Val,
    Ref(Reg),
}

type Stack = Vec<Slot>;

/// a jump target still expressed as a bytecode offset; patched after emission
struct Pending {
    op_index: usize,
    target_pc: u32,
}

struct Translator<'a> {
    section: &'a SectionBytecode,
    natives: &'a [NativeFunction],
    start: u32,
    end: u32,
    stacks: Vec<Option<Stack>>,
    is_jump_target: Vec<bool>,
    ops: Vec<KOp>,
    pending: Vec<Pending>,
    op_index_of_pc: Vec<u32>,
    /// per op, whether it is the `Move` of a push (a candidate for folding
    /// into the op that consumes the temporary)
    push_moves: Vec<bool>,
    /// per op, the pc it was emitted for
    pc_of_op: Vec<u32>,
    /// the pc the op currently being emitted belongs to
    current_pc: u32,
    own_section: u16,
    /// the absolute register of stack position 0; nonzero for an inline block
    base: Reg,
    /// for an inline block, the register its value is returned into
    result: Option<Reg>,
    /// op indices of the jumps a block's returns leave for its end
    exits: Vec<usize>,
    /// inline blocks compiled during dataflow, by the pc of their `MakeLambda`
    inlined: std::collections::HashMap<u32, InlinedBlock>,
    /// one past the highest register any op of this body or its blocks touches
    max_register: usize,
}

/// a translated `block { ... }` ready to splice into its parent
struct InlinedBlock {
    ops: Vec<KOp>,
    /// indices into `ops` of jumps that leave the block
    exits: Vec<usize>,
    max_register: usize,
}

pub fn compile(
    section: &SectionBytecode,
    shape: &LambdaShape,
    natives: &[NativeFunction],
) -> Result<Kernel, Reject> {
    if shape.has_reference_args {
        return Err(Reject::ReferenceArgs);
    }
    let (start, end) = body_bounds(section, shape.ip)?;
    let mut translator = Translator::new(section, natives, shape.ip.0, start, end, 0, None);

    let entry: Stack = vec![Slot::Val; shape.total_args as usize + shape.capture_count as usize];
    translator.dataflow(entry)?;
    translator.emit()?;

    let frame_size =
        Reg::try_from(translator.max_register).map_err(|_| Reject::TooManyRegisters)?;

    Ok(Kernel {
        ip: shape.ip,
        required_args: shape.required_args,
        total_args: shape.total_args,
        capture_count: shape.capture_count,
        frame_size,
        ops: translator.ops.into_boxed_slice(),
    })
}

/// the instruction range of the body starting at `ip`: the compiler places a
/// jump over every closure body just before it
fn body_bounds(section: &SectionBytecode, ip: InstructionPointer) -> Result<(u32, u32), Reject> {
    let start = ip.1;
    match section.instructions.get(start.wrapping_sub(1) as usize) {
        Some(Instruction::Jump { section: s, to })
            if *s == ip.0 && *to > start && *to as usize <= section.instructions.len() =>
        {
            Ok((start, *to))
        }
        _ => Err(Reject::NoBodyBounds),
    }
}

enum Flow {
    Next,
    /// the instruction and the one after it were translated together
    SkipNext,
    Jump(u32),
    Branch(u32),
    Stop,
    /// a block's `return`: the last emitted op jumps to the block's end
    BlockExit,
}

impl<'a> Translator<'a> {
    fn new(
        section: &'a SectionBytecode,
        natives: &'a [NativeFunction],
        own_section: u16,
        start: u32,
        end: u32,
        base: Reg,
        result: Option<Reg>,
    ) -> Self {
        let len = (end - start) as usize;
        Self {
            section,
            natives,
            start,
            end,
            stacks: vec![None; len],
            is_jump_target: vec![false; len],
            ops: Vec::new(),
            pending: Vec::new(),
            op_index_of_pc: vec![u32::MAX; len],
            push_moves: Vec::new(),
            pc_of_op: Vec::new(),
            current_pc: start,
            own_section,
            base,
            result,
            exits: Vec::new(),
            inlined: std::collections::HashMap::new(),
            max_register: base as usize,
        }
    }

    /// the absolute register of abstract stack position `position`
    fn abs(&self, position: usize) -> Reg {
        self.base + position as Reg
    }

    fn note_depth(&mut self, stack: &Stack) {
        self.max_register = self.max_register.max(self.base as usize + stack.len());
    }

    fn dataflow(&mut self, entry: Stack) -> Result<(), Reject> {
        let mut worklist = vec![self.start];
        self.stacks[0] = Some(entry);

        while let Some(pc) = worklist.pop() {
            let mut stack = self.stacks[(pc - self.start) as usize]
                .clone()
                .expect("worklist entries have a recorded stack");
            let instr = self.section.instructions[pc as usize];
            self.note_depth(&stack);
            let mut sink = Vec::new();
            let flow = self.step(pc, instr, &mut stack, &mut sink)?;
            self.note_depth(&stack);

            let mut successors: Vec<u32> = Vec::new();
            let fallthrough = match flow {
                Flow::SkipNext => pc + 2,
                _ => pc + 1,
            };
            match flow {
                Flow::Next | Flow::SkipNext => successors.push(fallthrough),
                Flow::Jump(to) => successors.push(to),
                Flow::Branch(to) => {
                    successors.push(fallthrough);
                    successors.push(to);
                }
                Flow::Stop | Flow::BlockExit => {}
            }
            for successor in successors {
                if successor >= self.end || successor < self.start {
                    return Err(Reject::JumpOutOfBody);
                }
                if successor != fallthrough {
                    self.is_jump_target[(successor - self.start) as usize] = true;
                }
                let slot = &mut self.stacks[(successor - self.start) as usize];
                match slot {
                    None => {
                        *slot = Some(stack.clone());
                        worklist.push(successor);
                    }
                    Some(existing) if *existing != stack => return Err(Reject::InconsistentStack),
                    Some(_) => {}
                }
            }
        }
        Ok(())
    }

    fn emit(&mut self) -> Result<(), Reject> {
        for pc in self.start..self.end {
            let index = (pc - self.start) as usize;
            let Some(mut stack) = self.stacks[index].clone() else {
                continue;
            };
            self.current_pc = pc;
            self.op_index_of_pc[index] = self.ops.len() as u32;
            let instr = self.section.instructions[pc as usize];
            let before = self.ops.len();
            let mut emitted = Vec::new();
            let flow = self.step(pc, instr, &mut stack, &mut emitted)?;
            if matches!(flow, Flow::SkipNext) {
                // an inline block: its ops were translated during dataflow
                // and carry their own peephole, so they splice in verbatim
                self.splice_block(pc);
                if let Some(next) = self.op_index_of_pc.get_mut(index + 1) {
                    *next = self.ops.len() as u32;
                }
                continue;
            }
            if self.fold_store(pc, instr, &emitted) {
                emitted.clear();
            }
            for op in emitted {
                self.push_op(op);
            }
            if matches!(
                instr,
                Instruction::PushCopy { .. } | Instruction::PushDeepCopy { .. }
            ) && self.ops.len() == before + 1
            {
                self.push_moves[before] = true;
            }
            match flow {
                Flow::Jump(to) | Flow::Branch(to) => self.pending.push(Pending {
                    op_index: self.ops.len() - 1,
                    target_pc: to,
                }),
                Flow::BlockExit => self.exits.push(self.ops.len() - 1),
                Flow::Next | Flow::SkipNext | Flow::Stop => {}
            }
        }

        for pending in std::mem::take(&mut self.pending) {
            let target = self.op_index_of_pc[(pending.target_pc - self.start) as usize];
            debug_assert_ne!(target, u32::MAX, "jump into an unreachable instruction");
            match &mut self.ops[pending.op_index] {
                KOp::Jump { to }
                | KOp::JumpIf { to, .. }
                | KOp::JumpIfNot { to, .. }
                | KOp::RangeTest { to, .. } => *to = target,
                other => unreachable!("{other:?} carries no jump target"),
            }
        }
        Ok(())
    }

    /// append a compiled inline block's ops, rebasing its jumps and pointing
    /// its exits just past the block
    fn splice_block(&mut self, pc: u32) {
        let block = self
            .inlined
            .remove(&pc)
            .expect("inline blocks are compiled during dataflow");
        let splice_base = self.ops.len() as u32;
        let block_end = splice_base + block.ops.len() as u32;
        for (index, mut op) in block.ops.into_iter().enumerate() {
            let target = match &mut op {
                KOp::Jump { to }
                | KOp::JumpIf { to, .. }
                | KOp::JumpIfNot { to, .. }
                | KOp::RangeTest { to, .. } => Some(to),
                _ => None,
            };
            if let Some(to) = target {
                *to = if block.exits.contains(&index) {
                    block_end
                } else {
                    *to + splice_base
                };
            }
            self.ops.push(op);
            self.push_moves.push(false);
            self.pc_of_op.push(self.current_pc);
        }
        self.max_register = self.max_register.max(block.max_register);
    }

    /// translate the `block { ... }` whose closure is made at `pc` and called by
    /// the next instruction, given the parent's stack at that point
    fn inline_block(
        &mut self,
        pc: u32,
        capture_count: usize,
        prototype_index: u32,
        stack: &Stack,
    ) -> Result<(), Reject> {
        let proto = &self.section.lambda_prototypes[prototype_index as usize];
        if proto.required_args != 0
            || proto.default_arg_count != 0
            || proto.section != self.own_section
        {
            return Err(Reject::Unsupported("closure creation"));
        }
        if stack.len() < capture_count {
            return Err(Reject::StackUnderflow);
        }
        if self.inlined.contains_key(&pc) {
            return Ok(());
        }
        let (start, end) = body_bounds(self.section, (proto.section, proto.ip))?;
        let first_capture = stack.len() - capture_count;
        let base = self.abs(first_capture);
        let mut child = Translator::new(
            self.section,
            self.natives,
            self.own_section,
            start,
            end,
            base,
            Some(base),
        );
        // the block's captures are the parent's top slots, references included,
        // so a `var` captured by lvalue is written through to its register
        let entry: Stack = stack[first_capture..].to_vec();
        child.dataflow(entry)?;
        child.emit()?;
        self.inlined.insert(
            pc,
            InlinedBlock {
                ops: child.ops,
                exits: child.exits,
                max_register: child.max_register,
            },
        );
        Ok(())
    }

    /// append an op, folding the copies the stack discipline forced before
    /// it: `Move t <- s; ...; Op(.., t, ..)` becomes `Op(.., s, ..)` when the
    /// op consumes the temporary (its result lands at or below it, so nothing
    /// can read it again), the ops in between neither touch `t` nor write `s`,
    /// and no jump lands in between
    fn push_op(&mut self, mut op: KOp) {
        // the register the op consumes is judged once: for a branch or a
        // return it is the operand itself, which a fold rewrites
        let consumed = op_dst(&op);
        while let Some(index) = consumed.and_then(|dst| self.foldable_push_move(&op, dst)) {
            let KOp::Move { dst: temp, src } = self.ops[index] else {
                unreachable!("only moves are push moves");
            };
            replace_reads(&mut op, temp, src);
            self.ops.remove(index);
            self.push_moves.remove(index);
            self.pc_of_op.remove(index);
            for start in &mut self.op_index_of_pc {
                if *start != u32::MAX && *start as usize > index {
                    *start -= 1;
                }
            }
        }
        self.ops.push(op);
        self.push_moves.push(false);
        self.pc_of_op.push(self.current_pc);
    }

    fn foldable_push_move(&self, op: &KOp, dst: Reg) -> Option<usize> {
        if self.is_jump_target[(self.current_pc - self.start) as usize] {
            return None;
        }
        let window = self.ops.len().saturating_sub(FOLD_WINDOW)..self.ops.len();
        for index in window.rev() {
            let KOp::Move { dst: temp, src } = self.ops[index] else {
                continue;
            };
            if !self.push_moves[index] || !reads_register(op, temp) {
                continue;
            }
            if dst > temp {
                return None;
            }
            let transparent = self.ops[index + 1..].iter().all(|between| {
                !reads_register(between, temp)
                    && !writes_register(between, temp)
                    && !writes_register(between, src)
                    && !is_control_flow(between)
            });
            // instructions that emit no ops still have their own pc, and a
            // jump landing on one of them would skip the move
            let no_landing = (self.pc_of_op[index] + 1..=self.current_pc)
                .all(|pc| !self.is_jump_target[(pc - self.start) as usize]);
            return (transparent && no_landing).then_some(index);
        }
        None
    }

    /// `Op -> t; StoreLocal t -> local; Pop` becomes `Op -> local`. the value
    /// the statement leaves on the stack is popped straight away, so the
    /// temporary is never read, and the op's own reads happen before its write
    fn fold_store(&mut self, pc: u32, instr: Instruction, emitted: &[KOp]) -> bool {
        if !matches!(instr, Instruction::StoreLocal { .. }) {
            return false;
        }
        let [
            KOp::Move {
                dst: local,
                src: temp,
            },
        ] = emitted
        else {
            return false;
        };
        let pops_next = matches!(
            self.section.instructions.get(pc as usize + 1),
            Some(Instruction::Pop { count }) if *count >= 1
        );
        if !pops_next || self.is_jump_target[(pc - self.start) as usize] {
            return false;
        }
        let Some(last) = self.ops.last_mut() else {
            return false;
        };
        if op_dst(last) != Some(*temp) || !retarget(last, *local) {
            return false;
        }
        // the op now stores into the local; it is no push, whatever it was
        *self
            .push_moves
            .last_mut()
            .expect("an op was just retargeted") = false;
        true
    }

    fn resolve(&self, stack: &Stack, position: usize) -> Reg {
        match stack[position] {
            Slot::Val => self.abs(position),
            Slot::Ref(reg) => reg,
        }
    }

    fn position(stack: &Stack, delta: i32) -> Result<usize, Reject> {
        let index = stack.len() as i64 + i64::from(delta);
        if index < 0 || index >= stack.len() as i64 {
            return Err(Reject::StackUnderflow);
        }
        Ok(index as usize)
    }

    fn pop(stack: &mut Stack, count: usize) -> Result<(), Reject> {
        if stack.len() < count {
            return Err(Reject::StackUnderflow);
        }
        stack.truncate(stack.len() - count);
        Ok(())
    }

    /// apply one instruction to the abstract stack, emitting its ops. jump
    /// targets are left as bytecode offsets
    fn step(
        &mut self,
        pc: u32,
        instr: Instruction,
        stack: &mut Stack,
        out: &mut Vec<KOp>,
    ) -> Result<Flow, Reject> {
        // `depth` is the absolute register a pushed value lands in
        let depth = self.abs(stack.len());
        let (own_section, start, end) = (self.own_section, self.start, self.end);
        let jump_within = move |to: u32, section: u16| -> Result<u32, Reject> {
            if section != own_section || to < start || to >= end {
                return Err(Reject::JumpOutOfBody);
            }
            Ok(to)
        };

        match instr {
            Instruction::PushNil => {
                out.push(KOp::Nil { dst: depth });
                stack.push(Slot::Val);
            }
            Instruction::PushInt { index } => {
                let value = self.section.int_pool[index as usize];
                out.push(KOp::Int { dst: depth, value });
                stack.push(Slot::Val);
            }
            Instruction::PushFloat { index } => {
                let value = self.section.float_pool[index as usize];
                out.push(KOp::Float { dst: depth, value });
                stack.push(Slot::Val);
            }
            Instruction::PushEmptyList => {
                out.push(KOp::EmptyList { dst: depth });
                stack.push(Slot::Val);
            }
            Instruction::PushImaginary { .. } => return Err(Reject::Unsupported("complex")),
            Instruction::PushChar { .. } | Instruction::PushString { .. } => {
                return Err(Reject::Unsupported("string"));
            }
            Instruction::PushEmptyMap => return Err(Reject::Unsupported("map")),

            // a local that would live in a heap slot lives in its register
            // instead. that is sound because every write through a reference is
            // translated to a write of that register, and nothing else can
            // observe the slot
            Instruction::ConvertVar { .. } | Instruction::BindLocal => {}
            Instruction::ConvertParam { .. } | Instruction::ConvertMesh { .. } => {
                return Err(Reject::Unsupported("leader"));
            }
            Instruction::SyncAllLeaders => return Err(Reject::Unsupported("leader")),

            Instruction::StoreLocal { stack_delta } => {
                let target = self.resolve(stack, Self::position(stack, stack_delta)?);
                let src = self.resolve(stack, Self::position(stack, -1)?);
                out.push(KOp::Move { dst: target, src });
            }
            Instruction::PushDeepCopy { stack_delta } => {
                let src = self.resolve(stack, Self::position(stack, stack_delta)?);
                out.push(KOp::Move { dst: depth, src });
                stack.push(Slot::Val);
            }
            Instruction::PushCopy {
                stack_delta,
                pop_tos,
                ..
            } => {
                let src = self.resolve(stack, Self::position(stack, stack_delta)?);
                if pop_tos {
                    Self::pop(stack, 1)?;
                }
                let dst = self.abs(stack.len());
                out.push(KOp::Move { dst, src });
                stack.push(Slot::Val);
            }
            Instruction::PushLvalue { stack_delta, .. } => {
                let target = self.resolve(stack, Self::position(stack, stack_delta)?);
                stack.push(Slot::Ref(target));
            }
            Instruction::PushStateful { .. } => return Err(Reject::Unsupported("stateful")),
            Instruction::BufferLabelOrAttribute { .. } => {
                return Err(Reject::Unsupported("labels"));
            }
            Instruction::MakeLambda {
                capture_count,
                prototype_index,
            } => {
                // only a block: a closure made and called on the spot
                let called_at_once = matches!(
                    self.section.instructions.get(pc as usize + 1),
                    Some(Instruction::LambdaInvoke {
                        stateful: false,
                        labeled: false,
                        num_args: 0,
                    })
                );
                if !called_at_once || pc + 1 >= self.end {
                    return Err(Reject::Unsupported("closure creation"));
                }
                let capture_count = capture_count as usize;
                self.inline_block(pc, capture_count, prototype_index, stack)?;
                Self::pop(stack, capture_count)?;
                stack.push(Slot::Val);
                if pc + 2 >= self.end {
                    return Err(Reject::JumpOutOfBody);
                }
                return Ok(Flow::SkipNext);
            }
            Instruction::MakeAnim { .. } | Instruction::MakeOperator => {
                return Err(Reject::Unsupported("closure creation"));
            }
            Instruction::OperatorInvoke { .. } | Instruction::ConvertToLiveOperator => {
                return Err(Reject::Unsupported("operator"));
            }

            Instruction::LambdaInvoke {
                stateful,
                labeled,
                num_args,
            } => {
                if stateful || labeled {
                    return Err(Reject::Unsupported("labeled call"));
                }
                let arg_count = num_args as usize;
                if stack.len() < arg_count + 1 {
                    return Err(Reject::StackUnderflow);
                }
                let callee_pos = stack.len() - 1;
                let arg_start = callee_pos - arg_count;
                if stack[arg_start..].iter().any(|slot| *slot != Slot::Val) {
                    return Err(Reject::Unsupported("reference argument"));
                }
                out.push(KOp::Call {
                    callee: self.abs(callee_pos),
                    arg_start: self.abs(arg_start),
                    arg_count: num_args as u16,
                });
                Self::pop(stack, arg_count + 1)?;
                stack.push(Slot::Val);
            }

            Instruction::Jump { section, to } => {
                let to = jump_within(to, section)?;
                out.push(KOp::Jump { to });
                return Ok(Flow::Jump(to));
            }
            Instruction::ConditionalJump { section, to } => {
                let to = jump_within(to, section)?;
                let cond = self.resolve(stack, Self::position(stack, -1)?);
                Self::pop(stack, 1)?;
                out.push(KOp::JumpIf { cond, to });
                return Ok(Flow::Branch(to));
            }
            Instruction::JumpIfFalse { section, to } => {
                let to = jump_within(to, section)?;
                let cond = self.resolve(stack, Self::position(stack, -1)?);
                Self::pop(stack, 1)?;
                out.push(KOp::JumpIfNot { cond, to });
                return Ok(Flow::Branch(to));
            }
            Instruction::RangeLoopTest { current_delta, to } => {
                let to = jump_within(to, own_section)?;
                let current = Self::position(stack, i32::from(current_delta))?;
                if current + 1 >= stack.len()
                    || stack[current] != Slot::Val
                    || stack[current + 1] != Slot::Val
                {
                    return Err(Reject::Unsupported("range loop shape"));
                }
                out.push(KOp::RangeTest {
                    current: self.abs(current),
                    to,
                });
                return Ok(Flow::Branch(to));
            }
            Instruction::Return { .. } => {
                let src = self.resolve(stack, Self::position(stack, -1)?);
                if let Some(result) = self.result {
                    out.push(KOp::Move { dst: result, src });
                    out.push(KOp::Jump { to: u32::MAX });
                    return Ok(Flow::BlockExit);
                }
                out.push(KOp::Return { src });
                return Ok(Flow::Stop);
            }
            Instruction::Pop { count } => Self::pop(stack, count as usize)?,

            Instruction::NativeInvoke { index, arg_count } => {
                let intrinsic = self
                    .natives
                    .get(index as usize)
                    .and_then(|native| native.intrinsic)
                    .ok_or(Reject::Native)?;
                let arg_count = arg_count as usize;
                if intrinsic.arity() != arg_count || stack.len() < arg_count {
                    return Err(Reject::Native);
                }
                let arg_start = stack.len() - arg_count;
                if stack[arg_start..].iter().any(|slot| *slot != Slot::Val) {
                    return Err(Reject::Unsupported("reference argument"));
                }
                out.push(KOp::Native {
                    intrinsic,
                    arg_start: self.abs(arg_start),
                    arg_count: arg_count as u16,
                });
                Self::pop(stack, arg_count)?;
                stack.push(Slot::Val);
                if intrinsic == KernelIntrinsic::Fallthrough {
                    // always faults, so the body may end right after it
                    if pc + 1 >= self.end {
                        return Ok(Flow::Stop);
                    }
                }
            }
            Instruction::IncrementByOne { stack_delta } => {
                let reg = self.resolve(stack, Self::position(stack, stack_delta)?);
                out.push(KOp::Inc { reg });
            }
            Instruction::Play | Instruction::Observe => {
                return Err(Reject::Unsupported("side effect"));
            }
            Instruction::Negate => {
                let src = self.resolve(stack, Self::position(stack, -1)?);
                out.push(KOp::Neg {
                    dst: depth - 1,
                    src,
                });
                Self::pop(stack, 1)?;
                stack.push(Slot::Val);
            }
            Instruction::Not => {
                let src = self.resolve(stack, Self::position(stack, -1)?);
                out.push(KOp::Not {
                    dst: depth - 1,
                    src,
                });
                Self::pop(stack, 1)?;
                stack.push(Slot::Val);
            }
            Instruction::Subscript { mutable } => {
                if mutable {
                    return Err(Reject::Unsupported("element write"));
                }
                let list = self.resolve(stack, Self::position(stack, -2)?);
                let index = self.resolve(stack, Self::position(stack, -1)?);
                out.push(KOp::Index {
                    dst: depth - 2,
                    list,
                    index,
                });
                Self::pop(stack, 2)?;
                stack.push(Slot::Val);
            }
            Instruction::SubscriptLocal {
                stack_delta,
                depth: index_count,
                ..
            } => {
                let index_count = index_count as usize;
                if index_count == 0 || stack.len() < index_count {
                    return Err(Reject::StackUnderflow);
                }
                let list = self.resolve(stack, Self::position(stack, stack_delta)?);
                let first = stack.len() - index_count;
                let dst = self.abs(first);
                for (offset, position) in (first..stack.len()).enumerate() {
                    let index = self.resolve(stack, position);
                    out.push(KOp::Index {
                        dst,
                        list: if offset == 0 { list } else { dst },
                        index,
                    });
                }
                Self::pop(stack, index_count)?;
                stack.push(Slot::Val);
            }
            Instruction::ContainerLen { stack_delta }
            | Instruction::LenLocal { stack_delta, .. } => {
                let src = self.resolve(stack, Self::position(stack, stack_delta)?);
                out.push(KOp::Len { dst: depth, src });
                stack.push(Slot::Val);
            }
            Instruction::Attribute { .. } => return Err(Reject::Unsupported("attribute")),

            Instruction::Add
            | Instruction::Sub
            | Instruction::Mul
            | Instruction::Div
            | Instruction::Power
            | Instruction::Lt
            | Instruction::Le
            | Instruction::Gt
            | Instruction::Ge
            | Instruction::Eq
            | Instruction::Ne
            | Instruction::IntDiv
            | Instruction::In => {
                let op = match instr {
                    Instruction::Add => BinKind::Add,
                    Instruction::Sub => BinKind::Sub,
                    Instruction::Mul => BinKind::Mul,
                    Instruction::Div => BinKind::Div,
                    Instruction::Power => BinKind::Power,
                    Instruction::Lt => BinKind::Lt,
                    Instruction::Le => BinKind::Le,
                    Instruction::Gt => BinKind::Gt,
                    Instruction::Ge => BinKind::Ge,
                    Instruction::Eq => BinKind::Eq,
                    Instruction::Ne => BinKind::Ne,
                    Instruction::IntDiv => BinKind::IntDiv,
                    _ => BinKind::In,
                };
                let a = self.resolve(stack, Self::position(stack, -2)?);
                let b = self.resolve(stack, Self::position(stack, -1)?);
                out.push(KOp::Bin {
                    op,
                    dst: depth - 2,
                    a,
                    b,
                });
                Self::pop(stack, 2)?;
                stack.push(Slot::Val);
            }
            Instruction::Assign => {
                let Slot::Ref(target) = stack[Self::position(stack, -2)?] else {
                    return Err(Reject::Unsupported("assignment target"));
                };
                let src = self.resolve(stack, Self::position(stack, -1)?);
                out.push(KOp::Move { dst: target, src });
                Self::pop(stack, 2)?;
                stack.push(Slot::Ref(target));
            }
            Instruction::AppendAssign => {
                let Slot::Ref(target) = stack[Self::position(stack, -2)?] else {
                    return Err(Reject::Unsupported("append target"));
                };
                let value = self.resolve(stack, Self::position(stack, -1)?);
                out.push(KOp::Append {
                    list: target,
                    value,
                });
                Self::pop(stack, 2)?;
                stack.push(Slot::Ref(target));
            }
            Instruction::Append => {
                let list = self.resolve(stack, Self::position(stack, -2)?);
                let value = self.resolve(stack, Self::position(stack, -1)?);
                if list != self.abs(stack.len() - 2) {
                    // appending through a reference copies first in the
                    // interpreter; keep that shape out of the tier
                    return Err(Reject::Unsupported("append through reference"));
                }
                out.push(KOp::Append { list, value });
                Self::pop(stack, 1)?;
            }
            Instruction::EndOfExecutionHead => return Err(Reject::Unsupported("head end")),
        }

        if pc + 1 >= self.end {
            return Err(Reject::JumpOutOfBody);
        }
        Ok(Flow::Next)
    }
}

/// how far back the peephole looks for the push it can fold
const FOLD_WINDOW: usize = 8;

fn is_control_flow(op: &KOp) -> bool {
    matches!(
        op,
        KOp::Jump { .. }
            | KOp::JumpIf { .. }
            | KOp::JumpIfNot { .. }
            | KOp::RangeTest { .. }
            | KOp::Return { .. }
    )
}

/// whether `op` writes `reg`, including the argument registers a call
/// clobbers
fn writes_register(op: &KOp, reg: Reg) -> bool {
    match *op {
        KOp::Nil { dst }
        | KOp::Int { dst, .. }
        | KOp::Float { dst, .. }
        | KOp::Move { dst, .. }
        | KOp::Bin { dst, .. }
        | KOp::Neg { dst, .. }
        | KOp::Not { dst, .. }
        | KOp::EmptyList { dst }
        | KOp::Index { dst, .. }
        | KOp::Len { dst, .. } => dst == reg,
        KOp::Inc { reg: r } => r == reg,
        KOp::Append { list, .. } => list == reg,
        KOp::Call {
            arg_start,
            arg_count,
            ..
        }
        | KOp::Native {
            arg_start,
            arg_count,
            ..
        } => (arg_start..arg_start + arg_count).contains(&reg),
        KOp::Jump { .. }
        | KOp::JumpIf { .. }
        | KOp::JumpIfNot { .. }
        | KOp::RangeTest { .. }
        | KOp::Return { .. } => false,
    }
}

fn reads_register(op: &KOp, reg: Reg) -> bool {
    match *op {
        KOp::Nil { .. } | KOp::Int { .. } | KOp::Float { .. } | KOp::EmptyList { .. } => false,
        KOp::Jump { .. } => false,
        KOp::Move { src, .. }
        | KOp::Neg { src, .. }
        | KOp::Not { src, .. }
        | KOp::Len { src, .. } => src == reg,
        KOp::Bin { a, b, .. } => a == reg || b == reg,
        KOp::JumpIf { cond, .. } | KOp::JumpIfNot { cond, .. } => cond == reg,
        KOp::RangeTest { current, .. } => current == reg || current + 1 == reg,
        KOp::Inc { reg: r } => r == reg,
        KOp::Append { list, value } => list == reg || value == reg,
        KOp::Index { list, index, .. } => list == reg || index == reg,
        KOp::Call {
            callee,
            arg_start,
            arg_count,
        } => callee == reg || (arg_start..arg_start + arg_count).contains(&reg),
        KOp::Native {
            arg_start,
            arg_count,
            ..
        } => (arg_start..arg_start + arg_count).contains(&reg),
        KOp::Return { src } => src == reg,
    }
}

/// the register an op overwrites, for ops whose result register is free to
/// move. calls and natives write their first argument register, which the
/// peephole must not confuse with a movable destination
fn op_dst(op: &KOp) -> Option<Reg> {
    match *op {
        KOp::Nil { dst }
        | KOp::Int { dst, .. }
        | KOp::Float { dst, .. }
        | KOp::Move { dst, .. }
        | KOp::Bin { dst, .. }
        | KOp::Neg { dst, .. }
        | KOp::Not { dst, .. }
        | KOp::EmptyList { dst }
        | KOp::Index { dst, .. }
        | KOp::Len { dst, .. } => Some(dst),
        // a branch consumes its condition without producing anything, and a
        // return ends the frame; both leave the temporary dead
        KOp::JumpIf { cond, .. } | KOp::JumpIfNot { cond, .. } => Some(cond),
        KOp::Return { src } => Some(src),
        // the appended value is popped, so that temporary is dead as well
        KOp::Append { value, .. } => Some(value),
        KOp::Inc { .. }
        | KOp::RangeTest { .. }
        | KOp::Jump { .. }
        | KOp::Call { .. }
        | KOp::Native { .. } => None,
    }
}

/// point a value-producing op at another destination register
fn retarget(op: &mut KOp, to: Reg) -> bool {
    match op {
        KOp::Nil { dst }
        | KOp::Int { dst, .. }
        | KOp::Float { dst, .. }
        | KOp::Move { dst, .. }
        | KOp::Bin { dst, .. }
        | KOp::Neg { dst, .. }
        | KOp::Not { dst, .. }
        | KOp::EmptyList { dst }
        | KOp::Index { dst, .. }
        | KOp::Len { dst, .. } => {
            *dst = to;
            true
        }
        _ => false,
    }
}

fn replace_reads(op: &mut KOp, from: Reg, to: Reg) {
    let swap = |r: &mut Reg| {
        if *r == from {
            *r = to;
        }
    };
    match op {
        KOp::Move { src, .. }
        | KOp::Neg { src, .. }
        | KOp::Not { src, .. }
        | KOp::Len { src, .. } => swap(src),
        KOp::Bin { a, b, .. } => {
            swap(a);
            swap(b);
        }
        KOp::JumpIf { cond, .. } | KOp::JumpIfNot { cond, .. } => swap(cond),
        KOp::Index { list, index, .. } => {
            swap(list);
            swap(index);
        }
        KOp::Append { value, .. } => swap(value),
        KOp::Return { src } => swap(src),
        _ => {}
    }
}
