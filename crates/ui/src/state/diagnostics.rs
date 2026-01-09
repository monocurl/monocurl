use gpui::Hsla;
use smallvec::SmallVec;
use structs::text::{Count8, Span8};

use crate::theme::TextEditorStyles;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticType {
    CompileTimeWarning,
    CompileTimeError,
    RuntimeError,
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub dtype: DiagnosticType,
    pub span: Span8,
    pub title: String,
    pub message: String,
}

impl Diagnostic {
    pub fn is_compile_time(&self) -> bool {
        matches!(self.dtype, DiagnosticType::CompileTimeError | DiagnosticType::CompileTimeWarning)
    }

    pub fn is_runtime(&self) -> bool {
        matches!(self.dtype, DiagnosticType::RuntimeError)
    }

    pub fn color(&self, style: &TextEditorStyles) -> Hsla {
        match self.dtype {
            DiagnosticType::CompileTimeWarning => style.compile_time_warning_color,
            DiagnosticType::CompileTimeError => style.compile_time_error_color,
            DiagnosticType::RuntimeError  => style.runtime_error_color,
        }
    }
}

// Technically this can also be done with ropes, but i expect the total number of diagnostics
// to be quite small
pub struct DiagnosticContainer {
    diagnostics: Vec<Diagnostic>,
    // compressed range that says on any range, the diagnostics present
    range_map: Vec<(Count8, SmallVec<[usize; 1]>)>,
    range_map_dirty: bool,
}

impl Default for DiagnosticContainer {
    fn default() -> Self {
        Self {
            diagnostics: Vec::new(),
            range_map: vec![(usize::MAX, SmallVec::new())],
            range_map_dirty: false,
        }
    }
}

impl DiagnosticContainer {
    fn recalculate_range_map(&mut self) {
        self.range_map.clear();

        enum Event {
            Start(Count8, usize),
            End(Count8, usize),
        }

        let mut events = self.diagnostics.iter()
            .enumerate()
            .flat_map(|(i, diag)| vec![
                Event::Start(diag.span.start, i),
                Event::End(diag.span.end, i),
            ])
            .collect::<Vec<_>>();

        events.sort_by_key(|e| match e {
            Event::Start(pos, _) => (*pos, 0),
            Event::End(pos, _) => (*pos, 1),
        });

        let mut last = 0;
        let mut active = SmallVec::<[usize; 1]>::new();
        for event in events {
            match event {
                Event::Start(pos, diag_index) => {
                    if pos > last {
                        self.range_map.push((pos - last, active.clone()));
                        last = pos;
                    }
                    active.push(diag_index);
                }
                Event::End(pos, diag_index) => {
                    if let Some(index) = active.iter().position(|&i| i == diag_index) {
                        if pos > last {
                            self.range_map.push((pos - last, active.clone()));
                            last = pos;
                        }
                        active.remove(index);
                    }
                }
            }
        }
        debug_assert!(active.is_empty());
        self.range_map.push((usize::MAX, active));
        self.range_map_dirty = false;
    }

    pub fn apply_replacement(&mut self, old: Span8, new: Count8) {
        let modify_pos = |pos: &mut Count8| {
            if *pos >= old.end {
                *pos = (*pos - old.len()) + new;
            } else if *pos <= old.start {
                // no change
            } else {
                *pos = old.start;
            }
        };

        self.diagnostics
            .iter_mut()
            .for_each(|d| {
                modify_pos(&mut d.span.start);
                modify_pos(&mut d.span.end);
            });

        self.range_map_dirty = true;
    }

    pub fn diagnostic_for_point(&self, point: Count8) -> Option<&Diagnostic> {
        self.diagnostics.iter().find(|d| d.span.start <= point && point < d.span.end)
    }

    pub fn diagnostics_list(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn set_compile_time_diagnostics(&mut self, diagnostics: impl Iterator<Item = Diagnostic>) {
        self.diagnostics = std::mem::take(&mut self.diagnostics)
            .into_iter()
            .filter(|d| !d.is_compile_time())
            .chain(diagnostics)
            .collect();

        self.range_map_dirty = true;
    }

    pub fn set_runtime_diagnostics(&mut self, diagnostics: impl Iterator<Item = Diagnostic>) {
        self.diagnostics = std::mem::take(&mut self.diagnostics)
            .into_iter()
            .filter(|d| !d.is_runtime())
            .chain(diagnostics)
            .collect();

        self.range_map_dirty = true;
    }

    pub fn prepare_iterator(&mut self) {
        if self.range_map_dirty {
            self.recalculate_range_map();
        }
    }

    pub fn iterator(&self, start: Count8) -> impl Iterator<Item = (Count8, SmallVec<[&Diagnostic; 1]>)> {
        debug_assert!(!self.range_map_dirty, "Call prepare_iterator before calling iterator");

        let mut remaining = start;
        self.range_map.iter().filter_map(move |(chunk_len, diag_indices)| {
            if remaining >= *chunk_len {
                remaining -= *chunk_len;
                return None;
            }

            let yield_len = *chunk_len - remaining;
            remaining = 0;

            let diags = diag_indices
                .iter()
                .map(|&i| &self.diagnostics[i])
                .collect();

            Some((yield_len, diags))
        })
    }
}
