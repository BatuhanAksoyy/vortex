//! Vortex's tools for agents, served over MCP. The agent starts `vortex mcp`,
//! which speaks MCP on its standard streams and hands every line on to the
//! running Vortex over a local socket; no port is opened. Each chat gets a
//! token of its own, which tells Vortex which chat a call is from: whose
//! browser it drives, whose changes it commits, or whose project's settings
//! it changes. No UI dependencies.

mod bridge;
mod mcp;
mod relay;
mod settings_tools;
mod tools;

pub use bridge::{Bridge, Call};
pub use relay::relay;
pub use settings_tools::{SettingsCall, SettingsScope};
pub use tools::{Answer, Browse, Content, Question, Reply, Request};

/// Where `vortex mcp` finds Vortex, and who it speaks for.
const SOCKET_VAR: &str = "VORTEX_BRIDGE";
const TOKEN_VAR: &str = "VORTEX_BRIDGE_TOKEN";
/// The server's name, which the agent puts before its tools'.
pub const SERVER_NAME: &str = "vortex";
