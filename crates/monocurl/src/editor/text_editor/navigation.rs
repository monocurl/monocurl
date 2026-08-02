use super::*;

impl TextEditor {
    pub(super) fn open_go_to_line(
        &mut self,
        _: &OpenGoToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let cursor = self.cursor(cx).head;
        self.go_to_line_visible = true;
        self.go_to_line_input.update(cx, |input, cx| {
            input.set_content(format!("{}:{}", cursor.row + 1, cursor.col + 1), cx);
            input.select_all(cx);
        });
        self.go_to_line_input.read(cx).focus(window);
        cx.notify();
    }

    pub(super) fn close_go_to_line(
        &mut self,
        _: &CloseGoToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to_line_visible = false;
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(super) fn confirm_go_to_line(
        &mut self,
        _: &ConfirmGoToLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.go_to_line_visible = false;
        self.focus_handle.focus(window);
        cx.notify();
    }

    pub(super) fn on_go_to_line_edited(&mut self, cx: &mut Context<Self>) {
        if !self.go_to_line_visible {
            return;
        }

        let query = self.go_to_line_input.read(cx).content();
        let Some(location) = parse_line_location(query) else {
            return;
        };
        self.move_to(location, false, false, cx);
    }

    pub(super) fn render_go_to_line_panel(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.go_to_line_visible {
            return None;
        }

        let theme = ThemeSettings::theme(cx);
        Some(
            div()
                .absolute()
                .top(px(8.0))
                .right(px(18.0))
                .w(px(260.0))
                .p(px(8.0))
                .flex()
                .items_center()
                .gap(px(8.0))
                .rounded(px(6.0))
                .border_1()
                .border_color(theme.navbar_border)
                .bg(theme.tab_active_background)
                .key_context("go-to-line")
                .on_mouse_down(MouseButton::Left, |_event, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .child(self.go_to_line_input.clone()),
                )
                .child(
                    div()
                        .id("go-to-line-close")
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
                        .child("Close")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.go_to_line_visible = false;
                            this.focus_handle.focus(window);
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }

    pub(super) fn next_diagnostic(
        &mut self,
        _: &NextDiagnostic,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_diagnostic(false, cx);
    }

    pub(super) fn previous_diagnostic(
        &mut self,
        _: &PreviousDiagnostic,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_diagnostic(true, cx);
    }

    fn jump_to_diagnostic(&mut self, backwards: bool, cx: &mut Context<Self>) {
        let location = (|| {
            let state = self.state.read(cx);
            let cursor = state.loc8_to_offset8(self.cursor(cx).head);
            let mut diagnostics = state.diagnostics().diagnostics_list().to_vec();
            diagnostics.sort_by_key(|diagnostic| diagnostic.span.start);
            let target = if backwards {
                diagnostics
                    .iter()
                    .rev()
                    .find(|diagnostic| diagnostic.span.start < cursor)
                    .or_else(|| diagnostics.last())
            } else {
                diagnostics
                    .iter()
                    .find(|diagnostic| diagnostic.span.start > cursor)
                    .or_else(|| diagnostics.first())
            }?;
            Some(state.offset8_to_loc8(target.span.start))
        })();
        if let Some(location) = location {
            self.move_to(location, false, true, cx);
        }
    }

    pub(super) fn next_slide_header(
        &mut self,
        _: &NextSlideHeader,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_slide_header(false, cx);
    }

    pub(super) fn previous_slide_header(
        &mut self,
        _: &PreviousSlideHeader,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_to_slide_header(true, cx);
    }

    fn jump_to_slide_header(&mut self, backwards: bool, cx: &mut Context<Self>) {
        let cursor_row = self.cursor(cx).head.row;
        let location = {
            let state = self.state.read(cx);
            let target = if backwards {
                state
                    .slides()
                    .iter()
                    .rev()
                    .find(|slide| slide.line < cursor_row)
            } else {
                state.slides().iter().find(|slide| slide.line > cursor_row)
            };
            target.map(|slide| Location8 {
                row: slide.line,
                col: 0,
            })
        };
        if let Some(location) = location {
            self.move_to(location, false, true, cx);
        }
    }

    pub(super) fn go_to_definition(
        &mut self,
        _: &GoToDefinition,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_row = self.cursor(cx).head.row;
        let location = {
            let state = self.state.read(cx);
            let cursor = state.loc8_to_offset8(self.cursor(cx).head);
            let word = state.word(cursor, false);
            if word.is_empty() {
                return;
            }
            let name = state.read(word);
            let source = state.read(0..state.len());
            let definitions = source
                .lines()
                .enumerate()
                .filter_map(|(row, line)| declaration_location(line, &name).map(|col| (row, col)))
                .collect::<Vec<_>>();
            definitions
                .iter()
                .rev()
                .find(|(row, _)| *row <= current_row)
                .or_else(|| definitions.first())
                .map(|(row, col)| Location8 {
                    row: *row,
                    col: *col,
                })
        };
        if let Some(location) = location {
            self.move_to(location, false, true, cx);
        }
    }
}

fn parse_line_location(query: &str) -> Option<Location8> {
    let (line, column) = query.trim().split_once(':').unwrap_or((query.trim(), "1"));
    let row = line.trim().parse::<usize>().ok()?.checked_sub(1)?;
    let col = column.trim().parse::<usize>().ok()?.checked_sub(1)?;
    Some(Location8 { row, col })
}

fn declaration_location(line: &str, name: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let indentation = line.len() - trimmed.len();
    for keyword in ["let", "var", "mesh", "anim", "param"] {
        let Some(after_keyword) = trimmed.strip_prefix(keyword) else {
            continue;
        };
        if !after_keyword.starts_with(char::is_whitespace) {
            continue;
        }
        let rest = after_keyword.trim_start();
        let identifier = rest
            .split(|character: char| !(character.is_alphanumeric() || character == '_'))
            .next()?;
        if identifier == name {
            return Some(indentation + keyword.len() + (after_keyword.len() - rest.len()));
        }
    }
    None
}
