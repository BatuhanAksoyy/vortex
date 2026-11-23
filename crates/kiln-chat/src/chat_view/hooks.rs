//! The hooks panel the composer strip opens: what it asks the agent for,
//! the form that adds a hook, and how an edit is saved and confirmed.

use std::time::Duration;

use gpui_kit::{
    AppContext as _, Context, Entity, Focusable as _, Subscription, Task, Window,
    component::{
        input::{InputEvent, InputState},
        select::{SearchableVec, SelectEvent, SelectState},
    },
};
use kiln_agent::{AgentEvent, Hook, HookEdit, HookScope, SessionCommand, StoredHook};

use super::{
    ChatView, StripPanel,
    hook_form::{EventChoice, EventSelect, ScopeChoice, ScopeSelect},
};
use crate::hook_list::HookList;

/// How long to wait before reading the hooks again after an edit: the
/// agent notices a settings file change only after a few seconds.
const CHECK_DELAY: Duration = Duration::from_secs(1);

/// The event the form starts on: the one most hooks are for.
const FIRST_EVENT: &str = "PreToolUse";

pub(super) struct HooksPanel {
    pub(super) list: HookList,
    /// Set while the form is open.
    pub(super) form: Option<HookForm>,
    pub(super) event: Entity<EventSelect>,
    pub(super) scope: Entity<ScopeSelect>,
    pub(super) matcher: Entity<InputState>,
    pub(super) command: Entity<InputState>,
    /// Reads the hooks again until a saved edit shows up.
    check: Option<Task<()>>,
    _subscriptions: [Subscription; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct HookForm {
    pub(super) event: String,
    pub(super) scope: HookScope,
    /// The hook being changed, by its event and stored form; `None` while
    /// a hook is added.
    pub(super) editing: Option<(String, StoredHook)>,
}

impl HooksPanel {
    pub(super) fn new(window: &mut Window, cx: &mut Context<ChatView>) -> Self {
        let event = cx.new(|cx| {
            SelectState::new(SearchableVec::new(Vec::new()), None, window, cx).searchable(true)
        });
        let scope = cx.new(|cx| SelectState::new(ScopeChoice::all(), None, window, cx));
        let matcher = cx.new(|cx| InputState::new(window, cx).placeholder("Bash or Edit|Write"));
        let command = cx.new(|cx| InputState::new(window, cx).placeholder("./scripts/check.sh"));
        let subscriptions = [
            cx.subscribe_in(&command, window, |this, _, event, window, cx| {
                if let InputEvent::PressEnter { .. } = event {
                    this.save_hook(window, cx);
                }
            }),
            cx.subscribe(
                &event,
                |this, _, event: &SelectEvent<SearchableVec<EventChoice>>, cx| {
                    if let SelectEvent::Confirm(Some(name)) = event {
                        this.choose_hook_event(name, cx);
                    }
                },
            ),
            cx.subscribe(
                &scope,
                |this, _, event: &SelectEvent<SearchableVec<ScopeChoice>>, cx| {
                    if let SelectEvent::Confirm(Some(scope)) = event {
                        this.choose_hook_scope(*scope, cx);
                    }
                },
            ),
        ];
        Self {
            list: HookList::default(),
            form: None,
            event,
            scope,
            matcher,
            command,
            check: None,
            _subscriptions: subscriptions,
        }
    }
}

impl ChatView {
    /// Opens or closes the hooks panel; opening asks for a fresh listing.
    /// The hooks come from the agent, so a chat whose process ended starts
    /// one, as a message would.
    pub(super) fn set_hooks_open(
        &mut self,
        open: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_panel(StripPanel::Hooks, open) {
            self.hooks.list.clear_messages();
            if let Some(commands) = self.ensure_session(window, cx) {
                let _ = commands.try_send(SessionCommand::RefreshHooks);
            }
        }
        cx.notify();
    }

    pub(super) fn open_hook_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events = self.hooks.list.events();
        let event = events
            .iter()
            .find(|event| event.name == FIRST_EVENT)
            .or_else(|| events.first())
            .map_or_else(|| FIRST_EVENT.to_owned(), |event| event.name.clone());
        let form = HookForm {
            event,
            scope: HookScope::default(),
            editing: None,
        };
        self.fill_hook_inputs("", "", window, cx);
        self.show_hook_form(form, window, cx);
    }

    /// Opens the form on a command hook kept in a settings file.
    pub(super) fn edit_hook(&mut self, hook: &Hook, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stored) = hook.stored.clone() else {
            return;
        };
        let form = HookForm {
            event: hook.event.clone(),
            scope: stored.scope,
            editing: Some((hook.event.clone(), stored.clone())),
        };
        self.fill_hook_inputs(&stored.matcher, &hook.text, window, cx);
        self.show_hook_form(form, window, cx);
    }

