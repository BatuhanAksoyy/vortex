//! Choosing a mode from the menu. Bypassing permissions is asked about
//! first, as nothing the agent does is asked about after it.

use gpui_kit::{
    Context, Window,
    component::{WindowExt as _, button::ButtonVariant},
};
use kiln_agent::PermissionMode;

use super::ChatView;

const WARNING: &str = "The agent will run commands, change and delete files, and use the network \
                       without asking. Use it only in a sandbox or where you can lose the data.";

impl ChatView {
    pub(super) fn choose_mode(
        &mut self,
        mode: PermissionMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if mode != PermissionMode::BypassPermissions {
            self.set_mode(mode, cx);
            return;
        }
        if self.mode() == mode {
            return;
        }
        let view = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let view = view.clone();
            alert
                .title("Bypass permissions in this chat?")
                .description(WARNING)
                .confirm()
                .ok_text("Bypass Permissions")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    let _ = view.update(cx, |view, cx| view.set_mode(mode, cx));
                    true
                })
        });
    }
}
