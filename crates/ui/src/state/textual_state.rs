use std::{cell::RefCell, isize, ops::Range, rc::Rc, sync::Arc};

use executor::transcript::SectionTranscript;
use gpui::{App, Context, Entity, ScrollHandle, Window};
use lexer::token::Token;
use smallvec::SmallVec;
use structs::{
    rope::{Attribute, RLEAggregate, RLEData, Rope, TextAggregate, leaves_from_str},
    text::{Count8, Count16, Location8, Span8, Span16},
};

use crate::{
    editor::text_editor::TextEditor,
    state::diagnostics::{Diagnostic, DiagnosticContainer},
};

pub type LexData = Token;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StaticAnalysisData {
    #[default]
    None,
    FunctionInvocation,
    OperatorInvocation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoCompleteCategory {
    Keyword,
    Variable,
    Function,
    Operator,
}

impl AutoCompleteCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::Variable => "variable",
            Self::Function => "function",
            Self::Operator => "operator",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoCompleteItem {
    pub head: String,
    pub replacement: String,
    pub cursor_anchor_delta: Location8,
    pub cursor_head_delta: Location8,
    pub category: AutoCompleteCategory,
}

impl AutoCompleteItem {
    pub fn apply(
        &self,
        replacement_size: Count8,
        to: &mut TextEditor,
        state: Entity<TextualState>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let cursor = {
            let state = state.read(cx);
            state.loc8_to_offset8(state.cursor().anchor)
        };
        let base = cursor - replacement_size;
        let range_utf8 = base..cursor;
        let new_text = &self.replacement;
        to.replace(range_utf8, new_text, window, cx);
        state.update(cx, |state, cx| {
            let base_loc8 = state.offset8_to_loc8(base);
            let cursor = Cursor {
                anchor: base_loc8 + self.cursor_anchor_delta,
                head: base_loc8 + self.cursor_head_delta,
            };
            state.set_cursor(cursor, cx);
        });
    }
}

#[derive(Default)]
pub struct AutoCompleteState {
    // expected items at this position
    pub items: Vec<AutoCompleteItem>,
    // index, and the the indices within head to be highlighted
    pub filtered_items: Vec<(usize, SmallVec<[usize; 8]>)>,

    // where we expect the cursor to be, if it is not here, then the autocomplete is disabled
    cursor_at: Location8,
    // the alphanumeric word that cursor lies after (possibly empty)
    pub cursor_token: String,
    pub selected_index: usize,
    pub forcefully_disabled: bool,
    pub scroll_handle: ScrollHandle,
}

impl AutoCompleteState {
    fn alphanumeric(&self, ch: char) -> bool {
        ch.is_alphanumeric() || ch == '_' || ch == ':'
    }

    fn token_starts_with_number(&self) -> bool {
        self.cursor_token
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_digit())
    }

    fn refilter(&mut self, can_only_shrink: bool) {
        if self.token_starts_with_number() {
            self.filtered_items.clear();
            self.selected_index = 0;
            return;
        }

        let base_indices: Vec<_> = if can_only_shrink {
            self.filtered_items.iter().map(|(i, _)| *i).collect()
        } else {
            (0..self.items.len()).collect()
        };
        self.filtered_items = base_indices
            .into_iter()
            .filter_map(|i| {
                let item = &self.items[i];
                // only allow if cursor_token is a subsequence of item.head
                let mut ct_iter = self.cursor_token.chars();
                let mut hd_iter = item.head.chars().enumerate();
                let mut subsequence = true;
                let mut indices = SmallVec::new();
                while let Some(ct_ch) = ct_iter.next() {
                    let mut found = false;
                    while let Some((idx, hd_ch)) = hd_iter.next() {
                        if ct_ch.to_ascii_lowercase() == hd_ch.to_ascii_lowercase() {
                            found = true;
                            indices.push(idx);
                            break;
                        }
                    }
                    if !found {
                        subsequence = false;
                        break;
                    }
                }
                if subsequence {
                    Some((i, indices))
                } else {
                    None
                }
            })
            .collect();
        // heuristic for ordering items
        self.filtered_items.sort_by_key(|(idx, indices)| {
            (
                indices.iter().map(|x| *x * *x).sum::<usize>(),
                self.items[*idx].head.len(),
            )
        });

        self.scroll_handle.scroll_to_item(0);
        self.selected_index = self.filtered_items.get(0).map(|(i, _)| *i).unwrap_or(0);
    }

    pub fn word_start(&self) -> Location8 {
        Location8 {
            row: self.cursor_at.row,
            col: self.cursor_at.col - self.cursor_token.len(),
        }
    }

    pub fn transition(&mut self, old: Span8, new: &str, state: &TextualState) {
        if new.is_empty() && old.len() == 1 {
            if self.cursor_token.len() == 1 {
                self.disable();
            } else if !self.forcefully_disabled {
                // assume backspace
                self.cursor_token.pop();
                self.cursor_at.col = self.cursor_at.col.saturating_sub(old.len());
                self.refilter(false);
            }
        } else if new.len() == 1 && old.is_empty() && self.alphanumeric(new.chars().next().unwrap())
        {
            if self.forcefully_disabled {
                // compute new state and enable
                self.forcefully_disabled = false;
                self.cursor_at = state.cursor().head;
                self.cursor_at.col += new.len();
                let chars_rev: String = state
                    .text_rope
                    .rev_iterator(state.loc8_to_offset8(self.cursor_at))
                    .take_while(|c| self.alphanumeric(*c))
                    .collect();
                self.cursor_token = chars_rev.chars().rev().collect();
                self.refilter(false);
            } else {
                self.cursor_at.col += new.len();
                self.cursor_token.push_str(new);
                self.refilter(true);
            }
        } else {
            self.disable();
        }
    }

    pub fn apply_selected(
        this: &Rc<RefCell<Self>>,
        editor: &mut TextEditor,
        state: Entity<TextualState>,
        window: &mut Window,
        cx: &mut Context<TextEditor>,
    ) {
        let index = this.borrow().selected_index;
        Self::apply_index(this, index, editor, state, window, cx);
    }

    pub fn apply_index(
        this: &Rc<RefCell<Self>>,
        index: usize,
        editor: &mut TextEditor,
        state: Entity<TextualState>,
        window: &mut Window,
        cx: &mut Context<TextEditor>,
    ) {
        let (replacement_size, item) = {
            let this = this.borrow();
            let item = this.items[index].clone();
            let replacement_size = this.cursor_token.len();
            (replacement_size, item)
        };
        item.apply(replacement_size, editor, state, window, cx);
        editor.reset_cursor_blink(cx);
        this.borrow_mut().disable();
    }

    pub fn cursor_moved_to(&mut self, new_cursor: &Cursor) {
        if new_cursor.is_empty() && new_cursor.head != self.cursor_at {
            self.disable();
        }
    }

    pub fn set_items(&mut self, items: Vec<AutoCompleteItem>) {
        self.items = items;
        self.refilter(false);
    }

    pub fn disable(&mut self) {
        self.cursor_token = "".to_string();
        self.selected_index = 0;
        self.forcefully_disabled = true;
    }

    pub fn move_index(&mut self, delta: isize) {
        let current_index_of_index = self
            .filtered_items
            .iter()
            .position(|(i, _)| *i == self.selected_index)
            .unwrap_or(0);
        let new_index = (current_index_of_index as isize + delta)
            .clamp(0, (self.filtered_items.len() as isize - 1).max(0))
            as usize;
        if new_index < self.filtered_items.len() {
            self.selected_index = self.filtered_items[new_index].0;
            self.scroll_handle.scroll_to_item(new_index);
        } else {
            self.selected_index = 0;
        }
    }

    pub fn recheck_should_display(&mut self, cursor: Cursor) -> bool {
        let check = Cursor {
            anchor: self.cursor_at,
            head: self.cursor_at,
        };
        if cursor != check {
            self.disable();
            return false;
        }

        !self.forcefully_disabled
            && !self.token_starts_with_number()
            && !self.filtered_items.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterHintArg {
    pub name: String,
    pub has_default: bool,
    pub is_reference: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParameterPositionHint {
    pub name: String,
    pub args: Vec<ParameterHintArg>,
    pub active_index: usize,
    pub function_start: Location8,
    pub is_operator: bool,
}

#[derive(Clone, Debug, Default)]
pub struct ParameterPositionState {
    pub hint: Option<ParameterPositionHint>,
    pub cursor_at: Location8,
}

impl ParameterPositionState {
    pub fn set_hint(&mut self, hint: Option<ParameterPositionHint>, cursor_at: Cursor) {
        self.hint = hint;
        self.cursor_at = cursor_at.head;
    }

    pub fn recheck_should_display(&mut self, cursor: Cursor) -> bool {
        if cursor.head != cursor.anchor {
            self.hint = None;
            return false;
        }

        self.hint.as_ref().is_some_and(|h| !h.args.is_empty())
    }
}

/// per-line index of root-originating transcript entries used for inline
/// rendering in the editor. non-root entries (library prints) are kept in the
/// console-side transcript only.
#[derive(Clone, Debug)]
pub struct InlineTranscriptEntry {
    pub span: Span8,
    pub text: String,
}

#[derive(Default)]
pub struct TranscriptIndex {
    sections: Vec<Arc<SectionTranscript>>,
    /// row -> entries that originated on that row (root sections only)
    by_line: std::collections::BTreeMap<usize, SmallVec<[InlineTranscriptEntry; 4]>>,
}

impl TranscriptIndex {
    pub fn entries_for_line(&self, row: usize) -> &[InlineTranscriptEntry] {
        self.by_line.get(&row).map(|v| v.as_slice()).unwrap_or(&[])
    }
}

#[derive(Default, Clone, Copy, Debug, PartialEq)]
pub struct Cursor {
    pub anchor: Location8,
    pub head: Location8,
}

impl Cursor {
    pub fn collapsed(pos: Location8) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn line_range(&self) -> Range<usize> {
        let start_row = self.anchor.min(self.head).row as usize;
        let end_row = self.anchor.max(self.head).row as usize;
        start_row..end_row + 1
    }

    pub fn reversed(&self) -> bool {
        self.head < self.anchor
    }
}

#[derive(Default)]
pub struct TransactionSummary {
    pub text_changes: Vec<(Span8, String, Rope<TextAggregate>, usize)>,
    pub new_cursor: Cursor,
    pub final_version: usize,
}

/// State that's relevant to the text editor + modified by compilation / lexing services
#[derive(Default)]
pub struct TextualState {
    cursor: Cursor,

    text_rope: Rope<TextAggregate>,

    // lex rope is the actual latest lexed attributes
    // rendered lex rope says at each char, what attributes are currently
    // used for the shaped lines. This may be differ for off-screen lines
    lex_rope: Rope<Attribute<LexData>>,
    rendered_lex_rope: Rope<Attribute<LexData>>,
    static_analysis_rope: Rope<Attribute<StaticAnalysisData>>,
    rendered_static_analysis_rope: Rope<Attribute<StaticAnalysisData>>,

    // we must relayout the lines corresponding to this span
    region_needing_relayout: Option<Range<usize>>,

    autocomplete: Rc<RefCell<AutoCompleteState>>,
    parameter_position: Rc<RefCell<ParameterPositionState>>,
    diagnostics: DiagnosticContainer,
    dirty_diagnostic_lines: Rope<RLEAggregate<bool>>,

    transcript: TranscriptIndex,

    version: usize,

    nested_transaction_count: usize,
    current_transaction: TransactionSummary,
    transaction_listeners: Vec<Box<dyn FnMut(&TransactionSummary, &mut App) + Send>>,
}

impl TextualState {
    pub fn add_transaction_listener(
        &mut self,
        f: impl FnMut(&TransactionSummary, &mut App) + Send + 'static,
    ) {
        self.transaction_listeners.push(Box::new(f));
    }

    pub fn start_transaction(&mut self) {
        self.nested_transaction_count += 1;
    }

    pub fn end_transaction(&mut self, cx: &mut App) {
        self.nested_transaction_count -= 1;
        if self.nested_transaction_count == 0 {
            self.current_transaction.final_version = self.version();
            self.current_transaction.new_cursor = self.cursor;
            self.notify_listeners(cx);
            self.current_transaction.text_changes.clear();
        }
    }

    fn notify_listeners(&mut self, cx: &mut App) {
        for listener in &mut self.transaction_listeners {
            listener(&self.current_transaction, cx);
        }
    }

    // returns the line range before and after that are affected
    pub fn replace(
        &mut self,
        span: Span8,
        new_text: &str,
        _cx: &mut App,
    ) -> (Range<usize>, Range<usize>) {
        debug_assert!(self.nested_transaction_count > 0);
        let transcript_dirty_rows = self.transcript.by_line.keys().copied().collect();
        let del_range = {
            let start_loc = self.offset8_to_loc8(span.start);
            let end_loc = self.offset8_to_loc8(span.end);
            start_loc.row..end_loc.row + 1
        };
        self.text_rope = self
            .text_rope
            .replace_range(span.clone(), leaves_from_str(new_text));
        let ins_range = {
            let start_loc = del_range.start;
            let end_loc = self.offset8_to_loc8(span.start + new_text.len());
            start_loc..end_loc.row + 1
        };

        self.autocomplete
            .borrow_mut()
            .transition(span.clone(), new_text, self);

        self.diagnostics
            .apply_replacement(span.clone(), new_text.len());
        self.dirty_diagnostic_lines = self.dirty_diagnostic_lines.replace_range(
            span.clone(),
            std::iter::once(RLEData {
                codeunits: ins_range.len(),
                attribute: false,
            }),
        );
        self.apply_transcript_replacement(span.clone(), new_text.len(), transcript_dirty_rows);

        // update lex_rope and static analysis rope with best effort of extending the previous runs
        // background threads will do the proper update asynchronously
        // Note that we update rendered lex_rope and static analysis rope too, but the actual value
        // does not really matter since they will be updated regardless when we relayout lines
        let lex_replacement = if span.start == 0 {
            LexData::default()
        } else {
            self.lex_rope.attribute_at(span.start - 1).clone()
        };
        self.lex_rope = self.lex_rope.replace_range(
            span.clone(),
            std::iter::once(RLEData {
                codeunits: new_text.len(),
                attribute: lex_replacement.clone(),
            }),
        );
        self.rendered_lex_rope = self.rendered_lex_rope.replace_range(
            span.clone(),
            std::iter::once(RLEData {
                codeunits: new_text.len(),
                attribute: lex_replacement,
            }),
        );

        let sa_replacement = if span.start == 0 {
            StaticAnalysisData::default()
        } else {
            *self.static_analysis_rope.attribute_at(span.start - 1)
        };
        self.static_analysis_rope = self.static_analysis_rope.replace_range(
            span.clone(),
            std::iter::once(RLEData {
                codeunits: new_text.len(),
                attribute: sa_replacement.clone(),
            }),
        );
        self.rendered_static_analysis_rope = self.rendered_static_analysis_rope.replace_range(
            span.clone(),
            std::iter::once(RLEData {
                codeunits: new_text.len(),
                attribute: sa_replacement,
            }),
        );

        self.version += 1;
        self.current_transaction.text_changes.push((
            span,
            new_text.into(),
            self.text_rope.clone(),
            self.version(),
        ));

        (del_range, ins_range)
    }

    pub fn read(&self, span: Span8) -> String {
        self.text_rope.iterator_range(span).collect()
    }

    pub fn version(&self) -> usize {
        self.version
    }

    pub fn len(&self) -> Count8 {
        self.text_rope.codeunits()
    }
}

impl TextualState {
    pub fn offset8_to_offset16(&self, offset: Count8) -> Count16 {
        let summary = self.text_rope.utf8_prefix_summary(offset);
        summary.codeunits_utf16
    }

    pub fn offset16_to_offset8(&self, offset: Count16) -> Count8 {
        let summary = self.text_rope.utf16_prefix_summary(offset);
        summary.bytes_utf8
    }

    pub fn loc8_to_offset8(&self, loc: Location8) -> Count8 {
        let summary = self.text_rope.utf8_line_pos_prefix(loc.row, loc.col);
        summary.bytes_utf8
    }

    pub fn offset8_to_loc8(&self, offset: Count8) -> Location8 {
        let summary = self.text_rope.utf8_prefix_summary(offset);
        Location8 {
            row: summary.newlines,
            col: summary.bytes_utf8_since_newline,
        }
    }

    pub fn span8_to_span16(&self, span8: &Span8) -> Span16 {
        self.offset8_to_offset16(span8.start)..self.offset8_to_offset16(span8.end)
    }

    pub fn span16_to_span8(&self, span16: &Span16) -> Span8 {
        self.offset16_to_offset8(span16.start)..self.offset16_to_offset8(span16.end)
    }
}

impl TextualState {
    pub fn cursor(&self) -> Cursor {
        self.cursor
    }

    pub fn cursor_range(&self) -> Span8 {
        let start = self.loc8_to_offset8(self.cursor.anchor.min(self.cursor.head));
        let end = self.loc8_to_offset8(self.cursor.anchor.max(self.cursor.head));
        start..end
    }

    pub fn set_cursor_head(&mut self, head: Location8, cx: &mut App) {
        debug_assert!(self.nested_transaction_count > 0);
        self.set_cursor(
            Cursor {
                anchor: self.cursor.anchor,
                head,
            },
            cx,
        );
    }

    pub fn set_cursor(&mut self, cursor: Cursor, _cx: &mut App) {
        debug_assert!(self.nested_transaction_count > 0);
        self.cursor = cursor;
        self.autocomplete_state()
            .borrow_mut()
            .cursor_moved_to(&self.cursor);
    }
}

impl TextualState {
    pub fn next_boundary(&self, offset: Count8) -> Count8 {
        grapheme_boundary(&self.text_rope, offset, true)
    }

    pub fn prev_boundary(&self, offset: Count8) -> Count8 {
        grapheme_boundary(&self.text_rope, offset, false)
    }

    // maximal word containing position
    // or, if this is empty due to whitespace, the previous word (along with contents up till offset)
    pub fn word(&self, offset: Count8, only_expand_left: bool) -> Span8 {
        let not_separator = |c: char| c.is_alphanumeric() || c == '_';

        let mut start = offset;
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
                } else {
                    first_nonwhitespace = first_nonwhitespace.max(start as isize);
                    if ch.chars().all(not_separator) {
                        start = prev;
                    } else if start as isize == first_nonwhitespace {
                        start = prev;
                        break;
                    } else {
                        break;
                    }
                }
            }
        }

        start..end
    }
}

