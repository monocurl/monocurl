use executor::time::Timestamp;
use gpui::*;

use crate::theme::{FontSet, Theme};

use super::metrics::{
    CONTENT_H, DUR_FONT_SIZE, LABEL_FONT_SIZE, LABEL_GAP, LABEL_LINE_H, LABEL_MAX_LINES,
    LABEL_TEXT_H, PADDING_V, PX_PER_SEC, SLIDE_H, SLIDE_W, compute_gap_ws, compute_painted_gap_ws,
    compute_playhead_x, compute_slide_xs, compute_track_width, effective_durations,
};

const CURRENT_PLAYHEAD_W: f32 = 2.0;
const TARGET_PLAYHEAD_W: f32 = 1.0;

struct TrackPrepaint {
    slide_xs: Vec<f32>,
    painted_gap_ws: Vec<f32>,
    playhead_x: f32,
    target_playhead_x: f32,
    durations: Vec<Option<f64>>,
    explicit: Vec<bool>,
    vert_offset: f32,
    dur_texts: Vec<ShapedLine>,
}

pub(super) fn render_track(
    current_timestamp: Timestamp,
    target_timestamp: Timestamp,
    slide_count: usize,
    slide_names: Vec<Option<String>>,
    durations: Vec<Option<f64>>,
    minimum_durations: Vec<Option<f64>>,
    zoom: f32,
    theme: Theme,
) -> impl IntoElement {
    let effective_for_width = effective_durations(
        slide_count,
        &durations,
        &minimum_durations,
        current_timestamp.slide,
        current_timestamp.time,
    );
    let track_w = compute_track_width(slide_count, &effective_for_width, zoom);
    let slide_xs_for_labels = compute_slide_xs(slide_count, &effective_for_width, zoom);
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
                    current_timestamp.slide,
                    current_timestamp.time,
                );
                let explicit = (0..slide_count)
                    .map(|i| durations.get(i).is_some_and(|d| d.is_some()))
                    .collect();

                let slide_xs = compute_slide_xs(slide_count, &effective, zoom);
                let gap_ws = compute_gap_ws(slide_count, &effective, zoom);
                let painted_gap_ws = compute_painted_gap_ws(slide_count, &effective, zoom);
                let playhead_x = compute_playhead_x(
                    current_timestamp.slide,
                    current_timestamp.time,
                    &slide_xs,
                    &gap_ws,
                    &effective,
                    zoom,
                );
                let target_playhead_x = compute_playhead_x(
                    target_timestamp.slide,
                    target_timestamp.time,
                    &slide_xs,
                    &gap_ws,
                    &effective,
                    zoom,
                );
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
                            &[make_run(&s, theme.timeline_text)],
                            None,
                        )
                    })
                    .collect();

                TrackPrepaint {
                    slide_xs,
                    painted_gap_ws,
                    playhead_x,
                    target_playhead_x,
                    durations: effective,
                    explicit,
                    vert_offset,
                    dur_texts,
                }
            }
        },
        move |bounds, prepaint, window, cx| {
            let TrackPrepaint {
                slide_xs,
                painted_gap_ws,
                playhead_x,
                target_playhead_x,
                durations,
                explicit,
                vert_offset,
                dur_texts,
            } = prepaint;
            let ox = bounds.origin.x;
            let oy_full = bounds.origin.y;
            let oy = oy_full + px(vert_offset);

            let line_y = PADDING_V + SLIDE_H / 2.0;
            let sec_px = PX_PER_SEC * zoom;

            for i in 0..slide_count {
                let gap_x = slide_xs[i] + SLIDE_W;
                let painted_gw = painted_gap_ws[i];

                if painted_gw > 0.0 {
                    window.paint_quad(fill(
                        Bounds::new(
                            point(ox + px(gap_x), oy + px(line_y - 0.5)),
                            size(px(painted_gw), px(1.0)),
                        ),
                        theme.timeline_connector,
                    ));
                }

                window.paint_quad(fill(
                    Bounds::new(
                        point(ox + px(gap_x), oy + px(line_y - 4.0)),
                        size(px(1.0), px(8.0)),
                    ),
                    theme.timeline_tick,
                ));

                let duration_secs = durations.get(i).and_then(|d| *d).unwrap_or(0.0);
                let num_marks = duration_secs.floor() as usize;
                for sec in 1..=num_marks {
                    let mark_x = gap_x + sec as f32 * sec_px;
                    if mark_x < gap_x + painted_gw {
                        window.paint_quad(fill(
                            Bounds::new(
                                point(ox + px(mark_x - 1.0), oy + px(line_y - 3.0)),
                                size(px(1.0), px(6.0)),
                            ),
                            theme.timeline_tick,
                        ));
                    }
                }
            }

            for i in 0..slide_count {
                let bx = slide_xs[i];
                let border_color: Hsla = if explicit.get(i).copied().unwrap_or(false) {
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
                    px(1.0),
                    border_color,
                    BorderStyle::Solid,
                ));

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
            }

            let mut paint_playhead = |x: f32, width: f32| {
                window.paint_quad(fill(
                    Bounds::new(
                        point(ox + px(x - width / 2.0), oy_full),
                        size(px(width), bounds.size.height),
                    ),
                    theme.timeline_playhead,
                ));
            };
            paint_playhead(target_playhead_x, TARGET_PLAYHEAD_W);
            paint_playhead(playhead_x, CURRENT_PLAYHEAD_W);
        },
    )
    .w(px(track_w))
    .h_full();

    let labels = (0..slide_count).map(|i| {
        let label = slide_names
            .get(i)
            .and_then(|name| name.as_ref())
            .cloned()
            .unwrap_or_else(|| format!("Slide {}", i + 1));
        div()
            .absolute()
            .left(px(slide_xs_for_labels[i]))
            .top(px(PADDING_V + SLIDE_H + LABEL_GAP))
            .w(px(SLIDE_W))
            .h(px(LABEL_TEXT_H))
            .overflow_hidden()
            .text_ellipsis()
            .line_clamp(LABEL_MAX_LINES)
            .text_center()
            .text_color(theme.timeline_text)
            .text_size(px(LABEL_FONT_SIZE))
            .line_height(px(LABEL_LINE_H))
            .child(label)
    });

    div()
        .flex_none()
        .relative()
        .w(px(track_w))
        .h_full()
        .child(track)
        .child(
            div()
                .absolute()
                .left(px(0.0))
                .top(px(0.0))
                .size_full()
                .flex()
                .items_center()
                .child(
                    div()
                        .relative()
                        .w(px(track_w))
                        .h(px(CONTENT_H))
                        .children(labels),
                ),
        )
}
