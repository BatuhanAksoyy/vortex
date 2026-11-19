//! Repository automation. `cargo xtask check` is the single quality gate used
//! by CI, the pre-commit hook and coding agents.

mod attributes;
mod complexity;
mod lock;
mod rules;
mod workspace;

use std::process::{Command, ExitCode};

use anyhow::{Context as _, bail};

const USAGE: &str = "usage: cargo xtask <check [--fast] | lock-config | install-hooks>";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            report(&format!("error: {error:#}"));
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = workspace::root()?;
    match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["check"] => check(&root, false),
        ["check", "--fast"] => check(&root, true),
        ["lock-config"] => lock::write(&root),
        ["install-hooks"] => cargo_like(&root, "git", &["config", "core.hooksPath", ".githooks"]),
        _ => bail!(USAGE),
    }
}

/// `--fast` skips tests and dependency audits, for hooks that run often.
fn check(root: &std::path::Path, fast: bool) -> anyhow::Result<()> {
    step("config lock", || lock::verify(root))?;
    step("custom rules", || rules::check(root))?;
    step("rustfmt", || {
        cargo_like(root, "cargo", &["fmt", "--all", "--check"])
    })?;
    step("clippy", || {
        cargo_like(
            root,
            "cargo",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--locked",
                "--",
                "-D",
                "warnings",
            ],
        )
    })?;
    if fast {
        return Ok(());
    }
    step("tests", || {
        cargo_like(root, "cargo", &["test", "--workspace", "--locked"])
    })?;
    step("cargo-deny", || {
        cargo_like(root, "cargo", &["deny", "--locked", "check"])
    })?;
    step("cargo-machete", || cargo_like(root, "cargo", &["machete"]))?;
    Ok(())
}

fn step(name: &str, run: impl FnOnce() -> anyhow::Result<()>) -> anyhow::Result<()> {
    report(&format!("==> {name}"));
    run().with_context(|| format!("{name} failed"))
}

fn cargo_like(root: &std::path::Path, program: &str, args: &[&str]) -> anyhow::Result<()> {
    // `cargo run` exports CARGO_PKG_* for xtask. cargo-machete reads
    // CARGO_PKG_NAME to tell `cargo machete` apart from a build script and
    // misparses its arguments when it is inherited.
    let status = Command::new(program)
        .args(args)
        .env_remove("CARGO_PKG_NAME")
        .current_dir(root)
        .status()
        .with_context(|| format!("cannot run `{program}`"))?;
    if !status.success() {
        bail!("`{program} {}` exited with {status}", args.join(" "));
    }
    Ok(())
}

#[expect(
    clippy::print_stderr,
    reason = "xtask is a CLI and stderr is its output channel"
)]
fn report(message: &str) {
    eprintln!("{message}");
}
