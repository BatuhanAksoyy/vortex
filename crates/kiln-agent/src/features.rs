//! What of Kiln's session features a harness has.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// Skills and hooks the chat lists and changes.
    pub skills_and_hooks: bool,
    /// A prompt cache that expires, which the chat counts down.
    pub prompt_cache: bool,
}

impl Features {
    pub const ALL: Self = Self {
        skills_and_hooks: true,
        prompt_cache: true,
    };
}
