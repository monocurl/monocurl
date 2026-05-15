mod diagnostics;
mod loop_state;
mod snapshot;

use std::collections::HashMap;

use bytecode::Bytecode;
use executor::time::Timestamp;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
pub use runtime::{
    ExecutionSnapshot, ExecutionStatus, MeshAttributeSnapshot, MeshEntrySnapshot,
    ParameterSnapshot, ParameterValue, PlaybackMode, PresentationUpdateTarget,
};
use structs::rope::{Rope, TextAggregate};

use crate::services::ServiceManagerMessage;

pub enum ExecutionMessage {
    UpdateBytecode {
        bytecode: Option<Bytecode>,
        root_text_rope: Rope<TextAggregate>,
        version: usize,
    },
    SetPlaybackMode(PlaybackMode),
    UpdateAspectRatio(f32),
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
