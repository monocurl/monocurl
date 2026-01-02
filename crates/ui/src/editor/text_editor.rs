use std::usize;
use std::{time::Duration};
use std::ops::Range;

use crate::editor::wrapped_line::WrappedLine;
use crate::editor::line_map::LineMap;
use crate::theme::{TextEditorStyles};
use crate::{editor::backing::{EditorBackend}};
use gpui::*;
use structs::text::{Location8, Span8};

use crate::actions::*;

const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const CURSOR_BLINK_DELAY: Duration = Duration::from_millis(500);
const TAB_SIZE: usize = 4;
const SCROLL_MARGIN: f32 = 4.0;
const AUTO_SCROLL_MAX_SPEED: f32 = 100.0;
const AUTO_SCROLL_INTERVAL: Duration = Duration::from_millis(16);
const AUTO_SCROLL_MIN_THRESHOLD: f32 = -15.0;
const AUTO_SCROLL_MAX_THRESHOLD: f32 = 70.0;
const BOTTOM_SCROLL_PADDING: f32 = 400.0;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("alt-backspace", BackspaceWord, None),
        KeyBinding::new("secondary-backspace", BackspaceLine, None),
        KeyBinding::new("delete shift-delete", Delete, None),
        KeyBinding::new("up", Up, None),
        KeyBinding::new("down", Down, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("enter", Enter, None),
        KeyBinding::new("tab", Tab, None),
        KeyBinding::new("shift-tab", Untab, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("shift-up", SelectUp, None),
        KeyBinding::new("shift-down", SelectDown, None),
        KeyBinding::new("secondary-a", SelectAll, None),
        KeyBinding::new("secondary-v", Paste, None),
        KeyBinding::new("secondary-c", Copy, None),
        KeyBinding::new("secondary-x", Cut, None),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        KeyBinding::new("ctrl-secondary-space", ShowCharacterPalette, None),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Cursor {
    anchor: Location8,
    head: Location8,
}

impl Cursor {
    fn collapsed(pos: Location8) -> Self {
        Self { anchor: pos, head: pos }
    }

    fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    fn line_range(&self) -> Range<usize> {
        let start_row = self.anchor.min(self.head).row as usize;
        let end_row = self.anchor.max(self.head).row as usize;
        start_row..end_row + 1
    }

    fn range(&self, backend: &impl EditorBackend) -> Span8 {
        let start = backend.loc8_to_offset8(self.anchor.min(self.head));
        let end = backend.loc8_to_offset8(self.anchor.max(self.head));
        start..end
    }

    fn reversed(&self) -> bool {
        self.head < self.anchor
    }
}

pub struct TextEditor<B: EditorBackend> {
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,

    pub state: Entity<B>,
    dirty: Entity<bool>,

    marked_range: Option<Span8>,
    is_selecting: bool,
    auto_scroll_epoch: usize,
    auto_scroll_last_mouse_position: Option<Point<Pixels>>,
    cursor: Cursor,
    cursor_blink_state: bool,
    cursor_blink_epoch: usize,

    last_click_position: Point<Pixels>,
    click_count: usize,

    text_styles: TextEditorStyles,
    line_map: LineMap,
    line_height: Pixels,
    gutter_width: Pixels,
    right_gutter_width: Pixels,

    last_bounds: Option<Bounds<Pixels>>,
    resize_anchor_line: Option<(usize, Pixels)>,
}

impl<B: EditorBackend> TextEditor<B> {
    pub fn new(state: Entity<B>, window: &mut Window, cx: &mut Context<Self>, content: String, dirty: Entity<bool>) -> Self {
        let text_styles = TextEditorStyles::default();
        let line_height = text_styles.line_height;

        let mut ret = TextEditor {
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            state,
            dirty,
            marked_range: None,
            is_selecting: false,
            auto_scroll_epoch: 0,
            auto_scroll_last_mouse_position: None,

            cursor: Cursor::collapsed(Location8 { row: 0, col: 0 }),
            cursor_blink_state: true,
            cursor_blink_epoch: 0,

            last_click_position: point(px(-1.0), px(0.0)),
            click_count: 0,
            text_styles: text_styles.clone(),
            line_map: LineMap::new(line_height),
            line_height,
            gutter_width: px(50.0),
            right_gutter_width: px(10.0),
            last_bounds: None,
            resize_anchor_line: None,
        };
        ret.replace(0..0, &content, window, cx);
        ret
    }
}

impl<B: EditorBackend> TextEditor<B> {
    fn replace(&mut self, utf8_range: Span8, new_text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (del_range, ins_range) = self.state.update(cx, |state, _| {
            let del_range = {
                let start_loc = state.offset8_to_loc8(utf8_range.start);
                let end_loc = state.offset8_to_loc8(utf8_range.end);
                start_loc.row .. end_loc.row + 1
            };
            state.replace(utf8_range.clone(), new_text);
            let ins_range = {
                let start_loc = state.offset8_to_loc8(utf8_range.start);
                let end_loc = state.offset8_to_loc8(utf8_range.start + new_text.len());
                start_loc.row .. end_loc.row + 1
            };

            (del_range, ins_range)
        });
        self.line_map.replace_lines(del_range, self.shape_lines(ins_range, window, cx).into_iter());
        self.dirty.update(cx, |dirty, _| *dirty = true);
    }

    fn capture_top_visible_line(&mut self) {
        let scroll_y = -self.scroll_handle.offset().y;
        let top_most = self.visible_lines().start;
        let y_range = self.line_map.y_range(top_most..top_most + 1);
        self.resize_anchor_line = Some((top_most, scroll_y - y_range.start));
    }

    fn restore_scroll_to_anchor_line(&mut self) {
        if let Some((anchor_line, offset)) = self.resize_anchor_line.take() {
            let target_y = self.line_map.y_range(anchor_line..anchor_line + 1).start +
                offset;
            let scroll_offset = self.scroll_handle.offset();
            self.scroll_handle.set_offset(point(scroll_offset.x, -target_y));
        }
    }
}

impl<B: EditorBackend> TextEditor<B> {
    fn reset_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_blink_state = true;
        self.cursor_blink_epoch += 1;
        cx.notify();

        let epoch = self.cursor_blink_epoch;
        cx.spawn(async move |editor: WeakEntity<TextEditor<B>>, cx: &mut AsyncApp| {
            cx.background_executor().timer(CURSOR_BLINK_DELAY).await;
            editor
                .update(cx, |editor, cx| {
                    if editor.cursor_blink_epoch == epoch {
                        editor.start_cursor_blinking(cx);
                    }
                })
                .ok();
        })
        .detach();
    }

    fn start_cursor_blinking(&mut self, cx: &mut Context<Self>) {
        let epoch = self.cursor_blink_epoch;

        cx.spawn(async move |editor: WeakEntity<TextEditor<B>>, cx: &mut AsyncApp| {
            loop {
                let should_continue = editor
                    .update(cx, |editor, cx| {
                        if editor.cursor_blink_epoch == epoch {
                            editor.cursor_blink_state = !editor.cursor_blink_state;
                            cx.notify();
                            true
                        } else {
                            false
                        }
                    })
                    .ok()
                    .unwrap_or(false);

                if !should_continue {
                    break;
                }

                cx.background_executor().timer(CURSOR_BLINK_INTERVAL).await;
            }
        })
        .detach();
    }

    fn move_to(&mut self, pos: Location8, mouse_origin: bool, cx: &mut Context<Self>) {
        self.cursor = Cursor::collapsed(pos);
        self.reset_cursor_blink(cx);
        if !mouse_origin {
            self.discretely_scroll_to_cursor();
        }
    }

    fn select_to(&mut self, pos: Location8, mouse_origin: bool, cx: &mut Context<Self>) {
        self.cursor.head = pos;
        self.reset_cursor_blink(cx);
        if !mouse_origin {
            self.discretely_scroll_to_cursor();
        }
    }

    fn discretely_scroll_to_cursor(&mut self) {
        let cursor_y = self.line_map.point_for_location(Location8 {
            row: self.cursor.head.row,
            col: self.cursor.head.col,
        }).y;

        let scroll_offset = self.scroll_handle.offset();
        let viewport_height = self.scroll_handle.bounds().size.height;

        let visible_top = -scroll_offset.y;
        let visible_bottom = visible_top + viewport_height;

        let margin_height = SCROLL_MARGIN * self.line_height;

        if cursor_y - margin_height < visible_top {
            let new_scroll_y = -(cursor_y - margin_height).max(px(0.0));
            self.scroll_handle.set_offset(point(scroll_offset.x, new_scroll_y));
        } else if cursor_y + self.line_height + margin_height > visible_bottom {
            let new_scroll_y = -(cursor_y + self.line_height - viewport_height + margin_height);
            self.scroll_handle.set_offset(point(scroll_offset.x, new_scroll_y));
        }
    }

    fn start_responding_to_mouse_movements(&mut self, cx: &mut Context<Self>) {
        self.auto_scroll_epoch += 1;
        let epoch = self.auto_scroll_epoch;

        cx.spawn(async move |editor: WeakEntity<TextEditor<B>>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(AUTO_SCROLL_INTERVAL).await;

                let should_continue = editor.update(cx, |editor, cx| {
                    if !editor.is_selecting || editor.auto_scroll_epoch != epoch {
                        return false;
                    }

                    if let Some(mouse_pos) = editor.auto_scroll_last_mouse_position {
                        let pos = editor.index_for_mouse_position(mouse_pos);
                        editor.select_to(pos, true, cx);

                        let scroll_bounds = editor.scroll_handle.bounds();
                        let viewport_top = scroll_bounds.top();
                        let viewport_bottom = scroll_bounds.bottom();

                        let distance_above = (viewport_top - mouse_pos.y - px(AUTO_SCROLL_MIN_THRESHOLD)).max(px(0.0));
                        let distance_below = (mouse_pos.y - viewport_bottom - px(AUTO_SCROLL_MIN_THRESHOLD)).max(px(0.0));

                        if distance_above > px(0.0) || distance_below > px(0.0) {
                            let scroll_offset = editor.scroll_handle.offset();
                            let distance = distance_above.max(distance_below);

                            let interpolate = |x: f64| x;
                            let t = (distance / (AUTO_SCROLL_MAX_THRESHOLD - AUTO_SCROLL_MIN_THRESHOLD)).min(px(1.0));
                            let scroll_speed = px(interpolate(t.to_f64()) as f32) * AUTO_SCROLL_MAX_SPEED;

                            let new_scroll_y = if distance_above > px(0.0) {
                                scroll_offset.y + scroll_speed
                            } else {
                                scroll_offset.y - scroll_speed
                            };

                            editor.scroll_handle.set_offset(point(scroll_offset.x, new_scroll_y));
                            cx.notify();
                        }
                    }

                    true
                }).ok().unwrap_or(false);

                if !should_continue {
                    break;
                }
            }
        }).detach();
    }

    fn stop_responding_to_mouse_movements(&mut self) {
        self.auto_scroll_epoch += 1;
    }
}

