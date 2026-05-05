use std::ops::Range;

use gpui::{
    App, Bounds, DispatchPhase, Element, ElementId, ElementInputHandler, Entity, GlobalElementId,
    Hsla, IntoElement, LayoutId, MouseMoveEvent, Pixels, Point, Style, TextRun, Window, fill,
    point, px, relative, size,
};

use crate::editor::{
    text_editor::{BOTTOM_SCROLL_PADDING, TextEditor},
    wrapped_line::WrappedLine,
};

pub struct TextElement {
    pub editor: Entity<TextEditor>,
}

struct ScrollBarState {
    wheel_bounds: Bounds<Pixels>,
    background_bounds: Bounds<Pixels>,
    diagnostic_bounds: Vec<(Bounds<Pixels>, Hsla)>,
}

pub struct PrepaintState {
    // number, y, line
    lines: Vec<(usize, Pixels, WrappedLine)>,
    cursor_bounds: Option<Bounds<Pixels>>,
    selection_bounds: Vec<Bounds<Pixels>>,
    search_bounds: Vec<(Bounds<Pixels>, bool)>,
    active_line_bounds: Option<Bounds<Pixels>>,
    scroll_wheel_state: Option<ScrollBarState>,
}

impl IntoElement for TextElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl TextElement {
    fn compute_cursor_bounds(
        &self,
        editor: &TextEditor,
        bounds: Bounds<Pixels>,
        window: &Window,
        cx: &App,
    ) -> Option<Bounds<Pixels>> {
        if !editor.cursor(cx).is_empty()
            || !editor.cursor_blink_state
            || !editor.focus_handle.is_focused(window)
        {
            return None;
        }

        let Point { x, y } = editor.line_map.point_for_location(editor.cursor(cx).head);
        Some(Bounds::new(
            point(bounds.left() + editor.gutter_width + x, bounds.top() + y),
            size(px(1.5), editor.line_height),
        ))
    }

    fn compute_active_line_bounds(
        &self,
        editor: &TextEditor,
        bounds: Bounds<Pixels>,
        window: &Window,
        cx: &App,
    ) -> Option<Bounds<Pixels>> {
        if !editor.cursor(cx).is_empty() || !editor.focus_handle.is_focused(window) {
            return None;
        }

        let line_num = editor.cursor(cx).head.row as usize;
        let y_range = editor.line_map.y_range(line_num..line_num + 1);
        Some(Bounds::new(
            point(bounds.left(), bounds.top() + y_range.start),
            size(bounds.size.width, y_range.end - y_range.start),
        ))
    }

    fn compute_selection_bounds(
        &self,
        editor: &TextEditor,
        bounds: Bounds<Pixels>,
        visible_lines: Range<usize>,
        window: &Window,
        cx: &App,
    ) -> Vec<Bounds<Pixels>> {
        if editor.cursor(cx).is_empty() || !editor.focus_handle.is_focused(window) {
            return Vec::new();
        }

        let cursor = editor.cursor(cx);
        let start_loc = cursor.anchor.min(cursor.head);
        let end_loc = cursor.anchor.max(cursor.head);

        let visible_selection =
            visible_lines.start.max(start_loc.row)..visible_lines.end.min(end_loc.row + 1);
        let mut selection_bounds = Vec::new();

        for multi_line in editor
            .line_map
            .unwrapped_lines_iter(visible_selection.start)
            .take(visible_selection.len())
        {
            let line_num = multi_line.unwrapped_line_no;
            let line_start = if line_num == start_loc.row {
                start_loc.col
            } else {
                0
            };
            let line_end = if line_num == end_loc.row {
                end_loc.col
            } else {
                editor.line_map.line_len(line_num)
            };
            let line_y = editor.line_map.y_range(line_num..line_num + 1).start;

            for (wrapped_ix, single_line) in multi_line.line.iter().enumerate() {
                let Some(mut x_pixels) = single_line.x_range(line_start..line_end) else {
                    continue;
                };
                x_pixels.end = x_pixels.end.max(x_pixels.start + px(5.0));
                let y = line_y + editor.line_height * wrapped_ix as f32;
                selection_bounds.push(Bounds::from_corners(
                    point(
                        bounds.left() + editor.gutter_width + x_pixels.start,
                        bounds.top() + y,
                    ),
                    point(
                        bounds.left() + editor.gutter_width + x_pixels.end,
                        bounds.top() + y + editor.line_height,
                    ),
                ));
            }
        }

        selection_bounds
    }

