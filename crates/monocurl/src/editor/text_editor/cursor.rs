use super::*;

impl TextEditor {
    pub(super) fn cursor(&self, cx: &App) -> Cursor {
        self.state.read(cx).cursor()
    }

    pub(super) fn set_cursor(&self, cursor: Cursor, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            state.start_transaction();
            state.set_cursor(cursor, cx);
            state.end_transaction(cx);
        });
    }

    pub fn reset_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_blink_state = true;
        cx.notify();

        let task = cx.spawn(
            async move |editor: WeakEntity<TextEditor>, cx: &mut AsyncApp| {
                cx.background_executor().timer(CURSOR_BLINK_DELAY).await;
                loop {
                    let should_continue = editor
                        .update(cx, |editor, cx| {
                            editor.cursor_blink_state = !editor.cursor_blink_state;
                            cx.notify();
                            true
                        })
                        .ok()
                        .unwrap_or(false);

                    if !should_continue {
                        break;
                    }

                    cx.background_executor().timer(CURSOR_BLINK_INTERVAL).await;
                }
            },
        );
        // cancels any previous tasks as well
        self.cursor_blink_task = Some(task);
    }

    pub(super) fn move_to(
        &mut self,
        pos: Location8,
        mouse_origin: bool,
        key_origin: bool,
        cx: &mut Context<Self>,
    ) {
        self.set_cursor(Cursor::collapsed(pos), cx);
        self.reset_cursor_blink(cx);
        if !mouse_origin {
            self.discretely_scroll_to_cursor(cx);
        }

        if key_origin || mouse_origin {
            self.undo_group_boundary(cx);
        }
    }

    pub(super) fn select_to(
        &mut self,
        pos: Location8,
        mouse_origin: bool,
        key_origin: bool,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, cx| {
            state.start_transaction();
            state.set_cursor_head(pos, cx);
            state.end_transaction(cx);
        });
        self.reset_cursor_blink(cx);
        if !mouse_origin {
            self.discretely_scroll_to_cursor(cx);
        }

        if key_origin || mouse_origin {
            self.undo_group_boundary(cx);
        }
    }

    fn vertical_cursor_movement(&self, delta_lines: isize, cx: &App) -> Location8 {
        let current_pos = self.line_map.point_for_location(self.cursor(cx).head);
        let target_y = current_pos.y + delta_lines as f32 * self.line_height;
        match self
            .line_map
            .location_for_point(point(current_pos.x, target_y))
        {
            Ok(loc) => loc,
            Err(loc) => loc,
        }
    }

    pub(super) fn previous_word_offset(&self, state: &TextualState, offset: Count8) -> Count8 {
        state.word(offset.min(state.len()), true).start
    }

    pub(super) fn next_word_offset(&self, state: &TextualState, offset: Count8) -> Count8 {
        fn is_word(ch: char) -> bool {
            ch.is_alphanumeric() || ch == '_'
        }

        let mut cursor = offset.min(state.len());
        while cursor < state.len() {
            let next = state.next_boundary(cursor);
            if !state
                .read(cursor..next)
                .chars()
                .all(|ch| ch.is_whitespace())
            {
                break;
            }
            cursor = next;
        }

        if cursor == state.len() {
            return cursor;
        }

        let next = state.next_boundary(cursor);
        let starts_word = state.read(cursor..next).chars().all(is_word);
        while cursor < state.len() {
            let next = state.next_boundary(cursor);
            let content = state.read(cursor..next);
            let is_same_boundary_kind = if starts_word {
                content.chars().all(is_word)
            } else {
                content
                    .chars()
                    .all(|ch| !ch.is_whitespace() && !is_word(ch))
            };
            if !is_same_boundary_kind {
                break;
            }
            cursor = next;
        }
        cursor
    }

    pub(super) fn line_end_offset(&self, state: &TextualState, row: usize) -> Count8 {
        state
            .loc8_to_offset8(Location8 {
                row,
                col: usize::MAX,
            })
            .min(state.len())
    }

    pub(super) fn do_autocomplete_action(&mut self, cx: &mut Context<Self>) -> bool {
        let state = self.state.read(cx);
        state
            .autocomplete_state()
            .borrow_mut()
            .recheck_should_display(state.cursor())
    }

    pub(super) fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.do_autocomplete_action(cx) {
            self.state
                .read(cx)
                .autocomplete_state()
                .borrow_mut()
                .move_index(-1);
            cx.notify();
        } else {
            self.move_to(self.vertical_cursor_movement(-1, cx), false, true, cx);
        }
    }

    pub(super) fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.do_autocomplete_action(cx) {
            self.state
                .read(cx)
                .autocomplete_state()
                .borrow_mut()
                .move_index(1);
            cx.notify();
        } else {
            self.move_to(self.vertical_cursor_movement(1, cx), false, true, cx);
        }
    }

    pub(super) fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !self.cursor(cx).is_empty() {
            let range = state.cursor_range();
            self.move_to(state.offset8_to_loc8(range.start), false, true, cx);
        } else {
            let offset = state.loc8_to_offset8(self.cursor(cx).head);
            let new_offset = state.prev_boundary(offset);
            self.move_to(state.offset8_to_loc8(new_offset), false, true, cx);
        }
    }

    pub(super) fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !self.cursor(cx).is_empty() {
            let range = state.cursor_range();
            self.move_to(state.offset8_to_loc8(range.end), false, true, cx);
        } else {
            let offset = state.loc8_to_offset8(self.cursor(cx).head);
            let new_offset = state.next_boundary(offset);
            self.move_to(state.offset8_to_loc8(new_offset), false, true, cx);
        }
    }

    pub(super) fn left_word(&mut self, _: &LeftWord, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !self.cursor(cx).is_empty() {
            let range = state.cursor_range();
            self.move_to(state.offset8_to_loc8(range.start), false, true, cx);
        } else {
            let offset = state.loc8_to_offset8(self.cursor(cx).head);
            let new_offset = self.previous_word_offset(state, offset);
            self.move_to(state.offset8_to_loc8(new_offset), false, true, cx);
        }
    }

    pub(super) fn right_word(&mut self, _: &RightWord, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !self.cursor(cx).is_empty() {
            let range = state.cursor_range();
            self.move_to(state.offset8_to_loc8(range.end), false, true, cx);
        } else {
            let offset = state.loc8_to_offset8(self.cursor(cx).head);
            let new_offset = self.next_word_offset(state, offset);
            self.move_to(state.offset8_to_loc8(new_offset), false, true, cx);
        }
    }

    pub(super) fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let offset = state.loc8_to_offset8(self.cursor(cx).head);
        let new_offset = state.prev_boundary(offset);
        self.select_to(state.offset8_to_loc8(new_offset), false, true, cx);
    }

    pub(super) fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let offset = state.loc8_to_offset8(self.cursor(cx).head);
        let new_offset = state.next_boundary(offset);
        self.select_to(state.offset8_to_loc8(new_offset), false, true, cx);
    }

    pub(super) fn select_left_word(
        &mut self,
        _: &SelectLeftWord,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.read(cx);
        let offset = state.loc8_to_offset8(self.cursor(cx).head);
        let new_offset = self.previous_word_offset(state, offset);
        self.select_to(state.offset8_to_loc8(new_offset), false, true, cx);
    }

    pub(super) fn select_right_word(
        &mut self,
        _: &SelectRightWord,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.read(cx);
        let offset = state.loc8_to_offset8(self.cursor(cx).head);
        let new_offset = self.next_word_offset(state, offset);
        self.select_to(state.offset8_to_loc8(new_offset), false, true, cx);
    }

    pub(super) fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical_cursor_movement(-1, cx), false, true, cx);
    }

    pub(super) fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical_cursor_movement(1, cx), false, true, cx);
    }

    pub(super) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        self.set_cursor(
            Cursor {
                anchor: Location8 { row: 0, col: 0 },
                head: state.offset8_to_loc8(state.len()),
            },
            cx,
        );
        self.discretely_scroll_to_cursor(cx);
        cx.notify();
    }

    pub(super) fn select_home(&mut self, _: &SelectHome, _: &mut Window, cx: &mut Context<Self>) {
        let new_pos = Location8 {
            row: self.cursor(cx).head.row,
            col: 0,
        };
        self.select_to(new_pos, false, true, cx);
    }

    pub(super) fn select_end(&mut self, _: &SelectEnd, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let row = self.cursor(cx).head.row;
        let line_end = self.line_end_offset(state, row);
        self.select_to(state.offset8_to_loc8(line_end), false, true, cx);
    }

    pub(super) fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let new_pos = Location8 {
            row: self.cursor(cx).head.row,
            col: 0,
        };
        self.move_to(new_pos, false, true, cx);
    }

    pub(super) fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let row = self.cursor(cx).head.row;
        let line_end = self.line_end_offset(state, row);
        self.move_to(state.offset8_to_loc8(line_end), false, true, cx);
    }
}
