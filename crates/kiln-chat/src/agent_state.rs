//! What the chat knows about the agent itself, apart from the conversation:
//! the models it offers, the model, mode and effort in use, context usage
//! and the prompt cache. Kept across restarts so a new process starts with
//! the user's choices.

use std::{collections::HashMap, time::SystemTime};

use kiln_agent::{
    Account, AgentEvent, ContextUsage, Effort, Limited, ModelOption, PermissionMode,
    SessionCommand, SlashCommand, Usage,
};

use crate::prompt_cache::PromptCache;

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
    /// `None` until the agent reports it, or while the model takes none.
    effort: Option<Effort>,
    context: Option<ContextUsage>,
    cache: PromptCache,
    account: Option<Account>,
    usage: Usage,
    usage_updated_at: Option<SystemTime>,
    /// Usage last known for the accounts the chat moved away from, by
    /// account id; `None` is the usual sign-in.
    left_accounts: HashMap<Option<String>, (Usage, Option<SystemTime>)>,
    commands: Vec<SlashCommand>,
}

impl AgentState {
    /// Folds in `event`; `true` when something shown in the composer changed.
    pub fn apply(&mut self, event: &AgentEvent) -> bool {
        match event {
            AgentEvent::Ready(info) => {
                self.models.clone_from(&info.models);
                self.mode = Some(info.mode);
                self.account.clone_from(&info.account);
                self.commands.clone_from(&info.commands);
            }
            AgentEvent::CommandsChanged(commands) => self.commands.clone_from(commands),
            AgentEvent::ConversationReset => self.reset_context(),
            AgentEvent::Usage(usage) => {
                // A report may leave a window out; the last known value
                // stands until a report includes it.
                self.usage = Usage {
                    session: usage.session.or(self.usage.session),
                    weekly: usage.weekly.or(self.usage.weekly),
                    limited: usage.limited,
                };
                self.usage_updated_at = Some(SystemTime::now());
            }
            AgentEvent::Started { model, .. } | AgentEvent::ModelChanged(model) => {
                self.model = Some(model.clone());
            }
            AgentEvent::ModeChanged(mode) => self.mode = Some(*mode),
            AgentEvent::EffortChanged(effort) => self.effort = *effort,
            AgentEvent::PromptTokens(tokens) => self.cache.record(*tokens, SystemTime::now()),
            AgentEvent::ContextUsage(usage) => self.context = Some(usage.clone()),
            _ => return false,
        }
        true
    }

    /// What to ask the agent for after `event`, to keep the composer and
    /// header current.
    pub fn refreshes_after(event: &AgentEvent) -> &'static [SessionCommand] {
        match event {
            // A turn may have run `/effort` or `/model`.
            AgentEvent::Ready(_) | AgentEvent::TurnEnded(_) => &[
                SessionCommand::RefreshContext,
                SessionCommand::RefreshUsage,
                SessionCommand::RefreshEffort,
            ],
            // Each model has an effort of its own.
            AgentEvent::ModelChanged(_) => &[
                SessionCommand::RefreshContext,
                SessionCommand::RefreshEffort,
            ],
            AgentEvent::ConversationReset => &[SessionCommand::RefreshContext],
            _ => &[],
        }
    }

    /// The slash commands the agent runs, as it last listed them.
    pub fn commands(&self) -> &[SlashCommand] {
        &self.commands
    }

    pub fn account(&self) -> Option<&Account> {
        self.account.as_ref()
    }

    pub fn usage(&self) -> Usage {
        self.usage
    }

    /// When usage was last reported.
    pub fn usage_updated_at(&self) -> Option<SystemTime> {
        self.usage_updated_at
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

    /// Takes `usage`, reported at `updated_at` in another chat, unless this
    /// one has had a report of its own.
    pub fn recall_usage(&mut self, usage: Usage, updated_at: Option<SystemTime>) {
        if self.usage_updated_at.is_none() {
            self.usage = usage;
            self.usage_updated_at = updated_at;
        }
    }

    /// Forgets the conversation's usage; the models and choices stay.
    pub fn reset_context(&mut self) {
        self.context = None;
        self.cache = PromptCache::default();
    }

    /// Moves from account `from` to `to`: keeps how much of `from`'s plan
    /// is spent and brings back what was last known of `to`'s. The
    /// conversation and the choices stay.
    pub fn change_account(&mut self, from: Option<String>, to: Option<&str>) {
        self.account = None;
        let left = (
            std::mem::take(&mut self.usage),
            self.usage_updated_at.take(),
        );
        self.left_accounts.insert(from, left);
        if let Some((usage, updated_at)) = self.left_accounts.remove(&to.map(str::to_owned)) {
            self.usage = usage;
            self.usage_updated_at = updated_at;
        }
    }

    /// The limit account `id` was last known to be under at `now`, if the
    /// chat moved away from it.
    pub fn left_limit_at(&self, id: &str, now: SystemTime) -> Option<Limited> {
        self.left_accounts
            .get(&Some(id.to_owned()))
            .and_then(|(usage, _)| usage.limited)
            .filter(|limited| limited.until.is_none_or(|until| until > now))
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

    pub fn effort(&self) -> Option<Effort> {
        self.effort
    }

    /// The effort to start a new process with, once one is known.
    pub fn start_effort(&self) -> Option<Effort> {
        self.effort
    }

    /// The efforts the selected model takes.
    pub fn efforts(&self) -> &[Effort] {
        self.selected_model()
            .map_or(&[], |option| option.efforts.as_slice())
    }

    pub fn prompt_cache(&self) -> &PromptCache {
        &self.cache
    }

    pub fn models(&self) -> &[ModelOption] {
        &self.models
    }

    /// The option the composer shows as selected. The model the agent
    /// reports wins over the user's pick: the agent may refuse or replace a
    /// model the plan does not include. The pick shows when it resolves to
    /// that model, else a named model (not the "default" alias) that does.
    /// Before the agent names one, the pick or the default shows.
    pub fn selected_model(&self) -> Option<&ModelOption> {
        let by_id = |id: &str| self.models.iter().find(|option| option.id == id);
        let chosen = self.chosen_model.as_deref().and_then(by_id);
        let Some(model) = self.model.as_deref() else {
            return chosen.or_else(|| by_id(DEFAULT_MODEL));
        };
        if let Some(chosen) = chosen.filter(|option| option.resolved == model) {
            return Some(chosen);
        }
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

    /// Whole percent of the context window in use: the agent's own figure,
    /// so Kiln and the CLI agree; else worked out, rounded up so any use
    /// shows as at least 1%.
    pub fn context_percent(&self) -> Option<u64> {
        let usage = self.context.as_ref()?;
        usage.percent.or_else(|| {
            (usage.max_tokens > 0).then(|| (usage.used_tokens * 100).div_ceil(usage.max_tokens))
        })
    }

    pub fn context(&self) -> Option<&ContextUsage> {
        self.context.as_ref()
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

pub fn effort_label(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => "Low",
        Effort::Medium => "Medium",
        Effort::High => "High",
        Effort::Xhigh => "Extra high",
        Effort::Max => "Max",
    }
}

pub fn effort_description(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => "Quick, straightforward implementation",
        Effort::Medium => "Balanced approach with standard testing",
        Effort::High => "Comprehensive implementation with extensive testing",
        Effort::Xhigh => "Extended reasoning with thorough analysis",
        Effort::Max => "Maximum capability with deepest reasoning",
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
mod tests;