    fn compute_search_bounds(
        &self,
        editor: &TextEditor,
        bounds: Bounds<Pixels>,
        visible_lines: Range<usize>,
        cx: &App,
    ) -> Vec<(Bounds<Pixels>, bool)> {
        if !editor.search.visible || editor.search.matches.is_empty() {
            return Vec::new();
        }

        let state = editor.state.read(cx);
        let mut search_bounds = Vec::new();
        for (match_ix, span) in editor.search.matches.iter().enumerate() {
            let start_loc = state.offset8_to_loc8(span.start);
            let end_loc = state.offset8_to_loc8(span.end);
            let visible_match_lines =
                visible_lines.start.max(start_loc.row)..visible_lines.end.min(end_loc.row + 1);
            if visible_match_lines.is_empty() {
                continue;
            }

            for multi_line in editor
                .line_map
                .unwrapped_lines_iter(visible_match_lines.start)
                .take(visible_match_lines.len())
            {
                let line_num = multi_line.unwrapped_line_no;
                let line_start = if line_num == start_loc.row {
                    start_loc.col
                } else {
                    0
                };
                let line_end = if line_num == end_loc.row {
                    end_loc.col
                } else {
                    editor.line_map.line_len(line_num)
                };
                let line_y = editor.line_map.y_range(line_num..line_num + 1).start;

                for (wrapped_ix, single_line) in multi_line.line.iter().enumerate() {
                    let Some(mut x_pixels) = single_line.x_range(line_start..line_end) else {
                        continue;
                    };
                    x_pixels.end = x_pixels.end.max(x_pixels.start + px(3.0));
                    let y = line_y + editor.line_height * wrapped_ix as f32;
                    search_bounds.push((
                        Bounds::from_corners(
                            point(
                                bounds.left() + editor.gutter_width + x_pixels.start,
                                bounds.top() + y,
                            ),
                            point(
                                bounds.left() + editor.gutter_width + x_pixels.end,
                                bounds.top() + y + editor.line_height,
                            ),
                        ),
                        editor.search.active_match == Some(match_ix),
                    ));
                }
            }
        }

        search_bounds
    }

    fn compute_scroll_bar_state(
        &self,
        editor: &TextEditor,
        bounds: Bounds<Pixels>,
        _window: &Window,
        cx: &App,
    ) -> Option<ScrollBarState> {
        let scroll_offset = editor.scroll_handle.offset();
        let viewport_height = editor.scroll_handle.bounds().size.height;
        let content_height = editor.line_map.total_height() + px(BOTTOM_SCROLL_PADDING);

        if content_height <= viewport_height {
            return None;
        }

        let wheel_height = (viewport_height / content_height) * viewport_height;
        // offset scroll (realistically this should just be drawn outside of the scroll)
        let wheel_y =
            -scroll_offset.y + (-scroll_offset.y / content_height) * viewport_height + bounds.top();
        let width = editor.right_gutter_width;

        let wheel_bounds = Bounds::new(
            point(bounds.right() - width, wheel_y),
            size(width, wheel_height),
        );

        let background_bounds = Bounds::new(
            point(bounds.right() - width, -scroll_offset.y + bounds.top()),
            size(width, viewport_height),
        );

        let state = editor.state.read(cx);
        let diagnostic_bounds = state
            .diagnostics()
            .diagnostics_list()
            .iter()
            .map(|d| {
                let color = d.color(&editor.text_styles);
                let start = d.span.start;
                let line = state.offset8_to_loc8(start).row as usize;
                let y_start = editor.line_map.y_range(line..line + 1).start;
                let width = editor.right_gutter_width;
                let bounds = Bounds::new(
                    point(
                        bounds.right() - width,
                        bounds.top() - scroll_offset.y
                            + (y_start / content_height) * viewport_height,
                    ),
                    size(width, px(3.0)),
                );

                (bounds, color)
            })
            .collect();

        Some(ScrollBarState {
            wheel_bounds,
            background_bounds,
            diagnostic_bounds,
        })
    }

