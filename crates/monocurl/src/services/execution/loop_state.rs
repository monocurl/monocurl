use std::time::Instant;

use futures::{
    FutureExt, StreamExt,
    channel::mpsc::UnboundedSender,
    future::{self, LocalBoxFuture},
};
use runtime::{CommandEffect, RuntimeCommand, RuntimeController};
use smol::Timer;
use structs::{
    futures::yield_now,
    rope::{Rope, TextAggregate},
};

use crate::services::ServiceManagerMessage;

use super::{ExecutionMessage, ExecutionService, PlaybackMode};

struct RuntimeState {
    version: usize,
    controller: RuntimeController,
    root_text_rope: Rope<TextAggregate>,
    started_at: Instant,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            version: 0,
            controller: RuntimeController::new(),
            root_text_rope: Rope::default(),
            started_at: Instant::now(),
        }
    }

    fn now_seconds(&self) -> f64 {
        self.started_at.elapsed().as_secs_f64()
    }

    fn requires_future_reset(&self, message: &ExecutionMessage) -> bool {
        match message {
            ExecutionMessage::UpdateBytecode { .. } | ExecutionMessage::SetPlaybackMode(_) => true,
            ExecutionMessage::SeekTo { target } => self.controller.seek_to_requires_reset(*target),
            ExecutionMessage::UpdateParameters { .. } | ExecutionMessage::TogglePlay => false,
        }
    }

    fn apply_message(&mut self, message: ExecutionMessage) -> CommandEffect {
        let now_seconds = self.now_seconds();
        match message {
            ExecutionMessage::UpdateBytecode {
                bytecode,
                root_text_rope,
                version,
            } => {
                self.version = version;
                self.root_text_rope = root_text_rope;
                self.controller
                    .apply_command(RuntimeCommand::UpdateBytecode { bytecode }, now_seconds)
            }
            ExecutionMessage::SetPlaybackMode(playback_mode) => {
                log::info!(
                    "playback mode -> {}",
                    match playback_mode {
                        PlaybackMode::Presentation => "presentation",
                        PlaybackMode::Preview => "preview",
                    }
                );
                self.controller
                    .apply_command(RuntimeCommand::SetPlaybackMode(playback_mode), now_seconds)
            }
            ExecutionMessage::SeekTo { target } => {
                log::info!("seek_to {:?}", target);
                self.controller
                    .apply_command(RuntimeCommand::SeekTo { target }, now_seconds)
            }
            ExecutionMessage::TogglePlay => {
                let effect = self
                    .controller
                    .apply_command(RuntimeCommand::TogglePlay, now_seconds);
                log::info!(
                    "playback toggled -> {}",
                    if self.controller.is_playing() {
                        "playing"
                    } else {
                        "paused"
                    }
                );
                effect
            }
            ExecutionMessage::UpdateParameters { updates } => self
                .controller
                .apply_command(RuntimeCommand::UpdateParameters { updates }, now_seconds),
        }
    }

    fn play_session(
        &self,
        sm_tx: UnboundedSender<ServiceManagerMessage>,
    ) -> LocalBoxFuture<'static, ()> {
        let controller = self.controller.clone();
        let root_text_rope = self.root_text_rope.clone();
        let version = self.version;
        let started_at = self.started_at;

        async move {
            loop {
                let iteration_started_at = Instant::now();
                let now_seconds = started_at.elapsed().as_secs_f64();
                let iteration = controller.run_iteration(now_seconds).await;
                let next_frame_interval = iteration.next_frame_interval;

                for snapshot in iteration.snapshots {
                    ExecutionService::emit_snapshot(
                        &sm_tx,
                        &controller,
                        &root_text_rope,
                        version,
                        snapshot,
                    );
                }

                yield_now().await;

                if let Some(frame_interval) = next_frame_interval {
                    let frame_elapsed = iteration_started_at.elapsed();
                    if controller.is_playing() && frame_interval > frame_elapsed {
                        Timer::after(frame_interval - frame_elapsed).await;
                    }
                }
            }
        }
        .boxed_local()
    }
}

impl ExecutionService {
    pub fn new(
        rx: futures::channel::mpsc::UnboundedReceiver<ExecutionMessage>,
        sm_tx: futures::channel::mpsc::UnboundedSender<crate::services::ServiceManagerMessage>,
    ) -> Self {
        Self { rx, sm_tx }
    }

