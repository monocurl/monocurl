use structs::text::Span8;

use crate::{
    error::{ExecutorError, RuntimeCallFrame, RuntimeError},
    state::ExecutionState,
    value::InstructionPointer,
};

use super::Executor;

const RUNTIME_ERROR_CALLSTACK_LIMIT: usize = 5;

impl Executor {
    pub fn record_runtime_error(&mut self, error: ExecutorError) -> RuntimeError {
        let runtime_error = self.build_runtime_error_at_stack(error, self.state.last_stack_idx);
        self.state.error(runtime_error.clone());
        runtime_error
    }

    pub fn record_runtime_error_at_root(&mut self, error: ExecutorError) -> RuntimeError {
        let runtime_error =
            self.build_runtime_error_at_stack(error, ExecutionState::ROOT_STACK_IDX);
        self.state.error(runtime_error.clone());
        runtime_error
    }

    pub fn record_runtime_error_at_root_with_hint(
        &mut self,
        error: ExecutorError,
        hint: impl Into<String>,
    ) -> RuntimeError {
        let mut runtime_error =
            self.build_runtime_error_at_stack(error, ExecutionState::ROOT_STACK_IDX);
        runtime_error.hint = Some(hint.into());
        self.state.error(runtime_error.clone());
        runtime_error
    }

    pub(crate) fn build_runtime_error(&self, error: crate::error::ExecutorError) -> RuntimeError {
        self.build_runtime_error_at_stack(error, self.state.last_stack_idx)
    }

    fn build_runtime_error_at_stack(
        &self,
        error: crate::error::ExecutorError,
        stack_idx: usize,
    ) -> RuntimeError {
        let fallback_span = self.current_instruction_span(stack_idx);
        let mut recovered_callstack = Vec::new();

        for frame in self.recover_call_stack(stack_idx) {
            let span = self.best_effort_span(frame.stack_idx, frame.next_ip);
            recovered_callstack.push(RuntimeCallFrame {
                section: frame.next_ip.0,
                span: span.clone(),
            });
        }

        recovered_callstack.reverse();
        let primary_span = recovered_callstack
            .iter()
            .rev()
            .find(|frame| self.is_user_code_section(frame.section as usize))
            .map(|frame| frame.span.clone())
            .or_else(|| recovered_callstack.last().map(|frame| frame.span.clone()))
            .unwrap_or(fallback_span);
        recovered_callstack.truncate(RUNTIME_ERROR_CALLSTACK_LIMIT);

        RuntimeError {
            error,
            span: primary_span,
            callstack: recovered_callstack,
            hint: None,
        }
    }

    fn is_user_code_section(&self, section_idx: usize) -> bool {
        self.bytecode
            .sections
            .get(section_idx)
            .is_some_and(|section| !section.flags.is_stdlib)
    }

    pub(super) fn current_instruction_span(&self, stack_idx: usize) -> Span8 {
        let ip = self.state.stack_ip(stack_idx);
        self.best_effort_span(stack_idx, ip)
    }

    fn best_effort_span(&self, _stack_idx: usize, ip: InstructionPointer) -> Span8 {
        let section_idx = ip.0 as usize;

        self.span_for_recent_ip(ip)
            .or_else(|| self.annotation_span(ip))
            .or_else(|| self.recent_root_annotation_span(section_idx))
            .or_else(|| self.section_fallback_span(section_idx))
            .or_else(|| self.recent_root_section_fallback_span(section_idx))
            .or_else(|| self.global_fallback_span())
            .unwrap_or_else(|| nondegenerate_span(0..1))
    }

    fn annotation_span(&self, ip: InstructionPointer) -> Option<Span8> {
        let section = self.bytecode.sections.get(ip.0 as usize)?;
        let annotations = &section.annotations;
        if annotations.is_empty() {
            return None;
        }

        let idx = (ip.1 as usize).min(annotations.len().saturating_sub(1));
        meaningful_span(annotations[idx].source_loc.clone())
    }

    fn span_for_recent_ip(&self, next_ip: InstructionPointer) -> Option<Span8> {
        let section = self.bytecode.sections.get(next_ip.0 as usize)?;
        let annotations = &section.annotations;
        let last_idx = annotations.len().checked_sub(1)?;
        let start_idx = next_ip.1.saturating_sub(1).min(last_idx as u32) as usize;

        annotations[..=start_idx]
            .iter()
            .rev()
            .find_map(|annotation| meaningful_span(annotation.source_loc.clone()))
    }

