use futures::{FutureExt, StreamExt};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use crate::{services::ServiceManagerMessage};

pub enum ExecutionMessage {
    UpdateBytecode,
    UpdateParameter,
    TogglePlay,
    SeekTo,
}

pub struct ExecutionService {
    rx: Option<UnboundedReceiver<ExecutionMessage>>,

    sm_tx: UnboundedSender<ServiceManagerMessage>,
}

impl ExecutionService {
    pub fn new(rx: UnboundedReceiver<ExecutionMessage>, sm_tx: UnboundedSender<ServiceManagerMessage>) -> Self {
        Self {
            rx: Some(rx),
            sm_tx
        }
    }

    async fn handle_message(&self, message: ExecutionMessage) {
        match message {
            ExecutionMessage::UpdateBytecode => {
                // handle bytecode update. Must avoid await points so this is done atomically
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

    pub async fn run(mut self) {
        let mut last_item = None;
        let mut rx = self.rx.take().unwrap();
        loop {
            if last_item.is_none() {
                match rx.next().await {
                    Some(item) => last_item = Some(item),
                    None => break,
                }
            }

            // handle the last message, but stop if a new message arrives
            futures::select! {
                message = rx.next().fuse() => {
                    if let Some(msg) = message {
                        last_item = Some(msg);
                    } else {
                        break;
                    }
                },
                _ = self.handle_message(last_item.take().unwrap()).fuse() => {},
            }
        }
    }
}
