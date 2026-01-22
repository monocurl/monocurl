use std::{cell::{Cell, RefCell}, collections::{HashMap, HashSet}, ops::Range, path::PathBuf, usize};

use lexer::token::Token;
use structs::{rope::{Attribute, Rope, TextAggregate}, text::{Count8, Span8}};

use crate::{ast::{Anim, BinaryOperator, BinaryOperatorType, Block, Declaration, DirectionalLiteral, Expression, For, IdentifierDeclaration, IdentifierReference, If, LambdaBody, LambdaDefinition, LambdaInvocation, Literal, OperatorDefinition, OperatorInvocation, Play, Property, Return, Section, SectionType, SpanTagged, Statement, Subscript, UnaryOperatorType, UnaryPreOperator, VariableType, While}, parser::predicate::{BinaryOperator, Exact, InLambdaOrBlockPredicate, InLoopPredicate, NullPredicate, PlayablePredicate, StatePredicate, TokenPredicate, UnaryOperator, VariableDeclaration}};

type Result<T> = std::result::Result<T, ()>;
const CHAR_ESCAPE: &'static str = "\n%\"'\t";

macro_rules! try_all {
    (@branches $self:expr, $token:expr, $span:expr, [], [$($collected:tt)*]) => {
        try_all!(@execute $self, $token, $span, [$($collected)*])
    };

    (@branches $self:expr, $token:expr, $span:expr,
        [else $else_body: expr],
        [$($collected:tt)*]) => {{
        try_all!(@execute_head $self, $token, $span, [$($collected)*]);
        // execute else body
        return $else_body
    }};
    // state predicate and identifier
    (@branches $self:expr, $token:expr, $span:expr,
     [if $state_pred:expr, $var:ident = $token_pred:expr => $body:expr, $($rest:tt)*],
     [$($collected:tt)*]) => {
        try_all!(@branches $self, $token, $span, [$($rest)*],
                 [$($collected)* (Some($state_pred), $var, $token_pred, $body),])
    };

    // state predicate, no identifier
    (@branches $self:expr, $token:expr, $span:expr,
     [if $state_pred:expr, $token_pred:expr => $body:expr, $($rest:tt)*],
     [$($collected:tt)*]) => {
        try_all!(@branches $self, $token, $span, [$($rest)*],
                 [$($collected)* (Some($state_pred), _ignore, $token_pred, $body),])
    };

    // no state predicate, with identifier
    (@branches $self:expr, $token:expr, $span:expr,
     [$var:ident = $token_pred:expr => $body:expr, $($rest:tt)*],
     [$($collected:tt)*]) => {
        try_all!(@branches $self, $token, $span, [$($rest)*],
                 [$($collected)* (Option::<NullPredicate>::None, $var, $token_pred, $body),])
    };

    // no state predicate, no identifier
    (@branches $self:expr, $token:expr, $span:expr,
     [$token_pred:expr => $body:expr, $($rest:tt)*],
     [$($collected:tt)*]) => {
        try_all!(@branches $self, $token, $span, [$($rest)*],
                 [$($collected)* (Option::<NullPredicate>::None, _ignore, $token_pred, $body),])
    };

    (@execute_head $self:expr, $token:expr, $span:expr, [$(($state_pred:expr, $var:ident, $token_pred:expr, $body:expr),)*]) => {{
        if let Some(token) = $token {
            // if it contains the cursor, emit all the possibilities
            if $span.contains(&$self.cursor_position) {
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
                        return Ok($body);
                    }
                }
            )*
        }
    }};

    (@execute $self:expr, $token:expr, $span:expr, [$(($state_pred:expr, $var:ident, $token_pred:expr, $body:expr),)*]) => {{
        // first pass, try to match any branch
        try_all!(@execute_head $self, $token, $span, [$(($state_pred, $var, $token_pred, $body),)*]);

        // second pass - all branches failed, build error message
        let mut token_expectations: Vec<&'static str> = Vec::new();
        let mut failed_state_hints: Vec<&'static str> = Vec::new();
        $(
            let state_ok = if let Some(pred) = $state_pred {
                let ok = pred.ok(&$self.state);
                if !ok {
                    // if it only failed due to abnormal state, give that as a hint
                    if let Some(token) = $token {
                        if $token_pred.convert(token.clone()).is_some() {
                            failed_state_hints.push(pred.fail_description());
                        }
                    }
                }
                ok
            } else {
                true
            };
            if state_ok {
                // state was ok but token didn't match
                token_expectations.push($token_pred.description());
            }
        )*
        let mut error_msg = String::new();
        let token_desc = $self.peek_token_description();
        error_msg.push_str(&format!(
            "\"{}\" is illegal in the current context;\n",
            token_desc
        ));
        if !token_expectations.is_empty() {
            error_msg.push_str("expected ");
            for (i, expectation) in token_expectations.iter().enumerate() {
                if i > 0 {
                    if i == token_expectations.len() - 1 {
                        error_msg.push_str(", or ");
                    } else {
                        error_msg.push_str(", ");
                    }
                }
                error_msg.push_str(expectation);
            }
            error_msg.push('\n');
        }
        for hint in failed_state_hints {
            error_msg.push_str(&format!(
                "hint: may be illegal since {}\n",
                hint
            ));
        }
        $self.emit_error(error_msg, $span);
        Err(())
    }};

    // match token with or without binding to identifier
    (@match_token Some($var:ident), $token:expr, $token_pred:expr, $body:expr) => {

    };
    (@match_token None, $token:expr, $token_pred:expr, $body:expr) => {
        if let Some(_) = $token_pred.convert($token.clone()) {
            return Ok($body);
        }
    };

    // entry point
    ($self:expr, { $($branches:tt)+ }) => {{
        (|| {
                let (token, _span) = {
                if let Some((token, span)) = $self.peek_token() {
                    (Some(*token), span.clone())
                }
                else {
                    let codeunits = $self.text_rope.codeunits();
                    let span = codeunits.saturating_sub(1)..codeunits;
                    (None, span)
                }
            };
            try_all!(@branches $self, token, _span, [$($branches)*], [])
        })()
    }};
}

