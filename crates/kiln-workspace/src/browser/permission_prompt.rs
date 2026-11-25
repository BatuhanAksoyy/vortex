//! Asking the user before a page uses the camera, the microphone, the
//! location, notifications or MIDI, as the browser would; and the page's
//! notifications, which the browser cannot show.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, FontWeight, SharedString, TestSupportExt, WeakEntity, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, WindowExt,
        button::{Button, ButtonVariants as _},
        h_flex,
        notification::Notification,
        popover::Popover,
        v_flex,
    },
    div, px,
};
use kiln_browser::{Page, Permission, PermissionRequest};
use kiln_chat::scale::ScaledButton as _;

use super::{
    Browser,
    site_permissions::{SitePermissions, host, origin},
};

const WIDTH: f32 = 360.;

#[derive(Default)]
pub(super) struct Permits {
    /// Loaded when first needed.
    kept: Option<SitePermissions>,
    /// Requests the user is being asked about, oldest first, with the site
    /// asking.
    waiting: Vec<(PermissionRequest, String)>,
}

impl Browser {
    fn site_permissions(&mut self) -> &mut SitePermissions {
        let profile = self.profile.clone();
        self.permits.kept.get_or_insert_with(|| {
            profile.map_or_else(SitePermissions::default, |profile| {
                SitePermissions::load(&profile)
            })
        })
    }

    /// Tells a page just opened what the user decided before; the browser
    /// keeps none of it.
    pub(super) fn restore_permissions(&mut self, page: Page, cx: &mut Context<Self>) {
        let kept: Vec<(String, Permission, bool)> = self
            .site_permissions()
            .all()
            .map(|(origin, permission, allowed)| (origin.to_owned(), permission, allowed))
            .collect();
        cx.background_spawn(async move {
            // Unasked, a browser without a window says no.
            for permission in Permission::ALL {
                warn_on(page.set_permission(None, permission, None).await);
            }
            for (origin, permission, allowed) in kept {
                warn_on(
                    page.set_permission(Some(&origin), permission, Some(allowed))
                        .await,
                );
            }
        })
        .detach();
    }

    pub(super) fn ask_permission(&mut self, request: PermissionRequest, cx: &mut Context<Self>) {
        let Some(origin) = origin(&self.url) else {
            // A file or a blank page has no site to keep an answer for.
            self.permit(request, String::new(), false, cx);
            return;
        };
        if let Some(answer) = self
            .site_permissions()
            .answer(&origin, &request.permissions)
        {
            self.permit(request, origin, answer, cx);
            return;
        }
        self.permits.waiting.push((request, origin));
        self.agent.asked += 1;
        cx.notify();
    }

    /// The request waiting, for the agent to read.
    pub(super) fn permission_question(&self) -> Option<String> {
        let (request, _) = self.permits.waiting.first()?;
        Some(format!(
            "{}.",
            question(host(&self.url), &request.permissions)
        ))
    }

