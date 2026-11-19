//! Hash lock over the quality configuration.
//!
//! Lint levels, thresholds, the checker itself and the CI workflow decide
//! what "passing" means. Changing any of them must be a visible, deliberate
//! act by a person, so `check` fails until `cargo xtask lock-config` records
//! the new state. Coding agents are not allowed to run `lock-config`.

use std::{fmt::Write as _, path::Path};

use anyhow::{Context as _, bail};
use sha2::{Digest as _, Sha256};

use crate::{rules, workspace};

const LOCK_FILE: &str = "quality/config.lock";

/// Files locked verbatim, relative to the repository root.
const LOCKED_FILES: &[&str] = &[
    "clippy.toml",
    "rustfmt.toml",
    "deny.toml",
    "rust-toolchain.toml",
    ".github/workflows/ci.yml",
    ".githooks/pre-commit",
    ".claude/settings.json",
    rules::EXPECT_BASELINE,
];

pub fn write(root: &Path) -> anyhow::Result<()> {
    let baseline = rules::render_baseline(&rules::expect_counts(root)?);
    std::fs::write(root.join(rules::EXPECT_BASELINE), baseline)?;
    std::fs::write(root.join(LOCK_FILE), render(root)?)?;
    Ok(())
}

pub fn verify(root: &Path) -> anyhow::Result<()> {
    let expected = std::fs::read_to_string(root.join(LOCK_FILE))
        .with_context(|| format!("{LOCK_FILE} is missing"))?;
    let actual = render(root)?;
    if expected == actual {
        return Ok(());
    }
    let changed: Vec<&str> = actual
        .lines()
        .filter(|line| !expected.lines().any(|e| e == *line))
        .filter_map(|line| line.split_once("  ").map(|(_, name)| name))
        .collect();
    bail!(
        "quality configuration changed: {}. A person must review it and run \
         `cargo xtask lock-config`.",
        changed.join(", ")
    )
}

fn render(root: &Path) -> anyhow::Result<String> {
    let mut entries: Vec<(String, String)> = Vec::new();
    for file in LOCKED_FILES {
        let bytes =
            std::fs::read(root.join(file)).with_context(|| format!("cannot read {file}"))?;
        entries.push(((*file).to_owned(), hex_digest(&bytes)));
    }
    entries.push((
        "Cargo.toml [workspace.lints]".to_owned(),
        hex_digest(workspace_lints(root)?.as_bytes()),
    ));
    for path in workspace::rust_files(&root.join("xtask")) {
        let name = workspace::relative(root, &path);
        entries.push((name, hex_digest(&std::fs::read(&path)?)));
    }
    let mut out = String::new();
    for (name, digest) in entries {
        let _ = writeln!(out, "{digest}  {name}");
    }
    Ok(out)
}

fn workspace_lints(root: &Path) -> anyhow::Result<String> {
    let manifest: toml::Table = toml::from_str(&std::fs::read_to_string(root.join("Cargo.toml"))?)?;
    let lints = manifest
        .get("workspace")
        .and_then(|workspace| workspace.get("lints"))
        .context("root Cargo.toml has no [workspace.lints]")?;
    Ok(toml::to_string(lints)?)
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}
