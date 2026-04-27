use super::*;

impl TextEditor {
    pub(super) fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dist = point_dist(self.last_click_position - event.position);
        if dist <= MULTI_CLICK_TOLERANCE && self.focus_handle.is_focused(window) {
            self.click_count += 1;
        } else {
            self.click_count = 1;
        }
        self.focus_handle.focus(window);

        self.is_selecting = true;
        self.last_click_position = event.position;
        self.auto_scroll_last_mouse_position = Some(event.position);
        self.start_responding_to_mouse_movements(cx);

        let pos = self.closest_index_for_mouse_position(event.position);
        match self.click_count {
            1 => {
                self.is_selecting = true;
                if event.modifiers.shift {
                    self.select_to(pos, true, false, cx);
                } else {
                    self.move_to(pos, true, false, cx);
                }
            }
            2 => {
                let state = self.state.read(cx);
                let offset = state.loc8_to_offset8(pos);
                let word_range = state.word(offset, false);

                self.set_cursor(
                    Cursor {
                        anchor: state.offset8_to_loc8(word_range.start),
                        head: state.offset8_to_loc8(word_range.end),
                    },
                    cx,
                );
                self.is_selecting = true;
                self.reset_cursor_blink(cx);
            }
            _ => {
                let state = self.state.read(cx);
                let line_start = Location8 {
                    row: pos.row,
                    col: 0,
                };
                let line_end_offset = state.loc8_to_offset8(Location8 {
                    row: pos.row,
                    col: usize::MAX,
                });
                let line_end_offset = line_end_offset.min(state.len());
                let line_end = state.offset8_to_loc8(line_end_offset);

                self.set_cursor(
                    Cursor {
                        anchor: line_start,
                        head: line_end,
                    },
                    cx,
                );
                self.is_selecting = true;
                self.reset_cursor_blink(cx);
            }
        }

        if self.reset_hover_task_if_necessary(cx) {
            cx.notify();
        }
        self.state
            .read(cx)
            .autocomplete_state()
            .borrow_mut()
            .disable();
    }

    pub(super) fn on_mouse_up(
        &mut self,
        _event: &MouseUpEvent,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.is_selecting = false;
        self.stop_responding_to_mouse_movements();
        self.auto_scroll_last_mouse_position = None;
    }

    pub(super) fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // mouse position tracking is mainly done in the listener registered in the paint
        // since we don't get mouse move events if the mouse is outside the view in this method
        self.last_in_frame_mouse_position = Some(event.position);
        if self.reset_hover_task_if_necessary(cx) {
            cx.notify();
        }
    }

    pub(super) fn on_scroll_wheel(
        &mut self,
        _event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.reset_hover_task_if_necessary(cx) {
            cx.notify();
        }
    }
}
