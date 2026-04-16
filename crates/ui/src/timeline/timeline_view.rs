use executor::time::Timestamp;
use gpui::*;

use crate::{
    actions::{ZoomIn, ZoomOut},
    services::ServiceManager,
    theme::{ColorSet, FontSet},
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

const BG: Hsla            = Hsla { h: 0.61, s: 0.21, l: 0.87, a: 1.0 };
const TOOLBAR_BG: Rgba    = ColorSet::SIDE_PANEL_GRAY;  // #E6E9EE — matches app toolbar tone
const SLIDE_BG: Rgba      = ColorSet::TOOLBAR_GRAY;     // #D3D7E1 — slightly darker than bg
const ACTIVE_BORDER: Rgba = Rgba { r: 0.42, g: 0.58, b: 0.82, a: 1.0 }; // steel blue, toned down
const INACTIVE_BORDER: Rgba = Rgba { r: 0.54, g: 0.55, b: 0.60, a: 1.0 }; // medium gray
const CONNECTOR_COLOR: Rgba = Rgba { r: 0.780, g: 0.788, b: 0.808, a: 1.0 }; // mid-gray
const TICK_COLOR: Rgba    = Rgba { r: 0.620, g: 0.630, b: 0.660, a: 1.0 }; // darker tick
const TEXT: Rgba          = Rgba { r: 0.298, g: 0.310, b: 0.412, a: 1.0 }; // #4C4F69
const SUBTEXT: Rgba       = Rgba { r: 0.424, g: 0.435, b: 0.522, a: 1.0 }; // #6C6F85
const DIVIDER: Rgba       = ColorSet::LIGHT_GRAY;       // #DDE0E7
const ERROR_DOT: Rgba     = Rgba { r: 0.824, g: 0.059, b: 0.224, a: 1.0 }; // red
const OK_DOT: Rgba        = Rgba { r: 0.090, g: 0.573, b: 0.600, a: 1.0 }; // teal
const PLAYHEAD_COLOR: Rgba = Rgba { r: 0.28, g: 0.3, b: 0.3, a: 1.0 };

// --- geometry helpers ---

fn gap_w(duration: Option<f64>, zoom: f32) -> f32 {
    duration.map_or(MIN_GAP, |d| (d as f32 * PX_PER_SEC * zoom).max(MIN_GAP))
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

fn compute_track_width(slide_count: usize, durations: &[Option<f64>], zoom: f32) -> f32 {
    if slide_count == 0 { return 200.0; }
    let slide_xs = compute_slide_xs(slide_count, durations, zoom);
    let last = slide_count - 1;
    slide_xs[last] + SLIDE_W + gap_w(durations.get(last).and_then(|d| *d), zoom) + PADDING_H
}

fn compute_playhead_x(current_slide: usize, current_time: f64, slide_xs: &[f32], gap_ws: &[f32], zoom: f32) -> f32 {
    let x = slide_xs.get(current_slide).copied().unwrap_or(PADDING_H);
    let gap = gap_ws.get(current_slide).copied().unwrap_or(MIN_GAP);
    let time_px = ((current_time as f32) * PX_PER_SEC * zoom).min(gap);
    x + SLIDE_W + time_px
}

// --- prepaint state for the track canvas ---

struct TrackPrepaint {
    slide_xs: Vec<f32>,
    gap_ws: Vec<f32>,
    playhead_x: f32,
    durations: Vec<Option<f64>>,
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
    pub fn new(services: Entity<ServiceManager>, _cx: &mut Context<Self>) -> Self {
        Self { services, scroll: ScrollHandle::new(), zoom_idx: DEFAULT_ZOOM_IDX }
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
        let status_dot = if has_error { ERROR_DOT } else { OK_DOT };
        let svc = self.services.downgrade();
        let this = cx.weak_entity();
        let zoom_pct = ZOOM_LEVELS[self.zoom_idx];

        let nav_btn = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .w(px(22.0)).h(px(22.0))
                .flex().items_center().justify_center()
                .text_color(SUBTEXT).text_size(px(11.0))
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
            .bg(TOOLBAR_BG)
            .border_b(px(0.5))
            .border_color(DIVIDER)
            .child({
                let svc = svc.clone();
                nav_btn("tl-scene-start", "⏮")
                    .on_click(move |_, _, cx| { svc.update(cx, |s, _| s.scene_start()).ok(); })
            })
            .child({
                let svc = svc.clone();
                nav_btn("tl-prev-slide", "⏪")
                    .on_click(move |_, _, cx| { svc.update(cx, |s, cx| s.prev_slide(cx)).ok(); })
            })
            .child(
                div()
                    .id("tl-play")
                    .w(px(22.0)).h(px(22.0))
                    .flex().items_center().justify_center()
                    .text_color(TEXT).text_size(px(11.0))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.6))
                    .child(if is_playing { "▐▐" } else { "▶" })
                    .on_click({
                        let svc = svc.clone();
                        move |_, _, cx| { svc.update(cx, |s, _| s.toggle_play()).ok(); }
                    })
            )
            .child({
                let svc = svc.clone();
                nav_btn("tl-next-slide", "⏩")
                    .on_click(move |_, _, cx| { svc.update(cx, |s, cx| s.next_slide(cx)).ok(); })
            })
            .child({
                nav_btn("tl-scene-end", "⏭")
                    .on_click(move |_, _, cx| { svc.update(cx, |s, cx| s.scene_end(cx)).ok(); })
            })
            .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(status_dot))
            .child(
                div().text_color(TEXT).text_size(px(11.0))
                    .child(format!("Slide {} / {}", (current_slide + 1).min(slide_count), slide_count))
            )
            .child(
                div().text_color(SUBTEXT).text_size(px(11.0))
                    .child(format!("{:.2}s", current_time))
            )
            .child(div().flex_1())
            .child(
                div()
                    .id("tl-zoom-out")
                    .w(px(20.0)).h(px(20.0))
                    .flex().items_center().justify_center()
                    .text_color(SUBTEXT).text_size(px(14.0))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.6))
                    .child("−")
                    .on_click({
                        let this = this.clone();
                        move |_, w, cx| {
                            this.update(cx, |tl, cx| tl.zoom_out(&ZoomOut, w, cx)).ok();
                        }
                    })
            )
            .child(
                div().text_color(SUBTEXT).text_size(px(10.0))
                    .child(format!("{}%", zoom_pct))
                    .w(px(36.0))
                    .flex().justify_center()
            )
            .child(
                div()
                    .id("tl-zoom-in")
                    .w(px(20.0)).h(px(20.0))
                    .flex().items_center().justify_center()
                    .text_color(SUBTEXT).text_size(px(14.0))
                    .cursor_pointer()
                    .hover(|s| s.opacity(0.6))
                    .child("+")
                    .on_click({
                        move |_, w, cx| {
                            this.update(cx, |tl, cx| tl.zoom_in(&ZoomIn, w, cx)).ok();
                        }
                    })
            )
    }

    fn render_track(
        services: WeakEntity<ServiceManager>,
        current_slide: usize,
        current_time: f64,
        slide_count: usize,
        durations: Vec<Option<f64>>,
        zoom: f32,
    ) -> impl IntoElement {
        let track_w = compute_track_width(slide_count, &durations, zoom);
        let font = font(FontSet::UI);

        let track = canvas(
            {
                let durations = durations.clone();
                let font = font.clone();
                move |bounds, window, _cx| {
                    let slide_xs = compute_slide_xs(slide_count, &durations, zoom);
                    let gap_ws = compute_gap_ws(slide_count, &durations, zoom);
                    let playhead_x = compute_playhead_x(current_slide, current_time, &slide_xs, &gap_ws, zoom);
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

                    let dur_texts = (0..slide_count).map(|i| {
                        let s = durations.get(i).and_then(|d| *d)
                            .map(|d| format!("{:.2}s", d))
                            .unwrap_or_else(|| "—".to_string());
                        ts.shape_line(SharedString::from(s.clone()), px(DUR_FONT_SIZE), &[make_run(&s, SUBTEXT)], None)
                    }).collect();

                    let label_texts = (0..slide_count).map(|i| {
                        let s = format!("Slide {}", i + 1);
                        ts.shape_line(SharedString::from(s.clone()), px(LABEL_FONT_SIZE), &[make_run(&s, SUBTEXT)], None)
                    }).collect();

                    TrackPrepaint { slide_xs, gap_ws, playhead_x, durations, vert_offset, dur_texts, label_texts }
                }
            },
            move |bounds, prepaint, window, cx| {
                let TrackPrepaint { slide_xs, gap_ws, playhead_x, durations, vert_offset, dur_texts, label_texts } = prepaint;
                let ox = bounds.origin.x;
                // oy_full: top of canvas (for playhead spanning full height)
                // oy: content origin, offset to vertically center slides
                let oy_full = bounds.origin.y;
                let oy = oy_full + px(vert_offset);

                let line_y = PADDING_V + SLIDE_H / 2.0;
                let sec_px = PX_PER_SEC * zoom;

                // connectors: horizontal line, leading tick, and per-second stubs
                for i in 0..slide_count {
                    let gap_x = slide_xs[i] + SLIDE_W;
                    let gw = gap_ws[i];

                    // horizontal connector line
                    window.paint_quad(fill(
                        Bounds::new(
                            point(ox + px(gap_x), oy + px(line_y - 0.5)),
                            size(px(gw), px(1.0)),
                        ),
                        CONNECTOR_COLOR,
                    ));

                    // leading tick at start of gap
                    window.paint_quad(fill(
                        Bounds::new(
                            point(ox + px(gap_x), oy + px(line_y - 4.0)),
                            size(px(1.5), px(8.0)),
                        ),
                        TICK_COLOR,
                    ));

                    // per-second stub marks
                    let duration_secs = durations.get(i).and_then(|d| *d).unwrap_or(0.0);
                    let num_marks = duration_secs.floor() as usize;
                    for sec in 1..=num_marks {
                        let mark_x = gap_x + sec as f32 * sec_px;
                        if mark_x < gap_x + gw {
                            window.paint_quad(fill(
                                Bounds::new(
                                    point(ox + px(mark_x - 0.5), oy + px(line_y - 3.0)),
                                    size(px(1.0), px(6.0)),
                                ),
                                TICK_COLOR,
                            ));
                        }
                    }
                }

                // slide boxes with duration text at top and label below
                for i in 0..slide_count {
                    let bx = slide_xs[i];
                    let border_color: Hsla = if i <= current_slide { ACTIVE_BORDER } else { INACTIVE_BORDER }.into();
                    let box_bounds = Bounds::new(
                        point(ox + px(bx), oy + px(PADDING_V)),
                        size(px(SLIDE_W), px(SLIDE_H)),
                    );
                    window.paint_quad(quad(
                        box_bounds,
                        px(5.0),
                        SLIDE_BG,
                        px(1.5),
                        border_color,
                        BorderStyle::Solid,
                    ));

                    // duration: small text pinned to the top of the box
                    if let Some(shaped) = dur_texts.get(i) {
                        let tx = bx + (SLIDE_W - f32::from(shaped.width)) / 2.0;
                        let ty = PADDING_V + 5.0;
                        let _ = shaped.paint(point(ox + px(tx), oy + px(ty)), px(DUR_FONT_SIZE + 2.0), window, cx);
                    }

                    // "Slide N" label below box
                    if let Some(shaped) = label_texts.get(i) {
                        let tx = bx + (SLIDE_W - f32::from(shaped.width)) / 2.0;
                        let ty = PADDING_V + SLIDE_H + LABEL_GAP;
                        let _ = shaped.paint(point(ox + px(tx), oy + px(ty)), px(LABEL_FONT_SIZE + 2.0), window, cx);
                    }
                }

                // playhead: 2px vertical line spanning full canvas height
                window.paint_quad(fill(
                    Bounds::new(
                        point(ox + px(playhead_x - 1.0), oy_full),
                        size(px(2.0), bounds.size.height),
                    ),
                    PLAYHEAD_COLOR,
                ));

                // click-to-seek
                let slide_xs_c = slide_xs.clone();
                let gap_ws_c = gap_ws.clone();
                window.on_mouse_event(move |event: &MouseDownEvent, phase, _window, cx| {
                    if phase != DispatchPhase::Bubble || !bounds.contains(&event.position) {
                        return;
                    }
                    let local_x = f32::from(event.position.x - bounds.origin.x);
                    for i in 0..slide_count {
                        let bx = slide_xs_c[i];
                        let gw = gap_ws_c[i];
                        if local_x >= bx && local_x < bx + SLIDE_W {
                            services.update(cx, |s, _| s.seek_to(Timestamp::new(i, 0.0))).ok();
                            return;
                        }
                        let gap_start = bx + SLIDE_W;
                        if local_x >= gap_start && local_x < gap_start + gw {
                            let t = ((local_x - gap_start) / (PX_PER_SEC * zoom)) as f64;
                            services.update(cx, |s, _| s.seek_to(Timestamp::new(i, t))).ok();
                            return;
                        }
                    }
                });
            }
        )
        .w(px(track_w))
        .h_full();

        // flex_none + explicit width: prevents shrinking so overflow_x_scroll works.
        // h_full: canvas fills the scroll container height so the playhead spans it.
        div().flex_none().w(px(track_w)).h_full().child(track)
    }
}

impl Render for Timeline {
    fn render(&mut self, _window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let exec = self.services.read(cx).execution_state().read(cx);
        let current_slide = exec.current_timestamp.slide;
        let current_time = exec.current_timestamp.time;
        let is_playing = exec.is_playing();
        let slide_count = exec.slide_count;
        let durations = exec.slide_durations.clone();
        let has_error = exec.has_error();

        let toolbar = self.render_toolbar(is_playing, has_error, current_slide, slide_count, current_time, cx);
        let track = Self::render_track(
            self.services.downgrade(),
            current_slide,
            current_time,
            slide_count,
            durations,
            self.zoom_factor(),
        );

        div()
            .flex()
            .flex_col()
            .size_full()
            .bg(BG)
            .child(toolbar)
            .child(
                div()
                    .id("tl-scroll")
                    .flex()
                    .flex_1()
                    .overflow_x_scroll()
                    .track_scroll(&self.scroll)
                    .child(track)
            )
    }
}
