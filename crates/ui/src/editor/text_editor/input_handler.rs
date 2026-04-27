use super::*;

impl EntityInputHandler for TextEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let state = self.state.read(cx);
        let range = state.span16_to_span8(&range_utf16);
        actual_range.replace(state.span8_to_span16(&range));
        Some(state.read(range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let state = self.state.read(cx);
        let range = state.cursor_range();
        Some(UTF16Selection {
            range: state.span8_to_span16(&range),
            reversed: self.cursor(cx).reversed(),
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.state.read(cx).span8_to_span16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_text_in_utf16_range(range_utf16, new_text, true, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.state.read(cx).span16_to_span8(r))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.state.read(cx).cursor_range());

        self.replace(range.clone(), new_text, w, cx);

        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }

        if let Some(new_range_utf16) = new_selected_range_utf16 {
            let state = self.state.read(cx);
            let new_range = state.span16_to_span8(&new_range_utf16);
            let adjusted_start = range.start + new_range.start;
            let adjusted_end = range.start + new_range.end;
            self.set_cursor(
                Cursor {
                    anchor: state.offset8_to_loc8(adjusted_start),
                    head: state.offset8_to_loc8(adjusted_end),
                },
                cx,
            );
        }

        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let state = self.state.read(_cx);
        let range = state.span16_to_span8(&range_utf16);
        let start_loc = state.offset8_to_loc8(range.start);
        let end_loc = state.offset8_to_loc8(range.end);

        let start = self.line_map.point_for_location(start_loc);
        let end = self.line_map.point_for_location(end_loc);

        let scroll_offset = self.scroll_handle.offset();

        Some(Bounds::from_corners(
            point(
                bounds.left() + self.gutter_width + start.x,
                bounds.top() + start.y + scroll_offset.y,
            ),
            point(
                bounds.left() + self.gutter_width + end.x,
                bounds.top() + end.y + scroll_offset.y + self.line_height,
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<usize> {
        let state = self.state.read(cx);
        let loc8 = self.closest_index_for_mouse_position(point);
        let offset8 = state.loc8_to_offset8(loc8);
        Some(state.offset8_to_offset16(offset8) as usize)
    }
}

impl Focusable for TextEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
