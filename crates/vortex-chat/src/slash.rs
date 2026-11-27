//! The `/` command menu as data: the command name being typed, the commands
//! that match it, and the message once one is picked. Kept free of GPUI so
//! the matching rules are unit-tested directly.

use std::cmp::Ordering;

use vortex_agent::SlashCommand;

/// The command name being typed: the text between a leading `/` and the
/// cursor, while the cursor is still in the message's first word.
pub fn typed_name(text: &str, cursor: usize) -> Option<&str> {
    let name = text.get(..cursor)?.strip_prefix('/')?;
    (!name.contains(char::is_whitespace)).then_some(name)
}

/// The commands matching `typed`, best first: an exact name or alias, then
/// names starting with it, aliases starting with it, and names containing it
/// (so `delegate` finds `antigravity:delegate`). Ties go alphabetically.
pub fn matching<'a>(commands: &'a [SlashCommand], typed: &str) -> Vec<&'a SlashCommand> {
    let typed = typed.to_lowercase();
    let mut ranked: Vec<(Rank, &SlashCommand)> = commands
        .iter()
        .filter_map(|command| rank(command, &typed).map(|rank| (rank, command)))
        .collect();
    ranked.sort_by(|(rank_a, a), (rank_b, b)| {
        rank_a
            .cmp(rank_b)
            .then_with(|| compare_names(&a.name, &b.name))
    });
    ranked.into_iter().map(|(_, command)| command).collect()
}

/// The message after picking `command` while typing `text`: its first word
/// becomes `/name`, the rest stays. Also returns where the cursor goes: after
/// the name and one space, ready for arguments.
pub fn complete(text: &str, command: &SlashCommand) -> (String, usize) {
    let rest = text
        .find(char::is_whitespace)
        .map_or("", |end| &text[end..])
        .trim_start();
    let head = format!("/{} ", command.name);
    let cursor = head.len();
    (head + rest, cursor)
}

/// A command Vortex answers itself, at once and even while a turn runs,
/// instead of sending it to the agent as a message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NativeCommand {
    /// Show plan usage (`/usage`, `/cost`, `/stats`).
    Usage,
    /// Show what fills the context window.
    Context,
    /// Show the hooks; the agent's own `/hooks` menu needs a terminal.
    Hooks,
    /// Switch to the named model, or choose one when no name is given.
    Model(Option<String>),
    /// Start a new conversation (`/clear`, `/new`, `/reset`).
    Clear,
}

/// The native command `text` is, if it is one.
pub fn native(text: &str) -> Option<NativeCommand> {
    let rest = text.trim().strip_prefix('/')?;
    let (name, argument) = rest
        .split_once(char::is_whitespace)
        .map_or((rest, ""), |(name, argument)| (name, argument.trim()));
    match name {
        "usage" | "cost" | "stats" => Some(NativeCommand::Usage),
        "context" => Some(NativeCommand::Context),
        "hooks" => Some(NativeCommand::Hooks),
        "model" => Some(NativeCommand::Model(
            (!argument.is_empty()).then(|| argument.to_owned()),
        )),
        "clear" | "new" | "reset" => Some(NativeCommand::Clear),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    Exact,
    NamePrefix,
    AliasPrefix,
    NameContains,
}

fn rank(command: &SlashCommand, typed: &str) -> Option<Rank> {
    let name = command.name.to_lowercase();
    let aliases: Vec<String> = command.aliases.iter().map(|a| a.to_lowercase()).collect();
    if name == typed || aliases.iter().any(|alias| alias == typed) {
        Some(Rank::Exact)
    } else if name.starts_with(typed) {
        Some(Rank::NamePrefix)
    } else if aliases.iter().any(|alias| alias.starts_with(typed)) {
        Some(Rank::AliasPrefix)
    } else if name.contains(typed) {
        Some(Rank::NameContains)
    } else {
        None
    }
}

fn compare_names(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command(name: &str, aliases: &[&str]) -> SlashCommand {
        SlashCommand {
            name: name.into(),
            aliases: aliases.iter().map(|alias| (*alias).into()).collect(),
            ..SlashCommand::default()
        }
    }

    fn names(found: &[&SlashCommand]) -> Vec<String> {
        found.iter().map(|command| command.name.clone()).collect()
    }

    #[test]
    fn a_name_is_typed_only_in_the_first_word() {
        assert_eq!(typed_name("/comp", 5), Some("comp"));
        assert_eq!(typed_name("/comp", 3), Some("co"));
        assert_eq!(typed_name("/", 1), Some(""));
        assert_eq!(typed_name("/compact now", 12), None);
        assert_eq!(typed_name("hi /comp", 8), None);
        assert_eq!(typed_name("", 0), None);
    }

    #[test]
    fn a_cursor_inside_a_multibyte_character_types_nothing() {
        assert_eq!(typed_name("/ç", 2), None);
        assert_eq!(typed_name("/ç", 3), Some("ç"));
    }

    #[test]
    fn exact_matches_come_first_then_prefixes_then_the_rest() {
        let commands = [
            command("context", &[]),
            command("code-review", &["review"]),
            command("antigravity:review", &[]),
            command("compact", &[]),
            command("clear", &["reset", "new"]),
        ];
        assert_eq!(
            names(&matching(&commands, "review")),
            ["code-review", "antigravity:review"]
        );
        assert_eq!(
            names(&matching(&commands, "co")),
            ["code-review", "compact", "context"]
        );
        assert_eq!(names(&matching(&commands, "RES")), ["clear"]);
        assert_eq!(matching(&commands, "").len(), commands.len());
        assert_eq!(names(&matching(&commands, "zzz")), Vec::<String>::new());
    }

    #[test]
    fn some_commands_are_answered_natively() {
        assert_eq!(native("/usage"), Some(NativeCommand::Usage));
        assert_eq!(native(" /cost "), Some(NativeCommand::Usage));
        assert_eq!(native("/model"), Some(NativeCommand::Model(None)));
        assert_eq!(
            native("/model  sonnet "),
            Some(NativeCommand::Model(Some("sonnet".into())))
        );
        assert_eq!(native("/new"), Some(NativeCommand::Clear));
        assert_eq!(native("/context"), Some(NativeCommand::Context));
        assert_eq!(native("/hooks"), Some(NativeCommand::Hooks));
        assert_eq!(native("/compact"), None);
        assert_eq!(native("/usagex"), None);
        assert_eq!(native("usage"), None);
    }

    #[test]
    fn completing_replaces_the_first_word_and_keeps_the_rest() {
        let compact = command("compact", &[]);
        assert_eq!(complete("/comp", &compact), ("/compact ".into(), 9));
        assert_eq!(
            complete("/comp keep the plan", &compact),
            ("/compact keep the plan".into(), 9)
        );
    }
}
