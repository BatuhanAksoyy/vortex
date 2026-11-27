//! Codex as installed. Besides a Homebrew cask, it comes from npm as
//! `@openai/codex`, whose registry says the newest version.

use std::path::{Path, PathBuf};

use anyhow::Context as _;
use vortex_agent::{Cli, Method, Program, fetch, parse_version};

const PACKAGE: &str = "@openai/codex";
const REGISTRY: &str = "https://registry.npmjs.org";

pub struct CodexCli {
    program: Program,
}

impl CodexCli {
    #[must_use]
    pub fn new(program: impl Into<Program>) -> Self {
        Self {
            program: program.into(),
        }
    }
}

impl Cli for CodexCli {
    fn title(&self) -> &'static str {
        "Codex"
    }

    fn program(&self) -> PathBuf {
        self.program.path()
    }

    // Codex has no installer of its own that Vortex can tell apart.
    fn is_native(&self, _: &[&str]) -> bool {
        false
    }

    fn latest(&self, _: &Method) -> anyhow::Result<String> {
        let body = fetch(&format!("{REGISTRY}/{PACKAGE}/latest"))?;
        published_version(&body).context("the npm registry has no version")
    }

    fn updater(&self, method: &Method) -> Option<(PathBuf, Vec<String>)> {
        let args = ["install", "--global", &format!("{PACKAGE}@latest")].map(str::to_owned);
        (*method == Method::Npm).then(|| ("npm".into(), args.to_vec()))
    }

    fn cask(&self) -> &'static str {
        "codex"
    }

    /// Codex's own way besides Homebrew is npm, when it is there.
    fn installer(&self) -> Option<(PathBuf, Vec<String>)> {
        let npm = vortex_agent::on_path(Path::new("npm"));
        npm.then(|| self.updater(&Method::Npm)).flatten()
    }
}

fn published_version(body: &str) -> Option<String> {
    let package: serde_json::Value = serde_json::from_str(body).ok()?;
    parse_version(package.get("version")?.as_str()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_says_the_newest_version() {
        let body = r#"{"name":"@openai/codex","version":"0.173.0"}"#;
        assert_eq!(published_version(body).as_deref(), Some("0.173.0"));
        assert_eq!(published_version("{}"), None);
    }

    #[test]
    fn only_an_npm_install_is_updated_with_npm() {
        let cli = CodexCli::new("codex");
        let (program, args) = cli.updater(&Method::Npm).unwrap();
        assert_eq!(program, PathBuf::from("npm"));
        assert_eq!(args.join(" "), "install --global @openai/codex@latest");
        assert_eq!(cli.updater(&Method::Unknown), None);
    }
}
