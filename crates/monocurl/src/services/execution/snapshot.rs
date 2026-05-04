use futures::channel::mpsc::UnboundedSender;
use runtime::RuntimeController;
use structs::rope::{Rope, TextAggregate};

use crate::{services::ServiceManagerMessage, state::diagnostics::Diagnostic};

use super::{
    ExecutionService, ExecutionSnapshot, ExecutionStatus, diagnostics::format_runtime_error_message,
};

impl ExecutionService {
    pub(super) fn emit_snapshot(
        sm_tx: &UnboundedSender<ServiceManagerMessage>,
        runtime: &RuntimeController,
        root_text_rope: &Rope<TextAggregate>,
        version: usize,
        snapshot: ExecutionSnapshot,
    ) {
        let has_compiler_error = snapshot.status == ExecutionStatus::CompileError;
        let transcript = snapshot.transcript.clone();

        sm_tx
            .unbounded_send(ServiceManagerMessage::ExecutionStateUpdated {
                snapshot: Box::new(snapshot),
            })
            .ok();

        if let Some(transcript) = transcript {
            sm_tx
                .unbounded_send(ServiceManagerMessage::UpdateTranscript {
                    transcript,
                    version,
                })
                .ok();
        }

        let diagnostics = runtime.with_executor(|executor| {
            executor
                .state
                .errors
                .iter()
                .map(|runtime_error| Diagnostic {
                    dtype: crate::state::diagnostics::DiagnosticType::RuntimeError,
                    span: runtime_error.span.clone(),
                    title: "Runtime Error".into(),
                    message: format_runtime_error_message(executor, root_text_rope, runtime_error),
                })
                .collect()
        });

        if has_compiler_error {
            sm_tx
                .unbounded_send(ServiceManagerMessage::UpdateRuntimeDiagnostics {
                    diagnostics: Vec::new(),
                    version,
                })
                .ok();
        } else {
            sm_tx
                .unbounded_send(ServiceManagerMessage::UpdateRuntimeDiagnostics {
                    diagnostics,
                    version,
                })
                .ok();
        }
    }
}
