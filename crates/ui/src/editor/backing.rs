use std::usize;

use structs::{rope::{Rope, TextAggregate, leaves_from_str}, text::{Count8, Count16, Location8, Span8, Span16}};

use crate::document_state::DocumentState;

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

    fn version(&self) -> usize;

    fn diagnostics(&self) -> &[Diagnostic];

    // characters that have modified attributes
    // and resets their dirty status to non dirty
    fn mark_region_as_dirty(&mut self, span: Span8);
    fn take_dirty_region(&mut self) -> Option<Span8>;


    fn autocomplete_list(&self) -> &[AutoCompleteItem];

    fn do_undo(&mut self);

    fn do_redo(&mut self);

}


fn grapheme_boundary<const N: usize>(rope: &Rope<TextAggregate, N>, offset: Count8, forward: bool) -> Count8 {
    use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};

    let len = rope.codeunits();

    if forward && offset >= len {
        return len;
    }
    if !forward && offset == 0 {
        return 0;
    }

    // this works (on local tests) for chunk size >= 24, but is failing for smaller ones. Will just keep at 24 until issue is resolved
    //
    // Failing test case
    // related? https://github.com/unicode-rs/unicode-segmentation/issues/115
    /// ```rust
    /// # use unicode_segmentation::GraphemeCursor;
    /// let string = "👩‍❤️‍👩blank";
    /// let mut cursor = GraphemeCursor::new(0, string.len(), true);
    /// assert_eq!(cursor.next_boundary(&string[0..10], 0), Err(GraphemeIncomplete::NextChunk));
    /// assert_eq!(cursor.next_boundary(&string[10..20], 10), Err(GraphemeIncomplete::PreContext(10)));
    /// cursor.provide_context(&string[0..10], 0);
    /// assert_eq!(cursor.next_boundary(&string[10..20], 10), Ok(Some(16)));
    /// // but on the other hand..
    /// let indices = string.grapheme_indices(true).collect::<Vec<_>>();
    /// assert_eq!(indices[0].0, 0);
    /// assert_eq!(indices[1].0, 20); // 16 != 20
    /// ```

    const CHUNK: usize = 24;

    let mut cursor = GraphemeCursor::new(offset, len, true);

    let mut chunk_start = offset;
    let mut chunk_end = offset;
    let mut chunk = String::new();

    let mut res = if forward {
        Err(GraphemeIncomplete::NextChunk)
    } else {
        Err(GraphemeIncomplete::PrevChunk)
    };
    loop {
        match res {
            Ok(Some(b)) => return b,
            Ok(None) => return if forward { len } else { 0 },
            Err(GraphemeIncomplete::NextChunk) => {
                // need forward text
                let mut utf8 = 0;
                chunk.clear();
                for ch in rope.iterator(chunk_end) {
                    if utf8 >= CHUNK { break; }
                    chunk.push(ch);
                    utf8 += ch.len_utf8();
                }

                chunk_start = chunk_end;
                chunk_end += utf8;
            }
            Err(GraphemeIncomplete::PrevChunk) => {
                // need backward text
                let mut buffer = String::new();
                let mut utf8 = 0;
                for ch in rope.rev_iterator(chunk_start) {
                    if utf8 >= CHUNK { break; }
                    buffer.push(ch);
                    utf8 += ch.len_utf8();
                }

                chunk_start -= utf8;
                // put in reverse order
                chunk = buffer.chars().rev().collect::<String>() + &chunk;
            }
            Err(GraphemeIncomplete::PreContext(ctx_end)) => {
                let mut buffer = String::new();
                let mut bytes = 0;
                for ch in rope.rev_iterator(ctx_end) {
                    if bytes >= CHUNK { break; }
                    buffer.push(ch);
                    bytes += ch.len_utf8();
                }

                buffer = buffer.chars().rev().collect();

                cursor.provide_context(&buffer, ctx_end - bytes);
            }
            Err(GraphemeIncomplete::InvalidOffset) => {
                log::error!("Invalid offset passed to grapheme boundary detection: {}", offset);
                return offset;
            }
        }

        res = if forward {
            cursor.next_boundary(&chunk, chunk_start)
        } else {
            cursor.prev_boundary(&chunk, chunk_start)
        };
    }
}


impl EditorBackend for DocumentState {
    fn offset8_to_offset16(&self, offset: Count8) -> Count16 {
        let summary = self.text_rope.utf8_prefix_summary(offset);
        summary.codeunits_utf16
    }

    fn offset16_to_offset8(&self, offset: Count16) -> Count8 {
        let summary = self.text_rope.utf16_prefix_summary(offset);
        summary.bytes_utf8
    }

    fn loc8_to_offset8(&self, loc: Location8) -> Count8 {
        let summary = self.text_rope.utf8_line_pos_prefix(loc.row, loc.col);
        summary.bytes_utf8
    }

