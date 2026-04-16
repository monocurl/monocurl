use smallvec::SmallVec;
use std::rc::Rc;

use super::{RcValue, Value};

/// a stateful value node that tracks reactive dependencies.
/// only labeled lambda/operator calls (including 0-label calls) build stateful trees;
/// arithmetic is handled eagerly on the evaluated result.
#[derive(Clone)]
pub enum StatefulNode {
    /// reads the follower value of a state/param leader
    LeaderRef(RcValue),
    /// a constant value embedded in the graph
    Constant(Box<Value>),
    /// labeled lambda invocation (labels may be empty for 0-label calls)
    LabeledCall {
        func: Box<StatefulNode>,
        args: Vec<StatefulNode>,
        labels: SmallVec<[(usize, String); 4]>,
    },
    /// labeled operator invocation
    LabeledOperatorCall {
        operator: Box<StatefulNode>,
        operand: Box<StatefulNode>,
        extra_args: Vec<StatefulNode>,
        labels: SmallVec<[(usize, String); 4]>,
    },
}

#[derive(Clone)]
pub struct Stateful {
    /// leader cells (RcValues containing Value::Leader) this expression depends on
    pub roots: Vec<RcValue>,
    pub root: StatefulNode,
}

impl Stateful {
    /// evaluate the stateful expression using current follower values.
    /// returns None for LabeledCall/LabeledOperatorCall nodes (require async via Executor).
    pub fn evaluate(&self) -> Option<Value> {
        eval_node(&self.root)
    }
}

fn eval_node(node: &StatefulNode) -> Option<Value> {
    match node {
        StatefulNode::LeaderRef(rc) => {
            let inner = rc.borrow().clone();
            match inner {
                Value::Leader(leader) => Some(leader.follower_rc.borrow().clone()),
                other => Some(other),
            }
        }
        StatefulNode::Constant(val) => Some(*val.clone()),
        // function/operator calls need async evaluation via Executor
        StatefulNode::LabeledCall { .. } | StatefulNode::LabeledOperatorCall { .. } => None,
    }
}

// ---------------------------------------------------------------------------
// tree-building helpers used by invoke.rs
// ---------------------------------------------------------------------------

/// decompose a value into (StatefulNode, roots) for inserting into a stateful tree
pub fn value_into_stateful_parts(val: Value) -> (StatefulNode, Vec<RcValue>) {
    match val {
        Value::Stateful(s) => (s.root, s.roots),
        other => (StatefulNode::Constant(Box::new(other)), vec![]),
    }
}

/// remove duplicate roots (by Rc pointer identity) in-place
pub fn dedup_roots_by_ptr(roots: &mut Vec<RcValue>) {
    let mut seen: Vec<RcValue> = vec![];
    roots.retain(|r| {
        if seen.iter().any(|s| Rc::ptr_eq(s, r)) {
            false
        } else {
            seen.push(r.clone());
            true
        }
    });
}
