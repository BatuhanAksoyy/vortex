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
    /// The asks for the other added accounts' usage, for the accounts
    /// panel; opening it again replaces them.
    others: Vec<Task<()>>,
}

impl UsageAsks {
    pub(super) fn new(cx: &mut Context<ChatView>) -> Self {
        Self {
            _poll: cx.spawn(poll),
            apart: None,
            others: Vec::new(),
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

    /// Asks for the usage of every added Claude Code account the chat does
    /// not run with, in a process of its own each, as the accounts panel
    /// shows it. As fresh as the poll needs, since the endpoint refuses an
    /// account asked too often; Codex reports an account's usage only in
    /// its sessions, so its accounts show what they last reported.
    pub(super) fn ask_others_usage(&mut self, cx: &mut Context<Self>) {
        let claude = if self.backend.name() == "claude" {
            self.backend.clone()
        } else {
            match self
                .backends
                .as_ref()
                .map(|backends| backends.get("claude"))
            {
                Some(backend) if backend.name() == "claude" => backend,
                _ => return,
            }
        };
        let now = SystemTime::now();
        let ids: Vec<String> = (self.accounts.read(cx).added_for("claude"))
            .map(|account| account.id.clone())
            .filter(|id| self.account.as_ref() != Some(id))
            .collect();
        let asks = ids.into_iter().filter_map(|id| {
            let ask = (self.accounts).update(cx, |accounts, _| {
                accounts.ask_usage(&id, now, POLLED_FRESH_FOR)
            });
            let UsageAsk::Apart(env) = ask else {
                return None;
            };
            Some(ask_other(claude.ask_usage(env), id, cx))
        });
        self.usage_asks.others = asks.collect();
    }

    fn keep_other_usage(
        &self,
        id: &str,
        usage: anyhow::Result<vortex_agent::Usage>,
        cx: &mut Context<Self>,
    ) {
        let usage = match usage {
            Ok(usage) => usage,
            Err(error) => return self.usage_refused(Some(id), &error, cx),
        };
        let known = vortex_accounts::KnownUsage {
            usage,
            reported_at: SystemTime::now(),
        };
        let kept = self.accounts.update(cx, |accounts, cx| {
            cx.notify();
            accounts.set_usage(id, known)
        });
        if let Err(error) = kept {
            log::warn!("cannot keep the account's usage: {error:#}");
        }
        cx.notify();
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

/// Waits for account `id`'s usage and keeps it.
fn ask_other(
    asked: vortex_agent::Pending<vortex_agent::Usage>,
    id: String,
    cx: &mut Context<ChatView>,
) -> Task<()> {
    cx.spawn(async move |this, cx| {
        let usage = asked.await;
        let _ = this.update(cx, |this, cx| this.keep_other_usage(&id, usage, cx));
    })
}

/// Keeps the header's usage current while the chat sits idle.
async fn poll(this: WeakEntity<ChatView>, cx: &mut AsyncApp) {
    let mut alive = true;
    while alive {
        cx.background_executor().timer(USAGE_POLL).await;
        alive = this.update(cx, ChatView::poll_usage).is_ok();
    }
}
