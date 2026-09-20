mod dispatch;
mod memory;
mod params;
mod runtime_error;

mod access;
mod anim;
mod cacheing;
mod invoke;
mod lerp;
pub(crate) mod ops;

use std::pin::Pin;
use std::{future::Future, sync::Arc};

use bytecode::{Bytecode, Instruction};
use structs::futures::PeriodicYielder;

use crate::executor::cacheing::ExecutionCache;
use crate::time::Timestamp;
use crate::{error::ExecutorError, state::ExecutionState, value::Value};

pub use self::cacheing::LiveCheckpoint;
pub(crate) use self::invoke::{fill_defaults, prepare_eager_call_args};
use self::memory::{EXECUTOR_HEAP_SLOT_LIMIT, MEMORY_CHECK_PERIOD, PeriodicMemoryChecker};

pub type StdlibReturn<'a> = Pin<Box<dyn Future<Output = Result<Value, ExecutorError>> + 'a>>;

pub type StdlibFunc = for<'a> fn(&'a mut Executor, usize) -> StdlibReturn<'a>;

enum SeekPrimitiveResult {
    Error(ExecutorError),
    EndOfSection,
    PrimitiveAnim,
}

enum SeekPrimitiveAnimSkipResult {
    Error(ExecutorError),
    PrimitiveAnim {
        advanced_section: bool,
        completed_slide_duration: Option<f64>,
    },
    NoAnimsLeft {
        completed_slide_duration: Option<f64>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaybackCacheMode {
    Full,
    Disabled,
}

impl PlaybackCacheMode {
    pub(crate) fn records_durations(self) -> bool {
        matches!(self, Self::Full)
    }

    pub(crate) fn records_entries(self) -> bool {
        matches!(self, Self::Full)
    }
}

pub(crate) struct PlaybackAdvanceResult {
    pub(crate) advance: PlaybackAdvance,
    pub(crate) completed_slide_duration: Option<f64>,
}

#[derive(Debug)]
pub enum SeekToResult {
    Error(ExecutorError),
    SeekedTo(Timestamp),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SeekOptions {
    pub fast_seek: bool,
}

impl SeekOptions {
    pub const fn fast() -> Self {
        Self { fast_seek: true }
    }

    pub const fn strict() -> Self {
        Self { fast_seek: false }
    }

    pub(crate) fn validate_lerp_completion(self) -> bool {
        !self.fast_seek
    }

    pub(crate) fn verify_scene_snapshot_after_step(self) -> bool {
        !self.fast_seek
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackAdvance {
    Advanced,
    PreparedSection,
    Finished,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextRenderQuality {
    #[default]
    Normal,
    High,
}

/// outcome of a run of instructions attempted without suspending
pub(crate) enum SyncRun {
    /// the next instruction needs the asynchronous path
    Suspend,
    /// execution produced a result other than `Continue`
    Done(ExecSingle),
    /// the run hit its instruction budget; the caller should yield and resume
    BudgetExhausted,
}

/// how many instructions one synchronous run may execute before handing control
/// back so the cooperative yielder can run
pub(crate) const SYNC_RUN_BUDGET: u32 = 256;

/// result of executing a single instruction
pub(crate) enum ExecSingle {
    Continue,
    Play,
    EndOfHead,
    Error(ExecutorError),
}

pub struct Executor {
    pub state: ExecutionState,
    pub(crate) bytecode: Bytecode,
    pub(crate) native_funcs: Vec<StdlibFunc>,
    pub(crate) cache: ExecutionCache,
    pub(crate) yielder: PeriodicYielder,
    aspect_ratio: f32,
    text_render_quality: TextRenderQuality,
    memory_checker: PeriodicMemoryChecker,
}

fn normalize_aspect_ratio(aspect_ratio: f32) -> f32 {
    if aspect_ratio.is_finite() {
        aspect_ratio.max(0.1)
    } else {
        16.0 / 9.0
    }
}

impl Executor {
    pub fn new(bytecode: Bytecode, native_funcs: Vec<StdlibFunc>) -> Self {
        let cache = ExecutionCache::new(&bytecode);
        Self {
            state: ExecutionState::new(),
            bytecode,
            native_funcs,
            cache,
            yielder: PeriodicYielder::default(),
            aspect_ratio: 16.0 / 9.0,
            text_render_quality: TextRenderQuality::Normal,
            memory_checker: PeriodicMemoryChecker::new(
                EXECUTOR_HEAP_SLOT_LIMIT,
                MEMORY_CHECK_PERIOD,
            ),
        }
    }

    pub async fn tick_yielder(&mut self) {
        self.yielder.tick().await;
    }

    pub fn total_sections(&self) -> usize {
        self.bytecode.sections.len()
    }

    pub fn section_bytecode(&self, section_idx: usize) -> &bytecode::SectionBytecode {
        &self.bytecode.sections[section_idx]
    }

    pub fn sections(&self) -> &[Arc<bytecode::SectionBytecode>] {
        &self.bytecode.sections
    }

    pub fn user_to_internal_timestamp(&self, user_ts: Timestamp) -> Timestamp {
        Timestamp {
            slide: user_ts.slide + self.bytecode.library_sections(),
            time: user_ts.time,
        }
    }

    pub fn internal_to_user_timestamp(&self, internal_ts: Timestamp) -> Timestamp {
        Timestamp {
            slide: internal_ts
                .slide
                .saturating_sub(self.bytecode.library_sections()),
            time: internal_ts.time,
        }
    }

    pub fn set_text_render_quality(&mut self, quality: TextRenderQuality) {
        self.text_render_quality = quality;
    }

    pub fn text_render_quality(&self) -> TextRenderQuality {
        self.text_render_quality
    }

    pub fn update_aspect_ratio(&mut self, aspect_ratio: f32) {
        self.aspect_ratio = normalize_aspect_ratio(aspect_ratio);
    }

    pub fn aspect_ratio(&self) -> f32 {
        self.aspect_ratio
    }

    /// fetch and advance past the next instruction
    #[inline(always)]
    fn fetch(&mut self, stack_idx: usize) -> Result<(usize, Instruction), ExecutorError> {
        self.state.last_stack_idx = stack_idx;
        self.memory_checker.tick()?;

        let (section, offset) = self.state.stack(stack_idx).ip;
        let section_idx = section as usize;
        let instr = self.bytecode.sections[section_idx].instructions[offset as usize];
        self.state.stack_mut(stack_idx).ip = (section, offset + 1);

        Ok((section_idx, instr))
    }

    /// run instructions until one needs to suspend, the budget runs out, or
    /// execution produces a result. the great majority of instructions never
    /// await, so this loop is what the interpreter spends its time in
    #[inline(always)]
    pub(crate) fn execute_sync_run(&mut self, stack_idx: usize, mut budget: u32) -> SyncRun {
        while budget > 0 {
            let restore_ip = self.state.stack(stack_idx).ip;
            let (section_idx, instr) = match self.fetch(stack_idx) {
                Ok(fetched) => fetched,
                Err(error) => return SyncRun::Done(ExecSingle::Error(error)),
            };

            match self.execute_instr_sync(section_idx, stack_idx, instr) {
                Some(ExecSingle::Continue) => budget -= 1,
                Some(other) => return SyncRun::Done(other),
                None => {
                    // nothing was mutated; rewind so the async path re-fetches
                    self.state.stack_mut(stack_idx).ip = restore_ip;
                    return SyncRun::Suspend;
                }
            }
        }

        SyncRun::BudgetExhausted
    }

    /// drive one execution head until it produces something other than
    /// `Continue`, staying in the synchronous run for as long as the instruction
    /// stream allows and yielding cooperatively between runs
    pub(crate) async fn run_until_break(&mut self, stack_idx: usize) -> ExecSingle {
        loop {
            match self.execute_sync_run(stack_idx, SYNC_RUN_BUDGET) {
                SyncRun::Done(result) => return result,
                SyncRun::BudgetExhausted => self.tick_yielder().await,
                SyncRun::Suspend => {
                    self.tick_yielder().await;
                    match self.execute_one(stack_idx).await {
                        ExecSingle::Continue => {}
                        other => return other,
                    }
                }
            }
        }
    }

    #[inline(always)]
    pub(crate) async fn execute_one(&mut self, stack_idx: usize) -> ExecSingle {
        let (section_idx, instr) = match self.fetch(stack_idx) {
            Ok(fetched) => fetched,
            Err(error) => return ExecSingle::Error(error),
        };

        match self.execute_instr_sync(section_idx, stack_idx, instr) {
            Some(result) => result,
            None => {
                self.execute_instr_async(section_idx, stack_idx, instr)
                    .await
            }
        }
    }
}
