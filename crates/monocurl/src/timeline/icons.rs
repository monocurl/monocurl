use gpui::*;
use structs::assets::Assets;

pub(super) const TRANSPORT_BTN_W: f32 = 18.0;
pub(super) const TRANSPORT_BTN_H: f32 = 18.0;

#[derive(Clone, Copy)]
pub(super) enum TransportIcon {
    PrevSlide,
    Play,
    Pause,
    NextSlide,
}

fn icon_resource(icon: TransportIcon) -> String {
    let name = match icon {
        TransportIcon::PrevSlide => "timeline/player-skip-back.svg",
        TransportIcon::Play => "timeline/player-play.svg",
        TransportIcon::Pause => "timeline/player-pause.svg",
        TransportIcon::NextSlide => "timeline/player-skip-forward.svg",
    };
    Assets::image(name).to_string_lossy().into_owned()
}

pub(super) fn transport_icon(icon: TransportIcon, color: Rgba) -> impl IntoElement {
    svg()
        .path(icon_resource(icon))
        .text_color(color)
        .w(px(TRANSPORT_BTN_W))
        .h(px(TRANSPORT_BTN_H))
}
