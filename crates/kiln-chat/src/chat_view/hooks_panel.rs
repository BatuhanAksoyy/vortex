//! The hooks button in the composer strip and the panel it opens: each hook
//! under its event, where it is kept, and a button to remove the ones kept
//! in a settings file. Drawn like the skills panel; adding a hook swaps the
//! list for a form.

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, App, Context, Hsla, SharedString, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme, Disableable, Icon, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        tooltip::Tooltip,
        v_flex,
    },
    div, px,
};
use kiln_agent::{Hook, HookEvent};

use super::{
    ChatView, StripPanel,
    hook_form::{self, FormShown},
};
use crate::hook_list::scope_label;

const PANEL_WIDTH: f32 = 320.;
const LIST_MAX_HEIGHT: f32 = 360.;

const EMPTY: &str = "No hooks yet. A hook runs a command when something happens in a session, \
                     such as before a tool runs.";

/// What the panel shows, copied out of the view each render.
struct Shown {
    groups: Vec<(String, Vec<Hook>)>,
    events: Vec<HookEvent>,
    busy: bool,
    error: Option<String>,
    notice: Option<String>,
    locked: Option<String>,
    loaded: bool,
    form: Option<FormShown>,
}

impl ChatView {
    pub(super) fn hooks_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let view = cx.entity().downgrade();
        let list = &self.hooks.list;
        let shown = Shown {
            groups: list.by_event(),
            events: list.events().to_vec(),
            busy: list.is_busy(),
            error: list.error().map(str::to_owned),
            notice: list.notice().map(str::to_owned),
            locked: list.locked().map(str::to_owned),
            loaded: list.is_loaded(),
            form: self.hook_form_shown(),
        };
        let trigger = Button::new("chat-hooks")
            .ghost()
            .small()
            .icon(Icon::new(gpui_kit::assets::IconName::FishingHook))
            .tooltip("Hooks");
        let opener = view.clone();
        Popover::new("chat-hooks-popover")
            .anchor(Anchor::BottomLeft)
            .trigger(trigger)
            .open(self.is_open(StripPanel::Hooks))
            .on_open_change(move |open, window, cx| {
                let _ = opener.update(cx, |view, cx| view.set_hooks_open(*open, window, cx));
            })
            .content(move |_, _, cx| panel(&shown, &view, cx))
    }

    fn hook_form_shown(&self) -> Option<FormShown> {
        let form = self.hooks.form.clone()?;
        let event = self
            .hooks
            .list
            .events()
            .iter()
            .find(|event| event.name == form.event);
        Some(FormShown {
            summary: event.map(|event| event.summary.clone()),
            takes_matcher: event.is_none_or(|event| event.takes_matcher),
            form,
            event: self.hooks.event.clone(),
            scope: self.hooks.scope.clone(),
            matcher: self.hooks.matcher.clone(),
            command: self.hooks.command.clone(),
        })
    }
}

fn panel(shown: &Shown, view: &WeakEntity<ChatView>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let muted = theme.muted_foreground;
    let count: usize = shown.groups.iter().map(|(_, hooks)| hooks.len()).sum();
    let messages = [
        shown
            .error
            .as_deref()
            .map(|text| ("chat-hooks-error", text, theme.danger)),
        shown
            .notice
            .as_deref()
            .map(|text| ("chat-hooks-notice", text, muted)),
        shown
            .locked
            .as_deref()
            .map(|text| ("chat-hooks-locked", text, muted)),
    ];
    let body = match &shown.form {
        Some(form) => hook_form::render(form, shown.busy, view, cx).into_any_element(),
        None => list(shown, count, view, cx).into_any_element(),
    };
    v_flex()
        .id("chat-hooks-panel")
        .test_support()
        .w(px(PANEL_WIDTH))
        .gap_1()
        .text_sm()
        .child(header(shown, count, view, muted))
        .children(
            messages
                .into_iter()
                .flatten()
                .map(|(id, text, color)| message(id, text, color)),
        )
        .child(body)
}

/// The title, and on its trailing edge either the count or, while the list
/// shows, the command that adds a hook.
fn header(
    shown: &Shown,
    count: usize,
    view: &WeakEntity<ChatView>,
    muted: Hsla,
) -> impl IntoElement + use<> {
    let title = match &shown.form {
        Some(form) if form.form.editing.is_some() => "Edit hook",
        Some(_) => "New hook",
        None => "Hooks",
    };
    let can_add = shown.loaded && shown.locked.is_none() && shown.form.is_none();
    let view = view.clone();
    h_flex()
        .h_6()
        .justify_between()
        .child(title)
        .when(can_add, |this| {
            this.child(
                Button::new("hook-add")
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(gpui_kit::assets::IconName::Plus))
                    .label("Add")
                    .disabled(shown.busy)
                    .on_click(move |_, window, cx| {
                        let _ = view.update(cx, |view, cx| view.open_hook_form(window, cx));
                    }),
            )
        })
        .when(!can_add && shown.form.is_none() && count > 0, |this| {
            this.child(div().text_xs().text_color(muted).child(count.to_string()))
        })
}

