//! renders a `RuntimeError` as a rust-style report: a headline naming the
//! builtin that failed, a stack trace (most recent call first) with the
//! `file:line:col` and source line of every frame, and hints

use bytecode::SectionBytecode;

use crate::{
    error::{RuntimeCallFrame, RuntimeError},
    executor::Executor,
};

const MAX_SOURCE_LINE_CHARS: usize = 100;
/// keep in sync with the outer frames `build_runtime_error` keeps
const OUTER_FRAMES_KEPT: usize = 2;

pub struct RuntimeErrorReport {
    /// headline and stack trace
    pub message: String,
    pub hint: Option<String>,
}

impl RuntimeErrorReport {
    pub fn new(executor: &Executor, root_text: &str, error: &RuntimeError) -> Self {
        let innermost = error.callstack.last();
        let mut message = error
            .error
            .message_in_native(innermost.and_then(|frame| frame.native));

        if !error.callstack.is_empty() {
            message.push_str("\n\nstack trace (most recent call first):");
            let marker_after = error.callstack.len().saturating_sub(OUTER_FRAMES_KEPT);
            for (shown, frame) in error.callstack.iter().rev().enumerate() {
                if error.omitted_frames > 0 && shown == marker_after {
                    message.push_str(&format!(
                        "\n  ... {} frame{} omitted ...",
                        error.omitted_frames,
                        if error.omitted_frames == 1 { "" } else { "s" }
                    ));
                }
                message.push('\n');
                message.push_str(&format_frame(executor, root_text, frame));
            }
        }

        let hints: Vec<String> = error
            .hint
            .clone()
            .into_iter()
            .chain(library_context_hint(executor, root_text, error))
            .collect();

        Self {
            message,
            hint: (!hints.is_empty()).then(|| hints.join("\n")),
        }
    }

    /// single block of text, for terminals and logs
    pub fn to_plain_text(&self) -> String {
        match &self.hint {
            Some(hint) => format!(
                "{}\n\nhint: {}",
                self.message,
                hint.replace('\n', "\nhint: ")
            ),
            None => self.message.clone(),
        }
    }
}

struct FrameLocation {
    file: String,
    context: Option<String>,
    line: usize,
    column: usize,
    source_line: Option<String>,
}

fn format_frame(executor: &Executor, root_text: &str, frame: &RuntimeCallFrame) -> String {
    let in_native = frame
        .native
        .map(|name| format!(", in builtin `{name}`"))
        .unwrap_or_default();

    let Some(location) = locate_frame(executor, root_text, frame) else {
        let section = executor.section_bytecode(frame.section as usize);
        return format!("  --> {}{}", file_label(section), in_native);
    };

    let context = location
        .context
        .map(|context| format!(" ({context})"))
        .unwrap_or_default();
    let mut out = format!(
        "  --> {}:{}:{}{}{}",
        location.file, location.line, location.column, context, in_native
    );
    if let Some(source_line) = location.source_line {
        out.push_str("\n       |  ");
        out.push_str(&source_line);
    }
    out
}

fn locate_frame(
    executor: &Executor,
    root_text: &str,
    frame: &RuntimeCallFrame,
) -> Option<FrameLocation> {
    let section_idx = frame.section as usize;
    let section = executor.section_bytecode(section_idx);
    let text = if section.flags.is_root_module {
        // the prelude has no source of its own
        if section.flags.is_stdlib {
            return None;
        }
        root_text
    } else {
        section.source_text.as_deref()?
    };

    let start = frame.span.start.min(text.len());
    let start = (0..=start).rev().find(|&i| text.is_char_boundary(i))?;
    let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);

    Some(FrameLocation {
        file: file_label(section),
        context: section
            .flags
            .is_root_module
            .then(|| root_section_label(executor, section_idx)),
        line: text[..line_start].matches('\n').count() + 1,
        column: text[line_start..start].chars().count() + 1,
        source_line: display_source_line(&text[line_start..line_end]),
    })
}

fn display_source_line(line: &str) -> Option<String> {
    let line = line.replace("__monocurl__native__ ", "");
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    if line.chars().count() <= MAX_SOURCE_LINE_CHARS {
        return Some(line.to_owned());
    }
    let mut shortened: String = line.chars().take(MAX_SOURCE_LINE_CHARS).collect();
    shortened.push('…');
    Some(shortened)
}

fn file_label(section: &SectionBytecode) -> String {
    match (&section.source_file_name, section.flags.is_stdlib) {
        (Some(name), true) => format!("std/{name}"),
        (Some(name), false) => name.clone(),
        (None, _) if section.flags.is_root_module => "<scene>".into(),
        (None, _) => match section.import_display_index {
            Some(index) => format!("<imported library {index}>"),
            None => "<imported library>".into(),
        },
    }
}

/// which part of the root document a root section is
pub fn root_section_label(executor: &Executor, section_idx: usize) -> String {
    let sections = &executor.sections()[..=section_idx];
    let section = executor.section_bytecode(section_idx);
    if section.flags.is_init {
        let ordinal = sections
            .iter()
            .filter(|section| section.flags.is_root_module && section.flags.is_init)
            .count();
        if ordinal <= 1 {
            "init".into()
        } else {
            format!("init {ordinal}")
        }
    } else if section.flags.is_library {
        "prelude".into()
    } else {
        let ordinal = sections
            .iter()
            .filter(|section| {
                section.flags.is_root_module && !section.flags.is_library && !section.flags.is_init
            })
            .count();
        match &section.name {
            Some(name) => format!("slide {ordinal} \"{name}\""),
            None => format!("slide {ordinal}"),
        }
    }
}

/// when the failure happened inside a library, point back at the scene line
/// that called into it: that is where the bad value came from
fn library_context_hint(
    executor: &Executor,
    root_text: &str,
    error: &RuntimeError,
) -> Option<String> {
    let innermost = error.callstack.last()?;
    let failing_section = executor.section_bytecode(innermost.section as usize);
    if failing_section.flags.is_root_module {
        return None;
    }
    let scene_frame = error.callstack.iter().rev().find(|frame| {
        let flags = &executor.section_bytecode(frame.section as usize).flags;
        flags.is_root_module && !flags.is_stdlib
    })?;
    let line = root_text[..scene_frame.span.start.min(root_text.len())]
        .matches('\n')
        .count()
        + 1;
    let library = file_label(failing_section);
    let kind = if failing_section.flags.is_stdlib {
        "the standard library"
    } else {
        "an imported library"
    };
    Some(format!(
        "this failed inside {kind} ({library}) while running the call on line {line} of your \
         scene. The library code is likely fine: check the arguments passed on line {line}, \
         including the values returned by any functions you passed in"
    ))
}
