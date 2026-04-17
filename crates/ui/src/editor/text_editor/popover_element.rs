use std::{cell::RefCell, rc::Rc, time::Duration};

use crate::{
    state::{
        diagnostics::Diagnostic,
        textual_state::{AutoCompleteState, ParameterPositionState},
    },
    text_editor::TextEditor,
    theme::TextEditorStyles,
};
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Bounds, BoxShadow, Element, ElementId, Entity,
    FontWeight, GlobalElementId, InspectorElementId, InteractiveElement, IntoElement, LayoutId,
    ParentElement, Pixels, Point, Position, Size, StatefulInteractiveElement, Style, Styled,
    Window, div, point, prelude::FluentBuilder, px, relative, size,
};
use smallvec::SmallVec;
use structs::text::Location8;

const PARAMETER_SUPRESSION_DUE_TO_CURSOR: Duration = Duration::from_millis(500);
const PARAMETER_SUPPRESSION_DUE_TO_AUTOCOMPLETE: Duration = Duration::from_millis(500);

pub struct PopoverElement {
    pub editor: Entity<TextEditor>,
}

impl PopoverElement {
    pub fn new(editor: Entity<TextEditor>) -> Self {
        Self { editor }
    }

    fn is_parameter_hint_suppressed(&self, editor: &TextEditor) -> bool {
        editor.parameter_hint_suppressed
    }

    fn suppress_parameter_hint(&self, duration: Duration, cx: &mut App) {
        let weak_entity = self.editor.downgrade();

        let task = cx.spawn(async move |cx: &mut AsyncApp| {
            cx.background_executor().timer(duration).await;
            let Some(entity) = weak_entity.upgrade() else {
                return;
            };
            cx.update_entity(&entity, |entity, _cx| {
                entity.parameter_hint_suppressed = false;
            })
            .ok();
        });
        self.editor.update(cx, |editor, _| {
            editor.parameter_hint_suppressed = true;
            editor.parameter_hint_suppression_task = Some(task);
        });
    }

    fn pos_of_loc(&mut self, editor: &TextEditor, location8: Location8) -> Point<Pixels> {
        let text_area_pos = editor.line_map.point_for_location(location8);
        let editor_pos = editor.text_area_to_editor_pos(text_area_pos);
        let handle = &editor.scroll_handle;
        let scroll = handle
            .offset()
            .y
            .clamp(-handle.max_offset().height, px(0.0));
        point(editor_pos.x, editor_pos.y + scroll)
    }

    fn hovered_diagnostic(&mut self, cx: &mut App) -> Option<DiagnosticPopoverState> {
        let editor = self.editor.read(cx);

        editor.hover_item.as_ref().map(|(_, d)| {
            let state = editor.state.read(cx);
            let location8 = state.offset8_to_loc8(d.span.start);
            let pos_in_container = self.pos_of_loc(&editor, location8);

            DiagnosticPopoverState {
                diagnostic: d.clone(),
                pos_in_container,
            }
        })
    }

    fn autocomplete_state(
        &mut self,
        window: &Window,
        cx: &mut App,
    ) -> Option<AutoCompletePopoverState> {
        let editor = self.editor.read(cx);
        let state = editor.state.read(cx);
        let ac_state = state.autocomplete_state();
        if !ac_state.borrow_mut().recheck_should_display(state.cursor())
            || !editor.focus_handle.is_focused(window)
        {
            return None;
        }
        let pos = self.pos_of_loc(editor, ac_state.borrow().word_start());
        Some(AutoCompletePopoverState {
            autocomplete_state: state.autocomplete_state(),
            pos_in_container: pos,
        })
    }

    fn parameter_hint_state(
        &mut self,
        window: &Window,
        cx: &mut App,
    ) -> Option<ParameterHintPopoverState> {
        let editor = self.editor.read(cx);
        if self.is_parameter_hint_suppressed(editor) {
            return None;
        }

        let state = editor.state.read(cx);
        let ph_state = state.parameter_position_state();
        if !ph_state.borrow_mut().recheck_should_display(state.cursor())
            || !editor.focus_handle.is_focused(window)
        {
            self.editor
                .update(cx, |editor, _| editor.parameter_hint_allowed_base = None);
            return None;
        }
        let ph_state = ph_state.borrow();
        let hint = ph_state.hint.as_ref().unwrap();
        if Some(hint.function_start) != editor.parameter_hint_allowed_base {
            // reset timer
            self.editor.update(cx, |editor, _| {
                editor.parameter_hint_allowed_base = Some(hint.function_start)
            });
            self.suppress_parameter_hint(PARAMETER_SUPRESSION_DUE_TO_CURSOR, cx);
            return None;
        }
        let pos = self.pos_of_loc(editor, hint.function_start);
        Some(ParameterHintPopoverState {
            parameter_hint_state: state.parameter_position_state(),
            pos_in_container: pos,
        })
    }

