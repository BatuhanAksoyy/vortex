//! Replays captured app-server sessions (`tests/fixtures/*.jsonl`, made with
//! capture.py and scrub.py) through the translator.

// Inside a `cfg(test)` module so helpers may `unwrap`, like unit tests.
#[cfg(test)]
mod common;

#[cfg(test)]
mod replay {
    use serde_json::Value;
    use vortex_agent::{
        AgentEvent, Decision, Effort, PermissionRequest, SessionCommand, TurnOutcome,
    };
    use vortex_agent_codex::{Start, Translator, protocol};

    use super::common::{records, text, translate};

    const FIXTURES: &[&str] = &[
        "controls",
        "turn",
        "command",
        "deny",
        "edit",
        "interrupt",
        "resume",
        "history",
        "picture",
        "subagent",
        "thinking",
        "mcp",
        "login",
    ];

    fn permission(events: &[AgentEvent]) -> &PermissionRequest {
        events
            .iter()
            .find_map(|event| match event {
                AgentEvent::PermissionRequest(request) => Some(request),
                _ => None,
            })
            .unwrap()
    }

    fn result(events: &[AgentEvent]) -> (&str, bool, usize) {
        events
            .iter()
            .find_map(|event| match event {
                AgentEvent::ToolResult {
                    output,
                    is_error,
                    patch,
                    ..
                } => Some((output.as_str(), *is_error, patch.len())),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn fixtures_hold_no_personal_data() {
        for name in FIXTURES {
            let text = std::fs::read_to_string(format!(
                "{}/tests/fixtures/{name}.jsonl",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            for needle in ["/home/", "/Users/", "/tmp/", "@gmail", "@icloud"] {
                assert!(!text.contains(needle), "{name} contains {needle:?}");
            }
        }
    }

    fn reads<T: serde::de::DeserializeOwned>(value: &Value) -> bool {
        serde_json::from_value::<T>(value.clone()).is_ok()
    }

    /// Whether the generated type for `msg`'s method reads it; true for a
    /// message Vortex does not handle.
    fn decodes(msg: &Value) -> bool {
        let params = &msg["params"];
        match msg["method"].as_str() {
            Some("item/started") => reads::<protocol::ItemStartedNotification>(params),
            Some("item/completed") => reads::<protocol::ItemCompletedNotification>(params),
            Some("turn/completed") => reads::<protocol::TurnCompletedNotification>(params),
            Some("thread/tokenUsage/updated") => {
                reads::<protocol::ThreadTokenUsageUpdatedNotification>(params)
            }
            Some("account/rateLimits/updated") => {
                reads::<protocol::AccountRateLimitsUpdatedNotification>(params)
            }
            Some("item/commandExecution/requestApproval") => {
                reads::<protocol::CommandExecutionRequestApprovalParams>(params)
            }
            Some("item/fileChange/requestApproval") => {
                reads::<protocol::FileChangeRequestApprovalParams>(params)
            }
            _ => true,
        }
    }

    /// The generated types read every message Vortex handles, so a change in
    /// the protocol shows here rather than as a message silently dropped.
    #[test]
    fn every_handled_message_decodes() {
        for name in FIXTURES {
            for (dir, msg) in records(name) {
                assert!(decodes(&msg), "{name} {dir}: {msg}");
            }
        }
    }

    #[test]
    fn a_turn_streams_its_answer_and_ends() {
        let events = translate("turn");
        assert!(matches!(&events[0], AgentEvent::Started { model, .. } if model == "gpt-6-luna"));
        assert_eq!(text(&events), "ok");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::ContextUsage(c) if c.max_tokens > 0))
        );
        assert!(
            events
                .iter()
                .any(|e| matches!(e, AgentEvent::Usage(u) if u.weekly.is_some() && u.long_window_days == Some(30))),
            "the free plan has one 30-day window"
        );
        assert_eq!(
            events.last(),
            Some(&AgentEvent::TurnEnded(TurnOutcome::Completed))
        );
    }

    #[test]
    fn the_account_and_its_plan_are_read() {
        let events = translate("controls");
        let Some(AgentEvent::Ready(info)) =
            events.iter().find(|e| matches!(e, AgentEvent::Ready(_)))
        else {
            panic!("no Ready in {events:?}");
        };
        let account = info.account.clone().unwrap();
        assert_eq!(account.email, "person.example");
        assert_eq!(account.plan, "ChatGPT Free");
        assert_eq!(info.models[0].id, "gpt-6-luna", "the default model first");
    }

    #[test]
    fn a_thread_without_an_effort_takes_its_models_default() {
        // Codex reports a null effort when the user's configuration sets
        // none; the effort menu then needs the model's default.
        let events = translate("controls");
        assert!(
            events.contains(&AgentEvent::EffortChanged(Some(Effort::Medium))),
            "{events:?}"
        );
        assert!(!events.contains(&AgentEvent::EffortChanged(None)));
    }

    /// The models the server listed in `name`'s capture.
    fn listed_models(name: &str) -> Value {
        let records = records(name);
        let asked = records.iter().find_map(|(dir, msg)| {
            (dir == "in" && msg["method"] == "model/list").then(|| msg["id"].clone())
        });
        let asked = asked.unwrap();
        let answer = records
            .iter()
            .find(|(dir, msg)| dir == "out" && msg["id"] == asked && msg.get("method").is_none());
        answer.unwrap().1["result"].clone()
    }

    #[test]
    fn models_asked_for_after_ready_are_a_change_not_another_ready() {
        let mut translator = Translator::new(Start::default());
        let mut ready = 0;
        for (dir, msg) in records("controls") {
            match dir.as_str() {
                "in" => translator.on_sent(&msg.to_string()),
                "out" => {
                    let events = translator.on_line(&msg.to_string()).events;
                    ready += events
                        .iter()
                        .filter(|e| matches!(e, AgentEvent::Ready(_)))
                        .count();
                }
                _ => {}
            }
        }
        assert_eq!(ready, 1);
        // As the chat does after every Ready and every turn.
        let asked = translator.on_command(SessionCommand::RefreshModels).out;
        let id = asked[0]["id"].clone();
        let reply = serde_json::json!({ "id": id, "result": listed_models("controls") });
        let events = translator.on_line(&reply.to_string()).events;
        assert!(
            matches!(events.as_slice(), [AgentEvent::ModelsChanged(models)] if !models.is_empty()),
            "{events:?}"
        );
    }

    #[test]
    fn an_approved_command_runs_as_a_bash_call() {
        let events = translate("command");
        let call = events.iter().find_map(|e| match e {
            AgentEvent::ToolCall(call) => Some(call),
            _ => None,
        });
        let call = call.unwrap();
        assert_eq!(
            (call.name.as_str(), &call.input["command"]),
            ("Bash", &Value::from("echo vortex"))
        );
        let asked = permission(&events);
        assert_eq!(asked.tool, "Bash");
        assert_eq!(asked.input["command"], "echo vortex");
        assert_eq!(result(&events), ("vortex\n", false, 0));
        assert!(text(&events).ends_with("vortex"));
    }

    #[test]
    fn a_declined_command_ends_as_an_error() {
        let events = translate("deny");
        assert_eq!(result(&events), ("The command was declined.", true, 0));
    }

    #[test]
    fn a_file_change_shows_its_file_and_patch() {
        let events = translate("edit");
        let asked = permission(&events);
        assert_eq!(
            (asked.tool.as_str(), &asked.input["file_path"]),
            ("Edit", &Value::from("/work/note.txt"))
        );
        assert_eq!(result(&events), ("", false, 1));
    }

    #[test]
    fn an_interrupted_turn_ends_as_interrupted() {
        let events = translate("interrupt");
        assert!(events.contains(&AgentEvent::TurnEnded(TurnOutcome::Interrupted)));
    }

    #[test]
    fn a_resumed_thread_keeps_its_id() {
        let records = records("resume");
        let first = records[0].1["first_thread"].as_str().unwrap().to_owned();
        let events = translate("resume");
        assert!(
            matches!(&events[0], AgentEvent::Started { session_id, .. } if *session_id == first)
        );
        assert_eq!(text(&events), "marigold");
    }

    #[test]
    fn a_picture_reaches_the_model() {
        assert_eq!(text(&translate("picture")), "Red");
    }

    #[test]
    fn the_users_decision_answers_the_request_it_was_asked_under() {
        let mut translator = Translator::new(Start::default());
        let (_, asked) = records("command")
            .into_iter()
            .find(|(dir, msg)| {
                dir == "out" && msg["method"] == "item/commandExecution/requestApproval"
            })
            .unwrap();
        let step = translator.on_line(&asked.to_string());
        let asked = permission(&step.events);
        let answer = translator.on_command(SessionCommand::Permission {
            request_id: asked.id.clone(),
            decision: Decision::Allow,
        });
        let accepted = serde_json::json!({"id": 0, "result": {"decision": "accept"}});
        assert_eq!(answer.out, [accepted]);
    }

    #[test]
    fn vortexs_tool_server_is_given_with_its_tools_approved() {
        let tools = vortex_agent::ToolServer {
            name: "vortex".into(),
            command: "/Applications/Vortex".into(),
            args: vec!["mcp".into()],
            env: vec![("VORTEX_BRIDGE_TOKEN".into(), "abc".into())],
            commits: true,
            commit_directives: Vec::new(),
        };
        let mut translator = Translator::new(Start {
            tools: Some(tools),
            ..Start::default()
        });
        let initialize = translator.begin();
        let (_, answered) = records("mcp")
            .into_iter()
            .find(|(dir, msg)| dir == "out" && msg["result"]["userAgent"].is_string())
            .unwrap();
        let answered = serde_json::json!({"id": initialize["id"], "result": answered["result"]});
        let out = translator.on_line(&answered.to_string()).out;
        let thread = out
            .iter()
            .find(|msg| msg["method"] == "thread/start")
            .unwrap();
        let vortex = &thread["params"]["config"]["mcp_servers"]["vortex"];
        assert_eq!(vortex["default_tools_approval_mode"], "approve");
        assert_eq!(vortex["env"]["VORTEX_BRIDGE_TOKEN"], "abc");
        let told = thread["params"]["developerInstructions"].as_str().unwrap();
        assert!(told.contains("mcp__vortex__commit"), "{told}");
    }

    #[test]
    fn an_mcp_call_shows_as_the_servers_tool_with_its_picture() {
        let events = translate("mcp");
        let call = events.iter().find_map(|event| match event {
            AgentEvent::ToolCall(call) => Some(call),
            _ => None,
        });
        assert_eq!(call.unwrap().name, "mcp__vortex__page_title");
        let result = events.iter().find_map(|event| match event {
            AgentEvent::ToolResult {
                output,
                pictures,
                is_error,
                ..
            } => Some((output, pictures, *is_error)),
            _ => None,
        });
        let (output, pictures, is_error) = result.unwrap();
        assert_eq!(output, "The page is titled Vortex.");
        assert!(!is_error);
        assert_eq!(pictures.len(), 1);
        assert_eq!(pictures[0].media_type, "image/png");
        assert!(pictures[0].data.starts_with(b"\x89PNG"));
        assert!(text(&events).contains("Vortex"), "{}", text(&events));
    }
}
