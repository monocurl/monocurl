use gpui::*;

use crate::theme::ThemeSettings;

use super::{
    icons::{TRANSPORT_BTN_H, TRANSPORT_BTN_W, TransportIcon, transport_icon},
    metrics::{TOOLBAR_H, slide_label, slide_title_label, visual_slide_time},
    timeline_view::{BottomPanelMode, Timeline},
};

pub(super) fn render_toolbar(
    timeline: &Timeline,
    is_playing: bool,
    current_slide: usize,
    slide_count: usize,
    slide_names: Vec<Option<String>>,
    current_time: f64,
    durations: &[Option<f64>],
    cx: &mut Context<Timeline>,
) -> impl IntoElement + use<> {
    let theme = ThemeSettings::theme(cx);
    let svc = timeline.services.downgrade();

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

    let (slide_label, time_label, title_label) =
        match visual_slide_time(current_slide, current_time, durations) {
            None => (
                format!("Slide 0 / {}", slide_count),
                "0.00s".to_string(),
                None,
            ),
            Some((slide, time)) => (
                slide_label(slide, slide_count),
                format!("{:.2}s", time),
                slide_title_label(slide, &slide_names),
            ),
        };

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
                .child(slide_label),
        )
        .child(
            div()
                .text_color(theme.timeline_text)
                .text_size(px(11.0))
                .child(time_label),
        )
        .children(title_label.map(|title| {
            div()
                .text_color(theme.timeline_text)
                .text_size(px(11.0))
                .child(title)
        }));

    let panel_mode = timeline.panel_mode;
    let panel_tab = |id: &'static str, label: &'static str, mode: BottomPanelMode| {
        let is_active = panel_mode == mode;
        let this = cx.weak_entity();
        div()
            .id(id)
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .h_full()
            .px(px(12.0))
            .border_l(px(0.5))
            .border_color(theme.navbar_border)
            .bg(if is_active {
                theme.tab_active_background
            } else {
                theme.tab_background
            })
            .text_color(theme.timeline_text)
            .text_size(px(11.0))
            .cursor_pointer()
            .child(label)
            .on_click(move |_, _, cx| {
                this.update(cx, |tl, cx| tl.set_panel_mode(mode, cx)).ok();
            })
    };

    let panel_tabs = div()
        .flex()
        .flex_row()
        .items_center()
        .h_full()
        .border_t(px(0.5))
        .border_b(px(0.5))
        .border_color(theme.navbar_border)
        .child(panel_tab(
            "tl-tab-timeline",
            "Timeline",
            BottomPanelMode::Timeline,
        ))
        .child(panel_tab(
            "tl-tab-console",
            "Console",
            BottomPanelMode::Console,
        ));

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
        .child(panel_tabs)
}
