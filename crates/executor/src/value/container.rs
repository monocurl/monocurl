use std::collections::HashMap;
use std::hash::Hash;

use smallvec::SmallVec;

use crate::error::ExecutorError;

use super::{RcValue, Value};

/// list whose elements are reference-counted / COW for lvalue semantics.
#[derive(Clone)]
pub struct List {
    pub elements: SmallVec<[RcValue; 4]>,
}

impl List {
    pub fn new() -> Self {
        Self {
            elements: SmallVec::new()
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
/// allowed: integers, strings, and vectors of hashable types.
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
                    .map(|rc| HashableKey::try_from_value(&rc.borrow()))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(HashableKey::Vector(keys))
            }
            other => Err(ExecutorError::UnhashableKey(other.type_name())),
        }
    }
}

/// map whose values are reference-counted for lvalue semantics.
/// keys must be hashable (integers, strings, or vectors of hashable types).
/// insertion_order tracks the order keys were first inserted so iteration is deterministic.
#[derive(Clone)]
pub struct Map {
    pub entries: HashMap<HashableKey, RcValue>,
    /// keys in their original insertion order (no duplicates)
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

    /// insert or overwrite a key; records insertion order on first insert.
    pub fn insert(&mut self, key: HashableKey, value: RcValue) {
        if !self.entries.contains_key(&key) {
            self.insertion_order.push(key.clone());
        }
        self.entries.insert(key, value);
    }

    pub fn get(&self, key: &HashableKey) -> Option<&RcValue> {
        self.entries.get(key)
    }

    pub fn get_mut(&mut self, key: &HashableKey) -> Option<&mut RcValue> {
        self.entries.get_mut(key)
    }

    pub fn contains_key(&self, key: &HashableKey) -> bool {
        self.entries.contains_key(key)
    }

    /// iterate in insertion order
    pub fn iter(&self) -> impl Iterator<Item = (&HashableKey, &RcValue)> {
        self.insertion_order
            .iter()
            .filter_map(|k| self.entries.get(k).map(|v| (k, v)))
    }
}
