mod diagnostics;
mod loop_state;
mod snapshot;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bytecode::{Bytecode, Instruction, SectionBytecode, SectionFlags};
use executor::time::Timestamp;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use geo::{mesh::Mesh, simd::Float3};
use structs::rope::{Rope, TextAggregate};

use crate::services::ServiceManagerMessage;

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterValue {
    Int(i64),
    VectorInt(Vec<i64>),
    Float(f64),
    VectorFloat(Vec<f64>),
    Complex { re: f64, im: f64 },
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParameterSnapshot {
    pub parameters: HashMap<String, ParameterValue>,
    pub locked_params: HashSet<String>,
    pub param_order: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewportCameraSnapshot {
    pub position: Float3,
    pub look_at: Float3,
    pub up: Float3,
    pub fov: f32,
    pub near: f32,
    pub far: f32,
    pub ortho: bool,
}

impl Default for ViewportCameraSnapshot {
    fn default() -> Self {
        Self {
            position: Float3::new(0.0, 0.0, -10.0),
            look_at: Float3::ZERO,
            up: Float3::Y,
            fov: 0.698_131_7,
            near: 0.1,
            far: 100.0,
            ortho: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportBackgroundSnapshot {
    pub color: (f32, f32, f32, f32),
}

impl Default for ViewportBackgroundSnapshot {
    fn default() -> Self {
        Self {
            color: (0.0, 0.0, 0.0, 1.0),
        }
    }
}

pub struct ExecutionSnapshot {
    pub background: Option<ViewportBackgroundSnapshot>,
    pub camera: Option<ViewportCameraSnapshot>,
    pub meshes: Option<Vec<Arc<Mesh>>>,
    pub current_timestamp: Timestamp,
    pub status: ExecutionStatus,
    pub slide_count: usize,
    pub slide_durations: Vec<Option<f64>>,
    pub minimum_slide_durations: Vec<Option<f64>>,
    pub parameters: Option<ParameterSnapshot>,
}

#[derive(Clone, Copy, PartialEq)]
pub enum PlaybackMode {
    Presentation,
    Preview,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    Playing,
    Paused,
    Seeking,
    RuntimeError,
    CompileError,
}

impl PlaybackMode {
    pub fn default_time_interval(&self) -> f64 {
        match self {
            PlaybackMode::Presentation => 1.0 / 120.0,
            PlaybackMode::Preview => 1.0 / 60.0,
        }
    }
}

pub enum ExecutionMessage {
    UpdateBytecode {
        bytecode: Option<Bytecode>,
        root_text_rope: Rope<TextAggregate>,
        version: usize,
    },
    SetPlaybackMode(PlaybackMode),
    UpdateParameters {
        updates: HashMap<String, ParameterValue>,
    },
    TogglePlay,
    SeekTo {
        target: Timestamp,
    },
}

pub struct ExecutionService {
    rx: UnboundedReceiver<ExecutionMessage>,
    sm_tx: UnboundedSender<ServiceManagerMessage>,
}

fn default_bytecode() -> Bytecode {
    let mut section = SectionBytecode::new(SectionFlags {
        is_stdlib: true,
        is_library: true,
        is_init: false,
        is_root_module: true,
    });
    section.instructions.push(Instruction::EndOfExecutionHead);
    Bytecode::new(vec![Arc::new(section)])
}
