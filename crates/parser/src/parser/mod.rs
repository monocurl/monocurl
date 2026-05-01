use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::PathBuf,
    sync::Arc,
    usize,
};

use lexer::token::Token;
use smallvec::SmallVec;
use structs::{
    rope::{Attribute, Rope, TextAggregate},
    text::{Count8, Span8},
};

use crate::{
    ast::{
        Anim, BinaryOperator, BinaryOperatorType, Block, Declaration, DirectionalLiteral,
        Expression, For, IdentifierDeclaration, IdentifierReference, If, LambdaArg, LambdaBody,
        LambdaDefinition, LambdaInvocation, Literal, NativeInvocation, OperatorDefinition,
        OperatorInvocation, Play, Print, Property, Return, Section, SectionBundle, SectionType,
        SpanTagged, Statement, Subscript, UnaryOperatorType, UnaryPreOperator, VariableType, While,
    },
    flatten_rope,
    import_context::{FileResult, ParseImportContext},
    parser::predicate::{
        BinaryOperatorPred, ExactPred, ExactPredDesc, InLambdaOrBlockPredicate, InLoopPredicate,
        InStdLibPredicate, NullPredicate, PlayablePredicate, RootTopLevelPredicate, StatePredicate,
        TokenPredicate, UnaryOperatorPred, VariableDeclarationPred,
    },
};

macro_rules! try_all {
    (@branches $self:expr, $token:expr, $span:expr, [], [$($collected:tt)*], $expected_override:expr) => {
        try_all!(@execute $self, $token, $span, [$($collected)*], $expected_override)
    };

    (@branches $self:expr, $token:expr, $span:expr,
        [else $else_body:expr],
        [$($collected:tt)*],
        $expected_override:expr) => {{
        try_all!(@execute_head $self, $token, $span, $expected_override, [$($collected)*]);
        return $else_body
    }};

    // state predicate and identifier
    (@branches $self:expr, $token:expr, $span:expr,
     [if $state_pred:expr, $var:ident = $token_pred:expr => $body:expr, $($rest:tt)*],
     [$($collected:tt)*],
     $expected_override:expr) => {
        try_all!(
            @branches $self, $token, $span,
            [$($rest)*],
            [$($collected)* (Some($state_pred), $var, $token_pred, $body),],
            $expected_override
        )
    };

    // state predicate, no identifier
    (@branches $self:expr, $token:expr, $span:expr,
     [if $state_pred:expr, $token_pred:expr => $body:expr, $($rest:tt)*],
     [$($collected:tt)*],
     $expected_override:expr) => {
        try_all!(
            @branches $self, $token, $span,
            [$($rest)*],
            [$($collected)* (Some($state_pred), _ignore, $token_pred, $body),],
            $expected_override
        )
    };

    // no state predicate, with identifier
    (@branches $self:expr, $token:expr, $span:expr,
     [$var:ident = $token_pred:expr => $body:expr, $($rest:tt)*],
     [$($collected:tt)*],
     $expected_override:expr) => {
        try_all!(
            @branches $self, $token, $span,
            [$($rest)*],
            [$($collected)* (Option::<NullPredicate>::None, $var, $token_pred, $body),],
            $expected_override
        )
    };

    // no state predicate, no identifier
    (@branches $self:expr, $token:expr, $span:expr,
     [$token_pred:expr => $body:expr, $($rest:tt)*],
     [$($collected:tt)*],
     $expected_override:expr) => {
        try_all!(
            @branches $self, $token, $span,
            [$($rest)*],
            [$($collected)* (Option::<NullPredicate>::None, _ignore, $token_pred, $body),],
            $expected_override
        )
    };

    (@execute_head
     $self:expr,
     $token:expr,
     $span:expr,
     $expected_override:expr,
     [$(($state_pred:expr, $var:ident, $token_pred:expr, $body:expr),)*]
    ) => {{
        if let Some(token) = $token {
            if $self.cursor_position.is_some()
                && $span.contains(&$self.cursor_position.unwrap().saturating_sub(1))
            {
                $(
                    let state_ok = if let Some(pred) = $state_pred {
                        pred.ok(&$self.state)
                    } else {
                        true
                    };

                    if state_ok {
                        $token_pred.emit_possibilities(&mut $self.artifacts);
                    }
                )*
            }

            $(
                let state_ok = if let Some(pred) = $state_pred {
                    pred.ok(&$self.state)
                } else {
                    true
                };

                if state_ok {
                    if let Some($var) = $token_pred.convert(token.clone()) {
                        #[allow(unreachable_code)]
                        return Ok($body);
                    }
                    // did not match, add to expectations list
                    if $expected_override.is_none() {
                        $self.next_token_expectations.push($token_pred.description());
                    }
                }
                else if $token_pred.convert(token.clone()).is_some() {
                    // valid match, but not in this context
                    $self.next_token_hints.push($state_pred.unwrap().fail_description());
                }

            )*

            // fail
            if let Some(expected) = $expected_override {
                $self.next_token_expectations.push(expected);
            }
        }
    }};

    (@execute
     $self:expr,
     $token:expr,
     $span:expr,
     [$(($state_pred:expr, $var:ident, $token_pred:expr, $body:expr),)*],
     $expected_override:expr
    ) => {{
        try_all!(
            @execute_head
            $self,
            $token,
            $span.clone(),
            $expected_override,
            [$(($state_pred, $var, $token_pred, $body),)*]
        );

        $self.emit_default_error($span);
        Err(())
    }};

    ($self:expr, expected = $expected:expr, { $($branches:tt)+ }) => {{
        (|| {
            let (token, _span) = {
                if let Some((token, span)) = $self.peek_token() {
                    (Some(*token), span.clone())
                } else {
                    let end = $self.tokens.get($self.state.top().operating_range.end)
                        .map(|(_, span)| span.end)
                        .unwrap_or_else(|| $self.text_rope.codeunits());
                    let span = end.saturating_sub(1)..end;
                    (None, span)
                }
            };
            try_all!(
                @branches
                $self,
                token,
                _span,
                [$($branches)*],
                [],
                $expected
            )
        })()
    }};

    ($self:expr, { $($branches:tt)+ }) => {{
        try_all!($self, expected = Option::<&str>::None, { $($branches)+ })
    }};
}

