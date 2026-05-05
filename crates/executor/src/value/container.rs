use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use smallvec::SmallVec;

use crate::{
    error::ExecutorError,
    heap::{VRc, with_heap},
};

use super::Value;

#[derive(Clone)]
/// list whose elements are heap-allocated values accessed via owning heap refs.
pub struct List(Box<ListBody>);

#[derive(Clone)]
pub struct ListBody {
    pub(crate) elements: SmallVec<[VRc; 4]>,
}

impl List {
    pub fn new() -> Self {
        Self(Box::new(ListBody {
            elements: SmallVec::new(),
        }))
    }

    pub fn new_with(elements: impl IntoIterator<Item = VRc>) -> Self {
        Self(Box::new(ListBody {
            elements: elements.into_iter().collect(),
        }))
    }

    pub fn elements(&self) -> &[VRc] {
        &self.elements
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

impl Deref for List {
    type Target = ListBody;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for List {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

/// key types that can be used in a map.
#[derive(Clone, PartialEq, Eq, Hash)]
pub enum HashableKey {
    Integer(i64),
    Float(u64),
    String(String),
    List(Vec<HashableKey>),
}

impl HashableKey {
    fn float_bits(value: f64) -> u64 {
        if value == 0.0 {
            0.0f64.to_bits()
        } else if value.is_nan() {
            f64::NAN.to_bits()
        } else {
            value.to_bits()
        }
    }

    pub fn float_value(bits: u64) -> f64 {
        f64::from_bits(bits)
    }

    pub fn try_from_value(val: &Value) -> Result<Self, ExecutorError> {
        match val {
            Value::Integer(n) => Ok(HashableKey::Integer(*n)),
            Value::Float(f) => Ok(HashableKey::Float(Self::float_bits(*f))),
            Value::String(s) => Ok(HashableKey::String(s.to_string())),
            Value::List(list) => {
                let keys = list
                    .elements
                    .iter()
                    .map(|key| {
                        HashableKey::try_from_value(&with_heap(|h| h.get(key.key()).clone()))
                            .map_err(|e| e)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(HashableKey::List(keys))
            }
            other => Err(ExecutorError::UnhashableKey(other.type_name())),
        }
    }
}

#[derive(Clone)]
/// map whose values are heap-allocated and accessed via owning heap refs.
/// keys must be hashable (integers, strings, or lists of hashable types).
/// insertion_order tracks the order keys were first inserted so iteration is deterministic.
pub struct Map(Box<MapBody>);

#[derive(Clone)]
pub struct MapBody {
    pub entries: HashMap<HashableKey, VRc>,
    pub insertion_order: Vec<HashableKey>,
}

impl Map {
    pub fn new() -> Self {
        Self(Box::new(MapBody {
            entries: HashMap::new(),
            insertion_order: Vec::new(),
        }))
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

impl Deref for Map {
    type Target = MapBody;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Map {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
