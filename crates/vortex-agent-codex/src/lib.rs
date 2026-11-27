//! Codex backend: drives `codex app-server` over JSON-RPC on stdio and
//! translates it into `vortex-agent`'s session model.
//!
//! The app-server protocol is marked experimental. Its types are generated
//! from the CLI's own schema (`schema/generate.py`), and every shape read
//! here is covered by a captured fixture under `tests/fixtures/`.

mod approval;
mod client;
mod commands;
mod history;
mod install;
mod items;
mod notifications;
mod one_shot;
mod patch;
pub mod protocol;
mod report;
mod rpc;
mod session;
mod sign_in;
mod subagents;
mod thoughts;
mod tools;
mod translate;

pub use install::CodexCli;
pub use session::CodexBackend;
pub use translate::{Start, Step, Translator};