impl<B: EditorBackend> TextEditor<B> {

    fn line_text(&self, line: usize, cx: &App) -> String {
        let state = self.state.read(cx);
        let start_loc = Location8 { row: line, col: 0 };
        let start_offset = state.loc8_to_offset8(start_loc);

        let end_loc = Location8 { row: line + 1, col: 0 };
        let end_offset = state.loc8_to_offset8(end_loc).min(state.len());

        let mut text = state.read(start_offset..end_offset);
        if text.ends_with('\n') {
            text.pop();
        }
        text
    }

    fn shape_lines(&self, range: Range<usize>, window: &mut Window, cx: &App) -> Vec<WrappedLine> {
        let wrap_width =  if self.scroll_handle.bounds().size.width > self.gutter_width + self.right_gutter_width {
            self.scroll_handle.bounds().size.width - self.gutter_width - self.right_gutter_width
        } else if let Some(old_bounds) = self.last_bounds {
            old_bounds.size.width - self.gutter_width - self.right_gutter_width
        } else {
            Pixels::MAX
        };

        range.map(move |line_no| {
            let line_text = self.line_text(line_no, cx);
            let run = TextRun {
                len: line_text.len(),
                font: self.text_styles.text_font.clone(),
                color: self.text_styles.text_color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };

            WrappedLine::new(
                &line_text,
                self.text_styles.text_size,
                &[run],
                wrap_width,
                window
            )
        })
        .collect()
    }

