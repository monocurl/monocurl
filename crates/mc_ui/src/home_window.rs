use quarve::prelude::*;
use quarve::core::{Environment, MSlock, WindowProvider};
use quarve::state::FixedSignal;
use crate::Env;
use crate::home::home;
use crate::menu::menu;

pub struct HomeWindow;

impl WindowProvider for HomeWindow {
    type Environment = Env;

    fn title(&self, _env: &<Self::Environment as Environment>::Const, _s: MSlock) -> impl Signal<Target=String> {
        FixedSignal::new("Monocurl".into())
    }

    fn size(&self, _env: &<Self::Environment as Environment>::Const, _s: MSlock) -> (Size, Size, Size) {
        (
            Size::new(600.0, 500.0),
            Size::new(900.0, 700.0),
            Size::new(10000.0, 5000.0)
        )
    }

    fn root(&self, env: &<Self::Environment as Environment>::Const, s: MSlock) -> impl ViewProvider<Self::Environment, DownContext=()> {
        home()
            .into_view_provider(env, s)
    }

    fn menu(&self, env: &<Self::Environment as Environment>::Const, s: MSlock) -> WindowMenu {
        menu(env, s)
    }
}
