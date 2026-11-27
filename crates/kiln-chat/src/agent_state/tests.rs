use std::time::Duration;

use super::*;
use kiln_agent::SessionInfo;

fn option(id: &str, resolved: &str, name: &str) -> ModelOption {
    ModelOption {
        id: id.into(),
        resolved: resolved.into(),
        name: name.into(),
        description: String::new(),
        efforts: Vec::new(),
    }
}

fn ready() -> AgentEvent {
    AgentEvent::Ready(SessionInfo {
        models: vec![
            option("default", "claude-opus-5-5", "Default (recommended)"),
            option("opus", "claude-opus-5-5", "Opus 5.5"),
            option("sonnet", "claude-sonnet-5-5", "Sonnet 5.5"),
        ],
        mode: PermissionMode::AskBeforeEdits,
        account: None,
        commands: Vec::new(),
    })
}

#[test]
fn the_running_model_shows_by_its_own_name_not_the_default_alias() {
    let mut state = AgentState::default();
    state.apply(&ready());
    state.apply(&AgentEvent::Started {
        session_id: "s".into(),
        model: "claude-opus-5-5".into(),
        cli_version: None,
    });
    assert_eq!(state.model_label().as_deref(), Some("Opus 5.5"));
    state.choose_model("default");
    assert_eq!(
        state.model_label().as_deref(),
        Some("Default (recommended)")
    );
}

#[test]
fn before_the_first_turn_the_default_option_shows() {
    let mut state = AgentState::default();
    state.apply(&ready());
    assert_eq!(
        state.model_label().as_deref(),
        Some("Default (recommended)")
    );
}

#[test]
fn the_model_the_agent_reports_wins_over_the_pick() {
    let mut state = AgentState::default();
    state.apply(&ready());
    state.choose_model("sonnet");
    assert_eq!(state.model_label().as_deref(), Some("Sonnet 5.5"));
    state.apply(&AgentEvent::Started {
        session_id: "s".into(),
        model: "claude-opus-5-5".into(),
        cli_version: None,
    });
    assert_eq!(state.model_label().as_deref(), Some("Opus 5.5"));
}

#[test]
fn an_unknown_model_shows_its_id() {
    let mut state = AgentState::default();
    state.apply(&AgentEvent::ModelChanged("claude-x".into()));
    assert_eq!(state.model_label().as_deref(), Some("claude-x"));
}

#[test]
fn mode_is_unknown_until_reported() {
    let mut state = AgentState::default();
    assert_eq!(state.start_mode(), None);
    state.apply(&AgentEvent::ModeChanged(PermissionMode::Plan));
    assert_eq!(state.start_mode(), Some(PermissionMode::Plan));
}

#[test]
fn context_percent_rounds_up() {
    let mut state = AgentState::default();
    assert_eq!(state.context_percent(), None);
    state.apply(&AgentEvent::ContextUsage(ContextUsage {
        used_tokens: 15_338,
        max_tokens: 1_000_000,
        ..ContextUsage::default()
    }));
    assert_eq!(state.context_percent(), Some(2));
    state.apply(&AgentEvent::ContextUsage(ContextUsage {
        used_tokens: 13_925,
        max_tokens: 400_000,
        percent: Some(3),
        ..ContextUsage::default()
    }));
    assert_eq!(state.context_percent(), Some(3), "the agent's figure wins");
}

#[test]
fn shift_tab_cycles_the_menu_modes() {
    assert_eq!(
        next_mode(PermissionMode::AskBeforeEdits),
        PermissionMode::EditAutomatically
    );
    assert_eq!(
        next_mode(PermissionMode::Auto),
        PermissionMode::AskBeforeEdits
    );
    assert_eq!(
        next_mode(PermissionMode::DontAsk),
        PermissionMode::AskBeforeEdits
    );
}

