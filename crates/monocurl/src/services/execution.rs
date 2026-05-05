mod diagnostics;
mod loop_state;
mod snapshot;

use std::{collections::HashMap, sync::Arc};

use bytecode::{Bytecode, Instruction, SectionBytecode, SectionFlags};
use executor::{
    scene_snapshot::{BackgroundSnapshot, CameraSnapshot},
    time::Timestamp,
    transcript::SectionTranscript,
    value::MeshAttributePathSegment,
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

impl ParameterValue {
    pub fn is_supported_control(&self) -> bool {
        match self {
            Self::Int(_) | Self::Float(_) | Self::Complex { .. } => true,
            Self::VectorInt(values) => values.len() == 2,
            Self::VectorFloat(values) => values.len() == 2,
            Self::Camera(_) | Self::Other => false,
        }
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PresentationUpdateTarget {
    Scene {
        leader_index: usize,
    },
    MeshAttribute {
        leader_index: usize,
        path: Vec<MeshAttributePathSegment>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParameterEntrySnapshot {
    pub target: PresentationUpdateTarget,
    pub name: String,
    pub value: ParameterValue,
    pub locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshAttributeSnapshot {
    pub target: Option<PresentationUpdateTarget>,
    pub name: String,
    pub value: ParameterValue,
    pub children: Vec<MeshAttributeSnapshot>,
}

impl MeshAttributeSnapshot {
    pub fn has_supported_control(&self) -> bool {
        self.value.is_supported_control()
            || self
                .children
                .iter()
                .any(MeshAttributeSnapshot::has_supported_control)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeshEntrySnapshot {
    pub leader_index: usize,
    pub name: String,
    pub locked: bool,
    pub attributes: Vec<MeshAttributeSnapshot>,
}

impl MeshEntrySnapshot {
    pub fn has_supported_control(&self) -> bool {
        self.attributes
            .iter()
            .any(MeshAttributeSnapshot::has_supported_control)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ParameterSnapshot {
    pub params: Vec<ParameterEntrySnapshot>,
    pub meshes: Vec<MeshEntrySnapshot>,
}

pub struct ExecutionSnapshot {
    pub background: Option<BackgroundSnapshot>,
    pub camera: Option<CameraSnapshot>,
    pub camera_version: Option<u64>,
    pub meshes: Option<Vec<Arc<Mesh>>>,
    pub current_timestamp: Timestamp,
    pub target_timestamp: Timestamp,
    pub status: ExecutionStatus,
    pub is_loading: bool,
    pub slide_count: usize,
    pub slide_names: Vec<Option<String>>,
    pub slide_durations: Vec<Option<f64>>,
    pub minimum_slide_durations: Vec<Option<f64>>,
    pub parameters: Option<ParameterSnapshot>,
    pub transcript: Option<Vec<Arc<SectionTranscript>>>,
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
        updates: HashMap<PresentationUpdateTarget, ParameterValue>,
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