    pub fn run(self) {
        std::thread::spawn(move || {
            smol::block_on(self.run_loop());
        });
    }

    async fn run_loop(mut self) {
        let mut runtime = RuntimeState::new();
        let mut play_future = Some(runtime.play_session(self.sm_tx.clone()));

        loop {
            let Some(messages) = self
                .next_message_batch(&runtime, play_future.as_mut().expect("play future"))
                .await
            else {
                break;
            };
            let messages = compact_message_batch(messages);

            let mut reset_future = false;
            for message in messages {
                let message_resets_future = runtime.requires_future_reset(&message);
                if message_resets_future && !reset_future {
                    drop(play_future.take());
                    reset_future = true;
                }

                let effect = runtime.apply_message(message);
                debug_assert!(matches!(
                    (message_resets_future, effect),
                    (true, CommandEffect::ResetFuture) | (false, CommandEffect::KeepFuture)
                ));
            }

            if reset_future {
                play_future = Some(runtime.play_session(self.sm_tx.clone()));
            }
        }
    }

    async fn next_message_batch(
        &mut self,
        runtime: &RuntimeState,
        play_future: &mut LocalBoxFuture<'static, ()>,
    ) -> Option<Vec<ExecutionMessage>> {
        future::poll_fn(|cx| {
            let mut messages = Vec::new();
            loop {
                match self.rx.poll_next_unpin(cx) {
                    std::task::Poll::Ready(Some(message)) => messages.push(message),
                    std::task::Poll::Ready(None) => {
                        return std::task::Poll::Ready((!messages.is_empty()).then_some(messages));
                    }
                    std::task::Poll::Pending => break,
                }
            }

            if !messages.is_empty() {
                return std::task::Poll::Ready(Some(messages));
            }

            if runtime.controller.needs_work() {
                let _ = play_future.as_mut().poll(cx);
            }

            std::task::Poll::Pending
        })
        .await
    }
}

// coalesce consecutive timeline scrub seeks
fn compact_message_batch(messages: Vec<ExecutionMessage>) -> Vec<ExecutionMessage> {
    let mut compacted = Vec::with_capacity(messages.len());
    for message in messages {
        match message {
            ExecutionMessage::SeekTo { target } => {
                if let Some(ExecutionMessage::SeekTo {
                    target: existing_target,
                }) = compacted.last_mut()
                {
                    *existing_target = target;
                } else {
                    compacted.push(ExecutionMessage::SeekTo { target });
                }
            }
            message => compacted.push(message),
        }
    }
    compacted
}

#[cfg(test)]
mod tests {
    use executor::time::Timestamp;

    use super::{ExecutionMessage, compact_message_batch};

    #[test]
    fn compact_message_batch_keeps_latest_consecutive_seek() {
        let compacted = compact_message_batch(vec![
            ExecutionMessage::SeekTo {
                target: Timestamp::new(1, 0.25),
            },
            ExecutionMessage::SeekTo {
                target: Timestamp::new(1, 0.5),
            },
            ExecutionMessage::SeekTo {
                target: Timestamp::new(1, 0.75),
            },
        ]);

        assert_eq!(compacted.len(), 1);
        match &compacted[0] {
            ExecutionMessage::SeekTo { target } => {
                assert_eq!(*target, Timestamp::new(1, 0.75));
            }
            _ => panic!("expected compacted seek"),
        }
    }

    #[test]
    fn compact_message_batch_preserves_seek_order_around_other_messages() {
        let compacted = compact_message_batch(vec![
            ExecutionMessage::SeekTo {
                target: Timestamp::new(1, 0.25),
            },
            ExecutionMessage::TogglePlay,
            ExecutionMessage::SeekTo {
                target: Timestamp::new(1, 0.5),
            },
        ]);

        assert_eq!(compacted.len(), 3);
        match (&compacted[0], &compacted[1], &compacted[2]) {
            (
                ExecutionMessage::SeekTo { target: first },
                ExecutionMessage::TogglePlay,
                ExecutionMessage::SeekTo { target: second },
            ) => {
                assert_eq!(*first, Timestamp::new(1, 0.25));
                assert_eq!(*second, Timestamp::new(1, 0.5));
            }
            _ => panic!("expected seek, toggle, seek"),
        }
    }
}
