//! The harnesses a chat may run on.

use std::rc::Rc;

use kiln_agent::AgentBackend;

/// Every harness Kiln offers, the default one first.
#[derive(Clone)]
pub struct Backends {
    default: Rc<dyn AgentBackend>,
    all: Rc<[Rc<dyn AgentBackend>]>,
}

impl Backends {
    /// `default` and the `others` after it.
    #[must_use]
    pub fn new(default: Rc<dyn AgentBackend>, others: Vec<Rc<dyn AgentBackend>>) -> Self {
        let all = std::iter::once(default.clone()).chain(others).collect();
        Self { default, all }
    }

    /// The harness named `name` in the settings, such as `codex`; the
    /// default one for a name not offered.
    #[must_use]
    pub fn get(&self, name: &str) -> Rc<dyn AgentBackend> {
        let named = self.all.iter().find(|backend| backend.name() == name);
        named.unwrap_or(&self.default).clone()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Rc<dyn AgentBackend>> {
        self.all.iter()
    }

    #[must_use]
    pub fn names(&self) -> Vec<String> {
        self.iter()
            .map(|backend| backend.name().to_owned())
            .collect()
    }

    /// Whether there is more than one to pick from.
    #[must_use]
    pub fn several(&self) -> bool {
        self.all.len() > 1
    }
}

/// How the UI names harness `name`.
#[must_use]
pub fn title(name: &str) -> &str {
    match name {
        "claude" => "Claude Code",
        "codex" => "Codex",
        other => other,
    }
}
