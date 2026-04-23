mod dispatch;
mod memory;
mod runtime_error;

mod access;
mod anim;
mod cacheing;
mod invoke;
mod lerp;
pub(crate) mod ops;

use std::pin::Pin;
use std::{future::Future, sync::Arc};

use bytecode::Bytecode;
use structs::futures::PeriodicYielder;

use crate::executor::cacheing::ExecutionCache;
use crate::heap::{heap_replace, with_heap_mut};
use crate::time::Timestamp;
use crate::{error::ExecutorError, state::ExecutionState, value::Value};

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
    PrimitiveAnim,
    NoAnimsLeft,
}

pub enum SeekToResult {
    Error(ExecutorError),
    SeekedTo(Timestamp),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TextRenderQuality {
    #[default]
    Normal,
    High,
}

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
    text_render_quality: TextRenderQuality,
    memory_checker: PeriodicMemoryChecker,
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
            text_render_quality: TextRenderQuality::Normal,
            memory_checker: PeriodicMemoryChecker::new(
                EXECUTOR_HEAP_SLOT_LIMIT,
                MEMORY_CHECK_PERIOD,
            ),
        }
    }

    #[inline]
    pub(crate) async fn tick_yielder(&mut self) {
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
            slide: user_ts.slide + self.bytecode.non_slide_sections(),
            time: user_ts.time,
        }
    }

    pub fn internal_to_user_timestamp(&self, internal_ts: Timestamp) -> Timestamp {
        Timestamp {
            slide: internal_ts
                .slide
                .saturating_sub(self.bytecode.non_slide_sections()),
            time: internal_ts.time,
        }
    }

    pub fn update_parameter(&mut self, name: &str, value: Value) -> Result<(), ExecutorError> {
        let param = self
            .state
            .active_params
            .iter()
            .find(|param| param.name == name)
            .ok_or_else(|| ExecutorError::unknown_parameter(name))?;

        let leader_cell_key = param.leader_cell.key();
        let leader_value_key = param.leader_value;
        let follower_value_key = param.follower_value;

        heap_replace(leader_value_key, value.clone());
        heap_replace(follower_value_key, value);
        with_heap_mut(|h| {
            if let Value::Leader(l) = &mut *h.get_mut(leader_cell_key) {
                l.leader_version += 1;
                l.follower_version += 1;
            }
        });
        Ok(())
    }

    pub fn set_text_render_quality(&mut self, quality: TextRenderQuality) {
        self.text_render_quality = quality;
    }

    pub fn text_render_quality(&self) -> TextRenderQuality {
        self.text_render_quality
    }

    #[inline(always)]
    pub(crate) async fn execute_one(&mut self, stack_idx: usize) -> ExecSingle {
        self.state.last_stack_idx = stack_idx;

        if let Err(err) = self.memory_checker.tick() {
            return ExecSingle::Error(err);
        }

        let ip = self.state.stack(stack_idx).ip;
        let section_idx = ip.0 as usize;
        let instr_idx = ip.1 as usize;

        let instr = self.bytecode.sections[section_idx].instructions[instr_idx];

        self.state.stack_mut(stack_idx).ip = (section_idx as u16, (instr_idx + 1) as u32);

        self.execute_instr(section_idx, stack_idx, instr).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytecode::{Bytecode, SectionBytecode, SectionFlags};

    use super::Executor;
    use crate::{error::ExecutorError, heap::with_heap, state::LeaderKind, value::Value};

    fn executor_with_sections(flags: &[SectionFlags]) -> Executor {
        Executor::new(
            Bytecode::new(
                flags
                    .iter()
                    .cloned()
                    .map(SectionBytecode::new)
                    .map(Arc::new)
                    .collect(),
            ),
            Vec::new(),
        )
    }

    fn empty_executor() -> Executor {
        executor_with_sections(&[SectionFlags {
            is_stdlib: false,
            is_library: false,
            is_init: false,
            is_root_module: true,
        }])
    }

    #[test]
    fn update_parameter_syncs_leader_and_follower() {
        let mut executor = empty_executor();
        executor
            .state
            .stack_mut(crate::state::ExecutionState::ROOT_STACK_ID)
            .push(Value::Integer(5));
        executor.state.promote_to_leader(
            crate::state::ExecutionState::ROOT_STACK_ID,
            LeaderKind::Param,
            "speed".into(),
        );

        executor
            .update_parameter("speed", Value::Float(2.5))
            .unwrap();

        let param = &executor.state.active_params[0];
        let leader_val = with_heap(|h| h.get(param.leader_value).clone());
        match leader_val {
            Value::Float(value) => assert_eq!(value, 2.5),
            other => panic!("expected float leader value, got {}", other.type_name()),
        }
        let follower_val = with_heap(|h| h.get(param.follower_value).clone());
        match follower_val {
            Value::Float(value) => assert_eq!(value, 2.5),
            other => panic!("expected float follower value, got {}", other.type_name()),
        }
    }

    #[test]
    fn update_parameter_errors_for_missing_name() {
        let mut executor = empty_executor();
        let error = executor.update_parameter("missing", Value::Integer(1));
        assert!(matches!(error, Err(ExecutorError::UnknownParameter(_))));
    }
}
