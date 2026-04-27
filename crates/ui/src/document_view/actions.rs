use executor::time::Timestamp;
use ui_cli_shared::doc_type::DocumentType;

use super::*;

const EPSILON_STEP: f64 = 1.0 / 30.0;

fn visible_slide_for_global(slide: usize) -> Option<usize> {
    slide.checked_sub(1)
}

fn scene_boundary_slide(timestamp: Timestamp, slide_count: usize) -> Option<usize> {
    (slide_count > 0).then_some(timestamp.slide.min(slide_count))
}

fn cached_visible_slide_duration(
    visible_slide: usize,
    slide_durations: &[Option<f64>],
    minimum_slide_durations: &[Option<f64>],
) -> Option<f64> {
    slide_durations
        .get(visible_slide)
        .copied()
        .flatten()
        .or_else(|| {
            minimum_slide_durations
                .get(visible_slide)
                .copied()
                .flatten()
        })
}

fn resolved_global_slide_duration(
    slide: usize,
    slide_durations: &[Option<f64>],
    minimum_slide_durations: &[Option<f64>],
) -> f64 {
    visible_slide_for_global(slide)
        .and_then(|visible_slide| {
            cached_visible_slide_duration(visible_slide, slide_durations, minimum_slide_durations)
        })
        .unwrap_or_default()
}

fn epsilon_forward_target(timestamp: Timestamp, slide_count: usize) -> Timestamp {
    let Some(slide) = scene_boundary_slide(timestamp, slide_count) else {
        return Timestamp::default();
    };
    if timestamp.time.is_infinite() && slide < slide_count {
        Timestamp::new(slide + 1, EPSILON_STEP)
    } else if timestamp.time.is_infinite() {
        Timestamp::at_end_of_slide(slide)
    } else {
        Timestamp::new(slide, timestamp.time + EPSILON_STEP)
    }
}

fn step_backward_on_slide(slide: usize, time: f64) -> Timestamp {
    if time <= EPSILON_STEP {
        Timestamp::new(slide, 0.0)
    } else {
        Timestamp::new(slide, time - EPSILON_STEP)
    }
}

