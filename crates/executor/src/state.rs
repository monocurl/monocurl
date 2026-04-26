use std::collections::BTreeSet;

use smallvec::SmallVec;
use structs::text::Span8;

use crate::{
    error::RuntimeError,
    heap::{HeapKey, VRc, heap_alloc, heap_replace, with_heap},
    time::Timestamp,
    value::{
        InstructionPointer, Value, container::List, leader::Leader, primitive_anim::PrimitiveAnim,
    },
};

/// a single execution context (analogous to a thread / coroutine).
#[derive(Clone)]
pub struct ExecutionStack {
    pub var_stack: Vec<Value>,
    pub retained_prefix_len: usize,
    pub ip: InstructionPointer,
    pub call_stack: Vec<InstructionPointer>,
    pub label_buffer: SmallVec<[u32; 8]>,
    pub conditional_flag: bool,
    pub active_child_count: usize,
    pub parent_idx: Option<usize>,
    pub trace_parent_idx: Option<usize>,
}

#[derive(Clone)]
pub struct ExecutionStackGhost {
    pub ip: InstructionPointer,
    pub call_stack: Vec<InstructionPointer>,
    pub parent_idx: Option<usize>,
    pub trace_parent_idx: Option<usize>,
}

#[derive(Clone)]
pub enum ExecutionStackSlot {
    Alive(ExecutionStack),
    Ghost(ExecutionStackGhost),
}

