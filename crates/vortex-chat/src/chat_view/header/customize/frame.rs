//! An item in customize mode, the drop line that shows where a dragged one
//! goes, the menu that moves one without a drag, and the bars' names.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, SharedString, TestSupportExt, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Sizable as _,
        button::Button,
        menu::{DropdownMenu as _, PopupMenuItem},
    },
    div, px,
};

use crate::{
    chat_view::ChatView,
    look::{ChatLook, Item, Spot},
};

/// An item being dragged to another place.
#[derive(Clone, Copy)]
pub(super) struct DraggedItem(pub(super) Item);

impl Render for DraggedItem {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_0p5()
            .rounded(cx.theme().radius)
            .bg(cx.theme().secondary)
            .text_sm()
            .child(self.0.label())
    }
}

/// `item` in customize mode: dragged by its whole face, which takes the
/// clicks its button would, with a menu that moves or hides it.
pub(super) fn editable(
    item: Item,
    spot: Spot,
    element: AnyElement,
    cx: &Context<ChatView>,
) -> AnyElement {
    let name = item.name();
    let theme = cx.theme();
    div()
        .id(SharedString::from(format!("chat-item-{name}")))
        .relative()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(element)
        .child(
            div()
                .id(SharedString::from(format!("chat-item-drag-{name}")))
                .test_support()
                .absolute()
                .inset_0()
                .occlude()
                .rounded(theme.radius)
                .cursor_grab()
                .tooltip(move |window, cx| {
                    let tip = format!("Drag {} to move it", item.label());
                    gpui_kit::component::tooltip::Tooltip::new(tip).build(window, cx)
                })
                .on_drag(DraggedItem(item), |dragged, _, _, cx| cx.new(|_| *dragged))
                // Here, not on the item: the cover hides what is behind it
                // from the pointer, drops too. The line on its left is where
                // the dragged item goes, before this one.
                .drag_over::<DraggedItem>(|this, _, _, cx| {
                    this.border_l_4()
                        .border_color(cx.theme().primary)
                        .bg(cx.theme().primary.opacity(0.12))
                })
                .on_drop(cx.listener(move |_, dragged: &DraggedItem, _, cx| {
                    cx.stop_propagation();
                    let look = ChatLook::current(cx).moved(dragged.0, spot, Some(item));
                    ChatView::change_look(look, cx);
                })),
        )
        .child(
            div()
                .absolute()
                .top(px(-7.))
                .right(px(-7.))
                .child(item_menu(item, cx)),
        )
        .into_any_element()
}

/// Moves `item` without a drag: along its row, to another bar, or away.
fn item_menu(item: Item, cx: &Context<ChatView>) -> impl IntoElement + use<> {
    let view = cx.entity().downgrade();
    let look = ChatLook::current(cx).clone();
    let place = look.place(item);
    let last = place.is_some_and(|(spot, at)| at + 1 == look.items(spot).len());
    let first = place.is_none_or(|(_, at)| at == 0);
    Button::new(SharedString::from(format!(
        "chat-item-menu-{}",
        item.name()
    )))
    .xsmall()
    .icon(Lucide::Ellipsis)
    .tooltip(format!("Move or Hide {}", item.label()))
    .dropdown_menu_with_anchor(gpui_kit::Anchor::TopRight, move |menu, _, _| {
        let change = |label: &'static str, change: fn(&ChatLook, Item) -> ChatLook| {
            let view = view.clone();
            PopupMenuItem::new(label).on_click(move |_, _, cx| {
                let look = change(ChatLook::current(cx), item);
                let _ = view.update(cx, |_, cx| ChatView::change_look(look, cx));
            })
        };
        let mut menu = menu
            .item(change("Move Left", |look, item| look.shifted(item, false)).disabled(first))
            .item(change("Move Right", |look, item| look.shifted(item, true)).disabled(last))
            .separator();
        for spot in Spot::ALL {
            let view = view.clone();
            let here = place.is_some_and(|(at, _)| at == spot);
            let label: SharedString = format!("Move to {}", spot.label()).into();
            menu = menu.item(
                PopupMenuItem::new(label)
                    .checked(here)
                    .on_click(move |_, _, cx| {
                        let look = ChatLook::current(cx).moved(item, spot, None);
                        let _ = view.update(cx, |_, cx| ChatView::change_look(look, cx));
                    }),
            );
        }
        menu.separator().item(change("Hide", ChatLook::without))
    })
}

/// The bar's name, at its start in customize mode.
pub(super) fn caption(spot: Spot, cx: &Context<ChatView>) -> impl IntoElement + use<> {
    div()
        .flex_none()
        .px_1()
        .text_xs()
        .text_color(cx.theme().primary)
        .child(spot.label())
}