mod predicate {
    use lexer::token::Token;

    use crate::{ast::{BinaryOperatorType, UnaryOperatorType, VariableType}, parser::{ParseArtifacts, State}};

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

    pub(super) struct Exact(pub Token);
    impl TokenPredicate for Exact {
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

    pub(super) struct UnaryOperator;
    impl TokenPredicate for UnaryOperator {
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

    pub(super) struct BinaryOperator;
    impl TokenPredicate for BinaryOperator {
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

    pub(super) struct VariableDeclaration;
    impl TokenPredicate for VariableDeclaration {
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
    slide_boundaries: Vec<Span8>,
    bracket_partners: HashMap<usize, usize>
}

impl Precomputation {
    fn bracket_internal_range(&self, bracket_index: usize) -> Result<Range<usize>> {
        if let Some(end) = self.bracket_partners.get(&bracket_index) {
            Ok(bracket_index + 1 .. *end)
        }
        else {
            Err(())
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
    cursor_position: Count8,

    text_rope: Rope<TextAggregate>,
    tokens: Vec<(Token, Span8)>,
    token_index: usize,

    artifacts: ParseArtifacts
}

pub struct Parser;

pub struct ParseArtifacts {
    pub diagnostics: Vec<Diagnostic>,
    pub cursor_possibilities: HashSet<Token>
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
            return "<end of file>".to_string()
        };

        self.text_rope.iterator_range(span.clone()).collect()
    }

    fn peek_token(&self) -> Option<&(Token, Span8)> {
        if self.token_index == self.state.operating_range().end {
            return None;
        }

        Some(&self.tokens[self.token_index])
    }

    fn advance_token(&mut self) -> Span8 {
        debug_assert!(self.state.operating_range().contains(&self.token_index));
        let span = self.tokens[self.token_index].1.clone();
        self.token_index += 1;
        span
    }
}

impl SectionParser {
    fn emit_error(&mut self, error_message: String, span: Span8) {
        self.artifacts.diagnostics.push(
            Diagnostic { is_error: true, span, title: "Parse Error".to_string(), message: error_message }
        )
    }

    fn emit_cursor_possibility(&mut self, token: Token) {
        self.artifacts.cursor_possibilities.insert(token);
    }
}

impl SectionParser {
    fn read_token(&mut self, token: Token) -> Result<Span8> {
        try_all!(self, {
            Exact(token) => {
                self.advance_token()
            },
        })
    }