    /// The agent's answer, which holds for this request only: what the site
    /// may use for good is the user's to say.
    pub(super) fn answer_permission_for_agent(
        &mut self,
        allow: bool,
        cx: &mut Context<Self>,
    ) -> Result<&'static str, String> {
        if self.permits.waiting.is_empty() {
            return Err("No permission request is waiting.".to_owned());
        }
        self.settle_permission(allow, false, cx);
        Ok(if allow {
            "Allowed, for this request."
        } else {
            "Blocked, for this request."
        })
    }

    /// The user's answer about the request asked about first.
    fn decide_permission(&mut self, allow: bool, cx: &mut Context<Self>) {
        self.settle_permission(allow, true, cx);
    }

    fn settle_permission(&mut self, allow: bool, keep: bool, cx: &mut Context<Self>) {
        if self.permits.waiting.is_empty() {
            return;
        }
        let (request, origin) = self.permits.waiting.remove(0);
        if keep {
            let kept = self.site_permissions();
            kept.decide(&origin, &request.permissions, allow);
            if let Err(error) = kept.save() {
                log::warn!("the site's permissions are not kept: {error:#}");
            }
        }
        self.permit(request, origin, allow, cx);
        // The answer may cover others waiting.
        for (request, _) in std::mem::take(&mut self.permits.waiting) {
            self.ask_permission(request, cx);
        }
        cx.notify();
    }

    fn permit(&self, request: PermissionRequest, origin: String, allow: bool, cx: &Context<Self>) {
        let Some(page) = self.page().cloned() else {
            return;
        };
        cx.background_spawn(async move { warn_on(page.permit(&request, &origin, allow).await) })
            .detach();
    }

    /// The document that asked is gone.
    pub(super) fn drop_permission_requests(&mut self) {
        self.permits.waiting.clear();
    }

    fn forget_site(&mut self, cx: &mut Context<Self>) {
        let Some(origin) = origin(&self.url) else {
            return;
        };
        let kept = self.site_permissions();
        let forgotten = kept.forget(&origin);
        if let Err(error) = kept.save() {
            log::warn!("the site's permissions are not kept: {error:#}");
        }
        if let Some(page) = self.page().cloned() {
            cx.background_spawn(ask_again(page, origin, forgotten))
                .detach();
        }
        cx.notify();
    }

    pub(super) fn show_notification(
        &self,
        title: &str,
        body: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let site = host(&self.url).to_owned();
        let message = match (title.trim(), body.trim()) {
            (title, "") => title.to_owned(),
            ("", body) => body.to_owned(),
            (title, body) => format!("{title}\n{body}"),
        };
        window.push_notification(Notification::info(message).title(site), cx);
    }

    pub(super) fn render_permission_question(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let (request, _) = self.permits.waiting.first()?;
        let theme = cx.theme();
        let buttons = h_flex()
            .justify_end()
            .gap_2()
            .child(
                Button::new("browser-permission-block")
                    .control()
                    .label("Block")
                    .on_click(cx.listener(|this, _, _, cx| this.decide_permission(false, cx))),
            )
            .child(
                Button::new("browser-permission-allow")
                    .primary()
                    .control()
                    .label("Allow")
                    .on_click(cx.listener(|this, _, _, cx| this.decide_permission(true, cx))),
            );
        let card =
            v_flex()
                .id("browser-permission-question")
                .test_support()
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
                        .child(question(host(&self.url), &request.permissions)),
                )
                .child(div().text_sm().child(
                    "Kiln keeps your answer for this site. The lock above the page changes it.",
                ))
                .child(buttons);
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

    /// The lock that shows what this site may use, while it was asked about
    /// anything.
    pub(super) fn site_permissions_button(
        &mut self,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let origin = origin(&self.url)?;
        let kept: Vec<(Permission, bool)> = self.site_permissions().of(&origin).collect();
        if kept.is_empty() {
            return None;
        }
        let this = cx.entity().downgrade();
        let trigger = Button::new("browser-site-permissions")
            .ghost()
            .control_icon(Lucide::Lock)
            .tooltip("Site Permissions");
        Some(
            Popover::new("browser-site-permissions-popover")
                .trigger(trigger)
                .content(move |_, _, cx| site_permissions_panel(&kept, this.clone(), cx)),
        )
    }
}

/// The site is asked about these again, as before it was ever answered.
async fn ask_again(page: Page, origin: String, permissions: Vec<Permission>) {
    for permission in permissions {
        warn_on(page.set_permission(Some(&origin), permission, None).await);
    }
}

fn site_permissions_panel(
    kept: &[(Permission, bool)],
    this: WeakEntity<Browser>,
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let rows = kept.iter().map(|(permission, allowed)| {
        let state = if *allowed { "Allowed" } else { "Blocked" };
        h_flex()
            .justify_between()
            .gap_4()
            .child(permission.title())
            .child(div().text_color(theme.muted_foreground).child(state))
    });
    v_flex()
        .id("browser-site-permissions-panel")
        .test_support()
        .gap_2()
        .text_sm()
        .children(rows)
        .child(
            Button::new("browser-site-permissions-reset")
                .control()
                .label("Ask Again Next Time")
                .on_click(move |_, _, cx| {
                    let _ = this.update(cx, Browser::forget_site);
                }),
        )
}

fn warn_on(done: anyhow::Result<()>) {
    if let Err(error) = done {
        log::warn!("the browser did not take a permission: {error:#}");
    }
}

/// `localhost:3000 wants to use your camera and your microphone`.
fn question(site: &str, permissions: &[Permission]) -> SharedString {
    if permissions == [Permission::Notifications] {
        return format!("{site} wants to show notifications").into();
    }
    let labels: Vec<&str> = permissions
        .iter()
        .map(|permission| permission.label())
        .collect();
    let listed = match labels.split_last() {
        Some((last, [])) => (*last).to_owned(),
        Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
        None => "something".to_owned(),
    };
    format!("{site} wants to use {listed}").into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_question_names_what_the_site_wants() {
        let both = [Permission::Camera, Permission::Microphone];
        assert_eq!(
            question("meet.example", &both).as_ref(),
            "meet.example wants to use your camera and your microphone"
        );
        let notify = [Permission::Notifications];
        assert_eq!(
            question("news.example", &notify).as_ref(),
            "news.example wants to show notifications"
        );
    }
}
