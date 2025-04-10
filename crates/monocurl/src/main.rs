use quarve::core::{ApplicationProvider, MSlock};
use mc_ui::home_window::HomeWindow;

struct Application;

impl ApplicationProvider for Application {
    fn name(&self) -> &str {
        "com.enigmadux.monocurl"
    }

    fn will_spawn(&self, app: &quarve::core::Application, s: MSlock) {
        app.spawn_window(HomeWindow, s)
    }
}


fn main() {
    quarve::core::launch(Application);
}
