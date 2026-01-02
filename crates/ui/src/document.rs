use std::path::{PathBuf};

use gpui::*;
use server::doc_type::DocumentType;

use crate::{actions::{CloseActiveDocument, EpsilonBackward, EpsilonForward, NextSlide, PrevSlide, Redo, SaveActiveDocument, SaveActiveDocumentCustomPath, SceneEnd, SceneStart, TogglePlaying, TogglePresentationMode, Undo, UnfocusEditor}, components::split_pane::Split, document_state::DocumentState, editor::Editor, navbar::Navbar, state::WindowState, theme::ColorSet, timeline::Timeline, viewport::Viewport};


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
    _state: Entity<DocumentState>,
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
            self.is_presenting = false
        }
        else {
            self.was_fullscreen_before_presenting = w.is_fullscreen();
            if !w.is_fullscreen() {
                w.toggle_fullscreen();
            }
            self.is_presenting = true
        }
        log::info!("Toggled presentation mode to {}", self.is_presenting);
        cx.notify();
    }

    fn unfocus_editor(&mut self, _ : &UnfocusEditor, w: &mut Window, _cx: &mut Context<Self>) {
        w.focus(&self.focus_handle);
    }

    fn toggle_playing(&mut self, _ : &TogglePlaying, _w: &mut Window, _cx: &mut Context<Self>) {
        println!("Toggle Playing");
    }

    fn prev_slide(&mut self, _ : &PrevSlide, _w: &mut Window, _cx: &mut Context<Self>) {
        println!("Prev Slide");
    }

    fn next_slide(&mut self, _ : &NextSlide, _w: &mut Window, _cx: &mut Context<Self>) {
        println!("Next Slide");
    }

    fn scene_start(&mut self, _ : &SceneStart, _w: &mut Window, _cx: &mut Context<Self>) {
        println!("Scene Start");
    }

    fn scene_end(&mut self, _ : &SceneEnd, _w: &mut Window, _cx: &mut Context<Self>) {
        println!("Scene End");
    }

    fn epsilon_forward(&mut self, _ : &EpsilonForward, _w: &mut Window, _cx: &mut Context<Self>) {
        println!("Epsilon Forward");
    }

    fn epsilon_backward(&mut self, _ : &EpsilonBackward, _w: &mut Window, _cx: &mut Context<Self>) {
        println!("Epsilon Backward");
    }

    fn really_save(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if Some(path.clone()) != self.user_path {
            self.user_path = Some(path.clone());
        }

        self.editor.read(cx).write_to_user_path(&path, cx);

        self.window_state.upgrade().inspect(|ws| {
            ws.update(cx, |state, _cx| {
                state.set_user_path(&self.internal_path, path.clone());
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
    pub fn new(internal_path: PathBuf, user_path: Option<PathBuf>, window_state: WeakEntity<WindowState>, dirty: Entity<bool>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        // note that text editor is responsible for initially bootstrapping the content
        let state = cx.new(|_| DocumentState::default());

        let editor = cx.new(|cx| Editor::new(state.clone(), internal_path.clone(), dirty.clone(), window, cx));
        let viewport = cx.new(|cx| Viewport::new(cx));
        let timeline = cx.new(|cx| Timeline::new(cx));

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
            _state: state,
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
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::save_document_custom_path))
            .on_action(cx.listener(Self::close_document))
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
