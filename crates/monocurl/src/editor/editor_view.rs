use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use futures::{
    StreamExt,
    channel::mpsc::{UnboundedSender, unbounded},
};
use gpui::{
    App, AppContext, AsyncApp, Entity, IntoElement, Render, Subscription, WeakEntity, Window,
};
use notify_debouncer_full::{
    DebounceEventResult, Debouncer, RecommendedCache, new_debouncer,
    notify::{RecommendedWatcher, RecursiveMode},
};

use crate::{editor::text_editor::TextEditor, state::textual_state::TextualState};

const SAVE_INTERVAL: Duration = Duration::from_secs(5);
const WATCH_DEBOUNCE: Duration = Duration::from_millis(200);

type FileWatchDebouncer = Debouncer<RecommendedWatcher, RecommendedCache>;

pub struct Editor {
    path: PathBuf,
    editor: Entity<TextEditor>,
    state: Entity<TextualState>,
    dirty: Entity<bool>,
    save_dirty: Entity<bool>,
    last_disk_text: String,
    watch_tx: UnboundedSender<()>,
    _file_watcher: Option<FileWatchDebouncer>,

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
        let (watch_tx, mut watch_rx) = unbounded();

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
            editor.save(cx);
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
                while watch_rx.next().await.is_some() {
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

        let file_watcher = Self::watch_file(&path, &watch_tx);

        Self {
            path,
            editor,
            state,
            dirty,
            save_dirty,
            last_disk_text: content,
            watch_tx,
            _file_watcher: file_watcher,
            _subscriptions: subscriptions,
        }
    }

    fn absolute_path(path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        }
    }

    fn watch_root(path: &Path) -> PathBuf {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    fn watch_file(path: &Path, watch_tx: &UnboundedSender<()>) -> Option<FileWatchDebouncer> {
        let watched_path = Self::absolute_path(path);
        let watch_root = Self::watch_root(&watched_path);
        let watch_tx = watch_tx.clone();

        let mut debouncer =
            match new_debouncer(WATCH_DEBOUNCE, None, move |result: DebounceEventResult| {
                match result {
                    Ok(events) => {
                        if events.iter().any(|event| {
                            event.need_rescan()
                                || event
                                    .event
                                    .paths
                                    .iter()
                                    .any(|path| Self::absolute_path(path) == watched_path)
                        }) {
                            let _ = watch_tx.unbounded_send(());
                        }
                    }
                    Err(errors) => {
                        for error in errors {
                            log::warn!("File watcher error: {error}");
                        }
                    }
                }
            }) {
                Ok(debouncer) => debouncer,
                Err(error) => {
                    log::warn!(
                        "Unable to create file watcher for {}: {error}",
                        path.display()
                    );
                    return None;
                }
            };

        if let Err(error) = debouncer.watch(watch_root, RecursiveMode::NonRecursive) {
            log::warn!("Unable to watch {}: {error}", path.display());
            return None;
        }

        Some(debouncer)
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
        self._file_watcher = Self::watch_file(&self.path, &self.watch_tx);
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