    fn place(
        &self,
        line_height: Pixels,
        popover_size: Size<Pixels>,
        target_position: Point<Pixels>,
        container_bounds: Bounds<Pixels>,
        other_popovers: &Vec<Bounds<Pixels>>,
        prefer_up: bool,
    ) -> Option<PopoverPlacement> {
        let below_y = target_position.y + line_height;
        let above_y = target_position.y - popover_size.height;

        let margin = px(8.0);
        let x = target_position.x.clamp(
            container_bounds.left() + margin,
            container_bounds.right() - popover_size.width - margin,
        );

        let space_below = container_bounds.bottom() - (target_position.y + line_height + margin);
        let space_above = target_position.y - container_bounds.top();

        let below_fits = space_below >= popover_size.height
            && container_bounds.contains(&point(x, below_y))
            && !other_popovers.iter().any(|other| {
                let my_bounds = Bounds::new(point(x, below_y), popover_size);
                my_bounds.intersects(other)
            });

        let above_fits = space_above >= popover_size.height
            && container_bounds.contains(&point(x, target_position.y))
            && !other_popovers.iter().any(|other| {
                let my_bounds = Bounds::new(point(x, above_y), popover_size);
                my_bounds.intersects(other)
            });

        let y = if prefer_up && above_fits {
            if above_fits {
                above_y
            } else if below_fits {
                below_y
            } else {
                return None;
            }
        } else {
            if below_fits {
                below_y
            } else if above_fits {
                above_y
            } else {
                return None;
            }
        };

        Some(PopoverPlacement {
            origin: Point { x, y },
        })
    }

