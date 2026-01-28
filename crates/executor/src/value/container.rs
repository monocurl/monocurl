use std::collections::HashMap;

use crate::value::RcValue;

pub struct Map {
    pub map: HashMap<RcValue, RcValue>,
    hash_cache: Option<usize>
}

pub struct List {
    pub vec: Vec<RcValue>,
    hash_cache: Option<usize>
}
