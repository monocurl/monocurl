use gpui::TextRun;
use lexer::token::TokenCategory;
use structs::text::Count8;

use crate::{state::textual_state::{LexData, StaticAnalysisData}, theme::TextEditorStyles};

// It may be assumed that the style of any text run
// does not affect the layout. In particular, that on lex / static analysis rope changes
// we can lazily re-shape the line once it becomes visible instead of as soon as it changes
pub struct LineShaper<'a, LexIt, AnalysisIt>
where
    LexIt: Iterator<Item = (Count8, LexData)>,
    AnalysisIt: Iterator<Item = (Count8, StaticAnalysisData)>,
{
    style: &'a TextEditorStyles,
    lex_it: LexIt,
    analysis_it: AnalysisIt,
    lex_item: Option<(Count8, LexData)>,
    analysis_item: Option<(Count8, StaticAnalysisData)>,
    remaining: Count8
}

impl<'a, LexIt, AnalysisIt> LineShaper<'a, LexIt, AnalysisIt>
where
    LexIt: Iterator<Item = (Count8, LexData)>,
    AnalysisIt: Iterator<Item = (Count8, StaticAnalysisData)>,
{
    pub fn new(style: &'a TextEditorStyles, lex_it: LexIt, analysis_it: AnalysisIt, len: Count8) -> Self {
        Self {
            style,
            lex_it,
            analysis_it,
            lex_item: None,
            analysis_item: None,
            remaining: len,
        }
    }

    fn combine_chunk(&self, size: usize, lex_data: &LexData, _analysis_data: &StaticAnalysisData) -> TextRun {
        let t_category = lex_data.category();

        // ignore analysis data for now
        let color = match t_category {
            // doesn't really matter for this one
            TokenCategory::Whitespace => gpui::white(),
            TokenCategory::Comment => self.style.comment_color,
            TokenCategory::TextLiteral => self.style.text_literal_color,
            TokenCategory::NumericLiteral => self.style.numeric_literal_color,
            TokenCategory::Identifier => self.style.identifier_color,
            TokenCategory::Operator => self.style.operator_color,
            TokenCategory::Punctutation => self.style.punctuation_color,
            TokenCategory::ControlFlow => self.style.control_flow_color,
            TokenCategory::NonControlFlowKeyword => self.style.non_control_flow_keyword_color,
            TokenCategory::Unknown => self.style.default_text_color,
        };

        TextRun {
            len: size,
            font: self.style.text_font.clone(),
            color,
            background_color: None,
            underline: None,
            strikethrough: None
        }
    }
}

impl<'a, LexIt, AnalysisIt> Iterator for LineShaper<'a, LexIt, AnalysisIt>
where
    LexIt: Iterator<Item = (Count8, LexData)>,
    AnalysisIt: Iterator<Item = (Count8, StaticAnalysisData)>,
{
    type Item = TextRun;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        while self.lex_item.as_ref().is_none_or(|(c, _)| *c == 0) {
            self.lex_item = Some(self.lex_it.next()?);
        }
        while self.analysis_item.as_ref().is_none_or(|(c, _)| *c == 0) {
            self.analysis_item = Some(self.analysis_it.next()?);
        }

        let chunk_size =
            self.remaining
                .min(self.lex_item.as_ref().unwrap().0)
                .min(self.analysis_item.as_ref().unwrap().0);

        self.remaining -= chunk_size;
        self.lex_item.as_mut().unwrap().0 -= chunk_size;
        self.analysis_item.as_mut().unwrap().0 -= chunk_size;

        let lex_data = &self.lex_item.as_ref().unwrap().1;
        let analysis_data = &self.analysis_item.as_ref().unwrap().1;

        Some(self.combine_chunk(chunk_size, lex_data, analysis_data))
    }
}