impl TextualState {
    pub fn text_rope(&self) -> &Rope<TextAggregate> {
        &self.text_rope
    }

    pub fn lex_rope(&self) -> &Rope<Attribute<LexData>> {
        &self.lex_rope
    }

    pub fn set_lex_rope(&mut self, rope: Rope<Attribute<LexData>>, for_version: usize) -> bool {
        if for_version != self.version {
            return false;
        }
        self.lex_rope = rope;
        true
    }

    pub fn static_analysis_rope(&self) -> &Rope<Attribute<StaticAnalysisData>> {
        &self.static_analysis_rope
    }

    pub fn set_static_analysis_rope(
        &mut self,
        rope: Rope<Attribute<StaticAnalysisData>>,
        for_version: usize,
    ) -> bool {
        if for_version != self.version {
            return false;
        }
        self.static_analysis_rope = rope;
        true
    }

    pub fn mark_line_as_up_to_date_attributes(&mut self, line_no: usize, start: usize, end: usize) {
        let lex_content =
            self.lex_rope
                .iterator_range(start..end)
                .map(|(bytes_utf8, attribute)| RLEData {
                    codeunits: bytes_utf8,
                    attribute,
                });

        self.rendered_lex_rope = self
            .rendered_lex_rope
            .replace_range(start..end, lex_content);

        let sa_content =
            self.static_analysis_rope
                .iterator_range(start..end)
                .map(|(bytes_utf8, attribute)| RLEData {
                    codeunits: bytes_utf8,
                    attribute,
                });
        self.rendered_static_analysis_rope = self
            .rendered_static_analysis_rope
            .replace_range(start..end, sa_content);

        self.dirty_diagnostic_lines = self.dirty_diagnostic_lines.replace_range(
            line_no..line_no + 1,
            std::iter::once(RLEData {
                codeunits: 1,
                attribute: false,
            }),
        );
    }

