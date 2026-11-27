//! When Vortex commits an agent's work and what a message must say, chosen
//! for every project, one project or one chat. Each tab shows what work
//! there goes by and where each value comes from; a change is saved at once,
//! to the user's settings file, the project's `.vortex/settings.json` or the
//! chat itself.

mod controls;
mod form;
#[cfg(test)]
mod tests;

use std::path::PathBuf;

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, SharedString, WeakEntity, Window,
    component::{WindowExt as _, notification::Notification},
    px,
};
use serde_json::Value;
use vortex_settings::{Layers, Scope};

use crate::chat_panel::ChatPanel;
use controls::Controls;

/// A setting on the page: the id its controls are named after, its label
/// and its key.
pub(crate) struct Field {
    name: &'static str,
    label: &'static str,
    key: &'static [&'static str],
}

impl Field {
    const fn new(name: &'static str, label: &'static str, key: &'static [&'static str]) -> Self {
        Self { name, label, key }
    }
}

pub(crate) const ENABLED: Field = Field::new(
    "enabled",
    "Commit the chat's work for it",
    &["git", "commit", "enabled"],
);
pub(crate) const TRIGGER: Field = Field::new("trigger", "When", &["git", "commit", "trigger"]);
pub(crate) const REVIEW: Field =
    Field::new("review", "Review each commit", &["git", "commit", "review"]);
pub(crate) const PUSH: Field = Field::new("push", "Push", &["git", "push", "policy"]);
pub(crate) const AGENT: Field = Field::new(
    "agent",
    "Writer's agent",
    &["git", "commit", "writer", "agent"],
);
pub(crate) const MODEL: Field = Field::new(
    "writer",
    "Judges the work and writes the message",
    &["git", "commit", "writer", "model"],
);
pub(crate) const DIRECTIVES: Field = Field::new(
    "directives",
    "Directives",
    &["git", "commit", "instructions"],
);
pub(crate) const REQUIRE: Field =
    Field::new("require", "Must match", &["git", "commit", "require"]);
pub(crate) const FORBID: Field =
    Field::new("forbid", "Must not match", &["git", "commit", "forbid"]);
pub(crate) const SUBJECT_MAX: Field = Field::new(
    "subject-max",
    "First line's most characters",
    &["git", "commit", "subject_max"],
);
pub(crate) const CONVENTIONAL: Field = Field::new(
    "conventional",
    "Conventional Commits",
    &["git", "commit", "conventional"],
);

const WIDTH: f32 = 440.;

/// Where the page was opened from, and what its pickers offer.
pub struct Opened {
    pub scope: Scope,
    /// The project folder, when there is one.
    pub project: Option<PathBuf>,
    /// The chat, whose own settings get a tab.
    pub chat: Option<WeakEntity<ChatPanel>>,
    /// What the chat sets itself.
    pub own: Value,
    /// The harnesses Vortex runs, by name.
    pub agents: Vec<String>,
    /// The models the writer may be, by id.
    pub models: Vec<String>,
}

pub struct CommitSettingsPage {
    layers: Layers,
    scope: Scope,
    chat: Option<WeakEntity<ChatPanel>>,
    controls: Controls,
    /// What kept the last change from being saved.
    error: Option<SharedString>,
}

/// Opens the page in a sheet over the window.
pub fn open(opened: Opened, window: &mut Window, cx: &mut App) {
    let layers = match Layers::read(opened.project.as_deref(), opened.own.clone()) {
        Ok(layers) => layers,
        Err(error) => {
            let error = format!("Cannot read the settings: {error:#}");
            window.push_notification(Notification::error(error), cx);
            return;
        }
    };
    let page = cx.new(|cx| CommitSettingsPage::new(layers, opened, window, cx));
    window.open_sheet(cx, move |sheet, _, _| {
        sheet
            .title("Commit settings")
            .size(px(WIDTH))
            .child(page.clone())
    });
}

