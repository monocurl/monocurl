use std::collections::VecDeque;
use std::usize;
use std::{time::Duration};
use std::ops::Range;

use crate::editor::text_editor::popover_element::PopoverElement;
use crate::state::diagnostics::Diagnostic;
use crate::state::textual_state::{AutoCompleteState, Cursor, TextualState};
use crate::editor::line_shaper::LineShaper;
use crate::editor::wrapped_line::WrappedLine;
use crate::editor::line_map::LineMap;
use crate::editor::text_editor::text_element::TextElement;
use crate::theme::{TextEditorStyles};
use gpui::*;
use smallvec::SmallVec;
use structs::text::{Count8, Location8, Span8, Span16};

use crate::actions::*;

const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const CURSOR_BLINK_DELAY: Duration = Duration::from_millis(500);
const HOVER_MIN_DURATION: Duration = Duration::from_millis(250);
const MULTI_CLICK_TOLERANCE: Pixels = px(2.0);
const TAB_SIZE: usize = 4;
const SCROLL_MARGIN: f32 = 4.0;
const AUTO_SCROLL_MAX_SPEED: f32 = 100.0;
const AUTO_SCROLL_INTERVAL: Duration = Duration::from_millis(8);
const AUTO_SCROLL_MIN_THRESHOLD: f32 = -15.0;
const AUTO_SCROLL_MAX_THRESHOLD: f32 = 70.0;
const BOTTOM_SCROLL_PADDING: f32 = 400.0;
const MAX_UNDO_GROUPS: usize = 4096;

mod text_element;
mod popover_element;

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

fn point_dist(p: Point<Pixels>) -> Pixels {
    let hypot = (p.x.to_f64() * p.x.to_f64() + p.y.to_f64() * p.y.to_f64()).sqrt();
    px(hypot as f32)
}

struct HistoryItem {
    old: Span8,
    replacement: String,
}

struct HistoryGroup {
    items: SmallVec<[HistoryItem; 8]>,
    // before the group was applied, where was the cursor?
    cursor: Cursor,
}

pub struct TextEditor {
    focus_handle: FocusHandle,
    _focus_out_subscription: Subscription,
    _window_focus_subscription: Subscription,
    scroll_handle: ScrollHandle,

    history_disabled: bool,
    is_undoing: bool,
    is_redoing: bool,
    undo_stack: VecDeque<HistoryGroup>,
    redo_stack: VecDeque<HistoryGroup>,

    state: Entity<TextualState>,
    dirty: Entity<bool>,
    internal_dirty: Entity<bool>,

    last_op_matched_character: Option<Count8>,

    marked_range: Option<Span8>,
    is_selecting: bool,
    auto_scroll_task: Option<Task<()>>,
    auto_scroll_last_mouse_position: Option<Point<Pixels>>,
    cursor_blink_state: bool,
    cursor_blink_task: Option<Task<()>>,

    last_in_frame_mouse_position: Option<Point<Pixels>>,
    last_hover_start: Option<(Point<Pixels>, usize, Pixels)>,
    hover_task: Option<Task<()>>,
    // version, diagnostic
    hover_item: Option<(usize, Diagnostic)>,

