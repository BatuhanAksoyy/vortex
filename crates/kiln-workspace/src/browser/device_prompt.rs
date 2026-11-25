//! A page asking for a Bluetooth device: the browser's chooser, which it
//! cannot show itself, drawn over the page. The page waits until the user
//! picks a device or says no.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, ClickEvent, Context, FocusHandle, FontWeight, KeyDownEvent, SharedString,
    TestSupportExt, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Disableable as _, Icon, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        spinner::Spinner,
        v_flex,
    },
    div, px,
};
use kiln_browser::{Device, DeviceRequest};

use super::{Browser, site_permissions::host};

const WIDTH: f32 = 360.;
const MAX_LIST: f32 = 220.;

pub(super) struct DevicePrompt {
    request: DeviceRequest,
    /// The device chosen, by id.
    chosen: Option<String>,
    focus: FocusHandle,
}

impl Browser {
    /// Shows the devices found so far; the list grows as more are.
    pub(super) fn ask_device(
        &mut self,
        request: DeviceRequest,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kept = self
            .devices
            .take()
            .filter(|open| open.request.id == request.id);
        let prompt = if let Some(mut open) = kept {
            let listed = |id: &String| request.devices.iter().any(|device| device.id == *id);
            open.chosen = open.chosen.filter(listed);
            open.request = request;
            open
        } else {
            self.agent.asked += 1;
            let focus = cx.focus_handle();
            // Only where the user is: a page must not take the keyboard from
            // the chat.
            if self.focus.contains_focused(window, cx) {
                focus.focus(window, cx);
            }
            DevicePrompt {
                request,
                chosen: None,
                focus,
            }
        };
        self.devices = Some(prompt);
        cx.notify();
    }

    fn choose_device(
        &mut self,
        id: String,
        click: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = &mut self.devices {
            open.chosen = Some(id);
            open.focus.focus(window, cx);
        }
        if click.click_count() > 1 {
            self.answer_device(true, window, cx);
        }
        cx.notify();
    }

    /// The chooser and the devices found so far, for the agent to read.
    pub(super) fn device_question(&self) -> Option<String> {
        let open = self.devices.as_ref()?;
        let found: Vec<String> = (open.request.devices.iter())
            .map(|device| format!("- “{}” (id {})", shown_name(device), device.id))
            .collect();
        let found = if found.is_empty() {
            "None yet; it keeps looking, so ask again in a moment.".to_owned()
        } else {
            found.join("\n")
        };
        let site = host(&self.url);
        Some(format!(
            "{site} wants to pair with a Bluetooth device. Found so far:\n{found}"
        ))
    }

