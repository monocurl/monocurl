use gpui::*;
use std::{cell::Cell, rc::Rc};

use crate::theme::Theme;

const HANDLE_SIZE: f32 = 4.0;
const DIVIDER_SIZE: f32 = 1.0;
const MIN_SIZE: f32 = 50.0;

pub struct Split {
    orientation: Axis,
    first: AnyElement,
    second: AnyElement,
    default_flex: f32,
    divider_color: Hsla,
    flex_state: Option<Rc<Cell<f32>>>,
    on_flex_change: Option<Rc<dyn Fn(f32, &mut App)>>,
    min_first_size: Pixels,
    min_second_size: Pixels,
}

impl Split {
    pub fn new(orientation: Axis, first: AnyElement, second: AnyElement) -> Self {
        Self {
            orientation,
            first,
            second,
            default_flex: 0.5,
            divider_color: Theme::light().split_divider.into(),
            flex_state: None,
            on_flex_change: None,
            min_first_size: px(MIN_SIZE),
            min_second_size: px(MIN_SIZE),
        }
    }

    pub fn default_flex(mut self, ratio: f32) -> Self {
        self.default_flex = ratio;
        self
    }

    pub fn divider_color(mut self, color: impl Into<Hsla>) -> Self {
        self.divider_color = color.into();
        self
    }

    pub fn flex_state(mut self, state: Rc<Cell<f32>>) -> Self {
        self.flex_state = Some(state);
        self
    }

    pub fn on_flex_change(mut self, callback: impl Fn(f32, &mut App) + 'static) -> Self {
        self.on_flex_change = Some(Rc::new(callback));
        self
    }

    pub fn min_sizes(mut self, first: Pixels, second: Pixels) -> Self {
        self.min_first_size = first;
        self.min_second_size = second;
        self
    }
}

impl IntoElement for Split {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Split {
    type RequestLayoutState = ();
    type PrepaintState = SplitLayout;

