//! Which Claude Code is installed, whether a newer one is out, and how to
//! update it, decided as the CLI's own `claude update` decides: a Homebrew
//! cask is compared with its formula and updated by `brew upgrade`, any
//! other install with the release channel and by `claude update`.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context as _, bail};

use crate::session::PARENT_SESSION_VARS;

const RELEASES: &str = "https://downloads.claude.ai/claude-code-releases";
const CASKS: &str = "https://formulae.brew.sh/api/cask";
/// The CLI gives up on its own check after five seconds.
const CHECK_SECONDS: &str = "10";
/// Lines of a failed update's output kept to say why.
const ERROR_LINES: usize = 6;

/// The Claude Code a program name runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Install {
    pub version: String,
    pub method: Method,
    /// The program with its links resolved.
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// `brew install --cask claude-code` (or `claude-code@latest`).
    Homebrew {
        cask: String,
        brew: PathBuf,
    },
    /// The native installer's, which updates itself.
    Native,
    Npm,
    /// Kiln cannot tell, so cannot tell how to update it either.
    Unknown,
}

impl Install {
    /// Reads the version `program` reports and how it was installed.
    ///
    /// # Errors
    /// When `program` is not on `PATH` or does not say its version.
    pub fn find(program: &Path) -> anyhow::Result<Self> {
        let path =
            locate(program).with_context(|| format!("{} is not on PATH", program.display()))?;
        let output = cli(&path)
            .arg("--version")
            .output()
            .with_context(|| format!("cannot run {}", path.display()))?;
        let said = String::from_utf8_lossy(&output.stdout);
        let Some(version) = parse_version(&said) else {
            bail!("{} did not say its version", path.display());
        };
        Ok(Self {
            version,
            method: method(&path),
            path,
        })
    }

    /// The newest version this install would update to.
    ///
    /// # Errors
    /// When the release cannot be read, as offline.
    pub fn latest(&self) -> anyhow::Result<String> {
        if let Method::Homebrew { cask, .. } = &self.method {
            let body = fetch(&format!("{CASKS}/{cask}.json"))?;
            cask_version(&body).context("the Homebrew formula has no version")
        } else {
            let body = fetch(&format!("{RELEASES}/{}", channel()))?;
            parse_version(&body).context("the release channel has no version")
        }
    }

    /// The command that updates it, as a user would type it; `None` when
    /// Kiln cannot tell how it was installed.
    #[must_use]
    pub fn update_command(&self) -> Option<String> {
        match &self.method {
            Method::Homebrew { cask, .. } => Some(format!("brew upgrade {cask}")),
            Method::Native | Method::Npm => Some("claude update".to_owned()),
            Method::Unknown => None,
        }
    }

    /// Runs the update and waits for it.
    ///
    /// # Errors
    /// When it cannot run or fails; the error ends with its last lines.
    pub fn update(&self) -> anyhow::Result<()> {
        let mut command = match &self.method {
            Method::Homebrew { cask, brew } => {
                let mut command = Command::new(brew);
                command.args(["upgrade", cask]);
                command
            }
            Method::Native | Method::Npm => {
                let mut command = cli(&self.path);
                command.arg("update");
                command
            }
            Method::Unknown => bail!("Kiln cannot tell how Claude Code was installed"),
        };
        let output = command
            .stdin(Stdio::null())
            .output()
            .context("cannot start the update")?;
        if output.status.success() {
            return Ok(());
        }
        let said = [output.stdout, output.stderr].concat();
        bail!(
            "the update failed: {}",
            last_lines(&String::from_utf8_lossy(&said))
        )
    }
}

/// Whether `latest` is a later version than `current`, part by part.
#[must_use]
pub fn is_newer(latest: &str, current: &str) -> bool {
    let parts = |version: &str| -> Vec<u64> {
        version
            .split('.')
            .map(|part| {
                let digits: String = part.chars().take_while(char::is_ascii_digit).collect();
                digits.parse().unwrap_or(0)
            })
            .collect()
    };
    parts(latest) > parts(current)
}

/// Without the parent session's variables, which would make it a nested
/// session of whatever terminal started Kiln.
fn cli(path: &Path) -> Command {
    let mut command = Command::new(path);
    command.stdin(Stdio::null());
    for var in PARENT_SESSION_VARS {
        command.env_remove(var);
    }
    command
}