    parameter_hint_suppression_task: Option<Task<()>>,
    parameter_hint_suppressed: bool,
    parameter_hint_allowed_base: Option<Location8>,

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

impl TextEditor {
    pub fn new(state: Entity<TextualState>, window: &mut Window, cx: &mut Context<Self>, content: String, dirty: Entity<bool>, internal_dirty: Entity<bool>) -> Self {
        let text_styles = TextEditorStyles::default();
        let line_height = text_styles.line_height;

        // re render whenever state changes
        // (mainly want the rerender when theres external changes to the state)
        cx.observe(&state, |_me, _, cx| {
            cx.notify();
        }).detach();

        let window_focus_subscription = cx.observe_window_activation(window, |e, window, cx| {
            if window.is_window_active() && e.focus_handle.is_focused(window) {
                e.reset_cursor_blink(cx);
            } else {
                // stop blinking if we are
                e.stop_hover();
                e.cursor_blink_task = None;
                e.cursor_blink_state = true;
            }
            cx.notify()
        });

        let focus_out_subscription = cx.on_focus_lost(window, |editor, _window, cx| {
            editor.reset_hover_task_if_necessary(cx);
            editor.cursor_blink_task = None;
            editor.cursor_blink_state = false;
            cx.notify()
        });

        let mut ret = TextEditor {
            focus_handle: cx.focus_handle(),
            _focus_out_subscription: focus_out_subscription,
            _window_focus_subscription: window_focus_subscription,
            scroll_handle: ScrollHandle::new(),
            history_disabled: false,
            is_undoing: false,
            is_redoing: false,
            undo_stack: VecDeque::default(),
            redo_stack: VecDeque::default(),
            state,

            dirty,
            internal_dirty,
            last_op_matched_character: None,

            marked_range: None,
            is_selecting: false,
            auto_scroll_task: None,
            auto_scroll_last_mouse_position: None,
            cursor_blink_state: true,

            cursor_blink_task: None,

            last_in_frame_mouse_position: None,
            last_hover_start: None,
            hover_task: None,

            hover_item: None,
            parameter_hint_suppression_task: None,
            parameter_hint_suppressed: false,

            parameter_hint_allowed_base: None,
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
        {
            ret.history_disabled = true;
            ret.state.update(cx, |state, _| state.start_transaction());
            ret.replace(0..0, &content, window, cx);
            ret.state.update(cx, |state, cx| state.end_transaction(cx));
            ret.history_disabled = false;
        }
        ret
    }
}

impl TextEditor {
    fn perform_group(&mut self, group: HistoryGroup, window: &mut Window, cx: &mut Context<Self>) -> HistoryGroup {
        self.state.update(cx, |state, _| state.start_transaction());
        let mut inverse = HistoryGroup { items: SmallVec::new(), cursor: self.cursor(cx) };

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

    fn report_undo_candidate(&mut self, old: Span8, new_text: &str, cx: &App) {
        if self.history_disabled || self.is_redoing || self.is_undoing {
            return;
        }

        let must_form_isolated_group = new_text.contains('\n');
        if self.undo_stack.is_empty() || must_form_isolated_group {
            self.undo_stack.push_back(HistoryGroup { items: SmallVec::new(), cursor: self.cursor(cx)});

            while self.undo_stack.len() > MAX_UNDO_GROUPS {
                self.undo_stack.pop_front();
            }
        }

        let replacement = self.state.read(cx).read(old.clone());
        let range = old.start .. old.start + new_text.len();
        let group = self.undo_stack.back_mut().unwrap();
        if group.items.is_empty() {
            group.cursor = self.state.read(cx).cursor();
        }
        group.items.push(HistoryItem { old: range, replacement: replacement.to_string() });

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

    fn undo_group_boundary(&mut self, cx: &App) {
        if self.undo_stack.back().is_none_or(|g| !g.items.is_empty()) {
            self.undo_stack.push_back(HistoryGroup { items: SmallVec::new(), cursor: self.cursor(cx)});
        }

        while self.undo_stack.len() > MAX_UNDO_GROUPS {
            self.undo_stack.pop_front();
        }
    }

    pub fn replace(&mut self, utf8_range: Span8, new_text: &str, window: &mut Window, cx: &mut App) {
        self.report_undo_candidate(utf8_range.clone(), new_text, cx);

        let (del_range, ins_range) = self.state.update(cx, |state, subcx| {
            let ret = state.replace(utf8_range.clone(), new_text, subcx);
            subcx.notify();
            ret
        });
        self.reshape_lines(del_range, ins_range, window, cx);
        self.dirty.update(cx, |dirty, _| *dirty = true);
        self.internal_dirty.update(cx, |dirty, _| *dirty = true);
    }

    // 0. if not inserting a single parenthesis, do normal
    // 1. if in string, do normal
    // 2. if inserting closing parenthesis and next character is not closing, do normal
    // 3. if inserting closing parenthesis and next character is closing, skip insertion
    // 4. if inserting opening parenthesis, insert matching closing parenthesis after
    fn match_parenthesis(&mut self, del: Span16, new_text: &str, cx: &App) -> Option<(Span8, String)> {
        fn in_literal(s: &str) -> bool {
            let escape = '%';
            let mut in_string = false;
            let mut prev_was_escape = false;
            for ch in s.chars() {
                if ch == escape {
                    prev_was_escape = true;
                    continue;
                }

                if ch == '"' && !prev_was_escape  {
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
                }
                else {
                    return None;
                }
            };

            match ch {
                '(' | '{' | '[' | '"' | '|' => {
                    let state = self.state.read(cx);
                    let line = state.offset8_to_loc8(del.start);
                    let start_of_line = state.loc8_to_offset8(Location8 { row: line.row, col: 0 });
                    let line_content = self.state.read(cx).read(start_of_line..del.start);
                    if in_literal(&line_content) {
                        if ch == '"' || ch == '\'' {
                            return handle_closing();
                        }
                        return None;
                    }
                    else if ch == '|'  && in_lambda_definition(&line_content) {
                        return handle_closing();
                    }
                    return Some((del, format!("{}{}", ch, match ch {
                        '(' => ')',
                        '{' => '}',
                        '[' => ']',
                        '"' => '"',
                        '|' => '|',
                        _ => unreachable!(),
                    })));
                },
                ')' | '}' | ']' => {
                    return handle_closing();
                },
                _ => return None,
            }
        }
        else if del.len() == 1 && new_text.len() == 0 {
            // does this undo the last matched insertion?
            if Some(del.end) == self.last_op_matched_character {
                let state = self.state.read(cx);
                if del.end < state.len() {
                    let prev = state.read(del.end.saturating_sub(1)..del.end);
                    let next = state.read(del.end..del.end+ 1);
                    let matching = match prev.chars().next() {
                        Some('(') => ')',
                        Some('{') => '}',
                        Some('[') => ']',
                        Some('"') => '"',
                        Some('|') => '|',
                        _ => return None,
                    };
                    if next.chars().next() == Some(matching) {
                        return Some((Span8 { start: del.start, end: del.end + 1 }, String::new()));
                    }
                }
            }
            return None
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

        if raw_keystroke && let Some((range, matched)) = self.match_parenthesis(range.clone(), new_text, cx) {
            if matched.len() == 2 {
                self.last_op_matched_character = Some(range.start + 1);
            } else {
                self.last_op_matched_character = None;
            }
            self.replace(range, &matched, window, cx);
        }
        else {
            self.last_op_matched_character = None;
            self.replace(range.clone(), new_text, window, cx);
        }

        let new_offset = range.start + new_text.len();
        self.move_to(self.state.read(cx).offset8_to_loc8(new_offset), false, false, cx);
        self.discretely_scroll_to_cursor(cx);

        self.stop_hover();

        self.marked_range = None;
        self.reset_cursor_blink(cx);
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }
}

impl TextEditor {
    fn stop_hover(&mut self) {
        self.hover_task = None;
        self.last_hover_start = None;
        self.hover_item = None;
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
    fn reset_hover_task_if_necessary(&mut self, cx: &mut Context<Self>) -> bool {
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
            let position_changed = self.character_mouse_is_on_top_of(cx)
                .is_none_or(|pos| !hover.span.contains(&pos));
            let version_changed = version != self.state.read(cx).version();
            if position_changed || version_changed {
                return spawn_task(self, cx);
            }
            else {
                return false;
            }
        }
        else {
            let version = self.state.read(cx).version();
            if self.last_hover_start.is_none() || (mouse, version, scroll) != self.last_hover_start.unwrap() {
                return spawn_task(self, cx);
            }
            else {
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
            let Some(offset8) = app.read_entity(&editor, |e, cx| e.character_mouse_is_on_top_of(cx)).ok().flatten() else {
                return;
            };

            app.update_entity(&editor, |editor, cx| {
                let diagnostic = editor.state.read(cx).diagnostics().diagnostic_for_point(offset8).cloned();
                editor.hover_item = diagnostic.map(|d| (editor.state.read(cx).version(), d));
                cx.notify();
            }).ok();
        }));
    }
}

impl TextEditor {
    fn cursor(&self, cx: &App) -> Cursor {
        self.state.read(cx).cursor()
    }

    fn set_cursor(&self, cursor: Cursor, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            state.start_transaction();
            state.set_cursor(cursor, cx);
            state.end_transaction(cx);
        });
    }

