use std::{
    pin::pin,
    time::{Duration, Instant},
};

use executor::{
    executor::{Executor, SeekPrimitiveAnimSkipResult, SeekToResult},
    scene_snapshot::SceneSnapshot,
    time::Timestamp,
};
use futures::channel::mpsc::UnboundedSender;
use futures::{StreamExt, future};
use smol::Timer;
use stdlib::registry::registry;
use structs::rope::{Rope, TextAggregate};

use crate::services::ServiceManagerMessage;

use super::{ExecutionMessage, ExecutionService, PlaybackMode, default_bytecode};

struct RuntimeState {
    version: usize,
    has_compiler_error: bool,
    executor: Executor,
    target: Timestamp,
    is_playing: bool,
    has_seeked_for_play: bool,
    playback_mode: PlaybackMode,
    root_text_rope: Rope<TextAggregate>,
    last_update_at: Instant,
}

impl RuntimeState {
    fn new() -> Self {
        Self {
            version: 0,
            has_compiler_error: true,
            executor: Executor::new(default_bytecode(), registry().func_table()),
            target: Timestamp::default(),
            is_playing: false,
            has_seeked_for_play: false,
            playback_mode: PlaybackMode::Preview,
            root_text_rope: Rope::default(),
            last_update_at: Instant::now(),
        }
    }

    fn apply_message(&mut self, message: ExecutionMessage) {
        match message {
            ExecutionMessage::UpdateBytecode {
                bytecode,
                root_text_rope,
                version,
            } => {
                self.version = version;
                self.is_playing = false;
                self.root_text_rope = root_text_rope;
                if let Some(bytecode) = bytecode {
                    let old_user_timestamp = self.executor.internal_to_user_timestamp(self.target);
                    self.executor.update_bytecode(bytecode);
                    self.target = self.executor.user_to_internal_timestamp(old_user_timestamp);
                    self.has_compiler_error = false;
                } else {
                    self.has_compiler_error = true;
                }
            }
            ExecutionMessage::SetPlaybackMode(playback_mode) => {
                log::info!(
                    "playback mode -> {}",
                    match playback_mode {
                        PlaybackMode::Presentation => "presentation",
                        PlaybackMode::Preview => "preview",
                    }
                );
                self.is_playing = false;
                self.playback_mode = playback_mode;
                self.executor.clear_cache();
                self.target = self
                    .executor
                    .user_to_internal_timestamp(Timestamp::default());
                self.has_seeked_for_play = false;
            }
            ExecutionMessage::SeekTo { target } => {
                log::info!("seek_to {:?}", target);
                self.is_playing = false;
                self.target = self.executor.user_to_internal_timestamp(target);
            }
            ExecutionMessage::TogglePlay => {
                self.is_playing = !self.is_playing;
                log::info!(
                    "playback toggled -> {}",
                    if self.is_playing { "playing" } else { "paused" }
                );
                if self.is_playing {
                    self.last_update_at = Instant::now();
                    self.has_seeked_for_play = false;
                }
            }
            ExecutionMessage::UpdateParameters { updates } => {
                for (name, value) in updates {
                    let Some(value) = ExecutionService::runtime_value_from_parameter(&value) else {
                        log::warn!("parameter update failed for {}: unsupported value", name);
                        continue;
                    };
                    if let Err(error) = self.executor.update_parameter(&name, value) {
                        log::warn!("parameter update failed for {}: {}", name, error);
                    }
                }
            }
        }
    }

    async fn sync_to_target(&mut self, sm_tx: &UnboundedSender<ServiceManagerMessage>) {
        ExecutionService::emit_snapshot(
            sm_tx,
            &self.executor,
            &self.root_text_rope,
            self.has_compiler_error,
            self.is_playing,
            true,
            self.playback_mode,
            self.version,
            None,
        )
        .await;

        self.has_seeked_for_play = true;
        if self.executor.state.has_errors() {
            self.cancel_runtime_work();
            self.emit_snapshot(sm_tx, false, None).await;
            return;
        }

        match self.executor.seek_to(self.target).await {
            SeekToResult::SeekedTo(reached) => {
                self.target = reached;
            }
            SeekToResult::Error(_) => {
                self.cancel_runtime_work();
            }
        }

        let scene_snapshot = self.capture_scene_snapshot().await.ok().flatten();
        self.emit_snapshot(sm_tx, false, scene_snapshot).await;
    }

    async fn run_playback_until_paused(&mut self, sm_tx: &UnboundedSender<ServiceManagerMessage>) {
        while self.is_playing {
            self.playback_iteration(sm_tx).await;
        }
    }