/// `program` as `PATH` finds it, its links resolved.
fn locate(program: &Path) -> Option<PathBuf> {
    if program.components().count() > 1 {
        return std::fs::canonicalize(program).ok();
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .and_then(|found| std::fs::canonicalize(found).ok())
}

/// `2.1.311` out of `2.1.311 (Claude Code)`.
fn parse_version(said: &str) -> Option<String> {
    let first = said.split_whitespace().next()?;
    let numbered = first.split('.').count() >= 2
        && first
            .split('.')
            .all(|part| part.starts_with(|c: char| c.is_ascii_digit()));
    numbered.then(|| first.to_owned())
}

/// Told apart by where the resolved program lives.
fn method(path: &Path) -> Method {
    let parts: Vec<&str> = path.iter().filter_map(|part| part.to_str()).collect();
    if let Some(at) = parts.iter().position(|part| *part == "Caskroom")
        && let Some(cask) = parts.get(at + 1)
    {
        let prefix: PathBuf = path.iter().take(at).collect();
        return Method::Homebrew {
            cask: (*cask).to_owned(),
            brew: prefix.join("bin").join("brew"),
        };
    }
    if parts.contains(&"node_modules") {
        return Method::Npm;
    }
    let native = parts
        .windows(3)
        .any(|run| run == ["share", "claude", "versions"]);
    if native {
        return Method::Native;
    }
    Method::Unknown
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

fn cask_version(body: &str) -> Option<String> {
    let cask: serde_json::Value = serde_json::from_str(body).ok()?;
    parse_version(cask.get("version")?.as_str()?)
}

/// Read with `curl`, which every supported system has, so Kiln carries no
/// HTTP client for one request an hour.
fn fetch(url: &str) -> anyhow::Result<String> {
    let output = Command::new("curl")
        .args(["--fail", "--silent", "--show-error", "--location"])
        .args(["--max-time", CHECK_SECONDS, url])
        .stdin(Stdio::null())
        .output()
        .context("cannot run curl")?;
    if !output.status.success() {
        bail!(
            "cannot read {url}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn last_lines(said: &str) -> String {
    let lines: Vec<&str> = said
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();
    lines[lines.len().saturating_sub(ERROR_LINES)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_is_the_first_word() {
        assert_eq!(
            parse_version("2.1.311 (Claude Code)\n").as_deref(),
            Some("2.1.311")
        );
        assert_eq!(parse_version("2.1.332\n").as_deref(), Some("2.1.332"));
        assert_eq!(parse_version("<html>"), None);
        assert_eq!(parse_version(""), None);
    }

    #[test]
    fn later_versions_compare_by_number_not_by_text() {
        assert!(is_newer("2.1.326", "2.1.311"));
        assert!(is_newer("2.10.0", "2.9.9"));
        assert!(!is_newer("2.1.311", "2.1.311"));
        assert!(!is_newer("2.1.300", "2.1.311"));
    }

    #[test]
    fn a_cask_is_updated_with_the_brew_that_installed_it() {
        let path = Path::new("/opt/homebrew/Caskroom/claude-code@latest/2.1.332/claude");
        let install = Install {
            version: "2.1.332".into(),
            method: method(path),
            path: path.into(),
        };
        assert_eq!(
            install.method,
            Method::Homebrew {
                cask: "claude-code@latest".into(),
                brew: "/opt/homebrew/bin/brew".into(),
            }
        );
        assert_eq!(
            install.update_command().as_deref(),
            Some("brew upgrade claude-code@latest")
        );
    }

    #[test]
    fn other_installs_are_told_apart_by_where_they_live() {
        let npm = "/usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js";
        assert_eq!(method(Path::new(npm)), Method::Npm);
        let native = "/Users/someone/.local/share/claude/versions/2.1.311";
        assert_eq!(method(Path::new(native)), Method::Native);
        assert_eq!(method(Path::new("/usr/bin/claude")), Method::Unknown);
    }

    #[test]
    fn the_formula_says_the_cask_version() {
        let body = r#"{"token":"claude-code","version":"2.1.326","url":"x"}"#;
        assert_eq!(cask_version(body).as_deref(), Some("2.1.326"));
        assert_eq!(cask_version("{}"), None);
    }

    #[test]
    fn a_failure_keeps_its_last_lines() {
        let said: Vec<String> = (1..=10).map(|n| format!("line {n}")).collect();
        let said = said.join("\n");
        assert_eq!(last_lines(&said).lines().next(), Some("line 5"));
    }
}
