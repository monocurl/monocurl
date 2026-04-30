use super::*;

impl TextEditor {
    fn line_range_and_text(&self, state: &TextualState, line: usize) -> (Count8, Count8, String) {
        let start_loc = Location8 { row: line, col: 0 };
        let start_offset = state.loc8_to_offset8(start_loc);

        let end_loc = Location8 {
            row: line + 1,
            col: 0,
        };
        let end_offset = state.loc8_to_offset8(end_loc).min(state.len());

        let text = state.read(start_offset..end_offset);
        (start_offset, end_offset, text)
    }

    fn reshape_line(
        &mut self,
        wrap_width: Pixels,
        line_no: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> WrappedLine {
        self.state.update(cx, |state, _| {
            let (start, end, mut line_text) = self.line_range_and_text(state, line_no);
            state.mark_line_as_up_to_date_attributes(line_no, start, end);

            if line_text.ends_with('\n') {
                line_text.pop();
            }

            state.prepare_diagnostics_iterator();
            let runs: SmallVec<[TextRun; 32]> = LineShaper::new(
                &self.text_styles,
                state.lex_rope().iterator(start),
                state.static_analysis_rope().iterator(start),
                state.diagnostics().iterator(start),
                line_text.len(),
            )
            .collect();

            let line_span = start..end;
            let transcript_entries = state.transcript().entries_for_line(line_no);
            let transcript_run = TextRun {
                len: 0,
                font: self.text_styles.text_font.clone().italic(),
                color: self.text_styles.comment_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            const MAX_INLINE_ENTRIES: usize = 3;
            let mut transcript_rows: SmallVec<[(String, TextRun); 4]> = SmallVec::new();
            let transcript_entries: SmallVec<[_; 4]> = transcript_entries
                .iter()
                .filter(|entry| {
                    entry.span.start < line_span.end && line_span.start < entry.span.end
                })
                .collect();
            let total = transcript_entries.len();
            for entry in transcript_entries.iter().take(MAX_INLINE_ENTRIES) {
                transcript_rows.push((entry.text.clone(), transcript_run.clone()));
            }
            if total > MAX_INLINE_ENTRIES {
                let hidden = total - MAX_INLINE_ENTRIES;
                transcript_rows.push((
                    format!(
                        "<{} more {}>",
                        hidden,
                        if hidden == 1 { "instance" } else { "instances" }
                    ),
                    transcript_run.clone(),
                ));
            }

            WrappedLine::new_with_transcript(
                &line_text,
                self.text_styles.text_size,
                &runs,
                wrap_width,
                &transcript_rows,
                window,
            )
        })
    }

    pub(super) fn reshape_lines(
        &mut self,
        del_range: Range<usize>,
        ins_range: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let wrap_width = self.wrap_width();

        let replacement: SmallVec<[WrappedLine; 32]> = ins_range
            .map(|line_no| self.reshape_line(wrap_width, line_no, window, cx))
            .collect();

        self.line_map
            .replace_lines(del_range, replacement.into_iter());
    }

    pub(super) fn reshape_lines_needing_layout(&mut self, window: &mut Window, cx: &mut App) {
        let dirty = self
            .state
            .update(cx, |state, _cx| state.take_lines_needing_relayout());

        if let Some(dirty) = dirty {
            self.reshape_lines(dirty.clone(), dirty, window, cx);
        }
    }

    pub(super) fn reshape_visible_lines_with_stale_attributes(
        &mut self,
        window: &mut Window,
        cx: &mut App,
    ) {
        let wrap_width = self.wrap_width();
        for line in self.visible_lines() {
            let needs_reshaping = self.state.read(cx).line_has_new_attributes(line);
            if needs_reshaping {
                let new_line = self.reshape_line(wrap_width, line, window, cx);
                self.line_map
                    .replace_lines(line..line + 1, std::iter::once(new_line));
            }
        }
    }

    pub(super) fn visible_lines(&self) -> Range<usize> {
        let scroll_range = -self.scroll_handle.offset().y
            ..(-self.scroll_handle.offset().y + self.scroll_handle.bounds().size.height);
        self.line_map.prewrapped_visible_lines(scroll_range)
    }
}
