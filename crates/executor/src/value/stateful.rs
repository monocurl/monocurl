use smallvec::SmallVec;
use std::{
    cell::RefCell,
    rc::Rc,
};

use super::{RcValue, Value};
use crate::executor::ops::BinOp;

/// a stateful value node that tracks reactive dependencies.
/// only labeled lambda/operator calls (including 0-label calls) build stateful trees;
/// arithmetic sub-expressions containing $-refs are also valid as args.
#[derive(Clone)]
pub enum StatefulNode {
    /// reads the follower value of a param leader
    LeaderRef(RcValue),
    /// a constant value embedded in the graph
    Constant(Box<Value>),
    /// labeled lambda invocation whose args are stored as RcValues (may contain Value::Stateful)
    LabeledCall {
        func: Box<StatefulNode>,
        /// each element is an RcValue whose cell holds the current arg value (possibly Stateful)
        args: Vec<RcValue>,
        labels: SmallVec<[(usize, String); 4]>,
    },
    /// labeled operator invocation
    LabeledOperatorCall {
        operator: Box<StatefulNode>,
        /// RcValue so the operand can be reached via mutable attribute access
        operand: RcValue,
        extra_args: Vec<RcValue>,
        labels: SmallVec<[(usize, String); 4]>,
    },
    /// binary operation lifted over stateful operands
    BinaryOp {
        lhs: RcValue,
        rhs: RcValue,
        op: BinOp,
    },
    /// unary negation lifted over a stateful operand
    UnaryNeg(RcValue),
    /// logical not lifted over a stateful operand
    Not(RcValue),
    /// immutable subscript lifted over a stateful base
    Subscript {
        base: RcValue,
        index: RcValue,
    },
}

#[derive(Clone)]
pub struct Stateful {
    /// leader cells (RcValues containing Value::Leader) this expression depends on
    pub roots: Vec<RcValue>,
    pub root: StatefulNode,
    /// cached (versions_at_eval_time, result); versions parallel roots; boxed to break Value recursion
    cached: RefCell<Option<(Vec<u64>, Box<Value>)>>,
}

impl Stateful {
    pub fn new(roots: Vec<RcValue>, root: StatefulNode) -> Self {
        Self { roots, root, cached: RefCell::new(None) }
    }

    /// fast synchronous evaluation — only works for LeaderRef/Constant leaves.
    /// returns None when a lambda/operator call is needed (use Executor::eval_stateful instead).
    pub fn evaluate(&self) -> Option<Value> {
        eval_node_sync(&self.root)
    }

    /// check if all root versions match the cached snapshot
    pub fn cache_valid(&self) -> Option<Value> {
        let borrow = self.cached.borrow();
        let (versions, val) = borrow.as_ref()?;
        let still_valid = self.roots.iter().zip(versions.iter()).all(|(root, cached_ver)| {
            if let Value::Leader(leader) = &*root.borrow() {
                leader.follower_version == *cached_ver
            } else {
                false
            }
        });
        if still_valid { Some(*val.clone()) } else { None }
    }

    pub fn update_cache(&self, val: Value) {
        let versions: Vec<u64> = self.roots.iter().map(|root| {
            if let Value::Leader(leader) = &*root.borrow() {
                leader.follower_version
            } else {
                0
            }
        }).collect();
        *self.cached.borrow_mut() = Some((versions, Box::new(val)));
    }
}

fn eval_node_sync(node: &StatefulNode) -> Option<Value> {
    match node {
        StatefulNode::LeaderRef(rc) => {
            let inner = rc.borrow().clone();
            match inner {
                Value::Leader(leader) => Some(leader.follower_rc.borrow().clone()),
                other => Some(other),
            }
        }
        StatefulNode::Constant(val) => Some(*val.clone()),
        // function/operator calls and lifted ops need async evaluation
        StatefulNode::LabeledCall { .. }
        | StatefulNode::LabeledOperatorCall { .. }
        | StatefulNode::BinaryOp { .. }
        | StatefulNode::UnaryNeg(_)
        | StatefulNode::Not(_)
        | StatefulNode::Subscript { .. } => None,
    }
}

// ---------------------------------------------------------------------------
// tree-building helpers used by invoke.rs
// ---------------------------------------------------------------------------

/// decompose a value into (StatefulNode, roots) — used for func/operator nodes only.
/// args are now passed as RcValues directly, not converted to StatefulNode.
pub fn value_into_stateful_node(val: Value) -> (StatefulNode, Vec<RcValue>) {
    match val {
        Value::Stateful(s) => (s.root, s.roots),
        other => (StatefulNode::Constant(Box::new(other)), vec![]),
    }
}

/// collect stateful roots from a value, if it is stateful
pub fn collect_roots_from_value(val: &Value, roots: &mut Vec<RcValue>) {
    if let Value::Stateful(s) = val {
        roots.extend(s.roots.iter().cloned());
    }
}

/// collect roots from an RcValue cell
pub fn collect_roots_from_rc(rc: &RcValue, roots: &mut Vec<RcValue>) {
    collect_roots_from_value(&rc.borrow(), roots);
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
