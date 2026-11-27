//! The panel beside or under the chats: the active session's browser.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, Pixels, Window,
    assets::IconName as Lucide,
    base::Selectable as _,
    component::{
        button::{Button, ButtonVariants as _},
        resizable::{ResizableState, h_resizable, resizable_panel, v_resizable},
    },
    px,
};
use vortex_chat::scale::ScaledButton as _;

use super::{
    Workspace, chats,
    side::{MIN_WIDTH, share},
};
use vortex_settings::BrowserPlace;

use crate::{
    browser::{Browser, BrowserEvent},
    chat_panel::ChatPanel,
    config,
    side_panel::MAX_BAR,
};

/// Lower, the browser's page or the chats cannot be used.
const MIN_HEIGHT: f32 = 200.;

impl Workspace {
    /// The chats and, while it is open, the browser on the side the
    /// settings put it.
    pub(super) fn render_chats(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let chats = self.dock.clone();
        let Some(browser) = self.browser().filter(|_| self.side_panel.browser_open) else {
            return chats.into_any_element();
        };
        let place = config::settings(cx).layout.browser;
        let first = place == BrowserPlace::Left;
        let this = cx.entity().downgrade();
        let resized = move |state: &Entity<ResizableState>, _: &mut Window, cx: &mut App| {
            let size = state.read(cx).sizes().get(usize::from(!first)).copied();
            let size = size.map(f32::from);
            let _ = this.update(cx, |this, cx| this.keep_browser_size(place, size, cx));
        };
        let (size, least) = if place == BrowserPlace::Bottom {
            (self.browser_height(window), px(MIN_HEIGHT))
        } else {
            (self.browser_width(window, cx), px(MIN_WIDTH))
        };
        let browser = resizable_panel()
            .size(size)
            .fixed()
            .size_range(least..Pixels::MAX)
            .child(browser.clone());
        let chats = resizable_panel()
            .size_range(least..Pixels::MAX)
            .child(chats);
        let (one, two) = if first {
            (browser, chats)
        } else {
            (chats, browser)
        };
        let split = match place {
            BrowserPlace::Bottom => v_resizable("chats-over-browser"),
            BrowserPlace::Left => h_resizable("browser-and-chats"),
            BrowserPlace::Right => h_resizable("chats-and-browser"),
        };
        split
            .child(one)
            .child(two)
            .on_resize(resized)
            .into_any_element()
    }

    fn keep_browser_size(
        &mut self,
        place: BrowserPlace,
        size: Option<f32>,
        cx: &mut Context<Self>,
    ) {
        if place == BrowserPlace::Bottom {
            self.side_panel.browser_height = size;
        } else {
            self.side_panel.browser_width = size;
        }
        self.save_side_panel(cx);
    }

    fn browser_height(&self, window: &Window) -> Pixels {
        let height = window.viewport_size().height;
        let kept = self.side_panel.browser_height.map_or(height * 0.4, px);
        kept.min(height - px(MIN_HEIGHT * 2.)).max(px(MIN_HEIGHT))
    }

    fn browser_width(&self, window: &Window, cx: &App) -> Pixels {
        let side = if self.side_open() {
            self.side_width(window, cx)
        } else {
            px(0.)
        };
        let width = self.side_panel.browser_width;
        share(width, side + px(MIN_WIDTH + MAX_BAR), window)
    }

    /// The active session's browser, once it was shown.
    fn browser(&self) -> Option<&Entity<Browser>> {
        let chat = self.active_chat.as_ref()?.entity_id();
        let mut browsers = self.browsers.iter();
        browsers.find_map(|(id, browser)| (*id == chat).then_some(browser))
    }

    /// Follows the active session to its browser while the panel is open.
    pub(super) fn sync_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.side_panel.browser_open;
        if let Some(chat) = self.active_chat.clone().filter(|_| open)
            && self.browser().is_none()
        {
            let browser = self.new_browser(&chat, window, cx);
            self.browsers.push((chat.entity_id(), browser));
        }
        let shown = self.browser().filter(|_| open).map(Entity::entity_id);
        for (_, browser) in &self.browsers {
            let show = Some(browser.entity_id()) == shown;
            browser.update(cx, |browser, cx| browser.set_shown(show, window, cx));
        }
        cx.notify();
    }

    pub(super) fn new_browser(
        &self,
        chat: &Entity<ChatPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Browser> {
        let profile = self.data_dir.as_ref().map(|dir| dir.join("browser"));
        let browser = cx.new(|cx| Browser::new(profile, window, cx));
        let chat = chat.downgrade();
        let events = cx.subscribe_in(
            &browser,
            window,
            move |this, _, event, window, cx| match event {
                BrowserEvent::Close => this.toggle_browser(window, cx),
                BrowserEvent::Attach(attachments) => {
                    let Some(panel) = chat.upgrade() else {
                        return;
                    };
                    let chat = panel.read(cx).chat().clone();
                    chat.update(cx, |chat, cx| chat.attach(attachments.clone(), cx));
                    chats::focus(&chat, window, cx);
                }
            },
        );
        events.detach();
        browser
    }

    pub fn toggle_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.project(cx).is_none() {
            Self::needs_project(window, cx);
            return;
        }
        let open = !self.side_panel.browser_open;
        self.side_panel.browser_open = open;
        // Opened while the side panel fills the window, it would not show.
        self.uncover_chats();
        self.save_side_panel(cx);
        self.sync_browser(window, cx);
        if open {
            self.focus_browser(window, cx);
        } else {
            self.focus_active_chat(window, cx);
        }
    }

    fn focus_browser(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(browser) = self.browser().cloned() {
            browser.update(cx, |browser, cx| browser.focus_content(window, cx));
        }
    }

    pub(super) fn render_browser_toggle(
        &self,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        self.project(cx)?;
        if !config::settings(cx).layout.title_bar_buttons {
            return None;
        }
        let open = self.side_panel.browser_open;
        let toggle = Button::new("toggle-browser")
            .ghost()
            .control_icon(Lucide::Globe)
            .selected(open)
            .tooltip(if open { "Hide Browser" } else { "Show Browser" })
            .on_click(cx.listener(|this, _, window, cx| this.toggle_browser(window, cx)));
        Some(toggle)
    }
}
