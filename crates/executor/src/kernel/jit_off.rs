//! the jit's interface where it is not built (wasm32, or the `jit` feature
//! off): nothing is ever compiled, so the typed and lane machines run

use std::sync::{Arc, atomic::AtomicBool};

use super::{
    KernelStats,
    input::NumArg,
    output::FLAT_MAX,
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

    pub(crate) fn run_args(
        &mut self,
        _spec: &Spec,
        _arena: &ClosureArena,
        _args: &[KVal],
    ) -> Result<(), Fault> {
        match *self {}
    }

    pub(crate) fn run_nums<'n>(
        &mut self,
        _spec: &Spec,
        _arena: &ClosureArena,
        _call: impl Iterator<Item = NumArg<'n>>,
        _defaults: &[KVal],
    ) -> Result<(), Fault> {
        match *self {}
    }

    pub(crate) fn result(&self, _spec: &Spec) -> KVal {
        match *self {}
    }

    pub(crate) fn flat(&self, _spec: &Spec, _width: usize) -> Option<[f32; FLAT_MAX]> {
        match *self {}
    }
}

pub fn enabled_by_env() -> bool {
    false
}