impl CommitSettingsPage {
    fn new(layers: Layers, opened: Opened, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let scope = if opened.chat.is_none() && opened.scope == Scope::Chat {
            Scope::Project
        } else {
            opened.scope
        };
        let controls = Controls::new(opened.agents, opened.models, window, cx);
        let mut page = Self {
            layers,
            scope,
            chat: opened.chat,
            controls,
            error: None,
        };
        page.show(window, cx);
        page
    }

    /// Sets `field` at the tab's scope, or takes it out there with `None`.
    pub(crate) fn save(
        &mut self,
        field: &Field,
        value: Option<Value>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let other_agent = field.key == AGENT.key && self.value(&AGENT) != value.as_ref();
        if let Err(error) = self.layers.set(self.scope, field.key, value.clone()) {
            self.refuse(format!("Not saved: {error:#}"), cx);
            return;
        }
        // Each agent names its models differently; another one starts on
        // its default.
        if other_agent && self.value(&MODEL).is_some_and(|model| model != "") {
            self.save(&MODEL, Some(Value::String(String::new())), window, cx);
            return;
        }
        self.error = None;
        if self.scope == Scope::Chat
            && let Some(chat) = self.chat.as_ref().and_then(WeakEntity::upgrade)
        {
            chat.update(cx, |chat, cx| {
                chat.set_session_setting(field.key, value, cx);
            });
        }
        self.show(window, cx);
    }

    /// Saves `value` unless it is what the tab goes by already.
    fn save_changed(
        &mut self,
        field: &Field,
        value: Value,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.value(field) != Some(&value) {
            self.save(field, Some(value), window, cx);
        }
    }

    fn refuse(&mut self, why: String, cx: &mut Context<Self>) {
        self.error = Some(why.into());
        cx.notify();
    }

    pub(crate) fn pick_scope(&mut self, scope: Scope, window: &mut Window, cx: &mut Context<Self>) {
        self.scope = scope;
        self.error = None;
        self.show(window, cx);
    }

    /// The tabs the page offers, the narrowest first.
    pub(crate) fn scopes(&self) -> Vec<Scope> {
        let chat = self.chat.is_some().then_some(Scope::Chat);
        let project = self.layers.has_project().then_some(Scope::Project);
        [chat, project, Some(Scope::Global)]
            .into_iter()
            .flatten()
            .collect()
    }

    /// Puts what the tab's scope goes by into the controls.
    fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shown = controls::Shown {
            trigger: self.text(&TRIGGER),
            push: self.text(&PUSH),
            agent: self.text(&AGENT),
            model: self.text(&MODEL),
            directives: self.lines(&DIRECTIVES),
            require: self.lines(&REQUIRE),
            forbid: self.lines(&FORBID),
            subject_max: self
                .value(&SUBJECT_MAX)
                .map_or_else(String::new, Value::to_string),
        };
        self.controls.show(&shown, window, cx);
        cx.notify();
    }

    fn value(&self, field: &Field) -> Option<&Value> {
        self.layers.value(self.scope, field.key).0
    }

    fn text(&self, field: &Field) -> SharedString {
        let text = self
            .value(field)
            .and_then(Value::as_str)
            .unwrap_or_default();
        text.to_owned().into()
    }

    fn lines(&self, field: &Field) -> String {
        let lines = self.value(field).and_then(Value::as_array);
        let lines = lines.map_or(&[][..], Vec::as_slice);
        let lines: Vec<&str> = lines.iter().filter_map(Value::as_str).collect();
        lines.join("\n")
    }

    pub(crate) fn flag(&self, field: &Field) -> bool {
        self.value(field)
            .and_then(Value::as_bool)
            .unwrap_or_default()
    }

    /// The scope that set `field` for the tab's, `None` for a default.
    pub(crate) fn source(&self, field: &Field) -> Option<Scope> {
        self.layers.value(self.scope, field.key).1
    }
}
