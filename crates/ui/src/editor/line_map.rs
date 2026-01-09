use std::{ops::Range, option::IntoIter};

use gpui::{Pixels, Point};
use structs::{rope::{self, Rope}, text::{Location8}};

use crate::{editor::{wrapped_line::WrappedLine}};

#[derive(Clone)]
struct LeafData {
    line: WrappedLine,
    // indicates 0 lines
    is_degenerate: bool
}

impl Default for LeafData {
    fn default() -> Self {
        Self {
            line: WrappedLine::default(),
            is_degenerate: true,
        }
    }
}

impl rope::LeafData for LeafData {
    type Iterator<'a> = IntoIter<&'a WrappedLine>;

    fn identity() -> Self {
        Self::default()
    }

    fn subrange(&self, range: std::ops::Range<usize>) -> Self {
        if range.is_empty() {
            Self { is_degenerate: true, line: WrappedLine::default() }
        } else {
            Self { is_degenerate: false, line: self.line.clone() }
        }
    }

    fn try_append(&mut self, from: Self) -> Option<Self> {
        if self.is_degenerate {
            *self = from;
            None
        } else if from.is_degenerate {
            None
        } else {
            Some(from)
        }
    }

    fn iterator(&self, at: structs::text::Count8, to: structs::text::Count8) -> Self::Iterator<'_> {
        if self.is_degenerate || at == to {
            None
        }
        else {
            Some(&self.line)
        }.into_iter()
    }

    // in this case, we don't actually do proper "codeunits", but instead index based on line count
    // useful for subrange aggregation, iterators
    fn codeunits(&self) -> usize {
        if self.is_degenerate {
            0
        } else {
            1
        }
    }
}

#[derive(Debug, Clone)]
struct AggregateData {
    prewrapped_line_count: usize,
    wrapped_line_count: usize,
    rope_height: u32,
}

impl rope::AggregateData for AggregateData {
    type LeafData = LeafData;

    fn from_leaf(data: &Self::LeafData) -> Self {
        if data.is_degenerate {
            Self {
                prewrapped_line_count: 0,
                wrapped_line_count: 0,
                rope_height: 0,
            }
        } else {
            Self {
                prewrapped_line_count: 1,
                wrapped_line_count: data.line.line_count(),
                rope_height: 0,
            }
        }
    }

    fn merge(children: impl DoubleEndedIterator<Item=Self> + Clone) -> Self {
        Self {
            prewrapped_line_count: children.clone().map(|c| c.prewrapped_line_count).sum(),
            wrapped_line_count: children.clone().map(|c| c.wrapped_line_count).sum(),
            rope_height: children.map(|c| c.rope_height).max().unwrap_or(0) + 1,
        }
    }

    fn codeunits(&self) -> usize {
        self.prewrapped_line_count
    }

    fn height(&self) -> u32 {
        self.rope_height
    }
}

pub struct LineMap {
    rope: Rope<AggregateData>,
    line_height: Pixels,
}

impl LineMap {
    pub fn new(line_height: Pixels) -> Self {
        Self {
            rope: Rope::default(),
            line_height
        }
    }

    pub fn wrapped_count_before(&self, unwrapped_line_index: usize) -> usize {
        let agg = self.rope.subrange_aggregate(0..unwrapped_line_index);
        agg.wrapped_line_count
    }

    pub fn y_range(&self, line_no: Range<usize>) -> Range<Pixels> {
        let s = self.wrapped_count_before(line_no.start);
        let e = self.wrapped_count_before(line_no.end);
        s as f32 * self.line_height .. e as f32 * self.line_height
    }

    // also = index of last line that starts < pos
    // which is typically the line that contains pos
    pub fn lines_ending_before_y(&self, y: Pixels) -> (usize, usize) {
        let data = self.rope.walk(|agg| {
            let line_count = agg.wrapped_line_count as f32;
            let y_end = line_count * self.line_height;
            y_end <= y
        }, |agg, leaf| {
            let my_wrapped_lines = leaf.line.line_count();
            let total_wrapped_lines = agg.wrapped_line_count + my_wrapped_lines;
            if !leaf.is_degenerate && total_wrapped_lines as f32 * self.line_height <= y {
                agg.prewrapped_line_count += 1;
                agg.wrapped_line_count = total_wrapped_lines;
            }
        });
        (data.prewrapped_line_count, data.wrapped_line_count)
    }