    pub fn reset_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_blink_state = true;
        cx.notify();

        let task = cx.spawn(async move |editor: WeakEntity<TextEditor>, cx: &mut AsyncApp| {
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
        });
        // cancels any previous tasks as well
        self.cursor_blink_task = Some(task);
    }

    fn move_to(&mut self, pos: Location8, mouse_origin: bool, key_origin: bool, cx: &mut Context<Self>) {
        self.set_cursor(Cursor::collapsed(pos), cx);
        self.reset_cursor_blink(cx);
        if !mouse_origin {
            self.discretely_scroll_to_cursor(cx);
        }

        if key_origin || mouse_origin {
            self.undo_group_boundary(cx);
        }
    }

    fn select_to(&mut self, pos: Location8, mouse_origin: bool, key_origin: bool, cx: &mut Context<Self>) {
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
}

impl TextEditor {
    pub(super) fn capture_top_visible_line(&mut self) {
        let scroll_y = -self.scroll_handle.offset().y;
        let top_most = self.visible_lines().start;
        let y_range = self.line_map.y_range(top_most..top_most + 1);
        self.resize_anchor_line = Some((top_most, scroll_y - y_range.start));
    }

    pub(super) fn restore_scroll_to_anchor_line(&mut self) {
        if let Some((anchor_line, offset)) = self.resize_anchor_line.take() {
            let target_y = self.line_map.y_range(anchor_line..anchor_line + 1).start +
                offset;
            let scroll_offset = self.scroll_handle.offset();
            self.scroll_handle.set_offset(point(scroll_offset.x, -target_y));
        }
    }

