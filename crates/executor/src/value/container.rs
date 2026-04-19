use std::collections::HashMap;

use smallvec::SmallVec;

use crate::{
    error::ExecutorError,
    heap::{VRc, with_heap},
};

use super::Value;

#[derive(Clone)]
/// list whose elements are heap-allocated values accessed via owning heap refs.
pub struct List {
    pub elements: SmallVec<[VRc; 4]>,
}

impl List {
    pub fn new() -> Self {
        Self {
            elements: SmallVec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

/// key types that can be used in a map.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum HashableKey {
    Integer(i64),
    String(String),
    Vector(Vec<HashableKey>),
}

impl HashableKey {
    pub fn try_from_value(val: &Value) -> Result<Self, ExecutorError> {
        match val {
            Value::Integer(n) => Ok(HashableKey::Integer(*n)),
            Value::String(s) => Ok(HashableKey::String(s.clone())),
            Value::List(list) => {
                let keys = list
                    .elements
                    .iter()
                    .map(|key| {
                        HashableKey::try_from_value(&with_heap(|h| h.get(key.key()).clone()))
                            .map_err(|e| e)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(HashableKey::Vector(keys))
            }
            other => Err(ExecutorError::UnhashableKey(other.type_name())),
        }
    }
}

#[derive(Clone)]
/// map whose values are heap-allocated and accessed via owning heap refs.
/// keys must be hashable (integers, strings, or vectors of hashable types).
/// insertion_order tracks the order keys were first inserted so iteration is deterministic.
pub struct Map {
    pub entries: HashMap<HashableKey, VRc>,
    pub insertion_order: Vec<HashableKey>,
}

impl Map {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            insertion_order: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn insert(&mut self, key: HashableKey, value: VRc) {
        if !self.entries.contains_key(&key) {
            self.insertion_order.push(key.clone());
        }
        self.entries.insert(key, value);
    }

    pub fn get(&self, key: &HashableKey) -> Option<&VRc> {
        self.entries.get(key)
    }

    pub fn get_mut(&mut self, key: &HashableKey) -> Option<&mut VRc> {
        self.entries.get_mut(key)
    }

    pub fn contains_key(&self, key: &HashableKey) -> bool {
        self.entries.contains_key(key)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&HashableKey, &VRc)> {
        self.insertion_order
            .iter()
            .filter_map(|k| self.entries.get(k).map(|v| (k, v)))
    }
}
