<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/lockup-horizontal-dark.svg">
    <img src="assets/brand/lockup-horizontal-light.svg" alt="Vortex" width="320">
  </picture>
</p>

<p align="center">
  A lightweight, native IDE for running Claude Code and Codex sessions side by side.
</p>

<p align="center">
  <a href="https://github.com/BatuhanAksoyy/vortex/releases/latest"><img src="https://img.shields.io/github/v/release/BatuhanAksoyy/vortex?label=release" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Linux-lightgrey" alt="macOS and Linux">
  <img src="https://img.shields.io/badge/rust-1.100-orange" alt="Rust 1.100">
</p>

---

Vortex drives the official `claude` and `codex` CLIs on your own subscriptions
and puts their sessions, your code and your git work in one GPU-drawn window.
There is no API key, no Electron and no bundled Chromium.

## Features

- **Claude Code and Codex together.** Open any number of sessions from either
  harness in tabs and splits, each with its own model, effort and permission mode.
- **Several subscriptions.** Sign in to more than one Claude or Codex account.
  Every chat and subagent shows the account it runs on, with its session and
  weekly usage.
- **Approvals in the chat.** Permission prompts, plans and questions from the
  agent are answered inline. Subagents get a card with their own steps and report.
- **A worktree per chat.** A chat can work on a branch of its own beside your
  checkout and merge back when it is done.
- **Git built in.** Status, diffs, staging and commits. Vortex commits a chat's
  work for it under the project's own commit rules.
- **Code reading and editing.** A file tree, fuzzy file search, project-wide
  text search, and a tree-sitter editor with diffs.
- **A browser the agent can use.** Pages render in your installed
  Chromium-based browser, headless. The agent drives it through Vortex's MCP
  tools and asks before it visits a new site.
- **Processes.** See what each session started, such as dev servers, with what
  they use, and stop them.
- **Made to fit.** Settings are VS Code-style JSONC with a settings page,
  keybindings come with a recorder, and the layout and chat toolbar can be
  rearranged. Every change shows at once.
- **Updates itself** from GitHub releases on macOS.

## Install

### macOS

1. Download `Vortex-<version>-macos-arm64.zip` (Apple silicon) or
   `-x86_64.zip` (Intel) from the [latest release](https://github.com/BatuhanAksoyy/vortex/releases/latest).
2. Unzip it and move `Vortex.app` to `Applications`.
3. The app is not notarized yet. The first time, open it from its context menu
   (**Open**), or run `xattr -dr com.apple.quarantine /Applications/Vortex.app`.

### From source

```sh
git clone https://github.com/BatuhanAksoyy/vortex.git
cd vortex
cargo run --release
```

`rust-toolchain.toml` pins the Rust version, and rustup installs it on the
first build. On Linux, install the system libraries GPUI needs (Wayland or X11,
Vulkan and fontconfig headers).

## Requirements

- **Claude Code** (`claude`) and/or **Codex** (`codex`) on your `PATH`.
  You can sign in from inside Vortex.
- **git**, for the git panel and worktrees.
- **A Chromium-based browser** such as Chrome, Edge, Brave or Chromium, for the
  browser panel. It is optional.

## Configuration

| What | Where |
|---|---|
| Settings | `~/.config/vortex/settings.json` |
| Keybindings | `~/.config/vortex/keybindings.json` |
| Project settings | `.vortex/settings.json` (shared), `.vortex/local.json` (personal, gitignored) |
| Accounts, chats, worktrees | `~/Library/Application Support/vortex` (macOS), `~/.local/share/vortex` (Linux) |

Open the settings page from the gear in the activity bar, or with <kbd>⌘</kbd> <kbd>,</kbd>.
Open the keyboard shortcuts with <kbd>⌘</kbd> <kbd>K</kbd> <kbd>⌘</kbd> <kbd>S</kbd>.

## Development

The workspace is split into crates by capability. The protocol, settings, git
and process logic are GPUI-free and tested without a UI.

| Crate | What it does |
|---|---|
| `vortex` | The binary: the window, and `vortex mcp` for the agents' tools |
| `vortex-workspace` | The window's layout, panels, pickers and commands |
| `vortex-chat` | The chat panel: transcript, prompts and composer |
| `vortex-agent` | The agent-neutral session model |
| `vortex-agent-claude` | Drives `claude -p` over `stream-json` |
| `vortex-agent-codex` | Drives `codex app-server` over JSON-RPC |
| `vortex-accounts` | Subscription accounts and their usage |
| `vortex-bridge` | Vortex's MCP tools for the agents |
| `vortex-browser` | The headless browser panel |
| `vortex-git` | git status, diffs, commits and worktrees |
| `vortex-project` | The file tree, search and file changes |
| `vortex-services` | The processes behind a session |
| `vortex-settings` | Layered JSONC settings |
| `vortex-keymap` | VS Code-compatible keybindings |
| `vortex-update` | Updates from GitHub releases |
| `vortex-devices` | USB, HID and serial devices for web pages |
| `vortex-file-icons` | File and folder icons |

Before you send a change, run the quality gate. It checks formatting, clippy
and the custom rules, then runs the tests:

```sh
cargo xtask check
```

Read [`AGENTS.md`](AGENTS.md) for the ground rules. The design lives in
[`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md), which is written in Turkish.

### Releases

`scripts/release.sh <version>` sets the version and writes `CHANGELOG.md`.
Pushing the `v<version>` tag makes the release workflow build both macOS
architectures and publish the release.

## License

Vortex is released under the [MIT License](LICENSE). The file icons are from
[Material Icon Theme](https://github.com/material-extensions/vscode-material-icon-theme)
(MIT).

Vortex is an independent project. It is not affiliated with or endorsed by
Anthropic or OpenAI. Claude Code and Codex are their respective owners'
products and are used under their own terms.
