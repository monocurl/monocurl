use std::{collections::{HashMap, HashSet}, ops::Range, path::PathBuf, usize};

use lexer::token::Token;
use smallvec::SmallVec;
use structs::{rope::{Rope, TextAggregate}, text::{Count8, Span8}};

use crate::{ast::{Anim, BinaryOperator, BinaryOperatorType, Block, Declaration, DirectionalLiteral, Expression, For, IdentifierDeclaration, IdentifierReference, If, LambdaBody, LambdaDefinition, LambdaInvocation, Literal, NativeInvocation, OperatorDefinition, OperatorInvocation, Play, Property, Return, Section, SectionType, SpanTagged, Statement, Subscript, UnaryOperatorType, UnaryPreOperator, VariableType, While}, parser::predicate::{BinaryOperatorPred, ExactPred, ExactPredDesc, InLambdaOrBlockPredicate, InLoopPredicate, InStdLibPredicate, NullPredicate, PlayablePredicate, StatePredicate, TokenPredicate, UnaryOperatorPred, VariableDeclarationPred}};

// Only gives error if unable to produce anything at all
type Result<T> = std::result::Result<T, ()>;

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


mod predicate {
    use lexer::token::Token;

    use crate::{ast::{BinaryOperatorType, SectionType, UnaryOperatorType, VariableType}, parser::{ParseArtifacts, State}};

    pub(super) trait StatePredicate {
        fn ok(&self, state: &State) -> bool;
        fn fail_description(&self) -> &'static str;
    }

    pub(super) trait TokenPredicate {
        type Output;

        fn convert(&self, token: Token) -> Option<Self::Output>;
        fn description(&self) -> &'static str;
        fn emit_possibilities(&self, dump: &mut ParseArtifacts);
    }

    pub(super) struct NullPredicate;
    impl StatePredicate for NullPredicate {
        fn ok(&self, _state: &State) -> bool {
            true
        }

        fn fail_description(&self) -> &'static str {
            unreachable!()
        }
    }

    pub(super) struct InLoopPredicate;
    impl StatePredicate for InLoopPredicate {
        fn ok(&self, state: &State) -> bool {
            state.top().in_loop
        }

        fn fail_description(&self) -> &'static str {
            "we are not directly inside a loop"
        }
    }

    pub(super) struct PlayablePredicate;
    impl StatePredicate for PlayablePredicate {
        fn ok(&self, state: &State) -> bool {
            state.top().in_playable_block
        }

        fn fail_description(&self) -> &'static str {
            "we are not in an anim body"
        }
    }

    pub(super) struct InLambdaOrBlockPredicate;
    impl StatePredicate for InLambdaOrBlockPredicate {
        fn ok(&self, state: &State) -> bool {
            state.top().in_lambda_or_block
        }

        fn fail_description(&self) -> &'static str {
            "we are not in a lambda or block body"
        }
    }

    pub(super) struct InStdLibPredicate;
    impl StatePredicate for InStdLibPredicate {
        fn ok(&self, state: &State) -> bool {
            state.section_type == SectionType::StandardLibrary
        }

        fn fail_description(&self) -> &'static str {
            "this keyword is reserved for the Monocurl compiler"
        }
    }

    pub(super) struct ExactPredDesc(pub Token, pub &'static str);
    impl TokenPredicate for ExactPredDesc {
        type Output = Token;

        fn convert(&self, token: Token) -> Option<Self::Output> {
            if token == self.0 {
                Some(token)
            }
            else {
                None
            }
        }

        fn description(&self) -> &'static str {
            self.1
        }

        fn emit_possibilities(&self, dump: &mut ParseArtifacts) {
            dump.cursor_possibilities.insert(self.0);
        }
    }

    pub(super) struct ExactPred(pub Token);
    impl TokenPredicate for ExactPred {
        type Output = Token;

        fn convert(&self, token: Token) -> Option<Self::Output> {
            if token == self.0 {
                Some(token)
            }
            else {
                None
            }
        }

        fn description(&self) -> &'static str {
            self.0.description()
        }

        fn emit_possibilities(&self, dump: &mut ParseArtifacts) {
            dump.cursor_possibilities.insert(self.0);
        }
    }

    pub(super) struct UnaryOperatorPred;
    impl TokenPredicate for UnaryOperatorPred {
        type Output = UnaryOperatorType;

        fn convert(&self, token: Token) -> Option<Self::Output> {
            match token {
                Token::Not => Some(UnaryOperatorType::Not),
                Token::Minus => Some(UnaryOperatorType::Negative),
                _ => None
            }
        }

        fn description(&self) -> &'static str {
            "<unary operator>"
        }

        fn emit_possibilities(&self, dump: &mut ParseArtifacts) {
            dump.cursor_possibilities.insert(Token::Not);
            dump.cursor_possibilities.insert(Token::Minus);
        }
    }

    pub(super) struct BinaryOperatorPred;
    impl TokenPredicate for BinaryOperatorPred {
        type Output = BinaryOperatorType;

        fn convert(&self, token: Token) -> Option<Self::Output> {
            match token {
                Token::Append => Some(BinaryOperatorType::Append),
                Token::And => Some(BinaryOperatorType::And),
                Token::Or => Some(BinaryOperatorType::Or),
                Token::Plus => Some(BinaryOperatorType::Add),
                Token::Minus => Some(BinaryOperatorType::Subtract),
                Token::Multiply => Some(BinaryOperatorType::Multiply),
                Token::Divide => Some(BinaryOperatorType::Divide),
                Token::IntegerDivide => Some(BinaryOperatorType::IntegerDivide),
                Token::Power => Some(BinaryOperatorType::Power),
                Token::Eq => Some(BinaryOperatorType::Power),
                Token::Ne => Some(BinaryOperatorType::Ne),
                Token::Lt => Some(BinaryOperatorType::Lt),
                Token::Le => Some(BinaryOperatorType::Le),
                Token::Gt => Some(BinaryOperatorType::Gt),
                Token::Ge => Some(BinaryOperatorType::In),
                Token::Assign => Some(BinaryOperatorType::Assign),
                Token::DotAssign => Some(BinaryOperatorType::DotAssign),
                _ => None
            }
        }

        fn description(&self) -> &'static str {
            "<binary operator>"
        }

        fn emit_possibilities(&self, dump: &mut ParseArtifacts) {
            dump.cursor_possibilities.insert(Token::Append);
            dump.cursor_possibilities.insert(Token::And);
            dump.cursor_possibilities.insert(Token::Or);
            dump.cursor_possibilities.insert(Token::Plus);
            dump.cursor_possibilities.insert(Token::Minus);
            dump.cursor_possibilities.insert(Token::Multiply);
            dump.cursor_possibilities.insert(Token::Divide);
            dump.cursor_possibilities.insert(Token::IntegerDivide);
            dump.cursor_possibilities.insert(Token::Power);
            dump.cursor_possibilities.insert(Token::Eq);
            dump.cursor_possibilities.insert(Token::Ne);
            dump.cursor_possibilities.insert(Token::Lt);
            dump.cursor_possibilities.insert(Token::Le);
            dump.cursor_possibilities.insert(Token::Gt);
            dump.cursor_possibilities.insert(Token::Ge);
            dump.cursor_possibilities.insert(Token::Assign);
            dump.cursor_possibilities.insert(Token::DotAssign);
        }
    }

    pub(super) struct VariableDeclarationPred;
    impl TokenPredicate for VariableDeclarationPred {
        type Output = VariableType;

        fn convert(&self, token: Token) -> Option<Self::Output> {
            match token {
                Token::Let => Some(VariableType::Let),
                Token::Var => Some(VariableType::Var),
                Token::Mesh => Some(VariableType::Mesh),
                Token::State => Some(VariableType::State),
                Token::Param => Some(VariableType::Param),
                _ => None
            }
        }

        fn description(&self) -> &'static str {
            "<variable declaration>"
        }

        fn emit_possibilities(&self, dump: &mut ParseArtifacts) {
            dump.cursor_possibilities.insert(Token::Let);
            dump.cursor_possibilities.insert(Token::Var);
            dump.cursor_possibilities.insert(Token::Mesh);
            dump.cursor_possibilities.insert(Token::State);
            dump.cursor_possibilities.insert(Token::Param);
        }
    }
}

// context mainly related about finding additional
struct ExternalContext {
    working_directory: PathBuf,
}

#[derive(Clone, Debug)]
struct ContextFrame {
    // token range that we are allowed to consider
    operating_range: Range<usize>,
    in_playable_block: bool,
    in_lambda_or_block: bool,
    in_loop: bool,
}

struct State {
    frames: Vec<ContextFrame>,
    section_type: SectionType
}

impl State {
    fn new(section_type: SectionType, token_count: usize) -> Self {
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
                in_loop: false
            }
        };

        State {
            frames: vec![top],
            section_type,
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
            current.operating_range.start >= old.start &&
            current.operating_range.end <= old.end
        );
    }

    fn pop_frame(&mut self) {
        self.frames.pop();
    }
}

struct Precomputation {
    bracket_partners: HashMap<usize, usize>
}

