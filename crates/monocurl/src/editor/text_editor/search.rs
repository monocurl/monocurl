use super::*;

impl TextEditor {
    pub(super) fn on_find_query_edited(&mut self, cx: &mut Context<Self>) {
        if !self.search.visible {
            return;
        }

        let offset = self.state.read(cx).cursor_range().start;
        self.rebuild_search_matches(cx);
        self.select_search_match_near_offset(offset, false, cx);
    }

    pub(super) fn refresh_search_after_text_change(&mut self, cx: &mut App) {
        if !self.search.visible || self.search.suppress_refresh {
            return;
        }

        let offset = self.state.read(cx).cursor_range().start;
        self.rebuild_search_matches(cx);
        self.search.active_match = self.search_match_near_offset(offset, false);
    }

    pub fn open_find_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let was_visible = self.search.visible;
        self.search.visible = true;

        if was_visible {
            self.find_query_input
                .update(cx, |input, cx| input.set_content("", cx));
        } else if let Some(selected_text) = self.selected_text_for_find_query(cx) {
            self.find_query_input.update(cx, |input, cx| {
                input.set_content(selected_text, cx);
                input.select_all(cx);
            });
        } else {
            self.find_query_input
                .update(cx, |input, cx| input.select_all(cx));
        }

        let offset = self.state.read(cx).cursor_range().start;
        self.rebuild_search_matches(cx);
        self.select_search_match_near_offset(offset, false, cx);
        self.find_query_input.read(cx).focus(window);
        cx.notify();
    }

    pub(super) fn open_find(&mut self, _: &OpenFind, window: &mut Window, cx: &mut Context<Self>) {
        self.open_find_panel(window, cx);
    }

    pub(super) fn close_find(
        &mut self,
        _: &CloseFind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_find_panel(window, cx);
    }

    pub(super) fn find_next(&mut self, _: &FindNext, _window: &mut Window, cx: &mut Context<Self>) {
        self.find_next_match(cx);
    }

    pub(super) fn find_previous(
        &mut self,
        _: &FindPrevious,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.find_previous_match(cx);
    }

    pub(super) fn replace_current(
        &mut self,
        _: &ReplaceCurrent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_current_match(window, cx);
    }

    pub(super) fn replace_all(
        &mut self,
        _: &ReplaceAll,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.replace_all_matches(window, cx);
    }

    pub(super) fn find_query_has_focus(&self, window: &Window, cx: &App) -> bool {
        self.search.visible && self.find_query_input.read(cx).is_focused(window)
    }

    pub(super) fn find_replace_has_focus(&self, window: &Window, cx: &App) -> bool {
        self.search.visible && self.find_replace_input.read(cx).is_focused(window)
    }

    pub(super) fn focus_next_find_field(
        &mut self,
        _: &Tab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_next_find_field_inner(window, cx);
    }

    pub(super) fn focus_previous_find_field(
        &mut self,
        _: &Untab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_next_find_field_inner(window, cx);
    }

    pub(super) fn render_find_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.search.visible {
            return None;
        }

        let theme = ThemeSettings::theme(cx);
        let status = if self.find_query_input.read(cx).content().is_empty() {
            String::new()
        } else if let Some(active) = self.search.active_match {
            format!("{} / {}", active + 1, self.search.matches.len())
        } else {
            format!("0 / {}", self.search.matches.len())
        };
        let button = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .px(px(8.0))
                .h(px(24.0))
                .flex()
                .items_center()
                .rounded(px(4.0))
                .border_1()
                .border_color(theme.navbar_border)
                .text_size(px(11.0))
                .text_color(theme.text_primary)
                .bg(theme.viewport_stage_background)
                .hover({
                    let hover = theme.row_hover_overlay;
                    move |style| style.bg(hover)
                })
                .cursor_pointer()
                .child(label)
        };

        Some(
            div()
                .absolute()
                .top(px(8.0))
                .right(px(18.0))
                .w(px(440.0))
                .p(px(8.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(theme.navbar_border)
                .bg(theme.tab_active_background)
                .key_context("find-panel")
                .on_action(cx.listener(Self::focus_next_find_field))
                .on_action(cx.listener(Self::focus_previous_find_field))
                .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_mouse_up(MouseButton::Left, |_event, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .on_mouse_move(|_event, _window, cx| {
                    cx.stop_propagation();
                })
                .on_scroll_wheel(|_event, _window, cx| {
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(self.find_query_input.clone()),
                        )
                        .child(
                            div()
                                .w(px(52.0))
                                .text_size(px(11.0))
                                .text_color(theme.text_muted)
                                .child(status),
                        )
                        .child(button("find-previous", "Prev").on_click(cx.listener(
                            |this, _, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.find_previous_match(cx);
                            },
                        )))
                        .child(button("find-next", "Next").on_click(cx.listener(
                            |this, _, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.find_next_match(cx);
                            },
                        )))
                        .child(button("find-close", "Close").on_click(cx.listener(
                            |this, _, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.close_find_panel(window, cx);
                            },
                        ))),
                )
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap(px(6.0))
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .child(self.find_replace_input.clone()),
                        )
                        .child(button("replace-current", "Replace").on_click(cx.listener(
                            |this, _, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.replace_current_match(window, cx);
                            },
                        )))
                        .child(button("replace-all", "All").on_click(cx.listener(
                            |this, _, window, cx| {
                                window.prevent_default();
                                cx.stop_propagation();
                                this.replace_all_matches(window, cx);
                            },
                        ))),
                )
                .into_any_element(),
        )
    }

    fn selected_text_for_find_query(&self, cx: &App) -> Option<String> {
        let state = self.state.read(cx);
        let range = state.cursor_range();
        if range.is_empty() {
            return None;
        }

        let text = state.read(range);
        (!text.contains('\n')).then_some(text)
    }

    pub fn close_find_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.hide_find_panel();
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(super) fn close_find_panel_without_focus(&mut self, cx: &mut Context<Self>) {
        self.hide_find_panel();
        cx.notify();
    }

    fn hide_find_panel(&mut self) {
        self.search.visible = false;
        self.search.matches.clear();
        self.search.active_match = None;
    }

    fn focus_next_find_field_inner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_query_has_focus(window, cx) {
            self.find_replace_input.read(cx).focus(window);
        } else {
            self.find_query_input.read(cx).focus(window);
        }
        cx.notify();
    }

    fn query(&self, cx: &App) -> String {
        self.find_query_input.read(cx).content().to_string()
    }

    fn replacement(&self, cx: &App) -> String {
        self.find_replace_input.read(cx).content().to_string()
    }

    fn rebuild_search_matches(&mut self, cx: &App) {
        self.search.matches.clear();
        self.search.active_match = None;

        let query = self.query(cx);
        if query.is_empty() {
            return;
        }

        let state = self.state.read(cx);
        let text = state.read(0..state.len());
        self.search.matches = text
            .match_indices(&query)
            .map(|(start, _)| start..start + query.len())
            .collect();
    }

    fn select_search_match_near_offset(
        &mut self,
        offset: usize,
        backwards: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.search_match_near_offset(offset, backwards) else {
            self.search.active_match = None;
            cx.notify();
            return;
        };

        self.select_search_match(index, cx);
    }

    fn search_match_near_offset(&self, offset: usize, backwards: bool) -> Option<usize> {
        if self.search.matches.is_empty() {
            return None;
        }

        Some(if backwards {
            self.search
                .matches
                .iter()
                .rposition(|span| span.start < offset)
                .unwrap_or(self.search.matches.len() - 1)
        } else {
            self.search
                .matches
                .iter()
                .position(|span| span.end > offset)
                .unwrap_or(0)
        })
    }

    fn select_search_match(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(span) = self.search.matches.get(index).cloned() else {
            self.search.active_match = None;
            cx.notify();
            return;
        };

        let cursor = {
            let state = self.state.read(cx);
            Cursor {
                anchor: state.offset8_to_loc8(span.start),
                head: state.offset8_to_loc8(span.end),
            }
        };
        self.search.active_match = Some(index);
        self.set_cursor(cursor, cx);
        self.discretely_scroll_to_cursor(cx);
        cx.notify();
    }

    pub(super) fn find_next_match(&mut self, cx: &mut Context<Self>) {
        if self.search.matches.is_empty() {
            self.rebuild_search_matches(cx);
        }
        if self.search.matches.is_empty() {
            cx.notify();
            return;
        }

        let index = self
            .search
            .active_match
            .map(|index| (index + 1) % self.search.matches.len())
            .unwrap_or_else(|| {
                let offset = self.state.read(cx).loc8_to_offset8(self.cursor(cx).head);
                self.search
                    .matches
                    .iter()
                    .position(|span| span.end > offset)
                    .unwrap_or(0)
            });
        self.select_search_match(index, cx);
    }

    fn find_previous_match(&mut self, cx: &mut Context<Self>) {
        if self.search.matches.is_empty() {
            self.rebuild_search_matches(cx);
        }
        if self.search.matches.is_empty() {
            cx.notify();
            return;
        }

        let index = self
            .search
            .active_match
            .map(|index| {
                if index == 0 {
                    self.search.matches.len() - 1
                } else {
                    index - 1
                }
            })
            .unwrap_or_else(|| {
                let offset = self.state.read(cx).loc8_to_offset8(self.cursor(cx).head);
                self.search
                    .matches
                    .iter()
                    .rposition(|span| span.start < offset)
                    .unwrap_or(self.search.matches.len() - 1)
            });
        self.select_search_match(index, cx);
    }

    pub(super) fn replace_current_match(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search.active_match.is_none() {
            self.rebuild_search_matches(cx);
            let offset = self.state.read(cx).loc8_to_offset8(self.cursor(cx).head);
            self.select_search_match_near_offset(offset, false, cx);
        }

        let Some(index) = self.search.active_match else {
            return;
        };
        let Some(span) = self.search.matches.get(index).cloned() else {
            return;
        };

        let query = self.query(cx);
        if query.is_empty() || self.state.read(cx).read(span.clone()) != query {
            self.rebuild_search_matches(cx);
            let offset = self.state.read(cx).loc8_to_offset8(self.cursor(cx).head);
            self.select_search_match_near_offset(offset, false, cx);
            return;
        }

        let replacement = self.replacement(cx);
        let next_offset = span.start + replacement.len();
        self.search.suppress_refresh = true;
        self.undo_group_boundary(cx);
        self.state.update(cx, |state, _| state.start_transaction());
        self.replace(span, &replacement, window, cx);
        self.state.update(cx, |state, cx| state.end_transaction(cx));
        self.undo_group_boundary(cx);
        self.search.suppress_refresh = false;

        self.rebuild_search_matches(cx);
        self.select_search_match_near_offset(next_offset, false, cx);
    }

    fn replace_all_matches(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.rebuild_search_matches(cx);
        if self.search.matches.is_empty() {
            cx.notify();
            return;
        }

        let replacement = self.replacement(cx);
        let matches = self.search.matches.clone();
        let first_offset = matches[0].start;

        self.search.suppress_refresh = true;
        self.undo_group_boundary(cx);
        self.state.update(cx, |state, _| state.start_transaction());
        for span in matches.into_iter().rev() {
            self.replace(span, &replacement, window, cx);
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
        self.undo_group_boundary(cx);
        self.search.suppress_refresh = false;

        self.rebuild_search_matches(cx);
        self.select_search_match_near_offset(first_offset, false, cx);
    }
}