    fn build_diagnostic_popover(
        &self,
        diagnostic: &Diagnostic,
        styles: &TextEditorStyles,
    ) -> AnyElement {
        let color = diagnostic.color(styles);
        let padding = px(8.0);
        let margin = px(4.0);
        let max_w = px(600.0);

        div()
            .flex()
            .absolute()
            .max_w(max_w)
            .pb(margin)
            .pt(margin)
            .child(
                div()
                    .p(padding)
                    .flex()
                    .flex_col()
                    .max_w(max_w)
                    .bg(styles.popover_background_color)
                    .rounded_md()
                    .border_1()
                    .border_color(color)
                    .shadow(vec![BoxShadow {
                        offset: Point {
                            x: px(0.),
                            y: px(0.),
                        },
                        blur_radius: px(2.),
                        spread_radius: px(2.),
                        color: styles.popover_shadow_color,
                    }])
                    .child(
                        div()
                            .text_sm()
                            .text_color(styles.popover_title_color)
                            .child(diagnostic.title.clone()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(styles.popover_text_color)
                            .child(diagnostic.message.clone()),
                    ),
            )
            .on_mouse_move(|_, window, app| {
                window.prevent_default();
                app.stop_propagation();
            })
            .into_any_element()
    }

    fn render_highlighted_text(
        text: &str,
        highlight_indices: &[usize],
        is_selected: bool,
        styles: &TextEditorStyles,
    ) -> AnyElement {
        let base_color = if is_selected {
            styles.popover_title_color
        } else {
            styles.popover_text_color
        };
        let highlight_color = styles.popover_highlight_color;

        let mut segments = Vec::new();
        let mut current_segment = String::new();
        let mut current_is_highlighted = false;
        text.char_indices()
            .map(|(i, ch)| (highlight_indices.contains(&i), ch))
            .for_each(|(is_highlighted, ch)| {
                if current_segment.is_empty() {
                    current_segment.push(ch);
                    current_is_highlighted = is_highlighted;
                } else if is_highlighted == current_is_highlighted && false {
                    // disabled for now since it causes visual glitches
                    current_segment.push(ch);
                } else {
                    segments.push((current_segment.clone(), current_is_highlighted));
                    current_segment.clear();
                    current_segment.push(ch);
                    current_is_highlighted = is_highlighted;
                }
            });

        if !current_segment.is_empty() {
            segments.push((current_segment, current_is_highlighted));
        }

        let container =
            div()
                .flex()
                .items_center()
                .text_size(px(14.))
                .children(segments.into_iter().map(|(segment_text, is_highlighted)| {
                    div()
                        .text_color(if is_highlighted {
                            highlight_color
                        } else {
                            base_color
                        })
                        .when(is_highlighted, |this| {
                            this.font_weight(FontWeight::BOLD).underline()
                        })
                        .child(segment_text)
                }));

        container.into_any_element()
    }

    fn build_autocomplete_popover(
        &self,
        autocomplete_state: &Rc<RefCell<AutoCompleteState>>,
        styles: &TextEditorStyles,
    ) -> AnyElement {
        let padding = px(4.0);
        let item_padding_x = px(8.0);
        let item_padding_y = px(3.0);
        let min_w = px(200.0);
        let max_w = px(400.0);

        let ac = autocomplete_state.borrow();

        div()
            .flex()
            .absolute()
            .p(padding)
            .bg(styles.popover_background_color)
            .rounded_md()
            .border_1()
            .border_color(styles.popover_border_color)
            .max_w(max_w)
            .shadow(vec![BoxShadow {
                offset: Point {
                    x: px(0.),
                    y: px(0.),
                },
                blur_radius: px(2.),
                spread_radius: px(2.),
                color: styles.popover_shadow_color,
            }])
            .child(
                div()
                    .min_w(min_w)
                    .flex()
                    .flex_col()
                    .max_h(px(200.0))
                    .id("autocomplete-bar")
                    .overflow_y_scroll()
                    .track_scroll(&ac.scroll_handle)
                    .on_scroll_wheel(|_scroll, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .children(ac.filtered_items.iter().map(|(index, highlights)| {
                        let index_copy = *index;
                        let item1 = ac.items[*index].clone();
                        let head = item1.head.clone();
                        let is_selected = *index == ac.selected_index;

                        let ac_copy = autocomplete_state.clone();
                        let editor_copy = self.editor.clone();

                        div().child(
                            div()
                                .px(item_padding_x)
                                .py(item_padding_y)
                                .rounded_sm()
                                .when(is_selected, |this| {
                                    this.bg(styles.popover_selected_background_color)
                                })
                                .when(!is_selected, |this| {
                                    this.bg(styles.popover_background_color).hover({
                                        let hover = styles.popover_hover_background_color;
                                        move |style| style.bg(hover)
                                    })
                                })
                                .cursor_pointer()
                                .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                    window.prevent_default();
                                    cx.stop_propagation();
                                    editor_copy.update(cx, |editor, cx| {
                                        AutoCompleteState::apply_index(
                                            &ac_copy,
                                            index_copy,
                                            editor,
                                            editor.state.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                })
                                .child(div().flex().items_center().child(
                                    Self::render_highlighted_text(
                                        &head,
                                        highlights,
                                        is_selected,
                                        styles,
                                    ),
                                )),
                        )
                    })),
            )
            .into_any_element()
    }

    fn build_parameter_hint_popover(
        &self,
        parameter_hint_state: &Rc<RefCell<ParameterPositionState>>,
        styles: &TextEditorStyles,
    ) -> AnyElement {
        let item_padding_x = px(12.0);
        let item_padding_y = px(6.0);
        let min_w = px(150.0);
        let max_w = px(500.0);

        let state = parameter_hint_state.borrow();
        let hint = state.hint.as_ref().unwrap();

        div()
            .flex()
            .absolute()
            .bg(styles.popover_background_color)
            .rounded_md()
            .border_1()
            .border_color(styles.popover_border_color)
            .min_w(min_w)
            .max_w(max_w)
            .shadow(vec![BoxShadow {
                offset: Point {
                    x: px(0.),
                    y: px(0.),
                },
                blur_radius: px(1.),
                spread_radius: px(1.),
                color: styles.popover_shadow_color,
            }])
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .px(item_padding_x)
                    .py(item_padding_y)
                    .text_sm()
                    .child(
                        div()
                            .text_color(styles.popover_title_color)
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(hint.name.clone()),
                    )
                    .child(
                        div()
                            .text_color(styles.popover_title_color)
                            .child(if hint.is_operator { "{" } else { "(" }),
                    )
                    .children(hint.args.iter().enumerate().flat_map(|(i, arg)| {
                        let is_active = i == hint.active_index;
                        let mut elements = vec![
                            div()
                                .when(is_active, |this| {
                                    this.text_color(styles.popover_active_argument_color)
                                        .font_weight(gpui::FontWeight::BOLD)
                                        .underline()
                                })
                                .when(!is_active, |this| {
                                    this.text_color(styles.popover_inactive_argument_color)
                                })
                                .child(arg.clone())
                                .into_any_element(),
                        ];

                        if i < hint.args.len() - 1 {
                            elements.push(
                                div()
                                    .text_color(styles.popover_title_color)
                                    .child(", ")
                                    .into_any_element(),
                            );
                        }

                        elements
                    }))
                    .child(
                        div()
                            .text_color(styles.popover_title_color)
                            .child(if hint.is_operator { "}" } else { ")" }),
                    ),
            )
            .into_any_element()
    }
}

struct DiagnosticPopoverState {
    diagnostic: Diagnostic,
    pos_in_container: Point<Pixels>,
}

struct AutoCompletePopoverState {
    autocomplete_state: Rc<RefCell<AutoCompleteState>>,
    pos_in_container: Point<Pixels>,
}

struct ParameterHintPopoverState {
    parameter_hint_state: Rc<RefCell<ParameterPositionState>>,
    pos_in_container: Point<Pixels>,
}

struct ChildElementState {
    popover_content: AnyElement,
    pos_in_container: Point<Pixels>,
    prefer_up: bool,
    content_layout_id: LayoutId,
}

pub struct RequestLayoutState {
    children: SmallVec<[ChildElementState; 2]>,
}

#[derive(Debug, Clone, Copy)]
struct PopoverPlacement {
    origin: Point<Pixels>,
}

impl IntoElement for PopoverElement {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PopoverElement {
    type RequestLayoutState = RequestLayoutState;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let diagnostic_state = self.hovered_diagnostic(cx);
        let styles = self.editor.read(cx).text_styles.clone();

        let mut children: SmallVec<_> = SmallVec::new();
        if let Some(ref ac_state) = self.autocomplete_state(window, cx) {
            let mut popover_content =
                self.build_autocomplete_popover(&ac_state.autocomplete_state, &styles);
            let content_layout_id = popover_content.request_layout(window, cx);
            let pos_in_container = ac_state.pos_in_container;

            self.suppress_parameter_hint(PARAMETER_SUPPRESSION_DUE_TO_AUTOCOMPLETE, cx);
            children.push(ChildElementState {
                popover_content,
                pos_in_container,
                prefer_up: false,
                content_layout_id,
            });
        }

        if let Some(ref parameter_hint) = self.parameter_hint_state(window, cx) {
            let mut popover_content =
                self.build_parameter_hint_popover(&parameter_hint.parameter_hint_state, &styles);
            let content_layout_id = popover_content.request_layout(window, cx);
            let pos_in_container = parameter_hint.pos_in_container;

            children.push(ChildElementState {
                popover_content,
                pos_in_container,
                prefer_up: false,
                content_layout_id,
            });
        }

        if let Some(ref diag_state) = diagnostic_state {
            let mut popover_content =
                self.build_diagnostic_popover(&diag_state.diagnostic, &styles);
            let content_layout_id = popover_content.request_layout(window, cx);

            children.push(ChildElementState {
                popover_content,
                pos_in_container: diag_state.pos_in_container,
                prefer_up: false,
                content_layout_id,
            });
        }

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        style.position = Position::Absolute;

        let child_layout_id: Vec<_> = children
            .iter()
            .map(|c: &ChildElementState| c.content_layout_id)
            .collect();

        (
            window.request_layout(style, child_layout_id, cx),
            RequestLayoutState { children },
        )
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let content_bounds = Bounds::new(
            bounds.origin,
            size(window.bounds().size.width, bounds.size.height),
        );

        let mut current_popovers = vec![];
        request_layout.children.retain_mut(
            |ChildElementState {
                 popover_content,
                 pos_in_container: target_position,
                 prefer_up,
                 content_layout_id,
             }| {
                let line_height = self.editor.read(cx).line_height;
                let popover_size = window.layout_bounds(*content_layout_id).size;
                let screen_pos = *target_position + point(px(0.), bounds.top());
                if let Some(placement) = self.place(
                    line_height,
                    popover_size,
                    screen_pos,
                    content_bounds,
                    &current_popovers,
                    *prefer_up,
                ) {
                    // ensure no two popovers overlap
                    let my_bounds = Bounds::new(placement.origin, popover_size);
                    current_popovers.push(my_bounds);
                    popover_content.prepaint_as_root(
                        placement.origin,
                        bounds.size.into(),
                        window,
                        cx,
                    );
                    true
                } else {
                    false
                }
            },
        );

        ()
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        for ChildElementState {
            popover_content, ..
        } in &mut request_layout.children
        {
            popover_content.paint(window, cx);
        }
    }
}
