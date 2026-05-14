use std::sync::{OnceLock, RwLock};

use crate::{LatexBackendConfig, SystemBackendConfig, SystemBackendStatus, SystemToolPaths, cache};

#[cfg(not(target_arch = "wasm32"))]
use crate::system;

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
    #[cfg(not(target_arch = "wasm32"))]
    {
        system::discover_backend()
    }

    #[cfg(target_arch = "wasm32")]
    {
        SystemToolPaths {
            latex: None,
            dvisvgm: None,
        }
    }
}

pub fn system_backend_status(config: &SystemBackendConfig) -> SystemBackendStatus {
    #[cfg(not(target_arch = "wasm32"))]
    {
        system::backend_status(config)
    }

    #[cfg(target_arch = "wasm32")]
    {
        let _ = config;
        SystemBackendStatus {
            latex: false,
            dvisvgm: false,
        }
    }
}

fn backend_config_lock() -> &'static RwLock<LatexBackendConfig> {
    BACKEND_CONFIG.get_or_init(|| RwLock::new(LatexBackendConfig::Bundled))
}
