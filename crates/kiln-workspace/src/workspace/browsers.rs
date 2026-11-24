//! The panel on the chats' right: the active session's browser.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, Pixels, Window,
    assets::IconName as Lucide,
    base::Selectable as _,
    component::{
        Sizable as _,
        button::{Button, ButtonVariants as _},
        resizable::{ResizableState, h_resizable, resizable_panel},
    },
    px,
};

use super::{
    Workspace,
    side::{MIN_WIDTH, share},
};
use crate::{
    browser::{Browser, BrowserEvent},
    side_panel::MAX_BAR,
};

impl Workspace {
    /// The chats and, while it is open, the browser on their right.
    pub(super) fn render_chats(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        let chats = self.dock.clone();
        let Some(browser) = self.browser().filter(|_| self.side_panel.browser_open) else {
            return chats.into_any_element();
        };
        let this = cx.entity().downgrade();
        let resized = move |state: &Entity<ResizableState>, _: &mut Window, cx: &mut App| {
            let width = state.read(cx).sizes().get(1).copied().map(f32::from);
            let _ = this.update(cx, |this, cx| {
                this.side_panel.browser_width = width;
                this.save_side_panel(cx);
            });
        };
        h_resizable("chats-and-browser")
            .child(
                resizable_panel()
                    .size_range(px(MIN_WIDTH)..Pixels::MAX)
                    .child(chats),
            )
            .child(
                resizable_panel()
                    .size(self.browser_width(window))
                    .fixed()
                    .size_range(px(MIN_WIDTH)..Pixels::MAX)
                    .child(browser.clone()),
            )
            .on_resize(resized)
            .into_any_element()
    }

    fn browser_width(&self, window: &Window) -> Pixels {
        let side = if self.side_open() {
            self.side_width(window)
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
        if let Some(chat) = self.active_chat.as_ref().filter(|_| open) {
            let chat = chat.entity_id();
            if self.browser().is_none() {
                let browser = self.new_browser(window, cx);
                self.browsers.push((chat, browser));
            }
        }
        let shown = self.browser().filter(|_| open).map(Entity::entity_id);
        for (_, browser) in &self.browsers {
            let show = Some(browser.entity_id()) == shown;
            browser.update(cx, |browser, cx| browser.set_shown(show, window, cx));
        }
        cx.notify();
    }

    fn new_browser(&self, window: &mut Window, cx: &mut Context<Self>) -> Entity<Browser> {
        let profile = self.browser_profile.clone();
        let browser = cx.new(|cx| Browser::new(profile, window, cx));
        let close = cx.subscribe_in(&browser, window, |this, _, event, window, cx| match event {
            BrowserEvent::Close => this.toggle_browser(window, cx),
        });
        close.detach();
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
        self.side_maximized = false;
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
        let open = self.side_panel.browser_open;
        let toggle = Button::new("toggle-browser")
            .ghost()
            .small()
            .icon(Lucide::Globe)
            .selected(open)
            .tooltip(if open { "Hide Browser" } else { "Show Browser" })
            .on_click(cx.listener(|this, _, window, cx| this.toggle_browser(window, cx)));
        Some(toggle)
    }
}
