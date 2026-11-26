//! The Git page's Push, beside how far the branch is from its upstream. A
//! protected branch asks first.

use gpui_kit::prelude::*;
use gpui_kit::{
    Context, SharedString, Window,
    assets::IconName as Lucide,
    component::{
        Disableable as _, WindowExt as _,
        button::{Button, ButtonVariants as _},
    },
};
use kiln_chat::scale::ScaledButton as _;
use kiln_git::Status;
use kiln_settings::PushPolicy;

use super::{GitPage, committing::Done};
use crate::pushing;

impl GitPage {
    /// `None` when the settings say Kiln never pushes, or no branch is out.
    pub(super) fn render_push(
        &self,
        status: &Status,
        cx: &Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let push = self.settings(cx).git.push;
        if push.policy == PushPolicy::Never {
            return None;
        }
        let branch = status.branch.clone()?;
        let something = status.ahead > 0 || status.upstream.is_none();
        let tip: SharedString = match &status.upstream {
            Some(upstream) => format!("Push to {upstream}").into(),
            None => format!("Push to {}", push.remote).into(),
        };
        Some(
            Button::new("git-push")
                .ghost()
                .control_icon(Lucide::ArrowUp)
                .tooltip(tip)
                .disabled(!something || self.write.is_some())
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.ask_push(&branch, window, cx);
                })),
        )
    }

    fn ask_push(&mut self, branch: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !pushing::is_protected(branch, &self.settings(cx).git.push) {
            self.push(window, cx);
            return;
        }
        let view = cx.entity().downgrade();
        let title = format!("Push to {branch}?");
        let description = format!(
            "{branch} is a protected branch. Pushing sends its new commits to everyone \
             who works on it."
        );
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .confirm()
                .title(title.clone())
                .description(description.clone())
                .ok_text("Push")
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| this.push(window, cx));
                    true
                })
        });
    }

    fn push(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(status) = self.repo.read(cx).status() else {
            return;
        };
        let Some(branch) = status.branch.clone() else {
            return;
        };
        let upstream = status.upstream.clone();
        let remote = self.settings(cx).git.push.remote;
        self.write(window, cx, Done::Staging, move |root| {
            kiln_git::push(root, &branch, upstream.as_deref(), &remote)
        });
    }
}
