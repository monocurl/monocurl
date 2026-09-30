//! the jit's interface where it is not built (wasm32, or the `jit` feature
//! off): nothing is ever compiled, so the typed and lane machines run

use std::sync::{Arc, atomic::AtomicBool};

use super::{
    KernelStats,
    run::Fault,
    typed::Spec,
    value::{ClosureArena, KVal},
};

pub enum JitEntry {}

pub struct JitCache;

impl JitCache {
    pub fn new(_enabled: bool) -> Self {
        Self
    }

    pub fn prepare(&mut self, _spec: &Spec, _stats: &mut KernelStats) -> Option<Arc<JitEntry>> {
        None
    }
}

pub enum JitVm {}

impl JitVm {
    pub fn new(
        entry: Arc<JitEntry>,
        _spec: &Spec,
        _arena: &ClosureArena,
        _abort: Option<Arc<AtomicBool>>,
    ) -> Self {
        match *entry {}
    }

    pub fn call(
        &mut self,
        _spec: &Spec,
        _arena: &ClosureArena,
        _args: &[KVal],
    ) -> Result<KVal, Fault> {
        match *self {}
    }
}

pub fn enabled_by_env() -> bool {
    false
}
