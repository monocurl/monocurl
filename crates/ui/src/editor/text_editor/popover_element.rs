use gpui::{
    AnyElement, App, Bounds, BoxShadow, Element, ElementId, Entity, GlobalElementId, Hsla, InspectorElementId, IntoElement, LayoutId, ParentElement, Pixels, Point, Position, Size, Style, Styled, Window, div, point, px, relative, rgb, size
};
use crate::{
    editor::text_editor::TextEditor,
    state::diagnostics::{Diagnostic}, theme::TextEditorStyles
};

pub struct PopoverElement {
    pub editor: Entity<TextEditor>,
}

struct DiagnosticPopoverState {
    diagnostic: Diagnostic,
    pos_in_container: Point<Pixels>,
}

pub struct RequestLayoutState {
    child: Option<(AnyElement, Point<Pixels>, LayoutId)>,
}

#[derive(Debug, Clone, Copy)]
struct PopoverPlacement {
    origin: Point<Pixels>,
}

impl PopoverElement {
    fn hovered_diagnostic(&mut self, cx: &mut App) -> Option<DiagnosticPopoverState> {
        let editor = self.editor.read(cx);
        let Some(loc) = editor.hover_confirmed_position else {
            return None;
        };
        editor
            .state
            .read(cx)
            .diagnostics()
            .diagnostic_for_point(loc)
            .map(|d| {
                let state = editor.state.read(cx);
                let location8 = state.offset8_to_loc8(d.span.start);
                let text_area_pos = editor.line_map.point_for_location(location8);
                let editor_pos = editor.text_area_to_editor_pos(text_area_pos);
                let handle= &editor.scroll_handle;
                let scroll = handle.offset()
                    .y
                    .clamp(-handle.max_offset().height, px(0.0));
                let pos_in_container = point(
                    editor_pos.x,
                    editor_pos.y + scroll,
                );
                DiagnosticPopoverState {
                    diagnostic: d.clone(),
                    pos_in_container,
                }
            })
    }

    fn place(
        line_height: Pixels,
        popover_size: Size<Pixels>,
        target_position: Point<Pixels>,
        container_bounds: Bounds<Pixels>,
    ) -> PopoverPlacement {
        let margin = px(4.0);
        let below_y = target_position.y + line_height + margin;
        let above_y = target_position.y - popover_size.height - margin;

        let mut x = target_position.x;
        x = x.max(container_bounds.left() + margin);
        x = x.min(container_bounds.right() - popover_size.width - margin);

        let space_below = container_bounds.bottom() - (target_position.y + line_height + margin);
        let space_above = target_position.y - margin - container_bounds.top();

        let y = if space_below >= popover_size.height + margin {
            below_y
        } else if space_above >= popover_size.height + margin {
            above_y
        } else if space_below > space_above {
            below_y
        } else {
            above_y
        };

        PopoverPlacement {
            origin: Point { x, y },
        }
    }

    fn build_popover(
        diagnostic: &Diagnostic,
    ) -> AnyElement {
        let color = diagnostic.color(&TextEditorStyles::default());
        let padding = px(8.0);
        let max_w = px(400.0);

        div()
            .flex()
            .max_w(max_w)
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

        let mut child: Option<_> = None;

        if let Some(ref diag_state) = diagnostic_state {

            let mut popover_content = Self::build_popover(&diag_state.diagnostic);
            let content_layout_id = popover_content.request_layout(window, cx);

            child = Some((popover_content, diag_state.pos_in_container, content_layout_id));
        }

        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        style.position = Position::Absolute;

        (window.request_layout(style, child.as_ref().map(|c| c.2), cx), RequestLayoutState { child })
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

        if let Some((child, target_position, layout_id)) = &mut request_layout.child {
            let line_height = self.editor.read(cx).line_height;
            let popover_size = window.layout_bounds(*layout_id).size;
            let screen_pos = *target_position + point(px(0.), bounds.top());
            let placement = Self::place(line_height, popover_size, screen_pos, content_bounds);
            child.prepaint_as_root(placement.origin, bounds.size.into(), window, cx);
        }

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
        if let Some((child, _,  _)) = &mut request_layout.child {
            child.paint(window, cx);
        }
    }
}
