//! The settings page, opened from the gear at the activity bar's foot: the
//! settings of the catalog, for every project or for the one shown. A
//! change is written at once to the user's settings file or the project's
//! `.kiln/settings.json`, keeping their comments.

mod fields;
#[cfg(test)]
mod tests;

use std::path::Path;

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, EventEmitter, FocusHandle, Focusable, KeyDownEvent, SharedString, TestSupportExt,
    Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, IconName, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        setting::{SettingGroup, SettingPage, Settings},
        tab::{Tab, TabBar},
        v_flex,
    },
    div,
};
use kiln_chat::scale::{BAR_HEIGHT, ScaledButton as _};
use kiln_settings::{CATALOG, Entry, Layers, Scope, Section};
use serde_json::Value;

pub enum SettingsPageEvent {
    /// A setting was written; the window applies it.
    Changed,
    Close,
    OpenSettingsFile,
    OpenCommitSettings,
}

pub struct SettingsPage {
    layers: Layers,
    scope: Scope,
    focus: FocusHandle,
    /// What kept the last change from being saved.
    error: Option<SharedString>,
}

impl EventEmitter<SettingsPageEvent> for SettingsPage {}

impl SettingsPage {
    /// The page for what `layers` read: every project's settings, and the
    /// shown project's if there is one.
    pub fn new(layers: Layers, cx: &mut Context<Self>) -> Self {
        Self {
            layers,
            scope: Scope::Global,
            focus: cx.focus_handle(),
            error: None,
        }
    }

    /// Reads the files again, after they changed elsewhere.
    pub fn reread(&mut self, project: Option<&Path>, cx: &mut Context<Self>) {
        match Layers::read(project, Value::Null) {
            Ok(layers) => self.layers = layers,
            Err(error) => self.error = Some(format!("{error:#}").into()),
        }
        if !self.layers.has_project() {
            self.scope = Scope::Global;
        }
        cx.notify();
    }

    /// What the tab goes by at `entry`, and the scope that set it.
    fn value(&self, entry: &Entry) -> (Option<&Value>, Option<Scope>) {
        self.layers.value(self.scope, &entry.path())
    }

    /// Sets `entry` at the tab's scope, or takes it out there with `None`.
    fn save(&mut self, entry: &Entry, value: Option<Value>, cx: &mut Context<Self>) {
        if value.is_some() && self.value(entry).0 == value.as_ref() {
            return;
        }
        let checked = value
            .as_ref()
            .map_or(Ok(()), |value| kiln_settings::check(entry.key, value));
        let saved = checked.and_then(|()| self.layers.set(self.scope, &entry.path(), value));
        self.error = saved
            .err()
            .map(|error| format!("Not saved: {error:#}").into());
        if self.error.is_none() {
            cx.emit(SettingsPageEvent::Changed);
        }
        cx.notify();
    }

    fn pick_scope(&mut self, scope: Scope, cx: &mut Context<Self>) {
        self.scope = scope;
        self.error = None;
        cx.notify();
    }

    fn render_header(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let scopes = if self.layers.has_project() {
            vec![
                (Scope::Global, "All projects"),
                (Scope::Project, "This project"),
            ]
        } else {
            vec![(Scope::Global, "All projects")]
        };
        let selected = scopes.iter().position(|(scope, _)| *scope == self.scope);
        let tabs = TabBar::new("settings-scopes")
            .segmented()
            .small()
            .selected_index(selected.unwrap_or_default())
            .children(scopes.iter().map(|(_, label)| Tab::new().label(*label)))
            .on_click(cx.listener(move |this, index: &usize, _, cx| {
                if let Some((scope, _)) = scopes.get(*index) {
                    this.pick_scope(*scope, cx);
                }
            }));
        h_flex()
            .flex_none()
            .h(BAR_HEIGHT)
            .px_2()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .child(div().text_sm().child("Settings"))
            .child(tabs)
            .child(div().flex_1())
            .child(
                Button::new("settings-open-file")
                    .ghost()
                    .control()
                    .label("Edit in settings.json")
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(SettingsPageEvent::OpenSettingsFile);
                    })),
            )
            .child(
                Button::new("settings-close")
                    .ghost()
                    .control_icon(IconName::Close)
                    .tooltip("Close Settings")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SettingsPageEvent::Close))),
            )
    }

    fn pages(&self, cx: &Context<Self>) -> Vec<SettingPage> {
        let page = cx.entity();
        Section::ALL
            .iter()
            .map(|section| {
                let items = CATALOG
                    .iter()
                    .filter(|entry| entry.section == *section)
                    .map(|entry| fields::item(entry, self, &page, cx));
                let mut group = SettingGroup::new().items(items);
                if *section == Section::Git {
                    group = group.item(fields::commit_settings_link(&page));
                }
                SettingPage::new(section.title())
                    .icon(icon(*section))
                    .group(group)
            })
            .collect()
    }
}

fn icon(section: Section) -> Lucide {
    match section {
        Section::Appearance => Lucide::Palette,
        Section::Editor => Lucide::SquareCode,
        Section::Files => Lucide::Folder,
        Section::Layout => Lucide::PanelsTopLeft,
        Section::Chat => Lucide::MessageSquare,
        Section::Git => Lucide::GitBranch,
        Section::Updates => Lucide::CircleArrowUp,
    }
}

impl Focusable for SettingsPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        v_flex()
            .id("settings-page")
            .test_support()
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|_, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape" {
                    cx.stop_propagation();
                    cx.emit(SettingsPageEvent::Close);
                }
            }))
            .size_full()
            .bg(theme.background)
            .child(self.render_header(cx))
            .when_some(self.error.clone(), |this, error| {
                this.child(
                    div()
                        .id("settings-error")
                        .test_support()
                        .px_3()
                        .py_1()
                        .text_sm()
                        .text_color(theme.danger)
                        .child(error),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(Settings::new("settings").pages(self.pages(cx))),
            )
    }
}
