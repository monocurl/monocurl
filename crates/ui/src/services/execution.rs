use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use crate::{services::ServiceManagerMessage};

pub enum ExecutionMessage {
    UpdateBytecode,
    TogglePlay,
    SeekTo,
    UpdateParameter,
}

pub struct ExecutionService {
    rx: UnboundedReceiver<ExecutionMessage>,

    sm_tx: UnboundedSender<ServiceManagerMessage>,
}

impl ExecutionService {
    pub fn new(rx: UnboundedReceiver<ExecutionMessage>, sm_tx: UnboundedSender<ServiceManagerMessage>) -> Self {
        Self {
            rx,
            sm_tx
        }
    }

    pub async fn run(mut self) {
        log::info!("Starting Execution Service");
        while let Some(message) = self.rx.next().await {
            match message {
                ExecutionMessage::UpdateBytecode => {
                    // handle bytecode update
                }
                ExecutionMessage::TogglePlay => {
                    // handle play/pause toggle
                }
                ExecutionMessage::SeekTo => {
                    // handle seek
                }
                ExecutionMessage::UpdateParameter => {
                    // handle parameter update
                }
            }
        }
        log::info!("Exiting Execution Service");
    }
}
