# AGENTS.md

Vortex is a lightweight, GPUI-based IDE for running Claude Code and Codex sessions side by side.
The design is in `docs/ARCHITECTURE.md`. Read it before you change structure, add a crate, or add a dependency.

## Ground rules

- All code, identifiers, UI strings and commit messages are in English.
- YAGNI, DRY, KISS. Build what the current milestone needs, nothing speculative.
- Crates are split by capability. Protocol, settings, git and process logic live in GPUI-free crates and are unit-tested without UI. Only view-owning feature crates (`vortex-workspace`, later `vortex-chat`, …) and the `vortex` binary depend on `gpui-kit`.
- For any GPUI code, load the `gpui-kit` skill in `.claude/skills/` first and follow its Coding Guides. Never invent a GPUI API; search the source in `~/.cargo/registry/src/*/gpui-component-0.10.0`.
- `unsafe` lives only in `vortex-sys`.
- No LSP, no plugin system, no Chromium/webview dependencies.
- Code comments explain *why* something non-obvious is the way it is. Never write narrative comments ("fixed X", "there was a problem with Y"); that belongs in commit messages.

## Quality gate

Run `cargo xtask check` before you report any task as done. It must pass with no edits to the quality config.

You must not:
- write `#[allow(...)]` (use `#[expect(lint, reason = "...")]`, and only when the code is genuinely right);
- add crate-level `#![allow]` / `#![expect]`;
- edit `[workspace.lints]`, `clippy.toml`, `deny.toml`, `rustfmt.toml` or anything under `quality/`;
- use `--no-verify`, `#[ignore]` without a reason, or `unwrap`/`expect`/`panic!` outside tests.

If a lint fires, restructure the code: split the function, extract a type, flatten the nesting.
If you believe a threshold is wrong, stop and ask the user. Do not work around it.

## Agent protocol code

- Claude (`vortex-agent-claude`) and Codex (`vortex-agent-codex`) protocols are not formally stable.
- Every new event shape needs a captured real-world fixture under the crate's `tests/fixtures/` and a replay test.
- Capture with `tests/fixtures/capture.py` in an empty directory outside the repo (never inside it: project hooks would run in the child), then run `scrub.py` on the result. Commit only scrubbed fixtures; `fixtures_hold_no_personal_data` enforces it.
- Spawned CLIs must not inherit the parent Claude Code session's variables (`CLAUDECODE`, `CLAUDE_CODE_SESSION_ID`, …); see `PARENT_SESSION_VARS`.
- Codex protocol types are generated from `codex app-server generate-json-schema`. Do not hand-edit them.

## Git

Do not create commits, branches, tags, issues or MRs/PRs unless the user explicitly asks in the current task.
