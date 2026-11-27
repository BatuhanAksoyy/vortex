//! The account button in the header and the panel it opens: the accounts
//! Vortex knows, under the harness each signs in to, which one the chat runs
//! with, and adding another. Picking another harness's account hands the
//! conversation over to it. Drawn like the usage panel beside it.

use std::time::SystemTime;

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, App, Context, Hsla, SharedString, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme, Icon, IconName, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        tooltip::Tooltip,
        v_flex,
    },
    div, px,
};

use super::ChatView;
use crate::{
    backends::title,
    scale::{ScaledButton as _, ScaledIcon as _, ScaledText as _},
    transcript::Status,
};

const PANEL_WIDTH: f32 = 280.;
const TRIGGER_MAX_WIDTH: f32 = 240.;

/// One account as the panel lists it.
struct Row {
    id: String,
    /// `None` for a harness's usual sign-in.
    account: Option<String>,
    agent: &'static str,
    name: String,
    detail: String,
}

/// One harness's accounts.
struct Group {
    agent: &'static str,
    rows: Vec<Row>,
}

/// What the panel shows, copied out of the view each render.
struct Shown {
    /// The chat's own harness first.
    groups: Vec<Group>,
    agent: &'static str,
    /// `None` while the chat runs with the usual sign-in.
    current: Option<String>,
    /// A turn is running; switching would cut it off.
    busy: bool,
    signing_in: bool,
    error: Option<String>,
    notice: Option<String>,
}

impl ChatView {
    pub(super) fn account_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let shown = self.shown_accounts(cx);
        let trigger = Button::new("chat-account")
            .ghost()
            .control()
            .px_1p5()
            .max_w(px(TRIGGER_MAX_WIDTH))
            .label(self.current_account_label(cx))
            .tooltip("Accounts");
        let view = cx.entity().downgrade();
        let opener = view.clone();
        Popover::new("chat-account-popover")
            .anchor(Anchor::TopRight)
            .trigger(trigger)
            .open(self.accounts_panel.open)
            .on_open_change(move |open, _, cx| {
                let _ = opener.update(cx, |view, cx| view.set_accounts_open(*open, cx));
            })
            .content(move |_, _, cx| panel(&shown, &view, cx))
    }

    /// The account in use and its plan, e.g. "dev@example.com · Pro".
    /// An added account is named as its sign-in reported it: every account
    /// shares one Claude Code profile, whose account details describe
    /// whichever account signed in last.
    fn current_account_label(&self, cx: &App) -> String {
        let both = |email: &str, plan: &str| (email.to_owned(), plan.to_owned());
        let (email, plan) = match &self.account {
            Some(id) => self.accounts.read(cx).find(id).map_or_else(
                || (id.clone(), String::new()),
                |account| both(&account.email, &account.plan),
            ),
            None => self
                .agent
                .account()
                .map_or_else(Default::default, |account| {
                    both(&account.email, &account.plan)
                }),
        };
        let plan = short_plan(&plan);
        match (email.is_empty(), plan.is_empty()) {
            (true, _) => "Account".to_owned(),
            (false, true) => email,
            (false, false) => format!("{email} · {plan}"),
        }
    }

    /// On the usage limit notice: carry on with the first other account
    /// not known to be limited too, else with another harness.
    pub(super) fn limit_offer(&self, now: SystemTime, cx: &Context<Self>) -> Option<Button> {
        let accounts = self.accounts.read(cx);
        let mut added = accounts.added_for(self.backend.name());
        let other = added.find(|account| {
            self.account.as_deref() != Some(account.id.as_str())
                && self.agent.left_limit_at(&account.id, now).is_none()
        });
        let (agent, id, label) = if let Some(account) = other {
            let label = format!("Continue with {}", account.email);
            (self.backend.name(), Some(account.id.clone()), label)
        } else {
            let agent = *self.offered_agents(cx).get(1)?;
            let id = super::first_added(accounts, agent);
            (agent, id, format!("Continue in {}", title(agent)))
        };
        Some(
            Button::new("chat-limit-continue")
                .primary()
                .small()
                .label(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.pick_account(agent, id.clone(), window, cx);
                })),
        )
    }

    fn shown_accounts(&self, cx: &App) -> Shown {
        let accounts = self.accounts.read(cx);
        let groups = self.offered_agents(cx).into_iter().map(|agent| Group {
            agent,
            rows: (accounts.added_for(agent))
                .map(|account| Row {
                    id: account.id.clone(),
                    account: Some(account.id.clone()),
                    agent,
                    name: account.email.clone(),
                    detail: short_plan(&account.plan).to_owned(),
                })
                .collect(),
        });
        let panel = &self.accounts_panel;
        Shown {
            groups: groups.collect(),
            agent: self.backend.name(),
            current: self.account.clone(),
            busy: self.status() == Status::Running,
            signing_in: panel.signing_in.is_some(),
            error: panel.error.clone(),
            notice: panel.notice.clone(),
        }
    }
}

