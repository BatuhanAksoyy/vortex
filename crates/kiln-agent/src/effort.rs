use std::time::Duration;

/// How hard the model thinks before it answers. A model supports some of
/// these, as [`crate::ModelOption::efforts`] lists.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

/// How one model request of the conversation read its prompt. The agent
/// caches the prompt so the next request reads it back for a fraction of
/// the cost, until the cache entry expires.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PromptTokens {
    /// Read back from the cache.
    pub cached: u64,
    /// Written to the cache for later requests.
    pub written: u64,
    /// Neither read from nor written to the cache.
    pub uncached: u64,
    /// How long what was written stays cached after its last use; `None`
    /// when nothing was written or the agent did not say.
    pub kept_for: Option<Duration>,
}
