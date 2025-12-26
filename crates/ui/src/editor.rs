use gpui::*;
use structs::rope::{Rope, TextAggregate};

use crate::editor::{backing::NaiveBackend, text_editor::TextEditor};

mod backing;
pub mod text_editor;

pub struct Editor {
    editor: Entity<text_editor::TextEditor<Rope<TextAggregate>>>,
    // editor: Entity<text_editor::TextEditor<NaiveBackend>>,
}

impl Editor {
    pub fn new(cx: &mut gpui::Context<Self>) -> Self {
        Self {
            editor: cx.new(|cx| TextEditor::new(cx))
        }
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .child("Editor")
            .child(self.editor.clone())
    }
}
