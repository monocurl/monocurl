use std::path::PathBuf;

use gpui::*;

use crate::{actions::{EpsilonBackward, EpsilonForward, NextSlide, PrevSlide, Redo, SceneEnd, SceneStart, TogglePlaying, TogglePresentationMode, ToggleTextEditor, Undo}, editor::Editor, navbar::Navbar, state::WindowState, timeline::Timeline, viewport::Viewport};


pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-z", Undo, None),
        KeyBinding::new("cmd-shift-z", Redo, None),

        KeyBinding::new("cmd-shift-l", ToggleTextEditor, None),
        KeyBinding::new("cmd-p", TogglePresentationMode, None),

        KeyBinding::new("space", TogglePlaying, None),

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
    pub path: PathBuf,
    pub view: Entity<DocumentView>,
}

pub struct DocumentView {
    path: PathBuf,
    using_text_editor: bool,
    is_presenting: bool,

    state: Entity<()>,

    navbar: Entity<Navbar>,
    editor: Entity<Editor>,
    viewport: Entity<Viewport>,
    timeline: Entity<Timeline>,

    focus_handle: FocusHandle,
}

/* action handlers */
impl DocumentView {
    fn toggle_presentation(&mut self, _ : &TogglePresentationMode, _w: &mut Window, cx: &mut Context<Self>) {
        self.is_presenting = !self.is_presenting;
        println!("Toggle Presentation");
    }

    fn toggle_playing(&mut self, _ : &TogglePlaying, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Toggle Playing");
    }

    fn toggle_text_editor(&mut self, _ : &ToggleTextEditor, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Toggle Text Editor");
    }

    fn prev_slide(&mut self, _ : &PrevSlide, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Prev Slide");
    }

    fn next_slide(&mut self, _ : &NextSlide, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Next Slide");
    }

    fn scene_start(&mut self, _ : &SceneStart, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Scene Start");
    }

    fn scene_end(&mut self, _ : &SceneEnd, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Scene End");
    }

    fn epsilon_forward(&mut self, _ : &EpsilonForward, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Epsilon Forward");
    }

    fn epsilon_backward(&mut self, _ : &EpsilonBackward, _w: &mut Window, cx: &mut Context<Self>) {
        println!("Epsilon Backward");
    }

}

impl DocumentView {
    pub fn new(path: PathBuf, window_state: WeakEntity<WindowState>, cx: &mut Context<Self>) -> Self {
        Self {
            path,
            is_presenting: false,
            using_text_editor: true,
            state: cx.new(|_| ()),
            navbar: cx.new(move |cx| Navbar::new(window_state, cx)),
            editor: cx.new(move |cx| Editor::new(cx)),
            viewport: cx.new(move |cx| Viewport::new(cx)),
            timeline: cx.new(move |cx| Timeline::new(cx)),

            focus_handle: cx.focus_handle(),
        }
    }

    fn render_presentation(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .child("Presenting")
            .text_color(white())
            .key_context("editor")
            .on_action(cx.listener(Self::toggle_presentation))
            .on_action(cx.listener(Self::toggle_playing))
            .on_action(cx.listener(Self::prev_slide))
            .on_action(cx.listener(Self::next_slide))
            .on_action(cx.listener(Self::scene_start))
            .on_action(cx.listener(Self::scene_end))
            .on_action(cx.listener(Self::epsilon_forward))
            .on_action(cx.listener(Self::epsilon_backward))
    }

    fn render_editing(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .child("Document View")
            .child(self.navbar.clone())
            .child(self.editor.clone())
            .child(self.viewport.clone())
            .child(self.timeline.clone())
            .text_color(white())
            .key_context("editor")
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
