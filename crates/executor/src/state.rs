use std::{collections::BTreeSet, rc::Rc};

use smallvec::SmallVec;
use structs::text::Span8;

use crate::{
    time::Timestamp,
    value::{
        InstructionPointer, RcValue, Value, container::List, leader::Leader, primitive_anim::PrimitiveAnim, rc_value
    },
};

/// a single execution context (analogous to a thread / coroutine).
/// each anim block spawns a new execution stack.
#[derive(Clone)]
pub struct ExecutionStack {
    /// unique id for tracking which stack last touched a leader
    pub stack_id: usize,
    /// operand / variable stack
    pub var_stack: Vec<Value>,
    /// current instruction pointer
    pub ip: InstructionPointer,
    /// call return addresses (pushed on LambdaInvoke, popped on Return)
    pub call_stack: Vec<InstructionPointer>,
    /// buffered label string-pool indices for labeled invocations
    pub label_buffer: SmallVec<[u32; 8]>,
    /// set by comparison instructions, consumed by ConditionalJump
    pub conditional_flag: bool,
    /// number of child execution stacks (or primitive animations) still running
    pub active_child_count: usize,
    /// index of the parent execution stack (None for the root)
    pub parent_idx: Option<usize>,
}

impl ExecutionStack {
    pub fn new(stack_id: usize, ip: InstructionPointer, parent_idx: Option<usize>) -> Self {
        Self {
            stack_id,
            var_stack: Vec::new(),
            ip,
            call_stack: Vec::new(),
            label_buffer: SmallVec::new(),
            conditional_flag: false,
            active_child_count: 0,
            parent_idx,
        }
    }

    pub fn push(&mut self, val: Value) {
        self.var_stack.push(val);
    }

    pub fn pop(&mut self) -> Value {
        self.var_stack.pop().expect("stack underflow")
    }

    pub fn peek(&self) -> &Value {
        self.var_stack.last().expect("stack underflow")
    }

    /// read a value at an offset from the current stack top.
    /// stack_delta is typically negative (pointing below TOS).
    /// the compiler computes delta as (target_position - current_stack_depth).
    pub fn read_at(&self, stack_delta: i32) -> &Value {
        let idx = (self.var_stack.len() as i32 + stack_delta) as usize;
        &self.var_stack[idx]
    }

    pub fn stack_len(&self) -> usize {
        self.var_stack.len()
    }

    pub fn pop_n(&mut self, n: usize) {
        let new_len = self.var_stack.len() - n;
        self.var_stack.truncate(new_len);
    }
}

/// a primitive animation that has been "baked" with timing info
#[derive(Clone)]
pub struct BakedPrimitiveAnim {
    pub anim: PrimitiveAnim,
    pub start_time: f64,
    pub end_time: f64,
    /// which execution stack spawned this (to resume when finished)
    pub parent_stack_idx: usize,
    /// which stack id for leader tracking
    pub stack_id: usize,
    pub span: Span8
}

/// a leader-follower pair entry for quick lookup
#[derive(Clone)]
pub struct LeaderEntry {
    /// the RcValue containing Value::Leader
    pub leader_rc: RcValue,
    pub kind: LeaderKind,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LeaderKind {
    Mesh,
    State,
    Param,
}

/// max sum of call_stack depths across all active stacks before we report overflow.
pub const MAX_CALL_DEPTH: usize = 2000;
// max number of concurrent execution heads
pub const MAX_EXECUTION_HEADS: usize = 1000;

#[derive(Clone)]
pub struct ExecutionState {
    pub timestamp: Timestamp,

    global_stack_counter: usize,
    // execution stacks that have not finished yet
    pub alive_stack_count: usize,
    /// execution stacks always appended, never reused, None = finished.
    pub execution_stacks: Vec<Option<ExecutionStack>>,
    /// indices of currently active execution heads (stacks awaiting a Play)
    pub execution_heads: BTreeSet<usize>,
    /// currently running primitive animations
    pub primitive_anims: Vec<BakedPrimitiveAnim>,

    /// all leader-follower pairs registered during execution
    pub leaders: Vec<LeaderEntry>,

    /// strong refs for force_ephemeral lvalues (captured variables that may outlive
    /// their var_stack slot). cleared at section boundaries.
    pub ephemeral_pool: Vec<RcValue>,

    /// accumulated error messages
    pub errors: Vec<(String, Span8)>,

