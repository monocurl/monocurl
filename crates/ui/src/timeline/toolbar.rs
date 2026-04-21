use gpui::*;

use crate::{
    actions::{ZoomIn, ZoomOut},
    theme::ThemeSettings,
};

use super::{
    icons::{TRANSPORT_BTN_H, TRANSPORT_BTN_W, TransportIcon, transport_icon},
    metrics::{TOOLBAR_H, ZOOM_LEVELS},
    timeline_view::Timeline,
};

pub(super) fn render_toolbar(
    timeline: &Timeline,
    is_playing: bool,
    current_slide: usize,
    slide_count: usize,
    current_time: f64,
    cx: &mut Context<Timeline>,
) -> impl IntoElement {
    let theme = ThemeSettings::theme(cx);
    let svc = timeline.services.downgrade();
    let this = cx.weak_entity();
    let zoom_pct = ZOOM_LEVELS[timeline.zoom_idx];

    let nav_btn = |id: &'static str, icon: TransportIcon| {
        div()
            .id(id)
            .w(px(TRANSPORT_BTN_W))
            .h(px(TRANSPORT_BTN_H))
            .p(px(2.0))
            .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.opacity(0.6))
                .child(transport_icon(icon, theme.timeline_transport_color))
    };

    let transport_controls = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(2.0))
        .child({
            let svc = svc.clone();
            nav_btn("tl-prev-slide", TransportIcon::PrevSlide).on_click(move |_, _, cx| {
                svc.update(cx, |s, cx| s.prev_slide(cx)).ok();
            })
        })
        .child(
            div()
                .id("tl-play")
                .w(px(TRANSPORT_BTN_W))
                .h(px(TRANSPORT_BTN_H))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|s| s.opacity(0.6))
                .child(transport_icon(
                    if is_playing {
                        TransportIcon::Pause
                    } else {
                        TransportIcon::Play
                    },
                    theme.timeline_transport_color,
                ))
                .on_click({
                    let svc = svc.clone();
                    move |_, _, cx| {
                        svc.update(cx, |s, _| s.toggle_play()).ok();
                    }
                }),
        )
        .child({
            let svc = svc.clone();
            nav_btn("tl-next-slide", TransportIcon::NextSlide).on_click(move |_, _, cx| {
                svc.update(cx, |s, cx| s.next_slide(cx)).ok();
            })
        });

    let center_group = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(7.0))
        .child(transport_controls)
        .child(
            div()
                .text_color(theme.timeline_text)
                .text_size(px(11.0))
                .child(format!(
                    "Slide {} / {}",
                    (current_slide + 1).min(slide_count),
                    slide_count
                )),
        )
        .child(
            div()
                .text_color(theme.timeline_text)
                .text_size(px(11.0))
                .child(format!("{:.2}s", current_time)),
        );

    let zoom_group = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(7.0))
        .child(
            div()
                .id("tl-zoom-out")
                .w(px(20.0))
                .h(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.timeline_transport_color)
                .text_size(px(14.0))
                .cursor_pointer()
                .hover(|s| s.opacity(0.6))
                .child("−")
                .on_click({
                    let this = this.clone();
                    move |_, w, cx| {
                        this.update(cx, |tl, cx| tl.zoom_out(&ZoomOut, w, cx)).ok();
                    }
                }),
        )
        .child(
            div()
                .text_color(theme.timeline_subtext)
                .text_size(px(10.0))
                .child(format!("{}%", zoom_pct))
                .w(px(36.0))
                .flex()
                .justify_center(),
        )
        .child(
            div()
                .id("tl-zoom-in")
                .w(px(20.0))
                .h(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .text_color(theme.timeline_transport_color)
                .text_size(px(14.0))
                .cursor_pointer()
                .hover(|s| s.opacity(0.6))
                .child("+")
                .on_click({
                    move |_, w, cx| {
                        this.update(cx, |tl, cx| tl.zoom_in(&ZoomIn, w, cx)).ok();
                    }
                }),
        );

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(7.0))
        .px(px(8.0))
        .h(px(TOOLBAR_H))
        .bg(theme.timeline_toolbar_background)
        .border_b(px(1.0))
        .border_color(theme.split_divider)
        .pl(px(24.0))
        .child(center_group)
        .child(div().flex_1())
        .child(zoom_group)
}
