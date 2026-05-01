use std::sync::{OnceLock, RwLock};

use crate::{
    LatexBackendConfig, SystemBackendConfig, SystemBackendStatus, SystemToolPaths, cache, system,
};

static BACKEND_CONFIG: OnceLock<RwLock<LatexBackendConfig>> = OnceLock::new();

pub fn backend_config() -> LatexBackendConfig {
    backend_config_lock().read().unwrap().clone()
}

pub fn set_backend_config(config: LatexBackendConfig) {
    let mut current = backend_config_lock().write().unwrap();
    if *current != config {
        *current = config;
        cache::clear_memory_cache();
    }
}

pub fn discover_system_backend() -> SystemToolPaths {
    system::discover_backend()
}

pub fn system_backend_status(config: &SystemBackendConfig) -> SystemBackendStatus {
    system::backend_status(config)
}

fn backend_config_lock() -> &'static RwLock<LatexBackendConfig> {
    BACKEND_CONFIG.get_or_init(|| RwLock::new(LatexBackendConfig::Bundled))
}
