use gpui::{prelude::FluentBuilder, *};

use crate::{document::OpenDocument, state::{ActiveScreen, WindowState}, theme::ColorSet, util::link_button};

pub struct Navbar {
    window_state: WeakEntity<WindowState>,
    document_list: Entity<DocumentList>
}

struct DocumentList {
    window_state: WeakEntity<WindowState>,
}

impl DocumentList {

    fn render_tab(&self, doc: &OpenDocument, is_active: bool, cx: &Context<Self>) -> impl IntoElement {
        let filename = doc.user_path
            .as_ref()
            .and_then(|f| f.file_name())
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or(
                "Untitled".to_string()
                 + &doc.internal_path
                    .extension()
                    .map(|e| ".".to_string() + &e.to_string_lossy().to_string())
                    .unwrap_or_default()
            );

        let dirty = *doc.dirty.read(cx);

        let up = doc.user_path.clone();
        let ip0 = doc.internal_path.clone();
        let ip = doc.internal_path.clone();

        let col = if is_active { ColorSet::SUPER_LIGHT_GRAY } else { ColorSet::TOOLBAR_GRAY };

        div()
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .pl_3()
            .pr_1()
            .h_full()
            .border_r(px(0.5))
            .border_color(ColorSet::PURPLE)
            .h(px(30.))
            .bg(col)
            .child(
                div()
                    .size_1()
                    .rounded_full()
                    .bg(if dirty { ColorSet::PURPLE } else { col })
            )
            .child(filename)
            .text_color(black())
            .id(SharedString::new(doc.internal_path.to_string_lossy().to_string()))
            .child(
                div()
                    .size_3()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_sm()
                    .hover(|style| style.bg(ColorSet::LIGHT_GRAY))
                    .child("×")
                    .id("close-button")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let state = this.window_state.upgrade().unwrap();
                        let ip = ip0.clone();
                        state.update(cx, move |wstate, cx| {
                            wstate.close_tab(&ip, cx, window);
                        })
                    }))
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                let state = this.window_state.upgrade().unwrap();
                let statec = state.clone();
                let up = up.clone();
                let ip = ip.clone();
                state.update(cx, move |wstate, cx| {
                    wstate.navigate_to(up.clone(), ip.clone(), statec, cx);
                })
            }))
            .cursor_pointer()
    }
}

impl Render for DocumentList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = self.window_state.upgrade().unwrap();
        let state = entity.read(cx);
        let active = match state.screen {
            ActiveScreen::Home => None,
            ActiveScreen::Document(ref open_document) => {
                Some(open_document.internal_path.clone())
            }
        };

        div()
            .flex()
            .flex_row()
            .flex_shrink()
            .h_full()
            .id("document-list")
            .border(px(1.0))
            .children(
                state
                    .open_documents()
                    .map(|doc|  self.render_tab(doc, Some(&doc.internal_path) == active.as_ref(), cx))
            )
            .text_size(px(12.0))
            .pr_32()
            .overflow_x_scroll()
            .track_scroll(state.navbar_scroll())
    }
}

impl Navbar {
    pub fn new(state: WeakEntity<WindowState>, cx: &mut Context<Self>) -> Self {
        let s = state.clone();
        Self {
            window_state: state,
            document_list: cx.new(|_| DocumentList {
                window_state: s,
            })
        }
    }
}

impl Render for Navbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = self.window_state.upgrade().unwrap();
        let state = entity.read(cx);

        let home_active = matches!(state.screen, ActiveScreen::Home);

        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(30.))
            .bg(ColorSet::LIGHT_GRAY)
            .border_color(ColorSet::SUPER_DARK_GRAY)
            .border_b(px(0.5))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .child(
                        div()
                            .child(
                                link_button("Home",  cx.listener(|this, _, _, cx| {
                                    let state = this.window_state.upgrade().unwrap();
                                    state.update(cx, |state, _| {
                                        state.navigate_to_home();
                                    })
                                }))
                            )
                            .px_3()
                    )
                    .bg(if home_active { ColorSet::SUPER_LIGHT_GRAY } else { ColorSet::TOOLBAR_GRAY })
                    .h(px(30.))
                    .border_r(px(0.5))
                    .border_color(ColorSet::PURPLE)
            )
            .child(
                div()
                    .w_full()
                    .child(
                        self.document_list.clone()
                    )
            )
    }
}