    pub fn line_has_new_attributes(&self, line_no: usize) -> bool {
        if *self.dirty_diagnostic_lines.attribute_at(line_no) {
            return true;
        }

        let line_start = self.text_rope.utf8_line_pos_prefix(line_no, 0).bytes_utf8;
        let line_end = self
            .text_rope
            .utf8_line_pos_prefix(line_no + 1, 0)
            .bytes_utf8;

        let lex_diff = {
            let lex_content = self.lex_rope.iterator_range(line_start..line_end);
            let rendered_lex_content = self.rendered_lex_rope.iterator_range(line_start..line_end);

            std::iter::zip(lex_content, rendered_lex_content).any(|(a, b)| a != b)
        };

        if lex_diff {
            return true;
        }

        let sa_diff = {
            let sa_content = self
                .static_analysis_rope
                .iterator_range(line_start..line_end);
            let rendered_sa_content = self
                .rendered_static_analysis_rope
                .iterator_range(line_start..line_end);

            std::iter::zip(sa_content, rendered_sa_content).any(|(a, b)| a != b)
        };

        return sa_diff;
    }

    // line that have modified attributes
    // and resets their dirty status to non dirty
    // only used for width changes (and probably would need work for anything more since we would
    // have to modify the range upon insertions / deletions)
    pub fn mark_lines_needing_relayout(&mut self, span: Range<usize>) {
        if let Some(dirty) = &mut self.region_needing_relayout {
            dirty.start = dirty.start.min(span.start);
            dirty.end = dirty.end.max(span.end);
        } else {
            self.region_needing_relayout = Some(span);
        }
    }

