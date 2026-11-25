//! The browser beside the chats: one page per session, drawn by the browser
//! installed on this machine.

mod activity;
mod agent;
mod console_panel;
mod device_prompt;
mod dialog;
mod downloads;
mod host;
mod keyboard;
mod live;
mod menus;
mod network_panel;
mod page_view;
mod permission_prompt;
mod picking;
mod requests;
mod site_permissions;
mod site_question;
mod storage_panel;
mod toolbar;
mod tools_panel;
mod usage_panel;

use std::{path::PathBuf, sync::Arc};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Bounds, Context, CursorStyle, Entity, EventEmitter, FocusHandle, Focusable, Pixels,
    Point, RenderImage, SharedString, Subscription, Task, TestSupportExt, Window,
    component::{
        ActiveTheme, WindowExt,
        input::{InputEvent, InputState},
        notification::Notification,
        v_flex,
    },
    div, px,
};
use kiln_browser::{Frame, History, Viewport};
use kiln_chat::Attachment;

pub use keyboard::bind_keys;
use live::State;

const PHONE_WIDTH: f32 = 390.;
const KEY_CONTEXT: &str = "BrowserPage";

#[derive(Clone, Copy, PartialEq, Eq)]
enum Device {
    Desktop,
    Phone,
}

pub enum BrowserEvent {
    Close,
    /// To go with the next message.
    Attach(Vec<Attachment>),
}

pub struct Browser {
    address: Entity<InputState>,
    device: Device,
    /// Where the browser keeps cookies and the like; `None` runs no browser.
    profile: Option<PathBuf>,
    state: State,
    url: String,
    history: History,
    loading: bool,
    /// Whether the panel shows this page.
    shown: bool,
    /// The latest picture, and its pixels per page pixel.
    picture: Option<(Arc<RenderImage>, f32)>,
    /// Whether a frame not painted yet was acknowledged.
    ahead: bool,
    /// Frames to acknowledge when the window is painted.
    unpainted: Vec<Frame>,
    viewport: Viewport,
    bounds: Bounds<Pixels>,
    /// The window's device pixels per pixel.
    scale: f32,
    focus: FocusHandle,
    pressed: Option<kiln_browser::Button>,
    /// Where the mouse was last pressed, in the window.
    last_press: Point<Pixels>,
    cursor: CursorStyle,
    menu: Option<menus::OpenMenu>,
    dialog: Option<dialog::OpenDialog>,
    /// The page waits on the user choosing a device.
    devices: Option<device_prompt::DevicePrompt>,
    permits: permission_prompt::Permits,
    downloads: Vec<downloads::Download>,
    console: console_panel::Console,
    tools: tools_panel::Tools,
    network: network_panel::Network,
    storage: storage_panel::Storage,
    usage: usage_panel::Usage,
    picking: picking::Picking,
    agent: agent::AgentUse,
    activity: activity::Activity,
    /// The length of the text an input method is composing.
    marked: usize,
    /// Opens the page and follows it; dropping it lets go of both.
    follow: Task<()>,
    navigation: Task<()>,
    reading: Task<()>,
    showing: Task<()>,
    close_later: Task<()>,
    _address: Subscription,
}

impl EventEmitter<BrowserEvent> for Browser {}

