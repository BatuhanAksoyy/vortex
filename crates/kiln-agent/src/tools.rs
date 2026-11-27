/// An MCP server of Kiln's own that a session gets, started as a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ToolServer {
    /// What the agent calls it; its tools are named after it.
    pub name: String,
    pub command: std::path::PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// What the project asks of every commit message, told to the agent
    /// as it starts.
    pub commit_directives: Vec<String>,
}

impl ToolServer {
    /// What the agent is told of Kiln's tools. Commits and pushes are
    /// Kiln's: it holds commit messages to the project's rules.
    #[must_use]
    pub fn instructions(&self) -> String {
        let name = &self.name;
        let mut told = format!(
            "This session runs in Kiln, an IDE whose tools are named mcp__{name}__*. Commit with \
             mcp__{name}__commit, never with `git commit`: Kiln holds the message to the \
             project's rules and the user may review it first. Do not push; Kiln pushes as the \
             user set it to. When the user asks to change how Kiln looks or works (fonts, theme, \
             editor, layout, auto save), use mcp__{name}__settings_get and \
             mcp__{name}__settings_set rather than editing its settings files."
        );
        if !self.commit_directives.is_empty() {
            told.push_str("\n\nThe project asks this of every commit message:");
            for directive in &self.commit_directives {
                told.push_str("\n- ");
                told.push_str(directive);
            }
        }
        told
    }
}
