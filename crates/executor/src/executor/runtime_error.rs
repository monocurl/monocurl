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
        let mut recovered_callstack = Vec::new();

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
            recovered_callstack.push(RuntimeCallFrame {
                section: frame.next_ip.0,
                span: span.clone(),
            });
        }

        recovered_callstack.reverse();
        recovered_callstack.truncate(RUNTIME_ERROR_CALLSTACK_LIMIT);

        RuntimeError {
            error,
            span: root_span.unwrap_or(fallback_span),
            callstack: recovered_callstack,
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
        let mut skip_current_ip = false;

        while let Some(idx) = cursor {
            let parent_idx = self
                .state
                .stack_trace_parent_idx(idx)
                .or_else(|| self.state.stack_parent_idx(idx));

            if let Some(parent_idx) = parent_idx {
                frames.push(RecoveredFrame {
                    stack_idx: parent_idx,
                    next_ip: self.state.stack_ip(parent_idx),
                });
            }

            if !skip_current_ip {
                frames.push(RecoveredFrame {
                    stack_idx: idx,
                    next_ip: self.state.stack_ip(idx),
                });
            }
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
            cursor = parent_idx;
            skip_current_ip = true;
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bytecode::{Bytecode, InstructionAnnotation, SectionBytecode, SectionFlags};

    use crate::executor::Executor;

    fn executor_with_root_annotations(spans: &[(usize, usize)]) -> Executor {
        let mut section = SectionBytecode::new(SectionFlags {
            is_stdlib: false,
            is_library: false,
            is_init: false,
            is_root_module: true,
        });
        section.annotations = spans
            .iter()
            .map(|(start, end)| InstructionAnnotation {
                source_loc: *start..*end,
            })
            .collect();
        section.instructions = vec![bytecode::Instruction::PushNil; spans.len()];

        Executor::new(Bytecode::new(vec![Arc::new(section)]), Vec::new())
    }

    #[test]
    fn recover_call_stack_prioritizes_spawning_play_frame() {
        let mut executor =
            executor_with_root_annotations(&[(10, 14), (20, 24), (30, 34), (40, 44)]);

        let root_idx = crate::state::ExecutionState::ROOT_STACK_ID;
        executor.state.stack_mut(root_idx).ip = (0, 1);
        executor.state.stack_mut(root_idx).call_stack.push((0, 3));

        let child_idx = executor
            .state
            .alloc_stack((0, 2), Some(root_idx), Some(root_idx))
            .expect("failed to allocate child stack");
        executor.state.stack_mut(child_idx).call_stack.push((0, 4));

        let spans: Vec<_> = executor
            .recover_call_stack(child_idx)
            .map(|frame| executor.best_effort_span(frame.stack_idx, frame.next_ip))
            .collect();

        assert_eq!(spans, vec![10..14, 20..24, 40..44, 30..34]);
    }
}
