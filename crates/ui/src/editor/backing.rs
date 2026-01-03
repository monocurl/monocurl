use std::usize;

use structs::{text::{Count8, Count16, Location8, Span8, Span16}};

pub struct AutoCompleteItem {
    pub head: String,
    pub replacement: String,
    pub cursor_position: isize,
}

pub struct Operation<B: EditorBackend> {
    backend: B,
    cursor: Location8,
    anchor: Location8,
}

pub struct Diagnostic {
    version: usize,
}

pub trait EditorBackend: Default + 'static {
    fn offset8_to_offset16(&self, offset: Count8) -> Count16;
    fn offset16_to_offset8(&self, offset: Count16) -> Count8;
    fn loc8_to_offset8(&self, loc: Location8) -> Count8;
    fn offset8_to_loc8(&self, offset: Count8) -> Location8;

    fn span8_to_span16(&self, span8: &Span8) -> Span16 {
        self.offset8_to_offset16(span8.start)..self.offset8_to_offset16(span8.end)
    }

    fn span16_to_span8(&self, span16: &Span16) -> Span8 {
        self.offset16_to_offset8(span16.start)..self.offset16_to_offset8(span16.end)
    }

    fn replace(&mut self, span: Span8, new_text: &str);
    fn read(&self, span: Span8) -> String;

    fn len(&self) -> Count8;

    fn next_boundary(&self, offset: Count8) -> Count8;
    fn prev_boundary(&self, offset: Count8) -> Count8;

    // maximal word containing position
    // or, if this is empty due to whitespace, the previous word (along with contents up till offset)
    fn word(&self, offset: Count8, only_expand_left: bool) -> Span8 {
        let not_separator = |c: char| {
            c.is_alphanumeric() || c == '_'
        };

        let mut start  = offset;
        while start > 0 {
            let prev = self.prev_boundary(start);
            let ch = self.read(prev..start);
            if ch.chars().all(not_separator) {
                start = prev;
            } else {
                break;
            }
        }

        let mut end = offset;
        let len = self.len();
        if !only_expand_left {
            while end < len {
                let next = self.next_boundary(end);
                let ch = self.read(end..next);
                if ch.chars().all(not_separator) {
                    end = next;
                } else {
                    break;
                }
            }
        }

        if start == offset && end == offset {
            // try to include at least one character that is not whitespace (not crossing newline)
            let mut first_nonwhitespace = -1;
            while start > 0 {
                let prev = self.prev_boundary(start);
                let ch = self.read(prev..start);

                if ch == "\n" {
                    break;
                }
                let is_whitespace = ch.chars().all(|c| c.is_whitespace());
                if is_whitespace && first_nonwhitespace == -1 {
                    start = prev;
                }
                else {
                    first_nonwhitespace = first_nonwhitespace.max(start as isize);
                    if ch.chars().all(not_separator) {
                        start = prev;
                    }
                    else if start as isize == first_nonwhitespace {
                        start = prev;
                        break;
                    }
                    else {
                        break;
                    }
                }
            }
        }

        start..end
    }

    fn diagnostics(&self) -> &[Diagnostic];

    // characters that have modified attributes
    // and resets their dirty status to non dirty
    fn mark_region_as_dirty(&mut self, span: Span8);
    fn take_dirty_region(&mut self) -> Option<Span8>;

    fn autocomplete_list(&self) -> &[AutoCompleteItem];
}