impl Precomputation {
    fn new(tokens: &[(Token, Span8)]) -> Self {
        let mut bracket_partners = HashMap::new();
        let mut stack = Vec::new();

        for (i, (token, _)) in tokens.iter().enumerate() {
            match token {
                Token::LFlower | Token::LParen | Token::LBracket => {
                    stack.push((token, i));
                },
                Token::RFlower | Token::RParen | Token::RBracket => {
                    while let Some((open_token, open_index)) = stack.pop() {
                        let is_match = match (open_token, token) {
                            (Token::LFlower, Token::RFlower) => true,
                            (Token::LParen, Token::RParen) => true,
                            (Token::LBracket, Token::RBracket) => true,
                            _ => false
                        };
                        bracket_partners.insert(open_index, i);
                        if is_match {
                            break;
                        }
                    }
                },
                _ => {}
            }
        }

        while let Some((_unmatched, open_index)) = stack.pop() {
            bracket_partners.insert(open_index, tokens.len());
        }

        Precomputation {
            bracket_partners
        }
    }

    fn bracket_internal_range(&self, bracket_index: usize) -> Range<usize> {
        if let Some(end) = self.bracket_partners.get(&bracket_index) {
            bracket_index + 1 .. *end
        }
        else {
            bracket_index + 1 .. bracket_index + 1
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub is_error: bool,
    pub span: Span8,
    pub title: String,
    pub message: String,
}

pub struct SectionParser {
    precomputation: Precomputation,
    state: State,
    cursor_position: Option<Count8>,

    text_rope: Rope<TextAggregate>,
    tokens: Vec<(Token, Span8)>,
    token_index: usize,

    // expectation strings of next token for error messages
    next_token_expectations: SmallVec<[&'static str; 8]>,
    next_token_hints: SmallVec<[&'static str; 1]>,

    artifacts: ParseArtifacts
}

pub struct Parser;

#[derive(Default)]
pub struct ParseArtifacts {
    pub diagnostics: Vec<Diagnostic>,
    pub cursor_possibilities: HashSet<Token>
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
        debug_assert!(self.peek_token().is_some_and(|(tok, _)| {
            pred(*tok)
        }));
    }

    fn debug_assert_token_eq(&self, want: Token) {
        self.debug_assert_token(|tok| tok == want);
    }

    fn peek_token_description(&self) -> String {
        let Some((_, span)) = self.tokens.get(self.token_index) else {
            return "<end of section>".to_string()
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
}

impl SectionParser {
    fn emit_default_error(&mut self, span: Span8) {
        let token_desc = self.peek_token_description();
        let title = format!("Illegal token: '{}'", token_desc);
        let mut error_message = String::new();

        if !self.next_token_hints.is_empty() {
            for hint in &self.next_token_hints {
                error_message.push_str(&format!(
                    "hint: may be illegal since {}\n",
                    hint
                ));
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
        while modified_span.start > 0 && self.text_rope.iterator(modified_span.start).next().is_none_or(|c| c == '\n') {
            modified_span.start = modified_span.start.saturating_sub(1);
        }

        self.artifacts.diagnostics.push(
            Diagnostic { is_error: true, span: modified_span, title, message: error_message }
        )
    }
}

impl SectionParser {
    fn read_token(&mut self, token: Token) -> Result<Span8> {
        try_all!(self, {
            ExactPred(token) => {
                self.advance_token()
            },
        })
    }

    fn read_if_token(&mut self, token: Token) -> Option<Span8> {
        try_all!(self, {
            ExactPred(token) => {
                self.advance_token()
            },
            else {
                Err(())
            }
        }).ok()
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
    pub fn parse_section(&mut self) -> std::result::Result<Section, Section> {
        let body = self.parse_statement_list();
        let section = Section {
            body,
            section_type: self.state.section_type,
            imported_section_indices: vec![]
        };
        if self.artifacts.diagnostics.is_empty() {
            Ok(section)
        }
        else {
            Err(section)
        }
    }

    // best effort parse
    pub fn parse_statement_list(&mut self) -> Vec<SpanTagged<Statement>> {
        let mut statements = Vec::new();
        loop {
            // skip newlines and semicolons
            while matches!(self.peek_token(), Some((Token::Newline, _)) | Some((Token::Semicolon, _))) {
                self.advance_token();
            }

            if self.peek_token().is_none() {
                break;
            }

            let mut read = || -> Result<SpanTagged<Statement>> {
                let ret = self.parse_statement()?;

                // ensure no hanging content
                if self.peek_token().is_some() {
                    try_all!(self, {
                        ExactPred(Token::Newline) => {
                            self.advance_token();
                        },
                        ExactPred(Token::Semicolon) => {
                            self.advance_token();
                        },
                        _op = BinaryOperatorPred => {
                            // solely to emit error that you can add binary operator instead
                            unreachable!()
                        },
                    })?;
                }

                Ok(ret)
            };

            match read() {
                Ok(statement) => statements.push(statement),
                Err(_e) => {
                    // gracefully handle errors
                    while self.peek_token().is_some() && !matches!(self.peek_token().unwrap(), (Token::Newline, _) | (Token::Semicolon, _)) {
                        self.advance_token();
                    }
                }
            };
        }

        statements
    }

    pub fn parse_statement(&mut self) -> Result<SpanTagged<Statement>> {
        try_all!(self, {
            vtype = VariableDeclarationPred => {
                let (span, statement) = self.parse_declaration(vtype)?;
                (span, Statement::Declaration(statement))
            },
            ExactPred(Token::For) => {
                let (span, statement) = self.parse_for()?;
                (span, Statement::For(statement))
            },
            ExactPred(Token::While) => {
                let (span, statement) = self.parse_while()?;
                (span, Statement::While(statement))
            },
            ExactPred(Token::If) => {
                let (span, statement) = self.parse_if()?;
                (span, Statement::If(statement))
            },
            if PlayablePredicate, ExactPred(Token::Play) => {
                let (span, statement) = self.parse_play()?;
                (span, Statement::Play(statement))
            },
            if InLoopPredicate, ExactPred(Token::Break) => {
                let span = self.advance_token();
                (span, Statement::Break)
            },
            if InLoopPredicate, ExactPred(Token::Continue) => {
                let span = self.advance_token();
                (span, Statement::Continue)
            },
            if InLambdaOrBlockPredicate, ExactPred(Token::Return) => {
                let (span, statement) = self.parse_return()?;
                (span, Statement::Return(statement))
            },
            if InLambdaOrBlockPredicate, ExactPred(Token::Dot) => {
                let base_span = self.advance_token();
                let (span, expr) = self.parse_expr()?;
                let desugared_expression = Statement::Expression(
                    Expression::BinaryOperator(
                        BinaryOperator {
                            lhs: (base_span.clone(), Box::new(Expression::IdentifierReference(
                                IdentifierReference::Value("_".into())
                            ))),
                            op_type: BinaryOperatorType::DotAssign,
                            rhs: (span.clone(), Box::new(expr))
                        }
                    )
                );
                (base_span.start..span.end, desugared_expression)
            },
            else {
                // otherwise must be expression, do not advance
                let (span, statement) = self.parse_expr()?;
                Ok((span, Statement::Expression(statement)))
            }
        })
    }

    fn parse_while(&mut self) -> Result<SpanTagged<While>> {
        self.debug_assert_token_eq(Token::While);
        let base_span = self.advance_token();
        self.read_token(Token::LParen).ok();
        let condition = self.parse_expr()
            .unwrap_or_default();

        self.read_token(Token::RParen).ok();

        let (terminal, body) = self.parse_body(|frame| {
            frame.in_loop = true;
        })?;

        Ok((base_span.start .. terminal.end, While {
            condition,
            body: (terminal, body),
        }))
    }

    fn parse_for(&mut self) -> Result<SpanTagged<For>> {
        self.debug_assert_token_eq(Token::For);
        let base_span = self.advance_token();

        self.read_token(Token::LParen).ok();
        let identifier = self.parse_identifier_declaration()?;
        self.read_token(Token::In).ok();

        let container = self.parse_expr()?;
        self.read_token(Token::RParen).ok();

        let (terminal, body) = self.parse_body(|frame| {
            frame.in_loop = true;
        })?;

        Ok((base_span.start .. terminal.end, For {
            var_name: identifier,
            body: (terminal, body),
            container,
        }))
    }

    fn parse_if(&mut self) -> Result<SpanTagged<If>> {
        self.debug_assert_token_eq(Token::If);
        let base_span = self.advance_token();
        self.read_token(Token::LParen).ok();
        let condition = self.parse_expr().unwrap_or_default();
        self.read_token(Token::RParen).ok();

        let body = self.parse_body(|_| {})?;
        self.advance_newlines();
        if self.read_if_token(Token::Else).is_some() {
            // parse else block
            let else_block = try_all!(self, {
                ExactPred(Token::If) => {
                    // else if
                    let (span, statement) = self.parse_if()?;
                    (span.clone(), vec![(span, Statement::If(statement))])
                },
                else {
                    self.parse_body(|_| {})
                }
            })?;

            Ok((base_span.start .. else_block.0.end, If {
                condition,
                if_block: body,
                else_block: Some(else_block)
            }))
        }
        else {
            Ok((base_span.start .. body.0.end, If {
                condition,
                if_block: body,
                else_block: None
            }))
        }
    }

    fn parse_play(&mut self) -> Result<SpanTagged<Play>> {
        self.debug_assert_token_eq(Token::Play);
        let base_span = self.advance_token();
        let animations = self.parse_expr()?;
        Ok((base_span.start .. animations.0.end, Play {
            animations
        }))
    }

    fn parse_declaration(&mut self, var_type: VariableType) -> Result<SpanTagged<Declaration>> {
        let base_span = self.advance_token();
        let identifier = self.parse_identifier_declaration()?;
        self.read_token(Token::Assign)?;
        let value = self.parse_expr()?;

        Ok((base_span.start..value.0.end, Declaration {
            var_type,
            identifier,
            value
        }))
    }

    fn parse_return(&mut self) -> Result<SpanTagged<Return>> {
        self.debug_assert_token_eq(Token::Return);
        let base_span = self.advance_token();
        let value = self.parse_expr()?;
        Ok((base_span.start .. value.0.end, Return {
            value,
        }))
    }

    fn parse_identifier_declaration(&mut self) -> Result<SpanTagged<IdentifierDeclaration>> {
        let span = self.read_token(Token::Identifier)?;
        let identifier: String = self.text_rope.iterator_range(span.clone()).collect();
        if !identifier.chars().all(|c| c.is_alphanumeric() || c == '_') || identifier.chars().next().is_none_or(|c| c.is_numeric()) {
            self.emit_error("Illegal Identifier".into(), format!("\"{}\" is not a valid identifier name", identifier), span.clone());
            return Err(());
        }

        Ok((span, IdentifierDeclaration(identifier)))
    }

    fn parse_body(&mut self, frame_builder: impl FnOnce(&mut ContextFrame)) -> Result<SpanTagged<Vec<SpanTagged<Statement>>>> {
        let body_range = self.precomputation.bracket_internal_range(self.token_index);
        let base_span = self.read_token(Token::LFlower)?;

        self.state.push_frame(|frame| {
            frame.operating_range = body_range;
            frame_builder(frame)
        });
        let result = self.parse_statement_list();
        self.state.pop_frame();

        let terminal = self.read_token(Token::RFlower)?;
        Ok((base_span.start .. terminal.end, result))
    }

    fn parse_expr(&mut self) -> Result<SpanTagged<Expression>> {
        self.parse_expr_priority(0)
    }
}

// expression parsing
impl SectionParser {
    fn parse_expr_priority(&mut self, priority: usize) -> Result<SpanTagged<Expression>> {
        let mut expr = Some(self.parse_unary()?);
        loop {
            let mut finished = false;
            try_all!(self, {
                op = BinaryOperatorPred => {
                    if (op.priority(), op.associativity()) >= (priority, 1) {
                        // combine right
                        self.advance_token();
                        let right = self.parse_expr_priority(op.priority())?;
                        let old_expr = expr.take().unwrap();
                        expr = Some((old_expr.0.start .. right.0.end, Expression::BinaryOperator(BinaryOperator {
                            lhs: (old_expr.0.clone(), Box::new(old_expr.1)),
                            op_type: op,
                            rhs: (right.0, Box::new(right.1))
                        })))
                    }
                    else {
                        finished = true;
                    }
                },
                else {
                    finished = true;
                    Ok(())
                }
            })?;

            if finished {
                break
            }
        }

        Ok(expr.unwrap())
    }

    fn read_pure_identifier(&mut self) -> Result<SpanTagged<String>> {
        let span = self.read_token(Token::Identifier)?;
        let str: String = self.text_rope.iterator_range(span.clone()).collect();
        return Ok((span, str));
    }

    fn parse_unary(&mut self) -> Result<SpanTagged<Expression>> {
        // parse base expression
        let expr = try_all!(self, expected = Some("<unary expression>"), {
            /* unary operators */
            op = UnaryOperatorPred => {
                self.parse_unary_preoperator(op)?
            },
            /* entire sub expression */
            ExactPred(Token::LParen) => {
                self.parse_parenthesis_sub_expression()?
            },
            /* identifier */
            ExactPred(Token::Identifier) => {
                let span = self.advance_token();
                let str: String = self.text_rope.iterator_range(span.clone()).collect();
                (span, Expression::IdentifierReference(IdentifierReference::Value(str)))
            },
            ExactPred(Token::Reference) => {
                let base_span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier()?;
                (base_span.start..full_span.end, Expression::IdentifierReference(IdentifierReference::Reference(str)))
            },
            ExactPred(Token::StatefulReference) => {
                let base_span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier()?;
                (base_span.start..full_span.end, Expression::IdentifierReference(IdentifierReference::Reference(str)))
            },
            ExactPred(Token::Multiply) => {
                let base_span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier()?;
                (base_span.start..full_span.end, Expression::IdentifierReference(IdentifierReference::Reference(str)))
            },
            /* operator definition */
            ExactPred(Token::Operator) => {
                let base_span = self.advance_token();
                let target = self.parse_unary()?;
                (base_span.start..target.0.end, Expression::OperationDefinition(OperatorDefinition { lambda: (target.0, Box::new(target.1)) }))
            },
            /* lambda definition */
            ExactPred(Token::Pipe) => {
                self.parse_lambda()?
            },
            /* blocks */
            ExactPred(Token::Block) => {
                self.parse_block()?
            },
            ExactPred(Token::Anim) => {
                self.parse_anim()?
            },
            /* literals */
            ExactPred(Token::IntegerLiteral) => {
                self.parse_int_literal()?
            },
            ExactPred(Token::FloatLiteral) => {
                self.parse_float_literal()?
            },
            ExactPred(Token::StringLiteral) => {
                self.parse_string_literal()?
            },
            ExactPred(Token::LBracket) => {
                self.parse_map_or_vector_literal()?
            },
            /* monocurl internal */
            if InStdLibPredicate, ExactPred(Token::Native) => {
                let span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier()?;
                let args = self.parse_invocation(Token::LParen, Token::RParen, false)?;
                let arguments = (args.0, args.1
                    .into_iter()
                    .map(|(_label, expr)| expr)
                    .collect());
                (span.start..arguments.0.end, Expression::NativeInvocation(NativeInvocation {
                    function: (full_span, IdentifierReference::Value(str)),
                    arguments: arguments.1,
                }))
            },
        })?;

        Ok(self.apply_postfixes(expr)?)
    }

    fn apply_postfixes(&mut self, base: SpanTagged<Expression>) -> Result<SpanTagged<Expression>> {
        let mut expr = Some(base);

        loop {
            let mut is_finished = false;

            let mut take_expr = || expr.take().unwrap();
            let boxify = |raw: SpanTagged<Expression>| (raw.0, Box::new(raw.1));
            let next = try_all!(self, expected = Some("<postfix operator>"), {
                ExactPredDesc(Token::LParen, "<function call>") => {
                    // lambda invocation
                    let arguments = self.parse_invocation(Token::LParen, Token::RParen, true)?;
                    let old = boxify(take_expr());
                    (old.0.start..arguments.0.end, Expression::LambdaInvocation(LambdaInvocation {
                        lambda: old,
                        arguments: arguments
                    }))
                },
                ExactPredDesc(Token::LFlower, "<operator invocation>") => {
                    // operator invocation
                    let arguments = self.parse_invocation(Token::LFlower, Token::RFlower, false)?;
                    let old = boxify(take_expr());
                    let operand = self.parse_unary()?;

                    (old.0.start..operand.0.end, Expression::OperatorInvocation(OperatorInvocation {
                        operator: old,
                        arguments,
                        operand: boxify(operand)
                    }))
                },
                ExactPredDesc(Token::LBracket, "<subscript>") => {
                    // subscript
                    self.advance_token();
                    let index = self.parse_expr()?;
                    let end_span = self.read_token(Token::RBracket)?;
                    let old = boxify(take_expr());
                    (old.0.start..end_span.end, Expression::Subscript(Subscript {
                        base: old,
                        index: boxify(index)
                    }))
                },
                ExactPredDesc(Token::Dot, "<property access>") => {
                    self.advance_token();
                    let attribute = self.read_pure_identifier()?;
                    let old = boxify(take_expr());
                    (old.0.start..attribute.0.end, Expression::Property(Property {
                        base: old,
                        attribute: (attribute.0, IdentifierReference::Value(attribute.1))
                    }))
                },
                else {
                    is_finished = true;
                    Ok(take_expr())
                }
            })?;

            if is_finished {
                return Ok(next);
            }
            else {
                expr = Some(next);
            }
        }
    }

    // (<token_index> a,b,c,d,e)
    fn parse_invocation(&mut self, start: Token, end: Token, allow_newlines: bool)
    -> Result<SpanTagged<Vec<(Option<SpanTagged<IdentifierDeclaration>>, SpanTagged<Expression>)>>>
    {
        self.debug_assert_token_eq(start);
        let range = self.precomputation.bracket_internal_range(self.token_index);
        let base_span = self.advance_token();

        self.state.push_frame(|frame| frame.operating_range = range);

        let mut arguments = Vec::new();
        let mut err = None;
        loop {
            if allow_newlines {
                self.advance_newlines();
            }
            if self.peek_token().is_none() {
                break;
            }

            let mut read = || {
                if arguments.len() > 0 {
                    self.read_token(Token::Comma)?;
                }

                if allow_newlines {
                    self.advance_newlines();
                }
                let label = self.peek_token()
                    .and_then(|tok| {
                        if !matches!(tok.0, Token::ArgumentLabel) {
                            return None;
                        }
                        let text: String = self.text_rope.iterator_range(tok.1.clone()).collect();
                        if text.ends_with(':') {
                            Some((tok.1.start..tok.1.end, IdentifierDeclaration(text[..text.len() - 1].to_string())))
                        }
                        else {
                            None
                        }
                    });

                if let Some(label) = label {
                    // consume label
                    self.advance_token();
                    if allow_newlines {
                        self.advance_newlines();
                    }

                    let argument = self.parse_expr()?;

                    Ok((Some(label), argument))
                }
                else {
                    let argument = self.parse_expr()?;
                    Ok((None, argument))
                }
            };

            match read() {
                Ok(argument) => arguments.push(argument),
                Err(e) => {
                    err = Some(e);
                    break;
                }
            }
        }
        self.state.pop_frame();

        let end_span = self.read_token(end)?;

        err.map_or(Ok((base_span.start .. end_span.end, arguments)), Err)
    }

    fn parse_unary_preoperator(&mut self, op: UnaryOperatorType) -> Result<SpanTagged<Expression>> {
        let base_span = self.advance_token();
        let (next_span, next) = self.parse_expr_priority(op.priority())?;
        Ok((base_span.start..next_span.end, Expression::UnaryPreOperator(UnaryPreOperator {
            op_type: op,
            operand: (next_span, Box::new(next))
        })))
    }

    fn parse_parenthesis_sub_expression(&mut self) -> Result<SpanTagged<Expression>> {
        self.debug_assert_token_eq(Token::LParen);
        let inner_range = self.precomputation.bracket_internal_range(self.token_index);
        let base_span = self.advance_token();

        self.state.push_frame(|frame| {
            frame.operating_range = inner_range;
        });
        let result = self.parse_expr();
        self.state.pop_frame();
        let result = result?;

        let terminal_span = self.read_token(Token::RParen)?;

        Ok((base_span.start .. terminal_span.end, result.1))
    }

    fn parse_anim(&mut self) -> Result<SpanTagged<Expression>> {
        self.debug_assert_token_eq(Token::Anim);
        let base_span = self.advance_token();

        let body = self.parse_body(|frame| {
            frame.in_playable_block = true;
            frame.in_lambda_or_block = false;
        })?;

        Ok((base_span.start .. body.0.end, Expression::Anim(Anim {
            body: body.1
        })))
    }

    fn parse_lambda(&mut self) -> Result<SpanTagged<Expression>> {
        self.debug_assert_token_eq(Token::Pipe);
        let base_span = self.advance_token();
        let mut args = Vec::new();
        // parse arguments (and default values)
        if self.read_if_token(Token::Pipe).is_none() {
            let mut is_first = true;
            loop {
                if !is_first {
                    let mut done = false;
                    try_all!(self, {
                        ExactPred(Token::Pipe) => {
                            self.advance_token();
                            done = true;
                        },
                        ExactPred(Token::Comma) => {
                            self.advance_token();
                        },
                    })?;
                    if done {
                        break;
                    }
                }
                is_first = false;

                let name = self.parse_identifier_declaration()?;
                if self.read_if_token(Token::Assign).is_some() {
                    let value = self.parse_expr()?;
                    args.push((name, Some(value)));
                }
                else {
                    args.push((name, None));
                }
            }
        }

        let body = try_all!(self, {
            ExactPred(Token::LFlower) => {
                let body = self.parse_body(|frame| {
                    frame.in_loop = false;
                    frame.in_playable_block = false;
                    frame.in_lambda_or_block = true;
                })?;
                (body.0, LambdaBody::Block(body.1))
            },
            else {
                let expr = self.parse_expr()?;
                Ok((expr.0, LambdaBody::Inline(Box::new(expr.1))))
            }
        })?;

        Ok((base_span.start..body.0.end, Expression::LambdaDefinition(LambdaDefinition {
            args,
            body,
        })))
    }

    fn parse_block(&mut self) -> Result<SpanTagged<Expression>> {
        self.debug_assert_token_eq(Token::Block);
        let base_span = self.advance_token();

        let body = self.parse_body(|frame| {
            frame.in_loop = false;
            frame.in_playable_block = false;
            frame.in_lambda_or_block = true;
        })?;

        Ok((base_span.start .. body.0.end, Expression::Block(Block {
            body: body.1
        })))
    }
}

impl SectionParser {
    fn parse_basic_literal(&mut self, tok: Token, f: impl FnOnce(&str) -> std::result::Result<Literal, &'static str>) -> Result<SpanTagged<Expression>> {
        self.debug_assert_token_eq(tok);
        let span = self.advance_token();
        let content: String = self.text_rope.iterator_range(span.clone()).collect();
        let literal = match f(&content) {
            Ok(literal) => literal,
            Err(message) => {
                self.emit_error("Illegal Literal".into(), message.into(), span);
                return Err(())
            }
        };
        Ok((span, Expression::Literal(literal)))
    }

    fn escape_char_literal(c: char) -> Option<char> {
        match c {
            'n' => Some('\n'),
            't' => Some('\t'),
            'r' => Some('\r'),
            '%' => Some('%'),
            '"' => Some('"'),
            '\'' => Some('\''),
            '\\' => Some('\\'),
            _ => None
        }
    }

    fn parse_string_literal(&mut self) -> Result<SpanTagged<Expression>> {
        self.parse_basic_literal(Token::StringLiteral, |string| {
            let mut build = String::new();
            let mut it = string.chars();
            if it.next() != Some('"') {
                return Err("Malformed string literal")
            }

            while let Some(curr) = it.next() {
                if curr == '%' {
                    let next = it.next();
                    if let Some(map) = next.and_then(Self::escape_char_literal) {
                        build.push(map);
                    }
                    else {
                        return Err("Illegal escape character")
                    }
                }
                else if curr == '"' {
                    // not the end
                    if it.next().is_some() {
                        return Err("Malformed string literal")
                    }
                    else {
                        return Ok(Literal::String(build));
                    }
                }
                else {
                    build.push(curr);
                }
            }

            return Err("Malformed string literal");
        })
    }

    fn parse_int_literal(&mut self) -> Result<SpanTagged<Expression>> {
        self.parse_basic_literal(Token::IntegerLiteral, |string| {
            Self::parse_numeric_with_suffix(string, |s| {
                s.parse::<i64>().map(|v| v as f64).map_err(|_| "Invalid integer literal")
            },|s| {
                Ok(Literal::Int(s.parse::<i64>().map_err(|_| "Invalid integer literal")?))
            })
        })
    }

    fn parse_float_literal(&mut self) -> Result<SpanTagged<Expression>> {
        self.parse_basic_literal(Token::FloatLiteral, |string| {
            Self::parse_numeric_with_suffix(string, |s| {
                s.parse::<f64>().map_err(|_| "Invalid float literal")
            },|s| {
                Ok(Literal::Float(s.parse::<f64>().map_err(|_| "Invalid float literal")?))
            })
        })
    }

    fn parse_numeric_with_suffix(
        string: &str,
        parse_value: impl Fn(&str) -> std::result::Result<f64, &'static str>,
        standard_parse_value: impl Fn(&str) -> std::result::Result<Literal, &'static str>
    ) -> std::result::Result<Literal, &'static str> {
        // directional suffixes
        if let Some(stripped) = string.strip_suffix('l') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Left(value)));
        }
        if let Some(stripped) = string.strip_suffix('r') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Right(value)));
        }
        if let Some(stripped) = string.strip_suffix('u') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Up(value)));
        }
        if let Some(stripped) = string.strip_suffix('d') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Down(value)));
        }
        if let Some(stripped) = string.strip_suffix('f') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Forward(value)));
        }
        if let Some(stripped) = string.strip_suffix('b') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Directional(DirectionalLiteral::Backward(value)));
        }
        if let Some(stripped) = string.strip_suffix('i') {
            let value = parse_value(stripped)?;
            return Ok(Literal::Imaginary(value));
        }
        if let Some(stripped) = string.strip_suffix("dg") {
            let value = parse_value(stripped)?;
            let radians = value * std::f64::consts::PI / 180.0;
            return Ok(Literal::Float(radians));
        }

        standard_parse_value(string)
    }

    fn parse_map_or_vector_literal(&mut self) -> Result<SpanTagged<Expression>> {
        self.debug_assert_token_eq(Token::LBracket);
        let base_span = self.advance_token();
        // empty
        self.advance_newlines();
        if self.read_if_token(Token::KeyValueMap).is_some() {
            self.advance_newlines();
            let end_span = self.read_token(Token::RBracket)?;
            return Ok((base_span.start..end_span.end, Expression::Literal(Literal::Map(vec![]))))
        }
        else if let Some(end_span) = self.read_if_token(Token::RBracket) {
            return Ok((base_span.start..end_span.end, Expression::Literal(Literal::Vector(vec![]))))
        }

        let mut vector_entries = Vec::new();
        let mut map_entries = Vec::new();
        let mut last_span = base_span.clone();
        loop {
            let entry = self.parse_expr()?;
            if let Some(span) = self.read_if_token(Token::KeyValueMap) {
                if !vector_entries.is_empty() {
                    self.emit_error("Ambiguous Literal".into(), "cannot decide if literal is list or map".into(), base_span.start..span.end);
                    return Err(());
                }
                let value = self.parse_expr()?;
                map_entries.push((entry, value));
            }
            else {
                if !map_entries.is_empty() {
                    self.emit_error("Ambiguous Literal".into(), "cannot decide if literal is list or map".into(), base_span.start..entry.0.end);
                    return Err(());
                }
                vector_entries.push(entry)
            }

            self.advance_newlines();
            let mut is_finished = false;
            try_all!(self, {
                ExactPred(Token::Comma) => {
                    self.advance_token();
                },
                ExactPred(Token::RBracket) => {
                    last_span = self.advance_token();
                    is_finished = true;
                },
            })?;
            if is_finished {
                break
            }
        }

        if !vector_entries.is_empty() {
            Ok((base_span.start..last_span.end, Expression::Literal(Literal::Vector(vector_entries))))
        }
        else {
            Ok((base_span.start..last_span.end, Expression::Literal(Literal::Map(map_entries))))
        }
    }
}

