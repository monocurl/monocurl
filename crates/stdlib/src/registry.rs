use std::collections::HashMap;
use std::sync::OnceLock;

use executor::executor::StdlibFunc;

pub struct FunctionEntry {
    pub name: &'static str,
    pub func: StdlibFunc,
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

    /// build a function table (Vec<NativeFunc>) ordered by index,
    /// suitable for passing to the executor.
    pub fn func_table(&self) -> Vec<StdlibFunc> {
        self.entries.iter().map(|e| e.func).collect()
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
