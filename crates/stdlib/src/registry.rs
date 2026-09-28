use std::collections::HashMap;
use std::sync::OnceLock;

use executor::{
    executor::{NativeFunction, StdlibFunc, StdlibSyncFunc},
    kernel::KernelIntrinsic,
};

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
                intrinsic: kernel_intrinsic(entry.name),
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

/// the natives the kernel tier evaluates itself. each entry must match the
/// stdlib function of that name exactly, argument handling included; the
/// `kernel_tier` scene tests cross-check them against the interpreter
fn kernel_intrinsic(name: &str) -> Option<KernelIntrinsic> {
    use KernelIntrinsic::*;
    Some(match name {
        "sqrt" => Sqrt,
        "cbrt" => Cbrt,
        "exp" => Exp,
        "ln" => Ln,
        "sin" => Sin,
        "cos" => Cos,
        "tan" => Tan,
        "arcsin" => Asin,
        "arccos" => Acos,
        "arctan" => Atan,
        "sinh" => Sinh,
        "cosh" => Cosh,
        "tanh" => Tanh,
        "pow" => Pow,
        "arctan2" => Atan2,
        "abs" => Abs,
        "sign" => Sign,
        "floor" => Floor,
        "ceil" => Ceil,
        "round" => Round,
        "trunc" => Trunc,
        "mod_func" => Mod,
        "min" => Min,
        "max" => Max,
        "dot" => Dot,
        "cross" => Cross,
        "len" | "list_len" => Len,
        "to_int" => ToInt,
        "to_float" => ToFloat,
        "lambda_fallthrough_error" => Fallthrough,
        _ => return None,
    })
}
