use executor::time::Timestamp;
use ui_cli_shared::doc_type::DocumentType;

use super::*;

impl DocumentView {
    pub fn focus(&self, window: &mut Window) {
        window.focus(&self.focus_handle);
    }

    pub(super) fn toggle_presentation(
        &mut self,
        _: &TogglePresentationMode,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus(w);

        if self.is_presenting {
            if w.is_fullscreen() && !self.was_fullscreen_before_presenting {
                w.toggle_fullscreen();
            }
            self.is_presenting = false;
            self.viewport
                .update(cx, |vp, cx| vp.set_presenting(false, cx));
            self.services.update(cx, |services, _| {
                services.set_playback_mode(PlaybackMode::Preview);
            });
        } else {
            self.is_presenting = true;
            self.was_fullscreen_before_presenting = w.is_fullscreen();
            if !w.is_fullscreen() {
                w.toggle_fullscreen();
            }
            self.viewport
                .update(cx, |vp, cx| vp.set_presenting(true, cx));
            self.services.update(cx, |services, _| {
                services.set_playback_mode(PlaybackMode::Presentation);
            });
        }
        log::info!("Toggled presentation mode to {}", self.is_presenting);
        cx.notify();
    }

    pub(super) fn toggle_params_panel(
        &mut self,
        _: &ToggleParamsPanel,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.viewport.update(cx, |vp, cx| vp.toggle_params(cx));
    }

    pub(super) fn sync_viewport_camera(
        &mut self,
        _: &SyncViewportCamera,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.viewport
            .update(cx, |viewport, cx| viewport.sync_viewport_camera(cx));
    }

    pub(super) fn unfocus_editor(
        &mut self,
        _: &UnfocusEditor,
        w: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.focus(w);
    }

    pub(super) fn toggle_playing(
        &mut self,
        _: &TogglePlaying,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::info!("Toggled playing");
        self.services
            .update(cx, |services, _| services.toggle_play());
    }

    fn timestamp_transform(&mut self, cx: &mut Context<Self>, f: impl Fn(Timestamp) -> Timestamp) {
        self.services.update(cx, |services, cx| {
            let timestamp = services.timestamp(cx);
            let next = f(timestamp);
            services.seek_to(next);
        });
    }

    pub(super) fn prev_slide(&mut self, _: &PrevSlide, _w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Prev Slide");
        self.services.update(cx, |s, cx| s.prev_slide(cx));
    }

    pub(super) fn next_slide(&mut self, _: &NextSlide, _w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Next Slide");
        self.services.update(cx, |s, cx| s.next_slide(cx));
    }

    pub(super) fn scene_start(&mut self, _: &SceneStart, _w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Scene Start");
        self.services.update(cx, |s, _| s.scene_start());
    }

    pub(super) fn scene_end(&mut self, _: &SceneEnd, _w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Scene End");
        self.services.update(cx, |s, cx| s.scene_end(cx));
    }

    pub(super) fn epsilon_forward(
        &mut self,
        _: &EpsilonForward,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        println!("Epsilon Forward");
        self.timestamp_transform(cx, |timestamp| {
            Timestamp::new(timestamp.slide, timestamp.time + 1.0 / 30.0)
        });
    }

    pub(super) fn epsilon_backward(
        &mut self,
        _: &EpsilonBackward,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        println!("Epsilon Backward");
        self.timestamp_transform(cx, |timestamp| {
            Timestamp::new(timestamp.slide, (timestamp.time - 1.0 / 30.0).max(0.0))
        });
    }

    pub(super) fn zoom_in(&mut self, action: &ZoomIn, w: &mut Window, cx: &mut Context<Self>) {
        self.timeline.update(cx, |tl, cx| tl.zoom_in(action, w, cx));
    }

    pub(super) fn zoom_out(&mut self, action: &ZoomOut, w: &mut Window, cx: &mut Context<Self>) {
        self.timeline
            .update(cx, |tl, cx| tl.zoom_out(action, w, cx));
    }

