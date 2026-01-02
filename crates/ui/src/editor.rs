use std::path::PathBuf;

use gpui::*;

use crate::{document_state::DocumentState, editor::{backing::EditorBackend, text_editor::TextEditor}};

mod backing;
mod line_map;
mod wrapped_line;
pub mod text_editor;

pub struct Editor {
    internal_path: PathBuf,
    editor: Entity<TextEditor<DocumentState>>,
    state: Entity<DocumentState>,
}

impl Editor {
    pub fn new(state: Entity<DocumentState>, internal_path: PathBuf, dirty: Entity<bool>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let content = std::fs::read_to_string(internal_path.clone()).unwrap_or_default();
        Self {
            internal_path,
            editor: cx.new(|cx| TextEditor::new(state.clone(), window, cx, content, dirty)),
            state,
        }
    }

    fn write_to_path(&self, path: &std::path::Path, cx: &App) {
        let state = self.state.read(cx);
        let content = state.read(0..state.len());
        let _ = std::fs::write(path, content).inspect_err(|e| {
            log::error!("Failed to save file to {}: {}", path.display(), e);
        });
    }

    pub fn write_to_internal_path(&self, cx: &App) {
        self.write_to_path(&self.internal_path, cx);
    }

    pub fn write_to_user_path(&self, path: &std::path::Path, cx: &App) {
        self.write_to_internal_path(cx);
        self.write_to_path(path, cx);
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}