    pub fn take_lines_needing_relayout(&mut self) -> Option<Range<usize>> {
        self.region_needing_relayout.take()
    }
}

impl TextualState {
    fn transcript_row_for_span(&self, span: &Span8) -> usize {
        let len = self.len();
        let start = span.start.min(len);
        let end = span.end.min(len).max(start);
        let offset = if end == start {
            start
        } else {
            end.saturating_sub(1)
        };

        self.offset8_to_loc8(offset).row
    }

    fn mark_transcript_rows_dirty(&mut self, rows: impl IntoIterator<Item = usize>) {
        let line_count = self.text_rope.utf8_prefix_summary(self.len()).newlines + 1;
        if line_count == 0 {
            return;
        }

        for row in rows {
            let row = row.min(line_count - 1);
            self.dirty_diagnostic_lines = self.dirty_diagnostic_lines.replace_range(
                row..row + 1,
                std::iter::once(RLEData {
                    codeunits: 1,
                    attribute: true,
                }),
            );
        }
    }

    fn apply_transcript_replacement(
        &mut self,
        old: Span8,
        new: Count8,
        mut changed_rows: std::collections::BTreeSet<usize>,
    ) {
        if self.transcript.sections.is_empty() {
            return;
        }

        let modify_pos = |pos: &mut Count8| {
            if *pos >= old.end {
                *pos = (*pos - old.len()) + new;
            } else if *pos > old.start {
                *pos = old.start;
            }
        };

        let sections = self
            .transcript
            .sections
            .iter()
            .map(|section| {
                let mut section = (**section).clone();
                for entry in &mut section.entries {
                    modify_pos(&mut entry.span.start);
                    modify_pos(&mut entry.span.end);
                }
                Arc::new(section)
            })
            .collect::<Vec<_>>();

        let mut by_line: std::collections::BTreeMap<usize, SmallVec<[InlineTranscriptEntry; 4]>> =
            std::collections::BTreeMap::new();
        for section in &sections {
            for entry in &section.entries {
                if !entry.is_root {
                    continue;
                }
                let row = self.transcript_row_for_span(&entry.span);
                changed_rows.insert(row);
                by_line.entry(row).or_default().push(InlineTranscriptEntry {
                    span: entry.span.clone(),
                    text: entry.text().to_string(),
                });
            }
        }

        self.transcript = TranscriptIndex { sections, by_line };
        self.mark_transcript_rows_dirty(changed_rows);
    }

