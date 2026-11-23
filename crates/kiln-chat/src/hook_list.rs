//! The hooks panel as data: the hooks by event, and an edit from the click
//! until the agent lists the change. Kept free of GPUI so the rules are
//! unit-tested directly.

use kiln_agent::{Hook, HookEdit, HookEvent, HookScope, Hooks, StoredHook};

/// How many times the hooks are read back after an edit before the panel
/// says the session has not picked it up. The agent notices a settings
/// file change only after a few seconds.
const CHECKS: u8 = 12;

const NOT_PICKED_UP: &str = "Saved, but this session doesn't list the change yet. Reopen the \
                             panel to check.";

#[derive(Default)]
pub struct HookList {
    /// `None` until the agent lists them.
    listed: Option<Hooks>,
    pending: Option<Pending>,
    error: Option<String>,
    notice: Option<String>,
}

struct Pending {
    edit: HookEdit,
    checks: u8,
    /// Set once the settings file holds the edit.
    saved: bool,
}

/// The hook an edit puts in a settings file, as the agent will list it.
struct Made<'a> {
    scope: HookScope,
    event: &'a str,
    matcher: &'a str,
    command: &'a str,
}

impl Made<'_> {
    fn is(&self, hook: &Hook) -> bool {
        hook.event == self.event
            && hook.text == self.command
            && hook
                .stored
                .as_ref()
                .is_some_and(|stored| stored.scope == self.scope && stored.matcher == self.matcher)
    }

    /// The row shown until the agent lists it. It has no stored form yet,
    /// so it cannot be changed again until then.
    fn row(&self) -> Hook {
        Hook {
            event: self.event.to_owned(),
            matcher: self.matcher.to_owned(),
            source: format!("Saving to {}…", scope_label(self.scope).to_lowercase()),
            text: self.command.to_owned(),
            kind: "command".to_owned(),
            disabled: false,
            stored: None,
        }
    }
}

impl Pending {
    fn made(&self) -> Option<Made<'_>> {
        match &self.edit {
            HookEdit::Add {
                scope,
                event,
                matcher,
                command,
            } => Some(Made {
                scope: *scope,
                event,
                matcher,
                command,
            }),
            HookEdit::Replace {
                event,
                matcher,
                command,
                target,
                ..
            } => Some(Made {
                scope: target.scope,
                event,
                matcher,
                command,
            }),
            HookEdit::Remove { .. } => None,
        }
    }

    /// The hook the edit takes out of its settings file.
    fn gone(&self) -> Option<(&str, &StoredHook)> {
        match &self.edit {
            HookEdit::Replace {
                target_event,
                target,
                ..
            } => Some((target_event, target)),
            HookEdit::Remove { event, stored } => Some((event, stored)),
            HookEdit::Add { .. } => None,
        }
    }

    /// A changed hook counts as shown once its new form is listed, even if
    /// nothing about it changed.
    fn shown_in(&self, hooks: &[Hook]) -> bool {
        match (self.made(), self.gone()) {
            (Some(made), _) => hooks.iter().any(|hook| made.is(hook)),
            (None, Some(gone)) => !hooks.iter().any(|hook| is_stored(hook, gone)),
            (None, None) => true,
        }
    }
}

fn is_stored(hook: &Hook, (event, stored): (&str, &StoredHook)) -> bool {
    hook.event == event && hook.stored.as_ref() == Some(stored)
}

impl HookList {
    pub fn is_loaded(&self) -> bool {
        self.listed.is_some()
    }

    /// An edit is being saved or confirmed; another waits for it.
    pub fn is_busy(&self) -> bool {
        self.pending.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub fn locked(&self) -> Option<&str> {
        self.listed.as_ref()?.locked.as_deref()
    }

    /// Every event a hook can be added for.
    pub fn events(&self) -> &[HookEvent] {
        self.listed.as_ref().map_or(&[], |listed| &listed.events)
    }

    /// The hooks with the edit under way already made, in runs of one event
    /// each, in the order events happen.
    pub fn by_event(&self) -> Vec<(String, Vec<Hook>)> {
        let mut hooks: Vec<Hook> = self
            .listed
            .as_ref()
            .map_or_default(|listed| listed.hooks.clone());
        if let Some(pending) = &self.pending {
            if let Some(gone) = pending.gone() {
                hooks.retain(|hook| !is_stored(hook, gone));
            }
            if let Some(made) = pending.made()
                && !hooks.iter().any(|hook| made.is(hook))
            {
                hooks.push(made.row());
            }
        }
        let order = |hook: &Hook| {
            self.events()
                .iter()
                .position(|event| event.name == hook.event)
                .unwrap_or(usize::MAX)
        };
        hooks.sort_by_key(order);
        let mut groups: Vec<(String, Vec<Hook>)> = Vec::new();
        for hook in hooks {
            match groups.last_mut() {
                Some((event, run)) if *event == hook.event => run.push(hook),
                _ => groups.push((hook.event.clone(), vec![hook])),
            }
        }
        groups
    }

    /// Shows `edit` at once; `false` while another edit runs or when hooks
    /// are locked.
    pub fn begin(&mut self, edit: HookEdit) -> bool {
        if self.pending.is_some() || self.locked().is_some() {
            return false;
        }
        self.pending = Some(Pending {
            edit,
            checks: 0,
            saved: false,
        });
        self.error = None;
        self.notice = None;
        true
    }

    /// The settings writer finished; on failure the hooks show as they are.
    pub fn saved(&mut self, error: Option<&str>) {
        match error {
            Some(error) => {
                self.pending = None;
                self.error = Some(format!("The hook was not saved: {error}"));
            }
            None => {
                if let Some(pending) = &mut self.pending {
                    pending.saved = true;
                }
            }
        }
    }

    /// The edit could not be sent at all.
    pub fn fail(&mut self, error: &str) {
        self.saved(Some(error));
    }

    /// Takes a listing from the agent; `true` when it should be read again
    /// because a saved edit has not shown up yet.
    pub fn apply(&mut self, hooks: Hooks) -> bool {
        let again = match self.pending.as_mut() {
            Some(pending) if pending.saved && !pending.shown_in(&hooks.hooks) => {
                pending.checks += 1;
                pending.checks < CHECKS
            }
            _ => false,
        };
        let saved = self.pending.as_ref().is_some_and(|pending| pending.saved);
        if saved && !again {
            if !self.pending_shown(&hooks.hooks) {
                self.notice = Some(NOT_PICKED_UP.to_owned());
            }
            self.pending = None;
        }
        self.listed = Some(hooks);
        again
    }

    pub fn clear_messages(&mut self) {
        self.error = None;
        self.notice = None;
    }

    fn pending_shown(&self, hooks: &[Hook]) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|pending| pending.shown_in(hooks))
    }
}

/// Where a hook in `scope` is kept, as the panel names it.
pub fn scope_label(scope: HookScope) -> &'static str {
    match scope {
        HookScope::Project => "This project",
        HookScope::SharedProject => "Shared project settings",
        HookScope::User => "All projects",
    }
}

#[cfg(test)]
mod tests;
