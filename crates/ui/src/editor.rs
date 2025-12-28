use std::path::PathBuf;

use gpui::*;
use structs::rope::{Rope, TextAggregate};

use crate::editor::{backing::{TextBackend}, text_editor::TextEditor};

mod backing;
pub mod text_editor;

pub struct Editor {
    internal_path: PathBuf,
    editor: Entity<text_editor::TextEditor<Rope<TextAggregate>>>,
    // editor: Entity<text_editor::TextEditor<NaiveBackend>>,
}

impl Editor {
    pub fn new(internal_path: PathBuf, dirty: Entity<bool>, cx: &mut gpui::Context<Self>) -> Self {
        let content = std::fs::read_to_string(internal_path.clone()).unwrap_or_default();
        Self {
            internal_path,
            editor: cx.new(|cx| TextEditor::new(cx, content, dirty)),
        }
    }

    fn write_to_path(&self, path: &std::path::Path, cx: &App) {
        let editor = self.editor.read(cx);
        let content = editor.backend.read(0..editor.backend.len());
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
