//! The settings page over the panels and chats, and the gear at the
//! activity bar's foot that opens it and its neighbours.

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, AnyElement, Context, Focusable as _, Pixels, WeakEntity, Window,
    assets::IconName as Lucide,
    component::{
        Icon, Sizable as _, Size, WindowExt as _,
        button::{Button, ButtonVariants as _},
        menu::{DropdownMenu as _, PopupMenuItem},
        notification::Notification,
    },
    px,
};
use serde_json::Value;
use vortex_settings::{Scope, ThemeMode};

use super::Workspace;
use crate::settings_page::{SettingsPage, SettingsPageEvent};

impl Workspace {
    /// Shows the settings page, or closes it when it is shown.
    pub fn toggle_settings_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_page.is_some() {
            self.close_settings_page(window, cx);
        } else {
            self.open_settings_page(window, cx);
        }
    }

    pub fn open_settings_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some((page, _)) = &self.settings_page {
            page.focus_handle(cx).focus(window, cx);
            return;
        }
        let layers = match vortex_settings::Layers::read(self.shown_root.as_deref(), Value::Null) {
            Ok(layers) => layers,
            Err(error) => {
                let error = format!("Cannot read the settings: {error:#}");
                window.push_notification(Notification::error(error), cx);
                return;
            }
        };
        self.shortcuts_page = None;
        let page = cx.new(|cx| SettingsPage::new(layers, cx));
        let events = cx.subscribe_in(&page, window, |this, _, event, window, cx| match event {
            SettingsPageEvent::Changed => this.reload_config(window, cx),
            SettingsPageEvent::Close => this.close_settings_page(window, cx),
            SettingsPageEvent::OpenSettingsFile => {
                this.close_settings_page(window, cx);
                this.open_user_settings(window, cx);
            }
            SettingsPageEvent::OpenCommitSettings => {
                this.commit_settings(Scope::Global, window, cx);
            }
        });
        page.focus_handle(cx).focus(window, cx);
        self.settings_page = Some((page, events));
        cx.notify();
    }

    pub(super) fn close_settings_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_page.take().is_some() {
            self.focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    pub(super) fn reread_settings_page(&mut self, cx: &mut Context<Self>) {
        let project = self.shown_root.clone();
        if let Some((page, _)) = &self.settings_page {
            page.update(cx, |page, cx| page.reread(project.as_deref(), cx));
        }
    }

    /// The settings or the shortcuts page, whichever is open.
    pub(super) fn render_settings_page(&self) -> Option<AnyElement> {
        if let Some((page, _)) = &self.shortcuts_page {
            return Some(page.clone().into_any_element());
        }
        let (page, _) = self.settings_page.as_ref()?;
        Some(page.clone().into_any_element())
    }

    /// The gear, sized like the activity bar's buttons.
    pub(super) fn render_gear(
        button: Pixels,
        icon: Pixels,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let view = cx.entity().downgrade();
        let mode = crate::config::settings(cx).theme.mode;
        Button::new("activity-settings")
            .ghost()
            .size(button)
            .p_0()
            .child(Icon::new(Lucide::Settings).with_size(Size::Size(icon)))
            .tooltip("Manage")
            .dropdown_menu_with_anchor(Anchor::BottomLeft, move |menu, _, _| {
                let menu = menu.min_w(px(220.));
                let menu = menu
                    .item(item("Settings", &view, Workspace::open_settings_page))
                    .item(item("Settings File", &view, Workspace::open_user_settings))
                    .item(item(
                        "Keyboard Shortcuts",
                        &view,
                        Workspace::open_shortcuts_page,
                    ))
                    .separator()
                    .label("Color Theme");
                let menu = [
                    (ThemeMode::System, "system", "System"),
                    (ThemeMode::Light, "light", "Light"),
                    (ThemeMode::Dark, "dark", "Dark"),
                ]
                .into_iter()
                .fold(menu, |menu, (shown, value, label)| {
                    menu.item(theme_item(label, value, shown == mode, &view))
                });
                menu.separator().item(item(
                    "Check for Updates…",
                    &view,
                    Workspace::check_for_updates,
                ))
            })
    }

    /// Sets `theme.mode` for every project, as the settings page would.
    fn set_theme_mode(&mut self, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        let key = ["theme", "mode"];
        let written = vortex_settings::Layers::read(None, Value::Null)
            .and_then(|mut layers| layers.set(Scope::Global, &key, Some(value.into())));
        match written {
            Ok(()) => self.reload_config(window, cx),
            Err(error) => {
                let error = format!("Cannot change the theme: {error:#}");
                window.push_notification(Notification::error(error), cx);
            }
        }
    }
}

fn item(
    label: &'static str,
    view: &WeakEntity<Workspace>,
    act: fn(&mut Workspace, &mut Window, &mut Context<Workspace>),
) -> PopupMenuItem {
    let view = view.clone();
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let _ = view.update(cx, |this, cx| act(this, window, cx));
    })
}

fn theme_item(
    label: &'static str,
    value: &'static str,
    checked: bool,
    view: &WeakEntity<Workspace>,
) -> PopupMenuItem {
    let view = view.clone();
    PopupMenuItem::new(label)
        .checked(checked)
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| this.set_theme_mode(value, window, cx));
        })
}
