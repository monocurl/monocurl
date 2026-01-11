use std::{cell::RefCell, rc::Rc};

use gpui::{
    AnyElement, App, Bounds, BoxShadow, Element, ElementId, Entity, FontWeight, GlobalElementId, Hsla, InspectorElementId, InteractiveElement, IntoElement, LayoutId, ParentElement, Pixels, Point, Position, Size, StatefulInteractiveElement, Style, Styled, Window, div, point, prelude::FluentBuilder, px, relative, rgb, size
};
use smallvec::SmallVec;
use structs::text::Location8;
use crate::{
    text_editor::TextEditor, state::{diagnostics::Diagnostic, textual_state::AutoCompleteState}, theme::TextEditorStyles
};

pub struct PopoverElement {
    pub editor: Entity<TextEditor>,
}

struct DiagnosticPopoverState {
    diagnostic: Diagnostic,
    pos_in_container: Point<Pixels>,
}

struct AutoCompletePopoverState {
    autocomplete_state: Rc<RefCell<AutoCompleteState>>,
    pos_in_container: Point<Pixels>,
}

struct ChildElementState(AnyElement, Point<Pixels>, LayoutId);
pub struct RequestLayoutState {
    children: SmallVec<[ChildElementState; 2]>,
}

#[derive(Debug, Clone, Copy)]
struct PopoverPlacement {
    origin: Point<Pixels>,
}

impl PopoverElement {
    fn pos_of_loc(&mut self, editor: &TextEditor, location8: Location8) -> Point<Pixels> {
        let text_area_pos = editor.line_map.point_for_location(location8);
        let editor_pos = editor.text_area_to_editor_pos(text_area_pos);
        let handle= &editor.scroll_handle;
        let scroll = handle.offset()
            .y
            .clamp(-handle.max_offset().height, px(0.0));
        point(
            editor_pos.x,
            editor_pos.y + scroll,
        )
    }

    fn hovered_diagnostic(&mut self, cx: &mut App) -> Option<DiagnosticPopoverState> {
        let editor =  self.editor.read(cx);

        editor
            .hover_item
            .as_ref()
            .map(|(_, d)| {
                let state = editor.state.read(cx);
                let location8 = state.offset8_to_loc8(d.span.start);
                let pos_in_container = self.pos_of_loc(&editor, location8);

                DiagnosticPopoverState {
                    diagnostic: d.clone(),
                    pos_in_container,
                }
            })
    }