impl ExecutionStack {
    pub fn new(
        ip: InstructionPointer,
        parent_idx: Option<usize>,
        trace_parent_idx: Option<usize>,
    ) -> Self {
        Self {
            var_stack: Vec::new(),
            retained_prefix_len: 0,
            ip,
            call_stack: Vec::new(),
            label_buffer: SmallVec::new(),
            conditional_flag: false,
            active_child_count: 0,
            parent_idx,
            trace_parent_idx,
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

    pub fn pop_n_retaining_prefix(&mut self, n: usize) {
        let new_len = self.var_stack.len() - n;
        self.var_stack
            .truncate(new_len.max(self.retained_prefix_len));
    }

    pub fn set_retained_prefix_len(&mut self, len: usize) {
        debug_assert!(len <= self.var_stack.len());
        self.retained_prefix_len = len;
    }

    pub fn truncate_to_retained_prefix(&mut self) {
        self.var_stack.truncate(self.retained_prefix_len);
    }
}

impl ExecutionStackSlot {
    fn as_alive(&self) -> Option<&ExecutionStack> {
        match self {
            Self::Alive(stack) => Some(stack),
            Self::Ghost(_) => None,
        }
    }

    fn as_alive_mut(&mut self) -> Option<&mut ExecutionStack> {
        match self {
            Self::Alive(stack) => Some(stack),
            Self::Ghost(_) => None,
        }
    }

    fn ip(&self) -> InstructionPointer {
        match self {
            Self::Alive(stack) => stack.ip,
            Self::Ghost(stack) => stack.ip,
        }
    }

    fn call_stack(&self) -> &[InstructionPointer] {
        match self {
            Self::Alive(stack) => &stack.call_stack,
            Self::Ghost(stack) => &stack.call_stack,
        }
    }

    fn parent_idx(&self) -> Option<usize> {
        match self {
            Self::Alive(stack) => stack.parent_idx,
            Self::Ghost(stack) => stack.parent_idx,
        }
    }

    fn trace_parent_idx(&self) -> Option<usize> {
        match self {
            Self::Alive(stack) => stack.trace_parent_idx,
            Self::Ghost(stack) => stack.trace_parent_idx,
        }
    }
}

#[derive(Clone)]
pub struct BakedPrimitiveAnim {
    pub anim_id: usize,
    pub anim: PrimitiveAnim,
    pub start_time: f64,
    pub end_time: f64,
    /// owning VRcs to leader cell slots
    pub targets: Vec<VRc>,
    pub destinations: Vec<Value>,
    pub embedded_starts: Vec<Value>,
    pub embedded_ends: Vec<Value>,
    pub embedded_states: Vec<Value>,
    pub parent_stack_idx: usize,
    pub span: Span8,
}

#[derive(Clone)]
pub struct LeaderEntry {
    pub name: String,
    /// owning VRc to the slot containing Value::Leader
    pub leader_cell: VRc,
    /// non-owning HeapKey for the leader value slot (owned by Leader inside leader_cell)
    pub leader_value: HeapKey,
    /// non-owning HeapKey for the follower value slot (owned by Leader inside leader_cell)
    pub follower_value: HeapKey,
    pub kind: LeaderKind,
}

#[derive(Clone)]
pub struct ActiveParam {
    pub name: String,
    pub leader_cell: VRc,
    pub leader_value: HeapKey,
    pub follower_value: HeapKey,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LeaderKind {
    Mesh,
    Param,
}

pub const MAX_CALL_DEPTH: usize = 2000;
pub const MAX_EXECUTION_HEADS: usize = 1000;

#[derive(Clone)]
pub struct ExecutionState {
    pub timestamp: Timestamp,
    pub pending_playback_time: f64,
    pub last_stack_idx: usize,
    rng_state: u64,

    global_primitive_anim_counter: usize,
    pub alive_stack_count: usize,
    pub execution_stacks: Vec<ExecutionStackSlot>,
    pub execution_heads: BTreeSet<usize>,
    pub primitive_anims: Vec<BakedPrimitiveAnim>,

    pub leaders: Vec<LeaderEntry>,
    pub active_params: Vec<ActiveParam>,

    /// strong VRc refs for stateful args; keeps them alive across the section
    pub ephemeral_pool: Vec<VRc>,

    pub errors: Vec<RuntimeError>,

    #[cfg(feature = "capture_tos")]
    pub captured_output: Vec<Value>,

    pub call_depth: usize,
}

impl ExecutionState {
    pub const ROOT_STACK_IDX: usize = 0;
    const RNG_GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;
    const RNG_TIMESTAMP_BASIS: u64 = 0xA076_1D64_78BD_642F;

    pub fn new() -> Self {
        let timestamp = Timestamp::right_before_slide(0);
        let mut ret = Self {
            timestamp,
            pending_playback_time: 0.0,
            last_stack_idx: Self::ROOT_STACK_IDX,
            rng_state: Self::seed_random_state(timestamp),
            global_primitive_anim_counter: 0,
            alive_stack_count: 0,
            execution_stacks: Vec::new(),
            execution_heads: BTreeSet::new(),
            primitive_anims: Vec::new(),
            leaders: Vec::new(),
            active_params: Vec::new(),
            ephemeral_pool: Vec::new(),
            errors: Vec::new(),
            #[cfg(feature = "capture_tos")]
            captured_output: Vec::new(),
            call_depth: 0,
        };

        let ip: InstructionPointer = (0, 0);
        let stack_idx = ret.alloc_stack(ip, None, None).unwrap();
        debug_assert_eq!(stack_idx, ExecutionState::ROOT_STACK_IDX);

        let mut heads = BTreeSet::new();
        heads.insert(stack_idx);
        ret.execution_heads = heads;

        ret
    }

    fn mix_random_bits(mut x: u64) -> u64 {
        x ^= x >> 30;
        x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
        x ^= x >> 27;
        x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
        x ^ (x >> 31)
    }

    fn seed_random_state(timestamp: Timestamp) -> u64 {
        let slide = timestamp.slide as u64;
        let time_bits = timestamp.time.to_bits();
        Self::mix_random_bits(
            Self::RNG_TIMESTAMP_BASIS ^ slide.rotate_left(17) ^ time_bits.rotate_right(11),
        )
    }

    #[inline]
    pub fn next_random_u64(&mut self) -> u64 {
        let timestamp_salt = Self::seed_random_state(self.timestamp);
        self.rng_state = self
            .rng_state
            .wrapping_add(timestamp_salt)
            .wrapping_add(Self::RNG_GAMMA);
        Self::mix_random_bits(self.rng_state)
    }

    #[inline]
    pub fn next_random_f64(&mut self) -> f64 {
        (self.next_random_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    pub fn alloc_stack(
        &mut self,
        ip: InstructionPointer,
        parent_idx: Option<usize>,
        trace_parent_idx: Option<usize>,
    ) -> Result<usize, ()> {
        if self.alive_stack_count >= MAX_EXECUTION_HEADS {
            return Err(());
        }

        self.alive_stack_count += 1;
        let stack = ExecutionStack::new(ip, parent_idx, trace_parent_idx);
        let idx = self.execution_stacks.len();
        self.execution_stacks.push(ExecutionStackSlot::Alive(stack));

        Ok(idx)
    }

    pub fn alloc_primitive_anim_id(&mut self) -> usize {
        let id = self.global_primitive_anim_counter;
        self.global_primitive_anim_counter += 1;
        id
    }

    pub fn free_stack(&mut self, idx: usize) {
        let ghost = match &mut self.execution_stacks[idx] {
            ExecutionStackSlot::Alive(stack) => ExecutionStackGhost {
                ip: stack.ip,
                call_stack: std::mem::take(&mut stack.call_stack),
                parent_idx: stack.parent_idx,
                trace_parent_idx: stack.trace_parent_idx,
            },
            ExecutionStackSlot::Ghost(_) => panic!("free_stack called on ghost stack"),
        };
        self.alive_stack_count -= 1;
        self.execution_stacks[idx] = ExecutionStackSlot::Ghost(ghost);
    }

    pub fn stack(&self, idx: usize) -> &ExecutionStack {
        self.execution_stacks[idx]
            .as_alive()
            .expect("ghost stack has no live frame")
    }

    pub fn stack_mut(&mut self, idx: usize) -> &mut ExecutionStack {
        self.execution_stacks[idx]
            .as_alive_mut()
            .expect("ghost stack has no live frame")
    }

    pub fn stack_ip(&self, idx: usize) -> InstructionPointer {
        self.execution_stacks[idx].ip()
    }

    pub fn stack_call_stack(&self, idx: usize) -> &[InstructionPointer] {
        self.execution_stacks[idx].call_stack()
    }

    pub fn stack_trace_parent_idx(&self, idx: usize) -> Option<usize> {
        self.execution_stacks[idx].trace_parent_idx()
    }

    pub fn stack_parent_idx(&self, idx: usize) -> Option<usize> {
        self.execution_stacks[idx].parent_idx()
    }

    pub fn is_stack_ancestor_of_stack(
        &self,
        ancestor_stack_idx: usize,
        descendant_stack_idx: usize,
    ) -> bool {
        let mut cursor = Some(descendant_stack_idx);
        while let Some(idx) = cursor {
            let slot = &self.execution_stacks[idx];
            if idx == ancestor_stack_idx {
                return true;
            }
            cursor = slot.parent_idx().or(slot.trace_parent_idx());
        }
        false
    }

    /// promote TOS to a heap variable: wrap in VRc, keep the strong ref on the
    /// var_stack (as Value::Lvalue). the push_lvalue instruction will create weak refs.
    pub fn promote_to_var(&mut self, stack_idx: usize) {
        let stack = self.stack_mut(stack_idx);
        let val = stack.pop();
        let vrc = VRc::new(val);
        stack.push(Value::Lvalue(vrc));
    }

    /// promote TOS to a leader-follower variable (mesh/param).
    pub fn promote_to_leader(&mut self, stack_idx: usize, kind: LeaderKind, name: String) {
        let stack = self.stack_mut(stack_idx);
        let init_val = stack.pop().elide_lvalue().elide_leader();

        let leader_key = heap_alloc(init_val.clone());
        let follower_init = match kind {
            LeaderKind::Mesh => Value::List(List::new()),
            LeaderKind::Param => init_val,
        };
        let follower_key = heap_alloc(follower_init);

        let leader_val = Value::Leader(Leader {
            kind,
            last_modified_stack: if kind == LeaderKind::Mesh {
                Some(stack_idx)
            } else {
                None
            },
            locked_by_anim: None,
            leader_rc: VRc::from_retained(leader_key),
            leader_version: 0,
            follower_rc: VRc::from_retained(follower_key),
            follower_version: 0,
        });
        let cell_vrc = VRc::new(leader_val);

        self.leaders.push(LeaderEntry {
            name: name.clone(),
            leader_cell: cell_vrc.clone(),
            leader_value: leader_key,
            follower_value: follower_key,
            kind,
        });

        if kind == LeaderKind::Param {
            self.active_params.push(ActiveParam {
                name,
                leader_cell: cell_vrc.clone(),
                leader_value: leader_key,
                follower_value: follower_key,
            });
        }

        self.stack_mut(stack_idx).push(Value::Lvalue(cell_vrc));
    }

    pub fn sync_all_leaders(&self) {
        for entry in &self.leaders {
            let cell_val = with_heap(|h| h.get(entry.leader_cell.key()).clone());
            if let Value::Leader(leader) = cell_val {
                let value =
                    with_heap(|h| h.get(leader.leader_rc.key()).clone()).to_follower_stateful();
                heap_replace(leader.follower_rc.key(), value);
                // update last_modified_stack and follower_version in the slot
                crate::heap::with_heap_mut(|h| {
                    if let Value::Leader(l) = &mut *h.get_mut(entry.leader_cell.key()) {
                        l.last_modified_stack = None;
                        l.follower_version += 1;
                    }
                });
            }
        }
    }

    pub fn error(&mut self, error: RuntimeError) {
        self.errors.push(error);
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn leader_value(leader: &Leader) -> Value {
        with_heap(|h| h.get(leader.leader_rc.key()).clone())
    }

    pub fn follower_value(leader: &Leader) -> Value {
        with_heap(|h| h.get(leader.follower_rc.key()).clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promote_to_param_tracks_leader_metadata_and_active_params() {
        let mut state = ExecutionState::new();
        state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .push(Value::Integer(7));

        state.promote_to_leader(
            ExecutionState::ROOT_STACK_IDX,
            LeaderKind::Param,
            "speed".into(),
        );

        assert_eq!(state.leaders.len(), 1);
        assert_eq!(state.active_params.len(), 1);
        assert_eq!(state.leaders[0].name, "speed");
        assert_eq!(state.active_params[0].name, "speed");
        assert_eq!(
            state.leaders[0].leader_cell.key(),
            state.active_params[0].leader_cell.key()
        );
        assert_eq!(
            state.leaders[0].leader_value,
            state.active_params[0].leader_value
        );
        assert_eq!(
            state.leaders[0].follower_value,
            state.active_params[0].follower_value
        );
    }

    #[test]
    fn promote_to_leader_elides_top_level_lvalue_init() {
        let mut state = ExecutionState::new();
        state
            .stack_mut(ExecutionState::ROOT_STACK_IDX)
            .push(Value::Lvalue(crate::heap::VRc::new(Value::Integer(7))));

        state.promote_to_leader(
            ExecutionState::ROOT_STACK_IDX,
            LeaderKind::Param,
            "speed".into(),
        );

        let leader_key = state.leaders[0].leader_value;
        match with_heap(|h| h.get(leader_key).clone()) {
            Value::Integer(7) => {}
            other => panic!(
                "expected top-level lvalue to be elided, got {}",
                other.type_name()
            ),
        }
    }

    #[test]
    fn freed_stack_ghosts_preserve_ancestry() {
        let mut state = ExecutionState::new();
        let child_idx = state.alloc_stack(
            (0, 1),
            Some(ExecutionState::ROOT_STACK_IDX),
            Some(ExecutionState::ROOT_STACK_IDX),
        );
        let child_idx = child_idx.expect("child alloc should succeed");
        let child_stack_idx = child_idx;

        let trace_idx = state.alloc_stack((0, 2), None, Some(child_idx));
        let trace_idx = trace_idx.expect("trace alloc should succeed");
        let trace_stack_idx = trace_idx;

        state.free_stack(trace_idx);

        assert!(state.is_stack_ancestor_of_stack(trace_stack_idx, trace_idx));
        assert!(state.is_stack_ancestor_of_stack(child_stack_idx, trace_idx));
        assert!(state.is_stack_ancestor_of_stack(ExecutionState::ROOT_STACK_IDX, trace_idx));
    }

    #[test]
    fn random_sequence_advances_and_survives_clone() {
        let mut state = ExecutionState::new();

        let first = state.next_random_u64();
        let second = state.next_random_u64();
        assert_ne!(first, second);

        let mut cloned = state.clone();
        assert_eq!(state.next_random_u64(), cloned.next_random_u64());
    }
}
