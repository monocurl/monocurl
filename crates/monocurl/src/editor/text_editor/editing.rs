use super::*;

impl TextEditor {
    pub fn replace(
        &mut self,
        utf8_range: Span8,
        new_text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.report_undo_candidate(utf8_range.clone(), new_text, cx);

        let (del_range, ins_range) = self.state.update(cx, |state, subcx| {
            let ret = state.replace(utf8_range.clone(), new_text, subcx);
            subcx.notify();
            ret
        });
        self.reshape_lines(del_range, ins_range, window, cx);
        self.dirty.update(cx, |dirty, _| *dirty = true);
        self.save_dirty.update(cx, |dirty, _| *dirty = true);
        self.refresh_search_after_text_change(cx);
    }

    // 0. if not inserting a single parenthesis, do normal
    // 1. if in string, do normal
    // 2. if inserting closing parenthesis and next character is not closing, do normal
    // 3. if inserting closing parenthesis and next character is closing, skip insertion
    // 4. if inserting opening parenthesis, insert matching closing parenthesis after
    fn match_parenthesis(
        &mut self,
        del: Span16,
        new_text: &str,
        cx: &App,
    ) -> Option<(Span8, String)> {
        fn in_literal(s: &str) -> bool {
            let escape = '%';
            let mut in_string = false;
            let mut prev_was_escape = false;
            for ch in s.chars() {
                if ch == escape {
                    prev_was_escape = true;
                    continue;
                }

                if ch == '"' && !prev_was_escape {
                    in_string = !in_string;
                }
                prev_was_escape = false;
            }
            in_string
        }
        fn in_lambda_definition(s: &str) -> bool {
            s.chars().filter(|&c| c == '|').count() % 2 == 1
        }

        if del.is_empty() && new_text.len() == 1 {
            let ch = new_text.chars().next().unwrap();
            let handle_closing = || {
                let state = self.state.read(cx);
                if del.start == state.len() {
                    return None;
                }
                let next = state.read(del.start..del.start + 1);
                if next.chars().next().unwrap() == ch {
                    // already exists
                    return Some((del.clone(), String::new()));
                } else {
                    return None;
                }
            };

            match ch {
                '(' | '{' | '[' | '"' | '|' => {
                    let state = self.state.read(cx);
                    let line = state.offset8_to_loc8(del.start);
                    let start_of_line = state.loc8_to_offset8(Location8 {
                        row: line.row,
                        col: 0,
                    });
                    let line_content = self.state.read(cx).read(start_of_line..del.start);
                    if in_literal(&line_content) {
                        if ch == '"' || ch == '\'' {
                            return handle_closing();
                        }
                        return None;
                    } else if ch == '|' && in_lambda_definition(&line_content) {
                        return handle_closing();
                    }
                    return Some((
                        del,
                        format!(
                            "{}{}",
                            ch,
                            match ch {
                                '(' => ')',
                                '{' => '}',
                                '[' => ']',
                                '"' => '"',
                                '|' => '|',
                                _ => unreachable!(),
                            }
                        ),
                    ));
                }
                ')' | '}' | ']' => {
                    return handle_closing();
                }
                _ => return None,
            }
        } else if del.len() == 1 && new_text.len() == 0 {
            // does this undo the last matched insertion?
            if Some(del.end) == self.last_op_matched_character {
                let state = self.state.read(cx);
                if del.end < state.len() {
                    let prev = state.read(del.end.saturating_sub(1)..del.end);
                    let next = state.read(del.end..del.end + 1);
                    let matching = match prev.chars().next() {
                        Some('(') => ')',
                        Some('{') => '}',
                        Some('[') => ']',
                        Some('"') => '"',
                        Some('|') => '|',
                        _ => return None,
                    };
                    if next.chars().next() == Some(matching) {
                        return Some((
                            Span8 {
                                start: del.start,
                                end: del.end + 1,
                            },
                            String::new(),
                        ));
                    }
                }
            }
            return None;
        }
        None
    }

