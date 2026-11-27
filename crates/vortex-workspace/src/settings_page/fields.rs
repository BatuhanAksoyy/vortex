//! Each catalog entry as a control of the settings page.

use gpui_kit::{
    App, Context, Entity, IntoElement as _, SharedString,
    component::{
        ThemeMode, ThemeRegistry,
        button::Button,
        setting::{NumberFieldOptions, SettingField, SettingItem},
    },
};
use serde_json::Value;
use vortex_chat::scale::ScaledButton as _;
use vortex_settings::{Entry, Kind, Scope};

use super::{SettingsPage, SettingsPageEvent};

pub(super) fn item(
    entry: &'static Entry,
    page: &SettingsPage,
    handle: &Entity<SettingsPage>,
    cx: &App,
) -> SettingItem {
    let item = match entry.kind {
        Kind::Toggle => SettingItem::new(entry.title, toggle(entry, handle)),
        Kind::Number { min, max, step } => {
            let options = NumberFieldOptions { min, max, step };
            SettingItem::new(entry.title, number(entry, options, handle))
        }
        Kind::Text => SettingItem::new(entry.title, text(entry, handle)),
        Kind::Choice(options) => {
            let options = options
                .iter()
                .map(|(value, label)| (SharedString::from(*value), SharedString::from(*label)))
                .collect();
            SettingItem::new(entry.title, choice(entry, options, handle))
        }
        Kind::Theme => SettingItem::new(entry.title, choice(entry, themes(entry, cx), handle)),
    };
    let item = item.keywords([entry.key]);
    match described(entry, page) {
        Some(description) => item.description(description),
        None => item,
    }
}

/// What the entry does, and where its value comes from when not from the
/// defaults.
pub(super) fn described(entry: &Entry, page: &SettingsPage) -> Option<SharedString> {
    let source = match (page.value(entry).1, page.scope) {
        (None, _) => None,
        (Some(set), shown) if set == shown => Some("Set here"),
        (Some(Scope::Global), _) => Some("From your settings for all projects"),
        (Some(_), _) => Some("From this project"),
    };
    let text = match (entry.description, source) {
        ("", None) => return None,
        ("", Some(source)) => source.to_owned(),
        (description, None) => description.to_owned(),
        (description, Some(source)) => format!("{description} {source}."),
    };
    Some(text.into())
}

fn current(entry: &Entry, handle: &Entity<SettingsPage>, cx: &App) -> Option<Value> {
    handle.read(cx).value(entry).0.cloned()
}

fn save(entry: &'static Entry, handle: &Entity<SettingsPage>, value: Option<Value>, cx: &mut App) {
    handle.update(cx, |page, cx: &mut Context<SettingsPage>| {
        page.save(entry, value, cx);
    });
}

/// Set at the shown scope, so a reset there has something to take out.
fn set_here(entry: &Entry, handle: &Entity<SettingsPage>, cx: &App) -> bool {
    let page = handle.read(cx);
    page.value(entry).1 == Some(page.scope)
}

macro_rules! resettable {
    ($field:expr, $entry:expr, $handle:expr) => {{
        let (dirty, reset) = ($handle.clone(), $handle.clone());
        $field.on_reset(
            move |cx| set_here($entry, &dirty, cx),
            move |_, cx| save($entry, &reset, None, cx),
        )
    }};
}

fn toggle(entry: &'static Entry, handle: &Entity<SettingsPage>) -> SettingField<bool> {
    let (read, write) = (handle.clone(), handle.clone());
    let field = SettingField::switch(
        move |cx| current(entry, &read, cx).and_then(|it| it.as_bool()) == Some(true),
        move |on, cx| save(entry, &write, Some(Value::Bool(on)), cx),
    );
    resettable!(field, entry, handle)
}

fn number(
    entry: &'static Entry,
    options: NumberFieldOptions,
    handle: &Entity<SettingsPage>,
) -> SettingField<f64> {
    let (read, write) = (handle.clone(), handle.clone());
    let field = SettingField::number_input(
        options,
        move |cx| {
            let value = current(entry, &read, cx);
            value.and_then(|it| it.as_f64()).unwrap_or_default()
        },
        // As text, so a whole number is kept as one: `16`, not `16.0`.
        move |value, cx| {
            let number = serde_json::from_str(&value.to_string()).ok();
            save(entry, &write, number, cx);
        },
    );
    resettable!(field, entry, handle)
}

fn text(entry: &'static Entry, handle: &Entity<SettingsPage>) -> SettingField<SharedString> {
    let (read, write) = (handle.clone(), handle.clone());
    let field = SettingField::input(
        move |cx| shown_text(entry, &read, cx),
        move |text, cx| save(entry, &write, Some(Value::String(text.into())), cx),
    );
    resettable!(field, entry, handle)
}

fn choice(
    entry: &'static Entry,
    options: Vec<(SharedString, SharedString)>,
    handle: &Entity<SettingsPage>,
) -> SettingField<SharedString> {
    let (read, write) = (handle.clone(), handle.clone());
    let field = SettingField::dropdown(
        options,
        move |cx| shown_text(entry, &read, cx),
        move |value, cx| save(entry, &write, Some(Value::String(value.into())), cx),
    );
    resettable!(field, entry, handle)
}

fn shown_text(entry: &Entry, handle: &Entity<SettingsPage>, cx: &App) -> SharedString {
    let value = current(entry, handle, cx);
    let text = value.as_ref().and_then(Value::as_str).unwrap_or_default();
    SharedString::from(text.to_owned())
}

/// The themes of the entry's mode, by name.
fn themes(entry: &Entry, cx: &App) -> Vec<(SharedString, SharedString)> {
    let mode = if entry.key.ends_with("light") {
        ThemeMode::Light
    } else {
        ThemeMode::Dark
    };
    let themes = ThemeRegistry::global(cx).sorted_themes();
    themes
        .into_iter()
        .filter(|theme| theme.mode == mode)
        .map(|theme| (theme.name.clone(), theme.name.clone()))
        .collect()
}

/// The commit rules are lists the commit settings page edits.
pub(super) fn commit_settings_link(handle: &Entity<SettingsPage>) -> SettingItem {
    let handle = handle.downgrade();
    SettingItem::render(move |_, _, _| {
        let handle = handle.clone();
        Button::new("settings-commit-rules")
            .outline()
            .control()
            .label("Commit Rules and Writer…")
            .on_click(move |_, _, cx| {
                let _ = handle.update(cx, |_, cx| {
                    cx.emit(SettingsPageEvent::OpenCommitSettings);
                });
            })
            .into_any_element()
    })
    .keywords(["commit", "message", "writer", "rules"])
}