    fn offset8_to_loc8(&self, offset: Count8) -> Location8 {
        let summary = self.text_rope.utf8_prefix_summary(offset);
        Location8 {
            row: summary.newlines,
            col: summary.bytes_utf8_since_newline,
        }
    }

    fn replace(&mut self, span: Span8, new_text: &str) {
        self.text_rope = self.text_rope.replace_range(span.clone(), leaves_from_str(new_text));
        self.version += 1;
    }

    fn read(&self, span: Span8) -> String {
        let mut utf8 = 0;
        self.text_rope
            .iterator(span.start)
            .take_while(|c| {
                utf8 += c.len_utf8();
                utf8 <= span.len()
            })
            .collect()
    }

    fn len(&self) -> Count8 {
        self.text_rope.codeunits()
    }

    fn next_boundary(&self, offset: Count8) -> Count8 {
        grapheme_boundary(&self.text_rope, offset, true)
    }

    fn prev_boundary(&self, offset: Count8) -> Count8 {
        grapheme_boundary(&self.text_rope, offset, false)
    }

    fn autocomplete_list(&self) -> &[AutoCompleteItem] {
        &[]
    }

    fn do_undo(&mut self) {

    }

    fn do_redo(&mut self) {

    }

    fn version(&self) -> usize {
        todo!()
    }

    fn diagnostics(&self) -> &[Diagnostic] {
        todo!()
    }

    fn mark_region_as_dirty(&mut self, span: Span8) {
        if let Some(dirty) = &mut self.dirty_range {
            dirty.start = dirty.start.min(span.start);
            dirty.end = dirty.end.max(span.end);
        } else {
            self.dirty_range = Some(span);
        }
    }

    fn take_dirty_region(&mut self) -> Option<Span8> {
        self.dirty_range.take()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use unicode_segmentation::UnicodeSegmentation;

    #[derive(Default, Clone, Debug)]
    pub struct NaiveBackend(pub String);

    impl NaiveBackend {

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

    }

    fn assert_grapheme_boundaries(s: &str) {
        let naive = NaiveBackend(s.to_string());
        let rope = DocumentState {
            text_rope: Rope::from_str(s),
            ..Default::default()
        };

        let len = s.len();

        let mut offset = 0;
        while offset < len {
            let n_next = naive.next_boundary(offset);
            let r_next = rope.next_boundary(offset);
            assert_eq!(
                n_next, r_next,
                "next_boundary mismatch at offset {} in {:?}",
                offset, s
            );
            let n_prev = naive.prev_boundary(offset);
            let r_prev = rope.prev_boundary(offset);
            assert_eq!(
                n_prev, r_prev,
                "prev_boundary mismatch at offset {} in {:?}",
                offset, s
            );
            offset = n_next;
        }
    }

    #[test]
    fn grapheme_ascii() {
        assert_grapheme_boundaries("hello world");
    }

    #[test]
    fn grapheme_combining_marks() {
        assert_grapheme_boundaries("a\u{0301}e\u{0308}o\u{0323}");
    }

    #[test]
    fn grapheme_emoji_modifiers() {
        assert_grapheme_boundaries("👍🏽👍🏻👍");
    }

    #[test]
    fn grapheme_zwj_sequences() {
        assert_grapheme_boundaries("👩‍❤️‍👩👨‍👩‍👧‍👦");
    }

    #[test]
    fn grapheme_regional_indicators() {
        assert_grapheme_boundaries("🇺🇸🇨🇦🇯🇵");
    }

    #[test]
    fn grapheme_mixed_lines() {
        assert_grapheme_boundaries(
            "a\u{0301}\n👩‍❤️‍👩\n🇺🇸x"
        );
    }

    #[test]
    fn grapheme_large_ascii_many_nodes() {
        let s = "abcdefghijklmnopqrstuvwxyz".repeat(500); // ~13k bytes
        assert_grapheme_boundaries(&s);
    }

    #[test]
    fn grapheme_large_combining_runs() {
        let cluster = "a\u{0301}\u{0308}\u{0323}\u{0332}";
        let s = cluster.repeat(1000);
        assert_grapheme_boundaries(&s);
    }

    #[test]
    fn grapheme_large_zwj_sequences() {
        let zwj = "👩‍❤️‍👩asdfv";
        let s = zwj.repeat(500);
        assert_grapheme_boundaries(&s);
    }

    #[test]
    fn grapheme_large_mixed_text() {
        let s = [
            "a\u{0301}".repeat(500),
            "wtasdf\n".into(),
            "👨‍👩‍👧‍👦".repeat(300),
            "\n".into(),
            "🇺🇸🇨🇦".repeat(400),
            "xyz".repeat(500),
        ].concat();

        assert_grapheme_boundaries(&s);
    }
}