#[test]
fn a_partial_report_keeps_the_other_window() {
    let window = |percent| kiln_agent::UsageWindow {
        percent,
        resets_at: None,
    };
    let mut state = AgentState::default();
    state.apply(&AgentEvent::Usage(Usage {
        session: Some(window(4)),
        weekly: Some(window(19)),
        limited: None,
        ..Usage::default()
    }));
    state.apply(&AgentEvent::Usage(Usage {
        session: Some(window(5)),
        weekly: None,
        limited: None,
        ..Usage::default()
    }));
    assert_eq!(state.usage().weekly, Some(window(19)));
    assert_eq!(state.usage().session, Some(window(5)));
}

#[test]
fn a_limit_ends_at_its_reset_time() {
    let now = SystemTime::now();
    let hour = std::time::Duration::from_secs(3600);
    let mut state = AgentState::default();
    state.apply(&AgentEvent::Usage(Usage {
        limited: Some(Limited {
            until: Some(now + hour),
        }),
        ..Usage::default()
    }));
    assert!(state.limit_at(now).is_some());
    assert!(state.limit_at(now + hour).is_none());
}

#[test]
fn token_counts_are_compact() {
    assert_eq!(tokens(950), "950");
    assert_eq!(tokens(15_338), "15.3k");
    assert_eq!(tokens(200_000), "200k");
    assert_eq!(tokens(1_000_000), "1M");
}

#[test]
fn each_account_keeps_its_own_usage() {
    let now = SystemTime::now();
    let mut state = AgentState::default();
    let limited = Usage {
        limited: Some(Limited {
            until: Some(now + Duration::from_hours(1)),
        }),
        ..Usage::default()
    };
    state.apply(&AgentEvent::Usage(limited));
    state.apply(&AgentEvent::ContextUsage(ContextUsage {
        used_tokens: 10,
        max_tokens: 100,
        ..ContextUsage::default()
    }));
    state.change_account(Some("a".into()), Some("b"));
    assert_eq!(
        state.usage(),
        Usage::default(),
        "b's usage is not known yet"
    );
    assert_eq!(state.usage_updated_at(), None);
    assert!(state.left_limit_at("a", now).is_some());
    assert_eq!(state.context_percent(), Some(10), "the conversation stays");
    state.change_account(Some("b".into()), Some("a"));
    assert_eq!(state.usage(), limited, "a's comes back");
    assert!(state.left_limit_at("a", now).is_none());
    assert!(state.left_limit_at("b", now).is_none());
}

#[test]
fn a_fresh_model_list_replaces_the_one_the_agent_started_with() {
    let mut state = AgentState::default();
    state.apply(&ready());
    state.apply(&AgentEvent::Started {
        session_id: "s".into(),
        model: "claude-opus-5-5".into(),
        cli_version: None,
    });
    let ended = AgentEvent::TurnEnded(kiln_agent::TurnOutcome::Completed);
    assert!(AgentState::refreshes_after(&ended).contains(&SessionCommand::RefreshModels));
    assert!(
        AgentState::refreshes_after(&ready()).contains(&SessionCommand::RefreshModels),
        "the list it starts with is short"
    );
    state.apply(&AgentEvent::ModelsChanged(vec![
        option("default", "claude-opus-5-5", "Default (recommended)"),
        option("opus", "claude-opus-5-5", "Opus"),
    ]));
    assert_eq!(state.models().len(), 2);
    assert_eq!(state.model_label().as_deref(), Some("Opus"));
}

#[test]
fn the_context_is_read_again_as_each_request_of_a_turn_starts() {
    let request = AgentEvent::PromptTokens(kiln_agent::PromptTokens::default());
    assert_eq!(
        AgentState::refreshes_after(&request),
        [SessionCommand::RefreshContext]
    );
}

#[test]
fn recalled_usage_replaces_only_older_usage() {
    let now = SystemTime::now();
    let at = |percent: u8| Usage {
        session: Some(kiln_agent::UsageWindow {
            percent,
            resets_at: None,
        }),
        ..Usage::default()
    };
    let mut state = AgentState::default();
    state.recall_usage(at(10), Some(now));
    state.recall_usage(at(20), Some(now - Duration::from_mins(1)));
    assert_eq!(state.usage(), at(10), "an older report is left out");
    state.recall_usage(at(30), Some(now + Duration::from_mins(1)));
    assert_eq!(state.usage(), at(30), "another chat learned newer usage");
}
