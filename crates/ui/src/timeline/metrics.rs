pub(super) const SLIDE_W: f32 = 80.0;
pub(super) const SLIDE_H: f32 = 60.0;
pub(super) const TOOLBAR_H: f32 = 30.0;
pub(super) const PADDING_H: f32 = 16.0;
pub(super) const PADDING_V: f32 = 16.0;
pub(super) const LABEL_GAP: f32 = 5.0;
pub(super) const LABEL_LINE_H: f32 = 13.0;
pub(super) const CONTENT_H: f32 = PADDING_V + SLIDE_H + LABEL_GAP + LABEL_LINE_H + PADDING_V;
pub(super) const LABEL_FONT_SIZE: f32 = 11.0;
pub(super) const DUR_FONT_SIZE: f32 = 9.0;
pub(super) const PX_PER_SEC: f32 = 40.0;
pub(super) const MIN_GAP: f32 = 24.0;

pub(super) const ZOOM_LEVELS: [u32; 9] = [25, 50, 75, 100, 150, 200, 300, 400, 800];
pub(super) const DEFAULT_ZOOM_IDX: usize = 3;

pub(super) fn gap_w(duration: Option<f64>, zoom: f32) -> f32 {
    duration.map_or(MIN_GAP, |d| (d as f32 * PX_PER_SEC * zoom).max(MIN_GAP))
}

pub(super) fn painted_gap_w(duration: Option<f64>, zoom: f32) -> f32 {
    duration.map_or(0.0, |d| (d as f32 * PX_PER_SEC * zoom).max(0.0))
}

pub(super) fn effective_durations(
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

pub(super) fn compute_slide_xs(
    slide_count: usize,
    durations: &[Option<f64>],
    zoom: f32,
) -> Vec<f32> {
    let mut xs = Vec::with_capacity(slide_count);
    let mut x = PADDING_H;
    for i in 0..slide_count {
        xs.push(x);
        x += SLIDE_W + gap_w(durations.get(i).and_then(|d| *d), zoom);
    }
    xs
}

pub(super) fn compute_gap_ws(slide_count: usize, durations: &[Option<f64>], zoom: f32) -> Vec<f32> {
    (0..slide_count)
        .map(|i| gap_w(durations.get(i).and_then(|d| *d), zoom))
        .collect()
}

pub(super) fn compute_painted_gap_ws(
    slide_count: usize,
    durations: &[Option<f64>],
    zoom: f32,
) -> Vec<f32> {
    (0..slide_count)
        .map(|i| painted_gap_w(durations.get(i).and_then(|d| *d), zoom))
        .collect()
}

pub(super) fn compute_track_width(slide_count: usize, durations: &[Option<f64>], zoom: f32) -> f32 {
    if slide_count == 0 {
        return 200.0;
    }
    let slide_xs = compute_slide_xs(slide_count, durations, zoom);
    let last = slide_count - 1;
    slide_xs[last] + SLIDE_W + gap_w(durations.get(last).and_then(|d| *d), zoom) + PADDING_H
}

pub(super) fn compute_playhead_x(
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
