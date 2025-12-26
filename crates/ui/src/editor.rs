use std::path::PathBuf;

use gpui::*;
use structs::rope::{Rope, TextAggregate};

use crate::editor::{text_editor::TextEditor};

mod backing;
pub mod text_editor;

pub struct Editor {
    editor: Entity<text_editor::TextEditor<Rope<TextAggregate>>>,
    // editor: Entity<text_editor::TextEditor<NaiveBackend>>,
}

impl Editor {
    pub fn new(internal_path: PathBuf, cx: &mut gpui::Context<Self>) -> Self {
        Self {
            editor: cx.new(|cx| TextEditor::new(cx))
        }
    }

    pub fn write_to_user_path(&self, path: &std::path::Path) {
        // self.editor.guser_et_mut().save_to_path(path);
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .child("Editor")
            .child(self.editor.clone())
    }
}
