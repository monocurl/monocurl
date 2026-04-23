use std::{
    sync::mpsc::{self, TryRecvError},
    time::Duration,
};

use exporter::{
    ExportKind as SceneExportKind, ExportOutcome, ExportProgress, ExportRequest, ExportSettings,
};

use super::*;

const EXPORT_POLL_INTERVAL: Duration = Duration::from_millis(50);

enum ExportThreadMessage {
    Progress(ExportProgress),
    Finished(Result<ExportOutcome, String>),
}

impl DocumentView {
    fn current_document_text(&self, cx: &App) -> String {
        let state = self.state.textual_state.read(cx);
        state.read(0..state.len())
    }

    fn live_open_document_texts(&self, cx: &App) -> HashMap<PathBuf, String> {
        let Some(window_state) = self.window_state.upgrade() else {
            return HashMap::new();
        };
        let window_state = window_state.read(cx);
        let mut documents = HashMap::new();

        for doc in window_state.open_documents() {
            if doc.internal_path == self.internal_path {
                continue;
            }
            let Some(user_path) = &doc.user_path else {
                continue;
            };
            let text = {
                let doc_view = doc.view.read(cx);
                let state = doc_view.state.textual_state.read(cx);
                state.read(0..state.len())
            };
            documents.insert(user_path.clone(), text);
        }

        documents
    }

    fn export_directory(&self) -> PathBuf {
        self.user_path
            .as_ref()
            .and_then(|path| path.parent().map(|path| path.to_path_buf()))
            .unwrap_or(dirs::home_dir().unwrap())
    }

    fn export_filename(&self, kind: RequestedExport) -> String {
        let stem = self
            .user_path
            .as_ref()
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string());
        format!("{stem}.{}", kind.extension())
    }

    fn normalize_export_path(mut path: PathBuf, kind: RequestedExport) -> PathBuf {
        path.set_extension(kind.extension());
        path
    }

    pub(super) fn request_export(&mut self, kind: RequestedExport, cx: &mut Context<Self>) {
        if self.export_overlay.running {
            return;
        }

        self.clear_export_state(cx);

        let directory = self.export_directory();
        let name = self.export_filename(kind);
        let path = cx.prompt_for_new_path(&directory, Some(name.as_str()));
        cx.spawn(async move |this, app| {
            let Some(this) = this.upgrade() else {
                return;
            };
            let Some(path) = path.await.ok().map(|path| path.ok()).flatten().flatten() else {
                return;
            };
            let path = Self::normalize_export_path(path, kind);

            let _ = app.update(move |app| {
                let _ = this.update(app, |this, cx| {
                    this.start_export(kind, path, cx);
                });
            });
        })
        .detach();
    }

    fn start_export(
        &mut self,
        kind: RequestedExport,
        output_path: PathBuf,
        cx: &mut Context<Self>,
    ) {
        if self.export_overlay.running {
            return;
        }

        let current_timestamp = self.state.execution_state.read(cx).current_timestamp;
        let request = ExportRequest {
            root_text: self.current_document_text(cx),
            root_user_path: self.user_path.clone(),
            open_documents: self.live_open_document_texts(cx),
            output_path,
            kind: match kind {
                RequestedExport::Image => SceneExportKind::Image {
                    timestamp: current_timestamp,
                },
                RequestedExport::Video => SceneExportKind::Video,
            },
            settings: ExportSettings::default(),
        };

        let (tx, rx) = mpsc::channel();
        self.export_overlay = ExportOverlayState::start(kind);
        cx.notify();

        std::thread::spawn(move || {
            let progress_tx = tx.clone();
            let result = exporter::export_scene(request, move |progress| {
                let _ = progress_tx.send(ExportThreadMessage::Progress(progress));
            })
            .map_err(|error| error.to_string());
            let _ = tx.send(ExportThreadMessage::Finished(result));
        });

        self.export_poll_task = Some(cx.spawn(async move |this, app| {
            loop {
                loop {
                    match rx.try_recv() {
                        Ok(message) => {
                            let mut finished = false;
                            let updated = this
                                .update(app, |this, cx| {
                                    finished = this.apply_export_message(message, cx);
                                })
                                .is_ok();
                            if !updated || finished {
                                return;
                            }
                        }
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            let _ = this.update(app, |this, cx| {
                                this.finish_export_with_error(
                                    "export worker disconnected unexpectedly".into(),
                                    cx,
                                );
                            });
                            return;
                        }
                    }
                }

                app.background_executor().timer(EXPORT_POLL_INTERVAL).await;
            }
        }));
    }

    fn apply_export_message(
        &mut self,
        message: ExportThreadMessage,
        cx: &mut Context<Self>,
    ) -> bool {
        match message {
            ExportThreadMessage::Progress(progress) => {
                self.export_overlay.message = progress.message;
                self.export_overlay.completed = progress.completed;
                self.export_overlay.total = progress.total;
                cx.notify();
                false
            }
            ExportThreadMessage::Finished(Ok(outcome)) => {
                log::info!(
                    "Exported {} to {}",
                    outcome.frames_written,
                    outcome.output_path.display()
                );
                self.finish_export_with_success(outcome, cx);
                true
            }
            ExportThreadMessage::Finished(Err(error)) => {
                self.finish_export_with_error(error, cx);
                true
            }
        }
    }

    fn finish_export_with_error(&mut self, error: String, cx: &mut Context<Self>) {
        self.export_overlay.running = false;
        self.export_overlay.error = Some(error);
        self.export_overlay.output_path = None;
        self.export_poll_task = None;
        cx.notify();
    }

    fn finish_export_with_success(&mut self, outcome: ExportOutcome, cx: &mut Context<Self>) {
        self.export_overlay.running = false;
        self.export_overlay.error = None;
        self.export_overlay.output_path = Some(outcome.output_path.clone());
        self.export_overlay.completed =
            self.export_overlay.total.max(self.export_overlay.completed);
        self.export_overlay.message = format!("Saved to {}", outcome.output_path.display());
        self.export_poll_task = None;
        cx.notify();
    }

    pub(super) fn open_export_output(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.export_overlay.output_path.clone() else {
            return;
        };

        match open::that(&path) {
            Ok(()) => {
                self.export_overlay.message = format!("Opened {}", path.display());
            }
            Err(error) => {
                self.export_overlay.message = format!(
                    "Saved to {}, but automatic open failed: {}",
                    path.display(),
                    error
                );
            }
        }

        cx.notify();
    }

    pub(super) fn clear_export_state(&mut self, cx: &mut Context<Self>) {
        self.export_overlay = ExportOverlayState::default();
        self.export_poll_task = None;
        cx.notify();
    }
}
