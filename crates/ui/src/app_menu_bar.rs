use gpui::*;

use crate::theme::{FontSet, ThemeSettings};

pub struct AppMenuBar {
    open_menu: Option<SharedString>,
}

impl AppMenuBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.observe_global::<ThemeSettings>(|_, cx| cx.notify())
            .detach();

        Self { open_menu: None }
    }

    fn visible_menus(cx: &mut Context<Self>) -> Vec<OwnedMenu> {
        cx.get_menus()
            .unwrap_or_default()
            .into_iter()
            .filter(|menu| menu.name != "Monocurl")
            .filter_map(|mut menu| {
                menu.items = Self::sanitize_menu_items(menu.items);
                if menu.items.is_empty() {
                    None
                } else {
                    Some(menu)
                }
            })
            .collect()
    }

    fn sanitize_menu_items(items: Vec<OwnedMenuItem>) -> Vec<OwnedMenuItem> {
        let mut cleaned = Vec::new();
        let mut last_was_separator = true;

        for item in items {
            match item {
                OwnedMenuItem::Separator => {
                    if !last_was_separator {
                        cleaned.push(OwnedMenuItem::Separator);
                        last_was_separator = true;
                    }
                }
                OwnedMenuItem::SystemMenu(_) => {}
                OwnedMenuItem::Submenu(mut submenu) => {
                    submenu.items = Self::sanitize_menu_items(submenu.items);
                    if !submenu.items.is_empty() {
                        cleaned.push(OwnedMenuItem::Submenu(submenu));
                        last_was_separator = false;
                    }
                }
                item => {
                    cleaned.push(item);
                    last_was_separator = false;
                }
            }
        }

        if matches!(cleaned.last(), Some(OwnedMenuItem::Separator)) {
            cleaned.pop();
        }

        cleaned
    }

    fn render_menu_item(
        &self,
        item: OwnedMenuItem,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let theme = ThemeSettings::theme(cx);

        match item {
            OwnedMenuItem::Action {
                name,
                action,
                checked,
                ..
            } => {
                let label = if checked { format!("* {name}") } else { name };

                div()
                    .id(format!("menu-item-{label}"))
                    .w_full()
                    .px_3()
                    .py_1()
                    .text_size(px(12.0))
                    .text_color(theme.text_primary)
                    .cursor_pointer()
                    .hover({
                        let hover = theme.row_hover_overlay;
                        move |this| this.bg(hover)
                    })
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open_menu = None;
                        window.prevent_default();
                        cx.stop_propagation();
                        window.dispatch_action(action.boxed_clone(), cx);
                        cx.notify();
                    }))
                    .into_any_element()
            }
            OwnedMenuItem::Submenu(submenu) => div()
                .id(format!("menu-submenu-{}", submenu.name))
                .w_full()
                .px_3()
                .py_1()
                .text_size(px(12.0))
                .text_color(theme.text_muted)
                .child(submenu.name)
                .into_any_element(),
            OwnedMenuItem::Separator => div()
                .id("menu-separator")
                .h(px(1.0))
                .mx_2()
                .my_1()
                .bg(theme.navbar_border)
                .into_any_element(),
            OwnedMenuItem::SystemMenu(_) => div().into_any_element(),
        }
    }

    fn render_menu(&self, menu: OwnedMenu, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let theme = ThemeSettings::theme(cx);
        let is_open = self.open_menu.as_ref() == Some(&menu.name);
        let label = menu.name.clone();

        div()
            .id(format!("menu-{label}"))
            .relative()
            .h_full()
            .flex()
            .items_center()
            .child(
                div()
                    .id(format!("menu-trigger-{label}"))
                    .px_3()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_size(px(12.0))
                    .text_color(theme.text_primary)
                    .cursor_pointer()
                    .bg(if is_open {
                        theme.tab_active_background
                    } else {
                        theme.navbar_background
                    })
                    .hover({
                        let hover = theme.tab_active_background;
                        move |this| this.bg(hover)
                    })
                    .child(label.clone())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.open_menu = if this.open_menu.as_ref() == Some(&label) {
                            None
                        } else {
                            Some(label.clone())
                        };
                        cx.notify();
                    })),
            )
            .on_hover(cx.listener(move |this, hover_enter, _window, cx| {
                if *hover_enter {
                    this.open_menu = Some(menu.name.clone());
                    cx.notify();
                }
            }))
    }

    fn render_open_menu(
        &self,
        menus: Vec<OwnedMenu>,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let theme = ThemeSettings::theme(cx);
        let open_menu = self.open_menu.as_ref()?;
        let menu = menus.into_iter().find(|menu| &menu.name == open_menu)?;

        Some(
            div()
                .id(format!("open-menu-{}", menu.name))
                .w(px(190.0))
                .py_1()
                .border_1()
                .border_color(theme.navbar_border)
                .bg(theme.tab_background)
                .children(
                    menu.items
                        .into_iter()
                        .map(|item| self.render_menu_item(item, cx)),
                )
                .into_any_element(),
        )
    }
}

impl Render for AppMenuBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeSettings::theme(cx);
        let menus = Self::visible_menus(cx);
        let open_menu = self.render_open_menu(menus.clone(), cx);

        div()
            .id("app-menu-bar")
            .flex()
            .flex_col()
            .w_full()
            .flex_none()
            .on_hover(cx.listener(|this, hover_enter: &bool, _window, cx| {
                if !*hover_enter && this.open_menu.is_some() {
                    this.open_menu = None;
                    cx.notify();
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .w_full()
                    .h(px(24.0))
                    .flex_none()
                    .bg(theme.navbar_background)
                    .border_b(px(0.5))
                    .border_color(theme.navbar_border)
                    .font_family(FontSet::UI)
                    .children(menus.into_iter().map(|menu| self.render_menu(menu, cx))),
            )
            .children(open_menu)
    }
}
