//! What the chat knows about the agent itself, apart from the conversation:
//! the models it offers, the model and mode in use, and context usage. Kept
//! across restarts so a new process starts with the user's choices.

use std::time::SystemTime;

use kiln_agent::{
    Account, AgentEvent, ContextUsage, Limited, ModelOption, PermissionMode, SessionCommand, Usage,
};

/// Modes offered in the composer's menu. `DontAsk` and `BypassPermissions`
/// are shown when the agent reports them but never offered.
pub const MENU_MODES: [PermissionMode; 4] = [
    PermissionMode::AskBeforeEdits,
    PermissionMode::EditAutomatically,
    PermissionMode::Plan,
    PermissionMode::Auto,
];

/// The model option that follows the user's own default.
const DEFAULT_MODEL: &str = "default";

#[derive(Default)]
pub struct AgentState {
    models: Vec<ModelOption>,
    /// The concrete model id in use, as the agent last reported it.
    model: Option<String>,
    /// The model option the user picked; passed to the next process.
    chosen_model: Option<String>,
    /// `None` until the agent reports it, so the first process starts in
    /// whatever mode the user's own settings say.
    mode: Option<PermissionMode>,
    context: Option<ContextUsage>,
    account: Option<Account>,
    usage: Usage,
}

impl AgentState {
    /// Folds in `event`; `true` when something shown in the composer changed.
    pub fn apply(&mut self, event: &AgentEvent) -> bool {
        match event {
            AgentEvent::Ready(info) => {
                self.models.clone_from(&info.models);
                self.mode = Some(info.mode);
                self.account.clone_from(&info.account);
            }
            AgentEvent::Usage(usage) => {
                // A report may leave a window out; the last known value
                // stands until a report includes it.
                self.usage = Usage {
                    session: usage.session.or(self.usage.session),
                    weekly: usage.weekly.or(self.usage.weekly),
                    limited: usage.limited,
                };
            }
            AgentEvent::Started { model, .. } | AgentEvent::ModelChanged(model) => {
                self.model = Some(model.clone());
            }
            AgentEvent::ModeChanged(mode) => self.mode = Some(*mode),
            AgentEvent::ContextUsage(usage) => self.context = Some(*usage),
            _ => return false,
        }
        true
    }

    /// What to ask the agent for after `event`, to keep the composer and
    /// header current.
    pub fn refreshes_after(event: &AgentEvent) -> &'static [SessionCommand] {
        match event {
            AgentEvent::Ready(_) | AgentEvent::TurnEnded(_) => {
                &[SessionCommand::RefreshContext, SessionCommand::RefreshUsage]
            }
            AgentEvent::ModelChanged(_) => &[SessionCommand::RefreshContext],
            _ => &[],
        }
    }

    pub fn account(&self) -> Option<&Account> {
        self.account.as_ref()
    }

    pub fn usage(&self) -> Usage {
        self.usage
    }

    /// The limit in force at `now`; one whose reset time has passed is over
    /// even before the agent reports again.
    pub fn limit_at(&self, now: SystemTime) -> Option<Limited> {
        self.usage
            .limited
            .filter(|limited| limited.until.is_none_or(|until| until > now))
    }

    /// Lifts the limit once its reset time has come.
    pub fn end_limit(&mut self) {
        self.usage.limited = None;
    }

    /// Forgets the conversation's usage; the models and choices stay.
    pub fn reset_context(&mut self) {
        self.context = None;
    }

    pub fn choose_model(&mut self, id: &str) {
        self.chosen_model = Some(id.to_owned());
    }

    pub fn chosen_model(&self) -> Option<&str> {
        self.chosen_model.as_deref()
    }

    pub fn mode(&self) -> PermissionMode {
        self.mode.unwrap_or_default()
    }

    /// The mode to start a new process in, once one is known.
    pub fn start_mode(&self) -> Option<PermissionMode> {
        self.mode
    }

    pub fn models(&self) -> &[ModelOption] {
        &self.models
    }

    /// The option the composer shows as selected: the one the user picked,
    /// else a named model (not the "default" alias) that resolves to the
    /// model in use, else the default. The agent names the model in use only
    /// once the first message starts a turn.
    pub fn selected_model(&self) -> Option<&ModelOption> {
        let by_id = |id: &str| self.models.iter().find(|option| option.id == id);
        if let Some(chosen) = &self.chosen_model {
            return by_id(chosen);
        }
        let Some(model) = self.model.as_deref() else {
            return by_id(DEFAULT_MODEL);
        };
        let resolves = |option: &&ModelOption| option.resolved == model;
        let named = self
            .models
            .iter()
            .filter(|option| option.id != DEFAULT_MODEL)
            .find(resolves);
        named.or_else(|| self.models.iter().find(resolves))
    }

    pub fn model_label(&self) -> Option<String> {
        self.selected_model()
            .map(|option| option.name.clone())
            .or_else(|| self.model.clone())
    }

    /// Whole percent of the context window in use, rounded up so any use
    /// shows as at least 1%.
    pub fn context_percent(&self) -> Option<u64> {
        let usage = self.context?;
        (usage.max_tokens > 0).then(|| (usage.used_tokens * 100).div_ceil(usage.max_tokens))
    }

    pub fn context(&self) -> Option<ContextUsage> {
        self.context
    }
}

pub fn mode_label(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::AskBeforeEdits => "Ask before edits",
        PermissionMode::EditAutomatically => "Edit automatically",
        PermissionMode::Plan => "Plan mode",
        PermissionMode::Auto => "Auto mode",
        PermissionMode::DontAsk => "Don't ask",
        PermissionMode::BypassPermissions => "Bypass permissions",
    }
}

