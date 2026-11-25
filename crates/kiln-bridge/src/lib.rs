//! Kiln's tools for agents, served over MCP. The agent starts `kiln mcp`,
//! which speaks MCP on its standard streams and hands every line on to the
//! running Kiln over a local socket; no port is opened. Each chat gets a
//! token of its own, which tells Kiln whose browser a call is for. No UI
//! dependencies.

mod bridge;
mod mcp;
mod relay;
mod tools;

pub use bridge::{Bridge, Call};
pub use relay::relay;
pub use tools::{Content, Reply, Request};

/// Where `kiln mcp` finds Kiln, and who it speaks for.
const SOCKET_VAR: &str = "KILN_BRIDGE";
const TOKEN_VAR: &str = "KILN_BRIDGE_TOKEN";
/// The server's name, which the agent puts before its tools'.
pub const SERVER_NAME: &str = "kiln";