    fn recent_root_annotation_span(&self, section_idx: usize) -> Option<Span8> {
        let upper_bound = section_idx.min(self.bytecode.sections.len().saturating_sub(1));
        self.bytecode.sections[..=upper_bound]
            .iter()
            .rev()
            .filter(|section| section.flags.is_root_module)
            .find_map(|section| {
                section
                    .annotations
                    .iter()
                    .rev()
                    .find_map(|annotation| meaningful_span(annotation.source_loc.clone()))
            })
    }

    fn section_fallback_span(&self, section_idx: usize) -> Option<Span8> {
        let section = self.bytecode.sections.get(section_idx)?;
        section_fallback_span_from_annotations(&section.annotations)
    }

    fn recent_root_section_fallback_span(&self, section_idx: usize) -> Option<Span8> {
        let upper_bound = section_idx.min(self.bytecode.sections.len().saturating_sub(1));
        self.bytecode
            .sections
            .get(..=upper_bound)?
            .iter()
            .rev()
            .filter(|section| section.flags.is_root_module)
            .find_map(|section| section_fallback_span_from_annotations(&section.annotations))
    }

    fn global_fallback_span(&self) -> Option<Span8> {
        self.bytecode
            .sections
            .iter()
            .rev()
            .find_map(|section| section_fallback_span_from_annotations(&section.annotations))
    }