    pub fn replace_text_in_utf16_range(
        &mut self,
        range_utf16: Option<Span16>,
        new_text: &str,
        raw_keystroke: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| state.start_transaction());
        let range = range_utf16
            .as_ref()
            .map(|r| self.state.read(cx).span16_to_span8(r))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.state.read(cx).cursor_range());

        if raw_keystroke
            && let Some((range, matched)) = self.match_parenthesis(range.clone(), new_text, cx)
        {
            if matched.len() == 2 {
                self.last_op_matched_character = Some(range.start + 1);
            } else {
                self.last_op_matched_character = None;
            }
            self.replace(range, &matched, window, cx);
        } else {
            self.last_op_matched_character = None;
            self.replace(range.clone(), new_text, window, cx);
        }

        let new_offset = range.start + new_text.len();
        self.move_to(
            self.state.read(cx).offset8_to_loc8(new_offset),
            false,
            false,
            cx,
        );
        self.discretely_scroll_to_cursor(cx);

        self.stop_hover();

        self.marked_range = None;
        self.reset_cursor_blink(cx);
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    pub(super) fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        if self.cursor(cx).is_empty() {
            let state = self.state.read(cx);
            let offset = state.loc8_to_offset8(self.cursor(cx).head);

            let line_start = state.loc8_to_offset8(Location8 {
                row: self.cursor(cx).head.row,
                col: 0,
            });
            let text_before = state.read(line_start..offset);

            if text_before.chars().all(|c| c == ' ') && text_before.len() >= TAB_SIZE {
                let spaces_to_delete = if text_before.len() % TAB_SIZE == 0 {
                    TAB_SIZE
                } else {
                    text_before.len() % TAB_SIZE
                };
                let new_offset = offset.saturating_sub(spaces_to_delete);
                // not really a key origin because the selction will instantly collapse
                self.select_to(state.offset8_to_loc8(new_offset), false, false, cx);
            } else {
                let new_offset = state.prev_boundary(offset);
                self.select_to(state.offset8_to_loc8(new_offset), false, false, cx);
            }

            self.replace_text_in_utf16_range(None, "", true, window, cx);
        } else {
            self.undo_group_boundary(cx);
            self.replace_text_in_utf16_range(None, "", true, window, cx);
            self.undo_group_boundary(cx);
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    pub(super) fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        if self.cursor(cx).is_empty() {
            let state = self.state.read(cx);
            let offset = state.loc8_to_offset8(self.cursor(cx).head);
            let new_offset = state.next_boundary(offset);
            self.select_to(state.offset8_to_loc8(new_offset), false, false, cx);
            self.replace_text_in_utf16_range(None, "", false, window, cx);
        } else {
            self.undo_group_boundary(cx);
            self.replace_text_in_utf16_range(None, "", false, window, cx);
            self.undo_group_boundary(cx);
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    pub(super) fn delete_word(
        &mut self,
        _: &DeleteWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| state.start_transaction());
        let had_selection = !self.cursor(cx).is_empty();
        if had_selection {
            self.undo_group_boundary(cx);
        } else {
            let state = self.state.read(cx);
            let offset = state.loc8_to_offset8(self.cursor(cx).head);
            let new_offset = self.next_word_offset(state, offset);
            self.select_to(state.offset8_to_loc8(new_offset), false, false, cx);
        }
        self.replace_text_in_utf16_range(None, "", false, window, cx);
        if had_selection {
            self.undo_group_boundary(cx);
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    pub(super) fn delete_line(
        &mut self,
        _: &DeleteLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| state.start_transaction());
        let had_selection = !self.cursor(cx).is_empty();
        if had_selection {
            self.undo_group_boundary(cx);
        } else {
            let state = self.state.read(cx);
            let row = self.cursor(cx).head.row;
            let line_end = self.line_end_offset(state, row);
            self.select_to(state.offset8_to_loc8(line_end), false, false, cx);
        }
        self.replace_text_in_utf16_range(None, "", false, window, cx);
        if had_selection {
            self.undo_group_boundary(cx);
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    pub(super) fn backspace_word(
        &mut self,
        _: &BackspaceWord,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| state.start_transaction());
        let state = self.state.read(cx);
        let mut selection = state.cursor_range();
        let word = state.word(selection.start, true);
        selection.start = word.start;
        let utf16 = state.span8_to_span16(&selection);
        self.undo_group_boundary(cx);
        self.replace_text_in_utf16_range(Some(utf16), "", false, window, cx);
        self.undo_group_boundary(cx);
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    pub(super) fn backspace_line(
        &mut self,
        _: &BackspaceLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| state.start_transaction());
        self.undo_group_boundary(cx);
        self.select_to(
            Location8 {
                row: self.cursor(cx).head.row,
                col: 0,
            },
            false,
            false,
            cx,
        );
        self.replace_text_in_utf16_range(None, "", false, window, cx);
        self.undo_group_boundary(cx);
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    pub(super) fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_query_has_focus(window, cx) {
            self.find_next_match(cx);
            return;
        }
        if self.find_replace_has_focus(window, cx) {
            self.replace_current_match(window, cx);
            return;
        }

        self.state.update(cx, |state, _| state.start_transaction());
        if self.do_autocomplete_action(cx) {
            let ac = self.state.read(cx).autocomplete_state();
            AutoCompleteState::apply_selected(&ac, self, self.state.clone(), window, cx);
        } else {
            if self.cursor(cx).is_empty() {
                // try to preserve indentation if possible
                let state = self.state.read(cx);
                let offset = state.loc8_to_offset8(self.cursor(cx).head);
                let line_start = state.loc8_to_offset8(Location8 {
                    row: self.cursor(cx).head.row,
                    col: 0,
                });
                let line_end = state.loc8_to_offset8(Location8 {
                    row: self.cursor(cx).head.row,
                    col: usize::MAX,
                });
                let text_before = state.read(line_start..offset);
                let text_after = state.read(offset..line_end);
                let leading_spaces = text_before.chars().take_while(|c| *c == ' ').count();
                let indent = " ".repeat(leading_spaces);
                if text_before.ends_with("{")
                    && (text_after.is_empty() || text_after.starts_with("}"))
                {
                    // special case: if we are between braces, insert a newline with indentation,
                    // then another newline with decreased indentation
                    let inner_indent = " ".repeat(leading_spaces + TAB_SIZE);

                    let org_loc = self.cursor(cx).head;
                    self.replace_text_in_utf16_range(
                        None,
                        &if text_after.starts_with("}") {
                            format!("\n{}\n{}", inner_indent, indent)
                        } else {
                            format!("\n{}", inner_indent)
                        },
                        true,
                        window,
                        cx,
                    );
                    // move cursor to inner line
                    let new_cursor_loc = Location8 {
                        row: org_loc.row + 1,
                        col: inner_indent.len(),
                    };
                    self.set_cursor(Cursor::collapsed(new_cursor_loc), cx);
                } else {
                    self.replace_text_in_utf16_range(
                        None,
                        &format!("\n{}", indent),
                        true,
                        window,
                        cx,
                    );
                }
            } else {
                self.replace_text_in_utf16_range(None, "\n", true, window, cx);
            }
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    fn selected_row_range(&self, cx: &App) -> Range<usize> {
        let cursor = self.cursor(cx);
        let start_row = cursor.anchor.min(cursor.head).row;
        let end_row = cursor.anchor.max(cursor.head).row;
        start_row..end_row + 1
    }

    fn line_comment_prefix_len(line_text: &str) -> Option<(usize, usize)> {
        if let Some(rest) = line_text.strip_prefix('#') {
            let removed = if rest.starts_with(' ') { 2 } else { 1 };
            Some((0, removed))
        } else {
            None
        }
    }

    pub(super) fn toggle_comment(
        &mut self,
        _: &ToggleComment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.update(cx, |state, _| state.start_transaction());
        self.undo_group_boundary(cx);

        let row_range = self.selected_row_range(cx);
        let should_uncomment = row_range.clone().all(|row| {
            let state = self.state.read(cx);
            let line_start = state.loc8_to_offset8(Location8 { row, col: 0 });
            let line_end = state
                .loc8_to_offset8(Location8 {
                    row: row + 1,
                    col: 0,
                })
                .min(state.len());
            let line_text = state.read(line_start..line_end);
            Self::line_comment_prefix_len(&line_text).is_some()
        });

        let mut cursor = self.cursor(cx);
        for row in row_range.rev() {
            let state = self.state.read(cx);
            let line_start = state.loc8_to_offset8(Location8 { row, col: 0 });
            let line_end = state
                .loc8_to_offset8(Location8 {
                    row: row + 1,
                    col: 0,
                })
                .min(state.len());
            let line_text = state.read(line_start..line_end);

            if should_uncomment {
                let Some((comment_col, removed_len)) = Self::line_comment_prefix_len(&line_text)
                else {
                    continue;
                };

                self.replace(
                    line_start + comment_col..line_start + comment_col + removed_len,
                    "",
                    window,
                    cx,
                );

                if row == cursor.anchor.row {
                    cursor.anchor.col =
                        adjust_cursor_after_uncomment(cursor.anchor.col, comment_col, removed_len);
                }
                if row == cursor.head.row {
                    cursor.head.col =
                        adjust_cursor_after_uncomment(cursor.head.col, comment_col, removed_len);
                }
            } else {
                let comment_col = 0;
                self.replace(
                    line_start + comment_col..line_start + comment_col,
                    LINE_COMMENT_PREFIX,
                    window,
                    cx,
                );

                if row == cursor.anchor.row && cursor.anchor.col >= comment_col {
                    cursor.anchor.col += LINE_COMMENT_PREFIX.len();
                }
                if row == cursor.head.row && cursor.head.col >= comment_col {
                    cursor.head.col += LINE_COMMENT_PREFIX.len();
                }
            }
        }

        self.set_cursor(cursor, cx);
        self.reset_cursor_blink(cx);
        self.undo_group_boundary(cx);
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    pub(super) fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        if self.do_autocomplete_action(cx) {
            let ac = self.state.read(cx).autocomplete_state();
            AutoCompleteState::apply_selected(&ac, self, self.state.clone(), window, cx);
        } else {
            self.undo_group_boundary(cx);
            if self.cursor(cx).is_empty() {
                self.replace_text_in_utf16_range(None, &" ".repeat(TAB_SIZE), false, window, cx);
            } else {
                let start_loc = self.cursor(cx).anchor.min(self.cursor(cx).head);
                let end_loc = self.cursor(cx).anchor.max(self.cursor(cx).head);

                for row in start_loc.row..=end_loc.row {
                    let line_start = self
                        .state
                        .read(cx)
                        .loc8_to_offset8(Location8 { row, col: 0 });
                    self.replace(line_start..line_start, &" ".repeat(TAB_SIZE), window, cx);
                }

                self.set_cursor(
                    Cursor {
                        anchor: Location8 {
                            row: self.cursor(cx).anchor.row,
                            col: self.cursor(cx).anchor.col + TAB_SIZE,
                        },
                        head: Location8 {
                            row: self.cursor(cx).head.row,
                            col: self.cursor(cx).head.col + TAB_SIZE,
                        },
                    },
                    cx,
                );
                self.reset_cursor_blink(cx);
            }
            self.undo_group_boundary(cx);
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    pub(super) fn untab(&mut self, _: &Untab, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        self.undo_group_boundary(cx);

        let mut cursor = self.cursor(cx);
        let start_loc = cursor.anchor.min(cursor.head);
        let end_loc = cursor.anchor.max(cursor.head);

        for row in (start_loc.row..=end_loc.row).rev() {
            let state = self.state.read(cx);
            let line_start = state.loc8_to_offset8(Location8 { row, col: 0 });
            let line_end = state
                .loc8_to_offset8(Location8 {
                    row: row + 1,
                    col: 0,
                })
                .min(state.len());
            let line_text = state.read(line_start..line_end);

            let spaces_to_remove = line_text
                .chars()
                .take(TAB_SIZE)
                .take_while(|&c| c == ' ')
                .count();

            if spaces_to_remove > 0 {
                self.replace(line_start..line_start + spaces_to_remove, "", window, cx);

                if row == cursor.anchor.row {
                    cursor.anchor.col = cursor.anchor.col.saturating_sub(spaces_to_remove);
                }
                if row == cursor.head.row {
                    cursor.head.col = cursor.head.col.saturating_sub(spaces_to_remove);
                }
            }
        }

        self.set_cursor(cursor, cx);
        self.reset_cursor_blink(cx);
        self.undo_group_boundary(cx);
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }
}
