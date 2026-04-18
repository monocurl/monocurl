use structs::text::Span8;

use crate::{
    error::{RuntimeCallFrame, RuntimeError},
    value::InstructionPointer,
};

use super::Executor;

const RUNTIME_ERROR_CALLSTACK_LIMIT: usize = 5;

impl Executor {
    pub(crate) fn build_runtime_error(&self, error: crate::error::ExecutorError) -> RuntimeError {
        let stack_idx = self.state.last_stack_idx;
        let mut fallback_span = self.current_instruction_span(stack_idx);
        let mut root_span = None;
        let mut callstack = Vec::new();

        for frame in self.recover_call_stack(stack_idx) {
            let span = self
                .span_for_next_ip(frame.next_ip)
                .unwrap_or_else(|| self.current_instruction_span(frame.stack_idx));
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
        self.span_for_next_ip(ip)
            .unwrap_or_else(|| self.annotation_span(ip))
    }

    fn annotation_span(&self, ip: InstructionPointer) -> Span8 {
        let raw = self.bytecode.sections[ip.0 as usize].annotations[ip.1 as usize]
            .source_loc
            .clone();
        nondegenerate_span(raw)
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
