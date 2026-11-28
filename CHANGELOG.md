# Changelog

## 1.2.0 — 2026-11-28

### Fixed

- Google's sign-in no longer turns the browser away as unsafe.
- Sorting the processes page by PID, CPU or memory keeps each chat's
  processes under its own heading.
- An agent working in a worktree it made in the project can commit there,
  and the turn's work is committed in that worktree, not looked for in the
  project.
- A Codex account just added no longer reads as signed in to no account.
- Long project, session and branch names are cut short instead of
  running out of the title bar menu and the Git page's branch row.
- The branch menu scrolls only when it cannot fit.

### Added

- After an update, a dialog says what changed since the version you had.
- The chat says when the conversation is compacted, with the tokens
  before and after.
- The advisor's consultations show in the chat, with their answers when
  the CLI gets them.
- The Git history is drawn as a graph of every local branch, with their
  names, tags and merges.
- The Git page's diff can be closed, leaving the changes and history.
- The accounts panel shows when each usage window resets.

### Changed

- An account's session and weekly usage share one bar.
- The account in use is marked by its row instead of a tick, and the rows
  have more room.
- What you put away in the Files and Git pages stays away in other windows
  and after a restart.
- A new chat starts on the account the active one runs with.

## 1.1.0 — 2026-11-27

### Fixed

- With commits off (the default), Vortex no longer stops the agent from
  running `git commit` and `git push`, even in Bypass and Auto mode, and no
  review opens: the agent uses git itself. With commits on, it works as
  before.
- The Keyboard Shortcuts icon on the start page shows.

### Added

- The usage panel splits the current session's bar into this chat's part and
  what was used elsewhere, and says what the chat did: messages, new and
  cached tokens, and changed files.
- The accounts panel shows each account's session and weekly usage before
  you move to it, with when it resets and when it was reported.
- The tokens panel says how many requests the conversation made and how much
  prompt each sent, with new tokens apart from those read from the cache.
- Vortex's own CPU and memory show beside the agents' in the status bar and
  on the processes page.
- A copy button on every tool output and diff.
- Settings has a preview of the chat for Appearance and Editor.
- Choose a CLI's path from the start page or the accounts panel when it is
  not found; more Node version managers are searched (nvm, fnm, volta, pnpm,
  yarn, asdf, mise) and MacPorts.
- Scrolled past your last message, it stays pinned at the top; a click goes
  back to it.

### Changed

- Settings opens as a dialog over the dimmed window, and closes before the
  commit settings open.
- With no project open, the start page offers Settings, Keyboard Shortcuts,
  updates and each CLI's state, and the activity bar is hidden.
- Updates are announced at the gear, once per version.
- The account button shows the email only; the plan is in the panel.
- After a usage limit, picking another account waits for you to go on.
- Context categories use distinct hues.
- The customize mode lists its items by bar.
- Claude Code loads MCP tool schemas only when they are used.
- Processes are read less often while Vortex is in the background.

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