    fn discretely_scroll_to_cursor(&mut self, cx: &App) {
        let cursor = self.state.read(cx).cursor();
        let cursor_y = self.line_map.point_for_location(cursor.head).y;

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
        let task = cx.spawn(async move |editor: WeakEntity<TextEditor>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(AUTO_SCROLL_INTERVAL).await;

                let should_continue = editor.update(cx, |editor, cx| {
                    if !editor.is_selecting {
                        return false;
                    }

                    if let Some(mouse_pos) = editor.auto_scroll_last_mouse_position {
                        // if no motion, don't falsely select to this point since it could just be a Float click
                        let delta = mouse_pos - editor.last_click_position;
                        let dist = point_dist(delta);
                        if dist < MULTI_CLICK_TOLERANCE {
                            return true;
                        }

                        let pos = editor.closest_index_for_mouse_position(mouse_pos);
                        editor.select_to(pos, true, false, cx);

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
                }).unwrap_or(false);

                if !should_continue {
                    break;
                }
            }
        });
        self.auto_scroll_task = Some(task);
    }

    fn stop_responding_to_mouse_movements(&mut self) {
        self.auto_scroll_task = None;
    }
}

impl TextEditor {
    fn wrap_width(&self) -> Pixels {
        if self.scroll_handle.bounds().size.width > self.gutter_width + self.right_gutter_width {
            self.scroll_handle.bounds().size.width - self.gutter_width - self.right_gutter_width
        } else if let Some(old_bounds) = self.last_bounds {
            old_bounds.size.width - self.gutter_width - self.right_gutter_width
        } else {
            Pixels::MAX
        }
    }

    fn text_area_to_editor_pos(&self, pos: Point<Pixels>) -> Point<Pixels> {
        point(pos.x + self.gutter_width, pos.y)
    }
}

impl TextEditor {
    fn line_range_and_text(&self, state: &TextualState, line: usize) -> (Count8, Count8, String) {
        let start_loc = Location8 { row: line, col: 0 };
        let start_offset = state.loc8_to_offset8(start_loc);

        let end_loc = Location8 { row: line + 1, col: 0 };
        let end_offset = state.loc8_to_offset8(end_loc).min(state.len());

        let text = state.read(start_offset..end_offset);
        (start_offset, end_offset, text)
    }

    fn reshape_line(&mut self, wrap_width: Pixels, line_no: usize, window: &mut Window, cx: &mut App) -> WrappedLine {
        self.state.update(cx, |state, _| {
            let (start, end, mut line_text) = self.line_range_and_text(state, line_no);
            state.mark_line_as_up_to_date_attributes(start, end);

            if line_text.ends_with('\n') {
                line_text.pop();
            }

            state.prepare_diagnostics_iterator();
            let runs: SmallVec<[TextRun; 32]> = LineShaper::new(
                &self.text_styles,
                state.lex_rope().iterator(start),
                state.static_analysis_rope().iterator(start),
                state.diagnostics().iterator(start),
                line_text.len()
            ).collect();

            WrappedLine::new(
                &line_text,
                self.text_styles.text_size,
                &runs,
                wrap_width,
                window
            )
        })
    }

