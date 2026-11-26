//! Replays an Edit that changes two lines next to each other, whose result
//! carries the patch the CLI shows: the hunk with its context lines.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod edit {
    use kiln_agent::{AgentEvent, Hunk};
    use kiln_agent_claude::Translator;

    use super::common::output_lines;

    fn expected() -> Vec<Hunk> {
        let lines = [
            " one", " two", " three", "-four", "-five", "+FOUR", "+5", " six", " seven", " eight",
        ];
        vec![Hunk {
            old_start: 1,
            new_start: 1,
            lines: lines.map(str::to_owned).to_vec(),
        }]
    }

    fn patch_of(event: &AgentEvent) -> Option<&Vec<Hunk>> {
        match event {
            AgentEvent::ToolResult { patch, .. } if !patch.is_empty() => Some(patch),
            _ => None,
        }
    }

    #[test]
    fn an_edits_result_carries_its_patch() {
        let mut translator = Translator::default();
        let events: Vec<AgentEvent> = output_lines("edit")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect();
        let patches: Vec<_> = events.iter().filter_map(patch_of).collect();
        assert_eq!(patches, [&expected()]);
        let read = events.iter().find(|event| {
            matches!(event, AgentEvent::ToolResult { output, .. } if output.contains("four"))
        });
        assert!(
            read.is_some_and(|read| patch_of(read).is_none()),
            "a Read has no patch"
        );
    }
}
