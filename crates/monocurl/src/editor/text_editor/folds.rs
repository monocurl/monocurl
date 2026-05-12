use std::collections::BTreeSet;

use crate::state::{
    diagnostics::DiagnosticType, text_replacement::TextReplacement, textual_state::SlideInfo,
};

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum FoldDiagnosticSeverity {
    Warning,
    CompileError,
    RuntimeError,
}

impl FoldDiagnosticSeverity {
    fn from_type(dtype: DiagnosticType) -> Self {
        match dtype {
            DiagnosticType::CompileTimeWarning => Self::Warning,
            DiagnosticType::CompileTimeError => Self::CompileError,
            DiagnosticType::RuntimeError => Self::RuntimeError,
        }
    }
}

impl TextEditor {
    pub(super) fn slide_body_line_range(
        slides: &[SlideInfo],
        slide_idx: usize,
        line_count: usize,
    ) -> Range<usize> {
        let Some(slide) = slides.get(slide_idx) else {
            return 0..0;
        };

        let start = (slide.line + 1).min(line_count);
        let end = slides
            .get(slide_idx + 1)
            .map(|slide| slide.line)
            .unwrap_or(line_count)
            .min(line_count);

        start..end.max(start)
    }

    fn line_start_offset(state: &TextualState, line: usize, line_count: usize) -> Count8 {
        if line >= line_count {
            state.len()
        } else {
            state
                .loc8_to_offset8(Location8 { row: line, col: 0 })
                .min(state.len())
        }
    }

    fn slide_body_offset_range(
        &self,
        state: &TextualState,
        slides: &[SlideInfo],
        slide_idx: usize,
    ) -> Span8 {
        let line_count = self.line_map.line_count();
        let line_range = Self::slide_body_line_range(slides, slide_idx, line_count);
        let start = Self::line_start_offset(state, line_range.start, line_count);
        let end = Self::line_start_offset(state, line_range.end, line_count);
        start..end.max(start)
    }

    fn reconcile_folded_slides(&mut self, slides: &[SlideInfo]) {
        let valid_starts = slides
            .iter()
            .map(|slide| slide.start_offset)
            .collect::<BTreeSet<_>>();
        self.folded_slide_starts
            .retain(|start| valid_starts.contains(start));
    }

    fn apply_folds_for_slides(&mut self, slides: &[SlideInfo]) {
        let line_count = self.line_map.line_count();

        if self.has_hidden_lines {
            self.line_map.set_hidden_range(0..line_count, false);
        }

        self.has_hidden_lines = false;
        for (slide_idx, slide) in slides.iter().enumerate() {
            if !self.folded_slide_starts.contains(&slide.start_offset) {
                continue;
            }

            let body = Self::slide_body_line_range(slides, slide_idx, line_count);
            if body.is_empty() {
                continue;
            }

            self.line_map.set_hidden_range(body, true);
            self.has_hidden_lines = true;
        }
    }

    pub(super) fn apply_folds(&mut self, cx: &mut App) {
        let slides = self.state.read(cx).slides().to_vec();
        self.reconcile_folded_slides(&slides);
        self.apply_folds_for_slides(&slides);
        self.observed_slides = slides;
    }

    pub(super) fn sync_folds_to_slides(&mut self, cx: &mut Context<Self>) {
        let slides = self.state.read(cx).slides().to_vec();
        if slides == self.observed_slides {
            return;
        }

        self.reconcile_folded_slides(&slides);
        self.apply_folds_for_slides(&slides);
        self.observed_slides = slides;
        if self.clip_cursor_out_of_folds(cx) {
            cx.notify();
        }
    }

    pub(super) fn remap_folded_slide_starts(&mut self, replacement: &TextReplacement) {
        self.folded_slide_starts = self
            .folded_slide_starts
            .iter()
            .map(|start| replacement.map_offset(*start))
            .collect();
    }

    pub(super) fn reapply_folds_after_text_change(&mut self, cx: &mut App) {
        self.apply_folds(cx);
        self.clip_cursor_out_of_folds(cx);
    }

    pub(super) fn unfold_folds_containing_cursor(
        &mut self,
        cursor: Cursor,
        cx: &mut App,
    ) -> Cursor {
        let state = self.state.read(cx);
        let slides = state.slides();
        let line_count = self.line_map.line_count();
        let starts = [cursor.anchor, cursor.head]
            .into_iter()
            .filter(|loc| self.line_map.is_line_hidden(loc.row))
            .filter_map(|loc| {
                slides.iter().enumerate().find_map(|(slide_idx, slide)| {
                    let body = Self::slide_body_line_range(slides, slide_idx, line_count);
                    (body.contains(&loc.row)
                        && self.folded_slide_starts.contains(&slide.start_offset))
                    .then_some(slide.start_offset)
                })
            })
            .collect::<Vec<_>>();

        if starts.is_empty() {
            return cursor;
        }

        for start in starts {
            self.folded_slide_starts.remove(&start);
        }
        self.apply_folds(cx);
        self.cursor_with_folded_endpoints(cursor, cx)
    }

