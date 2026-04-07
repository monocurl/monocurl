use std::{collections::BTreeMap};

use bytecode::{SectionBytecode};

use crate::{value::RcValue, vheap::{VHeap, VHeapPtr}};

pub struct Timestamp {
    slide: usize,
    time: f64,
}

pub struct ExecutionStack {
    // for determining what to lerp
    stack_id: usize,
    var_stack: Vec<VHeapPtr>,
    ip_stack: Vec<(u16, u32)>,
    // index into string labels
    label_buffer: Vec<usize>,
    conditional_flag: bool,

    active_child_count: usize,
    parent_stack_idx: Option<usize>,
}

struct BakedPrimitiveAnim {
    anim_id: usize,
    start_time: f64,
    end_time: f64,
    parent_stack_idx: Option<usize>
}

pub struct ExecutionState {
    pub timestamp: Timestamp,

    vheap: VHeap,

    stack_counter: usize,
    execution_stacks: Vec<Option<ExecutionStack>>,
    primitive_anims: Vec<BakedPrimitiveAnim>,
    execution_heads: Vec<usize>,

    error_state: Vec<u8>,

    mesh_followers: Vec<RcValue>,
    state_followers: Vec<RcValue>,
    parameter_followers: Vec<RcValue>,

    // for each anim block id
    dirty_followers: BTreeMap<i64, Vec<u8>>
}

pub struct SlideCache {
    execution_snapshot: ExecutionState,
    bytecode: SectionBytecode,
}
