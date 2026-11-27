//! Installing an agent's CLI that is missing: with Homebrew where it is,
//! else the CLI's own way.

use std::path::{Path, PathBuf};

use anyhow::bail;

use crate::{
    Cli,
    install::{locate, run, shown},
};

/// Where Homebrew puts itself, for a Vortex started without it on `PATH`.
const BREWS: [&str; 2] = ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"];

/// Whether `cli`'s program is on `PATH`.
#[must_use]
pub fn is_installed(cli: &dyn Cli) -> bool {
    on_path(&cli.program())
}

/// Whether `program` is on `PATH`, or is a file when given as a path.
#[must_use]
pub fn on_path(program: &Path) -> bool {
    locate(program).is_some()
}

/// The command that installs `cli`, as a user would type it; `None` when
/// Vortex knows no way.
#[must_use]
pub fn install_command(cli: &dyn Cli) -> Option<String> {
    let (program, args) = installer(cli)?;
    Some(shown(&program, &args))
}

/// Installs `cli` and waits for it.
///
/// # Errors
/// When there is no way to, or it fails; the error ends with its last lines.
pub fn install(cli: &dyn Cli) -> anyhow::Result<()> {
    let Some((program, args)) = installer(cli) else {
        bail!("Vortex knows no way to install {} here", cli.title());
    };
    run(&program, &args, "installation")
}

fn installer(cli: &dyn Cli) -> Option<(PathBuf, Vec<String>)> {
    let cask = || {
        ["install", "--cask", cli.cask()]
            .map(str::to_owned)
            .to_vec()
    };
    brew()
        .map(|brew| (brew, cask()))
        .or_else(|| cli.installer())
}

fn brew() -> Option<PathBuf> {
    locate(Path::new("brew"))
        .or_else(|| BREWS.iter().map(PathBuf::from).find(|brew| brew.is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shell_script_shows_as_its_script() {
        let script = "curl -fsSL https://claude.ai/install.sh | bash".to_owned();
        let args = ["-c".to_owned(), script.clone()];
        assert_eq!(shown(Path::new("/bin/sh"), &args), script);
        let args = ["install", "--cask", "codex"].map(str::to_owned);
        assert_eq!(
            shown(Path::new("/opt/homebrew/bin/brew"), &args),
            "brew install --cask codex"
        );
    }
}
