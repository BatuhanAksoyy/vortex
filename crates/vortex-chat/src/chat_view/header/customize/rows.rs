//! Customize mode's list of where each item is: a row per bar and one for
//! the hidden items, with every item as a chip of the same size. Dragging
//! there is short and plain to aim, where the bars themselves are far apart
//! and their items of every size.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, SharedString, TestSupportExt,
    component::{ActiveTheme, h_flex, v_flex},
    div, px,
};

use super::frame::DraggedItem;
use crate::{
    chat_view::ChatView,
    look::{ChatLook, Item, Spot},
};

/// A row of the list: a bar, or `None` for the hidden items.
type Row = Option<Spot>;

pub(super) fn rows(look: &ChatLook, cx: &Context<ChatView>) -> AnyElement {
    let bars = Spot::ALL.map(|spot| row(Some(spot), look.items(spot), cx));
    v_flex()
        .id("chat-look-rows")
        .test_support()
        .gap_1()
        .children(bars)
        .child(row(None, &look.hidden(), cx))
        .into_any_element()
}

/// Takes a chip dropped on its free part at its end.
fn row(at: Row, items: &[Item], cx: &Context<ChatView>) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let name = at.map_or("hidden", Spot::name);
    let label = at.map_or("Hidden", Spot::label);
    h_flex()
        .gap_3()
        .child(div().w(px(120.)).flex_none().child(label))
        .child(
            h_flex()
                .id(SharedString::from(format!("chat-look-row-{name}")))
                .test_support()
                .flex_1()
                .min_w_0()
                .min_h(px(30.))
                .flex_wrap()
                .gap_1()
                .p_1()
                .rounded(theme.radius)
                .border_1()
                .border_dashed()
                .border_color(theme.border)
                .drag_over::<DraggedItem>(|row, _, _, cx| {
                    row.border_color(cx.theme().primary)
                        .bg(cx.theme().primary.opacity(0.08))
                })
                .on_drop(cx.listener(move |_, dragged: &DraggedItem, _, cx| {
                    ChatView::change_look(placed(ChatLook::current(cx), dragged.0, at, None), cx);
                }))
                .children(items.iter().map(|item| chip(*item, at, cx)))
                .when(items.is_empty(), |row| {
                    row.child(
                        div()
                            .px_1()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Drop here"),
                    )
                }),
        )
}

/// Dragged to another row or place; one dropped on it goes before it,
/// where the line on its left shows.
fn chip(item: Item, at: Row, cx: &Context<ChatView>) -> impl IntoElement + use<> {
    let theme = cx.theme();
    div()
        .id(SharedString::from(format!(
            "chat-look-chip-{}",
            item.name()
        )))
        .test_support()
        .px_2()
        .py_0p5()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .text_sm()
        .cursor_grab()
        .child(item.label())
        .on_drag(DraggedItem(item), |dragged, _, _, cx| cx.new(|_| *dragged))
        .drag_over::<DraggedItem>(|chip, _, _, cx| {
            chip.border_l_4().border_color(cx.theme().primary)
        })
        .on_drop(cx.listener(move |_, dragged: &DraggedItem, _, cx| {
            cx.stop_propagation();
            let look = placed(ChatLook::current(cx), dragged.0, at, Some(item));
            ChatView::change_look(look, cx);
        }))
}

/// `look` with `item` moved to row `at`, before `before` there, else at its
/// end; the hidden row keeps no order.
fn placed(look: &ChatLook, item: Item, at: Row, before: Option<Item>) -> ChatLook {
    match at {
        Some(spot) => look.moved(item, spot, before),
        None => look.without(item),
    }
}