    fn read_if_token(&mut self, token: Token) -> Option<Span8> {
        try_all!(self, {
            Exact(token) => {
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
    pub fn parse_section(&mut self, section_type: SectionType) -> Result<Section> {
        let body = self.parse_statement_list()?;
        Ok(Section {
            body,
            section_type,
        })
    }

    pub fn parse_statement_list(&mut self) -> Result<Vec<SpanTagged<Statement>>> {
        let mut statements = Vec::new();
        let mut err = None;
        while self.peek_token().is_some() {
            let mut read = || -> Result<SpanTagged<Statement>> {
                // skip newlines and semicolons
                while matches!(self.peek_token(), Some((Token::Newline, _)) | Some((Token::Semicolon, _))) {
                    self.advance_token();
                }

                let ret = self.parse_statement()?;

                // ensure no hanging content
                if self.peek_token().is_some() {
                    try_all!(self, {
                        Exact(Token::Newline) => {
                            self.advance_token();
                        },
                        Exact(Token::Semicolon) => {
                            self.advance_token();
                        },
                    })?;
                }

                Ok(ret)
            };

            match read() {
                Ok(statement) => statements.push(statement),
                Err(e) => err = Some(e)
            };
        }

        err.map_or(Ok(statements), Err)
    }

    pub fn parse_statement(&mut self) -> Result<SpanTagged<Statement>> {
        try_all!(self, {
            vtype = VariableDeclaration => {
                let (span, statement) = self.parse_declaration(vtype)?;
                (span, Statement::Declaration(statement))
            },
            Exact(Token::For) => {
                let (span, statement) = self.parse_for()?;
                (span, Statement::For(statement))
            },
            Exact(Token::While) => {
                let (span, statement) = self.parse_while()?;
                (span, Statement::While(statement))
            },
            Exact(Token::If) => {
                let (span, statement) = self.parse_if()?;
                (span, Statement::If(statement))
            },
            if PlayablePredicate, Exact(Token::Play) => {
                let (span, statement) = self.parse_play()?;
                (span, Statement::Play(statement))
            },
            if InLoopPredicate, Exact(Token::Break) => {
                let span = self.advance_token();
                (span, Statement::Break)
            },
            if InLoopPredicate, Exact(Token::Continue) => {
                let span = self.advance_token();
                (span, Statement::Continue)
            },
            if InLambdaOrBlockPredicate, Exact(Token::Return) => {
                let (span, statement) = self.parse_return()?;
                (span, Statement::Return(statement))
            },
            if InLambdaOrBlockPredicate, Exact(Token::Dot) => {
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
        self.read_token(Token::LParen)?;
        let condition = self.parse_expr()?;
        self.read_token(Token::RParen)?;

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
        self.read_token(Token::LParen)?;
        let identifier = self.parse_identifier_declaration()?;
        self.read_token(Token::In)?;

        let container = self.parse_expr()?;
        self.read_token(Token::RParen)?;

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
        self.read_token(Token::LParen)?;
        let condition = self.parse_expr()?;
        self.read_token(Token::RParen)?;
        let body = self.parse_body(|_| {})?;
        if self.read_if_token(Token::Else).is_some() {
            // parse else block
            let else_block = if self.read_if_token(Token::If).is_some() {
                // nested if
                let (span, statement) = self.parse_if()?;
                (span.clone(), vec![(span, Statement::If(statement))])
            }
            else {
                // pure else block
                self.parse_body(|_| {})?
            };

            Ok((base_span.start .. else_block.0.end, If {
                condition,
                if_block: body,
                else_block: None
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
            self.emit_error(format!("\"{}\" is not a valid identifier name", identifier), span.clone());
            return Err(());
        }

        Ok((span, IdentifierDeclaration(identifier)))
    }

    fn parse_body(&mut self, frame_builder: impl FnOnce(&mut ContextFrame)) -> Result<SpanTagged<Vec<SpanTagged<Statement>>>> {
        let base_span = self.read_token(Token::LFlower)?;
        let body_range = self.precomputation.bracket_internal_range(self.token_index)?;

        self.state.push_frame(|frame| {
            frame.operating_range = body_range;
            frame_builder(frame)
        });
        let result = self.parse_statement_list();
        self.state.pop_frame();
        let result = result?;

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
                op = BinaryOperator => {
                    if (op.priority(), op.associativity()) >= (priority, 1) {
                        // combine right
                        self.advance_newlines();
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
        let expr = try_all!(self, {
            /* unary operators */
            op = UnaryOperator => {
                self.parse_unary_preoperator(op)?
            },
            /* entire sub expression */
            Exact(Token::LParen) => {
                self.parse_parenthesis_sub_expression()?
            },
            /* identifier */
            Exact(Token::Identifier) => {
                let span = self.advance_token();
                let str: String = self.text_rope.iterator_range(span.clone()).collect();
                (span, Expression::IdentifierReference(IdentifierReference::Value(str)))
            },
            Exact(Token::Reference) => {
                let base_span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier()?;
                (base_span.start..full_span.end, Expression::IdentifierReference(IdentifierReference::Reference(str)))
            },
            Exact(Token::StatefulReference) => {
                let base_span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier()?;
                (base_span.start..full_span.end, Expression::IdentifierReference(IdentifierReference::Reference(str)))
            },
            Exact(Token::Multiply) => {
                let base_span = self.advance_token();
                let (full_span, str) = self.read_pure_identifier()?;
                (base_span.start..full_span.end, Expression::IdentifierReference(IdentifierReference::Reference(str)))
            },
            /* operator definition */
            Exact(Token::Operator) => {
                let base_span = self.advance_token();
                let target = self.parse_unary()?;
                (base_span.start..target.0.end, Expression::OperationDefinition(OperatorDefinition { lambda: (target.0, Box::new(target.1)) }))
            },
            /* lambda definition */
            Exact(Token::Pipe) => {
                self.parse_lambda()?
            },
            /* literals */
            Exact(Token::IntegerLiteral) => {
                self.parse_int_literal()?
            },
            Exact(Token::DoubleLiteral) => {
                self.parse_double_literal()?
            },
            Exact(Token::StringLiteral) => {
                self.parse_string_literal()?
            },
            Exact(Token::CharLiteral) => {
                self.parse_char_literal()?
            },
            Exact(Token::LBracket) => {
                self.parse_map_or_vector_literal()?
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
            let next = try_all!(self, {
                Exact(Token::LParen) => {
                    // lambda invocation
                    self.advance_token();
                    let arguments = self.parse_invocation(Token::LParen, Token::RParen)?;
                    let old = boxify(take_expr());
                    (old.0.start..arguments.0.end, Expression::LambdaInvocation(LambdaInvocation {
                        lambda: old,
                        arguments: arguments
                    }))
                },
                Exact(Token::LFlower) => {
                    // operator invocation
                    self.advance_token();
                    let arguments = self.parse_invocation(Token::LFlower, Token::RFlower)?;
                    let old = boxify(take_expr());
                    let operand = self.parse_unary()?;

                    (old.0.start..operand.0.end, Expression::OperatorInvocation(OperatorInvocation {
                        operator: old,
                        arguments,
                        operand: boxify(operand)
                    }))
                },
                Exact(Token::LBracket) => {
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
                Exact(Token::Dot) => {
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
    fn parse_invocation(&mut self, start: Token, end: Token)
    -> Result<SpanTagged<Vec<(Option<SpanTagged<IdentifierDeclaration>>, SpanTagged<Expression>)>>>
    {
        self.debug_assert_token_eq(start);
        let range = self.precomputation.bracket_internal_range(self.token_index)?;
        let base_span = self.advance_token();

        self.state.push_frame(|frame| frame.operating_range = range);

        let mut arguments = Vec::new();
        let mut err = None;
        loop {
            self.advance_newlines();
            if self.peek_token().is_none() {
                break;
            }

            let mut read = || {
                if arguments.len() > 0 {
                    self.read_token(Token::Comma)?;
                }

                self.advance_newlines();
                let label = self.peek_token()
                    .and_then(|tok| {
                        if matches!(tok.0, Token::Identifier) {
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
                    self.advance_newlines();

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
        let base_span = self.advance_token();
        let inner_range = self.precomputation.bracket_internal_range(self.token_index)?;

        self.state.push_frame(|frame| {
            frame.operating_range = inner_range;
        });
        let result = self.parse_expr();
        self.state.pop_frame();
        let result = result?;

        let terminal_span = self.read_token(Token::RParen)?;

        Ok((base_span.start .. terminal_span.end, result.1))
    }

    fn parse_anim(&mut self) -> Result<SpanTagged<Anim>> {
        self.debug_assert_token_eq(Token::Anim);
        let base_span = self.advance_token();

        let body = self.parse_body(|frame| {
            frame.in_playable_block = true;
            frame.in_lambda_or_block = false;
        })?;

        Ok((base_span.start .. body.0.end, Anim {
            body: body.1
        }))
    }

    fn parse_lambda(&mut self) -> Result<SpanTagged<Expression>> {
        self.debug_assert_token_eq(Token::Pipe);
        let base_span = self.advance_token();
        let mut args = Vec::new();
        // parse arguments (and default values)
        if self.read_if_token(Token::Pipe).is_none() {
            let is_first = true;
            loop {
                if !is_first {
                    let mut done = false;
                    try_all!(self, {
                        Exact(Token::Pipe) => {
                            done = true;
                        },
                        Exact(Token::Comma) => {
                            self.advance_token();
                        },
                    })?;
                    if done {
                        break;
                    }
                }

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
            Exact(Token::LFlower) => {
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

    fn parse_block(&mut self) -> Result<SpanTagged<Block>> {
        self.debug_assert_token_eq(Token::Block);
        let base_span = self.advance_token();

        let body = self.parse_body(|frame| {
            frame.in_loop = false;
            frame.in_playable_block = false;
            frame.in_lambda_or_block = true;
        })?;

        Ok((base_span.start .. body.0.end, Block {
            body: body.1
        }))
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
                self.emit_error(message.into(), span);
                return Err(())
            }
        };
        Ok((span, Expression::Literal(literal)))
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
                    if next.is_some_and(|c| CHAR_ESCAPE.find(c).is_some()) {
                        build.push(next.unwrap());
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

    fn parse_char_literal(&mut self) -> Result<SpanTagged<Expression>> {
        self.parse_basic_literal(Token::CharLiteral, |string| {
            let mut it = string.chars();
            if it.next() != Some('\'') {
                return Err("Malformed char literal")
            }

            let ch = match it.next() {
                Some('%') => {
                    let next = it.next();
                    if let Some(c) = next {
                        if CHAR_ESCAPE.find(c).is_some() {
                            c
                        } else {
                            return Err("Illegal escape character")
                        }
                    } else {
                        return Err("Malformed escape character")
                    }
                }
                Some('\'') => {
                    return Err("Empty char literal")
                }
                Some(c) => c,
                None => {
                    return Err("Malformed char literal")
                }
            };

            if it.next() != Some('\'') {
                return Err("Malformed char literal")
            }

            if it.next().is_some() {
                return Err("Malformed char literal")
            }

            Ok(Literal::Char(ch))
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

    fn parse_double_literal(&mut self) -> Result<SpanTagged<Expression>> {
        self.parse_basic_literal(Token::DoubleLiteral, |string| {
            Self::parse_numeric_with_suffix(string, |s| {
                s.parse::<f64>().map_err(|_| "Invalid double literal")
            },|s| {
                Ok(Literal::Double(s.parse::<f64>().map_err(|_| "Invalid double literal")?))
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
            return Ok(Literal::Double(radians));
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
                    self.emit_error("Ambiguous literal; cannot resolve ambiguity between list and map".into(), base_span.start..span.end);
                    return Err(());
                }
                let value = self.parse_expr()?;
                map_entries.push((entry, value));
            }
            else {
                if !map_entries.is_empty() {
                    self.emit_error("Ambiguous literal; cannot resolve ambiguity between list and map".into(), base_span.start..entry.0.end);
                    return Err(());
                }
                vector_entries.push(entry)
            }

            self.advance_newlines();
            let mut is_finished = false;
            try_all!(self, {
                Exact(Token::Comma) => {},
                Exact(Token::RBracket) => {
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

impl Parser {
    fn gather_sections(&mut self) {

    }

    fn new(external_context: &ExternalContext, lex_rope: Rope<Attribute<Token>>, text_rope: TextAggregate) -> Self {
        // precompute on the section

        let mut utf8 = 0;
        let token_list: Vec<_> = lex_rope.iterator(0)
            .map(|(len, token)| {
                let span = utf8 .. utf8 + len;
                utf8 += len;
                (token, span)
            })
            .filter(|(tok, _)| tok != &Token::Whitespace && tok != &Token::Comment)
            .collect();

        SectionParser {
            precomputation: todo!(),
            state: todo!(),
            text_rope: todo!(),
            tokens: todo!(),
            token_index: todo!(),
            cursor_position: todo!(),
            artifacts: todo!(),
        }
    }

    pub fn parse(external_context: &'a ExternalContext, lex_rope: Rope<Attribute<Token>>, text_rope: Rope<TextAggregate>) -> Result<Vec<Section>, Vec<Diagnostic>> {
        // 1. crawl imports to gather sections
        Err(vec![])
    }
}