mod expressions;
mod imports;
mod literals;
mod predicate;
mod statements;
#[cfg(test)]
mod tests;

#[derive(Clone, Debug)]
struct ContextFrame {
    // token range that we are allowed to consider
    operating_range: Range<usize>,
    in_playable_block: bool,
    in_lambda_or_block: bool,
    in_loop: bool,
}

struct ShortTermState {
    frames: Vec<ContextFrame>,
    section_type: SectionType,
    root_import_span: Option<Span8>,
}

impl ShortTermState {
    fn new(section_type: SectionType, root_import_span: Option<Span8>, token_count: usize) -> Self {
        let top = match section_type {
            SectionType::Slide => ContextFrame {
                operating_range: 0..token_count,
                in_playable_block: true,
                in_lambda_or_block: false,
                in_loop: false,
            },
            _ => ContextFrame {
                operating_range: 0..token_count,
                in_playable_block: false,
                in_lambda_or_block: false,
                in_loop: false,
            },
        };

        ShortTermState {
            frames: vec![top],
            section_type,
            root_import_span,
        }
    }

    fn top(&self) -> &ContextFrame {
        self.frames.last().unwrap()
    }

    fn operating_range(&self) -> Range<usize> {
        self.top().operating_range.clone()
    }

    fn push_frame(&mut self, frame: impl FnOnce(&mut ContextFrame)) {
        let old = self.top().operating_range.clone();
        self.frames.push(self.top().clone());
        let current = self.frames.last_mut().unwrap();
        frame(current);
        debug_assert!(
            current.operating_range.start >= old.start && current.operating_range.end <= old.end
        );
    }