    fn reshape_lines(&mut self, del_range: Range<usize>, ins_range: Range<usize>, window: &mut Window, cx: &mut App) {
        let wrap_width = self.wrap_width();

        let replacement: SmallVec<[WrappedLine; 32]> = ins_range
            .map(|line_no| self.reshape_line(wrap_width, line_no, window, cx))
            .collect();

        self.line_map.replace_lines(del_range, replacement.into_iter());
    }

    pub(super) fn reshape_lines_needing_layout(&mut self, window: &mut Window, cx: &mut App) {
        let dirty = self.state.update(cx, |state, _cx| {
            state.take_lines_needing_relayout()
        });

        if let Some(dirty) = dirty {
            self.reshape_lines(dirty.clone(), dirty, window, cx);
        }
    }

    pub(super) fn reshape_visible_lines_with_stale_attributes(&mut self, window: &mut Window, cx: &mut App) {
        let wrap_width = self.wrap_width();
        for line in self.visible_lines() {
            let needs_reshaping = self.state.read(cx).line_has_new_attributes(line);
            if needs_reshaping {
                let new_line = self.reshape_line(wrap_width, line, window, cx);
                self.line_map.replace_lines(line..line + 1, std::iter::once(new_line));
            }
        }
    }

    pub(super) fn visible_lines(&self) -> Range<usize> {
        let scroll_range = -self.scroll_handle.offset().y ..
            (-self.scroll_handle.offset().y + self.scroll_handle.bounds().size.height);
        self.line_map.prewrapped_visible_lines(scroll_range)
    }
}

impl TextEditor {
    fn vertical_cursor_movement(&self, delta_lines: isize, cx: &App) -> Location8 {
        let current_pos = self.line_map.point_for_location(self.cursor(cx).head);
        let target_y = current_pos.y + delta_lines as f32 * self.line_height;
        match self.line_map.location_for_point(point(current_pos.x, target_y)) {
            Ok(loc) => loc,
            Err(loc) => loc,
        }
    }

    fn do_autocomplete_action(&mut self, cx: &mut Context<Self>) -> bool {
        let state = self.state.read(cx);
        state.autocomplete_state().borrow_mut().recheck_should_display(state.cursor())
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if self.do_autocomplete_action(cx) {
            self.state.read(cx).autocomplete_state()
                .borrow_mut()
                .move_index(-1);
            cx.notify();
        }
        else {
            self.move_to(self.vertical_cursor_movement(-1, cx), false, true, cx);
        }
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if self.do_autocomplete_action(cx) {
            self.state.read(cx).autocomplete_state()
                .borrow_mut()
                .move_index(1);
            cx.notify();
        }
        else {
            self.move_to(self.vertical_cursor_movement(1, cx), false, true, cx);
        }
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
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

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
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

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let offset = state.loc8_to_offset8(self.cursor(cx).head);
        let new_offset = state.prev_boundary(offset);
        self.select_to(state.offset8_to_loc8(new_offset), false, true, cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let offset = state.loc8_to_offset8(self.cursor(cx).head);
        let new_offset = state.next_boundary(offset);
        self.select_to(state.offset8_to_loc8(new_offset), false, true, cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical_cursor_movement(-1, cx), false, true, cx);
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.vertical_cursor_movement(1, cx), false, true, cx);
    }

    fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        self.set_cursor(Cursor {
            anchor: Location8 { row: 0, col: 0 },
            head: state.offset8_to_loc8(state.len()),
        }, cx);
        self.discretely_scroll_to_cursor(cx);
        cx.notify();
    }

    fn home(&mut self, _: &Home, _: &mut Window, cx: &mut Context<Self>) {
        let new_pos = Location8 { row: self.cursor(cx).head.row, col: 0 };
        self.move_to(new_pos, false, true, cx);
    }

    fn end(&mut self, _: &End, _: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.read(cx);
        let row = self.cursor(cx).head.row;
        let next_line = state.loc8_to_offset8(Location8 { row: row + 1, col: 0 });
        let line_end = next_line.saturating_sub(1).min(state.len());
        self.move_to(state.offset8_to_loc8(line_end), false, true, cx);
    }
}

