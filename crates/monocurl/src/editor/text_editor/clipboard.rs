use super::*;

impl TextEditor {
    pub(super) fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(mut text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.undo_group_boundary(cx);
            text = text.replace("\t", &" ".repeat(TAB_SIZE));
            if text.chars().any(|c| c == '\n') {
                let state = self.state.read(cx);
                let cursor_loc = self.cursor(cx).head.min(self.cursor(cx).anchor);
                let line_start_offset = state.loc8_to_offset8(Location8 {
                    row: cursor_loc.row,
                    col: 0,
                });
                let line_start_text =
                    state.read(line_start_offset..state.loc8_to_offset8(cursor_loc));
                let cursor_indent = line_start_text.chars().take_while(|c| *c == ' ').count();

                // base indent = minimum leading spaces across non-empty lines
                let base_indent = text
                    .split('\n')
                    .filter(|l| !l.trim().is_empty())
                    .map(|l| l.chars().take_while(|&c| c == ' ').count())
                    .min()
                    .unwrap_or(0);

                let mut new_text = String::new();
                let mut first = true;
                for line in text.split('\n') {
                    if !first {
                        new_text.push('\n');
                    }
                    let line_indent = line.chars().take_while(|&c| c == ' ').count();
                    let relative = line_indent.saturating_sub(base_indent);
                    let pad = if first {
                        relative
                    } else {
                        cursor_indent + relative
                    };
                    first = false;
                    new_text.push_str(&" ".repeat(pad));
                    new_text.push_str(&line[line_indent.min(line.len())..]);
                }

                text = new_text;
            }
            self.replace_text_in_utf16_range(None, &text, false, window, cx);
            self.undo_group_boundary(cx);
        }
    }

    pub(super) fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor(cx).is_empty() {
            let state = self.state.read(cx);
            let range = state.cursor_range();
            cx.write_to_clipboard(ClipboardItem::new_string(state.read(range)));
        }
    }

    pub(super) fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor(cx).is_empty() {
            self.undo_group_boundary(cx);
            let state = self.state.read(cx);
            let range = state.cursor_range();
            cx.write_to_clipboard(ClipboardItem::new_string(state.read(range)));
            self.replace_text_in_utf16_range(None, "", false, window, cx);
            self.undo_group_boundary(cx);
        }
    }

    pub(super) fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }
}
