//! The permission rules panel the composer strip opens: what it asks the
//! agent for, and a removal until the agent lists the rules again.

use gpui_kit::{Context, Window};
use kiln_agent::{AgentEvent, PermissionRule, SessionCommand};

use super::{ChatView, StripPanel};

impl ChatView {
    /// Opens or closes the rules panel; opening asks for a fresh listing.
    /// The rules come from the agent, so a chat whose process ended starts
    /// one, as a message would.
    pub(super) fn set_rules_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_panel(StripPanel::Rules, open) {
            self.rule_list.clear_messages();
            if let Some(commands) = self.ensure_session(window, cx) {
                let _ = commands.try_send(SessionCommand::RefreshRules);
            }
        }
        cx.notify();
    }

    pub(super) fn remove_rule(&mut self, rule: &PermissionRule, cx: &mut Context<Self>) {
        if !self.rule_list.begin_remove(rule) {
            return;
        }
        if !self.send_command(SessionCommand::RemoveRule(rule.clone())) {
            self.rule_list.fail("Claude is not running.");
        }
        cx.notify();
    }

    /// Folds the agent's rule news into the panel.
    pub(super) fn update_rules(&mut self, event: &AgentEvent, cx: &mut Context<Self>) {
        match event {
            AgentEvent::Rules(rules) => self.rule_list.apply(rules.clone()),
            AgentEvent::RuleRemovalFailed { error, .. } => self.rule_list.fail(error),
            _ => return,
        }
        cx.notify();
    }
}
