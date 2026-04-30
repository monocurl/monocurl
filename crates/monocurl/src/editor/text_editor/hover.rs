use super::*;

impl TextEditor {
    pub(super) fn stop_hover(&mut self) {
        self.hover_task = None;
        self.last_hover_start = None;
        self.hover_item = None;
        self.copied_hover_message = None;
    }

    fn character_mouse_is_on_top_of(&self, cx: &App) -> Option<Count8> {
        let mouse = self.last_in_frame_mouse_position?;
        let mut pos = self.index_for_mouse_position(mouse)?;
        pos.col = pos.col.saturating_sub(1);
        let state = self.state.read(cx);
        Some(state.loc8_to_offset8(pos))
    }

    // returns if state changed
    // if no hover should be present, stop it
    // if moved since last hover, start a timer for a new one
    // if not moved, do nothing
    pub(super) fn reset_hover_task_if_necessary(&mut self, cx: &mut Context<Self>) -> bool {
        let reset = |this: &mut Self| -> bool {
            let ret = this.hover_item.is_some();
            this.stop_hover();
            ret
        };
        let spawn_task = |this: &mut Self, cx: &mut Context<Self>| {
            reset(this);
            this.last_hover_start = None;
            this.reset_hover_task(cx);
            true
        };

        if self.is_selecting {
            return reset(self);
        }

        let Some(mouse) = self.last_in_frame_mouse_position else {
            return reset(self);
        };
        let scroll = self.scroll_handle.offset().y;

        if let Some((version, ref hover)) = self.hover_item {
            // only change if we move out of the hover item, or if version has changed
            let position_changed = self
                .character_mouse_is_on_top_of(cx)
                .is_none_or(|pos| !hover.span.contains(&pos));
            let version_changed = version != self.state.read(cx).version();
            if position_changed || version_changed {
                return spawn_task(self, cx);
            } else {
                return false;
            }
        } else {
            let version = self.state.read(cx).version();
            if self.last_hover_start.is_none()
                || (mouse, version, scroll) != self.last_hover_start.unwrap()
            {
                return spawn_task(self, cx);
            } else {
                false
            }
        }
    }

    fn reset_hover_task(&mut self, cx: &mut Context<Self>) {
        self.hover_task = Some(cx.spawn(async move |editor, app| {
            app.background_executor().timer(HOVER_MIN_DURATION).await;
            // if we have not been cancelled by this point, then we can assume this is valid
            let Some(editor) = editor.upgrade() else {
                return;
            };
            // show hover if directly on a position
            let Some(offset8) = app
                .read_entity(&editor, |e, cx| e.character_mouse_is_on_top_of(cx))
                .ok()
                .flatten()
            else {
                return;
            };

            app.update_entity(&editor, |editor, cx| {
                let diagnostic = editor
                    .state
                    .read(cx)
                    .diagnostics()
                    .diagnostic_for_point(offset8)
                    .cloned();
                editor.hover_item = diagnostic.map(|d| (editor.state.read(cx).version(), d));
                cx.notify();
            })
            .ok();
        }));
    }
}
