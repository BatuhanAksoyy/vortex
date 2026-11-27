//! Claude Code as installed, checked and updated as its own `claude update`
//! does: an install that is no Homebrew cask follows the release channel
//! and is updated by `claude update`.

use std::path::PathBuf;

use anyhow::Context as _;
use vortex_agent::{Cli, Method, Program, fetch, parse_version};

const RELEASES: &str = "https://downloads.claude.ai/claude-code-releases";
/// The native installer, as Claude Code's own setup page gives it.
const INSTALL_SCRIPT: &str = "curl -fsSL https://claude.ai/install.sh | bash";

pub struct ClaudeCli {
    program: Program,
}

impl ClaudeCli {
    #[must_use]
    pub fn new(program: impl Into<Program>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl Cli for ClaudeCli {
    fn title(&self) -> &'static str {
        "Claude Code"
    }

    fn program(&self) -> PathBuf {
        self.program.path()
    }

    fn is_native(&self, parts: &[&str]) -> bool {
        parts
            .windows(3)
            .any(|run| run == ["share", "claude", "versions"])
    }

    fn latest(&self, _: &Method) -> anyhow::Result<String> {
        let body = fetch(&format!("{RELEASES}/{}", channel()))?;
        parse_version(&body).context("the release channel has no version")
    }

    fn updater(&self, method: &Method) -> Option<(PathBuf, Vec<String>)> {
        matches!(method, Method::Native | Method::Npm)
            .then(|| (self.program.path(), vec!["update".to_owned()]))
    }

    fn cask(&self) -> &'static str {
        "claude-code"
    }

    fn installer(&self) -> Option<(PathBuf, Vec<String>)> {
        let args = vec!["-c".to_owned(), INSTALL_SCRIPT.to_owned()];
        Some(("/bin/sh".into(), args))
    }
}

/// The channel `claude update` follows: `stable` when the user's settings
/// say so, `latest` otherwise.
fn channel() -> &'static str {
    let settings = crate::history::config_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join("settings.json")).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let chosen = settings
        .as_ref()
        .and_then(|settings| settings.get("autoUpdatesChannel"))
        .and_then(serde_json::Value::as_str);
    if chosen == Some("stable") {
        "stable"
    } else {
        "latest"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_native_install_lives_under_its_versions() {
        // A path that is nowhere, so it stays as given on any machine.
        let cli = ClaudeCli::new("/nowhere/claude");
        let native = [
            "Users", "someone", ".local", "share", "claude", "versions", "2.1.311",
        ];
        assert!(cli.is_native(&native));
        assert!(!cli.is_native(&["usr", "bin", "claude"]));
        assert_eq!(
            cli.updater(&Method::Npm),
            Some(("/nowhere/claude".into(), vec!["update".to_owned()]))
        );
        assert_eq!(cli.updater(&Method::Unknown), None);
    }
}