    fn set_dirty_flags_on_diagnostics_change(
        &mut self,
        new_diags: &Vec<Diagnostic>,
        filter: impl Fn(&Diagnostic) -> bool + Copy,
    ) -> bool {
        debug_assert!(new_diags.iter().all(filter));

        for diag in self
            .diagnostics
            .diagnostics_list()
            .iter()
            .chain(new_diags.iter())
            .filter(|d| filter(d))
        {
            let start_loc = self.offset8_to_loc8(diag.span.start);
            let end_loc = self.offset8_to_loc8(diag.span.end);

            self.dirty_diagnostic_lines = self.dirty_diagnostic_lines.replace_range(
                start_loc.row..end_loc.row + 1,
                std::iter::once(RLEData {
                    codeunits: end_loc.row - start_loc.row + 1,
                    attribute: true,
                }),
            );
        }

        true
    }

    pub fn set_compile_diagnostics(
        &mut self,
        diagnostics: Vec<Diagnostic>,
        for_version: usize,
    ) -> bool {
        if for_version != self.version {
            return false;
        }

        self.set_dirty_flags_on_diagnostics_change(&diagnostics, |d| d.is_compile_time());
        self.diagnostics
            .set_compile_time_diagnostics(diagnostics.into_iter());
        true
    }

    pub fn set_runtime_diagnostics(
        &mut self,
        diagnostics: Vec<Diagnostic>,
        for_version: usize,
    ) -> bool {
        if for_version != self.version {
            return false;
        }

        self.set_dirty_flags_on_diagnostics_change(&diagnostics, |d| d.is_runtime());
        self.diagnostics
            .set_runtime_diagnostics(diagnostics.into_iter());
        true
    }

