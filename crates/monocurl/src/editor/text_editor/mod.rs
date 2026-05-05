use std::collections::VecDeque;
use std::ops::Range;
use std::time::Duration;
use std::usize;

use crate::components::text_input::{SingleLineInput, SingleLineInputEvent};
use crate::editor::line_map::LineMap;
use crate::editor::line_shaper::LineShaper;
use crate::editor::text_editor::popover_element::PopoverElement;
use crate::editor::text_editor::text_element::TextElement;
use crate::editor::wrapped_line::WrappedLine;
use crate::state::diagnostics::Diagnostic;
use crate::state::textual_state::{AutoCompleteState, Cursor, TextualState};
use crate::theme::{TextEditorStyles, ThemeSettings};
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
const LINE_COMMENT_PREFIX: &str = "# ";

mod clipboard;
mod cursor;
mod editing;
mod history;
mod hover;
mod input_handler;
mod layout;
mod mouse;
mod popover_element;
mod render;
mod scroll;
mod search;
mod text_element;

use history::HistoryGroup;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, None),
        KeyBinding::new("shift-backspace", Backspace, None),
        KeyBinding::new("alt-backspace", BackspaceWord, None),
        KeyBinding::new("shift-alt-backspace", BackspaceWord, None),
        KeyBinding::new("secondary-backspace", BackspaceLine, None),
        KeyBinding::new("shift-secondary-backspace", BackspaceLine, None),
        KeyBinding::new("delete", Delete, None),
        KeyBinding::new("shift-delete", Delete, None),
        KeyBinding::new("alt-delete", DeleteWord, None),
        KeyBinding::new("secondary-delete", DeleteLine, None),
        KeyBinding::new("up", Up, None),
        KeyBinding::new("down", Down, None),
        KeyBinding::new("left", Left, None),
        KeyBinding::new("right", Right, None),
        KeyBinding::new("alt-left", LeftWord, None),
        KeyBinding::new("alt-right", RightWord, None),
        KeyBinding::new("secondary-left", Home, None),
        KeyBinding::new("secondary-right", End, None),
        KeyBinding::new("enter", Enter, None),
        KeyBinding::new("tab", Tab, None),
        KeyBinding::new("shift-tab", Untab, None),
        KeyBinding::new("secondary-/", ToggleComment, None),
        KeyBinding::new("shift-left", SelectLeft, None),
        KeyBinding::new("shift-right", SelectRight, None),
        KeyBinding::new("shift-alt-left", SelectLeftWord, None),
        KeyBinding::new("shift-alt-right", SelectRightWord, None),
        KeyBinding::new("shift-secondary-left", SelectHome, None),
        KeyBinding::new("shift-secondary-right", SelectEnd, None),
        KeyBinding::new("shift-up", SelectUp, None),
        KeyBinding::new("shift-down", SelectDown, None),
        KeyBinding::new("secondary-a", SelectAll, None),
        KeyBinding::new("secondary-v", Paste, None),
        KeyBinding::new("secondary-c", Copy, None),
        KeyBinding::new("secondary-x", Cut, None),
        KeyBinding::new("secondary-f", OpenFind, None),
        KeyBinding::new("secondary-g", FindNext, None),
        KeyBinding::new("secondary-shift-g", FindPrevious, None),
        KeyBinding::new("shift-enter", FindPrevious, Some("find-panel")),
        KeyBinding::new("escape", CloseFind, Some("find-panel")),
        KeyBinding::new("escape", CloseFind, Some("single-line-input")),
        KeyBinding::new("home", Home, None),
        KeyBinding::new("end", End, None),
        KeyBinding::new("ctrl-secondary-space", ShowCharacterPalette, None),
    ]);
}

fn point_dist(p: Point<Pixels>) -> Pixels {
    let hypot = (p.x.to_f64() * p.x.to_f64() + p.y.to_f64() * p.y.to_f64()).sqrt();
    px(hypot as f32)
}

fn adjust_cursor_after_uncomment(
    cursor_col: usize,
    comment_col: usize,
    removed_len: usize,
) -> usize {
    if cursor_col <= comment_col {
        cursor_col
    } else if cursor_col < comment_col + removed_len {
        comment_col
    } else {
        cursor_col - removed_len
    }
}

#[derive(Default)]
struct SearchState {
    visible: bool,
    matches: Vec<Span8>,
    active_match: Option<usize>,
    suppress_refresh: bool,
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
    save_dirty: Entity<bool>,

    find_query_input: Entity<SingleLineInput>,
    find_replace_input: Entity<SingleLineInput>,
    search: SearchState,

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
    copied_hover_message: Option<String>,

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
    pub fn new(
        state: Entity<TextualState>,
        window: &mut Window,
        cx: &mut Context<Self>,
        content: String,
        dirty: Entity<bool>,
        save_dirty: Entity<bool>,
    ) -> Self {
        let text_styles = ThemeSettings::theme(cx).text_editor_styles();
        let line_height = text_styles.line_height;
        let focus_handle = cx.focus_handle();
        let find_query_input = cx.new(|cx| SingleLineInput::new("Find", cx));
        let find_replace_input = cx.new(|cx| SingleLineInput::new("Replace", cx));

        // re render whenever state changes
        // (mainly want the rerender when theres external changes to the state)
        cx.observe(&state, |_me, _, cx| {
            cx.notify();
        })
        .detach();
        cx.observe_global::<ThemeSettings>(|editor, cx| {
            let styles = ThemeSettings::theme(cx).text_editor_styles();
            editor.apply_theme(styles, cx);
            cx.notify();
        })
        .detach();
        cx.subscribe(
            &find_query_input,
            |editor, _, event: &SingleLineInputEvent, cx| match event {
                SingleLineInputEvent::Edited => editor.on_find_query_edited(cx),
            },
        )
        .detach();

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

        let focus_out_subscription =
            cx.on_focus_out(&focus_handle, window, |editor, _, _window, cx| {
                editor.close_find_panel_without_focus(cx);
                editor.reset_hover_task_if_necessary(cx);
                editor.cursor_blink_task = None;
                editor.cursor_blink_state = false;
                cx.notify()
            });
        let mut ret = TextEditor {
            focus_handle,
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
            save_dirty,
            find_query_input,
            find_replace_input,
            search: SearchState::default(),
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
            copied_hover_message: None,
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
            ret.dirty.update(cx, |dirty, _| *dirty = false);
            ret.save_dirty.update(cx, |dirty, _| *dirty = false);
        }
        ret
    }

    pub fn editor_focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }

    fn apply_theme(&mut self, styles: TextEditorStyles, cx: &mut Context<Self>) {
        self.line_height = styles.line_height;
        self.line_map.set_line_height(styles.line_height);
        self.text_styles = styles;

        let line_count = self.line_map.line_count();
        if line_count > 0 {
            self.state.update(cx, |state, _| {
                state.mark_lines_needing_relayout(0..line_count);
            });
        }
    }
}
