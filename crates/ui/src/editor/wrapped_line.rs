use std::{ops::Range, sync::Arc};

use gpui::{App, Bounds, DecorationRun, Half, Hsla, LineLayout, Pixels, Point, StrikethroughStyle, TextRun, UnderlineStyle, Window, WrapBoundary, black, fill, point, px, size};
use smallvec::SmallVec;
use structs::text::{Count8, Span8};

// adapted from gpui's wrapped line
#[derive(Clone, Debug, Default)]
pub struct WrappedLine {
    unwrapped_layout: Arc<LineLayout>,
    wrap_boundaries: SmallVec<[(WrapBoundary, Pixels); 1]>,
    decoration_runs: SmallVec<[DecorationRun; 32]>,
}

impl WrappedLine {
    fn build_wrap_boundaries(text: &str, unwrapped_layout: &LineLayout, wrap_width: Pixels) -> SmallVec<[(WrapBoundary, Pixels); 1]> {
        debug_assert!(!text.contains('\n'));

        let is_word_boundary = |ch: char| !ch.is_alphanumeric();

        let mut wrap_boundaries = SmallVec::new();
        // if first non whitespace is on first line, use that, otherwise use 0.0
        let mut hanging_indentation = None;
        let mut current_x_offset = px(0.0);

        let mut word_start = (WrapBoundary {
            run_ix: 0,
            glyph_ix: 0,
        }, px(0.0));
        let mut prev_ch = '\0';

        let mut glyphs = unwrapped_layout
            .runs.iter().enumerate()
            .flat_map(|(run_ix, run)| {
                run.glyphs.iter().enumerate().map(move |(glyph_ix, glyph)| {
                    let character = text[glyph.index..].chars().next().unwrap();
                    (
                        WrapBoundary { run_ix, glyph_ix },
                        character,
                        glyph.position.x,
                    )
                })
            })
            .peekable();

        while let Some((boundary, ch, x)) = glyphs.next() {
            // starting a new word
            if is_word_boundary(prev_ch) {
                word_start = (boundary, x);
            }

            let next_x = glyphs.peek().map_or(unwrapped_layout.width, |x| x.2);

            if next_x + current_x_offset > wrap_width {
                let indent = hanging_indentation.get_or_insert(px(0.0));

                let (start_boundary, start_x) = word_start;
                // try to wrap at start of word it fits
                let (wrap_boundary, wrap_x) = if next_x - start_x <= wrap_width {
                    // wrap at start of word
                    (start_boundary, start_x)
                } else {
                    // wrap at current position
                    (boundary, x)
                };

                word_start = (wrap_boundary, wrap_x);

                wrap_boundaries.push((wrap_boundary, *indent));
                current_x_offset = -wrap_x + *indent;
            }

            if hanging_indentation.is_none() && !ch.is_whitespace() {
                hanging_indentation = Some(x + current_x_offset);
            }

            prev_ch = ch;
        }

        wrap_boundaries
    }

    pub fn new(text: &str, size: Pixels, runs: &[TextRun], wrap_width: Pixels, window: &mut Window) -> Self {
        let unwrapped_layout = window.text_system().layout_line(text, size, runs, None);

        let decoration_runs = runs.iter().map(|run| {
            DecorationRun {
                len: run.len as u32,
                color: run.color,
                background_color: run.background_color,
                underline: run.underline.clone(),
                strikethrough: run.strikethrough.clone(),
            }
        }).collect();

        let wrap_boundaries = Self::build_wrap_boundaries(text, &unwrapped_layout, wrap_width);

        Self {
            unwrapped_layout,
            wrap_boundaries,
            decoration_runs,
        }
    }

