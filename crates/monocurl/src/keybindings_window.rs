use gpui::*;

use crate::theme::{FontSet, ThemeSettings};

#[derive(Default)]
struct KeyBindingsWindowHandle {
    window: Option<AnyWindowHandle>,
}

impl Global for KeyBindingsWindowHandle {}

pub struct KeyBindingsWindow {
    focus_handle: FocusHandle,
}

impl KeyBindingsWindow {
    pub fn open(cx: &mut App) {
        let window_size = size(px(560.0), px(620.0));
        if !cx.has_global::<KeyBindingsWindowHandle>() {
            cx.set_global(KeyBindingsWindowHandle::default());
        }

        let existing = cx.global::<KeyBindingsWindowHandle>().window;
        if let Some(handle) = existing
            && handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
        {
            return;
        }

        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("Key Bindings".into()),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::centered(window_size, cx)),
            window_min_size: Some(size(px(420.0), px(360.0))),
            #[cfg(target_os = "linux")]
            window_decorations: Some(WindowDecorations::Client),
            focus: true,
            ..Default::default()
        };
        if let Ok(handle) = cx.open_window(options, |_window, cx| cx.new(KeyBindingsWindow::new)) {
            cx.update_global::<KeyBindingsWindowHandle, _>(|key_bindings, _| {
                key_bindings.window = Some(handle.into());
            });
        }
    }

    fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<ThemeSettings>(|_this, cx| cx.notify())
            .detach();
        Self {
            focus_handle: cx.focus_handle(),
        }
    }
}

const BINDINGS: &[(&str, &[(&str, &str)])] = &[
    (
        "Application",
        &[
            ("Save", "Cmd/Ctrl+S"),
            ("Save As", "Cmd/Ctrl+Shift+S"),
            ("Close document", "Cmd/Ctrl+W"),
            ("Find", "Cmd/Ctrl+F"),
            ("Undo / redo", "Cmd/Ctrl+Z / Cmd/Ctrl+Shift+Z"),
            ("Copy / cut / paste", "Cmd/Ctrl+C / X / V"),
            ("Present", "Cmd/Ctrl+P"),
            ("Show presentation controls", "Cmd/Ctrl+T"),
            ("Toggle console", "Cmd/Ctrl+K"),
            ("Sync preview camera", "Cmd/Ctrl+L"),
            ("Toggle headless mode", "Cmd/Ctrl+Shift+H"),
        ],
    ),
    (
        "Timeline and playback — inside editor",
        &[
            ("Play / pause", "Cmd/Ctrl+Alt+P"),
            ("Previous / next slide", "Cmd/Ctrl+, / Cmd/Ctrl+."),
            ("Scene start / end", "Cmd/Ctrl+< / Cmd/Ctrl+>"),
            ("Step backward / forward", "Cmd/Ctrl+; / Cmd/Ctrl+'"),
            ("Timeline zoom", "Cmd/Ctrl+- / Cmd/Ctrl+="),
            ("Leave editor focus", "Esc"),
        ],
    ),
    (
        "Timeline and playback — outside editor",
        &[
            ("Play / pause", "Space / Shift+Space"),
            ("Previous / next slide", ", / . or Left / Right"),
            ("Scene start / end", "< / >"),
            ("Step backward / forward", "; / '"),
        ],
    ),
    (
        "Text editing",
        &[
            ("Move by word", "Alt+Arrow or Ctrl+Arrow"),
            ("Select by word", "Shift+Alt+Arrow or Shift+Ctrl+Arrow"),
            ("Move to line start / end", "Home / End"),
            ("Select to line start / end", "Shift+Home / Shift+End"),
            (
                "Select to line start / end (macOS)",
                "Shift+Cmd+Left / Right",
            ),
            ("Select all", "Cmd/Ctrl+A"),
            ("Toggle comment", "Cmd/Ctrl+/"),
            ("Fold current slide", "Cmd/Ctrl+Shift+."),
            ("Indent / unindent", "Tab / Shift+Tab"),
            (
                "Find next / previous",
                "Cmd/Ctrl+G / Cmd/Ctrl+Shift+G (in Find)",
            ),
        ],
    ),
    (
        "Code navigation — inside editor",
        &[
            ("Go to line (optionally :column)", "Cmd/Ctrl+G"),
            ("Next / previous diagnostic", "F8 / Shift+F8"),
            ("Next / previous slide header", "Cmd/Ctrl+Down / Up"),
            ("Go to local definition", "F12"),
        ],
    ),
];

fn platform_shortcut(shortcut: &str) -> String {
    let primary_modifier = if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    };
    let word_modifier = if cfg!(target_os = "macos") {
        "Option"
    } else {
        "Alt"
    };

    shortcut
        .replace("Cmd/Ctrl", primary_modifier)
        .replace("Alt", word_modifier)
}

impl KeyBindingsWindow {
    #[cfg(target_os = "linux")]
    fn render_linux_titlebar(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let theme = ThemeSettings::theme(cx);
        let controls = window.window_controls();

        div()
            .id("key-bindings-titlebar")
            .h(px(28.0))
            .flex_none()
            .flex()
            .items_center()
            .bg(theme.navbar_background)
            .border_b_1()
            .border_color(theme.navbar_border)
            .child(
                div()
                    .id("key-bindings-titlebar-drag")
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .px(px(10.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Key Bindings")
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        cx.stop_propagation();
                        window.start_window_move();
                    })
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        if event.click_count() == 2 && controls.maximize {
                            window.zoom_window();
                        }
                    }),
            )
            .child(
                div()
                    .id("key-bindings-window-close")
                    .w(px(42.0))
                    .h_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(18.0))
                    .cursor_pointer()
                    .hover({
                        let danger = theme.danger;
                        move |this| this.bg(danger)
                    })
                    .child("×")
                    .on_click(|_, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        window.remove_window();
                    }),
            )
            .into_any_element()
    }
}

impl Render for KeyBindingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(not(target_os = "linux"))]
        let _ = window;

        let theme = ThemeSettings::theme(cx);
        let sections = BINDINGS.iter().map(|(title, bindings)| {
            div()
                .flex()
                .flex_col()
                .gap(px(5.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(*title),
                )
                .children(bindings.iter().filter_map(|(action, shortcut)| {
                    if *action == "Select to line start / end (macOS)" && !cfg!(target_os = "macos")
                    {
                        return None;
                    }

                    let shortcut = platform_shortcut(shortcut);
                    Some(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap(px(12.0))
                            .child(div().flex_1().text_size(px(12.0)).child(*action))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child(shortcut),
                            ),
                    )
                }))
        });

        let content = div()
            .font_family(FontSet::UI)
            .flex_1()
            .min_h_0()
            .bg(theme.app_background)
            .text_color(theme.text_primary)
            .key_context("key-bindings")
            .track_focus(&self.focus_handle)
            .child(
                div()
                    .id("key-bindings-scroll")
                    .size_full()
                    .overflow_y_scroll()
                    .p(px(20.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(18.0))
                            .child(div().text_size(px(20.0)).child("Key Bindings"))
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(theme.text_muted)
                                    .child(if cfg!(target_os = "macos") {
                                        "Uses macOS keyboard shortcuts."
                                    } else {
                                        "Uses Windows/Linux keyboard shortcuts."
                                    }),
                            )
                            .children(sections),
                    ),
            );

        #[cfg(target_os = "linux")]
        {
            return div()
                .size_full()
                .flex()
                .flex_col()
                .child(self.render_linux_titlebar(window, cx))
                .child(content);
        }

        #[cfg(not(target_os = "linux"))]
        content
    }
}
