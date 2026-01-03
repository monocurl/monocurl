use structs::{rope::{RLEAggregate, Rope, TextAggregate, leaves_from_str}, text::{Count8, Count16, Location8, Span8}};

use crate::editor::backing::{AutoCompleteItem, Diagnostic, EditorBackend};

#[derive(Default)]
pub struct DocumentState {

    pub text_rope: Rope<TextAggregate>,
    pub lex_rope: Rope<RLEAggregate<i32>>,
    pub static_analysis_rope: Rope<RLEAggregate<i32>>,

    pub dirty_range: Option<Span8>,
    pub version: usize,

    pub listeners: Vec<Box<dyn FnMut(Span8, &str, &Rope<TextAggregate>, usize) + Send>>,
}

impl DocumentState {

    pub fn add_listener(&mut self, f: impl FnMut(Span8, &str, &Rope<TextAggregate>, usize) + Send + 'static) {
        self.listeners.push(Box::new(f));
    }

    pub fn notify_listeners(&mut self, span: Span8, new_text: &str) {
        for listener in &mut self.listeners {
            listener(span.clone(), new_text, &self.text_rope, self.version);
        }
    }

    pub fn set_lex_rope(&mut self, for_version: usize, dirty_range: Span8) {
        if for_version != self.version {
            return;
        }
        self.mark_region_as_dirty(dirty_range);
    }

    pub fn set_static_analysis_rope(&mut self, for_version: usize, dirty_range: Span8) {
        if for_version != self.version {
            return;
        }
        self.mark_region_as_dirty(dirty_range);
    }

    pub fn set_diagnostic_state(&self) {

    }
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
        self.notify_listeners(span, new_text);
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