fn list(
    shown: &Shown,
    count: usize,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    v_flex()
        .id("chat-hooks-list")
        .max_h(px(LIST_MAX_HEIGHT))
        .overflow_y_scroll()
        .gap_1()
        .when(!shown.loaded, |this| {
            this.child(message("chat-hooks-loading", "Loading hooks…", muted))
        })
        .when(shown.loaded && count == 0, |this| {
            this.child(message("chat-hooks-empty", EMPTY, muted))
        })
        .children(
            shown
                .groups
                .iter()
                .map(|(event, hooks)| group(event, hooks, shown, view, cx)),
        )
}

/// A status line, found in tests by its text.
fn message(id: &'static str, text: &str, color: Hsla) -> impl IntoElement + use<> {
    div()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(text.to_owned()))
        .text_xs()
        .text_color(color)
        .child(text.to_owned())
}

/// An event's hooks under its name, which says when it happens on hover.
fn group(
    event: &str,
    hooks: &[Hook],
    shown: &Shown,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    let summary = shown
        .events
        .iter()
        .find(|known| known.name == event)
        .map(|known| SharedString::from(known.summary.clone()));
    let label = SharedString::from(event.to_owned());
    v_flex()
        .child(
            div()
                .id(SharedString::from(format!("hooks-group-{event}")))
                .test_support()
                .aria_label(label.clone())
                .pt_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(label)
                .when_some(summary, |this, summary| {
                    this.tooltip(move |window, cx| Tooltip::new(summary.clone()).build(window, cx))
                }),
        )
        .children(hooks.iter().map(|hook| hook_row(hook, shown, view, cx)))
}

/// What runs, over what it matches and where it is kept.
fn hook_row(
    hook: &Hook,
    shown: &Shown,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let key = hook_key(hook);
    let detail = detail(hook);
    h_flex()
        .id(SharedString::from(format!("hook-{key}")))
        .test_support()
        .aria_label(SharedString::from(hook.text.clone()))
        .gap_2()
        .py_1()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .font_family(theme.mono_font_family.clone())
                        .when(hook.disabled, |this| {
                            this.text_color(theme.muted_foreground)
                        })
                        .child(hook.text.clone()),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("hook-detail-{key}")))
                        .test_support()
                        .aria_label(SharedString::from(detail.clone()))
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(detail),
                ),
        )
        .when(hook.stored.is_some() && shown.locked.is_none(), |this| {
            this.child(row_actions(hook, &key, shown.busy, view))
        })
}

/// What the hook matches, where it is kept, and why it may not run. A hook
/// in a settings file is named by scope; others by the agent's own words.
fn detail(hook: &Hook) -> String {
    let source = hook.stored.as_ref().map_or_else(
        || hook.source.clone(),
        |stored| scope_label(stored.scope).to_owned(),
    );
    [
        (!hook.matcher.is_empty()).then(|| hook.matcher.clone()),
        Some(source),
        (hook.kind != "command").then(|| hook.kind.clone()),
        hook.disabled.then(|| "Not running".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ")
}

/// Edit, for a command hook (the only kind the form writes), and remove.
fn row_actions(
    hook: &Hook,
    key: &str,
    busy: bool,
    view: &WeakEntity<ChatView>,
) -> impl IntoElement + use<> {
    let (edited, removed) = (hook.clone(), hook.clone());
    let (edit_view, remove_view) = (view.clone(), view.clone());
    h_flex()
        .flex_none()
        .when(hook.kind == "command", |this| {
            this.child(
                Button::new(SharedString::from(format!("hook-edit-{key}")))
                    .ghost()
                    .xsmall()
                    .icon(Icon::new(gpui_kit::assets::IconName::Pencil))
                    .tooltip("Edit hook")
                    .disabled(busy)
                    .on_click(move |_, window, cx| {
                        let _ =
                            edit_view.update(cx, |view, cx| view.edit_hook(&edited, window, cx));
                    }),
            )
        })
        .child(
            Button::new(SharedString::from(format!("hook-remove-{key}")))
                .ghost()
                .xsmall()
                .icon(Icon::new(gpui_kit::assets::IconName::Trash))
                .tooltip("Remove hook")
                .disabled(busy)
                .on_click(move |_, _, cx| {
                    let _ = remove_view.update(cx, |view, cx| view.remove_hook(&removed, cx));
                }),
        )
}

/// Names a row by what makes the hook unique: the same command may run on
/// several events, matchers or settings files.
fn hook_key(hook: &Hook) -> String {
    format!(
        "{}-{}-{}-{}",
        hook.event, hook.matcher, hook.source, hook.text
    )
}
