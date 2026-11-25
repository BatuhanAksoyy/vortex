//! The skills an agent has, and whether it may use them.

/// A skill the agent can load, as its skill settings list it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skill {
    /// The name its settings are saved under, e.g. `plugin:skill`.
    pub name: String,
    /// The name the user types after `/`.
    pub display_name: String,
    pub description: String,
    /// Where it comes from, e.g. `project`, `user` or `plugin`.
    pub source: String,
    /// What listing it costs in the system prompt.
    pub tokens: u64,
    pub state: SkillState,
    /// Set when the state cannot be changed here.
    pub locked_by: Option<SkillLock>,
    /// Whether it is offered as a slash command; a skill that is off is not.
    pub advertised: bool,
}

/// How a skill is offered, from most to least visible.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkillState {
    /// Listed for the model, and the user can run it.
    #[default]
    On,
    /// Listed for the model by name only, without its description.
    NameOnly,
    /// The user can run it; the model does not see it.
    UserOnly,
    /// Hidden from the model and from the command list.
    Off,
}

impl SkillState {
    pub const ALL: [Self; 4] = [Self::On, Self::NameOnly, Self::UserOnly, Self::Off];

    /// The state after this one, wrapping around.
    #[must_use]
    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|state| *state == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

/// Why a skill's state cannot be changed here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillLock {
    /// The skill's own file sets it.
    Author,
    /// Its plugin manages it.
    Plugin,
    /// Managed or start-up settings set it.
    Policy,
    /// Settings cannot hold an entry under its name.
    ReservedName,
}
