use gpui::{App, AppContext, Entity};

use crate::state::{execution_state::ExecutionState, textual_state::TextualState};

pub struct DocumentState {
    pub textual_state: Entity<TextualState>,
    pub execution_state: Entity<ExecutionState>,
}

impl DocumentState {
    pub fn new(cx: &mut App) -> Self {
        Self {
            textual_state: cx.new(|_| TextualState::default()),
            execution_state: cx.new(|_| ExecutionState::default()),
        }
    }
}
