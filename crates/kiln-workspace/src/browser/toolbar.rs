//! Where the page is, and what can be done with it.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context,
    assets::IconName as Lucide,
    base::Selectable as _,
    component::{
        ActiveTheme, Disableable as _, Icon, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::Input,
    },
    div, px,
};

use super::{Browser, BrowserEvent, Device};

/// Narrower, the address could not be read; the tools go under it instead.
const MIN_ADDRESS: f32 = 140.;

impl Browser {
    pub(super) fn toolbar(&mut self, cx: &Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .flex_none()
            .flex_wrap()
            .justify_end()
            .gap_1()
            .px_2()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex().flex_1().gap_1().child(self.navigation(cx)).child(
                    div()
                        .flex_1()
                        .min_w(px(MIN_ADDRESS))
                        .child(Input::new(&self.address).small()),
                ),
            )
            .child(self.tools(cx))
    }

    fn navigation(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let closed = self.page().is_none();
        let reload = if self.loading {
            button("browser-stop", IconName::Close, "Stop Loading")
                .on_click(cx.listener(|this, _, _, _| this.stop()))
        } else {
            button("browser-reload", Lucide::RefreshCw, "Reload")
                .on_click(cx.listener(|this, _, _, _| this.reload()))
        };
        h_flex()
            .flex_none()
            .gap_1()
            .child(
                button("browser-back", Lucide::ArrowLeft, "Back")
                    .disabled(closed || !self.history.can_go_back())
                    .on_click(cx.listener(|this, _, _, _| this.go_back())),
            )
            .child(
                button("browser-forward", Lucide::ArrowRight, "Forward")
                    .disabled(closed || !self.history.can_go_forward())
                    .on_click(cx.listener(|this, _, _, _| this.go_forward())),
            )
            .child(reload.disabled(closed))
    }

    fn tools(&mut self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let closed = self.page().is_none();
        h_flex()
            .flex_none()
            .gap_1()
            .child(
                button(
                    "browser-pick",
                    Lucide::MousePointerClick,
                    "Pick an Element for the Chat",
                )
                .disabled(closed)
                .selected(self.picking.on)
                .on_click(cx.listener(|this, _, window, cx| this.toggle_picking(window, cx))),
            )
            .child(
                button(
                    "browser-screenshot",
                    Lucide::Camera,
                    "Attach a Screenshot to the Message",
                )
                .disabled(closed)
                .on_click(cx.listener(|this, _, window, cx| this.attach_screenshot(window, cx))),
            )
            .children(self.site_permissions_button(cx))
            .child(self.tools_button(cx))
            .child(
                button("browser-phone", Lucide::Smartphone, "Phone Size")
                    .selected(self.device == Device::Phone)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_phone(cx))),
            )
            .child(
                button(
                    "browser-outside",
                    Lucide::ExternalLink,
                    "Open in Your Browser",
                )
                .disabled(!self.url.starts_with("http"))
                .on_click(cx.listener(|this, _, _, cx| this.open_outside(cx))),
            )
            .child(
                button("browser-close", IconName::Close, "Close Panel")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(BrowserEvent::Close))),
            )
    }

    fn toggle_phone(&mut self, cx: &mut Context<Self>) {
        self.set_picking(false, cx);
        self.device = match self.device {
            Device::Desktop => Device::Phone,
            Device::Phone => Device::Desktop,
        };
        cx.notify();
    }
}

fn button(id: &'static str, icon: impl Into<Icon>, tooltip: &'static str) -> Button {
    let button = Button::new(id).ghost().small();
    button.icon(icon).tooltip(tooltip)
}
