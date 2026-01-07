use std::path::PathBuf;

use gpui::{App, AppContext, AsyncApp, Entity, IntoElement, Render, Subscription, WeakEntity, Window};

use crate::{state::textual_state::TextualState, editor::text_editor::TextEditor};

const SAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

pub struct Editor {
    internal_path: PathBuf,
    editor: Entity<TextEditor>,
    state: Entity<TextualState>,
    internal_dirty: Entity<bool>,

    _drop_handle: Subscription,
}

impl Editor {
    pub fn new(state: Entity<TextualState>, internal_path: PathBuf, dirty: Entity<bool>, window: &mut Window, cx: &mut gpui::Context<Self>) -> Self {
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

    pub fn undo(&mut self, window: &mut Window, cx: &mut App) {
        self.editor.update(cx, |editor, cx| {
            editor.perform_undo(window, cx);
        });
    }

    pub fn redo(&mut self, window: &mut Window, cx: &mut App) {
        self.editor.update(cx, |editor, cx| {
            editor.perform_redo(window, cx);
        });
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