    fn autocomplete_state(&mut self, window: &Window, cx: &mut App) -> Option<AutoCompletePopoverState> {
        let editor = self.editor.read(cx);
        let pos = self.pos_of_loc(editor, editor.cursor(cx).anchor);
        let state = editor.state.read(cx);
        if !state.autocomplete_state().borrow_mut().recheck_should_display(state.cursor()) || !editor.focus_handle.is_focused(window) {
            return None;
        }
        Some(AutoCompletePopoverState {
            autocomplete_state: state.autocomplete_state(),
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
    ) -> Option<PopoverPlacement> {

        let below_y = target_position.y + line_height;
        let above_y = target_position.y - popover_size.height;

        let margin = px(8.0);
        let x = target_position.x
            .clamp(
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
            && container_bounds.contains(&point(x, above_y))
            && !other_popovers.iter().any(|other| {
                let my_bounds = Bounds::new(point(x, above_y), popover_size);
                my_bounds.intersects(other)
            });

        let y = if below_fits {
            below_y
        } else if above_fits {
            above_y
        } else {
            return None;
        };

        Some(PopoverPlacement {
            origin: Point { x, y },
        })
    }

    fn build_diagnostic_popover(
        &self,
        diagnostic: &Diagnostic,
    ) -> AnyElement {
        let color = diagnostic.color(&TextEditorStyles::default());
        let padding = px(8.0);
        let margin = px(4.0);
        let max_w = px(400.0);

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
                    .bg(rgb(0xe6e9ee))
                    .rounded_md()
                    .border_1()
                    .border_color(color)
                    .shadow(vec![BoxShadow {
                        offset: Point { x: px(0.), y: px(0.) },
                        blur_radius: px(2.),
                        spread_radius: px(2.),
                        color: Hsla { h: 0., s: 0., l: 0., a: 0.10 },
                    }])
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(0x000000))
                            .child(diagnostic.title.clone())
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x141414))
                            .child(diagnostic.message.clone())
                    )
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
    ) -> AnyElement {
        let base_color = if is_selected {
            rgb(0x000000)
        } else {
            rgb(0x333333)
        };
        let highlight_color = rgb(0xdd3377);

        let mut segments: Vec<(String, bool)> = Vec::new();
        let mut current_segment = String::new();
        let mut current_is_highlighted = false;
        text.char_indices()
            .map(|(i, ch)| (highlight_indices.contains(&i), ch))
            .for_each(|(is_highlighted, ch)| {
                if current_segment.is_empty() {
                    current_segment.push(ch);
                    current_is_highlighted = is_highlighted;
                } else if is_highlighted == current_is_highlighted {
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

        let mut container = div()
            .flex()
            .items_center()
            .text_size(px(14.));

        for (segment_text, is_highlighted) in segments {
            let segment = div()
                .text_color(if is_highlighted { highlight_color } else { base_color })
                .when(is_highlighted, |this| this.font_weight(FontWeight::BOLD))
                .child(segment_text);

            container = container.child(segment);
        }

        container.into_any_element()
    }

    fn build_autocomplete_popover(
        &self,
        autocomplete_state: &Rc<RefCell<AutoCompleteState>>,
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
            .bg(rgb(0xe6e9ee))
            .rounded_md()
            .border_1()
            .border_color(rgb(0xd0d3d8))
            .max_w(max_w)
            .shadow(vec![BoxShadow {
                offset: Point { x: px(0.), y: px(0.) },
                blur_radius: px(2.),
                spread_radius: px(2.),
                color: Hsla { h: 0., s: 0., l: 0., a: 0.10 },
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
                    .children(
                        ac.filtered_items
                            .iter()
                            .map(|(index, highlights)| {
                                let index_copy = *index;
                                let item1 = ac.items[*index].clone();
                                let head = item1.head.clone();
                                let is_selected = *index == ac.selected_index;

                                let ac_copy = autocomplete_state.clone();
                                let editor_copy = self.editor.clone();

                                div()
                                    .child(
                                        div()
                                            .px(item_padding_x)
                                            .py(item_padding_y)
                                            .rounded_sm()
                                            .when(is_selected, |this| {
                                                this.bg(rgb(0xCDCDDF))
                                            })
                                            .when(!is_selected, |this| {
                                                this.bg(rgb(0xe6e9ee))
                                                    .hover(|style| style.bg(rgb(0xd6dae0)))
                                            })
                                            .cursor_pointer()
                                            .on_mouse_down(gpui::MouseButton::Left, move |_, window, cx| {
                                                window.prevent_default();
                                                cx.stop_propagation();
                                                editor_copy.update(cx, |editor, cx| {
                                                    AutoCompleteState::apply_index(&ac_copy, index_copy, editor, editor.state.clone(), window, cx);
                                                });
                                            })
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .child(
                                                        Self::render_highlighted_text(
                                                            &head,
                                                            highlights,
                                                            is_selected
                                                        )
                                                    )
                                            )
                                    )
                            })
                    )
            )
            .into_any_element()
    }
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

        let mut children: SmallVec<_> = SmallVec::new();


        if let Some(ref ac_state) = self.autocomplete_state(window, cx) {
            let mut popover_content = self.build_autocomplete_popover(&ac_state.autocomplete_state);
            let content_layout_id = popover_content.request_layout(window, cx);
            let pos_in_container = ac_state.pos_in_container;

            children.push(ChildElementState(popover_content, pos_in_container, content_layout_id));
        }

        if let Some(ref diag_state) = diagnostic_state {

            let mut popover_content = self.build_diagnostic_popover(&diag_state.diagnostic);
            let content_layout_id = popover_content.request_layout(window, cx);

            children.push(ChildElementState(popover_content, diag_state.pos_in_container, content_layout_id));
        }

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        style.position = Position::Absolute;

        let child_layout_id: Vec<_> = children.iter().map(|c: &ChildElementState| c.2).collect();

        (window.request_layout(style, child_layout_id, cx), RequestLayoutState { children })
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
            size(window.bounds().size.width, bounds.size.height)
        );

        let mut current_popovers = vec![];
        request_layout.children.retain_mut(|ChildElementState(child, target_position, layout_id)| {
            let line_height = self.editor.read(cx).line_height;
            let popover_size = window.layout_bounds(*layout_id).size;
            let screen_pos = *target_position + point(px(0.), bounds.top());
            if let Some(placement) = self.place(line_height, popover_size, screen_pos, content_bounds, &current_popovers) {
                // ensure no two popovers overlap
                let my_bounds = Bounds::new(placement.origin, popover_size);
                current_popovers.push(my_bounds);
                child.prepaint_as_root(placement.origin, bounds.size.into(), window, cx);
                true
            }
            else {
                false
            }
        });

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
        for ChildElementState(child, _, _) in &mut request_layout.children {
            child.paint(window, cx);
        }
    }
}