    pub fn closest_index(&self, position: Point<Pixels>, line_height: Pixels) -> Result<Count8, Count8> {
        let line_count = self.wrap_boundaries.len() + 1;

        let wrapped_line_ix = (position.y / line_height) as usize;
        if wrapped_line_ix >= line_count {
            return Result::Err(self.unwrapped_layout.len);
        }

        let wrap_start_index = if wrapped_line_ix > 0 {
            let (line_start_boundary, _) = self.wrap_boundaries[wrapped_line_ix - 1];
            let run = &self.unwrapped_layout.runs[line_start_boundary.run_ix];
            let glyph = &run.glyphs[line_start_boundary.glyph_ix];
            glyph.index
        } else {
            0
        };

        let wrap_end_index = if wrapped_line_ix < self.wrap_boundaries.len() {
            let next_wrap_boundary_ix = wrapped_line_ix;
            let (next_wrap_boundary, _) = self.wrap_boundaries[next_wrap_boundary_ix];
            let run = &self.unwrapped_layout.runs[next_wrap_boundary.run_ix];
            let glyph = &run.glyphs[next_wrap_boundary.glyph_ix];
            glyph.index
        } else {
            self.unwrapped_layout.len
        };

        let mut x_offset = px(0.0);
        for (boundary, indent) in &self.wrap_boundaries[..wrapped_line_ix] {
            x_offset += self.unwrapped_layout.runs[boundary.run_ix]
                .glyphs[boundary.glyph_ix]
                .position.x - *indent;
        }

        let adjusted_x = position.x + x_offset;
        Ok(self.unwrapped_layout.closest_index_for_x(adjusted_x)
            // clamp to indices in this line
            .clamp(wrap_start_index, wrap_end_index))
    }

    pub fn location_for_index(&self, index: Count8, line_height: Pixels) -> (usize, Count8, Point<Pixels>) {
        let mut line_ix = 0;
        for (boundary, _) in &self.wrap_boundaries {
            let run = &self.unwrapped_layout.runs[boundary.run_ix];
            let glyph = &run.glyphs[boundary.glyph_ix];
            if index < glyph.index {
                break;
            }
            line_ix += 1;
        }

        let (x_offset, col_index) = if line_ix > 0 {
            let (line_start_boundary, indent) = self.wrap_boundaries[line_ix - 1];
            let run = &self.unwrapped_layout.runs[line_start_boundary.run_ix];
            let glyph = &run.glyphs[line_start_boundary.glyph_ix];
            (indent - glyph.position.x, index - glyph.index)
        } else {
            (px(0.0), index)
        };

        let local_x = self.unwrapped_layout.x_for_index(index) + x_offset;
        (line_ix, col_index, point(local_x, line_height * line_ix as f32))
    }

    pub fn len(&self) -> Count8 {
        self.unwrapped_layout.len
    }

    pub fn line_count(&self) -> usize {
        self.wrap_boundaries.len() + 1
    }

