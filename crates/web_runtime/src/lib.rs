#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

pub const BYTECODE_ABI_VERSION: u32 = 0;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Runtime {
    native_function_count: usize,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Runtime {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        Self {
            native_function_count: stdlib::registry::registry().len(),
        }
    }

    pub fn bytecode_abi_version(&self) -> u32 {
        BYTECODE_ABI_VERSION
    }

    pub fn native_function_count(&self) -> usize {
        self.native_function_count
    }

    pub fn bytecode_instruction_size(&self) -> usize {
        std::mem::size_of::<bytecode::Instruction>()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn renderer_size_probe() -> u32 {
    renderer::RenderSize::new(1, 1).width
}