    async fn playback_iteration(&mut self, sm_tx: &UnboundedSender<ServiceManagerMessage>) {
        let tick_started_at = Instant::now();
        let elapsed = (tick_started_at - self.last_update_at).as_secs_f64();
        let target_dt = self.playback_mode.default_time_interval().max(elapsed);
        let max_slide = self.max_slide();

        let mut skip_scene_snapshot = false;
        match self.executor.seek_primitive_anim_skip(max_slide).await {
            SeekPrimitiveAnimSkipResult::PrimitiveAnim => {}
            SeekPrimitiveAnimSkipResult::NoAnimsLeft => {
                skip_scene_snapshot = self.advance_section_without_materializing().await;
                self.is_playing = false;
            }
            SeekPrimitiveAnimSkipResult::Error(_) => {
                self.is_playing = false;
            }
        }

        if self.is_playing {
            match self.executor.advance_playback(max_slide, target_dt).await {
                Ok(true) => {}
                Ok(false) => {
                    self.advance_section_with_materialization(max_slide).await;
                    self.is_playing = false;
                }
                Err(_) => {
                    self.is_playing = false;
                }
            }
        }

        self.target = self.executor.state.timestamp;

        let scene_snapshot = if skip_scene_snapshot {
            None
        } else {
            self.capture_scene_snapshot().await.ok().flatten()
        };
        self.emit_snapshot(sm_tx, false, scene_snapshot).await;

        let full_elapsed = Instant::now()
            .duration_since(self.last_update_at)
            .as_secs_f64();
        self.last_update_at = Instant::now();
        if self.is_playing && target_dt > full_elapsed {
            Timer::after(Duration::from_secs_f64(target_dt - full_elapsed)).await;
        }
    }

    fn max_slide(&self) -> usize {
        match self.playback_mode {
            PlaybackMode::Presentation => self.executor.state.timestamp.slide,
            PlaybackMode::Preview => self.executor.total_sections(),
        }
    }

    fn cancel_runtime_work(&mut self) {
        self.is_playing = false;
        self.target = self.executor.state.timestamp;
    }

    async fn advance_section_without_materializing(&mut self) -> bool {
        if self.executor.state.has_errors() {
            self.cancel_runtime_work();
            return false;
        }

        if self.executor.state.timestamp.slide + 1 >= self.executor.total_sections() {
            return false;
        }

        self.executor.advance_section().await;
        self.target = self.executor.state.timestamp;
        true
    }

    async fn advance_section_with_materialization(&mut self, max_slide: usize) {
        if self.executor.state.has_errors() {
            self.cancel_runtime_work();
            return;
        }

        if self.executor.state.timestamp.slide + 1 >= self.executor.total_sections() {
            return;
        }

        self.executor.advance_section().await;
        let _ = self.executor.seek_primitive_anim_skip(max_slide).await;
        self.target = self.executor.state.timestamp;
    }

    async fn capture_scene_snapshot(&mut self) -> Result<Option<SceneSnapshot>, ()> {
        if self.has_compiler_error || self.executor.state.has_errors() {
            return Ok(None);
        }

        match ExecutionService::capture_stable_scene_snapshot(&mut self.executor).await {
            Ok(scene_snapshot) => Ok(Some(scene_snapshot)),
            Err(_) => {
                self.cancel_runtime_work();
                Err(())
            }
        }
    }

