//! What subagents wait for: the user's permission, or to be stopped.

use vortex_agent::PermissionRequest;

use super::{EntryId, StopAttempt, Subagent, Transcript};

impl Transcript {
    pub fn stop_asked(&mut self, id: EntryId) {
        if let Some(agent) = self.agents.0.get_mut(&id) {
            agent.stop = Some(StopAttempt::Asked);
        }
    }

    /// The tool `agent` waits for the user's permission to use.
    pub fn waits_for(&self, agent: &Subagent) -> Option<&str> {
        let asked =
            |request: &&PermissionRequest| request.subagent.as_ref() == Some(&agent.call.id);
        let request = self.pending.iter().find(asked)?;
        Some(&request.tool)
    }

    /// Drops the requests of subagents that ended, and the main agent's once
    /// its turn did: the CLI no longer waits for an answer.
    pub(in crate::transcript) fn drop_requests(&mut self, turn_ended: bool) {
        let agents = &self.agents;
        let runs = |call: &String| {
            let mut running = agents.iter().filter(|agent| agent.end.is_none());
            running.any(|agent| agent.call.id == *call)
        };
        self.pending.retain(|request| match &request.subagent {
            Some(call) => runs(call),
            None => !turn_ended,
        });
    }
}
