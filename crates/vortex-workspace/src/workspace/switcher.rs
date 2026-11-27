//! The folder's name in the title bar, which opens a menu to move about:
//! the window's sessions, the recent folders, opening a folder and a new
//! window.

use std::path::Path;

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, Context, SharedString, WeakEntity,
    component::{
        ActiveTheme as _, Icon,
        button::{Button, ButtonVariants as _},
        h_flex,
        menu::{DropdownMenu as _, PopupMenuItem},
    },
    div, img, px,
};
use vortex_chat::scale::{ScaledButton as _, ScaledIcon as _};

use super::Workspace;
use crate::{chat_panel::ChatPanel, project::folder_name};

const MENU_WIDTH: f32 = 280.;
/// Recent folders past this many are left to the projects page.
const RECENT_SHOWN: usize = 8;
/// Sessions and folders past this many make the menu scroll.
const SCROLLED_PAST: usize = 14;

/// A session as the menu lists it.
struct Shown {
    panel: WeakEntity<ChatPanel>,
    label: SharedString,
    active: bool,
}

impl Workspace {
    pub(super) fn render_switcher(&self, name: String, cx: &Context<Self>) -> impl IntoElement {
        let sessions: Vec<Shown> = self
            .chats
            .iter()
            .map(|tab| {
                let panel = tab.panel.read(cx);
                let title = panel.chat().read(cx).title().unwrap_or("New session");
                Shown {
                    panel: tab.panel.downgrade(),
                    label: format!("{} · {title}", panel.project().read(cx).name()).into(),
                    active: self.active_chat.as_ref() == Some(&tab.panel),
                }
            })
            .collect();
        let mut recent = self.recent.folders();
        recent.truncate(RECENT_SHOWN);
        let view = cx.entity().downgrade();
        Button::new("window-switcher")
            .ghost()
            .control()
            .tab_stop(false)
            .tooltip("Switch Session or Folder")
            .child(
                h_flex()
                    .gap_1()
                    .min_w_0()
                    .child(
                        // Its light ink on a dark theme, its dark ink on a light one.
                        img(if cx.theme().is_dark() {
                            "brand/mark-dark.svg"
                        } else {
                            "brand/mark-light.svg"
                        })
                        .flex_none()
                        // The drawing fills three quarters of its box.
                        .size(px(22.)),
                    )
                    .child(div().min_w_0().truncate().child(name))
                    .child(Icon::new(gpui_kit::assets::IconName::ChevronDown).in_row()),
            )
            .dropdown_menu_with_anchor(Anchor::TopLeft, move |menu, _, _| {
                let menu = menu.min_w(px(MENU_WIDTH));
                // Scrolled only when it cannot fit: a menu that fits, near
                // the limit, would otherwise scroll by a few pixels.
                let menu = if sessions.len() + recent.len() > SCROLLED_PAST {
                    menu.max_h(px(560.)).scrollable(true)
                } else {
                    menu
                };
                let menu = sessions.iter().fold(menu.label("Sessions"), |menu, shown| {
                    menu.item(session_item(shown, &view))
                });
                let menu = recent
                    .iter()
                    .fold(menu.separator().label("Recent"), |menu, folder| {
                        menu.item(folder_item(folder, &view))
                    });
                menu.separator()
                    .item(command_item("Open Folder…", &view, |this, window, cx| {
                        this.prompt_open_folder(window, cx);
                    }))
                    .item(PopupMenuItem::new("New Window").on_click(|_, _, cx| {
                        crate::windows::open_another(cx);
                    }))
            })
    }
}

fn session_item(shown: &Shown, view: &WeakEntity<Workspace>) -> PopupMenuItem {
    let (panel, view) = (shown.panel.clone(), view.clone());
    PopupMenuItem::new(shown.label.clone())
        .checked(shown.active)
        .on_click(move |_, window, cx| {
            let Some(panel) = panel.upgrade() else {
                return;
            };
            let _ = view.update(cx, |this, cx| this.show_chat_panel(&panel, window, cx));
        })
}

fn folder_item(folder: &Path, view: &WeakEntity<Workspace>) -> PopupMenuItem {
    let (path, view) = (folder.to_path_buf(), view.clone());
    PopupMenuItem::new(folder_name(folder))
        .icon(Icon::new(gpui_kit::assets::IconName::FolderOpen))
        .on_click(move |_, window, cx| {
            let _ = view.update(cx, |this, cx| this.open_folder(&path, window, cx));
        })
}

fn command_item(
    label: &'static str,
    view: &WeakEntity<Workspace>,
    act: fn(&mut Workspace, &mut gpui_kit::Window, &mut Context<Workspace>),
) -> PopupMenuItem {
    let view = view.clone();
    PopupMenuItem::new(label).on_click(move |_, window, cx| {
        let _ = view.update(cx, |this, cx| act(this, window, cx));
    })
}
