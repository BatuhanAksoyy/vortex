//! The chat's buttons and meters, set out as the chats' look has them, and
//! the customize mode that changes the look: items are dragged between the
//! title bar and the composer's strip or hidden, the usage meter reads one
//! window or both, and the harness takes a color of its own.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, SharedString, TestSupportExt,
    assets::IconName as Lucide,
    base::Selectable as _,
    component::{
        ActiveTheme, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        tab::{Tab, TabBar},
        v_flex,
    },
    div, px, rgb,
};

mod frame;

use frame::{DraggedItem, caption, editable, hidden_chip};

use crate::chat_view::{ChatEvent, ChatView};
use crate::{
    look::{ChatLook, Item, Spot, UsageShown},
    scale::ScaledButton as _,
    spinner::Brand,
};

/// The colors offered for a harness, beside its own.
const COLORS: [(u32, &str); 8] = [
    (0xe5_48_4d, "Red"),
    (0xf7_6b_15, "Orange"),
    (0xff_b2_24, "Amber"),
    (0x30_a4_6c, "Green"),
    (0x12_a5_94, "Teal"),
    (0x3e_63_dd, "Blue"),
    (0x8e_4e_c6, "Violet"),
    (0xd6_40_9f, "Pink"),
];

impl ChatView {
    /// The items of `spot`; in customize mode each can be dragged, and the
    /// row takes dropped ones at its end.
    pub(in crate::chat_view) fn render_spot(
        &self,
        spot: Spot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = ChatLook::current(cx).clone();
        let items: Vec<AnyElement> = look
            .items(spot)
            .iter()
            .filter_map(|item| {
                let element = self.item_element(*item, cx)?;
                Some(if look.editing {
                    editable(*item, spot, element, cx)
                } else {
                    element
                })
            })
            .collect();
        let empty = items.is_empty();
        let row = h_flex()
            .id(SharedString::from(format!("chat-spot-{}", spot.name())))
            .test_support()
            .gap_1()
            // The strip's start takes its room and wraps in a narrow chat,
            // so the send button stays in view.
            .when(spot == Spot::BottomStart, |row| {
                row.flex_1().min_w_0().flex_wrap()
            })
            .when(look.editing, |row| row.child(caption(spot, cx)))
            .children(items);
        if !look.editing {
            return row.into_any_element();
        }
        let theme = cx.theme();
        // Outlined, so it is plain which bar an item goes to.
        row.when(empty, |row| {
            row.child(
                div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child("Drop here"),
            )
        })
        .min_w(px(48.))
        .min_h(px(28.))
        .p_0p5()
        .rounded(theme.radius)
        .border_1()
        .border_dashed()
        .border_color(theme.primary.opacity(0.6))
        .bg(theme.primary.opacity(0.04))
        .drag_over::<DraggedItem>(|row, _, _, cx| {
            row.border_color(cx.theme().primary)
                .bg(cx.theme().primary.opacity(0.12))
        })
        .on_drop(cx.listener(move |_, dragged: &DraggedItem, _, cx| {
            let look = ChatLook::current(cx).moved(dragged.0, spot, None);
            Self::change_look(look, cx);
        }))
        .into_any_element()
    }

    fn item_element(&self, item: Item, cx: &mut Context<Self>) -> Option<AnyElement> {
        match item {
            Item::Processes | Item::Account | Item::Usage | Item::NewChat => {
                self.title_item(item, cx)
            }
            Item::Attach | Item::Skills | Item::Hooks | Item::Mode | Item::Model | Item::Effort => {
                self.choice_item(item, cx)
            }
            Item::Agents | Item::Tokens | Item::Cache | Item::Context => self.meter_item(item, cx),
        }
    }

    fn title_item(&self, item: Item, cx: &mut Context<Self>) -> Option<AnyElement> {
        Some(match item {
            Item::Processes => self.process_meter.clone()?.into_any_element(),
            Item::Account => self.account_button(cx).into_any_element(),
            Item::Usage => self.usage_element(cx),
            _ => Button::new("chat-new")
                .ghost()
                .control_icon(IconName::Plus)
                .accessibility_label("New conversation")
                .tooltip("New conversation")
                .on_click(cx.listener(|this, _, window, cx| this.new_conversation(window, cx)))
                .into_any_element(),
        })
    }

    fn choice_item(&self, item: Item, cx: &mut Context<Self>) -> Option<AnyElement> {
        let skills_and_hooks = self.backend.features().skills_and_hooks;
        let view = cx.entity().downgrade();
        Some(match item {
            Item::Skills if skills_and_hooks => self.skills_button(cx).into_any_element(),
            Item::Hooks if skills_and_hooks => self.hooks_button(cx).into_any_element(),
            Item::Skills | Item::Hooks => return None,
            Item::Mode => self.mode_menu(&view).into_any_element(),
            Item::Model => self.model_menu(&view)?.into_any_element(),
            Item::Effort => self.effort_menu(&view)?.into_any_element(),
            _ => Self::attach_button(cx).into_any_element(),
        })
    }

