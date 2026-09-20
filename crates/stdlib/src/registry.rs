use std::collections::HashMap;
use std::sync::OnceLock;

use executor::executor::{NativeFunction, StdlibFunc, StdlibSyncFunc};

pub struct FunctionEntry {
    pub name: &'static str,
    pub func: StdlibFunc,
    /// present when the native never suspends, letting the interpreter call it
    /// without allocating and polling a future
    pub sync_func: Option<StdlibSyncFunc>,
}

inventory::collect!(FunctionEntry);

pub struct Registry {
    entries: Vec<&'static FunctionEntry>,
    index_map: HashMap<&'static str, usize>,
}

impl Registry {
    fn build() -> Self {
        let mut entries: Vec<&'static FunctionEntry> = inventory::iter::<FunctionEntry>().collect();
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

    /// build the executor's native function table, ordered by index
    pub fn func_table(&self) -> Vec<NativeFunction> {
        self.entries
            .iter()
            .map(|entry| NativeFunction {
                call: entry.func,
                call_sync: entry.sync_func,
            })
            .collect()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

static REGISTRY: OnceLock<Registry> = OnceLock::new();

pub fn registry() -> &'static Registry {
    REGISTRY.get_or_init(Registry::build)
}
