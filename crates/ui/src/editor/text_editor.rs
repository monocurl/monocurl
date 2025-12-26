use std::{collections::HashMap, time::Duration};
use std::ops::Range;

use crate::{editor::backing::{TextBackend}};
use gpui::*;
use structs::text::{Location8, Span8};

use crate::actions::*;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("alt-backspace", BackspaceWord, None),
        KeyBinding::new("secondary-backspace", BackspaceLine, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("up", Up, None),
        KeyBinding::new("down", Down, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("enter", Enter, None),
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

// undo/redo operation storing complete backend state
struct Operation<B: TextBackend> {
    backend: B,
    cursor: Location8,
    anchor: Location8,
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

    fn range(&self, backend: &impl TextBackend) -> Span8 {
        let start = backend.loc8_to_offset8(self.anchor.min(self.head));
        let end = backend.loc8_to_offset8(self.anchor.max(self.head));
        start..end
    }

    fn reversed(&self) -> bool {
        self.head < self.anchor
    }
}

struct LineCache {
    lines: HashMap<usize, ShapedLine>,
    version: usize,
}

impl LineCache {
    fn new() -> Self {
        Self {
            lines: HashMap::new(),
            version: 0,
        }
    }

    fn invalidate(&mut self) {
        self.lines.clear();
        self.version += 1;
    }

    fn get(&self, line: usize) -> Option<&ShapedLine> {
        self.lines.get(&line)
    }

    fn insert(&mut self, line: usize, shaped: ShapedLine) {
        self.lines.insert(line, shaped);
    }
}

pub struct TextEditor<B: TextBackend> {
    pub focus_handle: FocusHandle,
    pub scroll_handle: ScrollHandle,

    pub backend: B,

    undo_stack: Vec<Operation<B>>,
    redo_stack: Vec<Operation<B>>,

    pub marked_range: Option<Span8>,
    pub is_selecting: bool,
    cursor: Cursor,
    cursor_blink_state: bool,
    cursor_blink_epoch: usize,
    cursor_blink_interval: Duration,
    cursor_blink_delay: Duration,

    last_click_position: Option<Location8>,
    click_count: usize,

    line_cache: LineCache,
    pub line_height: Pixels,
    pub gutter_width: Pixels,

    pub viewport_height: Pixels,
    pub last_bounds: Option<Bounds<Pixels>>,
}

impl<B: TextBackend> TextEditor<B> {
    pub fn new(cx: &mut Context<Self>) -> Self {
        TextEditor {
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            backend: B::default(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            marked_range: None,
            is_selecting: false,
            cursor: Cursor::collapsed(Location8 { row: 0, col: 0 }),

            cursor_blink_state: true,
            cursor_blink_epoch: 0,
            cursor_blink_interval: Duration::from_millis(500),
            cursor_blink_delay: Duration::from_millis(500),

            last_click_position: None,
            click_count: 0,

            line_cache: LineCache::new(),
            line_height: px(20.0),
            gutter_width: px(50.0),
            viewport_height: px(600.0),
            last_bounds: None,
        }
    }
}

impl<B: TextBackend> TextEditor<B> {
    fn reset_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_blink_state = true;
        self.cursor_blink_epoch += 1;
        cx.notify();

        let epoch = self.cursor_blink_epoch;
        let delay = self.cursor_blink_delay;
        cx.spawn(async move |editor: WeakEntity<TextEditor<B>>, cx: &mut AsyncApp| {
            cx.background_executor().timer(delay).await;
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
        let interval = self.cursor_blink_interval;

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

                cx.background_executor().timer(interval).await;
            }
        })
        .detach();
    }

    fn move_to(&mut self, pos: Location8, cx: &mut Context<Self>) {
        self.cursor = Cursor::collapsed(pos);
        self.reset_cursor_blink(cx);
    }

    fn select_to(&mut self, pos: Location8, cx: &mut Context<Self>) {
        self.cursor.head = pos;
        self.reset_cursor_blink(cx);
    }
}


impl<B: TextBackend> TextEditor<B> {
    fn line_count(&self) -> usize {
        let loc = self.backend.offset8_to_loc8(self.backend.len());
        (loc.row + 1) as usize
    }

    fn line_text(&self, line: usize) -> String {
        let start_loc = Location8 { row: line, col: 0 };
        let start_offset = self.backend.loc8_to_offset8(start_loc);

        let end_loc = Location8 { row: line + 1, col: 0 };
        let end_offset = self.backend.loc8_to_offset8(end_loc).min(self.backend.len());

        let mut text = self.backend.read(start_offset..end_offset);
        if text.ends_with('\n') {
            text.pop();
        }
        text
    }

    fn visible_lines(&self) -> Range<usize> {
        let scroll_offset = self.scroll_handle.offset();
        let start_line = (scroll_offset.y / self.line_height).floor() as usize;
        let visible_line_count = (self.viewport_height / self.line_height).ceil() as usize;
        let end_line = (start_line + visible_line_count + 1).min(self.line_count());
        start_line..end_line
    }

    fn shape_line(&mut self, line: usize, window: &mut Window) -> Option<ShapedLine> {
        if let Some(cached) = self.line_cache.get(line) {
            return Some(cached.clone());
        }

        if line >= self.line_count() {
            return None;
        }

        let text = self.line_text(line);
        let style = window.text_style();

        let run = TextRun {
            len: text.len(),
            font: style.font(),
            color: gpui::black(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };

        let font_size = style.font_size.to_pixels(window.rem_size());
        let shaped = window.text_system().shape_line(
            text.into(),
            font_size,
            &[run],
            None,
        );

        self.line_cache.insert(line, shaped.clone());
        Some(shaped)
    }
}


impl<B: TextBackend> TextEditor<B> {
    fn push_undo(&mut self) {
        let operation = Operation {
            backend: self.backend.clone(),
            cursor: self.cursor.head,
            anchor: self.cursor.anchor,
        };
        self.undo_stack.push(operation);
        self.redo_stack.clear();
    }
}

impl<B: TextBackend> TextEditor<B> {
    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.cursor.head;
        if current.row == 0 {
            self.move_to(Location8 { row: 0, col: 0 }, cx);
        } else {
            self.move_to(Location8 { row: current.row - 1, col: current.col }, cx);
        }
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.cursor.head;
        let new_pos = Location8 { row: current.row + 1, col: current.col };
        self.move_to(new_pos, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor.is_empty() {
            let range = self.cursor.range(&self.backend);
            self.move_to(self.backend.offset8_to_loc8(range.start), cx);
        } else {
            let offset = self.backend.loc8_to_offset8(self.cursor.head);
            let new_offset = self.backend.prev_boundary(offset);
            self.move_to(self.backend.offset8_to_loc8(new_offset), cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor.is_empty() {
            let range = self.cursor.range(&self.backend);
            self.move_to(self.backend.offset8_to_loc8(range.end), cx);
        } else {
            let offset = self.backend.loc8_to_offset8(self.cursor.head);
            let new_offset = self.backend.next_boundary(offset);
            self.move_to(self.backend.offset8_to_loc8(new_offset), cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.backend.loc8_to_offset8(self.cursor.head);
        let new_offset = self.backend.prev_boundary(offset);
        self.select_to(self.backend.offset8_to_loc8(new_offset), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let offset = self.backend.loc8_to_offset8(self.cursor.head);
        let new_offset = self.backend.next_boundary(offset);
        self.select_to(self.backend.offset8_to_loc8(new_offset), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.cursor.head;
        if current.row == 0 {
            self.select_to(Location8 { row: 0, col: 0 }, cx);
        } else {
            let new_pos = Location8 { row: current.row - 1, col: current.col };
            self.select_to(new_pos, cx);
        }
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let current = self.cursor.head;
        let new_pos = Location8 { row: current.row + 1, col: current.col };
        self.select_to(new_pos, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        self.cursor.anchor = Location8 { row: 0, col: 0 };
        self.cursor.head = self.backend.offset8_to_loc8(self.backend.len());
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let new_pos = Location8 { row: self.cursor.head.row, col: 0 };
        self.move_to(new_pos, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let row = self.cursor.head.row;
        let next_line = self.backend.loc8_to_offset8(Location8 { row: row + 1, col: 0 });
        let line_end = next_line.saturating_sub(1).min(self.backend.len());
        self.move_to(self.backend.offset8_to_loc8(line_end), cx);
    }
}

impl<B: TextBackend> TextEditor<B> {

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor.is_empty() {
            let offset = self.backend.loc8_to_offset8(self.cursor.head);
            let new_offset = self.backend.prev_boundary(offset);
            self.select_to(self.backend.offset8_to_loc8(new_offset), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        if self.cursor.is_empty() {
            let offset = self.backend.loc8_to_offset8(self.cursor.head);
            let new_offset = self.backend.next_boundary(offset);
            self.select_to(self.backend.offset8_to_loc8(new_offset), cx);
        }
        self.replace_text_in_range(None, "", window, cx);
    }

    fn backspace_word(&mut self, _: &BackspaceWord, window: &mut Window, cx: &mut Context<Self>) {
        let mut selection = self.cursor.range(&self.backend);
        let word = self.backend.word(selection.start, true);
        selection.start = word.start;
        self.replace_text_in_range(Some(selection), "", window, cx);
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_text_in_range(None, "\n", window, cx);
    }

    fn backspace_line(&mut self, _: &BackspaceLine, window: &mut Window, cx: &mut Context<Self>) {
        self.select_to(Location8 { row: self.cursor.head.row, col: 0 }, cx);
        self.replace_text_in_range(None, "", window, cx);
    }
}

impl<B: TextBackend> TextEditor<B> {
    fn index_for_mouse_position(&self, position: Point<Pixels>, _window: &Window) -> Location8 {
        let Some(bounds) = self.last_bounds else {
            return Location8 { row: 0, col: 0 };
        };

        // which line was clicked
        let scroll_offset = self.scroll_handle.offset();
        let y = position.y - bounds.top() + scroll_offset.y;
        let line = (y / self.line_height).floor() as usize;
        let line = line.min(self.line_count().saturating_sub(1));

        // find the shaped line
        if let Some(cached) = self.line_cache.get(line) {
            let x = position.x - bounds.left() - self.gutter_width;
            let col_offset = cached.closest_index_for_x(x);
            Location8 { row: line, col: col_offset }
        } else {
            Location8 { row: line, col: 0 }
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window);

        self.is_selecting = true;
        let pos = self.index_for_mouse_position(event.position, window);

        if event.modifiers.shift {
            self.select_to(pos, cx);
        } else {
            self.move_to(pos, cx);
        }

        let pos = self.index_for_mouse_position(event.position, window);

        // check if double-tap/triple tap
        let is_multi_click = self.last_click_position == Some(pos);

        if is_multi_click {
            self.click_count += 1;
        } else {
            self.click_count = 1;
        }

        self.last_click_position = Some(pos);

        match self.click_count {
            1 => {
                self.is_selecting = true;
                if event.modifiers.shift {
                    self.select_to(pos, cx);
                } else {
                    self.move_to(pos, cx);
                }
            }
            2 => {
                let offset = self.backend.loc8_to_offset8(pos);
                let word_range = self.backend.word(offset, false);

                self.cursor.anchor = self.backend.offset8_to_loc8(word_range.start);
                self.cursor.head = self.backend.offset8_to_loc8(word_range.end);
                self.is_selecting = true;
                self.reset_cursor_blink(cx);
            }
            _ => {
                let line_start = Location8 { row: pos.row, col: 0 };
                let line_end_offset = self.backend.loc8_to_offset8(
                    Location8 { row: pos.row + 1, col: 0 }
                );
                let line_end_offset = line_end_offset.min(self.backend.len());
                let line_end = self.backend.offset8_to_loc8(line_end_offset);

                self.cursor.anchor = line_start;
                self.cursor.head = line_end;
                self.is_selecting = true;
                self.reset_cursor_blink(cx);
            }
        }
    }

    fn on_mouse_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_selecting {
            let pos = self.index_for_mouse_position(event.position, window);
            self.select_to(pos, cx);

            // TODO: Auto-scroll when mouse is above/below viewport
        }
    }

    // clipboard operations
    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }
}

impl<B: TextBackend> TextEditor<B> {
    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor.is_empty() {
            let range = self.cursor.range(&self.backend);
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.backend.read(range),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor.is_empty() {
            let range = self.cursor.range(&self.backend);
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.backend.read(range),
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

// EntityInputHandler implementation for OS text input
impl<B: TextBackend> EntityInputHandler for TextEditor<B> {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.backend.span16_to_span8(&range_utf16);
        actual_range.replace(self.backend.span8_to_span16(&range));
        Some(self.backend.read(range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let range = self.cursor.range(&self.backend);
        Some(UTF16Selection {
            range: self.backend.span8_to_span16(&range),
            reversed: self.cursor.reversed(),
        })
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        self.marked_range
            .as_ref()
            .map(|range| self.backend.span8_to_span16(range))
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        self.marked_range = None;
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.push_undo();

        let range = range_utf16
            .as_ref()
            .map(|r| self.backend.span16_to_span8(r))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.cursor.range(&self.backend));

        self.backend = self.backend.replace(range.clone(), new_text);

        let new_offset = range.start + new_text.len();
        self.cursor = Cursor::collapsed(self.backend.offset8_to_loc8(new_offset));

        self.marked_range = None;
        self.line_cache.invalidate();
        self.reset_cursor_blink(cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range_utf16
            .as_ref()
            .map(|r| self.backend.span16_to_span8(r))
            .or(self.marked_range.clone())
            .unwrap_or_else(|| self.cursor.range(&self.backend));

        self.backend = self.backend.replace(range.clone(), new_text);

        if !new_text.is_empty() {
            self.marked_range = Some(range.start..range.start + new_text.len());
        } else {
            self.marked_range = None;
        }

        if let Some(new_range_utf16) = new_selected_range_utf16 {
            let new_range = self.backend.span16_to_span8(&new_range_utf16);
            let adjusted_start = range.start + new_range.start;
            let adjusted_end = range.start + new_range.end;
            self.cursor.anchor = self.backend.offset8_to_loc8(adjusted_start);
            self.cursor.head = self.backend.offset8_to_loc8(adjusted_end);
        }

        self.line_cache.invalidate();
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.backend.span16_to_span8(&range_utf16);
        let start_loc = self.backend.offset8_to_loc8(range.start);
        let end_loc = self.backend.offset8_to_loc8(range.end);

        let shaped = self.line_cache.get(start_loc.row as usize)?;

        let line_y = start_loc.row as f32 * self.line_height.0;
        let scroll_offset = self.scroll_handle.offset();

        Some(Bounds::from_corners(
            point(
                bounds.left() + self.gutter_width + shaped.x_for_index(start_loc.col as usize),
                bounds.top() + px(line_y) - scroll_offset.y,
            ),
            point(
                bounds.left() + self.gutter_width + shaped.x_for_index(end_loc.col as usize),
                bounds.top() + px(line_y) + self.line_height - scroll_offset.y,
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        let loc8 = self.index_for_mouse_position(point, window);
        let offset8 = self.backend.loc8_to_offset8(loc8);
        Some(self.backend.offset8_to_offset16(offset8) as usize)
    }
}

struct TextElement<B: TextBackend> {
    editor: Entity<TextEditor<B>>,
}

struct PrepaintState {
    lines: Vec<(usize, ShapedLine)>,  // (line_number, content)
    cursor_bounds: Option<Bounds<Pixels>>,
    selection_bounds: Vec<Bounds<Pixels>>,
    total_height: Pixels,
}

impl<B: TextBackend> IntoElement for TextElement<B> {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl<B: TextBackend> Element for TextElement<B> {
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
        let editor = self.editor.read(cx);
        let total_height = editor.line_count() as f32 * editor.line_height.0;

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(total_height).into();

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
        self.editor.update(cx, |editor, _cx| {
            editor.viewport_height = bounds.size.height;

            let visible_lines = editor.visible_lines();
            let mut lines = Vec::new();

            // shape visible lines
            for line_num in visible_lines.clone() {
                if let Some(shaped) = editor.shape_line(line_num, window) {
                    lines.push((line_num, shaped));
                }
            }

            let scroll_offset = editor.scroll_handle.offset();
            let line_height = editor.line_height;
            let gutter_width = editor.gutter_width;

            // calculate cursor bounds - only if visible and blink state is true
            let cursor_bounds = if editor.cursor.is_empty() && editor.cursor_blink_state {
                let line_num = editor.cursor.head.row as usize;
                if let Some(shaped) = editor.line_cache.get(line_num) {
                    let x = shaped.x_for_index(editor.cursor.head.col as usize);
                    let y = px(line_num as f32 * line_height.0) - scroll_offset.y;
                    Some(Bounds::new(
                        point(bounds.left() + gutter_width + x, bounds.top() + y),
                        size(px(1.5), line_height),
                    ))
                } else {
                    None
                }
            } else {
                None
            };

            // calculate selection bounds (unchanged)
            let mut selection_bounds = Vec::new();
            if !editor.cursor.is_empty() {
                let start_loc = editor.cursor.anchor.min(editor.cursor.head);
                let end_loc = editor.cursor.anchor.max(editor.cursor.head);

                for line_num in start_loc.row..=end_loc.row {
                    if let Some(shaped) = editor.line_cache.get(line_num as usize) {
                        let line_start = if line_num == start_loc.row { start_loc.col } else { 0 };
                        let line_end = if line_num == end_loc.row {
                            end_loc.col
                        } else {
                            shaped.len
                        };

                        let x1 = shaped.x_for_index(line_start as usize);
                        let x2 = shaped.x_for_index(line_end as usize).max(x1 + px(5.0));
                        let y = px(line_num as f32 * line_height.0) - scroll_offset.y;

                        selection_bounds.push(Bounds::from_corners(
                            point(bounds.left() + gutter_width + x1, bounds.top() + y),
                            point(bounds.left() + gutter_width + x2, bounds.top() + y + line_height),
                        ));
                    }
                }
            }

            let total_height = px(editor.line_count() as f32 * line_height.0);

            PrepaintState {
                lines,
                cursor_bounds,
                selection_bounds,
                total_height,
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
        let scroll_offset = editor.scroll_handle.offset();
        let line_height = editor.line_height;
        let gutter_width = editor.gutter_width;

        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );

        // selection
        for sel_bounds in &prepaint.selection_bounds {
            window.paint_quad(fill(*sel_bounds, rgba(0x3311ff30)));
        }

        // gutter and text
        for (line_num, shaped) in &prepaint.lines {
            let y = px(*line_num as f32 * line_height.0) - scroll_offset.y;
            let line_origin = point(bounds.left() + gutter_width, bounds.top() + y);

            // draw line number in gutter
            let line_number = format!("{}", line_num + 1);
            let gutter_run = TextRun {
                len: line_number.len(),
                font: window.text_style().font(),
                color: gpui::red(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let gutter_shaped = window.text_system().shape_line(
                line_number.into(),
                px(14.0),
                &[gutter_run],
                None,
            );
            let gutter_x = gutter_width - gutter_shaped.width - px(10.0);
            gutter_shaped.paint(
                point(bounds.left() + gutter_x, bounds.top() + y),
                line_height,
                window,
                cx,
            ).ok();

            // main text
            shaped.paint(line_origin, line_height, window, cx).ok();
        }

        // draw cursor
        if focus_handle.is_focused(window) {
            if let Some(cursor_bounds) = prepaint.cursor_bounds {
                window.paint_quad(fill(cursor_bounds, gpui::blue()));
            }
        }

        self.editor.update(cx, |editor, _| {
            editor.last_bounds = Some(bounds);
        });
    }
}

impl<B: TextBackend> Focusable for TextEditor<B> {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl<B: TextBackend> Render for TextEditor<B> {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let total_height = self.line_count() as f32 * self.line_height.0;

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
                    .flex()
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .cursor(CursorStyle::IBeam)
                    .bg(white())
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_mouse_move(cx.listener(Self::on_mouse_move))
                    .child(
                        div()
                            .h(px(total_height))
                            .w_full()
                            .child( TextElement { editor: cx.entity() })
                    )
            )
    }
}
