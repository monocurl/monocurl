use lexer::token::Token;

use crate::{
    ast::{BinaryOperatorType, SectionType, UnaryOperatorType, VariableType},
    parser::{ParseArtifacts, ShortTermState},
};

pub(super) trait StatePredicate {
    fn ok(&self, state: &ShortTermState) -> bool;
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
    fn ok(&self, _state: &ShortTermState) -> bool {
        true
    }

    fn fail_description(&self) -> &'static str {
        unreachable!()
    }
}

pub(super) struct InLoopPredicate;
impl StatePredicate for InLoopPredicate {
    fn ok(&self, state: &ShortTermState) -> bool {
        state.top().in_loop
    }

    fn fail_description(&self) -> &'static str {
        "we are not directly inside a loop"
    }
}

pub(super) struct PlayablePredicate;
impl StatePredicate for PlayablePredicate {
    fn ok(&self, state: &ShortTermState) -> bool {
        state.top().in_playable_block
    }

    fn fail_description(&self) -> &'static str {
        "we are not in an anim body"
    }
}

pub(super) struct InLambdaOrBlockPredicate;
impl StatePredicate for InLambdaOrBlockPredicate {
    fn ok(&self, state: &ShortTermState) -> bool {
        state.top().in_lambda_or_block
    }

    fn fail_description(&self) -> &'static str {
        "we are not in a lambda or block body"
    }
}

pub(super) struct InStdLibPredicate;
impl StatePredicate for InStdLibPredicate {
    fn ok(&self, state: &ShortTermState) -> bool {
        state.section_type == SectionType::StandardLibrary
    }

    fn fail_description(&self) -> &'static str {
        "this keyword is reserved for the Monocurl compiler"
    }
}

pub(super) struct RootTopLevelPredicate;
impl StatePredicate for RootTopLevelPredicate {
    fn ok(&self, state: &ShortTermState) -> bool {
        state.root_import_span.is_none() && state.frames.len() == 1
    }

    fn fail_description(&self) -> &'static str {
        "we are not at the root statement level"
    }
}

pub(super) struct ExactPredDesc(pub Token, pub &'static str);
impl TokenPredicate for ExactPredDesc {
    type Output = Token;

    fn convert(&self, token: Token) -> Option<Self::Output> {
        if token == self.0 { Some(token) } else { None }
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
        if token == self.0 { Some(token) } else { None }
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
            _ => None,
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
            Token::Eq => Some(BinaryOperatorType::Eq),
            Token::Ne => Some(BinaryOperatorType::Ne),
            Token::Lt => Some(BinaryOperatorType::Lt),
            Token::Le => Some(BinaryOperatorType::Le),
            Token::Gt => Some(BinaryOperatorType::Gt),
            Token::Ge => Some(BinaryOperatorType::Ge),
            Token::Assign => Some(BinaryOperatorType::Assign),
            Token::DotAssign => Some(BinaryOperatorType::DotAssign),
            Token::In => Some(BinaryOperatorType::In),
            _ => None,
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
            _ => None,
        }
    }

    fn description(&self) -> &'static str {
        "<variable declaration>"
    }

    fn emit_possibilities(&self, dump: &mut ParseArtifacts) {
        dump.cursor_possibilities.insert(Token::Let);
        dump.cursor_possibilities.insert(Token::Var);
        dump.cursor_possibilities.insert(Token::Mesh);
    }
}