    pub fn replace_lines(&mut self, old_range: Range<usize>, new_lines: impl Iterator<Item=WrappedLine>) {
        let new_data = new_lines.map(|line| LeafData { line, is_degenerate: false });
        self.rope = self.rope.replace_range(old_range, new_data);
    }

    pub fn prewrapped_visible_lines(&self, viewport_pixels: Range<Pixels>) -> Range<usize> {
        let start = self.lines_ending_before_y(viewport_pixels.start);
        // last line that is visible = last one which starts < viewport_pixels.end
        // (but using <= is not really a problem in continous domain)
        let end = self.lines_ending_before_y(viewport_pixels.end);
        start.0..(end.0 + 1).min(self.line_count())
    }

    // in error case, it gives the closest one
    pub fn location_for_point(&self, point: Point<Pixels>) -> Result<Location8, Location8> {
        if point.y >= self.total_height() {
            let last_line_no = self.rope.subrange_aggregate(0..usize::MAX).prewrapped_line_count;
            let last_line_len = self.line_len(last_line_no.saturating_sub(1));
            return Err(Location8 {
                row: last_line_no.saturating_sub(1),
                col: last_line_len
            });
        }
        else if point.y < gpui::px(0.0) {
            return Err(Location8 {
                row: 0,
                col: 0
            });
        }

        let (prewrapped_line_no, wrapped_line_no) = self.lines_ending_before_y(point.y);
        let leaf = self.rope.find_leaf(|agg| agg.prewrapped_line_count <= prewrapped_line_no);

        let local_position = gpui::point(
            point.x,
            point.y - wrapped_line_no as f32 * self.line_height
        );

        let debug = leaf.line.closest_index(local_position, self.line_height);
        println!("Local Position {:?} Answer {:?}", local_position, debug);
        leaf.line.closest_index(local_position, self.line_height)
            .map(|col| Location8 { row: prewrapped_line_no, col })
            .map_err(|col| Location8 { row: prewrapped_line_no, col })
    }

    pub fn line_count(&self) -> usize {
        self.rope.subrange_aggregate(0..usize::MAX).prewrapped_line_count
    }

    pub fn total_height(&self) -> Pixels {
        let agg = self.rope.subrange_aggregate(0..usize::MAX);
        agg.wrapped_line_count as f32 * self.line_height
    }

    pub fn line_len(&self, line_no: usize) -> usize {
        let leaf = self.rope.find_leaf(|agg| agg.prewrapped_line_count <= line_no);
        leaf.line.len()
    }

    pub fn point_for_location(&self, location: Location8) -> Point<Pixels> {
        let prefix = self.rope.subrange_aggregate(0..location.row);
        let leaf = self.rope.find_leaf(|agg| agg.prewrapped_line_count <= location.row);

        // take into account the internal wrapping
        let (_, _, delta) = leaf.line.location_for_index(location.col, self.line_height);

        gpui::point(
            delta.x,
            prefix.wrapped_line_count as f32 * self.line_height + delta.y
        )
    }

    pub fn unwrapped_lines_iter(&self, unwrapped_line_start_no: usize) -> impl Iterator<Item=ContextifiedLine<'_>> {
        let mut line_no = unwrapped_line_start_no;
        self.rope.iterator(unwrapped_line_start_no)
            .map(move |line| {
                let ret = ContextifiedLine {
                    line: line,
                    unwrapped_line_no: line_no,
                };
                line_no += 1;
                ret
            })
    }
}

pub struct ContextifiedLine<'a> {
    pub line: &'a WrappedLine,
    pub unwrapped_line_no: usize,
}
