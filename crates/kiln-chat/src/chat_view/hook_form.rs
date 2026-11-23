//! The form that replaces the hooks list while a hook is added: when it
//! runs, for what, the command, and which settings keep it. A label sits over
//! each field, with help under the field it explains.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Entity, SharedString, TestSupportExt, WeakEntity, Window,
    component::{
        ActiveTheme, Disableable, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        input::{Input, InputState},
        select::{SearchableVec, Select, SelectItem, SelectState},
        v_flex,
    },
    div,
};
use kiln_agent::{HookEvent, HookScope};

use super::{ChatView, composer::option_row, hooks::HookForm};
use crate::hook_list::scope_label;

pub(super) type EventSelect = SelectState<SearchableVec<EventChoice>>;
pub(super) type ScopeSelect = SelectState<SearchableVec<ScopeChoice>>;

/// A row of the event select: the event over when it happens.
#[derive(Clone)]
pub(super) struct EventChoice {
    name: String,
    summary: SharedString,
}

impl EventChoice {
    pub(super) fn new(event: &HookEvent) -> Self {
        Self {
            name: event.name.clone(),
            summary: event.summary.clone().into(),
        }
    }
}

impl SelectItem for EventChoice {
    type Value = String;

    fn title(&self) -> SharedString {
        self.name.clone().into()
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        option_row(
            format!("hook-event-{}", self.name).into(),
            self.title(),
            self.summary.clone(),
            cx.theme().muted_foreground,
        )
    }

    fn value(&self) -> &String {
        &self.name
    }

    fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        self.name.to_lowercase().contains(&query) || self.summary.to_lowercase().contains(&query)
    }
}

/// A row of the scope select: where the hook is kept, over its file.
#[derive(Clone)]
pub(super) struct ScopeChoice(HookScope);

impl ScopeChoice {
    pub(super) fn all() -> SearchableVec<Self> {
        SearchableVec::new(HookScope::ALL.map(Self).to_vec())
    }
}

impl SelectItem for ScopeChoice {
    type Value = HookScope;

    fn title(&self) -> SharedString {
        scope_label(self.0).into()
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        option_row(
            format!("hook-scope-{:?}", self.0).into(),
            self.title(),
            scope_file(self.0).into(),
            cx.theme().muted_foreground,
        )
    }

    fn value(&self) -> &HookScope {
        &self.0
    }
}

/// The form as the panel shows it, copied out of the view each render.
pub(super) struct FormShown {
    pub(super) form: HookForm,
    /// When the chosen event happens.
    pub(super) summary: Option<String>,
    pub(super) takes_matcher: bool,
    pub(super) event: Entity<EventSelect>,
    pub(super) scope: Entity<ScopeSelect>,
    pub(super) matcher: Entity<InputState>,
    pub(super) command: Entity<InputState>,
}

pub(super) fn render(
    shown: &FormShown,
    busy: bool,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    let help = |text: &str| {
        div()
            .text_xs()
            .text_color(muted)
            .child(text.to_owned())
            .into_any_element()
    };
    v_flex()
        .id("hook-form")
        .test_support()
        .gap_3()
        .child(field(
            "Event",
            Select::new(&shown.event)
                .id("hook-event")
                .small()
                .search_placeholder("Search events")
                .menu_max_h(gpui_kit::rems(16.))
                .into_any_element(),
            shown.summary.as_deref().map(help),
        ))
        .when(shown.takes_matcher, |this| {
            this.child(field(
                "Matcher",
                text_box("hook-matcher", &shown.matcher),
                Some(help("A tool name or pattern. Leave empty to match all.")),
            ))
        })
        .child(field(
            "Command",
            text_box("hook-command", &shown.command),
            None,
        ))
        .child(field(
            "Save to",
            // A hook keeps its settings file when it is changed.
            Select::new(&shown.scope)
                .id("hook-scope")
                .small()
                .disabled(shown.form.editing.is_some())
                .into_any_element(),
            Some(help(scope_file(shown.form.scope))),
        ))
        .child(actions(shown.form.editing.is_some(), busy, view))
}

/// Cancel, and the form's default commit, which Enter in the command runs.
fn actions(editing: bool, busy: bool, view: &WeakEntity<ChatView>) -> impl IntoElement + use<> {
    let (cancel, save) = (view.clone(), view.clone());
    h_flex()
        .justify_end()
        .gap_2()
        .child(
            Button::new("hook-cancel")
                .ghost()
                .small()
                .label("Cancel")
                .on_click(move |_, _, cx| {
                    let _ = cancel.update(cx, ChatView::close_hook_form);
                }),
        )
        .child(
            Button::new("hook-save")
                .primary()
                .small()
                .label(if editing { "Save" } else { "Add hook" })
                .disabled(busy)
                .on_click(move |_, window, cx| {
                    let _ = save.update(cx, |view, cx| view.save_hook(window, cx));
                }),
        )
}

/// A label over its control, with optional help under it.
fn field(label: &'static str, control: AnyElement, help: Option<AnyElement>) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(div().text_xs().child(label))
        .child(control)
        .children(help)
}

fn text_box(id: &'static str, state: &Entity<InputState>) -> AnyElement {
    div()
        .id(id)
        .test_support()
        .child(Input::new(state).small())
        .into_any_element()
}

/// The settings file a scope writes.
fn scope_file(scope: HookScope) -> &'static str {
    match scope {
        HookScope::Project => "Only you, in .claude/settings.local.json",
        HookScope::SharedProject => "Everyone, in .claude/settings.json",
        HookScope::User => "Every project, in ~/.claude/settings.json",
    }
}
