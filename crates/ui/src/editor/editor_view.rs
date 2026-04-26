use std::path::PathBuf;

use gpui::{
    App, AppContext, AsyncApp, Entity, IntoElement, Render, Subscription, WeakEntity, Window,
};

use crate::{editor::text_editor::TextEditor, state::textual_state::TextualState};

const SAVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
const WATCH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);

pub struct Editor {
    path: PathBuf,
    editor: Entity<TextEditor>,
    state: Entity<TextualState>,
    dirty: Entity<bool>,
    save_dirty: Entity<bool>,
    last_disk_text: String,

    _subscriptions: Vec<Subscription>,
}

impl Editor {
    pub fn new(
        state: Entity<TextualState>,
        path: PathBuf,
        dirty: Entity<bool>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let content = std::fs::read_to_string(path.clone()).unwrap_or_default();
        let save_dirty = cx.new(|_| false);

        let editor = cx.new(|cx| {
            TextEditor::new(
                state.clone(),
                window,
                cx,
                content.clone(),
                dirty.clone(),
                save_dirty.clone(),
            )
        });

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.observe_window_activation(window, |editor, window, cx| {
            if !window.is_window_active() {
                editor.save_if_dirty(cx);
            }
        }));

        let focus_handle = editor.read(cx).editor_focus_handle();
        subscriptions.push(cx.on_focus_out(&focus_handle, window, |editor, _, _, cx| {
            editor.save_if_dirty(cx);
        }));

        subscriptions.push(cx.on_release(|editor, cx| {
            editor.save_if_dirty(cx);
        }));

        cx.spawn(async move |editor: WeakEntity<Editor>, cx: &mut AsyncApp| {
            loop {
                cx.background_executor().timer(SAVE_INTERVAL).await;
                let finished = editor
                    .update(cx, |editor, cx| {
                        editor.save_if_dirty(cx);
                    })
                    .is_err();

                if finished {
                    break;
                }
            }
        })
        .detach();

        cx.spawn_in(
            window,
            async move |editor: WeakEntity<Editor>, window_cx| {
                loop {
                    smol::Timer::after(WATCH_INTERVAL).await;
                    let finished = window_cx
                        .update(|window, cx| {
                            editor
                                .update(cx, |editor, cx| {
                                    editor.reload_from_disk_if_changed(window, cx);
                                })
                                .is_err()
                        })
                        .unwrap_or(true);

                    if finished {
                        break;
                    }
                }
            },
        )
        .detach();

        Self {
            path,
            editor,
            state,
            dirty,
            save_dirty,
            last_disk_text: content,
            _subscriptions: subscriptions,
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

    pub fn next_undo_requires_reload_confirmation(&self, cx: &App) -> bool {
        self.editor
            .read(cx)
            .next_undo_requires_reload_confirmation()
    }

    fn current_text(&self, cx: &App) -> String {
        let state = self.state.read(cx);
        state.read(0..state.len())
    }

    pub fn save_if_dirty(&mut self, cx: &mut App) {
        if *self.save_dirty.read(cx) {
            self.write_to_disk(cx);
        }
    }

    pub fn save(&mut self, cx: &mut App) {
        self.write_to_disk(cx);
    }

    fn write_to_disk(&mut self, cx: &mut App) {
        let content = self.current_text(cx);
        if let Some(parent) = self.path.parent()
            && let Err(err) = std::fs::create_dir_all(parent)
        {
            log::error!("Failed to create {}: {}", parent.display(), err);
            return;
        }
        match std::fs::write(&self.path, &content) {
            Ok(()) => {
                self.last_disk_text = content;
                self.save_dirty.update(cx, |dirty, _| *dirty = false);
                self.dirty.update(cx, |dirty, _| *dirty = false);
            }
            Err(err) => {
                log::error!("Failed to save file to {}: {}", self.path.display(), err);
            }
        }
    }

    pub fn save_to_path(&mut self, path: PathBuf, cx: &mut App) {
        self.path = path;
        self.write_to_disk(cx);
    }

    fn reload_from_disk_if_changed(&mut self, window: &mut Window, cx: &mut App) {
        let Ok(content) = std::fs::read_to_string(&self.path) else {
            return;
        };

        if content == self.last_disk_text {
            return;
        }

        if content == self.current_text(cx) {
            self.last_disk_text = content;
            self.save_dirty.update(cx, |dirty, _| *dirty = false);
            self.dirty.update(cx, |dirty, _| *dirty = false);
            return;
        }

        self.editor.update(cx, |editor, cx| {
            editor.replace_entire_text_from_external_reload(content.clone(), window, cx);
        });
        self.last_disk_text = content;
        self.save_dirty.update(cx, |dirty, _| *dirty = false);
        self.dirty.update(cx, |dirty, _| *dirty = false);
    }
}

impl Render for Editor {
    fn render(
        &mut self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        self.editor.clone()
    }
}
