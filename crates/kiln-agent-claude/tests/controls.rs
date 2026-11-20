//! Replays the captured control requests: model and mode changes, context
//! usage, and the session info the CLI reports on start.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod controls {
    use kiln_agent::{
        AgentEvent, ContextUsage, PermissionMode, SessionCommand, SessionInfo, TurnOutcome, Usage,
    };
    use kiln_agent_claude::Translator;

    use super::common::{fixture_path, output_lines};

    fn input_lines(name: &str) -> Vec<serde_json::Value> {
        let text = std::fs::read_to_string(fixture_path(name)).unwrap();
        text.lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .filter(|record| record["dir"] == "in")
            .map(|record| record["msg"].clone())
            .collect()
    }

    fn translate_after(translator: &mut Translator, name: &str) -> Vec<AgentEvent> {
        output_lines(name)
            .iter()
            .flat_map(|line| translator.on_line(line))
            .collect()
    }

    #[test]
    fn controls_are_sent_as_recorded_and_their_replies_translated() {
        let mut translator = Translator::default();
        let sent: Vec<_> = [
            SessionCommand::SetModel("sonnet".into()),
            SessionCommand::SetMode(PermissionMode::EditAutomatically),
            SessionCommand::SetMode(PermissionMode::AskBeforeEdits),
            SessionCommand::RefreshContext,
            SessionCommand::RefreshUsage,
        ]
        .into_iter()
        .map(|command| translator.on_command(command).unwrap())
        .collect();
        let recorded = input_lines("controls");
        for (message, recorded) in sent.iter().zip(&recorded[1..]) {
            assert_eq!(message["request_id"], recorded["request_id"]);
            assert_eq!(
                message["request"]["subtype"],
                recorded["request"]["subtype"]
            );
        }

        let events = translate_after(&mut translator, "controls");
        let Some(AgentEvent::Ready(SessionInfo {
            models,
            mode,
            account,
            ..
        })) = events.first()
        else {
            panic!("{events:#?}");
        };
        assert_eq!(account.as_ref().unwrap().plan, "Claude Pro");
        assert_eq!(*mode, PermissionMode::AskBeforeEdits);
        let sonnet = models.iter().find(|model| model.id == "sonnet").unwrap();
        assert_eq!(sonnet.resolved, "claude-sonnet-5-5");
        assert!(events.contains(&AgentEvent::ModelChanged("claude-sonnet-5-5".into())));
        assert!(events.contains(&AgentEvent::ModeChanged(PermissionMode::EditAutomatically)));
        assert!(events.iter().any(
            |e| matches!(e, AgentEvent::CommandFailed(message) if message.contains("permission mode"))
        ));
        assert!(events.iter().any(|e| matches!(
            e,
            AgentEvent::ContextUsage(ContextUsage { max_tokens: 1_000_000, used_tokens }) if *used_tokens > 0
        )));
    }

    #[test]
    fn replies_to_requests_never_sent_are_ignored() {
        let events = translate_after(&mut Translator::default(), "controls");
        assert!(matches!(events.first(), Some(AgentEvent::Ready(_))));
        assert!(!events.iter().any(|e| matches!(
            e,
            AgentEvent::ModelChanged(_)
                | AgentEvent::CommandFailed(_)
                | AgentEvent::ContextUsage(_)
        )));
    }

    #[test]
    fn approving_the_plan_leaves_plan_mode() {
        let mut translator = Translator::default();
        let events = translate_after(&mut translator, "plan");
        let approval = events
            .iter()
            .position(|e| matches!(e, AgentEvent::PermissionRequest(r) if r.tool == "ExitPlanMode"))
            .unwrap();
        let AgentEvent::PermissionRequest(request) = &events[approval] else {
            unreachable!();
        };
        assert!(
            request.input["plan"]
                .as_str()
                .is_some_and(|plan| !plan.is_empty())
        );
        assert_eq!(
            request.remember, None,
            "the plan offers nothing to remember"
        );
        let after: Vec<_> = events[approval..]
            .iter()
            .filter_map(|e| match e {
                AgentEvent::ModeChanged(mode) => Some(*mode),
                _ => None,
            })
            .collect();
        assert_eq!(after, [PermissionMode::AskBeforeEdits]);
    }

    #[test]
    fn plan_usage_is_read_from_the_reply() {
        let mut translator = Translator::default();
        // The recorded request is the fifth Kiln sent: `kiln-usage-5`.
        for _ in 0..5 {
            translator.on_command(SessionCommand::RefreshUsage);
        }
        let events = translate_after(&mut translator, "controls");
        let usage = events
            .iter()
            .find_map(|e| match e {
                AgentEvent::Usage(usage) => Some(*usage),
                _ => None,
            })
            .unwrap();
        let (session, weekly) = (usage.session.unwrap(), usage.weekly.unwrap());
        assert!(session.percent <= 100 && weekly.percent <= 100);
        assert!(session.resets_at.is_some() && weekly.resets_at > session.resets_at);
        assert_eq!(usage.limited, None);
    }

    #[test]
    fn rate_limit_events_during_a_turn_report_usage() {
        let events = translate_after(&mut Translator::default(), "plan");
        let reports: Vec<Usage> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Usage(usage) => Some(*usage),
                _ => None,
            })
            .collect();
        assert_ne!(reports.len(), 0);
        assert!(
            reports
                .iter()
                .all(|usage| usage.session.is_some() && usage.limited.is_none())
        );
    }

    #[test]
    fn a_failed_background_refresh_is_not_reported() {
        let mut translator = Translator::default();
        translator.on_command(SessionCommand::RefreshUsage);
        let error = r#"{"type":"control_response","response":{"subtype":"error","request_id":"kiln-usage-1","error":"network"}}"#;
        assert_eq!(translator.on_line(error), []);
    }

    #[test]
    fn a_turns_usage_report_agrees_with_the_reply_after_it() {
        let mut translator = Translator::default();
        translator.on_command(SessionCommand::RefreshUsage);
        let reports: Vec<Usage> = translate_after(&mut translator, "usage")
            .into_iter()
            .filter_map(|e| match e {
                AgentEvent::Usage(usage) => Some(usage),
                _ => None,
            })
            .collect();
        let [during, reply] = reports[..] else {
            panic!("{reports:#?}");
        };
        let percent = |usage: Usage| i16::from(usage.session.unwrap().percent);
        assert!(
            (percent(during) - percent(reply)).abs() <= 2,
            "{during:?} vs {reply:?}"
        );
    }

    #[test]
    fn a_model_the_plan_lacks_fails_the_turn_without_a_usage_limit() {
        let events = translate_after(&mut Translator::default(), "fallback");
        assert!(events.iter().all(|e| !matches!(
            e,
            AgentEvent::Usage(Usage {
                limited: Some(_),
                ..
            })
        )));
        let failures: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::TurnEnded(TurnOutcome::Failed(message)) => Some(message.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(failures.len(), 2, "{events:#?}");
        assert!(
            failures
                .iter()
                .all(|message| message.contains("requires usage credits"))
        );
        let models: Vec<&str> = events
            .iter()
            .filter_map(|e| match e {
                AgentEvent::Started { model, .. } => Some(model.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(
            models,
            ["claude-fable-5-2", "claude-fable-5-2"],
            "each turn names its model"
        );
    }
}
