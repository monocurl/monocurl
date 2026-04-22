use std::{cell::RefCell, rc::Rc};

use smallvec::SmallVec;

use crate::heap::{HeapKey, VRc, with_heap};

use super::{Value, rc_cached::RcCached};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StatefulReadKind {
    Leader,
    Follower,
}

/// a stateful value node that tracks reactive dependencies.
/// roots are non-owning leader-cell keys — owned by LeaderEntry VRcs.
/// args/operands own their heap slots directly.
#[derive(Clone)]
pub enum StatefulNode {
    /// reads a leader cell (HeapKey → Value::Leader)
    LeaderRef(HeapKey),
    Constant(Box<Value>),
    LabeledCall {
        func: Box<StatefulNode>,
        /// owning refs to slots holding the arg values
        args: Vec<VRc>,
        labels: SmallVec<[(usize, String); 4]>,
    },
    LabeledOperatorCall {
        operator: Box<StatefulNode>,
        operand: VRc,
        extra_args: Vec<VRc>,
        labels: SmallVec<[(usize, String); 4]>,
    },
}

pub struct StatefulBody {
    /// non-owning HeapKeys to leader cells (LeaderEntry owns the VRc)
    pub roots: Vec<HeapKey>,
    pub root: StatefulNode,
}

pub struct StatefulCache {
    pub read_kind: StatefulReadKind,
    pub cached: RefCell<Option<(Vec<u64>, Box<Value>)>>,
}

impl Clone for StatefulCache {
    fn clone(&self) -> Self {
        let cached = self.cached.borrow().clone();
        Self {
            read_kind: self.read_kind,
            cached: RefCell::new(cached),
        }
    }
}

pub type Stateful = RcCached<StatefulBody, StatefulCache>;

pub fn make_stateful(
    roots: Vec<HeapKey>,
    root: StatefulNode,
    read_kind: StatefulReadKind,
) -> Stateful {
    let mut deduped = Vec::<HeapKey>::new();
    for key in roots {
        if !deduped.contains(&key) {
            deduped.push(key);
        }
    }
    RcCached {
        body: Rc::new(StatefulBody {
            roots: deduped,
            root,
        }),
        cache: StatefulCache {
            read_kind,
            cached: RefCell::new(None),
        },
    }
}

pub fn to_follower_stateful(s: &Stateful) -> Stateful {
    if s.cache.read_kind == StatefulReadKind::Follower {
        return s.clone();
    }
    RcCached {
        body: Rc::clone(&s.body),
        cache: StatefulCache {
            read_kind: StatefulReadKind::Follower,
            cached: RefCell::new(None),
        },
    }
}

pub fn stateful_cache_valid(s: &Stateful) -> Option<Value> {
    let borrow = s.cache.cached.borrow();
    let (versions, val) = borrow.as_ref()?;
    let still_valid = s
        .body
        .roots
        .iter()
        .zip(versions.iter())
        .all(|(&key, &cached_ver)| {
            with_heap(|h| {
                if let Value::Leader(leader) = &*h.get(key) {
                    match s.cache.read_kind {
                        StatefulReadKind::Leader => leader.leader_version == cached_ver,
                        StatefulReadKind::Follower => leader.follower_version == cached_ver,
                    }
                } else {
                    false
                }
            })
        });
    if still_valid {
        Some(*val.clone())
    } else {
        None
    }
}

pub fn stateful_update_cache(s: &Stateful, val: Value) {
    let versions: Vec<u64> = s
        .body
        .roots
        .iter()
        .map(|&key| {
            with_heap(|h| {
                if let Value::Leader(leader) = &*h.get(key) {
                    match s.cache.read_kind {
                        StatefulReadKind::Leader => leader.leader_version,
                        StatefulReadKind::Follower => leader.follower_version,
                    }
                } else {
                    0
                }
            })
        })
        .collect();
    *s.cache.cached.borrow_mut() = Some((versions, Box::new(val)));
}

pub fn reset_stateful_cache(s: &Stateful) {
    s.cache.cached.borrow_mut().take();
}

// tree-building helpers

/// decompose a value into (StatefulNode, roots)
pub fn value_into_stateful_node(val: Value) -> (StatefulNode, Vec<HeapKey>) {
    match val {
        Value::Stateful(s) => (s.body.root.clone(), s.body.roots.clone()),
        other => (StatefulNode::Constant(Box::new(other)), vec![]),
    }
}

/// collect stateful roots from a value
pub fn collect_roots_from_value(val: &Value, roots: &mut Vec<HeapKey>) {
    match val {
        Value::Stateful(s) => {
            roots.extend(s.body.roots.iter().copied());
        }
        Value::Lvalue(vrc) => {
            let inner = with_heap(|h| h.get(vrc.key()).clone());
            collect_roots_from_value(&inner, roots);
        }
        Value::WeakLvalue(vweak) => {
            let inner = with_heap(|h| h.get(vweak.key()).clone());
            collect_roots_from_value(&inner, roots);
        }
        Value::Leader(leader) => {
            let inner = with_heap(|h| h.get(leader.leader_rc.key()).clone());
            collect_roots_from_value(&inner, roots);
        }
        _ => {}
    }
}
