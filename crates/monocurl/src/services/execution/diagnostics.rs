use executor::{error::RuntimeError, error_report::RuntimeErrorReport, executor::Executor};
use structs::rope::{Rope, TextAggregate};

pub(super) fn runtime_error_report(
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
    runtime_error: &RuntimeError,
) -> RuntimeErrorReport {
    let root_text: String = root_text_rope.iterator(0).collect();
    RuntimeErrorReport::new(executor, &root_text, runtime_error)
}
