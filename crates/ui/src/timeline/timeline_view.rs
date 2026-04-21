use executor::time::Timestamp;
use gpui::*;

use crate::{
    actions::{ZoomIn, ZoomOut},
    services::ServiceManager,
    theme::{FontSet, ThemeSettings},
};

// layout
const SLIDE_W: f32 = 80.0;
const SLIDE_H: f32 = 60.0;
const TOOLBAR_H: f32 = 30.0;
const PADDING_H: f32 = 16.0;
const PADDING_V: f32 = 16.0;
const LABEL_GAP: f32 = 5.0;
const LABEL_LINE_H: f32 = 13.0;
const CONTENT_H: f32 = PADDING_V + SLIDE_H + LABEL_GAP + LABEL_LINE_H + PADDING_V;
const LABEL_FONT_SIZE: f32 = 11.0;
const DUR_FONT_SIZE: f32 = 9.0;
const PX_PER_SEC: f32 = 40.0;
const MIN_GAP: f32 = 24.0;

const ZOOM_LEVELS: [u32; 9] = [25, 50, 75, 100, 150, 200, 300, 400, 800];
const DEFAULT_ZOOM_IDX: usize = 3;

// --- geometry helpers ---

fn gap_w(duration: Option<f64>, zoom: f32) -> f32 {
    duration.map_or(MIN_GAP, |d| (d as f32 * PX_PER_SEC * zoom).max(MIN_GAP))
}

fn painted_gap_w(duration: Option<f64>, zoom: f32) -> f32 {
    duration.map_or(0.0, |d| (d as f32 * PX_PER_SEC * zoom).max(0.0))
}

fn effective_durations(
    slide_count: usize,
    durations: &[Option<f64>],
    minimum_durations: &[Option<f64>],
    current_slide: usize,
    current_time: f64,
) -> Vec<Option<f64>> {
    (0..slide_count)
        .map(|i| {
            let cached = durations.get(i).and_then(|d| *d);
            let minimum = minimum_durations.get(i).and_then(|d| *d);
            let inferred = if i == current_slide && current_time > 0.0 {
                Some(current_time)
            } else {
                None
            };
            cached
                .or(minimum)
                .map(|d| inferred.map_or(d, |t| d.max(t)))
                .or(inferred)
        })
        .collect()
}

fn compute_slide_xs(slide_count: usize, durations: &[Option<f64>], zoom: f32) -> Vec<f32> {
    let mut xs = Vec::with_capacity(slide_count);
    let mut x = PADDING_H;
    for i in 0..slide_count {
        xs.push(x);
        x += SLIDE_W + gap_w(durations.get(i).and_then(|d| *d), zoom);
    }
    xs
}

fn compute_gap_ws(slide_count: usize, durations: &[Option<f64>], zoom: f32) -> Vec<f32> {
    (0..slide_count)
        .map(|i| gap_w(durations.get(i).and_then(|d| *d), zoom))
        .collect()
}

fn compute_painted_gap_ws(slide_count: usize, durations: &[Option<f64>], zoom: f32) -> Vec<f32> {
    (0..slide_count)
        .map(|i| painted_gap_w(durations.get(i).and_then(|d| *d), zoom))
        .collect()
}

fn compute_track_width(slide_count: usize, durations: &[Option<f64>], zoom: f32) -> f32 {
    if slide_count == 0 {
        return 200.0;
    }
    let slide_xs = compute_slide_xs(slide_count, durations, zoom);
    let last = slide_count - 1;
    slide_xs[last] + SLIDE_W + gap_w(durations.get(last).and_then(|d| *d), zoom) + PADDING_H
}

fn compute_playhead_x(
    current_slide: usize,
    current_time: f64,
    slide_xs: &[f32],
    gap_ws: &[f32],
    zoom: f32,
) -> f32 {
    let x = slide_xs.get(current_slide).copied().unwrap_or(PADDING_H);
    let gap = gap_ws.get(current_slide).copied().unwrap_or(MIN_GAP);
    let time_px = ((current_time as f32) * PX_PER_SEC * zoom).min(gap);
    x + SLIDE_W + time_px
}

// --- prepaint state for the track canvas ---