    pub fn iter(&self) -> impl Iterator<Item=SingleWrappedLine<'_>> {
        (0..self.line_count()).map(move |line_ix| {
            let prev_boundary = if line_ix > 0 {
                Some(&self.wrap_boundaries[line_ix - 1])
            } else {
                None
            };
            let next_boundary = if line_ix < self.wrap_boundaries.len() {
                Some(&self.wrap_boundaries[line_ix])
            } else {
                None
            };

            SingleWrappedLine {
                line: self,
                prev_boundary,
                next_boundary,
            }
        })
    }

    pub fn paint(&self, origin: Point<Pixels>, line_height: Pixels, window: &mut Window, cx: &App) -> Result<(), anyhow::Error> {
        let layout = &self.unwrapped_layout;
        let wrap_boundaries = &self.wrap_boundaries;
        let decoration_runs = &self.decoration_runs;

        let line_bounds = Bounds::new(
            origin,
            gpui::size(
                layout.width,
                line_height * (wrap_boundaries.len() as f32 + 1.),
            ),
        );
        window.paint_layer(line_bounds, |window| {
            let padding_top = (line_height - layout.ascent - layout.descent) / 2.;
            let baseline_offset = point(px(0.), padding_top + layout.ascent);
            let mut decoration_runs = decoration_runs.iter();
            let mut wraps = wrap_boundaries.iter().peekable();
            let mut run_end = 0;
            let mut color = black();
            let mut current_underline: Option<(Point<Pixels>, UnderlineStyle)> = None;
            let mut current_strikethrough: Option<(Point<Pixels>, StrikethroughStyle)> = None;
            let text_system = cx.text_system().clone();
            let mut glyph_origin = origin;
            let mut prev_glyph_position = Point::default();
            let mut max_glyph_size = size(px(0.), px(0.));
            let mut first_glyph_x = origin.x;
            for (run_ix, run) in layout.runs.iter().enumerate() {
                max_glyph_size = text_system.bounding_box(run.font_id, layout.font_size).size;

                for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
                    glyph_origin.x += glyph.position.x - prev_glyph_position.x;
                    if glyph_ix == 0 && run_ix == 0 {
                        first_glyph_x = glyph_origin.x;
                    }

                    if wraps.peek().map(|w| &w.0) == Some(&&WrapBoundary { run_ix, glyph_ix }) {
                        let indent = wraps.peek().unwrap().1;
                        wraps.next();
                        if let Some((underline_origin, underline_style)) = current_underline.as_mut() {
                            if glyph_origin.x == underline_origin.x {
                                underline_origin.x -= max_glyph_size.width.half();
                            };
                            window.paint_underline(
                                *underline_origin,
                                glyph_origin.x - underline_origin.x,
                                underline_style,
                            );
                            underline_origin.x = origin.x;
                            underline_origin.y += line_height;
                        }
                        if let Some((strikethrough_origin, strikethrough_style)) =
                            current_strikethrough.as_mut()
                        {
                            if glyph_origin.x == strikethrough_origin.x {
                                strikethrough_origin.x -= max_glyph_size.width.half();
                            };
                            window.paint_strikethrough(
                                *strikethrough_origin,
                                glyph_origin.x - strikethrough_origin.x,
                                strikethrough_style,
                            );
                            strikethrough_origin.x = origin.x;
                            strikethrough_origin.y += line_height;
                        }

                        glyph_origin.x = origin.x + indent;
                        glyph_origin.y += line_height;
                    }
                    prev_glyph_position = glyph.position;

                    let mut finished_underline: Option<(Point<Pixels>, UnderlineStyle)> = None;
                    let mut finished_strikethrough: Option<(Point<Pixels>, StrikethroughStyle)> = None;
                    if glyph.index >= run_end {
                        let mut style_run = decoration_runs.next();

                        // ignore style runs that apply to a partial glyph
                        while let Some(run) = style_run {
                            if glyph.index < run_end + (run.len as usize) {
                                break;
                            }
                            run_end += run.len as usize;
                            style_run = decoration_runs.next();
                        }

                        if let Some(style_run) = style_run {
                            if let Some((_, underline_style)) = &mut current_underline
                                && style_run.underline.as_ref() != Some(underline_style)
                            {
                                finished_underline = current_underline.take();
                            }
                            if let Some(run_underline) = style_run.underline.as_ref() {
                                current_underline.get_or_insert((
                                    point(
                                        glyph_origin.x,
                                        glyph_origin.y + baseline_offset.y + (layout.descent * 0.618),
                                    ),
                                    UnderlineStyle {
                                        color: Some(run_underline.color.unwrap_or(style_run.color)),
                                        thickness: run_underline.thickness,
                                        wavy: run_underline.wavy,
                                    },
                                ));
                            }
                            if let Some((_, strikethrough_style)) = &mut current_strikethrough
                                && style_run.strikethrough.as_ref() != Some(strikethrough_style)
                            {
                                finished_strikethrough = current_strikethrough.take();
                            }
                            if let Some(run_strikethrough) = style_run.strikethrough.as_ref() {
                                current_strikethrough.get_or_insert((
                                    point(
                                        glyph_origin.x,
                                        glyph_origin.y
                                            + (((layout.ascent * 0.5) + baseline_offset.y) * 0.5),
                                    ),
                                    StrikethroughStyle {
                                        color: Some(run_strikethrough.color.unwrap_or(style_run.color)),
                                        thickness: run_strikethrough.thickness,
                                    },
                                ));
                            }

                            run_end += style_run.len as usize;
                            color = style_run.color;
                        } else {
                            run_end = layout.len;
                            finished_underline = current_underline.take();
                            finished_strikethrough = current_strikethrough.take();
                        }
                    }

                    if let Some((mut underline_origin, underline_style)) = finished_underline {
                        if underline_origin.x == glyph_origin.x {
                            underline_origin.x -= max_glyph_size.width.half();
                        };
                        window.paint_underline(
                            underline_origin,
                            glyph_origin.x - underline_origin.x,
                            &underline_style,
                        );
                    }

                    if let Some((mut strikethrough_origin, strikethrough_style)) =
                        finished_strikethrough
                    {
                        if strikethrough_origin.x == glyph_origin.x {
                            strikethrough_origin.x -= max_glyph_size.width.half();
                        };
                        window.paint_strikethrough(
                            strikethrough_origin,
                            glyph_origin.x - strikethrough_origin.x,
                            &strikethrough_style,
                        );
                    }

                    let max_glyph_bounds = Bounds {
                        origin: glyph_origin,
                        size: max_glyph_size,
                    };

                    let content_mask = window.content_mask();
                    if max_glyph_bounds.intersects(&content_mask.bounds) {
                        let vertical_offset = point(px(0.0), glyph.position.y);
                        if glyph.is_emoji {
                            window.paint_emoji(
                                glyph_origin + baseline_offset + vertical_offset,
                                run.font_id,
                                glyph.id,
                                layout.font_size,
                            )?;
                        } else {
                            window.paint_glyph(
                                glyph_origin + baseline_offset + vertical_offset,
                                run.font_id,
                                glyph.id,
                                layout.font_size,
                                color,
                            )?;
                        }
                    }
                }
            }

            let mut last_line_end_x = first_glyph_x + layout.width;
            if let Some((boundary, indent)) = wrap_boundaries.last() {
                let run = &layout.runs[boundary.run_ix];
                let glyph = &run.glyphs[boundary.glyph_ix];
                last_line_end_x -= glyph.position.x - *indent;
            }

            if let Some((mut underline_start, underline_style)) = current_underline.take() {
                if last_line_end_x == underline_start.x {
                    underline_start.x -= max_glyph_size.width.half()
                };
                window.paint_underline(
                    underline_start,
                    last_line_end_x - underline_start.x,
                    &underline_style,
                );
            }

            if let Some((mut strikethrough_start, strikethrough_style)) = current_strikethrough.take() {
                if last_line_end_x == strikethrough_start.x {
                    strikethrough_start.x -= max_glyph_size.width.half()
                };
                window.paint_strikethrough(
                    strikethrough_start,
                    last_line_end_x - strikethrough_start.x,
                    &strikethrough_style,
                );
            }

            Ok(())
        })
    }

    #[allow(unused)]
    pub fn paint_background(&self, origin: Point<Pixels>, line_height: Pixels, window: &mut Window, cx: &mut App) -> Result<(), anyhow::Error> {
        let layout = &self.unwrapped_layout;
        let wrap_boundaries = &self.wrap_boundaries;
        let decoration_runs = &self.decoration_runs;

        let line_bounds = Bounds::new(
            origin,
            size(
                layout.width,
                line_height * (wrap_boundaries.len() as f32 + 1.),
            ),
        );
        window.paint_layer(line_bounds, |window| {
            let mut decoration_runs = decoration_runs.iter();
            let mut wraps = wrap_boundaries.iter().peekable();
            let mut run_end = 0;
            let mut current_background: Option<(Point<Pixels>, Hsla)> = None;
            let text_system = cx.text_system().clone();
            let mut glyph_origin = origin;
            let mut prev_glyph_position = Point::default();
            let mut max_glyph_size = size(px(0.), px(0.));
            for (run_ix, run) in layout.runs.iter().enumerate() {
                max_glyph_size = text_system.bounding_box(run.font_id, layout.font_size).size;

                for (glyph_ix, glyph) in run.glyphs.iter().enumerate() {
                    glyph_origin.x += glyph.position.x - prev_glyph_position.x;

                    if wraps.peek().map(|w| &w.0) == Some(&&WrapBoundary { run_ix, glyph_ix }) {
                        let indent = wraps.peek().unwrap().1;
                        wraps.next();
                        if let Some((background_origin, background_color)) = current_background.as_mut()
                        {
                            if glyph_origin.x == background_origin.x {
                                background_origin.x -= max_glyph_size.width.half()
                            }
                            window.paint_quad(fill(
                                Bounds {
                                    origin: *background_origin,
                                    size: size(glyph_origin.x - background_origin.x, line_height),
                                },
                                *background_color,
                            ));
                            background_origin.x = origin.x;
                            background_origin.y += line_height;
                        }

                        glyph_origin.x = origin.x + indent;
                        glyph_origin.y += line_height;
                    }
                    prev_glyph_position = glyph.position;

                    let mut finished_background: Option<(Point<Pixels>, Hsla)> = None;
                    if glyph.index >= run_end {
                        let mut style_run = decoration_runs.next();

                        // ignore style runs that apply to a partial glyph
                        while let Some(run) = style_run {
                            if glyph.index < run_end + (run.len as usize) {
                                break;
                            }
                            run_end += run.len as usize;
                            style_run = decoration_runs.next();
                        }

                        if let Some(style_run) = style_run {
                            if let Some((_, background_color)) = &mut current_background
                                && style_run.background_color.as_ref() != Some(background_color)
                            {
                                finished_background = current_background.take();
                            }
                            if let Some(run_background) = style_run.background_color {
                                current_background.get_or_insert((
                                    point(glyph_origin.x, glyph_origin.y),
                                    run_background,
                                ));
                            }
                            run_end += style_run.len as usize;
                        } else {
                            run_end = layout.len;
                            finished_background = current_background.take();
                        }
                    }

                    if let Some((mut background_origin, background_color)) = finished_background {
                        let width = glyph_origin.x - background_origin.x;
                        if background_origin.x == glyph_origin.x {
                            background_origin.x -= max_glyph_size.width.half();
                        };
                        window.paint_quad(fill(
                            Bounds {
                                origin: background_origin,
                                size: size(width, line_height),
                            },
                            background_color,
                        ));
                    }
                }
            }

            let mut last_line_end_x = origin.x + layout.width;
            if let Some((boundary, indent)) = wrap_boundaries.last() {
                let run = &layout.runs[boundary.run_ix];
                let glyph = &run.glyphs[boundary.glyph_ix];
                last_line_end_x -= glyph.position.x - *indent;
            }

            if let Some((mut background_origin, background_color)) = current_background.take() {
                if last_line_end_x == background_origin.x {
                    background_origin.x -= max_glyph_size.width.half()
                };
                window.paint_quad(fill(
                    Bounds {
                        origin: background_origin,
                        size: size(last_line_end_x - background_origin.x, line_height),
                    },
                    background_color,
                ));
            }

            Ok(())
        })
    }
}

