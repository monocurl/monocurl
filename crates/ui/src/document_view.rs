use std::{collections::HashMap, path::PathBuf};

use executor::time::Timestamp;
use gpui::*;
use structs::rope::{Attribute, Rope, TextAggregate};
use ui_cli_shared::doc_type::DocumentType;

use crate::{actions::{CloseActiveDocument, EpsilonBackward, EpsilonForward, NextSlide, PrevSlide, Redo, SaveActiveDocument, SaveActiveDocumentCustomPath, SceneEnd, SceneStart, TogglePlaying, TogglePresentationMode, Undo, UnfocusEditor, ZoomIn, ZoomOut}, components::split_pane::Split, editor::editor_view::Editor, navbar_view::Navbar, services::{PlaybackMode, ServiceManager}, state::{document_state::DocumentState, textual_state::LexData, window_state::{ActiveScreen, WindowState}}, theme::ColorSet, timeline::timeline_view::Timeline, viewport::viewport_view::Viewport};


pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-s", SaveActiveDocument, None),
        KeyBinding::new("secondary-shift-s", SaveActiveDocumentCustomPath, None),
        KeyBinding::new("secondary-w", CloseActiveDocument, None),

        KeyBinding::new("secondary-z", Undo, None),
        KeyBinding::new("secondary-shift-z", Redo, None),

        KeyBinding::new("secondary-p", TogglePresentationMode, None),
        KeyBinding::new("escape", TogglePresentationMode, Some("presenter")),
        KeyBinding::new("escape", UnfocusEditor, Some("!presenter")),

        KeyBinding::new("space shift-space", TogglePlaying, Some("!editor")),
        KeyBinding::new("secondary-shift-space,", TogglePlaying, None),

        KeyBinding::new(",", PrevSlide, Some("!editor")),
        KeyBinding::new("secondary-,", PrevSlide, None),
        KeyBinding::new(".", NextSlide, Some("!editor")),
        KeyBinding::new("secondary-.", NextSlide, None),

        KeyBinding::new("<", SceneStart, Some("!editor")),
        KeyBinding::new("secondary-<", SceneStart, None),
        KeyBinding::new(">", SceneEnd, Some("!editor")),
        KeyBinding::new("secondary->", SceneEnd, None),

        KeyBinding::new(";", EpsilonBackward, Some("!editor")),
        KeyBinding::new("secondary-;", EpsilonBackward, None),
        KeyBinding::new("'", EpsilonForward, Some("!editor")),
        KeyBinding::new("secondary-'", EpsilonForward, None),

        KeyBinding::new("secondary-=", ZoomIn, None),
        KeyBinding::new("secondary--", ZoomOut, None),
    ]);
}

#[derive(Clone, Debug)]
pub struct OpenDocument {
    pub internal_path: PathBuf,
    pub user_path: Option<PathBuf>,
    pub view: Entity<DocumentView>,
    pub dirty: Entity<bool>,
}

pub struct DocumentView {
    internal_path: PathBuf,
    user_path: Option<PathBuf>,

    was_fullscreen_before_presenting: bool,
    is_presenting: bool,

    dirty: Entity<bool>,
    state: DocumentState,
    services: Entity<ServiceManager>,
    window_state: WeakEntity<WindowState>,

    navbar: Entity<Navbar>,
    editor: Entity<Editor>,
    viewport: Entity<Viewport>,
    timeline: Entity<Timeline>,

    focus_handle: FocusHandle,
}

fn dirty_file(internal: &PathBuf, user: &Option<PathBuf>) -> bool {
    let Some(user) = user else {
        return true;
    };

    let content_ip = std::fs::read_to_string(internal);
    let content_up = std::fs::read_to_string(user);

    match (content_ip, content_up) {
        (Ok(ci), Ok(cu)) => ci != cu,
        _ => true,
    }
}

/* action handlers */
impl DocumentView {

    fn toggle_presentation(&mut self, _ : &TogglePresentationMode, w: &mut Window, cx: &mut Context<Self>) {
        w.focus(&self.focus_handle);

        if self.is_presenting {
            if w.is_fullscreen() && !self.was_fullscreen_before_presenting {
                w.toggle_fullscreen();
            }
            self.is_presenting = false;
            self.services.update(cx, |services, _| {
                services.set_playback_mode(PlaybackMode::Presentation);
            });
        }
        else {
            self.was_fullscreen_before_presenting = w.is_fullscreen();
            if !w.is_fullscreen() {
                w.toggle_fullscreen();
            }
            self.services.update(cx, |services, _| {
                services.set_playback_mode(PlaybackMode::Preview);
            });
        }
        log::info!("Toggled presentation mode to {}", self.is_presenting);
        cx.notify();
    }