struct TrackPrepaint {
    slide_xs: Vec<f32>,
    gap_ws: Vec<f32>,
    painted_gap_ws: Vec<f32>,
    playhead_x: f32,
    // effective display duration per slide (may be inferred from current_time)
    durations: Vec<Option<f64>>,
    // true if the duration was explicitly provided (determines border color)
    explicit: Vec<bool>,
    // vertical offset to center content when canvas is taller than CONTENT_H
    vert_offset: f32,
    dur_texts: Vec<ShapedLine>,
    label_texts: Vec<ShapedLine>,
}

// --- main struct ---

pub struct Timeline {
    services: Entity<ServiceManager>,
    scroll: ScrollHandle,
    pub zoom_idx: usize,
}

impl Timeline {
    pub fn new(services: Entity<ServiceManager>, cx: &mut Context<Self>) -> Self {
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

    fn zoom_factor(&self) -> f32 {
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
}

// --- render helpers ---

impl Timeline {
    fn render_toolbar(
        &self,
        is_playing: bool,
        has_error: bool,
        current_slide: usize,
        slide_count: usize,
        current_time: f64,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let status_dot = if has_error {
            theme.timeline_status_error
        } else {
            theme.timeline_status_ok
        };
        let svc = self.services.downgrade();
        let this = cx.weak_entity();
        let zoom_pct = ZOOM_LEVELS[self.zoom_idx];

        let nav_btn = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .w(px(22.0))
                .h(px(22.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.timeline_subtext)
                .text_size(px(11.0))
                .cursor_pointer()
                .hover(|s| s.opacity(0.6))
                .child(label)
        };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(TOOLBAR_H))
            .bg(theme.timeline_toolbar_background)
            .border_b(px(0.5))
            .border_color(theme.timeline_divider)
            .child({
                let svc = svc.clone();
                nav_btn("tl-scene-start", "⏮").on_click(move |_, _, cx| {
                    svc.update(cx, |s, _| s.scene_start()).ok();
                })
            })
            .child({
                let svc = svc.clone();
                nav_btn("tl-prev-slide", "⏪").on_click(move |_, _, cx| {
                    svc.update(cx, |s, cx| s.prev_slide(cx)).ok();
                })
            })
            .child(
                div()
                    .id("tl-play")
                    .w(px(22.0))
                    .h(px(22.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.timeline_text)
                    .text_size(px(11.0))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.6))
                    .child(if is_playing { "▐▐" } else { "▶" })
                    .on_click({
                        let svc = svc.clone();
                        move |_, _, cx| {
                            svc.update(cx, |s, _| s.toggle_play()).ok();
                        }
                    }),
            )
            .child({
                let svc = svc.clone();
                nav_btn("tl-next-slide", "⏩").on_click(move |_, _, cx| {
                    svc.update(cx, |s, cx| s.next_slide(cx)).ok();
                })
            })
            .child({
                nav_btn("tl-scene-end", "⏭").on_click(move |_, _, cx| {
                    svc.update(cx, |s, cx| s.scene_end(cx)).ok();
                })
            })
            .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(status_dot))
            .child(
                div()
                    .text_color(theme.timeline_text)
                    .text_size(px(11.0))
                    .child(format!(
                        "Slide {} / {}",
                        (current_slide + 1).min(slide_count),
                        slide_count
                    )),
            )
            .child(
                div()
                    .text_color(theme.timeline_subtext)
                    .text_size(px(11.0))
                    .child(format!("{:.2}s", current_time)),
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("tl-zoom-out")
                    .w(px(20.0))
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.timeline_subtext)
                    .text_size(px(14.0))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.6))
                    .child("−")
                    .on_click({
                        let this = this.clone();
                        move |_, w, cx| {
                            this.update(cx, |tl, cx| tl.zoom_out(&ZoomOut, w, cx)).ok();
                        }
                    }),
            )
            .child(
                div()
                    .text_color(theme.timeline_subtext)
                    .text_size(px(10.0))
                    .child(format!("{}%", zoom_pct))
                    .w(px(36.0))
                    .flex()
                    .justify_center(),
            )
            .child(
                div()
                    .id("tl-zoom-in")
                    .w(px(20.0))
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(theme.timeline_subtext)
                    .text_size(px(14.0))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.6))
                    .child("+")
                    .on_click({
                        move |_, w, cx| {
                            this.update(cx, |tl, cx| tl.zoom_in(&ZoomIn, w, cx)).ok();
                        }
                    }),
            )
    }

    fn render_track(
        services: WeakEntity<ServiceManager>,
        current_slide: usize,
        current_time: f64,
        slide_count: usize,
        durations: Vec<Option<f64>>,
        minimum_durations: Vec<Option<f64>>,
        zoom: f32,
        theme: crate::theme::Theme,
    ) -> impl IntoElement {
        let effective_for_width = effective_durations(
            slide_count,
            &durations,
            &minimum_durations,
            current_slide,
            current_time,
        );
        let track_w = compute_track_width(slide_count, &effective_for_width, zoom);
        let font = font(FontSet::UI);

        let track = canvas(
            {
                let durations = durations.clone();
                let minimum_durations = minimum_durations.clone();
                let font = font.clone();
                move |bounds, window, _cx| {
                    let effective = effective_durations(
                        slide_count,
                        &durations,
                        &minimum_durations,
                        current_slide,
                        current_time,
                    );
                    let explicit: Vec<bool> = durations.iter().map(|d| d.is_some()).collect();

                    let slide_xs = compute_slide_xs(slide_count, &effective, zoom);
                    let gap_ws = compute_gap_ws(slide_count, &effective, zoom);
                    let painted_gap_ws = compute_painted_gap_ws(slide_count, &effective, zoom);
                    let playhead_x =
                        compute_playhead_x(current_slide, current_time, &slide_xs, &gap_ws, zoom);
                    let vert_offset = (f32::from(bounds.size.height) - CONTENT_H).max(0.0) / 2.0;

                    let ts = window.text_system();
                    let make_run = |text: &str, color: Rgba| TextRun {
                        len: text.len(),
                        font: font.clone(),
                        color: color.into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };

                    let dur_texts = (0..slide_count)
                        .map(|i| {
                            let s = effective
                                .get(i)
                                .and_then(|d| *d)
                                .map(|d| format!("{:.2}s", d))
                                .unwrap_or_else(|| "—".to_string());
                            ts.shape_line(
                                SharedString::from(s.clone()),
                                px(DUR_FONT_SIZE),
                                &[make_run(&s, theme.timeline_subtext)],
                                None,
                            )
                        })
                        .collect();

                    let label_texts = (0..slide_count)
                        .map(|i| {
                            let s = format!("Slide {}", i + 1);
                            ts.shape_line(
                                SharedString::from(s.clone()),
                                px(LABEL_FONT_SIZE),
                                &[make_run(&s, theme.timeline_subtext)],
                                None,
                            )
                        })
                        .collect();

                    TrackPrepaint {
                        slide_xs,
                        gap_ws,
                        painted_gap_ws,
                        playhead_x,
                        durations: effective,
                        explicit,
                        vert_offset,
                        dur_texts,
                        label_texts,
                    }
                }
            },
            move |bounds, prepaint, window, cx| {
                let TrackPrepaint {
                    slide_xs,
                    gap_ws,
                    painted_gap_ws,
                    playhead_x,
                    durations,
                    explicit,
                    vert_offset,
                    dur_texts,
                    label_texts,
                } = prepaint;
                let ox = bounds.origin.x;
                // oy_full: top of canvas (for playhead spanning full height)
                // oy: content origin, offset to vertically center slides
                let oy_full = bounds.origin.y;
                let oy = oy_full + px(vert_offset);
                let seek_hitbox = window.insert_hitbox(
                    Bounds::new(
                        point(ox + px(PADDING_H), oy),
                        size(
                            px((f32::from(bounds.size.width) - 2.0 * PADDING_H).max(0.0)),
                            px(CONTENT_H.min(f32::from(bounds.size.height))),
                        ),
                    ),
                    HitboxBehavior::Normal,
                );

                let line_y = PADDING_V + SLIDE_H / 2.0;
                let sec_px = PX_PER_SEC * zoom;

                // connectors: horizontal line, leading tick, and per-second stubs
                for i in 0..slide_count {
                    let gap_x = slide_xs[i] + SLIDE_W;
                    let painted_gw = painted_gap_ws[i];

                    // horizontal connector line
                    if painted_gw > 0.0 {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(ox + px(gap_x), oy + px(line_y - 0.5)),
                                size(px(painted_gw), px(1.0)),
                            ),
                            theme.timeline_connector,
                        ));
                    }

                    // leading tick at start of gap
                    window.paint_quad(fill(
                        Bounds::new(
                            point(ox + px(gap_x), oy + px(line_y - 4.0)),
                            size(px(1.5), px(8.0)),
                        ),
                        theme.timeline_tick,
                    ));

                    // per-second stub marks
                    let duration_secs = durations.get(i).and_then(|d| *d).unwrap_or(0.0);
                    let num_marks = duration_secs.floor() as usize;
                    for sec in 1..=num_marks {
                        let mark_x = gap_x + sec as f32 * sec_px;
                        if mark_x < gap_x + painted_gw {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(ox + px(mark_x - 1.0), oy + px(line_y - 3.0)),
                                    size(px(2.0), px(6.0)),
                                ),
                                theme.timeline_tick,
                            ));
                        }
                    }
                }

                // slide boxes with duration text at top and label below
                for i in 0..slide_count {
                    let bx = slide_xs[i];
                    let border_color: Hsla = if explicit[i] {
                        theme.timeline_active_border
                    } else {
                        theme.timeline_inactive_border
                    }
                    .into();
                    let box_bounds = Bounds::new(
                        point(ox + px(bx), oy + px(PADDING_V)),
                        size(px(SLIDE_W), px(SLIDE_H)),
                    );
                    window.paint_quad(quad(
                        box_bounds,
                        px(5.0),
                        theme.timeline_slide_background,
                        px(1.5),
                        border_color,
                        BorderStyle::Solid,
                    ));

                    // duration: small text pinned to the top of the box
                    if let Some(shaped) = dur_texts.get(i) {
                        let tx = bx + (SLIDE_W - f32::from(shaped.width)) / 2.0;
                        let ty = PADDING_V + 5.0;
                        let _ = shaped.paint(
                            point(ox + px(tx), oy + px(ty)),
                            px(DUR_FONT_SIZE + 2.0),
                            window,
                            cx,
                        );
                    }

                    // "Slide N" label below box
                    if let Some(shaped) = label_texts.get(i) {
                        let tx = bx + (SLIDE_W - f32::from(shaped.width)) / 2.0;
                        let ty = PADDING_V + SLIDE_H + LABEL_GAP;
                        let _ = shaped.paint(
                            point(ox + px(tx), oy + px(ty)),
                            px(LABEL_FONT_SIZE + 2.0),
                            window,
                            cx,
                        );
                    }
                }

                // playhead: 2px vertical line spanning full canvas height
                window.paint_quad(fill(
                    Bounds::new(
                        point(ox + px(playhead_x - 0.75), oy_full),
                        size(px(1.5), bounds.size.height),
                    ),
                    theme.timeline_playhead,
                ));

                // click-to-seek
                let slide_xs_c = slide_xs.clone();
                let gap_ws_c = gap_ws.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                    if phase != DispatchPhase::Bubble
                        || event.button != MouseButton::Left
                        || !seek_hitbox.is_hovered(window)
                    {
                        return;
                    }
                    let local_x = f32::from(event.position.x - bounds.origin.x);
                    for i in 0..slide_count {
                        let bx = slide_xs_c[i];
                        let gw = gap_ws_c[i];
                        if local_x >= bx && local_x < bx + SLIDE_W {
                            services
                                .update(cx, |s, _| s.seek_to(Timestamp::new(i, 0.0)))
                                .ok();
                            cx.stop_propagation();
                            return;
                        }
                        let gap_start = bx + SLIDE_W;
                        if local_x >= gap_start && local_x < gap_start + gw {
                            let t = ((local_x - gap_start) / (PX_PER_SEC * zoom)) as f64;
                            services
                                .update(cx, |s, _| s.seek_to(Timestamp::new(i, t)))
                                .ok();
                            cx.stop_propagation();
                            return;
                        }
                    }
                });
            },
        )
        .w(px(track_w))
        .h_full();

        // flex_none + explicit width: prevents shrinking so overflow_x_scroll works.
        // h_full: canvas fills the scroll container height so the playhead spans it.
        div().flex_none().w(px(track_w)).h_full().child(track)
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
        let has_error = exec.has_error();
        let theme = ThemeSettings::theme(cx);

        let toolbar = self.render_toolbar(
            is_playing,
            has_error,
            current_slide,
            slide_count,
            current_time,
            cx,
        );
        let track = Self::render_track(
            self.services.downgrade(),
            current_slide,
            current_time,
            slide_count,
            durations,
            minimum_durations,
            self.zoom_factor(),
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
                    .child(track),
            )
    }
}
