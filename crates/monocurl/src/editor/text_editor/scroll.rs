use super::*;

impl TextEditor {
    pub(super) fn capture_top_visible_line(&mut self) {
        let scroll_y = -self.scroll_handle.offset().y;
        let top_most = self.visible_lines().start;
        let y_range = self.line_map.y_range(top_most..top_most + 1);
        self.resize_anchor_line = Some((top_most, scroll_y - y_range.start));
    }

    pub(super) fn restore_scroll_to_anchor_line(&mut self) {
        if let Some((anchor_line, offset)) = self.resize_anchor_line.take() {
            let target_y = self.line_map.y_range(anchor_line..anchor_line + 1).start + offset;
            let scroll_offset = self.scroll_handle.offset();
            self.scroll_handle
                .set_offset(point(scroll_offset.x, -target_y));
        }
    }

    pub(super) fn discretely_scroll_to_cursor(&mut self, cx: &App) {
        let cursor = self.state.read(cx).cursor();
        let cursor_y = self.line_map.point_for_location(cursor.head).y;

        let scroll_offset = self.scroll_handle.offset();
        let viewport_height = self.scroll_handle.bounds().size.height;

        let visible_top = -scroll_offset.y;
        let visible_bottom = visible_top + viewport_height;

        let margin_height = SCROLL_MARGIN * self.line_height;

        if cursor_y - margin_height < visible_top {
            let new_scroll_y = -(cursor_y - margin_height).max(px(0.0));
            self.scroll_handle
                .set_offset(point(scroll_offset.x, new_scroll_y));
        } else if cursor_y + self.line_height + margin_height > visible_bottom {
            let new_scroll_y = -(cursor_y + self.line_height - viewport_height + margin_height);
            self.scroll_handle
                .set_offset(point(scroll_offset.x, new_scroll_y));
        }
    }

    pub(super) fn start_responding_to_mouse_movements(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(
            async move |editor: WeakEntity<TextEditor>, cx: &mut AsyncApp| {
                loop {
                    cx.background_executor().timer(AUTO_SCROLL_INTERVAL).await;

                    let should_continue = editor
                        .update(cx, |editor, cx| {
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

                                let distance_above =
                                    (viewport_top - mouse_pos.y - px(AUTO_SCROLL_MIN_THRESHOLD))
                                        .max(px(0.0));
                                let distance_below =
                                    (mouse_pos.y - viewport_bottom - px(AUTO_SCROLL_MIN_THRESHOLD))
                                        .max(px(0.0));

                                if distance_above > px(0.0) || distance_below > px(0.0) {
                                    let scroll_offset = editor.scroll_handle.offset();
                                    let distance = distance_above.max(distance_below);

                                    let interpolate = |x: f64| x;
                                    let t = (distance
                                        / (AUTO_SCROLL_MAX_THRESHOLD - AUTO_SCROLL_MIN_THRESHOLD))
                                        .min(px(1.0));
                                    let scroll_speed =
                                        px(interpolate(t.to_f64()) as f32) * AUTO_SCROLL_MAX_SPEED;

                                    let new_scroll_y = if distance_above > px(0.0) {
                                        scroll_offset.y + scroll_speed
                                    } else {
                                        scroll_offset.y - scroll_speed
                                    };

                                    editor
                                        .scroll_handle
                                        .set_offset(point(scroll_offset.x, new_scroll_y));
                                    cx.notify();
                                }
                            }

                            true
                        })
                        .unwrap_or(false);

                    if !should_continue {
                        break;
                    }
                }
            },
        );
        self.auto_scroll_task = Some(task);
    }

    pub(super) fn stop_responding_to_mouse_movements(&mut self) {
        self.auto_scroll_task = None;
    }

    pub(super) fn wrap_width(&self) -> Pixels {
        if self.scroll_handle.bounds().size.width > self.gutter_width + self.right_gutter_width {
            self.scroll_handle.bounds().size.width - self.gutter_width - self.right_gutter_width
        } else if let Some(old_bounds) = self.last_bounds {
            old_bounds.size.width - self.gutter_width - self.right_gutter_width
        } else {
            Pixels::MAX
        }
    }

    pub(super) fn text_area_to_editor_pos(&self, pos: Point<Pixels>) -> Point<Pixels> {
        point(pos.x + self.gutter_width, pos.y)
    }

    pub(super) fn index_for_mouse_position(&self, position: Point<Pixels>) -> Option<Location8> {
        let Some(bounds) = self.last_bounds else {
            return None;
        };

        self.line_map
            .location_for_point(position - point(self.gutter_width, bounds.top()))
            .ok()
    }

    pub(super) fn closest_index_for_mouse_position(&self, position: Point<Pixels>) -> Location8 {
        let Some(bounds) = self.last_bounds else {
            return Location8 { row: 0, col: 0 };
        };

        match self
            .line_map
            .location_for_point(position - point(self.gutter_width, bounds.top()))
        {
            Ok(loc) => loc,
            Err(loc) => loc,
        }
    }
}