impl TextEditor {
    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        if self.cursor(cx).is_empty() {
            let state = self.state.read(cx);
            let offset = state.loc8_to_offset8(self.cursor(cx).head);

            let line_start = state.loc8_to_offset8(Location8 {
                row: self.cursor(cx).head.row,
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
                // not really a key origin because the selction will instantly collapse
                self.select_to(state.offset8_to_loc8(new_offset), false, false, cx);
            } else {
                let new_offset = state.prev_boundary(offset);
                self.select_to(state.offset8_to_loc8(new_offset), false, false, cx);
            }

            self.replace_text_in_utf16_range(None, "", true, window, cx);
        }
        else {
            self.undo_group_boundary(cx);
            self.replace_text_in_utf16_range(None, "", true, window, cx);
            self.undo_group_boundary(cx);
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        if self.cursor(cx).is_empty() {
            let state = self.state.read(cx);
            let offset = state.loc8_to_offset8(self.cursor(cx).head);
            let new_offset = state.next_boundary(offset);
            self.select_to(state.offset8_to_loc8(new_offset), false, false, cx);
            self.replace_text_in_utf16_range(None, "", false, window, cx);
        }
        else {
            self.undo_group_boundary(cx);
            self.replace_text_in_utf16_range(None, "", false, window, cx);
            self.undo_group_boundary(cx);
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    fn backspace_word(&mut self, _: &BackspaceWord, window: &mut Window, cx: &mut Context<Self>) {
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

    fn backspace_line(&mut self, _: &BackspaceLine, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        self.undo_group_boundary(cx);
        self.select_to(Location8 { row: self.cursor(cx).head.row, col: 0 }, false, false, cx);
        self.replace_text_in_utf16_range(None, "", false, window, cx);
        self.undo_group_boundary(cx);
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        if self.do_autocomplete_action(cx) {
            let ac = self.state.read(cx).autocomplete_state();
            AutoCompleteState::apply_selected(&ac, self, self.state.clone(), window, cx);
        }
        else {
            if self.cursor(cx).is_empty() {
                // try to preserve indentation if possible
                let state = self.state.read(cx);
                let offset = state.loc8_to_offset8(self.cursor(cx).head);
                let line_start = state.loc8_to_offset8(Location8 {
                    row: self.cursor(cx).head.row,
                    col: 0
                });
                let line_end = state.loc8_to_offset8(Location8 {
                    row: self.cursor(cx).head.row,
                    col: usize::MAX
                });
                let text_before = state.read(line_start..offset);
                let text_after = state.read(offset..line_end);
                let leading_spaces = text_before.chars().take_while(|c| *c == ' ').count();
                let indent = " ".repeat(leading_spaces);
                if text_before.ends_with("{") && (text_after.is_empty() || text_after.starts_with("}")) {
                    // special case: if we are between braces, insert a newline with indentation,
                    // then another newline with decreased indentation
                    let inner_indent = " ".repeat(leading_spaces + TAB_SIZE);

                    let org_loc = self.cursor(cx).head;
                    self.replace_text_in_utf16_range(
                        None,
                       & if text_after.starts_with("}") {
                            format!("\n{}\n{}", inner_indent, indent)
                        }
                        else {
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
                    self.replace_text_in_utf16_range(None, &format!("\n{}", indent), true, window, cx);
                }
            } else {
                self.replace_text_in_utf16_range(None, "\n",  true, window, cx);
            }
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    fn tab(&mut self, _: &Tab, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        if self.do_autocomplete_action(cx) {
            let ac = self.state.read(cx).autocomplete_state();
            AutoCompleteState::apply_selected(&ac, self, self.state.clone(), window, cx);
        }
        else {
            self.undo_group_boundary(cx);
            if self.cursor(cx).is_empty() {
                self.replace_text_in_utf16_range(None, &" ".repeat(TAB_SIZE), false, window, cx);
            } else {
                let start_loc = self.cursor(cx).anchor.min(self.cursor(cx).head);
                let end_loc = self.cursor(cx).anchor.max(self.cursor(cx).head);

                for row in start_loc.row..=end_loc.row {
                    let line_start = self.state.read(cx).loc8_to_offset8(Location8 { row, col: 0 });
                    self.replace(line_start..line_start, &" ".repeat(TAB_SIZE), window, cx);
                }

                self.set_cursor(Cursor {
                    anchor: Location8 {
                        row: self.cursor(cx).anchor.row,
                        col: self.cursor(cx).anchor.col + TAB_SIZE,
                    },
                    head: Location8 {
                        row: self.cursor(cx).head.row,
                        col: self.cursor(cx).head.col + TAB_SIZE,
                    },
                }, cx);
                self.reset_cursor_blink(cx);
            }
            self.undo_group_boundary(cx);
        }
        self.state.update(cx, |state, cx| state.end_transaction(cx));
    }

    fn untab(&mut self, _: &Untab, window: &mut Window, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| state.start_transaction());
        self.undo_group_boundary(cx);

        let mut cursor = self.cursor(cx);
        let start_loc = cursor.anchor.min(cursor.head);
        let end_loc = cursor.anchor.max(cursor.head);

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

impl TextEditor {
    fn index_for_mouse_position(&self, position: Point<Pixels>) -> Option<Location8> {
        let Some(bounds) = self.last_bounds else {
            return None;
        };

        self.line_map.location_for_point(position - point(self.gutter_width, bounds.top())).ok()
    }

    fn closest_index_for_mouse_position(&self, position: Point<Pixels>) -> Location8 {
        let Some(bounds) = self.last_bounds else {
            return Location8 { row: 0, col: 0 };
        };

        match self.line_map.location_for_point(position - point(self.gutter_width, bounds.top())) {
            Ok(loc) => loc,
            Err(loc) => loc
        }
    }

    fn on_mouse_down(
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

                self.set_cursor(Cursor {
                    anchor: state.offset8_to_loc8(word_range.start),
                    head: state.offset8_to_loc8(word_range.end),
                }, cx);
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

                self.set_cursor(Cursor {
                    anchor: line_start,
                    head: line_end,
                }, cx);
                self.is_selecting = true;
                self.reset_cursor_blink(cx);
            }
        }

        if self.reset_hover_task_if_necessary(cx) {
            cx.notify();
        }
        self.state.read(cx).autocomplete_state().borrow_mut().disable();
    }

    fn on_mouse_up(&mut self, _event: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting = false;
        self.stop_responding_to_mouse_movements();
        self.auto_scroll_last_mouse_position = None;
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        // mouse position tracking is mainly done in the listener registered in the paint
        // since we don't get mouse move events if the mouse is outside the view in this method
        self.last_in_frame_mouse_position = Some(event.position);
        if self.reset_hover_task_if_necessary(cx) {
            cx.notify();
        }
    }

    fn on_scroll_wheel(&mut self, _event: &ScrollWheelEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.reset_hover_task_if_necessary(cx) {
            cx.notify();
        }
    }
}

impl TextEditor {
    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.undo_group_boundary(cx);
            self.replace_text_in_utf16_range(None, &text, false, window, cx);
            self.undo_group_boundary(cx);
        }
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor(cx).is_empty() {
            let state = self.state.read(cx);
            let range = state.cursor_range();
            cx.write_to_clipboard(ClipboardItem::new_string(
                state.read(range),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.cursor(cx).is_empty() {
            self.undo_group_boundary(cx);
            let state = self.state.read(cx);
            let range = state.cursor_range();
            cx.write_to_clipboard(ClipboardItem::new_string(
                state.read(range),
            ));
            self.replace_text_in_utf16_range(None, "", false, window, cx);
            self.undo_group_boundary(cx);
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

impl EntityInputHandler for TextEditor {
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
        let range = state.cursor_range();
        Some(UTF16Selection {
            range: state.span8_to_span16(&range),
            reversed: self.cursor(cx).reversed(),
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
        self.replace_text_in_utf16_range(range_utf16, new_text, true, window, cx);
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
            .unwrap_or_else(|| self.state.read(cx).cursor_range());

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
            self.set_cursor(Cursor {
                anchor: state.offset8_to_loc8(adjusted_start),
                head: state.offset8_to_loc8(adjusted_end),
            }, cx);
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
        let loc8 = self.closest_index_for_mouse_position(point);
        let offset8 = state.loc8_to_offset8(loc8);
        Some(state.offset8_to_offset16(offset8) as usize)
    }
}

impl Focusable for TextEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for TextEditor {
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
                    .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
                    .child( TextElement { editor: cx.entity() } )
            )
            .child (
                PopoverElement::new(cx.entity())
            )
    }
}
