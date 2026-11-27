//! Asking the agent for plan usage: every two minutes, when the usage panel
//! opens, and after events that change it. Usage spent elsewhere (another
//! session, claude.ai) shows only when asked for.

use std::time::{Duration, SystemTime};

use gpui_kit::{AsyncApp, Context, Task, WeakEntity};
use vortex_accounts::UsageAsk;
use vortex_agent::{AgentEvent, SessionCommand};

use super::ChatView;

const USAGE_POLL: Duration = Duration::from_mins(2);
/// Asked by the poll, an added Claude Code account's usage counts as fresh
/// this long: Claude's usage endpoint refuses an account asked too often,
/// and replies during turns keep the numbers current meanwhile.
const POLLED_FRESH_FOR: Duration = Duration::from_mins(10);
/// Asked by the user or after an event, usage this recent is still shown.
const ASKED_FRESH_FOR: Duration = Duration::from_secs(30);

pub(super) struct UsageAsks {
    _poll: Task<()>,
    /// A process started to ask for an added account's usage. The next ask
    /// replaces it, which ends one that never answered.
    apart: Option<Task<()>>,
}

impl UsageAsks {
    pub(super) fn new(cx: &mut Context<ChatView>) -> Self {
        Self {
            _poll: cx.spawn(poll),
            apart: None,
        }
    }
}

impl ChatView {
    /// Asks for the chat's plan usage. An added account's is asked as
    /// `Accounts::ask_usage` says: a Claude Code one in a process of its
    /// own, which needs no session of the chat's and answers at once.
    pub fn refresh_usage(&mut self, cx: &mut Context<Self>) {
        self.ask_usage(ASKED_FRESH_FOR, cx);
    }

    fn poll_usage(&mut self, cx: &mut Context<Self>) {
        self.ask_usage(POLLED_FRESH_FOR, cx);
    }

    fn ask_usage(&mut self, fresh_for: Duration, cx: &mut Context<Self>) {
        let ask = match &self.account {
            None => UsageAsk::Session,
            Some(id) => (self.accounts).update(cx, |accounts, _| {
                accounts.ask_usage(id, SystemTime::now(), fresh_for)
            }),
        };
        match ask {
            UsageAsk::Session => {
                self.send_command(SessionCommand::RefreshUsage);
            }
            UsageAsk::Apart(env) => self.ask_usage_apart(env, cx),
            UsageAsk::Fresh => {}
        }
    }

    fn ask_usage_apart(&mut self, env: Vec<(String, String)>, cx: &mut Context<Self>) {
        let asked = self.backend.ask_usage(env);
        let account = self.account.clone();
        self.usage_asks.apart = Some(cx.spawn(async move |this, cx| {
            let usage = asked.await;
            let _ = this.update(cx, |this, cx| match usage {
                // The chat may have moved to another account meanwhile.
                Ok(usage) if this.account == account => this.show_usage(usage, cx),
                Ok(_) => {}
                Err(error) => this.usage_refused(account.as_deref(), &error, cx),
            });
        }));
    }

    fn usage_refused(&self, account: Option<&str>, error: &anyhow::Error, cx: &mut Context<Self>) {
        log::warn!("cannot read the account's usage: {error:#}");
        if let Some(id) = account {
            (self.accounts).update(cx, |accounts, cx| {
                accounts.usage_refused(id, SystemTime::now());
                cx.notify();
            });
        }
        cx.notify();
    }

    /// Until when the chat's account's usage asks are refused.
    pub(super) fn usage_refused_until(&self, cx: &gpui_kit::App) -> Option<SystemTime> {
        let id = self.account.as_deref()?;
        self.accounts
            .read(cx)
            .usage_refused_until(id, SystemTime::now())
    }

    fn show_usage(&mut self, usage: vortex_agent::Usage, cx: &mut Context<Self>) {
        let event = AgentEvent::Usage(usage);
        self.agent.apply(&event);
        self.start_timers(&event, cx);
        cx.notify();
    }
}

/// Keeps the header's usage current while the chat sits idle.
async fn poll(this: WeakEntity<ChatView>, cx: &mut AsyncApp) {
    let mut alive = true;
    while alive {
        cx.background_executor().timer(USAGE_POLL).await;
        alive = this.update(cx, ChatView::poll_usage).is_ok();
    }
}
