//! How an agent stands, as the map and its cards show it.

use gpui_kit::prelude::*;
use gpui_kit::{App, Hsla, component::ActiveTheme, div, px};
use kiln_agent::SubagentEnd;

use super::ChatView;
use crate::{
    spinner,
    transcript::{Status, Subagent, Transcript},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Working,
    /// For the user's permission.
    Waiting,
    Failed,
    Stopped,
    Finished,
    /// The conversation's, with no turn running.
    Idle,
}

impl State {
    pub const ALL: [Self; 6] = [
        Self::Working,
        Self::Waiting,
        Self::Failed,
        Self::Stopped,
        Self::Finished,
        Self::Idle,
    ];

    pub fn of(agent: &Subagent, transcript: &Transcript) -> Self {
        match agent.end {
            None if transcript.waits_for(agent).is_some() => Self::Waiting,
            None => Self::Working,
            Some(SubagentEnd::Completed) => Self::Finished,
            Some(SubagentEnd::Failed) => Self::Failed,
            Some(SubagentEnd::Stopped) => Self::Stopped,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Working => "Working",
            Self::Waiting => "Waiting",
            Self::Failed => "Failed",
            Self::Stopped => "Stopped",
            Self::Finished => "Finished",
            Self::Idle => "Idle",
        }
    }

    pub fn color(self, cx: &App) -> Hsla {
        let theme = cx.theme();
        match self {
            Self::Working => spinner::accent(cx),
            Self::Waiting => theme.warning,
            Self::Failed => theme.danger,
            Self::Finished => theme.success,
            Self::Stopped | Self::Idle => theme.muted_foreground,
        }
    }

    pub fn dot(self, cx: &App) -> impl IntoElement + use<> {
        div()
            .flex_none()
            .size(px(7.))
            .rounded_full()
            .bg(self.color(cx))
    }
}

pub fn main_state(chat: &ChatView) -> State {
    if chat.status() == Status::Running {
        State::Working
    } else {
        State::Idle
    }
}