impl Focusable for Browser {
    fn focus_handle(&self, _: &gpui_kit::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Browser {
    pub fn new(profile: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let address = cx.new(|cx| InputState::new(window, cx).placeholder("Enter an address"));
        let enter = cx.subscribe_in(&address, window, |this, _, event, window, cx| match event {
            InputEvent::PressEnter { .. } => this.go_to_address(window, cx),
            InputEvent::Blur => this.show_url(window, cx),
            InputEvent::Change | InputEvent::Focus => {}
        });
        Self {
            address,
            device: Device::Desktop,
            profile,
            state: State::Blank,
            url: String::new(),
            history: History::default(),
            loading: false,
            shown: false,
            picture: None,
            ahead: false,
            unpainted: Vec::new(),
            viewport: Viewport {
                width: 800,
                height: 600,
                phone: false,
            },
            bounds: Bounds::default(),
            scale: window.scale_factor(),
            focus: cx.focus_handle(),
            pressed: None,
            last_press: Point::default(),
            cursor: CursorStyle::Arrow,
            menu: None,
            dialog: None,
            devices: None,
            permits: permission_prompt::Permits::default(),
            downloads: Vec::new(),
            console: console_panel::Console::default(),
            tools: tools_panel::Tools::default(),
            network: network_panel::Network::default(),
            storage: storage_panel::Storage::default(),
            usage: usage_panel::Usage::default(),
            picking: picking::Picking::default(),
            agent: agent::AgentUse::default(),
            activity: activity::Activity::default(),
            marked: 0,
            follow: Task::ready(()),
            navigation: Task::ready(()),
            reading: Task::ready(()),
            showing: Task::ready(()),
            close_later: Task::ready(()),
            _address: enter,
        }
    }

    /// Ready to be told where to go, unless a page is open.
    pub fn focus_content(&self, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.state {
            State::Open(_) => self.focus.clone(),
            State::Blank | State::Opening | State::PutAway | State::Failed(_) => {
                self.address.focus_handle(cx)
            }
        };
        target.focus(window, cx);
    }

    fn go_to_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.address.read(cx).value();
        match kiln_browser::normalize(&typed) {
            Ok(url) => self.go_to(url, window, cx),
            Err(error) => {
                window.push_notification(Notification::warning(error.to_string()), cx);
            }
        }
    }

    /// Shows where the page is, unless the user is typing somewhere to go.
    fn show_url(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.url.is_empty() || self.address.focus_handle(cx).is_focused(window) {
            return;
        }
        let url = self.url.clone();
        self.address
            .update(cx, |address, cx| address.set_value(url, window, cx));
    }

    fn reload(&self) {
        if let Some(page) = self.page() {
            page.reload();
        }
    }

    fn stop(&self) {
        if let Some(page) = self.page() {
            page.stop();
        }
    }

    fn go_back(&self) {
        if let Some(page) = self.page() {
            page.go_back(&self.history);
        }
    }

    fn go_forward(&self) {
        if let Some(page) = self.page() {
            page.go_forward(&self.history);
        }
    }

    fn content(&self, cx: &Context<Self>) -> AnyElement {
        match &self.state {
            State::Blank => notice(
                "Enter an address, such as localhost:3000, to open a page.",
                cx,
            ),
            State::Opening | State::PutAway => notice("Opening…", cx),
            State::Failed(message) => notice(message, cx),
            State::Open(_) => self.device_frame(cx),
        }
    }

    fn device_frame(&self, cx: &Context<Self>) -> AnyElement {
        let phone = self.device == Device::Phone;
        let theme = cx.theme();
        div()
            .when(phone, |this| {
                this.w(px(PHONE_WIDTH))
                    .max_w_full()
                    .border_1()
                    .border_color(theme.border)
            })
            .when(!phone, Styled::w_full)
            .h_full()
            .child(self.render_page(cx))
            .into_any_element()
    }
}

fn notice(message: &str, cx: &Context<Browser>) -> AnyElement {
    v_flex()
        .id("browser-notice")
        .test_support()
        .aria_label(SharedString::from(message.to_owned()))
        .size_full()
        .items_center()
        .justify_center()
        .p_6()
        .text_center()
        .text_color(cx.theme().muted_foreground)
        .child(message.to_owned())
        .into_any_element()
}

impl Render for Browser {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let phone = self.device == Device::Phone;
        v_flex()
            .id("browser")
            .test_support()
            .size_full()
            .border_l_1()
            .border_color(theme.border)
            .child(self.toolbar(cx))
            .child(
                v_flex()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .when(phone, |this| this.p_3().bg(theme.muted))
                    .child(self.content(cx))
                    .children(self.render_activity(cx))
                    .children(self.render_dialog(cx))
                    .children(self.render_device_prompt(cx))
                    .children(self.render_permission_question(cx))
                    .children(self.render_site_question(cx))
                    .children(self.render_menu()),
            )
            .children(self.render_tools(cx))
            .children(self.render_downloads(cx))
    }
}
