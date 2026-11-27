//! Which version of an agent's CLI is installed, whether a newer one is
//! out, and how to update it. A Homebrew cask is compared with its formula
//! and updated by `brew upgrade`, as the CLIs themselves do; any other
//! install is the agent's own [`Cli`] to judge.

use std::{
    fmt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
};

use anyhow::{Context as _, bail};

use crate::PARENT_SESSION_VARS;

const CASKS: &str = "https://formulae.brew.sh/api/cask";
/// The CLIs give up on their own checks after five seconds.
const CHECK_SECONDS: &str = "10";
/// Lines of a failed update's output kept to say why.
const ERROR_LINES: usize = 6;

/// What sets one agent's CLI apart: its name, and how an install that is
/// no Homebrew cask is told apart, checked and updated.
pub trait Cli: Send + Sync {
    /// As the user knows it, e.g. `Claude Code`.
    fn title(&self) -> &'static str;
    fn program(&self) -> &Path;
    /// Whether the resolved program, by its path's parts, is the CLI's own
    /// native install.
    fn is_native(&self, parts: &[&str]) -> bool;
    /// The newest version an install by `method`, never Homebrew, would
    /// update to.
    ///
    /// # Errors
    /// When it cannot be read, as offline.
    fn latest(&self, method: &Method) -> anyhow::Result<String>;
    /// The program and arguments that update an install by `method`, never
    /// Homebrew; `None` when Kiln cannot tell how.
    fn updater(&self, method: &Method) -> Option<(PathBuf, Vec<String>)>;
    /// The Homebrew cask that installs it.
    fn cask(&self) -> &'static str;
    /// The program and arguments that install it where Homebrew is not;
    /// `None` when there is no way Kiln can run.
    fn installer(&self) -> Option<(PathBuf, Vec<String>)>;
}

/// The CLI a program name runs.
#[derive(Clone)]
pub struct Install {
    pub version: String,
    pub method: Method,
    /// The program with its links resolved.
    pub path: PathBuf,
    cli: Arc<dyn Cli>,
}

impl fmt::Debug for Install {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Install")
            .field("cli", &self.cli.title())
            .field("version", &self.version)
            .field("method", &self.method)
            .field("path", &self.path)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    /// `brew install --cask claude-code` (or `claude-code@latest`, `codex`).
    Homebrew {
        cask: String,
        brew: PathBuf,
    },
    /// The CLI's own installer, which updates itself.
    Native,
    Npm,
    /// Kiln cannot tell, so cannot tell how to update it either.
    Unknown,
}

impl Install {
    /// Reads the version `cli`'s program reports and how it was installed.
    ///
    /// # Errors
    /// When the program is not on `PATH` or does not say its version.
    pub fn find(cli: Arc<dyn Cli>) -> anyhow::Result<Self> {
        let program = cli.program();
        let path =
            locate(program).with_context(|| format!("{} is not on PATH", program.display()))?;
        let output = command(&path)
            .arg("--version")
            .output()
            .with_context(|| format!("cannot run {}", path.display()))?;
        let said = String::from_utf8_lossy(&output.stdout);
        let Some(version) = parse_version(&said) else {
            bail!("{} did not say its version", path.display());
        };
        let parts: Vec<&str> = path.iter().filter_map(|part| part.to_str()).collect();
        let method = method(&path).unwrap_or_else(|| {
            if cli.is_native(&parts) {
                Method::Native
            } else {
                Method::Unknown
            }
        });
        Ok(Self {
            version,
            method,
            path,
            cli,
        })
    }