impl SectionParser {
    pub fn new(tokens: Vec<(Token, Span8)>, text: Rope<TextAggregate>, section_type: SectionType, cursor_position: Option<Count8>) -> Self {
        SectionParser {
            precomputation: Precomputation::new(&tokens),
            state: State::new(section_type, tokens.len()),
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

// impl Parser {
//     fn gather_sections(&mut self) {

//     }

//     fn new(external_context: &ExternalContext, lex_rope: Rope<Attribute<Token>>, text_rope: TextAggregate) -> Self {
//         // precompute on the section

//         let mut utf8 = 0;
//         let token_list: Vec<_> = lex_rope.iterator(0)
//             .map(|(len, token)| {
//                 let span = utf8 .. utf8 + len;
//                 utf8 += len;
//                 (token, span)
//             })
//             .filter(|(tok, _)| tok != &Token::Whitespace && tok != &Token::Comment)
//             .collect();

//         SectionParser {
//             precomputation: todo!(),
//             state: todo!(),
//             text_rope: todo!(),
//             tokens: todo!(),
//             token_index: todo!(),
//             cursor_position: todo!(),
//             artifacts: todo!(),
//         }
//     }

//     pub fn parse(external_context: &'a ExternalContext, lex_rope: Rope<Attribute<Token>>, text_rope: Rope<TextAggregate>) -> Result<Vec<Section>, Vec<Diagnostic>> {
//         // 1. crawl imports to gather sections
//         Err(vec![])
//     }
// }


#[cfg(test)]
mod test {
    use lexer::{lexer::Lexer, token::Token};
    use structs::{rope::Rope, text::Span8};

    use super::Result;
    use crate::{ast::*, parser::SectionParser};

    fn lex(content: &str) -> Vec<(Token, Span8)> {
        Lexer::token_stream(content.chars())
            .iter().filter(|(tok, _)| tok != &Token::Whitespace && tok != &Token::Comment)
            .cloned()
            .collect()
    }

    fn parse_expr_test(content: &str) -> Result<SpanTagged<Expression>> {
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None);
        let ret = parser.parse_expr();
        if ret.is_err() {
            dbg!(&parser.artifacts.diagnostics);
        }
        ret
    }

    fn parse_stmt_test(content: &str) -> Result<SpanTagged<Statement>> {
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None);
        let ret = parser.parse_statement();
        if ret.is_err() {
            dbg!(&parser.artifacts.diagnostics);
        }
        ret
    }

    // Literal tests
    #[test]
    fn test_integer_literal() {
        let result = parse_expr_test("42").unwrap();
        let expected = Expression::Literal(Literal::Int(42));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_float_literal() {
        let result = parse_expr_test("3.14").unwrap();
        if let Expression::Literal(Literal::Float(val)) = result.1 {
            assert!((val - 3.14).abs() < 0.0001);
        } else {
            panic!("Expected float literal");
        }
    }

    #[test]
    fn test_string_literal() {
        let result = parse_expr_test(r#""hello world""#).unwrap();
        let expected = Expression::Literal(Literal::String("hello world".to_string()));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_string_with_escapes() {
        let result = parse_expr_test(r#""hello%nworld%t%"test%'""#).unwrap();
        let expected = Expression::Literal(Literal::String("hello\nworld\t\"test'".to_string()));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_directional_literal_left() {
        let result = parse_expr_test("5l").unwrap();
        let expected = Expression::Literal(Literal::Directional(DirectionalLiteral::Left(5.0)));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_directional_literal_up() {
        let result = parse_expr_test("3.5u").unwrap();
        let expected = Expression::Literal(Literal::Directional(DirectionalLiteral::Up(3.5)));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_imaginary_literal() {
        let result = parse_expr_test("2i").unwrap();
        let expected = Expression::Literal(Literal::Imaginary(2.0));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_degrees_literal() {
        let result = parse_expr_test("90dg").unwrap();
        if let Expression::Literal(Literal::Float(val)) = result.1 {
            assert!((val - std::f64::consts::PI / 2.0).abs() < 0.0001);
        } else {
            panic!("Expected float literal");
        }
    }

    #[test]
    fn test_empty_vector() {
        let result = parse_expr_test("[]").unwrap();
        let expected = Expression::Literal(Literal::Vector(vec![]));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_vector_literal() {
        let result = parse_expr_test("[1, 2, 3]").unwrap();
        let expected = Expression::Literal(Literal::Vector(vec![
            (1..2, Expression::Literal(Literal::Int(1))),
            (4..5, Expression::Literal(Literal::Int(2))),
            (7..8, Expression::Literal(Literal::Int(3))),
        ]));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_empty_map() {
        let result = parse_expr_test("[->]").unwrap();
        let expected = Expression::Literal(Literal::Map(vec![]));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_map_literal() {
        let result = parse_expr_test("[1 -> 2, 3 -> 4]").unwrap();
        let expected = Expression::Literal(Literal::Map(vec![
            (
                (1..2, Expression::Literal(Literal::Int(1))),
                (6..7, Expression::Literal(Literal::Int(2)))
            ),
            (
                (9..10, Expression::Literal(Literal::Int(3))),
                (14..15, Expression::Literal(Literal::Int(4)))
            ),
        ]));
        assert_eq!(result.1, expected);
    }

    // Identifier tests
    #[test]
    fn test_identifier() {
        let result = parse_expr_test("foo").unwrap();
        let expected = Expression::IdentifierReference(IdentifierReference::Value("foo".to_string()));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_reference() {
        let result = parse_expr_test("&bar").unwrap();
        let expected = Expression::IdentifierReference(IdentifierReference::Reference("bar".to_string()));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_stateful_reference() {
        let result = parse_expr_test("$state_var").unwrap();
        let expected = Expression::IdentifierReference(IdentifierReference::Reference("state_var".to_string()));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_dereference() {
        let result = parse_expr_test("*ptr").unwrap();
        let expected = Expression::IdentifierReference(IdentifierReference::Reference("ptr".to_string()));
        assert_eq!(result.1, expected);
    }

    // Binary operator tests
    #[test]
    fn test_addition() {
        let result = parse_expr_test("1 + 2").unwrap();
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (0..1, Box::new(Expression::Literal(Literal::Int(1)))),
            op_type: BinaryOperatorType::Add,
            rhs: (4..5, Box::new(Expression::Literal(Literal::Int(2)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_append_operator() {
        let result = parse_expr_test("[1, 2] .. 3").unwrap();
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (0..6, Box::new(Expression::Literal(Literal::Vector(vec![
                (1..2, Expression::Literal(Literal::Int(1))),
                (4..5, Expression::Literal(Literal::Int(2))),
            ])))),
            op_type: BinaryOperatorType::Append,
            rhs: (10..11, Box::new(Expression::Literal(Literal::Int(3)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_comparison() {
        let result = parse_expr_test("x < y").unwrap();
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (0..1, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
            op_type: BinaryOperatorType::Lt,
            rhs: (4..5, Box::new(Expression::IdentifierReference(IdentifierReference::Value("y".to_string())))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_operator_precedence() {
        let result = parse_expr_test("1 + 2 * 3").unwrap();
        let expected = Expression::BinaryOperator(
            BinaryOperator {
                lhs: (0..1, Box::new(Expression::Literal(Literal::Int(1)))),
                op_type: BinaryOperatorType::Add,
                rhs: (4..9, Box::new(Expression::BinaryOperator(
                    BinaryOperator {
                        lhs: (4..5, Box::new(Expression::Literal(Literal::Int(2)))),
                        op_type: BinaryOperatorType::Multiply,
                        rhs: (8..9, Box::new(Expression::Literal(Literal::Int(3)))),
                    }
                )))
            }
        );
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_left_associativity_append() {
        let result = parse_expr_test("a .. b .. c").unwrap();
        // BinaryOperator(BinaryOperator { lhs: (0..6, BinaryOperator(BinaryOperator { lhs: (0..1, IdentifierReference(Value("a"))), op_type: Append, rhs: (5..6, IdentifierReference(Value("b"))) })), op_type: Append, rhs: (10..11, IdentifierReference(Value("c"))) })
        let expected = Expression::BinaryOperator(
            BinaryOperator {
                lhs: (0..6, Box::new(Expression::BinaryOperator(
                    BinaryOperator {
                        lhs: (0..1, Box::new(Expression::IdentifierReference(IdentifierReference::Value("a".to_string())))),
                        op_type: BinaryOperatorType::Append,
                        rhs: (5..6, Box::new(Expression::IdentifierReference(IdentifierReference::Value("b".to_string())))),
                    }
                ))),
                op_type: BinaryOperatorType::Append,
                rhs: (10..11, Box::new(Expression::IdentifierReference(IdentifierReference::Value("c".to_string())))),
            }
        );
        assert_eq!(result.1, expected);
    }

    // Unary operator tests
    #[test]
    fn test_negation() {
        let result = parse_expr_test("-5").unwrap();
        let expected = Expression::UnaryPreOperator(UnaryPreOperator {
            op_type: UnaryOperatorType::Negative,
            operand: (1..2, Box::new(Expression::Literal(Literal::Int(5)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_not() {
        let result = parse_expr_test("not x").unwrap();
        let expected = Expression::UnaryPreOperator(UnaryPreOperator {
            op_type: UnaryOperatorType::Not,
            operand: (4..5, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_float_negation() {
        let result = parse_expr_test("--5").unwrap();
        let expected = Expression::UnaryPreOperator(UnaryPreOperator {
            op_type: UnaryOperatorType::Negative,
            operand: (1..3, Box::new(Expression::UnaryPreOperator(UnaryPreOperator {
                op_type: UnaryOperatorType::Negative,
                operand: (2..3, Box::new(Expression::Literal(Literal::Int(5)))),
            }))),
        });
        assert_eq!(result.1, expected);
    }

    // Lambda tests
    #[test]
    fn test_simple_lambda() {
        let result = parse_expr_test("|x| x + 1").unwrap();
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![
                ((1..2, IdentifierDeclaration("x".to_string())), None)
            ],
            body: (4..9, LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                lhs: (4..5, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                op_type: BinaryOperatorType::Add,
                rhs: (8..9, Box::new(Expression::Literal(Literal::Int(1)))),
            })))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_lambda_with_multiple_args() {
        let result = parse_expr_test("|x, y| x + y").unwrap();
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![
                ((1..2, IdentifierDeclaration("x".to_string())), None),
                ((4..5, IdentifierDeclaration("y".to_string())), None),
            ],
            body: (7..12, LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                lhs: (7..8, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                op_type: BinaryOperatorType::Add,
                rhs: (11..12, Box::new(Expression::IdentifierReference(IdentifierReference::Value("y".to_string())))),
            })))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_lambda_with_default_arg() {
        let result = parse_expr_test("|x, y = 5| x + y").unwrap();
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![
                ((1..2, IdentifierDeclaration("x".to_string())), None),
                ((4..5, IdentifierDeclaration("y".to_string())), Some((8..9, Expression::Literal(Literal::Int(5))))),
            ],
            body: (11..16, LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                lhs: (11..12, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                op_type: BinaryOperatorType::Add,
                rhs: (15..16, Box::new(Expression::IdentifierReference(IdentifierReference::Value("y".to_string())))),
            })))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_lambda_block_body() {
        let result = parse_expr_test("|x| { return x + 1 }").unwrap();
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![((1..2, IdentifierDeclaration("x".to_string())), None)],
            body: (4..20, LambdaBody::Block(vec![
                (6..18, Statement::Return(Return {
                    value: (13..18, Expression::BinaryOperator(BinaryOperator {
                        lhs: (13..14, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                        op_type: BinaryOperatorType::Add,
                        rhs: (17..18, Box::new(Expression::Literal(Literal::Int(1)))),
                    })),
                }))
            ])),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_empty_lambda() {
        let result = parse_expr_test("|| 42").unwrap();
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![],
            body: (3..5, LambdaBody::Inline(Box::new(Expression::Literal(Literal::Int(42))))),
        });
        assert_eq!(result.1, expected);
    }

    // Function invocation tests
    #[test]
    fn test_function_call_no_args() {
        let result = parse_expr_test("foo()").unwrap();
        let expected = Expression::LambdaInvocation(LambdaInvocation {
            lambda: (0..3, Box::new(Expression::IdentifierReference(IdentifierReference::Value("foo".to_string())))),
            arguments: (3..5, vec![]),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_function_call_with_args() {
        let result = parse_expr_test("add(1, 2)").unwrap();
        let expected = Expression::LambdaInvocation(LambdaInvocation {
            lambda: (0..3, Box::new(Expression::IdentifierReference(IdentifierReference::Value("add".to_string())))),
            arguments: (3..9, vec![
                (None, (4..5, Expression::Literal(Literal::Int(1)))),
                (None, (7..8, Expression::Literal(Literal::Int(2)))),
            ]),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_function_call_with_labeled_args() {
        let result = parse_expr_test("foo(x: 1, y: 2)").unwrap();
        let expected = Expression::LambdaInvocation(LambdaInvocation {
            lambda: (0..3, Box::new(Expression::IdentifierReference(IdentifierReference::Value("foo".to_string())))),
            arguments: (3..15, vec![
                (Some((4..6, IdentifierDeclaration("x".to_string()))), (7..8, Expression::Literal(Literal::Int(1)))),
                (Some((10..12, IdentifierDeclaration("y".to_string()))), (13..14, Expression::Literal(Literal::Int(2)))),
            ]),
        });
        assert_eq!(result.1, expected);
    }

    // Operator invocation tests
    #[test]
    fn test_operator_invocation() {
        let result = parse_expr_test("op{x, y} z").unwrap();
        let expected = Expression::OperatorInvocation(OperatorInvocation {
            operator: (0..2, Box::new(Expression::IdentifierReference(IdentifierReference::Value("op".to_string())))),
            arguments: (2..8, vec![
                (None, (3..4, Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                (None, (6..7, Expression::IdentifierReference(IdentifierReference::Value("y".to_string())))),
            ]),
            operand: (9..10, Box::new(Expression::IdentifierReference(IdentifierReference::Value("z".to_string())))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_operator_invocation_complex() {
        let result = parse_expr_test("derivative{n: 2} (x * x)").unwrap();
        let expected = Expression::OperatorInvocation(OperatorInvocation {
            operator: (0..10, Box::new(Expression::IdentifierReference(IdentifierReference::Value("derivative".to_string())))),
            arguments: (10..16, vec![
                (Some((11..13, IdentifierDeclaration("n".to_string()))), (14..15, Expression::Literal(Literal::Int(2)))),
            ]),
            operand: (17..24, Box::new(Expression::BinaryOperator(BinaryOperator {
                lhs: (18..19, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                op_type: BinaryOperatorType::Multiply,
                rhs: (22..23, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
            }))),
        });
        assert_eq!(result.1, expected);
    }

    // Property access tests
    #[test]
    fn test_property_access() {
        let result = parse_expr_test("obj.field").unwrap();
        let expected = Expression::Property(Property {
            base: (0..3, Box::new(Expression::IdentifierReference(IdentifierReference::Value("obj".to_string())))),
            attribute: (4..9, IdentifierReference::Value("field".to_string())),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_chained_property_access() {
        let result = parse_expr_test("obj.a.b.c").unwrap();
        let expected = Expression::Property(Property {
            base: (0..7, Box::new(Expression::Property(Property {
                base: (0..5, Box::new(Expression::Property(Property {
                    base: (0..3, Box::new(Expression::IdentifierReference(IdentifierReference::Value("obj".to_string())))),
                    attribute: (4..5, IdentifierReference::Value("a".to_string())),
                }))),
                attribute: (6..7, IdentifierReference::Value("b".to_string())),
            }))),
            attribute: (8..9, IdentifierReference::Value("c".to_string())),
        });
        assert_eq!(result.1, expected);
    }

    // Subscript tests
    #[test]
    fn test_subscript() {
        let result = parse_expr_test("arr[0]").unwrap();
        let expected = Expression::Subscript(Subscript {
            base: (0..3, Box::new(Expression::IdentifierReference(IdentifierReference::Value("arr".to_string())))),
            index: (4..5, Box::new(Expression::Literal(Literal::Int(0)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_chained_subscript() {
        let result = parse_expr_test("matrix[i][j]").unwrap();
        let expected = Expression::Subscript(Subscript {
            base: (0..9, Box::new(Expression::Subscript(Subscript {
                base: (0..6, Box::new(Expression::IdentifierReference(IdentifierReference::Value("matrix".to_string())))),
                index: (7..8, Box::new(Expression::IdentifierReference(IdentifierReference::Value("i".to_string())))),
            }))),
            index: (10..11, Box::new(Expression::IdentifierReference(IdentifierReference::Value("j".to_string())))),
        });
        assert_eq!(result.1, expected);
    }

    // Parentheses tests
    #[test]
    fn test_parentheses() {
        let result = parse_expr_test("(1 + 2) * 3").unwrap();
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (0..7, Box::new(Expression::BinaryOperator(BinaryOperator {
                lhs: (1..2, Box::new(Expression::Literal(Literal::Int(1)))),
                op_type: BinaryOperatorType::Add,
                rhs: (5..6, Box::new(Expression::Literal(Literal::Int(2)))),
            }))),
            op_type: BinaryOperatorType::Multiply,
            rhs: (10..11, Box::new(Expression::Literal(Literal::Int(3)))),
        });
        assert_eq!(result.1, expected);
    }

    // Statement tests
    #[test]
    fn test_let_declaration() {
        let result = parse_stmt_test("let x = 5").unwrap();
        let expected = Statement::Declaration(Declaration {
            var_type: VariableType::Let,
            identifier: (4..5, IdentifierDeclaration("x".to_string())),
            value: (8..9, Expression::Literal(Literal::Int(5))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_var_declaration() {
        let result = parse_stmt_test("var y = 10").unwrap();
        let expected = Statement::Declaration(Declaration {
            var_type: VariableType::Var,
            identifier: (4..5, IdentifierDeclaration("y".to_string())),
            value: (8..10, Expression::Literal(Literal::Int(10))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_while_loop() {
        let result = parse_stmt_test("while (x < 10) { x = x + 1 }").unwrap();
        let expected = Statement::While(While {
            condition: (7..13, Expression::BinaryOperator(BinaryOperator {
                lhs: (7..8, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                op_type: BinaryOperatorType::Lt,
                rhs: (11..13, Box::new(Expression::Literal(Literal::Int(10)))),
            })),
            body: (15..28, vec![
                (17..26, Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                    lhs: (17..18, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                    op_type: BinaryOperatorType::Assign,
                    rhs: (21..26, Box::new(Expression::BinaryOperator(BinaryOperator {
                        lhs: (21..22, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                        op_type: BinaryOperatorType::Add,
                        rhs: (25..26, Box::new(Expression::Literal(Literal::Int(1)))),
                    }))),
                })))
            ]),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_for_loop() {
        let result = parse_stmt_test("for (i in [1, 2, 3]) { print(i) }").unwrap();
        let expected = Statement::For(For {
            var_name: (5..6, IdentifierDeclaration("i".to_string())),
            container: (10..19, Expression::Literal(Literal::Vector(vec![
                (11..12, Expression::Literal(Literal::Int(1))),
                (14..15, Expression::Literal(Literal::Int(2))),
                (17..18, Expression::Literal(Literal::Int(3))),
            ]))),
            body: (21..33, vec![
                (23..31, Statement::Expression(Expression::LambdaInvocation(LambdaInvocation {
                    lambda: (23..28, Box::new(Expression::IdentifierReference(IdentifierReference::Value("print".to_string())))),
                    arguments: (28..31, vec![
                        (None, (29..30, Expression::IdentifierReference(IdentifierReference::Value("i".to_string()))))
                    ]),
                })))
            ]),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_if_statement() {
        let result = parse_stmt_test("if (x > 0) { y = 1 }").unwrap();
        let expected = Statement::If(If {
            condition: (4..9, Expression::BinaryOperator(BinaryOperator {
                lhs: (4..5, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                op_type: BinaryOperatorType::Gt,
                rhs: (8..9, Box::new(Expression::Literal(Literal::Int(0)))),
            })),
            if_block: (11..20, vec![
                (13..18, Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                    lhs: (13..14, Box::new(Expression::IdentifierReference(IdentifierReference::Value("y".to_string())))),
                    op_type: BinaryOperatorType::Assign,
                    rhs: (17..18, Box::new(Expression::Literal(Literal::Int(1)))),
                })))
            ]),
            else_block: None,
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_if_else_statement() {
        let result = parse_stmt_test("if (x > 0) { y = 1 } else { y = -1 }").unwrap();
        if let Statement::If(if_stmt) = &result.1 {
            assert!(if_stmt.else_block.is_some());
            if let Some((_, else_stmts)) = &if_stmt.else_block {
                assert_eq!(else_stmts.len(), 1);
            }
        } else {
            panic!("Expected if statement");
        }
    }

    #[test]
    fn test_if_else_if() {
        let result = parse_stmt_test("if (x > 0) { y = 1 } else if (x < 0) { y = -1 }").unwrap();
        if let Statement::If(if_stmt) = &result.1 {
            assert!(if_stmt.else_block.is_some());
            if let Some((_, else_stmts)) = &if_stmt.else_block {
                assert_eq!(else_stmts.len(), 1);
                assert!(matches!(else_stmts[0].1, Statement::If(_)));
            }
        } else {
            panic!("Expected if statement");
        }
    }

    #[test]
    fn test_return_statement() {
        let result = parse_expr_test("|| { return 42 }").unwrap();
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![],
            body: (3..16, LambdaBody::Block(vec![(5..14, Statement::Return(Return {
                value: (12..14, Expression::Literal(Literal::Int(42))),
            }))]))
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_break_statement() {
        let result = parse_stmt_test("while (1) { break }").unwrap();
        if let Statement::While(w) = result.1 {
            assert_eq!(w.
                body.1.len(), 1);
            assert!(matches!(w.body.1[0].1, Statement::Break));
        } else {
            panic!("Expected while statement");
        }
    }

    #[test]
    fn test_continue_statement() {
        let result = parse_stmt_test("for (i in items) { continue }").unwrap();
        if let Statement::For(f) = result.1 {
            assert_eq!(f.body.1.len(), 1);
            assert!(matches!(f.body.1[0].1, Statement::Continue));
        } else {
            panic!("Expected for statement");
        }
    }

    #[test]
    fn test_dot_assign_statement() {
        let result = parse_expr_test("block {. x + 1}").unwrap();
        let expected = Expression::Block(Block {
            body: vec![
                (7..14, Statement::Expression(Expression::BinaryOperator(
                    BinaryOperator {
                        lhs: (7..8, Box::new(Expression::IdentifierReference(IdentifierReference::Value("_".into())))),
                        op_type: BinaryOperatorType::DotAssign,
                        rhs: (9..14, Box::new(Expression::BinaryOperator(
                            BinaryOperator {
                                lhs: (9..10, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".into())))),
                                op_type: BinaryOperatorType::Add,
                                rhs: (13..14, Box::new(Expression::Literal(Literal::Int(1)))),
                            }
                        )))
                    }
                )))
            ]
        });
        assert_eq!(result.1, expected);
    }

    // Complex expression tests
    #[test]
    fn test_complex_nested_expression() {
        let result = parse_expr_test("(a + b) * (c - d)").unwrap();
        let expected = Expression::BinaryOperator(BinaryOperator {
            lhs: (0..7, Box::new(Expression::BinaryOperator(BinaryOperator {
                lhs: (1..2, Box::new(Expression::IdentifierReference(IdentifierReference::Value("a".to_string())))),
                op_type: BinaryOperatorType::Add,
                rhs: (5..6, Box::new(Expression::IdentifierReference(IdentifierReference::Value("b".to_string())))),
            }))),
            op_type: BinaryOperatorType::Multiply,
            rhs: (10..17, Box::new(Expression::BinaryOperator(BinaryOperator {
                lhs: (11..12, Box::new(Expression::IdentifierReference(IdentifierReference::Value("c".to_string())))),
                op_type: BinaryOperatorType::Subtract,
                rhs: (15..16, Box::new(Expression::IdentifierReference(IdentifierReference::Value("d".to_string())))),
            }))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_lambda_invocation_chain() {
        let result = parse_expr_test("f(x)(y)(z)").unwrap();
        let expected = Expression::LambdaInvocation(LambdaInvocation {
            lambda: (0..7, Box::new(Expression::LambdaInvocation(LambdaInvocation {
                lambda: (0..4, Box::new(Expression::LambdaInvocation(LambdaInvocation {
                    lambda: (0..1, Box::new(Expression::IdentifierReference(IdentifierReference::Value("f".to_string())))),
                    arguments: (1..4, vec![
                        (None, (2..3, Expression::IdentifierReference(IdentifierReference::Value("x".to_string()))))
                    ]),
                }))),
                arguments: (4..7, vec![
                    (None, (5..6, Expression::IdentifierReference(IdentifierReference::Value("y".to_string()))))
                ]),
            }))),
            arguments: (7..10, vec![
                (None, (8..9, Expression::IdentifierReference(IdentifierReference::Value("z".to_string()))))
            ]),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_mixed_postfix_operations() {
        let result = parse_expr_test("obj.method()[0].field").unwrap();
        let expected = Expression::Property(Property {
            base: (0..15, Box::new(Expression::Subscript(Subscript {
                base: (0..12, Box::new(Expression::LambdaInvocation(LambdaInvocation {
                    lambda: (0..10, Box::new(Expression::Property(Property {
                        base: (0..3, Box::new(Expression::IdentifierReference(IdentifierReference::Value("obj".to_string())))),
                        attribute: (4..10, IdentifierReference::Value("method".to_string())),
                    }))),
                    arguments: (10..12, vec![]),
                }))),
                index: (13..14, Box::new(Expression::Literal(Literal::Int(0)))),
            }))),
            attribute: (16..21, IdentifierReference::Value("field".to_string())),
        });
        assert_eq!(result.1, expected);
    }

    // Operator definition test
    #[test]
    fn test_operator_definition() {
        let result = parse_expr_test("operator |x, y| x + y").unwrap();
        let expected = Expression::OperationDefinition(OperatorDefinition {
            lambda: (9..21, Box::new(Expression::LambdaDefinition(LambdaDefinition {
                args: vec![
                    ((10..11, IdentifierDeclaration("x".to_string())), None),
                    ((13..14, IdentifierDeclaration("y".to_string())), None),
                ],
                body: (16..21, LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                    lhs: (16..17, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                    op_type: BinaryOperatorType::Add,
                    rhs: (20..21, Box::new(Expression::IdentifierReference(IdentifierReference::Value("y".to_string())))),
                })))),
            }))),
        });
        assert_eq!(result.1, expected);
    }

    // Block and Anim tests
    #[test]
    fn test_block_expression() {
        let result = parse_expr_test("block { let x = 5\n x * 2 }").unwrap();
        let expected = Expression::Block(Block {
            body: vec![
                (8..17, Statement::Declaration(Declaration {
                    var_type: VariableType::Let,
                    identifier: (12..13, IdentifierDeclaration("x".to_string())),
                    value: (16..17, Expression::Literal(Literal::Int(5))),
                })),
                (19..24, Statement::Expression(Expression::BinaryOperator(BinaryOperator {
                    lhs: (19..20, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                    op_type: BinaryOperatorType::Multiply,
                    rhs: (23..24, Box::new(Expression::Literal(Literal::Int(2)))),
                }))),
            ],
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_anim_expression() {
        let result = parse_expr_test("anim { play circle }").unwrap();
        let expected = Expression::Anim(Anim {
            body: vec![
                (7..18, Statement::Play(Play {
                    animations: (12..18, Expression::IdentifierReference(IdentifierReference::Value("circle".to_string()))),
                }))
            ],
        });
        assert_eq!(result.1, expected);
    }

    // Multiline tests
    #[test]
    fn test_multiline_statement_list() {
        let content = "let x = 1\nlet y = 2\nlet z = 3";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None);
        let result = parser.parse_statement_list();
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_semicolon_separated_statements() {
        let content = "let x = 1; let y = 2; let z = 3";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Slide, None);
        let result = parser.parse_statement_list();
        assert_eq!(result.len(), 3);
    }

    // Edge cases
    #[test]
    fn test_nested_vectors() {
        let result = parse_expr_test("[[1, 2], [3, 4]]").unwrap();
        let expected = Expression::Literal(Literal::Vector(vec![
            (1..7, Expression::Literal(Literal::Vector(vec![
                (2..3, Expression::Literal(Literal::Int(1))),
                (5..6, Expression::Literal(Literal::Int(2))),
            ]))),
            (9..15, Expression::Literal(Literal::Vector(vec![
                (10..11, Expression::Literal(Literal::Int(3))),
                (13..14, Expression::Literal(Literal::Int(4))),
            ]))),
        ]));
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_map_with_complex_values() {
        let result = parse_expr_test("[1 -> [1, 2], 2 -> [3, 4]]").unwrap();
        let expected = Expression::Literal(Literal::Map(vec![
            (
                (1..2, Expression::Literal(Literal::Int(1))),
                (6..12, Expression::Literal(Literal::Vector(vec![
                    (7..8, Expression::Literal(Literal::Int(1))),
                    (10..11, Expression::Literal(Literal::Int(2))),
                ])))
            ),
            (
                (14..15, Expression::Literal(Literal::Int(2))),
                (19..25, Expression::Literal(Literal::Vector(vec![
                    (20..21, Expression::Literal(Literal::Int(3))),
                    (23..24, Expression::Literal(Literal::Int(4))),
                ])))
            ),
        ]));
        assert_eq!(result.1, expected);
    }

    // Complex integration tests
    #[test]
    fn test_complex_lambda_with_nested_blocks() {
        let result = parse_expr_test("|n| anim { for (i in [1, 2, 3]) { play circle } }").unwrap();
        let expected = Expression::LambdaDefinition(LambdaDefinition {
            args: vec![((1..2, IdentifierDeclaration("n".to_string())), None)],
            body: (4..49, LambdaBody::Inline(Box::new(Expression::Anim(Anim {
                body: vec![
                    (11..47, Statement::For(For {
                        var_name: (16..17, IdentifierDeclaration("i".to_string())),
                        container: (21..30, Expression::Literal(Literal::Vector(vec![
                            (22..23, Expression::Literal(Literal::Int(1))),
                            (25..26, Expression::Literal(Literal::Int(2))),
                            (28..29, Expression::Literal(Literal::Int(3))),
                        ]))),
                        body: (32..47, vec![
                            (34..45, Statement::Play(Play {
                                animations: (39..45, Expression::IdentifierReference(IdentifierReference::Value("circle".to_string()))),
                            }))
                        ]),
                    }))
                ],
            })))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_operator_invocation_with_lambda() {
        let result = parse_expr_test("map{|x| x * 2} [1, 2, 3]").unwrap();
        let expected = Expression::OperatorInvocation(OperatorInvocation {
            operator: (0..3, Box::new(Expression::IdentifierReference(IdentifierReference::Value("map".to_string())))),
            arguments: (3..14, vec![
                (None, (4..13, Expression::LambdaDefinition(LambdaDefinition {
                    args: vec![((5..6, IdentifierDeclaration("x".to_string())), None)],
                    body: (8..13, LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                        lhs: (8..9, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                        op_type: BinaryOperatorType::Multiply,
                        rhs: (12..13, Box::new(Expression::Literal(Literal::Int(2)))),
                    })))),
                })))
            ]),
            operand: (15..24, Box::new(Expression::Literal(Literal::Vector(vec![
                (16..17, Expression::Literal(Literal::Int(1))),
                (19..20, Expression::Literal(Literal::Int(2))),
                (22..23, Expression::Literal(Literal::Int(3))),
            ])))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_complex_property_chain_with_calls() {
        let result = parse_expr_test("obj.data.process(x).result[0]").unwrap();
        let expected = Expression::Subscript(Subscript {
            base: (0..26, Box::new(Expression::Property(Property {
                base: (0..19, Box::new(Expression::LambdaInvocation(LambdaInvocation {
                    lambda: (0..16, Box::new(Expression::Property(Property {
                        base: (0..8, Box::new(Expression::Property(Property {
                            base: (0..3, Box::new(Expression::IdentifierReference(IdentifierReference::Value("obj".to_string())))),
                            attribute: (4..8, IdentifierReference::Value("data".to_string())),
                        }))),
                        attribute: (9..16, IdentifierReference::Value("process".to_string())),
                    }))),
                    arguments: (16..19, vec![
                        (None, (17..18, Expression::IdentifierReference(IdentifierReference::Value("x".to_string()))))
                    ]),
                }))),
                attribute: (20..26, IdentifierReference::Value("result".to_string())),
            }))),
            index: (27..28, Box::new(Expression::Literal(Literal::Int(0)))),
        });
        assert_eq!(result.1, expected);
    }

    #[test]
    fn test_nested_operator_invocations() {
        let result = parse_expr_test("fold{|a, b| a + b, 0} map{|x| x * 2} data").unwrap();
        let expected = Expression::OperatorInvocation(OperatorInvocation {
            operator: (0..4, Box::new(Expression::IdentifierReference(IdentifierReference::Value("fold".to_string())))),
            arguments: (4..21, vec![
                (None, (5..17, Expression::LambdaDefinition(LambdaDefinition {
                    args: vec![
                        ((6..7, IdentifierDeclaration("a".to_string())), None),
                        ((9..10, IdentifierDeclaration("b".to_string())), None),
                    ],
                    body: (12..17, LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                        lhs: (12..13, Box::new(Expression::IdentifierReference(IdentifierReference::Value("a".to_string())))),
                        op_type: BinaryOperatorType::Add,
                        rhs: (16..17, Box::new(Expression::IdentifierReference(IdentifierReference::Value("b".to_string())))),
                    })))),
                }))),
                (None, (19..20, Expression::Literal(Literal::Int(0)))),
            ]),
            operand: (22..41, Box::new(Expression::OperatorInvocation(OperatorInvocation {
                operator: (22..25, Box::new(Expression::IdentifierReference(IdentifierReference::Value("map".to_string())))),
                arguments: (25..36, vec![
                    (None, (26..35, Expression::LambdaDefinition(LambdaDefinition {
                        args: vec![((27..28, IdentifierDeclaration("x".to_string())), None)],
                        body: (30..35, LambdaBody::Inline(Box::new(Expression::BinaryOperator(BinaryOperator {
                            lhs: (30..31, Box::new(Expression::IdentifierReference(IdentifierReference::Value("x".to_string())))),
                            op_type: BinaryOperatorType::Multiply,
                            rhs: (34..35, Box::new(Expression::Literal(Literal::Int(2)))),
                        })))),
                    })))
                ]),
                operand: (37..41, Box::new(Expression::IdentifierReference(IdentifierReference::Value("data".to_string())))),
            }))),
        });
        assert_eq!(result.1, expected);
    }

    // Error cases
    #[test]
    fn test_error_unmatched_paren() {
        let result = parse_expr_test("(1 + 2");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_empty_char_literal() {
        let result = parse_expr_test("''");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_invalid_escape() {
        let result = parse_expr_test(r#""hello%z""#);
        assert!(result.is_err());
    }

    #[test]
    fn test_error_missing_operand() {
        let result = parse_expr_test("1 +");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_invalid_identifier_start_with_number() {
        let result = parse_stmt_test("let 123x = 5");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_break_outside_loop() {
        let content = "break";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Init, None);
        let result = parser.parse_statement();
        assert!(result.is_err());
    }

    #[test]
    fn test_error_return_outside_function() {
        let content = "return 5";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Init, None);
        let result = parser.parse_statement();
        assert!(result.is_err());
    }

    #[test]
    fn test_error_play_outside_anim() {
        let content = "play animation";
        let lexed = lex(content);
        let text_rope = Rope::from_str(content);
        let mut parser = SectionParser::new(lexed, text_rope, SectionType::Init, None);
        let result = parser.parse_statement();
        assert!(result.is_err());
    }

    #[test]
    fn test_error_ambiguous_vector_map() {
        let result = parse_expr_test("[1, 2 -> 3]");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_missing_lambda_argument() {
        let result = parse_expr_test("|x,| x + 1");
        assert!(result.is_err());
    }
}