    pub fn diagnostics(&self) -> &DiagnosticContainer {
        &self.diagnostics
    }

    pub fn transcript(&self) -> &TranscriptIndex {
        &self.transcript
    }

    pub fn set_transcript(
        &mut self,
        new_sections: Vec<Arc<SectionTranscript>>,
        for_version: usize,
    ) -> bool {
        if for_version != self.version {
            return false;
        }

        // collect rows that change so we can dirty them
        let mut changed_rows: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();

        let old = &self.transcript.sections;
        let n_old = old.len();
        let n_new = new_sections.len();
        let max = n_old.max(n_new);
        let mut differs = false;
        for i in 0..max {
            let old_section = old.get(i);
            let new_section = new_sections.get(i);
            match (old_section, new_section) {
                (Some(a), Some(b)) if Arc::ptr_eq(a, b) => continue,
                _ => {
                    differs = true;
                }
            }
            for entry in old_section.iter().flat_map(|s| s.entries.iter()) {
                if !entry.is_root {
                    continue;
                }
                changed_rows.insert(self.transcript_row_for_span(&entry.span));
            }
            for entry in new_section.iter().flat_map(|s| s.entries.iter()) {
                if !entry.is_root {
                    continue;
                }
                changed_rows.insert(self.transcript_row_for_span(&entry.span));
            }
        }

        if !differs {
            return false;
        }

        // rebuild by_line index
        let mut by_line: std::collections::BTreeMap<usize, SmallVec<[InlineTranscriptEntry; 4]>> =
            std::collections::BTreeMap::new();
        for section in &new_sections {
            for entry in &section.entries {
                if !entry.is_root {
                    continue;
                }
                let row = self.transcript_row_for_span(&entry.span);
                by_line.entry(row).or_default().push(InlineTranscriptEntry {
                    span: entry.span.clone(),
                    text: entry.text().to_string(),
                });
            }
        }

        self.transcript = TranscriptIndex {
            sections: new_sections,
            by_line,
        };

        // dirty all changed rows so reshape picks them up
        for row in &changed_rows {
            self.dirty_diagnostic_lines = self.dirty_diagnostic_lines.replace_range(
                *row..*row + 1,
                std::iter::once(RLEData {
                    codeunits: 1,
                    attribute: true,
                }),
            );
        }

        true
    }

