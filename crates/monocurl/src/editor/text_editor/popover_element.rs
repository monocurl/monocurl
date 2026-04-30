use std::{cell::RefCell, rc::Rc, time::Duration};

use crate::{
    state::{
        diagnostics::Diagnostic,
        textual_state::{AutoCompleteState, ParameterHintArg, ParameterPositionState},
    },
    text_editor::TextEditor,
    theme::TextEditorStyles,
};
use gpui::{
    AnyElement, App, AppContext, AsyncApp, Bounds, BoxShadow, ClipboardItem, Element, ElementId,
    Entity, FontWeight, GlobalElementId, InspectorElementId, InteractiveElement, IntoElement,
    LayoutId, ParentElement, Pixels, Point, Position, Size, StatefulInteractiveElement, Style,
    Styled, Window, div, point, prelude::FluentBuilder, px, relative, size,
};
use smallvec::SmallVec;
use structs::text::Location8;

mod autocomplete;
mod diagnostic;
mod parameter_hint;

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
            prefer_up: hint.prefer_up,
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
        let min_x = container_bounds.left() + margin;
        let max_x = container_bounds.right() - popover_size.width - margin;
        let x = if max_x < min_x {
            min_x
        } else {
            target_position.x.clamp(min_x, max_x)
        };

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
    prefer_up: bool,
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
                prefer_up: parameter_hint.prefer_up,
                content_layout_id,
            });
        }

        if let Some(ref diag_state) = diagnostic_state {
            let is_copied = self.is_diagnostic_copied(&diag_state.diagnostic, cx);
            let mut popover_content =
                self.build_diagnostic_popover(&diag_state.diagnostic, &styles, is_copied);
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
