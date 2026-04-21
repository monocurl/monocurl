mod diagnostics;
mod loop_state;
mod snapshot;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bytecode::{Bytecode, Instruction, SectionBytecode, SectionFlags};
use executor::{
    scene_snapshot::{BackgroundSnapshot, CameraSnapshot},
    time::Timestamp,
};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use geo::mesh::Mesh;
use structs::rope::{Rope, TextAggregate};

use crate::services::ServiceManagerMessage;

#[derive(Clone, Debug, PartialEq)]
pub enum ParameterValue {
    Int(i64),
    VectorInt(Vec<i64>),
    Float(f64),
    VectorFloat(Vec<f64>),
    Complex { re: f64, im: f64 },
    Camera(CameraSnapshot),
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParameterSnapshot {
    pub parameters: HashMap<String, ParameterValue>,
    pub locked_params: HashSet<String>,
    pub param_order: Vec<String>,
}

pub struct ExecutionSnapshot {
    pub background: Option<BackgroundSnapshot>,
    pub camera: Option<CameraSnapshot>,
    pub camera_version: Option<u64>,
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