    pub(super) fn undo(&mut self, _: &Undo, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.undo(w, cx);
        });
    }

    pub(super) fn redo(&mut self, _: &Redo, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.redo(w, cx);
        });
    }

    fn really_save(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if Some(path.clone()) != self.user_path {
            self.user_path = Some(path.clone());
        }

        self.editor.update(cx, |editor, cx| {
            editor.write_to_user_path(&path, cx);
        });

        self.window_state.upgrade().inspect(|ws| {
            ws.update(cx, |state, cx| {
                state.set_user_path(&self.internal_path, path.clone());
                self.on_imports_may_have_changed(state, cx);
            })
        });

        self.dirty.update(cx, |dirty, _| {
            *dirty = dirty_file(&self.internal_path, &self.user_path);
        })
    }

    pub(super) fn save_document_custom_path(
        &mut self,
        _: &SaveActiveDocumentCustomPath,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = self
            .user_path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or(dirs::home_dir().unwrap());
        let name = self
            .user_path
            .as_ref()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or(
                "Untitled.".to_string()
                    + self
                        .internal_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or(DocumentType::Scene.extension()),
            );
        let path = cx.prompt_for_new_path(&directory, Some(name.as_str()));
        cx.spawn(async move |this, app| {
            let Some(this) = this.upgrade() else {
                return;
            };
            let Some(path) = path.await.ok().map(|s| s.ok()).flatten().flatten() else {
                return;
            };

            log::info!("Saving document to new path {:?}", &path);

            let _ = app.update(move |app| {
                let _ = this.update(app, |this, cx| {
                    this.really_save(path, cx);
                });
            });
        })
        .detach();
    }

    pub(super) fn save_document(
        &mut self,
        _: &SaveActiveDocument,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::info!(
            "Saving document {:?} {:?}",
            &self.internal_path,
            &self.user_path
        );
        if let Some(user_path) = &self.user_path {
            self.really_save(user_path.clone(), cx);
        } else {
            self.save_document_custom_path(&SaveActiveDocumentCustomPath, w, cx);
        }
    }

    pub(super) fn export_image(
        &mut self,
        _: &ExportImage,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_export(RequestedExport::Image, cx);
    }

    pub(super) fn export_video(
        &mut self,
        _: &ExportVideo,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_export(RequestedExport::Video, cx);
    }

    pub(super) fn close_document(
        &mut self,
        _: &CloseActiveDocument,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::info!(
            "Closing document {:?} {:?}",
            &self.internal_path,
            &self.user_path
        );

        self.window_state.upgrade().map(|state| {
            state.update(cx, |state, cx| {
                state.close_tab(&self.internal_path, cx, w);
                cx.notify();
            })
        });
    }
}

impl DocumentView {
    pub fn discard_unsaved_changes(&mut self, cx: &mut App) {
        let user_path = self.user_path.clone();

        self.editor.update(cx, |editor, cx| {
            if let Some(user_path) = &user_path {
                editor.discard_unsaved_changes(user_path, cx);
            } else {
                editor.abandon_unsaved_changes(cx);
            }
        });

        self.dirty.update(cx, |dirty, _| *dirty = false);
    }

    fn get_live_ropes(
        &self,
        window_state: &WindowState,
        cx: &App,
    ) -> HashMap<PathBuf, (Rope<Attribute<LexData>>, Rope<TextAggregate>)> {
        let mut ret = HashMap::new();
        for doc in window_state.open_documents() {
            if &doc.internal_path != &self.internal_path
                && let Some(ref physical) = doc.user_path
            {
                let state = doc.view.read(cx).state.textual_state.read(cx);
                let text_rope = state.text_rope().clone();
                let lex_rope = state.lex_rope().clone();
                ret.insert(physical.clone(), (lex_rope, text_rope));
            }
        }
        ret
    }
}

impl DocumentView {
    pub fn on_imports_may_have_changed(&self, window_state: &WindowState, cx: &mut App) {
        let live_ropes = self.get_live_ropes(window_state, cx);

        self.services.update(cx, |services, _| {
            services.invalidate_dependencies(self.user_path.clone(), live_ropes);
        });
    }
}
