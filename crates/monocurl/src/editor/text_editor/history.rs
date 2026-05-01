use super::*;

struct HistoryItem {
    old: Span8,
    replacement: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HistoryGroupKind {
    User,
    ExternalReload,
}

pub(super) struct HistoryGroup {
    items: SmallVec<[HistoryItem; 8]>,
    // before the group was applied, where was the cursor?
    cursor: Cursor,
    kind: HistoryGroupKind,
}

impl TextEditor {
    fn perform_group(
        &mut self,
        group: HistoryGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> HistoryGroup {
        self.state.update(cx, |state, _| state.start_transaction());
        let mut inverse = HistoryGroup {
            items: SmallVec::new(),
            cursor: self.cursor(cx),
            kind: group.kind,
        };

        for item in group.items.iter().rev() {
            let old_text = self.state.read(cx).read(item.old.clone());
            self.replace(item.old.clone(), &item.replacement, window, cx);

            inverse.items.push(HistoryItem {
                old: Span8 {
                    start: item.old.start,
                    end: item.old.start + item.replacement.len(),
                },
                replacement: old_text,
            });
        }

        self.set_cursor(group.cursor, cx);
        self.discretely_scroll_to_cursor(cx);
        self.reset_cursor_blink(cx);

        self.state.update(cx, |state, cx| state.end_transaction(cx));

        inverse
    }

    pub(super) fn report_undo_candidate(&mut self, old: Span8, new_text: &str, cx: &App) {
        if self.history_disabled || self.is_redoing || self.is_undoing {
            return;
        }

        let must_form_isolated_group = new_text.contains('\n');
        if self.undo_stack.is_empty() || must_form_isolated_group {
            self.undo_stack.push_back(HistoryGroup {
                items: SmallVec::new(),
                cursor: self.cursor(cx),
                kind: HistoryGroupKind::User,
            });

            while self.undo_stack.len() > MAX_UNDO_GROUPS {
                self.undo_stack.pop_front();
            }
        }

        let replacement = self.state.read(cx).read(old.clone());
        let range = old.start..old.start + new_text.len();
        let group = self.undo_stack.back_mut().unwrap();
        if group.items.is_empty() {
            group.cursor = self.state.read(cx).cursor();
        }
        group.items.push(HistoryItem {
            old: range,
            replacement: replacement.to_string(),
        });

        if must_form_isolated_group {
            self.undo_group_boundary(cx);
        }

        self.redo_stack.clear();
    }

    pub fn perform_undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while self.undo_stack.back().is_some_and(|b| b.items.is_empty()) {
            self.undo_stack.pop_back();
        }

        let Some(group) = self.undo_stack.pop_back() else {
            return;
        };

        self.is_undoing = true;
        let redo = self.perform_group(group, window, cx);
        self.is_undoing = false;

        self.redo_stack.push_back(redo);

        self.undo_group_boundary(cx);
    }

    pub fn perform_redo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while self.redo_stack.back().is_some_and(|b| b.items.is_empty()) {
            self.redo_stack.pop_back();
        }

        let Some(group) = self.redo_stack.pop_back() else {
            return;
        };

        self.is_redoing = true;
        let undo = self.perform_group(group, window, cx);
        self.is_redoing = false;

        self.undo_stack.push_back(undo);
    }

    pub fn next_undo_requires_reload_confirmation(&self) -> bool {
        self.undo_stack
            .iter()
            .rev()
            .find(|group| !group.items.is_empty())
            .is_some_and(|group| group.kind == HistoryGroupKind::ExternalReload)
    }

    pub fn replace_entire_text_from_external_reload(
        &mut self,
        new_text: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_len = self.state.read(cx).len();
        let old_text = self.state.read(cx).read(0..old_len);
        if old_text == new_text {
            self.dirty.update(cx, |dirty, _| *dirty = false);
            self.save_dirty.update(cx, |dirty, _| *dirty = false);
            return;
        }

        while self
            .undo_stack
            .back()
            .is_some_and(|group| group.items.is_empty())
        {
            self.undo_stack.pop_back();
        }
        let mut items = SmallVec::new();
        items.push(HistoryItem {
            old: 0..new_text.len(),
            replacement: old_text,
        });
        self.undo_stack.push_back(HistoryGroup {
            items,
            cursor: self.cursor(cx),
            kind: HistoryGroupKind::ExternalReload,
        });
        while self.undo_stack.len() > MAX_UNDO_GROUPS {
            self.undo_stack.pop_front();
        }
        self.redo_stack.clear();

        self.history_disabled = true;
        self.state.update(cx, |state, _| state.start_transaction());
        self.replace(0..old_len, &new_text, window, cx);
        self.state.update(cx, |state, cx| state.end_transaction(cx));
        self.history_disabled = false;

        self.dirty.update(cx, |dirty, _| *dirty = false);
        self.save_dirty.update(cx, |dirty, _| *dirty = false);
    }

    pub(super) fn undo_group_boundary(&mut self, cx: &App) {
        if self.undo_stack.back().is_none_or(|g| !g.items.is_empty()) {
            self.undo_stack.push_back(HistoryGroup {
                items: SmallVec::new(),
                cursor: self.cursor(cx),
                kind: HistoryGroupKind::User,
            });
        }

        while self.undo_stack.len() > MAX_UNDO_GROUPS {
            self.undo_stack.pop_front();
        }
    }
}
