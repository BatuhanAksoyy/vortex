//! Menus the browser leaves to the system, drawn by Vortex over the page.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClipboardItem, Context, DismissEvent, Entity, Focusable, Pixels, Point,
    Subscription, WeakEntity, Window, anchored,
    component::menu::{PopupMenu, PopupMenuItem},
    deferred, point, px,
};
use vortex_browser::{MenuTarget, Select};

use super::Browser;

/// Between a select and its menu.
const GAP: f32 = 2.;

pub(super) struct OpenMenu {
    menu: Entity<PopupMenu>,
    position: Point<Pixels>,
    _dismissed: Subscription,
}

type Act = fn(&mut Browser, &mut Window, &mut Context<Browser>);

impl Browser {
    pub(super) fn open_select(
        &mut self,
        select: &Select,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let from = if select.from_pointer {
            self.last_press
        } else {
            self.bounds.origin
        };
        let position = from + point(px(select.x), px(select.y + GAP));
        let this = cx.weak_entity();
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            let menu = menu.min_w(px(select.width)).scrollable(true);
            choices(menu, select, &this)
        });
        self.open_menu(menu, position, window, cx);
    }

    pub(super) fn open_context_menu(
        &mut self,
        target: &MenuTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        let (back, forward) = (self.history.can_go_back(), self.history.can_go_forward());
        let item = |label: &'static str, act: Act| {
            let this = this.clone();
            PopupMenuItem::new(label).on_click(move |_, window, cx| {
                let _ = this.update(cx, |this, cx| act(this, window, cx));
            })
        };
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            let menu = addresses(menu, target, &this);
            let menu = menu
                .item(item("Cut", |this, _, cx| this.copy(true, cx)).disabled(!target.editable))
                .item(item("Copy", |this, _, cx| this.copy(false, cx)))
                .item(
                    item("Paste", |this, _, cx| this.paste_clipboard(cx))
                        .disabled(!target.editable),
                )
                .separator();
            menu.item(item("Back", |this, _, _| this.go_back()).disabled(!back))
                .item(item("Forward", |this, _, _| this.go_forward()).disabled(!forward))
                .item(item("Reload", |this, _, _| this.reload()))
                .item(item("Open in Your Browser", |this, _, cx| {
                    this.open_outside(cx);
                }))
        });
        self.open_menu(menu, self.last_press, window, cx);
    }

    fn choose(&self, select: &Select, index: i64) {
        if let Some(page) = self.page() {
            page.choose(select, index);
        }
    }

    fn open_menu(
        &mut self,
        menu: Entity<PopupMenu>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        menu.focus_handle(cx).focus(window, cx);
        let dismissed = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            this.menu = None;
            this.focus.focus(window, cx);
            cx.notify();
        });
        self.menu = Some(OpenMenu {
            menu,
            position,
            _dismissed: dismissed,
        });
        cx.notify();
    }

    pub(super) fn render_menu(&self) -> Option<AnyElement> {
        let open = self.menu.as_ref()?;
        let place = anchored()
            .position(open.position)
            .snap_to_window_with_margin(px(8.))
            .child(open.menu.clone());
        Some(deferred(place).with_priority(1).into_any_element())
    }
}

fn choices(mut menu: PopupMenu, select: &Select, this: &WeakEntity<Browser>) -> PopupMenu {
    let mut group = None;
    for choice in &select.choices {
        if choice.group != group {
            group.clone_from(&choice.group);
            menu = menu.label(group.clone().unwrap_or_default());
        }
        let (this, select, index) = (this.clone(), select.clone(), choice.index);
        let item = PopupMenuItem::new(choice.label.clone())
            .checked(choice.index == select.selected)
            .disabled(choice.disabled)
            .on_click(move |_, _, cx| {
                let _ = this.update(cx, |this, _| this.choose(&select, index));
            });
        menu = menu.item(item);
    }
    menu
}

/// What can be done with the link or picture under the pointer.
fn addresses(mut menu: PopupMenu, target: &MenuTarget, this: &WeakEntity<Browser>) -> PopupMenu {
    let copy = |label: &'static str, text: &str| {
        let text = text.to_owned();
        PopupMenuItem::new(label).on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
        })
    };
    if let Some(link) = &target.link {
        let (this, url) = (this.clone(), link.clone());
        let open = PopupMenuItem::new("Open Link").on_click(move |_, window, cx| {
            let url = url.clone();
            let _ = this.update(cx, |this, cx| this.go_to_link(&url, window, cx));
        });
        menu = menu.item(open).item(copy("Copy Link Address", link));
    }
    if let Some(image) = &target.image {
        menu = menu.item(copy("Copy Image Address", image));
    }
    if target.link.is_some() || target.image.is_some() {
        menu = menu.separator();
    }
    menu
}