    pub fn prepare_diagnostics_iterator(&mut self) {
        self.diagnostics.prepare_iterator();
    }
}

impl TextualState {
    pub fn autocomplete_state(&self) -> Rc<RefCell<AutoCompleteState>> {
        self.autocomplete.clone()
    }

    pub fn set_autocomplete_state(
        &mut self,
        items: Vec<AutoCompleteItem>,
        for_version: usize,
        for_cursor: Cursor,
    ) -> bool {
        if for_version != self.version
            || self.cursor() != for_cursor
            || self.autocomplete.borrow().items == items
        {
            return false;
        }
        let mut ac_state = self.autocomplete.borrow_mut();
        ac_state.set_items(items);
        true
    }

    pub fn parameter_position_state(&self) -> Rc<RefCell<ParameterPositionState>> {
        self.parameter_position.clone()
    }

    pub fn set_parameter_position_state(
        &mut self,
        state: Option<ParameterPositionHint>,
        for_version: usize,
        for_cursor: Cursor,
    ) -> bool {
        if for_version != self.version || self.cursor() != for_cursor {
            return false;
        }
        let mut param_state = self.parameter_position.borrow_mut();
        // update latest cursor, but no need to notify
        let ret = state != param_state.hint || for_cursor.head != param_state.cursor_at;
        param_state.set_hint(state, for_cursor);
        ret
    }
}

