//! What Kiln's own jobs need of a chat: a single answer from a model on the
//! chat's account, and what the last turn was about.

use gpui_kit::App;
use kiln_agent::{OneShot, Pending};

use super::ChatView;
use crate::transcript::Entry;

/// A question for a model of a harness, outside the conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    /// The harness, by its name in the settings, such as `claude`.
    pub agent: String,
    pub model: String,
    /// What the model is, in place of the agent's own system prompt.
    pub system: String,
    pub prompt: String,
}

impl ChatView {
    /// One answer to `question`, in the chat's folder and on its account.
    pub fn ask_once(&self, question: Question, cx: &App) -> Pending<String> {
        let Question {
            agent,
            model,
            system,
            prompt,
        } = question;
        let Some((backend, account)) = self.asked_backend(&agent, cx) else {
            let missing = format!("Kiln has no agent named `{agent}`.");
            return Box::pin(async move { Err(anyhow::anyhow!(missing)) });
        };
        let env = account.map_or_else(Vec::new, |id| self.accounts.read(cx).env(&id));
        backend.ask_once(OneShot {
            cwd: self.cwd.clone(),
            env,
            model,
            system,
            prompt,
        })
    }

    /// Harness `agent` and the account to ask it on: the chat's own when
    /// it runs on that harness, else the harness's first added account.
    fn asked_backend(
        &self,
        agent: &str,
        cx: &App,
    ) -> Option<(std::rc::Rc<dyn kiln_agent::AgentBackend>, Option<String>)> {
        if agent == self.backend.name() {
            return Some((self.backend.clone(), self.account.clone()));
        }
        let backend = self.backends.as_ref()?.get(agent);
        (backend.name() == agent).then(|| {
            let account = super::first_added(self.accounts.read(cx), agent);
            (backend, account)
        })
    }

    /// Every harness the chat may ask, its own first.
    #[must_use]
    pub fn agent_names(&self) -> Vec<String> {
        let offered = self.offered_agents().into_iter();
        offered.map(str::to_owned).collect()
    }

    /// The name of the chat's harness, such as `claude`.
    #[must_use]
    pub fn agent_name(&self) -> &'static str {
        self.backend.name()
    }

    /// The models the chat's agent offers, once it said.
    #[must_use]
    pub fn models(&self) -> &[kiln_agent::ModelOption] {
        self.agent.models()
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