    /// values captured from the top of root execution stacks when they finish.
    /// used primarily for testing: the final TOS of each completed root head
    /// is pushed here so callers can inspect results after execution.
    #[cfg(feature = "capture_tos")]
    pub captured_output: Vec<Value>,

    /// sum of call_stack depths across all active stacks.
    /// incremented on every lambda call, decremented on every return.
    /// checked before each invocation to detect stack overflows.
    pub call_depth: usize,
}

impl ExecutionState {
    pub const ROOT_STACK_ID: usize = 0;

    pub fn new() -> Self {
        let mut ret = Self {
            timestamp: Timestamp::default(),
            global_stack_counter: 0,
            alive_stack_count: 0,
            execution_stacks: Vec::new(),
            execution_heads: BTreeSet::new(),
            primitive_anims: Vec::new(),
            leaders: Vec::new(),
            ephemeral_pool: Vec::new(),
            errors: Vec::new(),
            #[cfg(feature = "capture_tos")]
            captured_output: Vec::new(),
            call_depth: 0,
        };

        let ip: InstructionPointer = (0, 0);
        let stack_idx = ret.alloc_stack(ip, None).unwrap();
        debug_assert_eq!(stack_idx, ExecutionState::ROOT_STACK_ID);

        let mut heads = BTreeSet::new();
        heads.insert(stack_idx);

        ret.execution_heads = heads;

        ret
    }

    /// allocate a fresh execution stack and return its index.
    /// always appends — indices are never reused.
    pub fn alloc_stack(&mut self, ip: InstructionPointer, parent_idx: Option<usize>) -> Result<usize, ()> {
        if self.alive_stack_count >= MAX_EXECUTION_HEADS {
            return Err(());
        }

        self.alive_stack_count += 1;
        let id = self.global_stack_counter;
        self.global_stack_counter += 1;
        let stack = ExecutionStack::new(id, ip, parent_idx);
        let idx = self.execution_stacks.len();
        self.execution_stacks.push(Some(stack));

        Ok(idx)
    }

    /// free an execution stack slot
    pub fn free_stack(&mut self, idx: usize) {
        debug_assert!(self.execution_stacks[idx].is_some());
        self.alive_stack_count -= 1;
        self.execution_stacks[idx] = None;
    }

    pub fn stack(&self, idx: usize) -> &ExecutionStack {
        self.execution_stacks[idx].as_ref().expect("dead stack")
    }

    pub fn stack_mut(&mut self, idx: usize) -> &mut ExecutionStack {
        self.execution_stacks[idx].as_mut().expect("dead stack")
    }

    /// promote TOS to a heap variable: wrap in RcValue, keep the strong ref on the
    /// var_stack (as Value::Lvalue). the push_lvalue instruction will create weak refs.
    pub fn promote_to_var(&mut self, stack_idx: usize) {
        let stack = self.stack_mut(stack_idx);
        let val = stack.pop();
        let rc = rc_value(val);
        stack.push(Value::Lvalue(rc));
    }

    /// promote TOS to a leader-follower variable (mesh/state/param).
    pub fn promote_to_leader(&mut self, stack_idx: usize, kind: LeaderKind) {
        let stack = self.stack_mut(stack_idx);
        let init_val = stack.pop();

        let leader_rc = rc_value(init_val.clone());
        // mesh follower starts as []; state/param follower starts as initial value
        let follower_init = match kind {
            LeaderKind::Mesh => Value::List(Rc::new(List::new())),
            LeaderKind::State | LeaderKind::Param => init_val,
        };
        let follower_rc = rc_value(follower_init);

        let leader_val = Value::Leader(Leader {
            last_modified_stack: None,
            leader_rc: leader_rc.clone(),
            follower_rc: follower_rc.clone(),
        });
        let leader_cell = rc_value(leader_val);

        self.leaders.push(LeaderEntry {
            leader_rc: leader_cell.clone(),
            kind,
        });

        self.stack_mut(stack_idx).push(Value::Lvalue(leader_cell));
    }

    pub fn error(&mut self, msg: impl Into<String>, span: Span8) {
        self.errors.push((msg.into(), span));
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn leader_value(leader: &Leader) -> Value {
        leader.leader_rc.borrow().clone()
    }

    pub fn follower_value(leader: &Leader) -> Value {
        leader.follower_rc.borrow().clone()
    }
}
