use executor::time::Timestamp;
use gpui::*;

use crate::{
    actions::{ZoomIn, ZoomOut},
    services::ServiceManager,
    theme::ThemeSettings,
};

use super::{
    metrics::{
        DEFAULT_ZOOM_IDX, MIN_GAP, PX_PER_SEC, SLIDE_W, ZOOM_LEVELS, compute_gap_ws,
        compute_playhead_x, compute_slide_xs, effective_durations,
    },
    toolbar::render_toolbar,
    track::render_track,
};

pub struct Timeline {
    pub(super) services: Entity<ServiceManager>,
    pub(super) scroll: ScrollHandle,
    pub(super) zoom_idx: usize,
}

impl Timeline {
    pub fn new(services: Entity<ServiceManager>, cx: &mut Context<Self>) -> Self {
        let execution_state = services.read(cx).execution_state().clone();
        cx.observe(&execution_state, |this, _, cx| {
            this.recenter_playhead_if_needed(cx);
            cx.notify();
        })
        .detach();
        cx.observe_global::<ThemeSettings>(|_this, cx| {
            cx.notify();
        })
        .detach();

        Self {
            services,
            scroll: ScrollHandle::new(),
            zoom_idx: DEFAULT_ZOOM_IDX,
        }
    }

    pub(super) fn zoom_factor(&self) -> f32 {
        ZOOM_LEVELS[self.zoom_idx] as f32 / 100.0
    }

    pub fn zoom_in(&mut self, _: &ZoomIn, _w: &mut Window, cx: &mut Context<Self>) {
        self.zoom_idx = (self.zoom_idx + 1).min(ZOOM_LEVELS.len() - 1);
        cx.notify();
    }

    pub fn zoom_out(&mut self, _: &ZoomOut, _w: &mut Window, cx: &mut Context<Self>) {
        self.zoom_idx = self.zoom_idx.saturating_sub(1);
        cx.notify();
    }

    fn recenter_playhead_if_needed(&mut self, cx: &mut Context<Self>) {
        let exec_state = self.services.read(cx).execution_state().clone();
        let exec = exec_state.read(cx);
        if exec.slide_count == 0 {
            return;
        }

        let viewport_width = f32::from(self.scroll.bounds().size.width);
        if viewport_width <= 0.0 {
            return;
        }

        let zoom = self.zoom_factor();
        let effective = effective_durations(
            exec.slide_count,
            &exec.slide_durations,
            &exec.minimum_slide_durations,
            exec.current_timestamp.slide,
            exec.current_timestamp.time,
        );
        let slide_xs = compute_slide_xs(exec.slide_count, &effective, zoom);
        let gap_ws = compute_gap_ws(exec.slide_count, &effective, zoom);
        let playhead_x = compute_playhead_x(
            exec.current_timestamp.slide,
            exec.current_timestamp.time,
            &slide_xs,
            &gap_ws,
            zoom,
        );

        let scroll_offset = self.scroll.offset();
        let visible_left = -f32::from(scroll_offset.x);
        let visible_right = visible_left + viewport_width;
        if playhead_x >= visible_left && playhead_x <= visible_right {
            return;
        }

        let max_x = f32::from(self.scroll.max_offset().width).max(0.0);
        let centered_left = (playhead_x - viewport_width * 0.5).clamp(0.0, max_x);
        self.scroll
            .set_offset(point(px(-centered_left), scroll_offset.y));
    }
}

impl Render for Timeline {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let exec = self.services.read(cx).execution_state().read(cx);
        let current_slide = exec.current_timestamp.slide;
        let current_time = exec.current_timestamp.time;
        let is_playing = exec.is_playing();
        let slide_count = exec.slide_count;
        let durations = exec.slide_durations.clone();
        let minimum_durations = exec.minimum_slide_durations.clone();
        let theme = ThemeSettings::theme(cx);
        let zoom = self.zoom_factor();
        let effective_for_seek = effective_durations(
            slide_count,
            &durations,
            &minimum_durations,
            current_slide,
            current_time,
        );
        let slide_xs = compute_slide_xs(slide_count, &effective_for_seek, zoom);
        let gap_ws = compute_gap_ws(slide_count, &effective_for_seek, zoom);

        let toolbar = render_toolbar(
            self,
            is_playing,
            current_slide,
            slide_count,
            current_time,
            cx,
        );
        let track = render_track(
            current_slide,
            current_time,
            slide_count,
            durations,
            minimum_durations,
            zoom,
            theme,
        );

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(theme.timeline_background)
            .child(toolbar)
            .child(
                div()
                    .id("tl-scroll")
                    .flex()
                    .flex_1()
                    .overflow_x_scroll()
                    .track_scroll(&self.scroll)
                    .on_mouse_down(MouseButton::Left, {
                        let services = self.services.downgrade();
                        let scroll = self.scroll.clone();
                        move |event, _window, cx| {
                            let bounds = scroll.bounds();
                            let scroll_offset = scroll.offset();
                            let local_x =
                                f32::from(event.position.x - bounds.origin.x - scroll_offset.x);

                            for i in 0..slide_count {
                                let bx = slide_xs[i];
                                let gw = gap_ws[i];
                                if local_x >= bx && local_x < bx + SLIDE_W {
                                    services
                                        .update(cx, |s, _| s.seek_to(Timestamp::new(i, 0.0)))
                                        .ok();
                                    return;
                                }
                                let gap_start = bx + SLIDE_W;
                                if local_x >= gap_start && local_x < gap_start + gw.max(MIN_GAP) {
                                    let t = ((local_x - gap_start) / (PX_PER_SEC * zoom)) as f64;
                                    services
                                        .update(cx, |s, _| s.seek_to(Timestamp::new(i, t)))
                                        .ok();
                                    return;
                                }
                            }
                        }
                    })
                    .child(track),
            )
    }
}
