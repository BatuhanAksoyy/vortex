//! Which account the chat runs with: moving the conversation to another
//! account, and adding one by signing in through the browser.

use gpui_kit::{AppContext as _, Context, Task, Window};
use kiln_accounts::{Account, Accounts, KnownUsage};

use super::ChatView;
use crate::transcript::Status;

/// A sign-in in progress; dropping it ends the CLI and with it the sign-in.
pub(super) struct PendingSignIn {
    id: String,
    _url: Task<()>,
    _done: Task<()>,
}

/// The header's accounts panel.
#[derive(Default)]
pub(super) struct AccountsPanel {
    pub(super) open: bool,
    pub(super) signing_in: Option<PendingSignIn>,
    pub(super) error: Option<String>,
    pub(super) notice: Option<String>,
}

impl ChatView {
    /// Keeps the usage an added account just reported, for the other chats
    /// and the next start. The usual sign-in is asked for its usage instead.
    pub(super) fn remember_usage(&self, cx: &mut Context<Self>) {
        let (Some(id), Some(reported_at)) = (&self.account, self.agent.usage_updated_at()) else {
            return;
        };
        let known = KnownUsage {
            usage: self.agent.usage(),
            reported_at,
        };
        let kept = self.accounts.update(cx, |accounts, cx| {
            let kept = accounts.set_usage(id, known);
            cx.notify();
            kept
        });
        if let Err(error) = kept {
            log::warn!("cannot keep the account's usage: {error:#}");
        }
    }

    /// Shows the usage this chat's account last reported, in any chat.
    pub(crate) fn recall_usage(&mut self, cx: &Context<Self>) {
        let known = self
            .account
            .as_ref()
            .and_then(|id| self.accounts.read(cx).usage(id));
        if let Some(known) = known {
            self.agent
                .recall_usage(known.usage, Some(known.reported_at));
        }
    }

    /// The added account the chat runs with, by id; `None` is the usual
    /// sign-in.
    #[must_use]
    pub fn account(&self) -> Option<&str> {
        self.account.as_deref()
    }

    /// Runs the chat with added account `id` from its first process, as
    /// when a kept layout reopens it. Ignored once a process has started,
    /// and for an account no longer added.
    pub fn start_with_account(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.session.is_some() || self.accounts.read(cx).find(id).is_none() {
            return;
        }
        let left = self.account.replace(id.to_owned());
        self.agent.change_account(left, Some(id));
        self.recall_usage(cx);
        cx.notify();
    }