    fn unfocus_editor(&mut self, _ : &UnfocusEditor, w: &mut Window, _cx: &mut Context<Self>) {
        w.focus(&self.focus_handle);
    }

    fn toggle_playing(&mut self, _ : &TogglePlaying, _w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Toggled playing");
        self.services.update(cx, |services, _| services.toggle_play());
    }

    fn timestamp_transform(&mut self, cx: &mut Context<Self>, f: impl Fn(Timestamp) -> Timestamp) {
        self.services.update(cx, |services, cx| {
            let timestamp = services.timestamp(cx);
            let next = f(timestamp);
            services.seek_to(next);
        });
    }

    fn prev_slide(&mut self, _: &PrevSlide, _w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Prev Slide");
        self.services.update(cx, |s, cx| s.prev_slide(cx));
    }

    fn next_slide(&mut self, _: &NextSlide, _w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Next Slide");
        self.services.update(cx, |s, cx| s.next_slide(cx));
    }

    fn scene_start(&mut self, _: &SceneStart, _w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Scene Start");
        self.services.update(cx, |s, _| s.scene_start());
    }

    fn scene_end(&mut self, _: &SceneEnd, _w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Scene End");
        self.services.update(cx, |s, cx| s.scene_end(cx));
    }

    fn epsilon_forward(&mut self, _ : &EpsilonForward, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Epsilon Forward");
        self.timestamp_transform(cx, |timestamp| {
            Timestamp::new(timestamp.slide, timestamp.time + 1e-3)
        });
    }

    fn epsilon_backward(&mut self, _ : &EpsilonBackward, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Epsilon Backward");
        self.timestamp_transform(cx, |timestamp| {
            Timestamp::new(timestamp.slide, (timestamp.time - 1e-3).max(0.0))
        });
    }

    fn zoom_in(&mut self, action: &ZoomIn, w: &mut Window, cx: &mut Context<Self>) {
        self.timeline.update(cx, |tl, cx| tl.zoom_in(action, w, cx));
    }

    fn zoom_out(&mut self, action: &ZoomOut, w: &mut Window, cx: &mut Context<Self>) {
        self.timeline.update(cx, |tl, cx| tl.zoom_out(action, w, cx));
    }

    fn undo(&mut self, _ : &Undo, w: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            editor.undo(w, cx);
        });
    }

    fn redo(&mut self, _ : &Redo, w: &mut Window, cx: &mut Context<Self>) {
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

    fn save_document_custom_path(&mut self, _ : &SaveActiveDocumentCustomPath, _w: &mut Window, cx: &mut Context<Self>) {
        let directory =
            self.user_path
                .as_ref().and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or(dirs::home_dir().unwrap());
        let name =
            self.user_path
                .as_ref().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or("Untitled.".to_string() + self.internal_path.extension().and_then(|e| e.to_str()).unwrap_or(DocumentType::Scene.extension()));
        let path = cx.prompt_for_new_path(&directory, Some(name.as_str()));
        cx.spawn(async move |this, app| {
            let Some(this) = this.upgrade() else {
                return;
            };
            let Some(path) = path.await
                .ok().map(|s| s.ok())
                .flatten().flatten() else {
                return;
            };

            log::info!("Saving document to new path {:?}", &path);

            let _ = app.update(move |app| {
                let _ = this.update(app, |this, cx| {
                    this.really_save(path, cx);
                });
            });
        }).detach();
    }

    fn save_document(&mut self, _ : &SaveActiveDocument, w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Saving document {:?} {:?}", &self.internal_path, &self.user_path);
        if let Some(user_path) = &self.user_path {
            self.really_save(user_path.clone(), cx);
        } else {
            self.save_document_custom_path(&SaveActiveDocumentCustomPath, w, cx);
        }
    }

    fn close_document(&mut self, _ : &CloseActiveDocument, w: &mut Window, cx: &mut Context<Self>) {
        log::info!("Closing document {:?} {:?}", &self.internal_path, &self.user_path);

        self.window_state.upgrade().map(|state| {
            state.update(cx, |state, cx| {
                state.close_tab(&self.internal_path, cx, w);
                cx.notify();
            })
        });
    }
}

