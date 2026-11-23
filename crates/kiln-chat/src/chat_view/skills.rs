//! The skills panel the composer strip opens: what it asks the agent for,
//! and how a state chosen in it is saved and confirmed.

use std::time::Duration;

use gpui_kit::{Context, Window};
use kiln_agent::{AgentEvent, SessionCommand, SkillState};

use super::{ChatView, StripPanel};

/// How long the CLI may take to pick up a saved setting before it is read
/// again.
const CHECK_DELAY: Duration = Duration::from_millis(300);

impl ChatView {
    /// Opens or closes the skills panel; opening asks for a fresh listing.
    /// The skills come from the agent, so a chat whose process ended starts
    /// one, as a message would.
    pub(super) fn set_skills_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_panel(StripPanel::Skills, open) {
            self.skill_menu.clear_messages();
            if let Some(commands) = self.ensure_session(window, cx) {
                let _ = commands.try_send(SessionCommand::RefreshSkills);
            }
        }
        cx.notify();
    }

    pub(super) fn set_skill_state(
        &mut self,
        name: &str,
        state: SkillState,
        cx: &mut Context<Self>,
    ) {
        if !self.skill_menu.begin_save(name, state) {
            return;
        }
        let change = SessionCommand::SetSkillState {
            name: name.to_owned(),
            state,
        };
        if !self.send_command(change) {
            self.skill_menu.fail(name, "Claude is not running.");
        }
        cx.notify();
    }

    /// Folds the agent's skill news into the panel.
    pub(super) fn update_skills(&mut self, event: &AgentEvent, cx: &mut Context<Self>) {
        match event {
            AgentEvent::Skills(skills) => {
                if self.skill_menu.apply(skills.clone()) {
                    self.check_skills_later(cx);
                }
            }
            AgentEvent::SkillStateFailed { name, error } => self.skill_menu.fail(name, error),
            // A skill turned on or off adds or drops its command; the
            // listing may have changed with it.
            AgentEvent::CommandsChanged(_) if self.is_open(StripPanel::Skills) => {
                self.send_command(SessionCommand::RefreshSkills);
                return;
            }
            _ => return,
        }
        cx.notify();
    }

    fn check_skills_later(&mut self, cx: &mut Context<Self>) {
        self.skill_check = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CHECK_DELAY).await;
            let _ = this.update(cx, |this, _| {
                this.send_command(SessionCommand::RefreshSkills)
            });
        }));
    }
}
