use unicode_segmentation::*;
use structs::text::{Count16, Count8, Location16, Location8, Span16, Span8};

pub trait TextBackend: 'static {
    fn offset8_to_offset16(&self, offset: Count8) -> Count16;
    fn offset16_to_offset8(&self, offset: Count16) -> Count8;
    fn loc8_to_offset8(&self, loc: Location8) -> Count8;
    fn offset8_to_loc8(&self, offset: Count8) -> Location8;
    fn loc16_to_offset16(&self, loc: Location16) -> Count16;
    fn offset16_to_loc16(&self, offset: Count16) -> Location16;

    fn span8_to_span16(&self, span8: &Span8) -> Span16 {
        self.offset8_to_offset16(span8.start)..self.offset8_to_offset16(span8.end)
    }

    fn span16_to_span8(&self, span16: &Span16) -> Span8 {
        self.offset16_to_offset8(span16.start)..self.offset16_to_offset8(span16.end)
    }

    fn replace(&mut self, span: Span8, new_text: &str);
    fn read(&self, span: Span8) -> String;

    fn content(&self) -> String {
        self.read(0..self.len())
    }

    fn len(&self) -> Count8;

    fn next_boundary(&self, offset: Count8) -> Count8;
    fn prev_boundary(&self, offset: Count8) -> Count8;
    fn reset(&mut self);
}

pub struct NaiveBackend(pub String);

impl TextBackend for NaiveBackend {
    fn offset8_to_offset16(&self, offset: Count8) -> Count16 {
        let mut utf16_offset = 0;
        let mut utf8_count = 0;

        for ch in self.0.chars() {
            if utf8_count >= offset {
                break;
            }
            utf8_count += ch.len_utf8();
            utf16_offset += ch.len_utf16();
        }

        utf16_offset
    }

    fn offset16_to_offset8(&self, offset: Count16) -> Count8 {
        let mut utf8_offset = 0;
        let mut utf16_count = 0;

        for ch in self.0.chars() {
            if utf16_count >= offset {
                break;
            }
            utf16_count += ch.len_utf16();
            utf8_offset += ch.len_utf8();
        }

        utf8_offset
    }

    fn loc8_to_offset8(&self, loc: Location8) -> Count8 {
        let mut current_row = 0;
        let mut offset = 0;

        for line in self.0.lines() {
            if current_row == loc.row {
                return offset + loc.col.min(line.len());
            }
            current_row += 1;
            offset += line.len() + 1; // newline
        }

        // EOf
        self.0.len()
    }

    fn offset8_to_loc8(&self, offset: Count8) -> Location8 {
        let mut current_offset = 0;
        let mut row = 0;

        for line in self.0.lines() {
            let line_end_offset = current_offset + line.len();

            if offset <= line_end_offset {
                let col = offset - current_offset;
                return Location8 { row, col };
            }

            // dont forget newline
            current_offset = line_end_offset + 1;
            if offset == current_offset - 1 {
                return Location8 { row, col: line.len() };
            }
            row += 1;
        }

        // EOF
        Location8 { row, col: 0 }
    }

    fn loc16_to_offset16(&self, loc: Location16) -> Count16 {
        let offset8_col = self.offset16_to_offset8(loc.col);
        let loc8 = Location8 { row: loc.row, col: offset8_col };

        let offset8 = self.loc8_to_offset8(loc8);
        self.offset8_to_offset16(offset8)
    }

    fn offset16_to_loc16(&self, offset: Count16) -> Location16 {
        let offset8 = self.offset16_to_offset8(offset);
        let loc8 = self.offset8_to_loc8(offset8);
        let col16 = self.offset8_to_offset16(loc8.col);

        Location16 { row: loc8.row, col: col16 }
    }

    fn replace(&mut self, span: Span8, new_text: &str) {
        self.0 = self.0[..span.start].to_string() + new_text + &self.0[span.end..];
    }

    fn read(&self, span: Span8) -> String {
        self.0[span].into()
    }

    fn len(&self) -> Count8 {
        self.0.as_str().len()
    }

    fn next_boundary(&self, offset: Count8) -> Count8 {
        self.0
            .grapheme_indices(true)
            .find_map(|(idx, _)| (idx > offset).then_some(idx))
            .unwrap_or(self.0.len())
    }

    fn prev_boundary(&self, offset: Count8) -> Count8 {
        self.0
            .grapheme_indices(true)
            .rev()
            .find_map(|(idx, _)| (idx < offset).then_some(idx))
            .unwrap_or(0)
    }

    fn reset(&mut self) {
        self.0 = "".into();
    }
}