    fn fill_hook_inputs(
        &self,
        matcher: &str,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (matcher, command) = (matcher.to_owned(), command.to_owned());
        self.hooks
            .matcher
            .update(cx, |input, cx| input.set_value(matcher, window, cx));
        self.hooks
            .command
            .update(cx, |input, cx| input.set_value(command, window, cx));
    }

    fn show_hook_form(&mut self, form: HookForm, window: &mut Window, cx: &mut Context<Self>) {
        let choices: Vec<EventChoice> = self
            .hooks
            .list
            .events()
            .iter()
            .map(EventChoice::new)
            .collect();
        self.hooks.event.update(cx, |select, cx| {
            select.set_items(SearchableVec::new(choices), window, cx);
            select.set_selected_value(&form.event, window, cx);
        });
        self.hooks.scope.update(cx, |select, cx| {
            select.set_selected_value(&form.scope, window, cx);
        });
        self.hooks.form = Some(form);
        self.hooks.list.clear_messages();
        self.hooks
            .command
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        cx.notify();
    }

    pub(super) fn close_hook_form(&mut self, cx: &mut Context<Self>) {
        self.hooks.form = None;
        cx.notify();
    }

    pub(super) fn choose_hook_event(&mut self, event: &str, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.hooks.form {
            event.clone_into(&mut form.event);
        }
        cx.notify();
    }

    pub(super) fn choose_hook_scope(&mut self, scope: HookScope, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.hooks.form {
            form.scope = scope;
        }
        cx.notify();
    }

    /// Saves the form's hook; without a command it waits for one.
    pub(super) fn save_hook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(form) = self.hooks.form.clone() else {
            return;
        };
        let command = self.hooks.command.read(cx).value().trim().to_owned();
        if command.is_empty() {
            self.hooks
                .command
                .read(cx)
                .focus_handle(cx)
                .focus(window, cx);
            return;
        }
        let takes_matcher = self
            .hooks
            .list
            .events()
            .iter()
            .any(|event| event.name == form.event && event.takes_matcher);
        let matcher = if takes_matcher {
            self.hooks.matcher.read(cx).value().trim().to_owned()
        } else {
            String::new()
        };
        let edit = match form.editing {
            Some((target_event, target)) => {
                let unchanged = target_event == form.event
                    && target.matcher == matcher
                    && target.config["command"].as_str() == Some(command.as_str());
                if unchanged {
                    self.close_hook_form(cx);
                    return;
                }
                HookEdit::Replace {
                    event: form.event,
                    matcher,
                    command,
                    target_event,
                    target,
                }
            }
            None => HookEdit::Add {
                scope: form.scope,
                event: form.event,
                matcher,
                command,
            },
        };
        if self.start_hook_edit(edit) {
            self.hooks.form = None;
            self.clear_hook_inputs(window, cx);
        }
        cx.notify();
    }

    pub(super) fn remove_hook(&mut self, hook: &Hook, cx: &mut Context<Self>) {
        let Some(stored) = hook.stored.clone() else {
            return;
        };
        self.start_hook_edit(HookEdit::Remove {
            event: hook.event.clone(),
            stored,
        });
        cx.notify();
    }

    /// Folds the agent's hook news into the panel.
    pub(super) fn update_hooks(&mut self, event: &AgentEvent, cx: &mut Context<Self>) {
        match event {
            AgentEvent::Hooks(hooks) => {
                if self.hooks.list.apply(hooks.clone()) {
                    self.check_hooks_later(cx);
                }
            }
            AgentEvent::HookEdited { error } => self.hooks.list.saved(error.as_deref()),
            _ => return,
        }
        cx.notify();
    }

    /// `false` when the edit was refused before it was sent.
    fn start_hook_edit(&mut self, edit: HookEdit) -> bool {
        if !self.hooks.list.begin(edit.clone()) {
            return false;
        }
        if !self.send_command(SessionCommand::EditHook(edit)) {
            self.hooks.list.fail("Claude is not running.");
        }
        true
    }

    fn clear_hook_inputs(&self, window: &mut Window, cx: &mut Context<Self>) {
        for input in [&self.hooks.matcher, &self.hooks.command] {
            input.update(cx, |input, cx| input.clean(window, cx));
        }
    }

    fn check_hooks_later(&mut self, cx: &mut Context<Self>) {
        self.hooks.check = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CHECK_DELAY).await;
            let _ = this.update(cx, |this, _| {
                this.send_command(SessionCommand::RefreshHooks)
            });
        }));
    }
}
