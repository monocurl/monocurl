#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(not(target_arch = "wasm32"))]
type JsValue = String;

#[cfg(target_arch = "wasm32")]
fn js_error(message: String) -> JsValue {
    JsValue::from_str(&message)
}

#[cfg(not(target_arch = "wasm32"))]
fn js_error(message: String) -> JsValue {
    message
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub struct Runtime {
    controller: runtime::RuntimeController,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
impl Runtime {
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen(constructor))]
    pub fn new() -> Self {
        Self {
            controller: runtime::RuntimeController::new(),
        }
    }

    pub fn monocurl_version(&self) -> String {
        bytecode::MONOCURL_VERSION.to_string()
    }

    pub fn supports_monocurl_version(&self, version: &str) -> bool {
        version == bytecode::MONOCURL_VERSION
    }

    pub fn native_function_count(&self) -> usize {
        stdlib::registry::registry().len()
    }

    pub fn bytecode_instruction_size(&self) -> usize {
        std::mem::size_of::<bytecode::Instruction>()
    }

    pub fn needs_work(&self) -> bool {
        self.controller.needs_work()
    }

    pub fn is_playing(&self) -> bool {
        self.controller.is_playing()
    }

    pub fn seek_to(&self, slide: usize, time: f64) {
        self.controller.apply_command(
            runtime::RuntimeCommand::SeekTo {
                target: executor::time::Timestamp::new(slide, time),
            },
            0.0,
        );
    }

    pub fn toggle_play(&self, now_seconds: f64) {
        self.controller
            .apply_command(runtime::RuntimeCommand::TogglePlay, now_seconds);
    }

    pub fn set_presentation_mode(&self) {
        self.controller.apply_command(
            runtime::RuntimeCommand::SetPlaybackMode(runtime::PlaybackMode::Presentation),
            0.0,
        );
    }

    pub fn set_preview_mode(&self) {
        self.controller.apply_command(
            runtime::RuntimeCommand::SetPlaybackMode(runtime::PlaybackMode::Preview),
            0.0,
        );
    }

    pub fn load_bytecode_json(&self, json: &str) -> Result<(), JsValue> {
        let bytecode = bytecode::Bytecode::from_versioned_json(json)
            .map_err(|error| js_error(error.to_string()))?;
        self.controller.apply_command(
            runtime::RuntimeCommand::UpdateBytecode {
                bytecode: Some(bytecode),
            },
            0.0,
        );
        Ok(())
    }

    pub async fn step(&self, now_seconds: f64) -> usize {
        self.controller
            .run_iteration(now_seconds)
            .await
            .snapshots
            .len()
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn engine_version() -> String {
    monocurl_version()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn monocurl_version() -> String {
    bytecode::MONOCURL_VERSION.to_string()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn renderer_size_probe() -> u32 {
    renderer::RenderSize::new(1, 1).width
}