pub fn mode_description(mode: PermissionMode) -> &'static str {
    match mode {
        PermissionMode::AskBeforeEdits => "Claude asks before each edit and most commands",
        PermissionMode::EditAutomatically => "Claude edits files without asking",
        PermissionMode::Plan => "Claude plans first and changes nothing until you approve",
        PermissionMode::Auto => "A classifier approves most actions instead of asking",
        PermissionMode::DontAsk => "Anything not already allowed is denied",
        PermissionMode::BypassPermissions => "Claude runs everything without asking",
    }
}

/// The mode after `mode` in the menu, for Shift+Tab.
pub fn next_mode(mode: PermissionMode) -> PermissionMode {
    let index = MENU_MODES
        .iter()
        .position(|m| *m == mode)
        .map_or(0, |i| i + 1);
    MENU_MODES[index % MENU_MODES.len()]
}

/// `15.3k` / `1M` style token counts.
pub fn tokens(count: u64) -> String {
    match count {
        0..1_000 => count.to_string(),
        1_000..1_000_000 => compact(count, 1_000, "k"),
        _ => compact(count, 1_000_000, "M"),
    }
}

fn compact(count: u64, unit: u64, suffix: &str) -> String {
    let tenths = count * 10 / unit;
    if tenths.is_multiple_of(10) {
        format!("{}{suffix}", tenths / 10)
    } else {
        format!("{}.{}{suffix}", tenths / 10, tenths % 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_agent::SessionInfo;

    fn option(id: &str, resolved: &str, name: &str) -> ModelOption {
        ModelOption {
            id: id.into(),
            resolved: resolved.into(),
            name: name.into(),
            description: String::new(),
        }
    }

    fn ready() -> AgentEvent {
        AgentEvent::Ready(SessionInfo {
            models: vec![
                option("default", "claude-opus-5-5", "Default (recommended)"),
                option("opus", "claude-opus-5-5", "Opus 5.5"),
                option("sonnet", "claude-sonnet-5-5", "Sonnet 5.5"),
            ],
            mode: PermissionMode::AskBeforeEdits,
            account: None,
        })
    }

    #[test]
    fn the_running_model_shows_by_its_own_name_not_the_default_alias() {
        let mut state = AgentState::default();
        state.apply(&ready());
        state.apply(&AgentEvent::Started {
            session_id: "s".into(),
            model: "claude-opus-5-5".into(),
        });
        assert_eq!(state.model_label().as_deref(), Some("Opus 5.5"));
        state.choose_model("default");
        assert_eq!(
            state.model_label().as_deref(),
            Some("Default (recommended)")
        );
    }

    #[test]
    fn before_the_first_turn_the_default_option_shows() {
        let mut state = AgentState::default();
        state.apply(&ready());
        assert_eq!(
            state.model_label().as_deref(),
            Some("Default (recommended)")
        );
    }

    #[test]
    fn an_unknown_model_shows_its_id() {
        let mut state = AgentState::default();
        state.apply(&AgentEvent::ModelChanged("claude-x".into()));
        assert_eq!(state.model_label().as_deref(), Some("claude-x"));
    }

    #[test]
    fn mode_is_unknown_until_reported() {
        let mut state = AgentState::default();
        assert_eq!(state.start_mode(), None);
        state.apply(&AgentEvent::ModeChanged(PermissionMode::Plan));
        assert_eq!(state.start_mode(), Some(PermissionMode::Plan));
    }

    #[test]
    fn context_percent_rounds_up() {
        let mut state = AgentState::default();
        assert_eq!(state.context_percent(), None);
        state.apply(&AgentEvent::ContextUsage(ContextUsage {
            used_tokens: 15_338,
            max_tokens: 1_000_000,
        }));
        assert_eq!(state.context_percent(), Some(2));
    }

    #[test]
    fn shift_tab_cycles_the_menu_modes() {
        assert_eq!(
            next_mode(PermissionMode::AskBeforeEdits),
            PermissionMode::EditAutomatically
        );
        assert_eq!(
            next_mode(PermissionMode::Auto),
            PermissionMode::AskBeforeEdits
        );
        assert_eq!(
            next_mode(PermissionMode::DontAsk),
            PermissionMode::AskBeforeEdits
        );
    }

    #[test]
    fn a_partial_report_keeps_the_other_window() {
        let window = |percent| kiln_agent::UsageWindow {
            percent,
            resets_at: None,
        };
        let mut state = AgentState::default();
        state.apply(&AgentEvent::Usage(Usage {
            session: Some(window(4)),
            weekly: Some(window(19)),
            limited: None,
        }));
        state.apply(&AgentEvent::Usage(Usage {
            session: Some(window(5)),
            weekly: None,
            limited: None,
        }));
        assert_eq!(state.usage().weekly, Some(window(19)));
        assert_eq!(state.usage().session, Some(window(5)));
    }

    #[test]
    fn a_limit_ends_at_its_reset_time() {
        let now = SystemTime::now();
        let hour = std::time::Duration::from_secs(3600);
        let mut state = AgentState::default();
        state.apply(&AgentEvent::Usage(Usage {
            limited: Some(Limited {
                until: Some(now + hour),
            }),
            ..Usage::default()
        }));
        assert!(state.limit_at(now).is_some());
        assert!(state.limit_at(now + hour).is_none());
    }

    #[test]
    fn token_counts_are_compact() {
        assert_eq!(tokens(950), "950");
        assert_eq!(tokens(15_338), "15.3k");
        assert_eq!(tokens(200_000), "200k");
        assert_eq!(tokens(1_000_000), "1M");
    }
}
