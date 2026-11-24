//! A stand-in for the browser beside the chats: its place, its controls and
//! a drawn page, to settle the design. It loads nothing yet.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, EventEmitter, FontWeight, Hsla, TestSupportExt, Window,
    assets::IconName as Lucide,
    base::Selectable as _,
    component::{
        ActiveTheme, Disableable as _, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{Input, InputState},
        tag::Tag,
        v_flex,
    },
    div, px, relative,
};

const PHONE_WIDTH: f32 = 390.;
const ADDRESS: &str = "http://localhost:3000";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Device {
    Desktop,
    Phone,
}

pub enum BrowserEvent {
    Close,
}

pub struct Browser {
    address: Entity<InputState>,
    device: Device,
}

impl EventEmitter<BrowserEvent> for Browser {}

impl Browser {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            address: cx.new(|cx| InputState::new(window, cx).default_value(ADDRESS)),
            device: Device::Desktop,
        }
    }

    fn header(cx: &Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .flex_none()
            .gap_2()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(div().font_weight(FontWeight::SEMIBOLD).child("Browser"))
            .child(Tag::secondary().small().child("Demo"))
            .child(div().flex_1())
            .child(
                Button::new("browser-close")
                    .ghost()
                    .small()
                    .icon(IconName::Close)
                    .tooltip("Close Panel")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(BrowserEvent::Close))),
            )
    }

    /// What needs a real page is shown disabled.
    fn toolbar(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let waiting = |id, icon, tooltip| {
            let button = Button::new(id).ghost().small().icon(icon);
            button.tooltip(tooltip).disabled(true)
        };
        h_flex()
            .flex_none()
            .gap_1()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(waiting("browser-back", Lucide::ArrowLeft, "Back"))
            .child(waiting("browser-forward", Lucide::ArrowRight, "Forward"))
            .child(waiting("browser-reload", Lucide::RefreshCw, "Reload"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.address).small()),
            )
            .child(self.device_button(Device::Desktop, cx))
            .child(self.device_button(Device::Phone, cx))
            .child(waiting(
                "browser-pick",
                Lucide::MousePointerClick,
                "Pick an Element for the Chat",
            ))
    }

    fn device_button(&self, device: Device, cx: &Context<Self>) -> impl IntoElement + use<> {
        let (id, icon, tooltip) = match device {
            Device::Desktop => ("browser-desktop", Lucide::Monitor, "Desktop Size"),
            Device::Phone => ("browser-phone", Lucide::Smartphone, "Phone Size"),
        };
        Button::new(id)
            .ghost()
            .small()
            .icon(icon)
            .tooltip(tooltip)
            .selected(self.device == device)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.device = device;
                cx.notify();
            }))
    }

    fn page(&self, cx: &App) -> AnyElement {
        let theme = cx.theme();
        let phone = self.device == Device::Phone;
        let cards = if phone { v_flex() } else { h_flex() };
        v_flex()
            .id("browser-page")
            .test_support()
            .when(phone, |this| this.w(px(PHONE_WIDTH)).max_w_full())
            .when(!phone, Styled::w_full)
            .h_full()
            .overflow_y_scroll()
            .bg(theme.background)
            .when(phone, |this| {
                this.rounded(theme.radius_lg)
                    .border_1()
                    .border_color(theme.border)
            })
            .child(
                h_flex()
                    .flex_none()
                    .gap_2()
                    .p_4()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(div().size_5().rounded_full().bg(theme.primary))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("Your app"))
                    .child(div().flex_1())
                    .child(div().w(px(96.)).child(bar(1.0, px(10.), theme.muted))),
            )
            .child(
                v_flex()
                    .flex_none()
                    .gap_4()
                    .p_6()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::BOLD)
                            .child("The page the agent builds shows here"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child("This is a drawing, not a web page."),
                    )
                    .child(
                        cards
                            .w_full()
                            .items_stretch()
                            .gap_3()
                            .children((0..3).map(|_| card(cx))),
                    ),
            )
            .into_any_element()
    }
}

fn card(cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    v_flex()
        .flex_1()
        .gap_2()
        .p_3()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .child(bar(1.0, px(56.), theme.muted))
        .child(bar(0.7, px(10.), theme.muted))
        .child(bar(0.5, px(10.), theme.muted))
}

/// A line or picture of the drawn page.
fn bar(width: f32, height: gpui_kit::Pixels, color: Hsla) -> impl IntoElement + use<> {
    div().w(relative(width)).h(height).rounded_md().bg(color)
}

impl Render for Browser {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .id("browser")
            .test_support()
            .size_full()
            .border_l_1()
            .border_color(theme.border)
            .child(Self::header(cx))
            .child(self.toolbar(cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .when(self.device == Device::Phone, |this| {
                        this.p_3().bg(theme.muted)
                    })
                    .child(self.page(cx)),
            )
    }
}