    /// Picks `device`, by id or name, or cancels the chooser.
    pub(super) fn pick_device_for_agent(
        &mut self,
        accept: bool,
        device: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<String, String> {
        let open = self
            .devices
            .as_mut()
            .ok_or("No Bluetooth chooser is open.")?;
        if !accept {
            self.answer_device(false, window, cx);
            return Ok("Cancelled.".to_owned());
        }
        let wanted = device
            .ok_or("Name the device to pick with `device`.")?
            .trim();
        let devices = &open.request.devices;
        let by_name = |device: &&Device| shown_name(device).eq_ignore_ascii_case(wanted);
        let found = (devices.iter().find(|device| device.id == wanted))
            .or_else(|| devices.iter().find(by_name))
            .ok_or_else(|| {
                let listed: Vec<String> = devices
                    .iter()
                    .map(|it| format!("“{}” ({})", shown_name(it), it.id))
                    .collect();
                format!(
                    "No device “{wanted}” was found. Listed: {}.",
                    listed.join(", ")
                )
            })?;
        let picked = format!("Picked {}; the page goes on with it.", shown_name(found));
        open.chosen = Some(found.id.clone());
        self.answer_device(true, window, cx);
        Ok(picked)
    }

    /// Gives the page the chosen device, or none.
    fn answer_device(&mut self, pair: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.devices.take() else {
            return;
        };
        if let Some(page) = self.page() {
            match open.chosen.as_deref().filter(|_| pair) {
                Some(device) => page.pick_device(&open.request.id, device),
                None => page.refuse_device(&open.request.id),
            }
        }
        if open.focus.contains_focused(window, cx) {
            self.focus.focus(window, cx);
        }
        cx.notify();
    }

    fn on_device_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let chosen = self
            .devices
            .as_ref()
            .is_some_and(|open| open.chosen.is_some());
        match event.keystroke.key.as_str() {
            "enter" if chosen => self.answer_device(true, window, cx),
            "escape" => self.answer_device(false, window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    pub(super) fn render_device_prompt(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let open = self.devices.as_ref()?;
        let theme = cx.theme();
        let card = v_flex()
            .id("browser-device-prompt")
            .test_support()
            .track_focus(&open.focus)
            .on_key_down(cx.listener(Self::on_device_key))
            .w(px(WIDTH))
            .max_w_full()
            .gap_3()
            .p_4()
            .rounded(theme.radius_lg)
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_lg()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{} wants to pair", host(&self.url))),
            )
            .child(Self::device_list(open, cx))
            .child(device_buttons(open.chosen.is_some(), cx));
        let over = div()
            .absolute()
            .inset_0()
            .occlude()
            .flex()
            .justify_center()
            .items_start()
            .p_4()
            .bg(theme.overlay)
            .child(card);
        Some(over.into_any_element())
    }

    fn device_list(open: &DevicePrompt, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        if open.request.devices.is_empty() {
            return h_flex()
                .id("browser-device-searching")
                .test_support()
                .gap_2()
                .py_2()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(Spinner::new().xsmall())
                .child("Looking for Bluetooth devices…")
                .into_any_element();
        }
        let rows =
            open.request.devices.iter().map(|device| {
                Self::device_row(device, open.chosen.as_ref() == Some(&device.id), cx)
            });
        v_flex()
            .id("browser-device-list")
            .max_h(px(MAX_LIST))
            .overflow_y_scroll()
            .rounded(theme.radius)
            .border_1()
            .border_color(theme.border)
            .children(rows)
            .into_any_element()
    }

    fn device_row(device: &Device, chosen: bool, cx: &Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let name = shown_name(device);
        let id = device.id.clone();
        let hover = theme.list_hover;
        h_flex()
            .id(SharedString::from(format!("browser-device-{}", device.id)))
            .test_support()
            .aria_label(SharedString::from(name.clone()))
            .gap_2()
            .px_3()
            .py_1p5()
            .text_sm()
            .cursor_pointer()
            .hover(move |row| row.bg(hover))
            .when(chosen, |row| row.bg(theme.list_active))
            .on_click(cx.listener(move |this, click, window, cx| {
                this.choose_device(id.clone(), click, window, cx);
            }))
            .child(
                Icon::new(Lucide::Bluetooth)
                    .small()
                    .text_color(theme.muted_foreground),
            )
            .child(div().min_w_0().truncate().child(name))
            .into_any_element()
    }
}

fn device_buttons(chosen: bool, cx: &Context<Browser>) -> impl IntoElement + use<> {
    h_flex()
        .justify_end()
        .gap_2()
        .child(
            Button::new("browser-device-cancel")
                .small()
                .label("Cancel")
                .on_click(cx.listener(|this, _, window, cx| this.answer_device(false, window, cx))),
        )
        .child(
            Button::new("browser-device-pair")
                .primary()
                .small()
                .label("Pair")
                .disabled(!chosen)
                .on_click(cx.listener(|this, _, window, cx| this.answer_device(true, window, cx))),
        )
}

/// As the browser shows a device that does not say its name.
fn shown_name(device: &Device) -> String {
    match device.name.trim() {
        "" => format!("Unknown device ({})", device.id),
        name => name.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_without_a_name_is_told_apart_by_its_address() {
        let device = Device {
            id: "09:09".into(),
            name: " ".into(),
        };
        assert_eq!(shown_name(&device), "Unknown device (09:09)");
    }
}