    async fn emit_snapshot(
        &mut self,
        sm_tx: &UnboundedSender<ServiceManagerMessage>,
        is_loading: bool,
        scene_snapshot: Option<SceneSnapshot>,
    ) {
        ExecutionService::emit_snapshot(
            sm_tx,
            &self.executor,
            &self.root_text_rope,
            self.has_compiler_error,
            self.is_playing,
            is_loading,
            self.playback_mode,
            self.version,
            scene_snapshot,
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, sync::Arc};

    use compiler::cache::CompilerCache;
    use futures::channel::mpsc::unbounded;
    use lexer::{lexer::Lexer, token::Token};
    use parser::{
        ast::{Section, SectionBundle, SectionType},
        parser::SectionParser,
    };
    use structs::{
        assets::Assets,
        rope::{Rope, TextAggregate},
    };

    use super::*;

    fn parse_section(src: &str, section_type: SectionType) -> Section {
        let tokens = Lexer::token_stream(src.chars())
            .into_iter()
            .filter(|(token, _)| token != &Token::Whitespace && token != &Token::Comment)
            .collect();
        let rope: Rope<TextAggregate> = Rope::from_str(src);
        let mut parser = SectionParser::new(tokens, rope, section_type.clone(), None, None);
        let statements = parser.parse_statement_list();
        let artifacts = parser.artifacts();
        assert!(
            artifacts.error_diagnostics.is_empty(),
            "parse errors: {:?}",
            artifacts
                .error_diagnostics
                .iter()
                .map(|error| error.message.clone())
                .collect::<Vec<_>>()
        );

        Section {
            body: statements,
            section_type,
        }
    }

    fn load_stdlib_bundle(path: impl AsRef<std::path::Path>) -> Arc<SectionBundle> {
        let src = fs::read_to_string(path).expect("failed to read stdlib file");
        Arc::new(SectionBundle {
            file_path: None,
            file_index: 0,
            imported_files: vec![],
            sections: vec![parse_section(&src, SectionType::StandardLibrary)],
            root_import_span: Some(0..0),
            was_cached: false,
        })
    }

    fn compile_runtime_state(
        sections: &[(&str, SectionType)],
        stdlib_bundles: &[Arc<SectionBundle>],
    ) -> RuntimeState {
        let imported_files: Vec<_> = (0..stdlib_bundles.len()).collect();
        let user_sections = sections
            .iter()
            .map(|(src, section_type)| parse_section(src, section_type.clone()))
            .collect();
        let user_bundle = Arc::new(SectionBundle {
            file_path: None,
            file_index: 0,
            imported_files,
            sections: user_sections,
            root_import_span: None,
            was_cached: false,
        });

        let mut bundles = stdlib_bundles.to_vec();
        bundles.push(user_bundle);

        let mut cache = CompilerCache::default();
        let compile_result = compiler::compiler::compile(&mut cache, None, &bundles);
        assert!(
            compile_result.errors.is_empty(),
            "compile errors: {:?}",
            compile_result
                .errors
                .iter()
                .map(|error| error.message.clone())
                .collect::<Vec<_>>()
        );

        let mut runtime = RuntimeState::new();
        let root_text = sections
            .iter()
            .map(|(src, _)| *src)
            .collect::<Vec<_>>()
            .join("\n");
        runtime.apply_message(ExecutionMessage::UpdateBytecode {
            bytecode: Some(compile_result.bytecode),
            root_text_rope: Rope::from_str(&root_text),
            version: 1,
        });
        runtime
    }

    #[test]
    fn snapshot_error_cancels_further_runtime_work() {
        let anim_mcl = load_stdlib_bundle(Assets::std_lib().join("std/anim.mcl"));
        let mut runtime = compile_runtime_state(
            &[("background = 0\nplay Set()", SectionType::Slide)],
            &[anim_mcl],
        );
        let (sm_tx, _sm_rx) = unbounded();

        runtime.is_playing = true;
        runtime.target = runtime
            .executor
            .user_to_internal_timestamp(executor::time::Timestamp::new(0, f64::INFINITY));

        smol::block_on(async {
            runtime.sync_to_target(&sm_tx).await;
        });

        assert!(runtime.executor.state.has_errors());
        assert!(!runtime.is_playing);
        assert_eq!(runtime.target, runtime.executor.state.timestamp);

        let timestamp_after_error = runtime.executor.state.timestamp;
        runtime.target = runtime
            .executor
            .user_to_internal_timestamp(executor::time::Timestamp::new(0, 0.0));
        runtime.is_playing = true;

        smol::block_on(async {
            runtime.sync_to_target(&sm_tx).await;
        });

        assert_eq!(runtime.executor.state.timestamp, timestamp_after_error);
        assert_eq!(runtime.target, timestamp_after_error);
        assert!(!runtime.is_playing);
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
        let sm_tx = self.sm_tx.clone();

        let Some(mut message) = self.rx.next().await else {
            return;
        };

        loop {
            runtime.apply_message(message);

            let state_update = async {
                if !runtime.is_playing || !runtime.has_seeked_for_play {
                    runtime.sync_to_target(&sm_tx).await;
                }
                runtime.run_playback_until_paused(&sm_tx).await;
            };

            match future::select(self.rx.next(), pin!(state_update)).await {
                future::Either::Left((Some(next_message), _)) => {
                    message = next_message;
                }
                future::Either::Left((None, _)) => break,
                future::Either::Right((_, _)) => {
                    message = match self.rx.next().await {
                        Some(next_message) => next_message,
                        None => break,
                    };
                }
            }
        }
    }
}