    fn recover_call_stack(&self, stack_idx: usize) -> std::vec::IntoIter<RecoveredFrame> {
        let mut frames = Vec::new();
        let mut cursor = Some(stack_idx);

        while let Some(idx) = cursor {
            let direct_parent_idx = self.state.stack_parent_idx(idx);
            let trace_parent_idx = self.state.stack_trace_parent_idx(idx);
            let parent_idx = trace_parent_idx.or(direct_parent_idx);

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
            cursor = parent_idx;
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

fn meaningful_span(raw: Span8) -> Option<Span8> {
    (!raw.is_empty()).then_some(raw)
}

fn section_fallback_span_from_annotations(
    annotations: &[bytecode::InstructionAnnotation],
) -> Option<Span8> {
    let first = annotations
        .iter()
        .find_map(|annotation| meaningful_span(annotation.source_loc.clone()))?;
    let last = annotations
        .iter()
        .rev()
        .find_map(|annotation| meaningful_span(annotation.source_loc.clone()))
        .unwrap_or_else(|| first.clone());
    Some(nondegenerate_span(first.start..last.end))
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

    fn section_with_annotations(
        flags: SectionFlags,
        spans: &[(usize, usize)],
    ) -> Arc<SectionBytecode> {
        let mut section = SectionBytecode::new(flags);
        section.annotations = spans
            .iter()
            .map(|(start, end)| InstructionAnnotation {
                source_loc: *start..*end,
            })
            .collect();
        section.instructions = vec![bytecode::Instruction::PushNil; spans.len()];
        Arc::new(section)
    }

    #[test]
    fn recover_call_stack_keeps_spawned_child_before_parent() {
        let mut executor =
            executor_with_root_annotations(&[(10, 14), (20, 24), (30, 34), (40, 44)]);

        let root_idx = crate::state::ExecutionState::ROOT_STACK_IDX;
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

        assert_eq!(spans, vec![20..24, 40..44, 10..14, 30..34]);
    }

    #[test]
    fn runtime_error_callstack_orders_spawn_parent_before_child() {
        let mut executor =
            executor_with_root_annotations(&[(10, 14), (20, 24), (30, 34), (40, 44)]);

        let root_idx = crate::state::ExecutionState::ROOT_STACK_IDX;
        executor.state.stack_mut(root_idx).ip = (0, 1);

        let child_idx = executor
            .state
            .alloc_stack((0, 2), Some(root_idx), Some(root_idx))
            .expect("failed to allocate child stack");
        executor.state.stack_mut(child_idx).call_stack.push((0, 4));
        executor.state.last_stack_idx = child_idx;

        let runtime_error =
            executor.build_runtime_error(crate::error::ExecutorError::invalid_operation("test"));

        assert_eq!(
            runtime_error
                .callstack
                .iter()
                .map(|frame| frame.span.clone())
                .collect::<Vec<_>>(),
            vec![10..14, 40..44, 20..24]
        );
    }

    #[test]
    fn trace_only_eager_frames_keep_callee_before_trace_parent() {
        let mut executor =
            executor_with_root_annotations(&[(10, 14), (20, 24), (30, 34), (40, 44)]);

        let root_idx = crate::state::ExecutionState::ROOT_STACK_IDX;
        executor.state.stack_mut(root_idx).ip = (0, 1);
        executor.state.stack_mut(root_idx).call_stack.push((0, 3));

        let trace_idx = executor
            .state
            .alloc_stack((0, 2), None, Some(root_idx))
            .expect("failed to allocate trace stack");
        executor.state.stack_mut(trace_idx).call_stack.push((0, 4));

        let spans: Vec<_> = executor
            .recover_call_stack(trace_idx)
            .map(|frame| executor.best_effort_span(frame.stack_idx, frame.next_ip))
            .collect();

        assert_eq!(spans, vec![20..24, 40..44, 10..14, 30..34]);
    }

    #[test]
    fn runtime_error_span_uses_deepest_non_stdlib_frame() {
        let root = section_with_annotations(
            SectionFlags {
                is_stdlib: false,
                is_library: false,
                is_init: true,
                is_root_module: true,
            },
            &[(10, 14), (20, 24)],
        );
        let stdlib = section_with_annotations(
            SectionFlags {
                is_stdlib: true,
                is_library: true,
                is_init: false,
                is_root_module: false,
            },
            &[(100, 104)],
        );
        let mut executor = Executor::new(Bytecode::new(vec![root, stdlib]), Vec::new());

        let root_idx = crate::state::ExecutionState::ROOT_STACK_IDX;
        executor.state.stack_mut(root_idx).ip = (0, 1);
        let trace_idx = executor
            .state
            .alloc_stack((1, 1), None, Some(root_idx))
            .expect("failed to allocate trace stack");
        executor.state.stack_mut(trace_idx).call_stack.push((0, 2));
        executor.state.last_stack_idx = trace_idx;

        let runtime_error =
            executor.build_runtime_error(crate::error::ExecutorError::invalid_operation("test"));

        assert_eq!(runtime_error.span, 20..24);
        assert_eq!(
            runtime_error
                .callstack
                .iter()
                .map(|frame| frame.span.clone())
                .collect::<Vec<_>>(),
            vec![10..14, 20..24, 100..104]
        );
    }

    #[test]
    fn best_effort_span_skips_empty_end_of_section_annotations() {
        let executor = executor_with_root_annotations(&[(10, 14), (20, 24), (0, 0)]);

        assert_eq!(executor.best_effort_span(0, (0, 3)), 20..24);
    }

    #[test]
    fn best_effort_span_uses_latest_prior_root_section_span() {
        let mut first = SectionBytecode::new(SectionFlags {
            is_stdlib: false,
            is_library: false,
            is_init: true,
            is_root_module: true,
        });
        first.annotations = vec![InstructionAnnotation { source_loc: 10..24 }];
        first.instructions = vec![bytecode::Instruction::PushNil];

        let mut second = SectionBytecode::new(SectionFlags {
            is_stdlib: false,
            is_library: false,
            is_init: false,
            is_root_module: true,
        });
        second.annotations = vec![InstructionAnnotation { source_loc: 0..0 }];
        second.instructions = vec![bytecode::Instruction::EndOfExecutionHead];

        let executor = Executor::new(
            Bytecode::new(vec![Arc::new(first), Arc::new(second)]),
            Vec::new(),
        );

        assert_eq!(executor.best_effort_span(0, (1, 1)), 10..24);
    }

    #[test]
    fn record_runtime_error_at_root_with_hint_keeps_recovered_span() {
        let mut executor = executor_with_root_annotations(&[(10, 14), (20, 30)]);

        let root_idx = crate::state::ExecutionState::ROOT_STACK_IDX;
        executor.state.stack_mut(root_idx).ip = (0, 2);

        executor.record_runtime_error_at_root_with_hint(
            crate::error::ExecutorError::invalid_operation("test"),
            "extra context",
        );

        let runtime_error = executor
            .state
            .errors
            .last()
            .expect("expected recorded runtime error");
        assert_eq!(runtime_error.span, 20..30);
        assert_eq!(runtime_error.hint.as_deref(), Some("extra context"));
    }
}