impl DocumentView {
    fn get_live_ropes(&self, window_state: &WindowState, cx: &App) -> HashMap<PathBuf, (Rope<Attribute<LexData>>, Rope<TextAggregate>)> {
        let mut ret = HashMap::new();
        for doc in window_state.open_documents() {
            if &doc.internal_path != &self.internal_path && let Some(ref physical) = doc.user_path {
                let state = doc.view.read(cx).state.textual_state.read(cx);
                let text_rope = state.text_rope().clone();
                let lex_rope = state.lex_rope().clone();
                ret.insert(physical.clone(), (lex_rope, text_rope));
            }
        }
        return ret;
    }
}

impl DocumentView {
    // initialize once window_state can be read
    // basically, the listener for active screen will not be called for the first screen
    // so this gets around that
    pub fn on_imports_may_have_changed(&self, window_state: &WindowState, cx: &mut App) {
        let live_ropes = self.get_live_ropes(window_state, cx);

        self.services.update(cx, |services, _| {
            services.invalidate_dependencies(self.user_path.clone(), live_ropes);
        });
    }

    pub fn new(internal_path: PathBuf, user_path: Option<PathBuf>,  window_state: WeakEntity<WindowState>, dirty: Entity<bool>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // note that text editor is responsible for initially bootstrapping the content
        let state = DocumentState::new(cx);
        // connects everything together
        let services = cx.new(|cx| ServiceManager::new(state.textual_state.clone(), state.execution_state.clone(), cx));

        let editor = cx.new(|cx| Editor::new(state.textual_state.clone(), internal_path.clone(), dirty.clone(), window, cx));
        let viewport = cx.new(|cx| Viewport::new(cx));
        let timeline = cx.new(|cx| Timeline::new(services.clone(), cx));

        // whenever we switch over to here, we recompute the live dependencies cache
        let virtual_path = internal_path.clone();
        let window_state_up = window_state.upgrade().unwrap();
        cx.observe(&window_state_up, move |dv, ws, cx| {
            ws.update(cx, |window_state, cx| {
                if let ActiveScreen::Document(doc) = &window_state.screen {
                    if doc.internal_path == virtual_path {
                        dv.on_imports_may_have_changed(window_state, cx);
                    }
                }
            });
        }).detach();

        dirty.update(cx, |dirty, _| {
            *dirty = dirty_file(&internal_path, &user_path);
        });

        Self {
            internal_path,
            user_path,
            was_fullscreen_before_presenting: false,
            is_presenting: false,
            dirty,
            window_state: window_state.clone(),
            state: state,
            services: services,
            navbar: cx.new(move |cx| Navbar::new(window_state, cx)),
            editor: editor.clone(),
            viewport: viewport.clone(),
            timeline,
            focus_handle: cx.focus_handle(),
        }
    }

    fn render_presentation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .child("Presenting")
            .text_color(white())
            .key_context("document presenter")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::toggle_presentation))
            .on_action(cx.listener(Self::toggle_playing))
            .on_action(cx.listener(Self::prev_slide))
            .on_action(cx.listener(Self::next_slide))
            .on_action(cx.listener(Self::scene_start))
            .on_action(cx.listener(Self::scene_end))
            .on_action(cx.listener(Self::epsilon_forward))
            .on_action(cx.listener(Self::epsilon_backward))
    }

    fn viewport_timeline(&self) -> Split {
        Split::new(
            Axis::Vertical,
            self.viewport.clone().into_any_element(),
            self.timeline.clone().into_any_element(),
        )
    }

    fn render_editing(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .child(self.navbar.clone())
            .child(
                Split::new(
                    Axis::Horizontal,
                    self.editor.clone().into_any_element(),
                    self.viewport_timeline().into_any_element()
                )
                .default_flex(0.5)
            )
            .text_color(white())
            .bg(ColorSet::DARK_GRAY)
            .size_full()
            .key_context("document")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::toggle_presentation))
            .on_action(cx.listener(Self::toggle_playing))
            .on_action(cx.listener(Self::unfocus_editor))
            .on_action(cx.listener(Self::prev_slide))
            .on_action(cx.listener(Self::next_slide))
            .on_action(cx.listener(Self::scene_start))
            .on_action(cx.listener(Self::scene_end))
            .on_action(cx.listener(Self::epsilon_forward))
            .on_action(cx.listener(Self::epsilon_backward))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::save_document_custom_path))
            .on_action(cx.listener(Self::close_document))
            .on_action(cx.listener(Self::zoom_in))
            .on_action(cx.listener(Self::zoom_out))
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if window.focused(cx).is_none() {
            window.focus(&self.focus_handle);
        }

        if self.is_presenting {
            self.render_presentation(cx).into_any_element()
        } else {
            self.render_editing(cx).into_any_element()
        }
    }
}