    fn meter_item(&self, item: Item, cx: &mut Context<Self>) -> Option<AnyElement> {
        Some(match item {
            Item::Agents => self.agents_button(cx).into_any_element(),
            Item::Tokens => self.tokens_button(cx)?.into_any_element(),
            Item::Cache if self.backend.features().prompt_cache => {
                self.cache_button(cx).into_any_element()
            }
            Item::Context => self.context_meter(cx)?.into_any_element(),
            _ => return None,
        })
    }

    /// The button that starts and ends customize mode, which every chat
    /// shows at once.
    pub(in crate::chat_view) fn customize_button(cx: &Context<Self>) -> impl IntoElement + use<> {
        let editing = ChatLook::current(cx).editing;
        Button::new("chat-customize")
            .ghost()
            .control_icon(Lucide::PenLine)
            .selected(editing)
            .tooltip(if editing {
                "Done Customizing"
            } else {
                "Customize Chat"
            })
            .on_click(cx.listener(|_, _, _, cx| {
                let look = ChatLook::current(cx);
                let look = ChatLook {
                    editing: !look.editing,
                    ..look.clone()
                };
                cx.set_global(look);
                cx.refresh_windows();
            }))
    }

    /// Shows `look` in every chat at once, and hands it to the window to
    /// keep in the settings.
    fn change_look(look: ChatLook, cx: &mut Context<Self>) {
        cx.set_global(look.clone());
        cx.refresh_windows();
        cx.emit(ChatEvent::LookChanged(look));
    }

    /// The bar of customize mode, under the title bar.
    pub(in crate::chat_view) fn render_look_bar(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let look = ChatLook::current(cx).clone();
        if !look.editing {
            return None;
        }
        let theme = cx.theme();
        let brand = Brand::of(self.backend.name());
        let hidden = look.hidden().into_iter().map(|item| hidden_chip(item, cx));
        let bar = v_flex()
            .id("chat-look-bar")
            .test_support()
            .gap_2()
            .px_5()
            .py_2()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.secondary)
            .text_sm()
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().min_w_0().child(
                        "Drag an item onto another to put it before it, or onto a \
                             bar's free end. The ⋯ button moves or hides it without a drag.",
                    ))
                    .child(
                        Button::new("chat-look-reset")
                            .ghost()
                            .control()
                            .label("Reset")
                            .on_click(cx.listener(|_, _, _, cx| {
                                let editing = ChatLook {
                                    editing: true,
                                    ..ChatLook::default()
                                };
                                Self::change_look(editing, cx);
                            })),
                    ),
            )
            .child(labelled(
                "Hidden",
                h_flex()
                    .flex_wrap()
                    .gap_1()
                    .children(hidden)
                    .into_any_element(),
            ))
            .child(labelled("Usage", Self::usage_choice(&look, cx)))
            .child(labelled(
                brand.title(),
                Self::color_choice(&look, brand, cx),
            ));
        Some(bar.into_any_element())
    }

    fn usage_choice(look: &ChatLook, cx: &Context<Self>) -> AnyElement {
        let selected = UsageShown::ALL
            .iter()
            .position(|shown| *shown == look.usage);
        TabBar::new("chat-look-usage")
            .segmented()
            .small()
            .selected_index(selected.unwrap_or_default())
            .children(UsageShown::ALL.map(|shown| Tab::new().label(shown.label())))
            .on_click(cx.listener(|_, index: &usize, _, cx| {
                if let Some(shown) = UsageShown::ALL.get(*index) {
                    let look = ChatLook {
                        usage: *shown,
                        ..ChatLook::current(cx).clone()
                    };
                    Self::change_look(look, cx);
                }
            }))
            .into_any_element()
    }

    fn color_choice(look: &ChatLook, brand: Brand, cx: &Context<Self>) -> AnyElement {
        let current = look.color(brand);
        let own = std::iter::once((None, "Its own"));
        let offered = own.chain(COLORS.iter().map(|(color, name)| (Some(*color), *name)));
        let swatches = offered.map(|(color, name)| {
            let shown = color.map_or_else(|| brand.own_color(cx), |color| rgb(color).into());
            div()
                .id(SharedString::from(format!(
                    "chat-look-color-{}",
                    name.to_lowercase()
                )))
                .test_support()
                .size(px(18.))
                .rounded_full()
                .bg(shown)
                .border_2()
                .border_color(if color == current {
                    cx.theme().foreground
                } else {
                    cx.theme().border
                })
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(name).build(window, cx)
                })
                .on_click(cx.listener(move |_, _, _, cx| {
                    let look = ChatLook::current(cx).colored(brand, color);
                    Self::change_look(look, cx);
                }))
        });
        h_flex().gap_1p5().children(swatches).into_any_element()
    }
}

fn labelled(label: &'static str, element: AnyElement) -> impl IntoElement {
    h_flex()
        .gap_3()
        .child(div().w(px(88.)).flex_none().child(label))
        .child(element)
}
