use super::{RcValue, Value};

/// list whose elements are reference-counted for lvalue semantics.
/// `list[i]` returns a mutable lvalue by cloning the Rc.
/// COW: if `Rc::strong_count > 1`, clone inner value into new Rc before mutating.
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

/// map whose values are reference-counted for lvalue semantics.
/// keys are stored inline (by value) for lookup.
#[derive(Clone)]
pub struct Map {
    pub entries: Vec<(Value, RcValue)>,
}

impl Map {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// find the index of an entry by key (linear scan)
    pub fn find_key(&self, key: &Value) -> Option<usize> {
        self.entries.iter().position(|(k, _)| values_equal(k, key))
    }
}

/// structural equality for map key lookup
fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Nil, Value::Nil) => true,
        _ => false,
    }
}