fn panel(shown: &Shown, view: &WeakEntity<ChatView>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    v_flex()
        .id("chat-account-panel")
        .test_support()
        .w(px(PANEL_WIDTH))
        .gap_1()
        .text_sm()
        .child("Accounts")
        .when(shown.current.is_none(), |this| {
            this.child(message(
                "chat-account-usual",
                &format!(
                    "Using your usual {} sign-in. Accounts you add here are used instead.",
                    title(shown.agent)
                ),
                muted,
            ))
        })
        .children(
            shown
                .groups
                .iter()
                .map(|group| render_group(group, shown, view, cx)),
        )
        .child(div().my_1().h_px().bg(theme.border))
        .children(
            shown
                .error
                .as_deref()
                .map(|text| message("chat-account-error", text, theme.danger)),
        )
        .children(
            shown
                .notice
                .as_deref()
                .map(|text| message("chat-account-notice", text, muted)),
        )
        .child(add_control(shown, view, cx))
}

/// A harness's accounts under its name, when there are several harnesses;
/// another harness with none offers its usual sign-in.
fn render_group(
    group: &Group,
    shown: &Shown,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    let several = shown.groups.len() > 1;
    let usual = (group.agent != shown.agent && group.rows.is_empty()).then(|| Row {
        id: format!("usual-{}", group.agent),
        account: None,
        agent: group.agent,
        name: format!("Usual {} sign-in", title(group.agent)),
        detail: String::new(),
    });
    v_flex()
        .gap_1()
        .when(several, |this| {
            this.child(
                div()
                    .mt_1()
                    .text_caption()
                    .text_color(cx.theme().muted_foreground)
                    .child(title(group.agent).to_owned()),
            )
        })
        .children(
            group
                .rows
                .iter()
                .map(|row| account_row(row, shown, view, cx)),
        )
        .children(usual.map(|row| account_row(&row, shown, view, cx)))
}

fn account_row(
    row: &Row,
    shown: &Shown,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let current = row.agent == shown.agent && shown.current == row.account;
    let (agent, account) = (row.agent, row.account.clone());
    let (view, busy) = (view.clone(), shown.busy);
    h_flex()
        .id(SharedString::from(format!("chat-account-{}", row.id)))
        .test_support()
        .aria_label(SharedString::from(row.name.clone()))
        .gap_2()
        .px_1()
        .py_0p5()
        .rounded(theme.radius)
        .child(div().flex_none().w(px(16.)).when(current, |this| {
            this.child(Icon::new(IconName::Check).in_row())
        }))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().truncate().child(row.name.clone()))
                .child(
                    div()
                        .truncate()
                        .text_caption()
                        .text_color(theme.muted_foreground)
                        .child(row.detail.clone()),
                ),
        )
        .when(!current && !busy, |this| {
            this.cursor_pointer()
                .hover(|style| style.bg(theme.accent))
                .on_click(move |_, window, cx| {
                    let _ = view.update(cx, |view, cx| {
                        view.set_accounts_open(false, cx);
                        view.pick_account(agent, account.clone(), window, cx);
                    });
                })
        })
        .when(!current && busy, |this| {
            this.text_color(theme.muted_foreground)
                .tooltip(|window, cx| {
                    Tooltip::new("Switch when the agent has finished").build(window, cx)
                })
        })
}

fn add_control(shown: &Shown, view: &WeakEntity<ChatView>, cx: &App) -> gpui_kit::AnyElement {
    let view = view.clone();
    if shown.signing_in {
        return h_flex()
            .gap_2()
            .justify_between()
            .child(div().flex_1().min_w_0().child(message(
                "chat-account-waiting",
                "Finish signing in in your browser.",
                cx.theme().muted_foreground,
            )))
            .child(
                Button::new("chat-account-cancel")
                    .ghost()
                    .small()
                    .flex_none()
                    .label("Cancel")
                    .on_click(move |_, _, cx| {
                        let _ = view.update(cx, ChatView::cancel_sign_in);
                    }),
            )
            .into_any_element();
    }
    let several = shown.groups.len() > 1;
    let buttons = shown.groups.iter().map(|group| {
        let (agent, view) = (group.agent, view.clone());
        // The chat's own harness keeps the plain id.
        let id: SharedString = if agent == shown.agent {
            "chat-account-add".into()
        } else {
            format!("chat-account-add-{agent}").into()
        };
        let label: SharedString = if several {
            format!("Add {} account", title(agent)).into()
        } else {
            "Add account".into()
        };
        Button::new(id)
            .ghost()
            .row_labelled(Icon::new(IconName::Plus), label)
            .on_click(move |_, _, cx| {
                let _ = view.update(cx, |view, cx| view.add_account(agent, cx));
            })
    });
    v_flex().children(buttons).into_any_element()
}

/// A status line, found in tests by its text.
fn message(id: &'static str, text: &str, color: Hsla) -> impl IntoElement + use<> {
    div()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(text.to_owned()))
        .text_caption()
        .text_color(color)
        .child(text.to_owned())
}

/// "Claude Max" → "Max".
fn short_plan(plan: &str) -> &str {
    plan.strip_prefix("Claude ").unwrap_or(plan)
}
