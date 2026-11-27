//! The chats' look, read from the settings and written back when a chat's
//! customize mode changes it.

use gpui_kit::{
    App, Context, Window, component::WindowExt as _, component::notification::Notification,
};
use serde_json::Value;
use vortex_chat::{ChatItem, ChatLook, ChatSpot, UsageShown};
use vortex_settings::{Scope, Settings, UsageMeter};

use super::Workspace;
use crate::config;

/// Sets the look `settings` give, keeping whether the chats are being
/// customized.
pub fn apply(cx: &mut App) {
    let editing = cx.try_global::<ChatLook>().is_some_and(|look| look.editing);
    let look = ChatLook {
        editing,
        ..from_settings(config::settings(cx))
    };
    cx.set_global(look);
}

fn from_settings(settings: &Settings) -> ChatLook {
    let chat = &settings.chat;
    let items = |names: &[String]| {
        names
            .iter()
            .filter_map(|name| ChatItem::named(name))
            .collect()
    };
    ChatLook {
        top: items(&chat.toolbar.top),
        bottom_start: items(&chat.toolbar.bottom_start),
        bottom_end: items(&chat.toolbar.bottom_end),
        usage: match chat.usage_meter {
            UsageMeter::Fuller => UsageShown::Fuller,
            UsageMeter::Session => UsageShown::Session,
            UsageMeter::Weekly => UsageShown::Weekly,
            UsageMeter::Both => UsageShown::Both,
        },
        claude_color: vortex_settings::parse_color(&chat.colors.claude),
        codex_color: vortex_settings::parse_color(&chat.colors.codex),
        editing: false,
    }
}

/// The settings `look` takes that `kept` does not have yet.
fn changes(kept: &ChatLook, look: &ChatLook) -> Vec<(Vec<&'static str>, Value)> {
    let mut changes = Vec::new();
    for spot in ChatSpot::ALL {
        if kept.items(spot) != look.items(spot) {
            let names = look.items(spot).iter().map(|item| Value::from(item.name()));
            changes.push((vec!["chat", "toolbar", spot.name()], names.collect()));
        }
    }
    if kept.usage != look.usage {
        changes.push((vec!["chat", "usage_meter"], look.usage.name().into()));
    }
    let color =
        |color: Option<u32>| color.map_or_else(String::new, |color| format!("#{color:06x}"));
    if kept.claude_color != look.claude_color {
        changes.push((
            vec!["chat", "colors", "claude"],
            color(look.claude_color).into(),
        ));
    }
    if kept.codex_color != look.codex_color {
        changes.push((
            vec!["chat", "colors", "codex"],
            color(look.codex_color).into(),
        ));
    }
    changes
}

impl Workspace {
    /// Keeps `look` in the user's settings, for every project.
    pub(super) fn keep_chat_look(
        &mut self,
        look: &ChatLook,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let kept = from_settings(config::settings(cx));
        let changes = changes(&kept, look);
        if changes.is_empty() {
            return;
        }
        let written = vortex_settings::Layers::read(None, Value::Null).and_then(|mut layers| {
            changes
                .into_iter()
                .try_for_each(|(key, value)| layers.set(Scope::Global, &key, Some(value)))
        });
        match written {
            Ok(()) => self.reload_config(window, cx),
            Err(error) => {
                let error = format!("Cannot keep the chat's look: {error:#}");
                window.push_notification(Notification::error(error), cx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vortex_chat::{ChatItem, ChatLook, ChatSpot, UsageShown};

    use super::{changes, from_settings};

    #[test]
    fn the_default_settings_give_the_default_look() {
        let settings =
            vortex_settings::load(&vortex_settings::SettingsSources { layers: Vec::new() })
                .unwrap()
                .settings;
        assert_eq!(from_settings(&settings), ChatLook::default());
    }

    #[test]
    fn only_what_changed_is_written() {
        let kept = ChatLook::default();
        let look = ChatLook {
            usage: UsageShown::Both,
            codex_color: Some(0xe5_48_4d),
            ..kept.moved(ChatItem::Usage, ChatSpot::BottomEnd, None)
        };
        let written: Vec<_> = changes(&kept, &look)
            .into_iter()
            .map(|(key, value)| (key.join("."), value))
            .collect();
        assert_eq!(
            written,
            [
                (
                    "chat.toolbar.top".to_owned(),
                    json!(["processes", "account", "new_chat"])
                ),
                (
                    "chat.toolbar.bottom_end".to_owned(),
                    json!(["agents", "tokens", "cache", "context", "usage"])
                ),
                ("chat.usage_meter".to_owned(), json!("both")),
                ("chat.colors.codex".to_owned(), json!("#e5484d")),
            ]
        );
    }
}
