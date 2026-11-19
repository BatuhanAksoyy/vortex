//! Claude Code backend: drives `claude -p` over its `stream-json` input and
//! output plus the stdio control protocol for permission prompts.
//!
//! The protocol is not a stable public API. Every shape read here is covered by
//! a captured fixture under `tests/fixtures/`.

mod outgoing;
mod session;
mod translate;
mod wire;

pub use session::ClaudeBackend;
pub use translate::Translator;
