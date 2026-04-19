use smallvec::SmallVec;
use std::{cell::RefCell, rc::Rc};

use super::{RcValue, Value};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatefulReadKind {
    Leader,
    Follower,
}

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
}

#[derive(Clone)]
pub struct Stateful {
    /// leader cells (RcValues containing Value::Leader) this expression depends on
    pub roots: Vec<RcValue>,
    pub root: StatefulNode,
    /// whether stateful leader refs read code-side leaders or on-screen followers
    pub read_kind: StatefulReadKind,
    /// cached (versions_at_eval_time, result); versions parallel roots; boxed to break Value recursion
    cached: RefCell<Option<(Vec<u64>, Box<Value>)>>,
}

impl Stateful {
    pub fn new(mut roots: Vec<RcValue>, root: StatefulNode, read_kind: StatefulReadKind) -> Self {
        dedup_roots_by_ptr(&mut roots);
        Self {
            roots,
            root,
            read_kind,
            cached: RefCell::new(None),
        }
    }

    pub fn to_follower_read(&self) -> Self {
        let mut ret = self.clone();
        if ret.read_kind != StatefulReadKind::Follower {
            ret.read_kind = StatefulReadKind::Follower;
            ret.reset_cache();
        }
        ret
    }

    pub fn reset_cache(&self) {
        self.cached.borrow_mut().take();
    }

    /// check if all root versions match the cached snapshot
    pub fn cache_valid(&self) -> Option<Value> {
        let borrow = self.cached.borrow();
        let (versions, val) = borrow.as_ref()?;
        let still_valid = self
            .roots
            .iter()
            .zip(versions.iter())
            .all(|(root, cached_ver)| {
                if let Value::Leader(leader) = &*root.borrow() {
                    match self.read_kind {
                        StatefulReadKind::Leader => leader.leader_version == *cached_ver,
                        StatefulReadKind::Follower => leader.follower_version == *cached_ver,
                    }
                } else {
                    false
                }
            });
        if still_valid {
            Some(*val.clone())
        } else {
            None
        }
    }

    pub fn update_cache(&self, val: Value) {
        let versions: Vec<u64> = self
            .roots
            .iter()
            .map(|root| {
                if let Value::Leader(leader) = &*root.borrow() {
                    match self.read_kind {
                        StatefulReadKind::Leader => leader.leader_version,
                        StatefulReadKind::Follower => leader.follower_version,
                    }
                } else {
                    0
                }
            })
            .collect();
        *self.cached.borrow_mut() = Some((versions, Box::new(val)));
    }
}

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
fn dedup_roots_by_ptr(roots: &mut Vec<RcValue>) {
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