    fn paint_gutter_line(
        &self,
        line_num: usize,
        y: Pixels,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let editor = self.editor.read(cx);
        let line_range = editor.cursor(cx).line_range();
        let line_selected =
            line_range.contains(&line_num) && editor.focus_handle.is_focused(window);
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
        gutter_shaped
            .paint(
                point(bounds.left() + gutter_x, bounds.top() + y),
                editor.line_height,
                window,
                cx,
            )
            .ok();
    }

    fn paint_text_line(
        &self,
        gutter_width: Pixels,
        line_height: Pixels,
        shaped: &WrappedLine,
        y: Pixels,
        bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let line_origin = point(bounds.left() + gutter_width, bounds.top() + y);
        shaped.paint(line_origin, line_height, window, cx).ok();
    }

    fn paint_scroll_bar(
        &self,
        state: &ScrollBarState,
        scroll_color: Hsla,
        scroll_background_color: Hsla,
        _bounds: Bounds<Pixels>,
        window: &mut Window,
    ) {
        window.paint_quad(fill(state.background_bounds, scroll_background_color));
        for (diagnostic_bound, color) in &state.diagnostic_bounds {
            window.paint_quad(fill(*diagnostic_bound, *color));
        }
        window.paint_quad(fill(state.wheel_bounds, scroll_color));
    }

    fn handle_width_resize(&self, editor: &mut TextEditor, bounds: Bounds<Pixels>, cx: &mut App) {
        if editor
            .last_bounds
            .is_none_or(|b| b.size.width != bounds.size.width)
        {
            editor.capture_top_visible_line();
            editor.state.update(cx, |state, _| {
                state.mark_lines_needing_relayout(0..editor.line_map.line_count());
            });
        }
        editor.last_bounds = Some(bounds);
    }
}

impl Element for TextElement {
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
            editor.reshape_lines_needing_layout(window, cx);
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
            editor.reshape_lines_needing_layout(window, cx);
            editor.reshape_visible_lines_with_stale_attributes(window, cx);

            let visible_lines = editor.visible_lines();

            let lines = editor
                .line_map
                .unwrapped_lines_iter(visible_lines.start)
                .take(visible_lines.len())
                .map(|line| {
                    let line_no = line.unwrapped_line_no;
                    let y = editor.line_map.y_range(line_no..line_no + 1).start;
                    (line_no, y, line.line.clone())
                })
                .collect();

            let cursor_bounds = self.compute_cursor_bounds(editor, bounds, window, cx);
            let active_line_bounds = self.compute_active_line_bounds(editor, bounds, window, cx);
            let search_bounds =
                self.compute_search_bounds(editor, bounds, visible_lines.clone(), cx);
            let selection_bounds =
                self.compute_selection_bounds(editor, bounds, visible_lines, window, cx);
            let scroll_wheel_bounds = self.compute_scroll_bar_state(editor, bounds, window, cx);

            PrepaintState {
                lines,
                cursor_bounds,
                selection_bounds,
                search_bounds,
                active_line_bounds,
                scroll_wheel_state: scroll_wheel_bounds,
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
        let scroll_color = editor.text_styles.scroll_color;
        let scroll_background_color = editor.text_styles.scroll_background_color;
        let active_line_color = editor.text_styles.active_line_color;
        let selection_color = editor.text_styles.selection_color;
        let search_match_color = editor.text_styles.search_match_color;
        let active_search_match_color = editor.text_styles.active_search_match_color;
        let gutter_width = editor.gutter_width;
        let line_height = editor.line_height;

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
            window.paint_quad(fill(active_bounds, active_line_color));
        }

        for (search_bounds, active) in &prepaint.search_bounds {
            let color = if *active {
                active_search_match_color
            } else {
                search_match_color
            };
            window.paint_quad(fill(*search_bounds, color));
        }

        for sel_bounds in &prepaint.selection_bounds {
            window.paint_quad(fill(*sel_bounds, selection_color));
        }

        for (line_num, y, shaped) in &prepaint.lines {
            self.paint_text_line(gutter_width, line_height, shaped, *y, bounds, window, cx);
            self.paint_gutter_line(*line_num, *y, bounds, window, cx);
        }

        if let Some(cursor_bounds) = prepaint.cursor_bounds {
            window.paint_quad(fill(cursor_bounds, cursor_color));
        }

        if let Some(state) = &prepaint.scroll_wheel_state {
            self.paint_scroll_bar(state, scroll_color, scroll_background_color, bounds, window);
        }
    }
}
