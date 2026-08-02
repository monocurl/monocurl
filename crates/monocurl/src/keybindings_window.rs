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
        "Timeline and playback",
        &[
            ("Play / pause", "Cmd/Ctrl+G"),
            ("Previous / next slide", "Cmd/Ctrl+, / Cmd/Ctrl+."),
            ("Scene start / end", "Cmd/Ctrl+< / Cmd/Ctrl+>"),
            ("Step backward / forward", "Cmd/Ctrl+; / Cmd/Ctrl+'"),
            ("Timeline zoom", "Cmd/Ctrl+- / Cmd/Ctrl+="),
            ("Play / pause outside the editor", "Space / Shift+Space"),
            (
                "Previous / next slide outside the editor",
                ", / . or Left / Right",
            ),
            ("Leave editor focus", "Esc"),
        ],
    ),
    (
        "Editor",
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
            ("Fold current slide", "Cmd/Ctrl+."),
            ("Indent / unindent", "Tab / Shift+Tab"),
            ("Find next / previous", "Cmd/Ctrl+G / Cmd/Ctrl+Shift+G"),
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

impl Render for KeyBindingsWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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

        div()
            .font_family(FontSet::UI)
            .size_full()
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
            )
    }
}
