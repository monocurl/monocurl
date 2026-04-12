use std::collections::HashMap;
use std::hash::Hash;

use crate::error::ExecutorError;

use super::{RcValue, Value};

/// list whose elements are reference-counted / COW for lvalue semantics.
#[derive(Clone)]
pub struct List {
    pub elements: Vec<RcValue>,
}

impl List {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
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
                let keys = list.elements.iter()
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
#[derive(Clone)]
pub struct Map {
    pub entries: HashMap<HashableKey, RcValue>,
}

impl Map {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
