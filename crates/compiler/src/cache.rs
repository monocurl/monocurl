use std::sync::Arc;

use crate::compiler::CompileBundle;

// caches library compilation
#[derive(Default)]
pub struct CompilerCache {
    pub(crate) last_bundles: Vec<Arc<CompileBundle>>,
}
