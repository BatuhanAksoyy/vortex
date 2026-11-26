//! What Kiln's own jobs need of a chat: a single answer from a model on the
//! chat's account, and what the last turn was about.

use gpui_kit::App;
use kiln_agent::{OneShot, Pending};

use super::ChatView;
use crate::transcript::Entry;

impl ChatView {
    /// One answer from `model` on this chat's account, in its folder.
    pub fn ask_once(&self, model: &str, system: &str, prompt: String, cx: &App) -> Pending<String> {
        let env = self
            .account
            .as_deref()
            .map_or_else(Vec::new, |id| self.accounts.read(cx).env(id));
        self.backend.ask_once(OneShot {
            cwd: self.cwd.clone(),
            env,
            model: model.to_owned(),
            system: system.to_owned(),
            prompt,
        })
    }

    /// What the user last asked, and what the agent wrote back since.
    #[must_use]
    pub fn last_turn(&self) -> (String, String) {
        let entries = (0..self.transcript.len()).rev();
        let entries = entries.filter_map(|index| self.transcript.entry(index).map(|(_, e)| e));
        let mut answer = Vec::new();
        for entry in entries {
            match entry {
                Entry::User(message) => {
                    answer.reverse();
                    return (message.text.clone(), answer.join("\n\n"));
                }
                Entry::Text { text, .. } => answer.push(text.clone()),
                _ => {}
            }
        }
        (String::new(), String::new())
    }
}