fn grapheme_boundary<const N: usize>(
    rope: &Rope<TextAggregate, N>,
    offset: Count8,
    forward: bool,
) -> Count8 {
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
                    if utf8 >= CHUNK {
                        break;
                    }
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
                    if utf8 >= CHUNK {
                        break;
                    }
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
                    if bytes >= CHUNK {
                        break;
                    }
                    buffer.push(ch);
                    bytes += ch.len_utf8();
                }

                buffer = buffer.chars().rev().collect();

                cursor.provide_context(&buffer, ctx_end - bytes);
            }
            Err(GraphemeIncomplete::InvalidOffset) => {
                log::error!(
                    "Invalid offset passed to grapheme boundary detection: {}",
                    offset
                );
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

#[cfg(test)]
mod tests {
    use super::*;
    use executor::transcript::{TranscriptEntry, TranscriptEntryKind};
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
        let rope = TextualState {
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

    fn autocomplete_item(head: &str) -> AutoCompleteItem {
        AutoCompleteItem {
            head: head.to_string(),
            replacement: head.to_string(),
            cursor_anchor_delta: Location8 { row: 0, col: 0 },
            cursor_head_delta: Location8 { row: 0, col: 0 },
            category: AutoCompleteCategory::Keyword,
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
        assert_grapheme_boundaries("a\u{0301}\n👩‍❤️‍👩\n🇺🇸x");
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
        ]
        .concat();

        assert_grapheme_boundaries(&s);
    }

    #[test]
    fn autocomplete_hidden_when_token_starts_with_digit() {
        let mut state = AutoCompleteState {
            cursor_at: Location8 { row: 0, col: 2 },
            cursor_token: "10".to_string(),
            ..Default::default()
        };
        state.set_items(vec![autocomplete_item("log10")]);

        assert!(state.filtered_items.is_empty());
        assert!(!state.recheck_should_display(Cursor::collapsed(state.cursor_at)));
    }

    #[test]
    fn autocomplete_still_shows_for_identifier_prefixes() {
        let mut state = AutoCompleteState {
            cursor_at: Location8 { row: 0, col: 2 },
            cursor_token: "lo".to_string(),
            ..Default::default()
        };
        state.set_items(vec![autocomplete_item("log10")]);

        assert_eq!(state.filtered_items.len(), 1);
        assert!(state.recheck_should_display(Cursor::collapsed(state.cursor_at)));
    }

    #[test]
    fn transcript_entry_for_multiline_span_indexes_tail_row_only() {
        let src = "print [\n    1\n]\n";
        let mut state = TextualState {
            text_rope: Rope::from_str(src),
            ..Default::default()
        };
        let section = SectionTranscript {
            entries: vec![TranscriptEntry {
                span: 0..src.find(']').unwrap() + 1,
                section: 0,
                is_root: true,
                kind: TranscriptEntryKind::String("[1]".to_string()),
            }],
        };

        assert!(state.set_transcript(vec![Arc::new(section)], 0));

        assert!(state.transcript().entries_for_line(0).is_empty());
        assert!(state.transcript().entries_for_line(1).is_empty());
        assert_eq!(state.transcript().entries_for_line(2).len(), 1);
    }
}