    /// The CLI's name, e.g. `Codex`.
    #[must_use]
    pub fn title(&self) -> &'static str {
        self.cli.title()
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
            self.cli.latest(&self.method)
        }
    }

    /// The command that updates it, as a user would type it; `None` when
    /// Kiln cannot tell how it was installed.
    #[must_use]
    pub fn update_command(&self) -> Option<String> {
        let (program, args) = self.updater()?;
        Some(shown(&program, &args))
    }

    /// Runs the update and waits for it.
    ///
    /// # Errors
    /// When it cannot run or fails; the error ends with its last lines.
    pub fn update(&self) -> anyhow::Result<()> {
        let Some((program, args)) = self.updater() else {
            bail!("Kiln cannot tell how {} was installed", self.title());
        };
        run(&program, &args, "update")
    }

    fn updater(&self) -> Option<(PathBuf, Vec<String>)> {
        match &self.method {
            Method::Homebrew { cask, brew } => {
                Some((brew.clone(), vec!["upgrade".to_owned(), cask.clone()]))
            }
            method => self.cli.updater(method),
        }
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

/// The first word that is a version: `2.1.311` out of
/// `2.1.311 (Claude Code)`, `0.172.0` out of `codex-cli 0.172.0`.
#[must_use]
pub fn parse_version(said: &str) -> Option<String> {
    let numbered = |word: &&str| {
        word.split('.').count() >= 2
            && word
                .split('.')
                .all(|part| part.starts_with(|c: char| c.is_ascii_digit()))
    };
    // Only the first line, so a page read by mistake is no version.
    let line = said.lines().next()?;
    line.split_whitespace()
        .take(2)
        .find(numbered)
        .map(str::to_owned)
}

/// Read with `curl`, which every supported system has, so Kiln carries no
/// HTTP client for one request an hour.
///
/// # Errors
/// When curl cannot run or the request fails.
pub fn fetch(url: &str) -> anyhow::Result<String> {
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

/// A command as a user would type it: `sh -c` shows the script it runs.
pub(crate) fn shown(program: &Path, args: &[String]) -> String {
    if let [flag, script] = args
        && flag == "-c"
    {
        return script.clone();
    }
    let name = program.file_name().unwrap_or(program.as_os_str());
    let words = std::iter::once(name.to_string_lossy().into_owned()).chain(args.iter().cloned());
    words.collect::<Vec<_>>().join(" ")
}

/// Runs `program` and waits for it; `what` names it in the errors, which
/// end with its last lines.
pub(crate) fn run(program: &Path, args: &[String], what: &str) -> anyhow::Result<()> {
    let output = command(program)
        .args(args)
        .output()
        .with_context(|| format!("cannot start the {what}"))?;
    if output.status.success() {
        return Ok(());
    }
    let said = [output.stdout, output.stderr].concat();
    bail!(
        "the {what} failed: {}",
        last_lines(&String::from_utf8_lossy(&said))
    )
}

/// Without the parent session's variables, which would make it a nested
/// session of whatever terminal started Kiln.
fn command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command.stdin(Stdio::null());
    for var in PARENT_SESSION_VARS {
        command.env_remove(var);
    }
    command
}

/// `program` as `PATH` finds it, its links resolved.
pub(crate) fn locate(program: &Path) -> Option<PathBuf> {
    if program.components().count() > 1 {
        return std::fs::canonicalize(program).ok();
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .and_then(|found| std::fs::canonicalize(found).ok())
}

/// Homebrew and npm, told apart by where the resolved program lives;
/// `None` for anything else.
fn method(path: &Path) -> Option<Method> {
    let parts: Vec<&str> = path.iter().filter_map(|part| part.to_str()).collect();
    if let Some(at) = parts.iter().position(|part| *part == "Caskroom")
        && let Some(cask) = parts.get(at + 1)
    {
        let prefix: PathBuf = path.iter().take(at).collect();
        return Some(Method::Homebrew {
            cask: (*cask).to_owned(),
            brew: prefix.join("bin").join("brew"),
        });
    }
    parts.contains(&"node_modules").then_some(Method::Npm)
}

fn cask_version(body: &str) -> Option<String> {
    let cask: serde_json::Value = serde_json::from_str(body).ok()?;
    parse_version(cask.get("version")?.as_str()?)
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
    fn the_version_is_the_first_versioned_word() {
        assert_eq!(
            parse_version("2.1.311 (Claude Code)\n").as_deref(),
            Some("2.1.311")
        );
        assert_eq!(parse_version("2.1.332\n").as_deref(), Some("2.1.332"));
        assert_eq!(
            parse_version("codex-cli 0.172.0\n").as_deref(),
            Some("0.172.0")
        );
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
        let path = Path::new("/opt/homebrew/Caskroom/codex/0.172.0/bin/codex");
        assert_eq!(
            method(path),
            Some(Method::Homebrew {
                cask: "codex".into(),
                brew: "/opt/homebrew/bin/brew".into(),
            })
        );
        let npm = "/usr/local/lib/node_modules/@anthropic-ai/claude-code/cli.js";
        assert_eq!(method(Path::new(npm)), Some(Method::Npm));
        assert_eq!(method(Path::new("/usr/bin/claude")), None);
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
