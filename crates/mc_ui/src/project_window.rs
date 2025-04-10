use quarve::core::{Environment, MSlock, WindowProvider};
use quarve::prelude::{text, IntoViewProvider, Signal, Size, ViewProvider, WindowMenu};
use quarve::state::FixedSignal;
use crate::Env;
use crate::menu::menu;

pub struct ProjectWindow;

impl WindowProvider for ProjectWindow {
    type Environment = Env;

    fn title(&self, env: &<Self::Environment as Environment>::Const, s: MSlock) -> impl Signal<Target=String> {
        FixedSignal::new("".into())
    }

    fn size(&self, env: &<Self::Environment as Environment>::Const, s: MSlock) -> (Size, Size, Size) {
        todo!()
    }

    fn root(&self, env: &<Self::Environment as Environment>::Const, s: MSlock) -> impl ViewProvider<Self::Environment, DownContext=()> {
        text("project")
            .into_view_provider(env, s)
    }

    fn menu(&self, env: &<Self::Environment as Environment>::Const, s: MSlock) -> WindowMenu {
        menu(env, s)
    }
}