    fn reshape_dirty_lines(&mut self, window: &mut Window, cx: &mut App) {
        let dirty = self.state.update(cx, |state, _cx| {
            state.take_dirty_region()
        });

        if let Some(dirty) = dirty {
            self.line_map.replace_lines(
                dirty.clone(),
                self.shape_lines(dirty, window, cx).into_iter()
            );
        }
    }

    fn visible_lines(&self) -> Range<usize> {
        let scroll_range = -self.scroll_handle.offset().y ..
            (-self.scroll_handle.offset().y + self.scroll_handle.bounds().size.height);
        self.line_map.prewrapped_visible_lines(scroll_range)
    }
}

impl<B: EditorBackend> TextEditor<B> {
    fn vertical_cursor_movement(&self, delta_lines: isize) -> Location8 {
        let current_pos = self.line_map.point_for_location(self.cursor.head);
        let target_y = current_pos.y + delta_lines as f32 * self.line_height;
        self.line_map.location_for_point(point(current_pos.x, target_y))
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical_cursor_movement(-1), false, cx);
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        self.move_to(self.vertical_cursor_movement(1), false, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !self.cursor.is_empty() {
            let range = self.cursor.range(state);
            self.move_to(state.offset8_to_loc8(range.start), false, cx);
        } else {
            let offset = state.loc8_to_offset8(self.cursor.head);
            let new_offset = state.prev_boundary(offset);
            self.move_to(state.offset8_to_loc8(new_offset), false, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        if !self.cursor.is_empty() {
            let range = self.cursor.range(state);
            self.move_to(state.offset8_to_loc8(range.end), false, cx);
        } else {
            let offset = state.loc8_to_offset8(self.cursor.head);
            let new_offset = state.next_boundary(offset);
            self.move_to(state.offset8_to_loc8(new_offset), false, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let offset = state.loc8_to_offset8(self.cursor.head);
        let new_offset = state.prev_boundary(offset);
        self.select_to(state.offset8_to_loc8(new_offset), false, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let offset = state.loc8_to_offset8(self.cursor.head);
        let new_offset = state.next_boundary(offset);
        self.select_to(state.offset8_to_loc8(new_offset), false, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical_cursor_movement(-1), false, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical_cursor_movement(1), false, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        self.cursor.anchor = Location8 { row: 0, col: 0 };
        self.cursor.head = state.offset8_to_loc8(state.len());
        self.discretely_scroll_to_cursor();
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let new_pos = Location8 { row: self.cursor.head.row, col: 0 };
        self.move_to(new_pos, false, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let row = self.cursor.head.row;
        let next_line = state.loc8_to_offset8(Location8 { row: row + 1, col: 0 });
        let line_end = next_line.saturating_sub(1).min(state.len());
        self.move_to(state.offset8_to_loc8(line_end), false, cx);
    }
}

impl<B: EditorBackend> TextEditor<B> {
    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor.is_empty() {
            let state = self.state.read(cx);
            let offset = state.loc8_to_offset8(self.cursor.head);

            let line_start = state.loc8_to_offset8(Location8 {
                row: self.cursor.head.row,
                col: 0
            });
            let text_before = state.read(line_start..offset);

            if text_before.chars().all(|c| c == ' ') && text_before.len() >= TAB_SIZE {
                let spaces_to_delete = if text_before.len() % TAB_SIZE == 0 {
                    TAB_SIZE
                } else {
                    text_before.len() % TAB_SIZE
                };
                let new_offset = offset.saturating_sub(spaces_to_delete);
                self.select_to(state.offset8_to_loc8(new_offset), false, cx);
            } else {
                let new_offset = state.prev_boundary(offset);
                self.select_to(state.offset8_to_loc8(new_offset), false, cx);
            }
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor.is_empty() {
            let state = self.state.read(cx);
            let offset = state.loc8_to_offset8(self.cursor.head);
            let new_offset = state.next_boundary(offset);
            self.select_to(state.offset8_to_loc8(new_offset), false, cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn backspace_word(&mut self, _: &BackspaceWord, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let mut selection = self.cursor.range(state);
        let word = state.word(selection.start, true);
        selection.start = word.start;
        let utf16 = state.span8_to_span16(&selection);
        self.replace_text_in_range(Some(utf16), "", window, cx);
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor.is_empty() {
            self.replace_text_in_range(None, &" ".repeat(TAB_SIZE), window, cx);
        } else {
            let start_loc = self.cursor.anchor.min(self.cursor.head);
            let end_loc = self.cursor.anchor.max(self.cursor.head);

            for row in start_loc.row..=end_loc.row {
                let line_start = self.state.read(cx).loc8_to_offset8(Location8 { row, col: 0 });
                self.replace(line_start..line_start, &" ".repeat(TAB_SIZE), window, cx);
            }

            self.cursor.anchor.col += TAB_SIZE;
            self.cursor.head.col += TAB_SIZE;
            self.reset_cursor_blink(cx);
        }
    }

    fn untab(&mut self, _: &Untab, window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor.is_empty() {
            let line_start = self.state.read(cx).loc8_to_offset8(Location8 {
                row: self.cursor.head.row,
                col: 0
            });
            let cursor_offset = self.state.read(cx).loc8_to_offset8(self.cursor.head);
            let text_before = self.state.read(cx).read(line_start..cursor_offset);

            let spaces_to_remove = text_before.chars()
                .rev()
                .take(TAB_SIZE)
                .take_while(|&c| c == ' ')
                .count();

            if spaces_to_remove > 0 {
                let remove_start = cursor_offset.saturating_sub(spaces_to_remove);
                self.replace(remove_start..cursor_offset, "", window, cx);
                self.cursor = Cursor::collapsed(self.state.read(cx).offset8_to_loc8(remove_start));
                self.dirty.update(cx, |dirty, _| *dirty = true);
                self.reset_cursor_blink(cx);
            }
        } else {
            let start_loc = self.cursor.anchor.min(self.cursor.head);
            let end_loc = self.cursor.anchor.max(self.cursor.head);

            for row in (start_loc.row..=end_loc.row).rev() {
                let state = self.state.read(cx);
                let line_start = state.loc8_to_offset8(Location8 { row, col: 0 });
                let line_end = state.loc8_to_offset8(Location8 { row: row + 1, col: 0 })
                    .min(state.len());
                let line_text = state.read(line_start..line_end);

                let spaces_to_remove = line_text.chars()
                    .take(TAB_SIZE)
                    .take_while(|&c| c == ' ')
                    .count();

                if spaces_to_remove > 0 {
                    self.replace(line_start..line_start + spaces_to_remove, "", window, cx);

                    if row == self.cursor.anchor.row {
                        self.cursor.anchor.col = self.cursor.anchor.col.saturating_sub(spaces_to_remove);
                    }
                    if row == self.cursor.head.row {
                        self.cursor.head.col = self.cursor.head.col.saturating_sub(spaces_to_remove);
                    }
                }
            }

            self.reset_cursor_blink(cx);
        }
    }

    fn backspace_line(&mut self, _: &BackspaceLine, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(Location8 { row: self.cursor.head.row, col: 0 }, false, cx);
        self.replace_text_in_range(None, "", window, cx);
    }
}

impl<B: EditorBackend> TextEditor<B> {
    fn index_for_mouse_position(&self, position: Point<Pixels>) -> Location8 {
        let Some(bounds) = self.last_bounds else {
            return Location8 { row: 0, col: 0 };
        };

        self.line_map.location_for_point(position - point(self.gutter_width, bounds.top()))
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window);

        if self.last_click_position == event.position {
            self.click_count += 1;
        } else {
            self.click_count = 1;
        }

        self.is_selecting = true;
        self.last_click_position = event.position;
        self.auto_scroll_last_mouse_position = Some(event.position);
        self.start_responding_to_mouse_movements(cx);

        let pos = self.index_for_mouse_position(event.position);
        match self.click_count {
            1 => {
                self.is_selecting = true;
                if event.modifiers.shift {
                    self.select_to(pos, true, cx);
                } else {
                    self.move_to(pos, true, cx);
                }
            }
            2 => {
                let state = self.state.read(cx);
                let offset = state.loc8_to_offset8(pos);
                let word_range = state.word(offset, false);

                self.cursor.anchor = state.offset8_to_loc8(word_range.start);
                self.cursor.head = state.offset8_to_loc8(word_range.end);
                self.is_selecting = true;
                self.reset_cursor_blink(cx);
            }
            _ => {
                let state = self.state.read(cx);
                let line_start = Location8 { row: pos.row, col: 0 };
                let line_end_offset = state.loc8_to_offset8(
                    Location8 { row: pos.row, col: usize::MAX }
                );
                let line_end_offset = line_end_offset.min(state.len());
                let line_end = state.offset8_to_loc8(line_end_offset);

                self.cursor.anchor = line_start;
                self.cursor.head = line_end;
                self.is_selecting = true;
                self.reset_cursor_blink(cx);
            }
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
        self.stop_responding_to_mouse_movements();
        self.auto_scroll_last_mouse_position = None;
    }

    fn on_mouse_move(&mut self, _event: &MouseMoveEvent, _window: &mut Window, _cx: &mut Context<Self>) {
        // mouse position tracking is mainly done in the listener registered in the paint
        // since we don't get mouse move events if the mouse is outside the view in this method
    }
}

impl<B: EditorBackend> TextEditor<B> {
    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor.is_empty() {
            let state = self.state.read(cx);
            let range = self.cursor.range(state);
            cx.write_to_clipboard(ClipboardItem::new_string(
                state.read(range),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor.is_empty() {
            let state = self.state.read(cx);
            let range = self.cursor.range(state);
            cx.write_to_clipboard(ClipboardItem::new_string(
                state.read(range),
            ));
            self.replace_text_in_range(None, "", window, cx);
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }
}

impl<B: EditorBackend> EntityInputHandler for TextEditor<B> {
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
        let range = self.cursor.range(state);
        Some(UTF16Selection {
            range: state.span8_to_span16(&range),
            reversed: self.cursor.reversed(),
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
        let range = range_utf16
            .as_ref()
            .map(|r| self.state.read(cx).span16_to_span8(r))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.cursor.range(self.state.read(cx)));

        self.replace(range.clone(), new_text, window, cx);

        let new_offset = range.start + new_text.len();
        self.cursor = Cursor::collapsed(self.state.read(cx).offset8_to_loc8(new_offset));
        self.discretely_scroll_to_cursor();

        self.marked_range = None;
        self.reset_cursor_blink(cx);
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
            .unwrap_or_else(|| self.cursor.range(self.state.read(cx)));

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
            self.cursor.anchor = state.offset8_to_loc8(adjusted_start);
            self.cursor.head = state.offset8_to_loc8(adjusted_end);
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
        let loc8 = self.index_for_mouse_position(point);
        let offset8 = state.loc8_to_offset8(loc8);
        Some(state.offset8_to_offset16(offset8) as usize)
    }
}

struct TextElement<B: EditorBackend> {
    editor: Entity<TextEditor<B>>,
}

struct PrepaintState {
    lines: Vec<(usize, WrappedLine)>,
    cursor_bounds: Option<Bounds<Pixels>>,
    selection_bounds: Vec<Bounds<Pixels>>,
    active_line_bounds: Option<Bounds<Pixels>>,
}

impl<B: EditorBackend> IntoElement for TextElement<B> {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl<B: EditorBackend> TextElement<B> {
    fn compute_cursor_bounds(&self, editor: &TextEditor<B>, bounds: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
        if !editor.cursor.is_empty() || !editor.cursor_blink_state {
            return None;
        }

        let Point { x, y } = editor.line_map.point_for_location(editor.cursor.head);
        Some(Bounds::new(
            point(bounds.left() + editor.gutter_width + x, bounds.top() + y),
            size(px(1.5), editor.line_height),
        ))
    }

    fn compute_active_line_bounds(&self, editor: &TextEditor<B>, bounds: Bounds<Pixels>, window: &Window) -> Option<Bounds<Pixels>> {
        if !editor.cursor.is_empty() || !editor.focus_handle.is_focused(window) {
            return None;
        }

        let line_num = editor.cursor.head.row as usize;
        let y_range = editor.line_map.y_range(line_num..line_num + 1);
        Some(Bounds::new(
            point(bounds.left(), bounds.top() + y_range.start),
            size(bounds.size.width, y_range.end - y_range.start),
        ))
    }

    fn compute_selection_bounds(&self, editor: &TextEditor<B>, bounds: Bounds<Pixels>, visible_lines: Range<usize>, window: &Window) -> Vec<Bounds<Pixels>> {
        if editor.cursor.is_empty() || !editor.focus_handle.is_focused(window) {
            return Vec::new();
        }

        let start_loc = editor.cursor.anchor.min(editor.cursor.head);
        let end_loc = editor.cursor.anchor.max(editor.cursor.head);

        let visible_selection = visible_lines.start.max(start_loc.row) ..
            visible_lines.end.min(end_loc.row + 1);
        let mut y = editor.line_map.y_range(0..visible_selection.start).end;

        editor.line_map
            .unwrapped_lines_iter(visible_selection.start)
            .take(visible_selection.len())
            .flat_map(|multi_line| {
                let line_num = multi_line.unwrapped_line_no;
                let line_start = if line_num == start_loc.row { start_loc.col } else { 0 };
                let line_end = if line_num == end_loc.row {
                    end_loc.col
                } else {
                    editor.line_map.line_len(line_num)
                };
                multi_line.line.iter()
                    .map(move |single_line| {
                        (line_start..line_end, single_line)
                    })
            })
            .filter_map(|(local_range, single_line)| {
                y += editor.line_height;
                let mut x_pixels = single_line.x_range(local_range)?;
                x_pixels.end = x_pixels.end.max(x_pixels.start + px(5.0));
                Some(Bounds::from_corners(
                    point(bounds.left() + editor.gutter_width + x_pixels.start, bounds.top() + y - editor.line_height),
                    point(bounds.left() + editor.gutter_width + x_pixels.end, bounds.top() + y),
                ))
            })
            .collect()
    }

    fn paint_gutter_line(&self, line_num: usize, y: Pixels, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let editor = self.editor.read(cx);
        let line_range = editor.cursor.line_range();
        let line_selected = line_range.contains(&line_num);
        let gutter_color = if line_selected {
            editor.text_styles.gutter_active_color
        } else {
            editor.text_styles.gutter_text_color
        };

        let line_number = format!("{}", line_num + 1);
        let gutter_run = TextRun {
            len: line_number.len(),
            font: editor.text_styles.gutter_font.clone(),
            color: gutter_color,
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let gutter_shaped = window.text_system().shape_line(
            line_number.into(),
            editor.text_styles.text_size,
            &[gutter_run],
            None,
        );
        let gutter_x = editor.gutter_width - gutter_shaped.width - px(10.0);
        gutter_shaped.paint(
            point(bounds.left() + gutter_x, bounds.top() + y),
            editor.line_height,
            window,
            cx,
        ).ok();
    }

    fn paint_text_line(&self, editor: &TextEditor<B>, shaped: &WrappedLine, y: Pixels, bounds: Bounds<Pixels>, window: &mut Window, cx: &App) {
        let line_origin = point(bounds.left() + editor.gutter_width, bounds.top() + y);
        shaped.paint(line_origin, editor.line_height, window, cx).ok();
    }

    fn handle_width_resize(&self, editor: &mut TextEditor<B>, bounds: Bounds<Pixels>, cx: &mut App) {
        if editor.last_bounds.is_none_or(|b| b.size.width != bounds.size.width) {
            editor.capture_top_visible_line();
            editor.state.update(cx, |state, _| {
                state.mark_region_as_dirty(0..editor.line_map.line_count());
            });
        }
        editor.last_bounds = Some(bounds);
    }
}

impl<B: EditorBackend> Element for TextElement<B> {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.editor.update(cx, |editor, cx| {
            editor.reshape_dirty_lines(window, cx);
            if editor.resize_anchor_line.is_some() {
                editor.restore_scroll_to_anchor_line();
            }
        });

        let editor = self.editor.read(cx);
        let total_height = editor.line_map.total_height();

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(f32::from(total_height) + BOTTOM_SCROLL_PADDING).into();

        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.editor.update(cx, |editor, cx| {
            self.handle_width_resize(editor, bounds, cx);
            // in case any new dirty lines due to resize, make a best effort of adapting to the new layout
            // it is possible (and likely) that now our bounds are slightly off from what they really should be, but this should not be that bad and will be
            // fixed by next frame (so should be relatively transparent to user)
            editor.reshape_dirty_lines(window, cx);

            let visible_lines = editor.visible_lines();

            let lines = editor.line_map.unwrapped_lines_iter(visible_lines.start)
                .take(visible_lines.len())
                .map(|line| (line.unwrapped_line_no, line.line.clone()))
                .collect();

            let cursor_bounds = self.compute_cursor_bounds(editor, bounds);
            let active_line_bounds = self.compute_active_line_bounds(editor, bounds, window);
            let selection_bounds = self.compute_selection_bounds(editor, bounds, visible_lines, window);

            PrepaintState {
                lines,
                cursor_bounds,
                selection_bounds,
                active_line_bounds,
            }
        })
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let editor = self.editor.read(cx);
        let focus_handle = editor.focus_handle.clone();
        let cursor_color = editor.text_styles.cursor_color;

        // handle input
        if editor.is_selecting {
            let editor = self.editor.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
                if phase == DispatchPhase::Bubble {
                    editor.update(cx, |editor, _| {
                        editor.auto_scroll_last_mouse_position = Some(event.position);
                    });
                }
            });
        }
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        if let Some(active_bounds) = prepaint.active_line_bounds {
            window.paint_quad(fill(active_bounds, editor.text_styles.active_line_color));
        }

        for sel_bounds in &prepaint.selection_bounds {
            window.paint_quad(fill(*sel_bounds, editor.text_styles.selection_color));
        }

        for (line_num, shaped) in &prepaint.lines {
            let editor = self.editor.read(cx);
            let y = editor.line_map.point_for_location(Location8 { row: *line_num, col: 0}).y;

            self.paint_text_line(editor, shaped, y, bounds, window, cx);
            self.paint_gutter_line(*line_num, y, bounds, window, cx);
        }

        if focus_handle.is_focused(window) {
            if let Some(cursor_bounds) = prepaint.cursor_bounds {
                window.paint_quad(fill(cursor_bounds, cursor_color));
            }
        }
    }
}

impl<B: EditorBackend> Focusable for TextEditor<B> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl<B: EditorBackend> Render for TextEditor<B> {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .key_context("editor")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::backspace_word))
            .on_action(cx.listener(Self::backspace_line))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::untab))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::show_character_palette))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .child(
                div()
                    .id("text-editor-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .cursor(CursorStyle::IBeam)
                    .bg(self.text_styles.bg_color)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_mouse_move(cx.listener(Self::on_mouse_move))
                    .child( TextElement { editor: cx.entity() } )
            )
    }
}