    pub(super) fn unfold_folds_touched_by_span(&mut self, span: Span8, cx: &mut App) {
        if self.folded_slide_starts.is_empty() {
            return;
        }

        let state = self.state.read(cx);
        let slides = state.slides();
        let touched = slides
            .iter()
            .enumerate()
            .filter(|(_, slide)| self.folded_slide_starts.contains(&slide.start_offset))
            .filter(|(slide_idx, slide)| {
                let body_offsets = self.slide_body_offset_range(state, slides, *slide_idx);
                if body_offsets.is_empty() {
                    return false;
                }

                if span.is_empty() {
                    let loc = state.offset8_to_loc8(span.start.min(state.len()));
                    self.line_map.is_line_hidden(loc.row)
                        && body_offsets.start <= span.start
                        && span.start <= body_offsets.end
                } else {
                    span.start < slide.source_range.end && slide.source_range.start < span.end
                }
            })
            .map(|(_, slide)| slide.start_offset)
            .collect::<Vec<_>>();

        if touched.is_empty() {
            return;
        }

        for start in touched {
            self.folded_slide_starts.remove(&start);
        }
        self.apply_folds(cx);
    }

    pub(super) fn is_slide_folded(&self, slide: &SlideInfo) -> bool {
        self.folded_slide_starts.contains(&slide.start_offset)
    }

    pub(super) fn toggle_slide_fold_at_index(&mut self, slide_idx: usize, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let slides = state.slides();
        let Some(slide) = slides.get(slide_idx) else {
            return;
        };

        let body = Self::slide_body_line_range(slides, slide_idx, self.line_map.line_count());
        if body.is_empty() {
            return;
        }

        let start = slide.start_offset;

        if !self.folded_slide_starts.insert(start) {
            self.folded_slide_starts.remove(&start);
        }

        self.apply_folds(cx);
        self.clip_cursor_out_of_folds(cx);
        cx.notify();
    }

    pub(super) fn toggle_slide_fold_at_start(
        &mut self,
        start_offset: Count8,
        cx: &mut Context<Self>,
    ) {
        let slide_idx = self
            .state
            .read(cx)
            .slides()
            .iter()
            .position(|slide| slide.start_offset == start_offset);

        if let Some(slide_idx) = slide_idx {
            self.toggle_slide_fold_at_index(slide_idx, cx);
        }
    }

    pub(super) fn toggle_slide_fold(
        &mut self,
        _: &ToggleSlideFold,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row = self.cursor(cx).head.row;
        let slide_idx = self
            .state
            .read(cx)
            .slides()
            .iter()
            .rposition(|slide| slide.line <= row);

        if let Some(slide_idx) = slide_idx {
            self.toggle_slide_fold_at_index(slide_idx, cx);
        }
    }

    fn clip_location_out_of_folds(&self, loc: Location8, cx: &App) -> Location8 {
        if !self.line_map.is_line_hidden(loc.row) {
            return loc;
        }

        let state = self.state.read(cx);
        let slides = state.slides();
        let line_count = self.line_map.line_count();
        for (slide_idx, slide) in slides.iter().enumerate() {
            let body = Self::slide_body_line_range(slides, slide_idx, line_count);
            if body.contains(&loc.row) {
                let row = slide.header_end.row.min(line_count.saturating_sub(1));
                return Location8 {
                    row,
                    col: slide.header_end.col.min(self.line_map.line_len(row)),
                };
            }
        }

        loc
    }

    pub(super) fn cursor_with_folded_endpoints(&self, cursor: Cursor, cx: &App) -> Cursor {
        Cursor {
            anchor: self.clip_location_out_of_folds(cursor.anchor, cx),
            head: self.clip_location_out_of_folds(cursor.head, cx),
        }
    }

    pub(super) fn clip_cursor_out_of_folds(&mut self, cx: &mut App) -> bool {
        let cursor = self.cursor(cx);
        let clipped = self.cursor_with_folded_endpoints(cursor, cx);
        if cursor == clipped {
            return false;
        }

        self.set_cursor_without_unfolding(clipped, cx);
        self.discretely_scroll_to_cursor(cx);
        true
    }

    pub(super) fn debug_assert_cursor_not_in_fold(&self, cx: &App) {
        let cursor = self.cursor(cx);
        debug_assert!(
            !self.line_map.is_line_hidden(cursor.anchor.row),
            "cursor anchor is in a folded region"
        );
        debug_assert!(
            !self.line_map.is_line_hidden(cursor.head.row),
            "cursor head is in a folded region"
        );
    }

    pub(super) fn folded_slide_diagnostic_type(
        &self,
        slide_idx: usize,
        cx: &App,
    ) -> Option<DiagnosticType> {
        let state = self.state.read(cx);
        let slides = state.slides();
        let body_offsets = self.slide_body_offset_range(state, slides, slide_idx);
        if body_offsets.is_empty() {
            return None;
        }

        state
            .diagnostics()
            .diagnostics_list()
            .iter()
            .filter(|diagnostic| {
                body_offsets.start <= diagnostic.span.start
                    && diagnostic.span.end <= body_offsets.end
            })
            .max_by_key(|diagnostic| FoldDiagnosticSeverity::from_type(diagnostic.dtype))
            .map(|diagnostic| diagnostic.dtype)
    }
}
