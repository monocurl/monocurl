use executor::{error::RuntimeCallFrame, executor::Executor};
use structs::rope::{Rope, TextAggregate};

pub(super) fn format_runtime_error_message(
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
    runtime_error: &executor::error::RuntimeError,
) -> String {
    let mut message = runtime_error.error.to_string();
    if runtime_error.callstack.is_empty() {
        return message;
    }

    let formatted_callstack = runtime_error
        .callstack
        .iter()
        .map(|frame| format_runtime_call_frame(executor, root_text_rope, frame))
        .collect::<Vec<_>>()
        .join("\n");

    message.push_str("\n\ntop of callstack:\n");
    message.push_str(&formatted_callstack);
    message
}

fn format_runtime_call_frame(
    executor: &Executor,
    root_text_rope: &Rope<TextAggregate>,
    frame: &RuntimeCallFrame,
) -> String {
    let section_idx = frame.section as usize;
    let section = executor.section_bytecode(section_idx);

    if section.flags.is_root_module {
        let line = root_text_rope
            .utf8_prefix_summary(frame.span.start)
            .newlines
            + 1;
        format!("{}:{}", root_section_label(executor, section_idx), line)
    } else if let Some(name) = &section.source_file_name {
        format!("<{}>", name)
    } else if let Some(index) = section.import_display_index {
        format!("<imported library {}>", index)
    } else {
        "<imported library>".into()
    }
}

fn root_section_label(executor: &Executor, section_idx: usize) -> String {
    let section = executor.section_bytecode(section_idx);
    if section.flags.is_init {
        let ordinal = executor.sections()[..=section_idx]
            .iter()
            .filter(|section| section.flags.is_root_module && section.flags.is_init)
            .count();
        if ordinal <= 1 {
            "<init>".into()
        } else {
            format!("<init {}>", ordinal)
        }
    } else if section.flags.is_library {
        "<prelude>".into()
    } else {
        let ordinal = executor.sections()[..=section_idx]
            .iter()
            .filter(|section| {
                section.flags.is_root_module && !section.flags.is_library && !section.flags.is_init
            })
            .count();
        format!("<slide {}>", ordinal)
    }
}