    fn id(&self) -> Option<ElementId> {
        Some("Split".into())
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let style = Style {
            flex_grow: 1.0,
            flex_shrink: 1.0,
            size: Size {
                width: relative(1.0).into(),
                height: relative(1.0).into(),
            },
            ..Default::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let drag_handle = window.with_element_state(id.unwrap(), |state, _| {
            let state = state.unwrap_or_else(|| Rc::new(Cell::new(false)));
            (state.clone(), state)
        });
        let flex_handle = self.flex_state.clone().unwrap_or_else(|| {
            window.with_element_state(id.unwrap(), |state, _| {
                let state = state.unwrap_or_else(|| Rc::new(Cell::new(self.default_flex)));
                (state.clone(), state)
            })
        });

        let is_horizontal = self.orientation == Axis::Horizontal;

        // Get main and cross axis dimensions
        let main_axis = if is_horizontal {
            bounds.size.width
        } else {
            bounds.size.height
        };
        let cross_axis = if is_horizontal {
            bounds.size.height
        } else {
            bounds.size.width
        };
        let (min_first, min_second) =
            scaled_min_sizes(main_axis, self.min_first_size, self.min_second_size);
        let min_flex = (min_first / main_axis).max(0.0);
        let max_flex = (1.0 - min_second / main_axis).min(1.0);
        let flex = flex_handle.get().clamp(min_flex, max_flex);
        let split_pos = main_axis * flex;

        // Helper to create bounds based on orientation
        let make_bounds = |main_offset: Pixels, main_size: Pixels| {
            if is_horizontal {
                Bounds {
                    origin: point(bounds.origin.x + main_offset, bounds.origin.y),
                    size: Size {
                        width: main_size,
                        height: cross_axis,
                    },
                }
            } else {
                Bounds {
                    origin: point(bounds.origin.x, bounds.origin.y + main_offset),
                    size: Size {
                        width: cross_axis,
                        height: main_size,
                    },
                }
            }
        };

        let first_bounds = make_bounds(px(0.0), split_pos);
        let second_bounds = make_bounds(split_pos, main_axis - split_pos);
        let handle_bounds = make_bounds(split_pos - px(HANDLE_SIZE / 2.0), px(HANDLE_SIZE));
        let divider_bounds = make_bounds(split_pos, px(DIVIDER_SIZE));

        self.second
            .prepaint_as_root(second_bounds.origin, second_bounds.size.into(), window, cx);
        self.first
            .prepaint_as_root(first_bounds.origin, first_bounds.size.into(), window, cx);

        SplitLayout {
            container_bounds: bounds,
            handle_hitbox: window.insert_hitbox(handle_bounds, HitboxBehavior::BlockMouse),
            divider_bounds,
            drag_handle,
            flex_handle,
            on_flex_change: self.on_flex_change.clone(),
            axis: self.orientation,
            min_first,
            min_second,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _state: &mut Self::RequestLayoutState,
        layout: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        paint_split_handle(layout, window, cx);

        self.second.paint(window, cx);

        window.paint_quad(fill(layout.divider_bounds, self.divider_color));

        self.first.paint(window, cx);
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }
}

pub struct SplitLayout {
    container_bounds: Bounds<Pixels>,
    handle_hitbox: Hitbox,
    divider_bounds: Bounds<Pixels>,
    flex_handle: Rc<Cell<f32>>,
    on_flex_change: Option<Rc<dyn Fn(f32, &mut App)>>,
    drag_handle: Rc<Cell<bool>>,
    axis: Axis,
    min_first: Pixels,
    min_second: Pixels,
}

fn scaled_min_sizes(main_axis: Pixels, first: Pixels, second: Pixels) -> (Pixels, Pixels) {
    if main_axis <= px(0.0) {
        return (px(0.0), px(0.0));
    }
    let requested = first + second;
    if requested <= main_axis {
        (first, second)
    } else {
        let scale = f32::from(main_axis) / f32::from(requested);
        (first * scale, second * scale)
    }
}

fn paint_split_handle(layout: &mut SplitLayout, window: &mut Window, _cx: &mut App) {
    let cursor_style = match layout.axis {
        Axis::Horizontal => CursorStyle::ResizeColumn,
        Axis::Vertical => CursorStyle::ResizeRow,
    };

    window.set_cursor_style(cursor_style, &layout.handle_hitbox);

    window.on_mouse_event({
        let hitbox = layout.handle_hitbox.clone();
        let is_dragging = layout.drag_handle.clone();
        move |_event: &MouseDownEvent, phase, window, cx| {
            if phase.bubble() && hitbox.is_hovered(window) {
                is_dragging.set(true);
                cx.stop_propagation();
            }
        }
    });

    window.on_mouse_event({
        let bounds = layout.container_bounds;
        let axis = layout.axis;
        let is_dragging = layout.drag_handle.clone();
        let flex = layout.flex_handle.clone();
        let on_flex_change = layout.on_flex_change.clone();
        let min_first = layout.min_first;
        let min_second = layout.min_second;
        move |event: &MouseMoveEvent, phase, window, cx| {
            if phase.bubble() && is_dragging.get() {
                let container_size = bounds.size.along(axis);
                let offset = (event.position - bounds.origin).along(axis);
                let min_px = min_first;
                let max_px = container_size - min_second;

                let clamped_offset = offset.max(min_px).min(max_px);
                let new_flex = (clamped_offset / container_size).clamp(0.0, 1.0);

                flex.set(new_flex);
                if let Some(callback) = &on_flex_change {
                    callback(new_flex, cx);
                }
                window.refresh();
            }
        }
    });

    window.on_mouse_event({
        let drag_handle = layout.drag_handle.clone();
        move |_event: &MouseUpEvent, phase, _window, _cx| {
            if phase.bubble() {
                drag_handle.set(false);
            }
        }
    });
}
