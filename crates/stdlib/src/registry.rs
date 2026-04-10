use std::collections::HashMap;
use std::sync::OnceLock;


pub(crate) type StdFunc = fn(i32) -> i32;

pub(crate) struct FunctionEntry {
    pub name: &'static str,
    pub func: StdFunc,
}

inventory::collect!(FunctionEntry);

pub struct Registry {
    entries: Vec<&'static FunctionEntry>,
    index_map: HashMap<&'static str, usize>,
}

impl Registry {
    fn build() -> Self {
        let mut entries: Vec<&'static FunctionEntry> =
            inventory::iter::<FunctionEntry>().collect();
        entries.sort_unstable_by_key(|e| e.name);

        let index_map = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name, i))
            .collect();

        Self { entries, index_map }
    }

    #[inline]
    pub fn index_of(&self, name: &str) -> usize {
        *self.index_map.get(name).unwrap()
    }

    #[inline]
    pub fn call_by_index(&self, idx: usize, arg: i32) -> i32 {
        (self.entries[idx].func)(arg)
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::build)
}
