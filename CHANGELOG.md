# Changelog

## 1.0.1 — 2026-11-27

### Fixed

- Vortex finds `claude` and `codex` when they are not on `PATH`: it also
  looks where their installers put them (`~/.local/bin`, `~/.claude/local`,
  `~/.npm-global/bin`, `~/.bun/bin`, `/opt/homebrew/bin`, `/usr/local/bin`).
- Before a chat is open, Vortex no longer assumes Claude Code: the status bar
  shows no harness's CLI status, and the colors are Vortex's own graphite
  instead of Claude's orange.

### Added

- Choose where each CLI is: **Settings → Chat → Claude Code program** and
  **Codex program** take a path (`~/` for the home folder). A new path applies
  at once, without a restart.

### Changed

- The mark in the title bar is larger.

## 1.0.0 — 2026-11-27

The first release of Vortex: a lightweight, native IDE for running Claude Code
and Codex sessions side by side on your own subscriptions.

### Install

Download `Vortex-1.0.0-macos-arm64.zip` (Apple silicon) or
`Vortex-1.0.0-macos-x86_64.zip` (Intel), unzip it and move `Vortex.app` to
`Applications`. The app is not notarized yet. The first time, open it from its
context menu (**Open**), or run
`xattr -dr com.apple.quarantine /Applications/Vortex.app`. Vortex needs
`claude` and/or `codex` on your `PATH`, and offers to install a missing one.

### Chats

- Chat with Claude Code and Codex in tabs and side-by-side splits. Each tab
  keeps its own model, effort, permission mode and account.
- Hand a conversation over from one harness to the other.
- Stream text, thinking and tool calls as they happen. Each tool call shows
  its command output, diffs and pictures.
- Answer permission prompts, plans and the agent's questions inline. Choose how
  long "don't ask again" lasts and manage the permission rules and hooks.
- Follow subagents as cards with their own steps, model and report, including
  subagents of subagents. An agent map shows them as a tree, and any agent can
  be stopped.
- Complete slash commands. `/usage`, `/model`, `/context` and `/clear` are
  answered by Vortex itself.
- Manage skills from the composer, and attach pictures and files to messages.
- Find in a chat. Chats are named after the conversation, and you can switch
  conversations from the chat's name.
- See the context breakdown, the prompt cache and the tokens spent. The next
  message is predicted, and recaps show when a turn ended.
- Idle chats sleep and wake on the same conversation. The chat layout and each
  tab's account come back after a restart.
- Customize the chat: move buttons and meters between the title bar and the
  composer, show session and/or weekly usage, and set each harness's color.

### Accounts

- Sign in to several Claude and Codex subscriptions from inside Vortex,
  without a terminal.
- Every chat and subagent shows the account it runs on, with its plan's
  session and weekly usage.

### Git and worktrees

- See status and diffs, stage and commit, and browse the commit history.
  Switch and create branches from the Git page.
- Commit messages are cleaned and checked against the project's rules.
- Agents commit through Vortex's commit tool, reviewed by you. A turn's
  changes can be committed with a message a small model writes, from a button
  or once the chat is done.
- Push by the project's push policy. Vortex asks before it pushes `main`.
- Start a chat on a worktree of its own, then merge it back or discard it.

### Files and editor

- A file tree with file icons, fuzzy file search and search in files.
- A tree-sitter editor with git diffs and an optional minimap.
- Files and changes open beside the chat for each session's folder.

### Browser

- Real pages in the browser panel, drawn by your installed Chromium-based
  browser (Chrome, Edge, Brave, Chromium) without a window of its own.
- Hand the page to the chat. Agents use the browser through Vortex's MCP tools,
  in Claude Code and in Codex, and you can watch what they do as they do it.
- Vortex asks before a page uses devices, and shows what it loads and keeps.
- Pages can use USB, HID and serial devices through Vortex.

### Processes

- See what each agent started, such as dev servers, with what they use, and
  stop them.

### Settings and layout

- A settings page, opened from the gear, with every change shown at once.
  Settings are VS Code-style layered JSONC: defaults, then your own, then the
  project's. AI tools can change settings for you.
- A keyboard shortcuts page with a key recorder, and VS Code-compatible
  keybindings.
- A customizable layout: put the side panel and the browser on either side and
  hide the status bar. Each title bar button sits on the side of what it opens.
- Reopen the last window, and open new windows.

### Updates

- Vortex checks GitHub for a newer release, then downloads and installs it in
  place with a restart. It can also update Claude Code from the status bar.

### Project

- Vortex is open source under the MIT License.