    pub(super) fn set_accounts_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if open && !self.accounts_panel.open {
            self.accounts_panel.error = None;
            self.accounts_panel.notice = None;
        }
        self.accounts_panel.open = open;
        cx.notify();
    }

    /// Continues the conversation with account `id`: the process restarts
    /// with that account's sign-in and resumes the session, and a turn the
    /// usage limit cut off is taken up again. Not while a turn runs, which
    /// the restart would cut off.
    pub(super) fn switch_account(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.account.as_deref() == Some(id) || self.status() == Status::Running {
            return;
        }
        let label = self.account_label(id, cx);
        let left = self.account.replace(id.to_owned());
        self.agent.change_account(left, Some(id));
        self.recall_usage(cx);
        self.session = None;
        self.sync_limit_timer(cx);
        let change = self.transcript.push_notice(format!("Switched to {label}"));
        self.apply_change(change, cx);
        self.ensure_session(window, cx);
        if std::mem::take(&mut self.cut_by_limit) {
            self.submit(super::handoff::CARRY_ON.into(), window, cx);
        }
        cx.emit(super::ChatEvent::AccountChanged);
        cx.notify();
    }

    /// Leaves the usual sign-in for the first added account once there is
    /// one, between turns.
    pub(super) fn adopt_added_account(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.account.is_some() {
            return;
        }
        let first = first_added(self.accounts.read(cx), self.backend.name());
        if let Some(id) = first {
            self.switch_account(&id, window, cx);
        }
    }

    /// Signs a new account of harness `agent` in; the CLI's sign-in page
    /// opens in the browser.
    pub(super) fn add_account(&mut self, agent: &str, cx: &mut Context<Self>) {
        if self.accounts_panel.signing_in.is_some() {
            return;
        }
        self.accounts_panel.error = None;
        self.accounts_panel.notice = None;
        match self.start_sign_in(agent, cx) {
            Ok(pending) => self.accounts_panel.signing_in = Some(pending),
            Err(error) => {
                self.accounts_panel.error = Some(format!("Cannot add an account: {error:#}"));
            }
        }
        cx.notify();
    }

    fn start_sign_in(&self, agent: &str, cx: &mut Context<Self>) -> anyhow::Result<PendingSignIn> {
        let backend = match &self.backends {
            Some(backends) => backends.get(agent),
            None => self.backend.clone(),
        };
        let agent = backend.name();
        let (id, env) = self.accounts.read(cx).reserve(agent)?;
        let sign_in = match backend.sign_in(env) {
            Ok(sign_in) => sign_in,
            Err(error) => {
                self.accounts.read(cx).release(&id);
                return Err(error);
            }
        };
        let url = sign_in.url;
        let opener = cx.spawn(async move |_, cx| {
            if let Ok(url) = url.recv().await {
                cx.update(|cx| cx.open_url(&url));
            }
        });
        let signing_in = cx.background_spawn(sign_in.done);
        let finished = id.clone();
        let done = cx.spawn(async move |this, cx| {
            let result = signing_in.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_sign_in(&finished, agent, result, cx);
            });
        });
        Ok(PendingSignIn {
            id,
            _url: opener,
            _done: done,
        })
    }

    fn finish_sign_in(
        &mut self,
        id: &str,
        agent: &str,
        result: anyhow::Result<kiln_agent::Account>,
        cx: &mut Context<Self>,
    ) {
        self.accounts_panel.signing_in = None;
        let added = result.and_then(|signed_in| {
            self.accounts.update(cx, |accounts, cx| {
                let kept = keep(accounts, id, agent, signed_in);
                cx.notify();
                kept
            })
        });
        match added {
            Ok(notice) => self.accounts_panel.notice = Some(notice),
            Err(error) => {
                self.accounts.read(cx).release(id);
                self.accounts_panel.error = Some(format!("The sign-in failed: {error:#}"));
            }
        }
        cx.notify();
    }

    pub(super) fn cancel_sign_in(&mut self, cx: &mut Context<Self>) {
        if let Some(pending) = self.accounts_panel.signing_in.take() {
            self.accounts.read(cx).release(&pending.id);
        }
        cx.notify();
    }

    /// The name the panel and the transcript give added account `id`.
    pub(super) fn account_label(&self, id: &str, cx: &gpui_kit::App) -> String {
        self.accounts
            .read(cx)
            .find(id)
            .map_or_else(|| id.to_owned(), |account| account.email.clone())
    }
}

/// The id of the first account added for harness `agent`.
pub(crate) fn first_added(accounts: &Accounts, agent: &str) -> Option<String> {
    accounts
        .added_for(agent)
        .next()
        .map(|account| account.id.clone())
}

/// Adds the account signed in under `id`, unless it is already added; what
/// to tell the user.
fn keep(
    accounts: &mut Accounts,
    id: &str,
    agent: &str,
    signed_in: kiln_agent::Account,
) -> anyhow::Result<String> {
    if accounts.find_email(agent, &signed_in.email).is_some() {
        accounts.release(id);
        return Ok(format!("{} is already added.", signed_in.email));
    }
    let notice = format!("Added {}.", signed_in.email);
    accounts.add(Account {
        id: id.to_owned(),
        email: signed_in.email,
        plan: signed_in.plan,
        agent: agent.to_owned(),
    })?;
    Ok(notice)
}