fn epsilon_backward_target(
    timestamp: Timestamp,
    slide_count: usize,
    slide_durations: &[Option<f64>],
    minimum_slide_durations: &[Option<f64>],
) -> Timestamp {
    let Some(slide) = scene_boundary_slide(timestamp, slide_count) else {
        return Timestamp::default();
    };

    if timestamp.time.is_infinite() {
        if slide == 0 {
            return Timestamp::default();
        }

        let duration =
            resolved_global_slide_duration(slide, slide_durations, minimum_slide_durations);
        return step_backward_on_slide(slide, duration);
    }

    if timestamp.time > 0.0 {
        return step_backward_on_slide(slide, timestamp.time);
    }

    if slide == 0 {
        return Timestamp::default();
    }

    let prev_slide = slide - 1;
    if prev_slide == 0 {
        Timestamp::default()
    } else {
        let prev_duration =
            resolved_global_slide_duration(prev_slide, slide_durations, minimum_slide_durations);
        step_backward_on_slide(prev_slide, prev_duration)
    }
}

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

    pub(super) fn play_or_show_pause_hint(
        &mut self,
        _: &PlayOrShowPauseHint,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_playing = self
            .services
            .read(cx)
            .execution_state()
            .read(cx)
            .is_playing();
        if self.is_presenting && is_playing {
            self.viewport
                .update(cx, |viewport, cx| viewport.show_pause_hint(cx));
            return;
        }

        self.viewport
            .update(cx, |viewport, cx| viewport.clear_pause_hint(cx));
        log::info!("Toggled playing");
        self.services
            .update(cx, |services, _| services.toggle_play());
    }

    pub(super) fn toggle_params_panel(
        &mut self,
        _: &ToggleParamsPanel,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.viewport.update(cx, |vp, cx| vp.toggle_params(cx));
    }

    pub(super) fn toggle_timeline_console(
        &mut self,
        _: &ToggleTimelineConsole,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.timeline
            .update(cx, |timeline, cx| timeline.toggle_panel_mode(cx));
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
        cx: &mut Context<Self>,
    ) {
        self.editor.update(cx, |editor, cx| {
            editor.save_if_dirty(cx);
        });
        self.focus(w);
    }

    pub(super) fn toggle_headless(
        &mut self,
        _: &ToggleHeadlessMode,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_headless = !self.is_headless;
        if self.is_headless {
            self.focus(w);
        }
        cx.notify();
    }

    pub(super) fn toggle_playing(
        &mut self,
        _: &TogglePlaying,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.viewport
            .update(cx, |viewport, cx| viewport.clear_pause_hint(cx));
        log::info!("Toggled playing");
        self.services
            .update(cx, |services, _| services.toggle_play());
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
        log::info!("Epsilon Forward");
        self.services.update(cx, |services, cx| {
            let next = {
                let execution = services.execution_state().read(cx);
                epsilon_forward_target(execution.current_timestamp, execution.slide_count)
            };
            services.seek_to(next);
        });
    }

    pub(super) fn epsilon_backward(
        &mut self,
        _: &EpsilonBackward,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::info!("Epsilon Backward");
        self.services.update(cx, |services, cx| {
            let next = {
                let execution = services.execution_state().read(cx);
                epsilon_backward_target(
                    execution.current_timestamp,
                    execution.slide_count,
                    &execution.slide_durations,
                    &execution.minimum_slide_durations,
                )
            };
            services.seek_to(next);
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
        if self
            .editor
            .read(cx)
            .next_undo_requires_reload_confirmation(cx)
        {
            let confirm = w.prompt(
                PromptLevel::Warning,
                "Undo Reload From Disk?",
                Some("This will restore the editor buffer that existed before the external file change."),
                &[
                    PromptButton::Cancel("Cancel".into()),
                    PromptButton::Ok("Undo Reload".into()),
                ],
                cx,
            );
            let this = cx.weak_entity();
            w.spawn(cx, async move |window_cx| {
                if confirm.await != Ok(1) {
                    return;
                }
                let _ = window_cx.update(move |window, cx| {
                    let _ = this.update(cx, |this, cx| {
                        this.editor.update(cx, |editor, cx| {
                            editor.undo(window, cx);
                        });
                    });
                });
            })
            .detach();
            return;
        }

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
        let old_path = self.path.clone();
        self.path = path.clone();

        self.editor.update(cx, |editor, cx| {
            editor.save_to_path(path.clone(), cx);
        });

        self.window_state.upgrade().inspect(|ws| {
            ws.update(cx, |state, cx| {
                state.set_document_path(&old_path, path.clone());
                self.on_imports_may_have_changed(state, cx);
            })
        });
    }

    pub(super) fn save_document_custom_path(
        &mut self,
        _: &SaveActiveDocumentCustomPath,
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let directory = self
            .path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or(dirs::home_dir().unwrap());
        let name = self
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or(
                "Untitled.".to_string()
                    + self
                        .path
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
        _w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::info!("Saving document {:?}", &self.path);
        self.really_save(self.path.clone(), cx);
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
        log::info!("Closing document {:?}", &self.path);

        let Some(state) = self.window_state.upgrade() else {
            return;
        };
        let path = self.path.clone();

        w.defer(cx, move |w, cx| {
            state.update(cx, |state, cx| {
                state.close_tab(&path, cx, w);
                cx.notify();
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::{EPSILON_STEP, epsilon_backward_target, epsilon_forward_target};
    use executor::time::Timestamp;

    #[test]
    fn epsilon_backward_wraps_to_previous_slide_end() {
        let target = epsilon_backward_target(
            Timestamp::new(4, 0.0),
            5,
            &[Some(1.0), Some(2.0), Some(4.0), None, None],
            &[None; 5],
        );

        assert_eq!(target.slide, 3);
        assert!((target.time - (4.0 - EPSILON_STEP)).abs() < 1e-12);
    }

    #[test]
    fn epsilon_forward_from_slide_end_wraps_to_next_slide_start() {
        let target = epsilon_forward_target(Timestamp::at_end_of_slide(3), 5);

        assert_eq!(target, Timestamp::new(4, EPSILON_STEP));
    }

    #[test]
    fn epsilon_forward_at_cached_duration_stays_in_slide() {
        let target = epsilon_forward_target(Timestamp::new(3, 4.0), 5);

        assert_eq!(target, Timestamp::new(3, 4.0 + EPSILON_STEP));
    }

    #[test]
    fn epsilon_forward_inside_slide_stays_in_slide() {
        let target = epsilon_forward_target(Timestamp::new(3, 1.25), 5);

        assert_eq!(target, Timestamp::new(3, 1.25 + EPSILON_STEP));
    }
}

impl DocumentView {
    pub fn save_before_close(&mut self, cx: &mut App) {
        self.editor.update(cx, |editor, cx| {
            editor.save(cx);
        });
    }

    fn get_live_ropes(
        &self,
        window_state: &WindowState,
        cx: &App,
    ) -> HashMap<PathBuf, (Rope<Attribute<LexData>>, Rope<TextAggregate>)> {
        let mut ret = HashMap::new();
        for doc in window_state.open_documents() {
            if doc.path != self.path {
                let state = doc.view.read(cx).state.textual_state.read(cx);
                let text_rope = state.text_rope().clone();
                let lex_rope = state.lex_rope().clone();
                ret.insert(doc.path.clone(), (lex_rope, text_rope));
            }
        }
        ret
    }
}

impl DocumentView {
    pub fn on_imports_may_have_changed(&self, window_state: &WindowState, cx: &mut App) {
        let live_ropes = self.get_live_ropes(window_state, cx);

        self.services.update(cx, |services, _| {
            services.invalidate_dependencies(self.path.clone(), live_ropes);
        });
    }
}
