use gpui::*;

use crate::{state::WindowState, util::link_button};

pub struct Navbar {
    window_state: WeakEntity<WindowState>
}

impl Navbar {
    pub fn new(state: WeakEntity<WindowState>, _cx: &mut Context<Self>) -> Self {
        Self {
            window_state: state
        }
    }
}

impl Render for Navbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = self.window_state.upgrade().unwrap();
        let state = entity.read(cx);
        div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .w_full()
            .h(px(50.))
            .child(
                link_button("Home",  cx.listener(|this, _, _, cx| {
                    let state = this.window_state.upgrade().unwrap();
                    state.update(cx, |state, _| {
                        state.navigate_to_home();
                    })
                }))
            )
            .child(
                div()
            )
    }
}
