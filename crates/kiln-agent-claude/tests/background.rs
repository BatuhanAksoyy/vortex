//! Replays a turn that leaves a shell command running in the background:
//! the CLI lists its background tasks when it starts and again when it ends.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod background {
    use kiln_agent::AgentEvent;
    use kiln_agent_claude::Translator;

    use super::common::output_lines;

    #[test]
    fn the_background_tasks_are_counted_as_they_start_and_end() {
        let mut translator = Translator::default();
        let counts: Vec<usize> = output_lines("background")
            .iter()
            .flat_map(|line| translator.on_line(line))
            .filter_map(|event| match event {
                AgentEvent::BackgroundTasks(count) => Some(count),
                _ => None,
            })
            .collect();
        assert_eq!(counts, [1, 0]);
    }
}
