//! Claude Code backend: drives `claude -p` over its `stream-json` input and
//! output plus the stdio control protocol for permission prompts.
//!
//! The protocol is not a stable public API. Every shape read here is covered by
//! a captured fixture under `tests/fixtures/`.

mod ask;
mod cli_edit;
mod control;
mod history;
mod hooks;
mod install;
mod one_shot;
mod outgoing;
mod picture;
mod recap;
mod session;
mod sign_in;
mod skills;
mod stream;
mod subagent;
mod system;
mod tool_events;
mod tool_server;
mod translate;
mod turn_end;
mod usage;
mod usage_ask;
mod wire;

pub use install::ClaudeCli;
pub use session::ClaudeBackend;
pub use translate::Translator;