    fn pop_frame(&mut self) {
        self.frames.pop();
    }
}

struct Precomputation {
    bracket_partners: HashMap<usize, usize>,
}

impl Precomputation {
    fn new(tokens: &[(Token, Span8)]) -> Self {
        let mut bracket_partners = HashMap::new();
        let mut stack = Vec::new();

        for (i, (token, _)) in tokens.iter().enumerate() {
            match token {
                Token::LFlower | Token::LParen | Token::LBracket => {
                    stack.push((token, i));
                }
                Token::RFlower | Token::RParen | Token::RBracket => {
                    while let Some((open_token, open_index)) = stack.pop() {
                        let is_match = match (open_token, token) {
                            (Token::LFlower, Token::RFlower) => true,
                            (Token::LParen, Token::RParen) => true,
                            (Token::LBracket, Token::RBracket) => true,
                            _ => false,
                        };
                        bracket_partners.insert(open_index, i);
                        if is_match {
                            break;
                        }
                    }
                }
                _ => {}
            }
        }

        while let Some((_unmatched, open_index)) = stack.pop() {
            bracket_partners.insert(open_index, tokens.len());
        }

        Precomputation { bracket_partners }
    }

    fn bracket_internal_range(&self, bracket_index: usize) -> Range<usize> {
        if let Some(end) = self.bracket_partners.get(&bracket_index) {
            bracket_index + 1..*end
        } else {
            bracket_index..bracket_index
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub span: Span8,
    pub title: String,
    pub message: String,
}

pub struct SectionParser {
    precomputation: Precomputation,
    state: ShortTermState,
    cursor_position: Option<Count8>,

    text_rope: Rope<TextAggregate>,
    tokens: Vec<(Token, Span8)>,
    token_index: usize,

    // expectation strings of next token for error messages
    next_token_expectations: SmallVec<[&'static str; 8]>,
    next_token_hints: SmallVec<[&'static str; 1]>,

    artifacts: ParseArtifacts,
}

#[derive(Default, Clone)]
pub struct ParseArtifacts {
    pub error_diagnostics: Vec<Diagnostic>,
    pub cursor_possibilities: HashSet<Token>,
}

impl ParseArtifacts {
    fn extend(&mut self, other: ParseArtifacts) {
        self.error_diagnostics.extend(other.error_diagnostics);
        self.cursor_possibilities.extend(other.cursor_possibilities);
    }
}

impl SectionParser {
    pub fn artifacts(self) -> ParseArtifacts {
        self.artifacts
    }

    pub fn autocomplete_possibilities(&self) -> &HashSet<Token> {
        &self.artifacts.cursor_possibilities
    }
}

impl SectionParser {
    fn debug_assert_token(&self, pred: impl Fn(Token) -> bool) {
        debug_assert!(self.peek_token().is_some_and(|(tok, _)| { pred(*tok) }));
    }

    fn debug_assert_token_eq(&self, want: Token) {
        self.debug_assert_token(|tok| tok == want);
    }

    fn peek_token_description(&self) -> String {
        let Some((_, span)) = self.tokens.get(self.token_index) else {
            return "<end of section>".to_string();
        };
        self.text_rope
            .iterator_range(span.clone())
            .fold(String::new(), |mut acc, c| {
                if c == '\n' {
                    acc.push_str("<end of line>");
                } else {
                    acc.push(c);
                }
                acc
            })
    }

    fn peek_token(&self) -> Option<&(Token, Span8)> {
        if self.token_index == self.state.operating_range().end {
            return None;
        }

        Some(&self.tokens[self.token_index])
    }

    fn advance_token(&mut self) -> Span8 {
        debug_assert!(self.state.operating_range().contains(&self.token_index));
        self.next_token_expectations.clear();
        self.next_token_hints.clear();
        let span = self.tokens[self.token_index].1.clone();
        self.token_index += 1;
        span
    }

    fn nil_range(&self) -> Span8 {
        let last = if self.token_index == 0 {
            0
        } else {
            self.tokens[self.token_index - 1].1.end
        };
        return last..last;
    }
}

impl SectionParser {
    fn emit_default_error(&mut self, span: Span8) {
        let token_desc = self.peek_token_description();
        let title = format!("Illegal token: '{}'", token_desc);
        let mut error_message = String::new();

        if !self.next_token_hints.is_empty() {
            for hint in &self.next_token_hints {
                error_message.push_str(&format!("hint: may be illegal since {}\n", hint));
            }
            error_message.push('\n');
        }

        let mut seen = HashSet::new();
        self.next_token_expectations.retain(|x| seen.insert(*x));
        error_message.push_str("expected ");
        for (i, expectation) in self.next_token_expectations.iter().enumerate() {
            if i > 0 {
                if i == self.next_token_expectations.len() - 1 {
                    error_message.push_str(", or ");
                } else {
                    error_message.push_str(", ");
                }
            }
            error_message.push_str(expectation);
        }

        self.emit_error(title, error_message, span);
    }

    fn emit_error(&mut self, title: String, error_message: String, span: Span8) {
        // if on a newline, do it one before
        let mut modified_span = span;
        while modified_span.start > 0
            && self
                .text_rope
                .iterator(modified_span.start)
                .next()
                .is_none_or(|c| c == '\n')
        {
            modified_span.start = modified_span.start.saturating_sub(1);
        }

        if let Some(root_span) = self.state.root_import_span.clone() {
            // basically by only having "root span", querying the text rope
            self.artifacts.error_diagnostics.push(Diagnostic {
                span: root_span,
                title: "Nested Error".to_string(),
                message: title + " " + &error_message,
            })
        } else {
            self.artifacts.error_diagnostics.push(Diagnostic {
                span: modified_span,
                title,
                message: error_message,
            })
        }
    }
}

impl SectionParser {
    fn read_token(&mut self, token: Token) -> Result<Span8, ()> {
        try_all!(self, {
            ExactPred(token) => {
                self.advance_token()
            },
        })
    }

    fn read_token_best_effort(&mut self, token: Token) -> Span8 {
        self.read_token(token).unwrap_or(self.nil_range())
    }

    fn read_if_token(&mut self, token: Token) -> Option<Span8> {
        try_all!(self, {
            ExactPred(token) => {
                self.advance_token()
            },
            else {
                Err(())
            }
        })
        .ok()
    }

    fn advance_newlines(&mut self) {
        loop {
            if self.read_if_token(Token::Newline).is_none() {
                break;
            }
        }
    }
}

impl SectionParser {
    pub fn new(
        tokens: Vec<(Token, Span8)>,
        text: Rope<TextAggregate>,
        section_type: SectionType,
        root_import_span: Option<Span8>,
        cursor_position: Option<Count8>,
    ) -> Self {
        SectionParser {
            precomputation: Precomputation::new(&tokens),
            state: ShortTermState::new(section_type, root_import_span, tokens.len()),
            cursor_position,
            text_rope: text,
            tokens,
            token_index: 0,
            next_token_expectations: SmallVec::new(),
            next_token_hints: SmallVec::new(),
            artifacts: ParseArtifacts::default(),
        }
    }
}

pub struct Parser {
    errors: Vec<Diagnostic>,
    preparsed_files: Vec<PreparsedFile>,
    import_stack: Vec<PathBuf>,
}

struct PreparsedFile {
    imports: Vec<PathBuf>,
    path: PathBuf,
    text_rope: Rope<TextAggregate>,
    root_import_span: Option<Span8>,
    tokens: Vec<(Token, Span8)>,
    is_stdlib: bool,
}
