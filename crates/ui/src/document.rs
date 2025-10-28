use std::path::PathBuf;

use gpui::*;

use crate::{actions::{CloseActiveDocument, EpsilonBackward, EpsilonForward, NextSlide, PrevSlide, Redo, SaveActiveDocument, SceneEnd, SceneStart, TogglePlaying, TogglePresentationMode, ToggleTextEditor, Undo}, components::split_pane::Split, editor::Editor, navbar::Navbar, state::WindowState, theme::ColorSet, timeline::Timeline, viewport::Viewport};


pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-s", SaveActiveDocument, None),
        KeyBinding::new("secondary-w", CloseActiveDocument, None),

        KeyBinding::new("secondary-z", Undo, None),
        KeyBinding::new("secondary-shift-z", Redo, None),

        KeyBinding::new("secondary-shift-l", ToggleTextEditor, None),
        KeyBinding::new("secondary-p", TogglePresentationMode, None),
        KeyBinding::new("escape", TogglePresentationMode, Some("presenter")),

        KeyBinding::new("space", TogglePlaying, None),

        // all of these should also allow a combination with "secondary" to use with editor
        KeyBinding::new(",", PrevSlide, None),
        KeyBinding::new(".", NextSlide, None),

        KeyBinding::new("<", SceneStart, None),
        KeyBinding::new(">", SceneEnd, None),

        KeyBinding::new(";", EpsilonBackward, None),
        KeyBinding::new("'", EpsilonForward, None),
    ]);
}

#[derive(Clone, Debug)]
pub struct OpenDocument {
    pub internal_path: PathBuf,
    pub user_path: Option<PathBuf>,
    pub view: Entity<DocumentView>,
}

pub struct DocumentView {
    internal_path: PathBuf,
    user_path: Option<PathBuf>,

    was_fullscreen_before_presenting: bool,
    is_presenting: bool,

    state: Entity<()>,
    window_state: WeakEntity<WindowState>,

    navbar: Entity<Navbar>,
    editor: Entity<Editor>,
    viewport: Entity<Viewport>,
    timeline: Entity<Timeline>,

    focus_handle: FocusHandle,
}

/* action handlers */
impl DocumentView {
    fn toggle_presentation(&mut self, _ : &TogglePresentationMode, w: &mut Window, cx: &mut Context<Self>) { if self.is_presenting {
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

    fn toggle_playing(&mut self, _ : &TogglePlaying, _w: &mut Window, _cx: &mut Context<Self>) {
        println!("Toggle Playing");
    }

    fn toggle_text_editor(&mut self, _ : &ToggleTextEditor, _w: &mut Window, _cx: &mut Context<Self>) {
        // for now, not going to do anything?
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

    fn save_document(&mut self, _ : &SaveActiveDocument, _w: &mut Window, _cx: &mut Context<Self>) {
        println!("Saving Document Backward");

        // TODO
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
    pub fn new(internal_path: PathBuf, user_path: Option<PathBuf>, window_state: WeakEntity<WindowState>, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| Editor::new(cx));
        let viewport = cx.new(|cx| Viewport::new(cx));
        let timeline = cx.new(|cx| Timeline::new(cx));

        Self {
            internal_path,
            user_path,
            was_fullscreen_before_presenting: false,
            is_presenting: false,
            window_state: window_state.clone(),
            state: cx.new(|_| ()),
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
            .on_action(cx.listener(Self::toggle_text_editor))
            .on_action(cx.listener(Self::toggle_presentation))
            .on_action(cx.listener(Self::toggle_playing))
            .on_action(cx.listener(Self::prev_slide))
            .on_action(cx.listener(Self::next_slide))
            .on_action(cx.listener(Self::scene_start))
            .on_action(cx.listener(Self::scene_end))
            .on_action(cx.listener(Self::epsilon_forward))
            .on_action(cx.listener(Self::epsilon_backward))
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::close_document))
    }
}

impl Render for DocumentView {
    fn render(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        window.focus(&self.focus_handle);

        if self.is_presenting {
            self.render_presentation(cx).into_any_element()
        } else {
            self.render_editing(cx).into_any_element()
        }
    }
}