pub struct SingleWrappedLine<'a> {
    pub line: &'a WrappedLine,
    pub prev_boundary: Option<&'a (WrapBoundary, Pixels)>,
    pub next_boundary: Option<&'a (WrapBoundary, Pixels)>,
}

impl<'a> SingleWrappedLine<'a> {
    pub fn x_range(&self, unwrapped_char_range: Span8) -> Option<Range<Pixels>> {
        let (start_x, org_start_x, line_start_index) = if let Some((prev_boundary, indent)) = self.prev_boundary {
            let glyph = &self.line.unwrapped_layout.runs[prev_boundary.run_ix].glyphs[prev_boundary.glyph_ix];
            (*indent, glyph.position.x, glyph.index)
        } else {
            (px(0.0), px(0.0), 0)
        };

        let line_end_index = if let Some((next_boundary, _)) = self.next_boundary {
            let glyph = &self.line.unwrapped_layout.runs[next_boundary.run_ix].glyphs[next_boundary.glyph_ix];
            glyph.index
        } else {
            self.line.unwrapped_layout.len
        };

        let pos = |index: usize| {
            let x = self.line.unwrapped_layout.x_for_index(index);
            start_x + (x - org_start_x)
        };

        if line_start_index >= unwrapped_char_range.end || line_end_index <= unwrapped_char_range.start {
            return None;
        }

        let clamped_start_index = unwrapped_char_range.start.clamp(line_start_index, line_end_index);
        let clamped_end_index = unwrapped_char_range.end.clamp(line_start_index, line_end_index);

        let range_start_x = pos(clamped_start_index);
        let range_end_x = pos(clamped_end_index);

        Some(range_start_x..range_end_x)
    }
}
