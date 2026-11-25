/// An MCP server of Kiln's own that a session gets, started as a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolServer {
    /// What the agent calls it; its tools are named after it.
    pub name: String,
    pub command: std::path::PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}
