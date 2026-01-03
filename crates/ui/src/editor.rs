use std::path::PathBuf;

use gpui::*;

use crate::{document_state::DocumentState, editor::{backing::EditorBackend, text_editor::TextEditor}};

mod backing;
mod line_map;
mod wrapped_line;
pub mod text_editor;

const SAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

pub struct Editor {
    internal_path: PathBuf,
    editor: Entity<TextEditor<DocumentState>>,
    state: Entity<DocumentState>,
    internal_dirty: Entity<bool>,

    _drop_handle: Subscription,
}

impl Editor {
    pub fn new(state: Entity<DocumentState>, internal_path: PathBuf, dirty: Entity<bool>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
        let content = std::fs::read_to_string(internal_path.clone()).unwrap_or_default();
        let internal_dirty = cx.new(|_| false);

        // schedule a save every interval if internal is dirty
        cx.spawn(async move |editor: WeakEntity<Editor>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(SAVE_INTERVAL).await;
                let finished = editor
                    .update(cx, |editor, cx| {
                        if *editor.internal_dirty.read(cx) {
                            editor.write_to_internal_path(cx);
                        }
                    })
                    .is_err();

                if finished {
                    break;
                }
            }
        })
        .detach();

        let drop_handle = cx.on_release(|editor, cx| {
            if *editor.internal_dirty.read(cx) {
                editor.write_to_internal_path(cx);
            }
        });

        Self {
            internal_path,
            editor: cx.new(|cx| TextEditor::new(state.clone(), window, cx, content, dirty, internal_dirty.clone())),
            state,
            internal_dirty,
            _drop_handle: drop_handle,
        }
    }

    fn write_to_path(&self, path: &std::path::Path, cx: &App) {
        let state = self.state.read(cx);
        let content = state.read(0..state.len());
        let _ = std::fs::write(path, content).inspect_err(|e| {
            log::error!("Failed to save file to {}: {}", path.display(), e);
        });
    }

    pub fn write_to_internal_path(&self, cx: &mut App) {
        self.write_to_path(&self.internal_path, cx);
        self.internal_dirty.update(cx, |id, _| *id = false);
    }

    pub fn write_to_user_path(&self, path: &std::path::Path, cx: &mut App) {
        self.write_to_internal_path(cx);
        self.write_to_path(path, cx);
    }
}

impl Render for Editor {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        self.editor.clone()
    }
}
