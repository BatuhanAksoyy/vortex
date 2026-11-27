//! Which harnesses' CLIs are missing, and whether one is being installed.
//! The window that checks the installs keeps it; a chat on a missing CLI
//! offers to install it, and a missing other harness is not offered.

use std::collections::HashMap;

use gpui_kit::{App, Global};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Setup {
    Missing,
    Installing,
    /// The installation failed, with why.
    Failed(String),
}

/// By harness name; a harness not listed is installed, or not checked yet.
#[derive(Debug, Default)]
pub struct AgentSetup(HashMap<String, Setup>);

impl Global for AgentSetup {}

impl AgentSetup {
    #[must_use]
    pub fn of(agent: &str, cx: &App) -> Option<Setup> {
        cx.try_global::<Self>()?.0.get(agent).cloned()
    }

    #[must_use]
    pub fn is_missing(agent: &str, cx: &App) -> bool {
        Self::of(agent, cx).is_some()
    }

    /// `None` marks it installed.
    pub fn set(agent: &str, setup: Option<Setup>, cx: &mut App) {
        let all = &mut cx.default_global::<Self>().0;
        match setup {
            Some(setup) => all.insert(agent.to_owned(), setup),
            None => all.remove(agent),
        };
    }
}

/// Whether `error`, from starting the agent, says its program is missing.
pub(crate) fn is_not_found(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| {
        cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound)
    })
}
