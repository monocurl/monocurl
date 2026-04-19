use structs::text::Span8;

use crate::{
    error::{ExecutorError, RuntimeCallFrame, RuntimeError},
    state::ExecutionState,
    value::InstructionPointer,
};

use super::Executor;

const RUNTIME_ERROR_CALLSTACK_LIMIT: usize = 5;

impl Executor {
    pub fn record_runtime_error(&mut self, error: ExecutorError) {
        let runtime_error = self.build_runtime_error_at_stack(error, self.state.last_stack_idx);
        self.state.error(runtime_error);
    }

    pub fn record_runtime_error_at_root(&mut self, error: ExecutorError) {
        let runtime_error = self.build_runtime_error_at_stack(error, ExecutionState::ROOT_STACK_ID);
        self.state.error(runtime_error);
    }

    pub(crate) fn build_runtime_error(&self, error: crate::error::ExecutorError) -> RuntimeError {
        self.build_runtime_error_at_stack(error, self.state.last_stack_idx)
    }

    fn build_runtime_error_at_stack(
        &self,
        error: crate::error::ExecutorError,
        stack_idx: usize,
    ) -> RuntimeError {
        let mut fallback_span = self.current_instruction_span(stack_idx);
        let mut root_span = None;
        let mut callstack = Vec::new();

        for frame in self.recover_call_stack(stack_idx) {
            let span = self.best_effort_span(frame.stack_idx, frame.next_ip);
            fallback_span = span.clone();
            if root_span.is_none()
                && self.bytecode.sections[frame.next_ip.0 as usize]
                    .flags
                    .is_root_module
            {
                root_span = Some(span.clone());
            }
            if callstack.len() < RUNTIME_ERROR_CALLSTACK_LIMIT {
                callstack.push(RuntimeCallFrame {
                    section: frame.next_ip.0,
                    span: span.clone(),
                });
            }
        }

        RuntimeError {
            error,
            span: root_span.unwrap_or(fallback_span),
            callstack,
        }
    }

    pub(super) fn current_instruction_span(&self, stack_idx: usize) -> Span8 {
        let ip = self.state.stack_ip(stack_idx);
        self.best_effort_span(stack_idx, ip)
    }

    fn best_effort_span(&self, _stack_idx: usize, ip: InstructionPointer) -> Span8 {
        self.span_for_next_ip(ip)
            .or_else(|| self.annotation_span(ip))
            .or_else(|| self.first_root_annotation_span())
            .unwrap_or_else(|| nondegenerate_span(0..1))
    }

    fn annotation_span(&self, ip: InstructionPointer) -> Option<Span8> {
        let section = self.bytecode.sections.get(ip.0 as usize)?;
        let annotations = &section.annotations;
        if annotations.is_empty() {
            return None;
        }

        let idx = (ip.1 as usize).min(annotations.len().saturating_sub(1));
        Some(nondegenerate_span(annotations[idx].source_loc.clone()))
    }

    fn span_for_next_ip(&self, next_ip: InstructionPointer) -> Option<Span8> {
        let instr_idx = next_ip.1.checked_sub(1)? as usize;
        let raw = self.bytecode.sections[next_ip.0 as usize]
            .annotations
            .get(instr_idx)?
            .source_loc
            .clone();
        Some(nondegenerate_span(raw))
    }

    fn first_root_annotation_span(&self) -> Option<Span8> {
        self.bytecode
            .sections
            .iter()
            .find(|section| section.flags.is_root_module)
            .and_then(|section| section.annotations.first())
            .map(|annotation| nondegenerate_span(annotation.source_loc.clone()))
    }

    fn recover_call_stack(&self, stack_idx: usize) -> std::vec::IntoIter<RecoveredFrame> {
        let mut frames = Vec::new();
        let mut cursor = Some(stack_idx);

        while let Some(idx) = cursor {
            frames.push(RecoveredFrame {
                stack_idx: idx,
                next_ip: self.state.stack_ip(idx),
            });
            frames.extend(
                self.state
                    .stack_call_stack(idx)
                    .iter()
                    .rev()
                    .copied()
                    .map(|next_ip| RecoveredFrame {
                        stack_idx: idx,
                        next_ip,
                    }),
            );
            cursor = self.state.stack_trace_parent_idx(idx);
        }

        frames.into_iter()
    }
}

#[derive(Clone, Copy)]
struct RecoveredFrame {
    #[allow(dead_code)]
    stack_idx: usize,
    next_ip: InstructionPointer,
}

fn nondegenerate_span(raw: Span8) -> Span8 {
    if raw.is_empty() {
        raw.start..raw.end + 1
    } else {
        raw
    }
}
