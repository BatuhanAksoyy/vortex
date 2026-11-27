//! The harnesses' CLI programs, which every window's chats and status bar
//! share, run from where the settings say or else found where they are.

use std::path::PathBuf;

use gpui_kit::{App, BorrowAppContext as _, Global};
use vortex_agent::Program;
use vortex_settings::AgentSettings;

/// Under the home folder, where the CLIs' installers put them: Claude
/// Code's native installer, its older one that left only a shell alias,
/// and npm or Bun set to install into the home folder.
const CLAUDE_KNOWN: &[&str] = &[
    ".local/bin/claude",
    ".claude/local/claude",
    ".npm-global/bin/claude",
    ".bun/bin/claude",
];
const CODEX_KNOWN: &[&str] = &[
    ".local/bin/codex",
    ".npm-global/bin/codex",
    ".bun/bin/codex",
];

#[derive(Clone)]
pub struct Programs {
    pub claude: Program,
    pub codex: Program,
}

impl Global for Programs {}

impl Default for Programs {
    fn default() -> Self {
        Self {
            claude: Program::new("claude", CLAUDE_KNOWN),
            codex: Program::new("codex", CODEX_KNOWN),
        }
    }
}

pub fn init(cx: &mut App) {
    cx.set_global(Programs::default());
}

pub fn programs(cx: &App) -> Programs {
    cx.try_global::<Programs>().cloned().unwrap_or_default()
}

/// Runs the programs `agents` gives from now on. Whatever watches
/// [`Programs`] hears of a change, so a CLI found there shows at once.
pub fn apply(agents: &AgentSettings, cx: &mut App) {
    let Some(programs) = cx.try_global::<Programs>().cloned() else {
        return;
    };
    let claude = programs.claude.choose(chosen(&agents.claude_path));
    let codex = programs.codex.choose(chosen(&agents.codex_path));
    if claude || codex {
        cx.update_global::<Programs, _>(|_, _| {});
    }
}

/// `path` with `~/` as the home folder; `None` when it is empty.
fn chosen(path: &str) -> Option<PathBuf> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    Some(match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_path_looks_and_a_home_path_is_expanded() {
        assert_eq!(chosen("  "), None);
        assert_eq!(chosen("/opt/claude"), Some(PathBuf::from("/opt/claude")));
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
        assert_eq!(chosen("~/bin/claude"), Some(home.join("bin/claude")));
    }
}
