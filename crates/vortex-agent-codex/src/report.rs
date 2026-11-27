//! What the app-server reports about the account, its limits, the models
//! and the tokens a thread used, in `vortex-agent`'s terms.

use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vortex_agent::{
    Account, ContextUsage, Effort, Limited, ModelOption, SpentTokens, Usage, UsageWindow,
};

use crate::protocol::{self, RateLimitSnapshot, RateLimitWindow, ThreadTokenUsage};

/// The longest window still counted as the short one; Codex's are five
/// hours and a week on paid plans, a month on the free one.
const SHORT_WINDOW_MINUTES: i64 = 24 * 60;

pub fn usage(snapshot: &RateLimitSnapshot) -> Usage {
    let windows = [&snapshot.primary, &snapshot.secondary];
    let mut usage = Usage::default();
    for window in windows.into_iter().flatten() {
        let short = window
            .window_duration_mins
            .is_some_and(|minutes| minutes <= SHORT_WINDOW_MINUTES);
        let slot = if short {
            &mut usage.session
        } else {
            usage.long_window_days = long_window_days(window);
            &mut usage.weekly
        };
        *slot = Some(usage_window(window));
    }
    if snapshot.rate_limit_reached_type.is_some() {
        let until = windows
            .into_iter()
            .flatten()
            .filter(|window| window.used_percent >= 100)
            .filter_map(|window| window.resets_at)
            .max();
        usage.limited = Some(Limited {
            until: until.map(at),
        });
    }
    usage
}

/// The span of a long window in days, unless it is a week.
fn long_window_days(window: &RateLimitWindow) -> Option<u16> {
    let days = window.window_duration_mins? / (24 * 60);
    (days != 7).then(|| u16::try_from(days).ok()).flatten()
}

fn usage_window(window: &RateLimitWindow) -> UsageWindow {
    UsageWindow {
        percent: u8::try_from(window.used_percent.clamp(0, 100)).unwrap_or(100),
        resets_at: window.resets_at.map(at),
    }
}

/// A time the app-server gives in seconds since the epoch.
pub fn at(seconds: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(u64::try_from(seconds).unwrap_or_default())
}

fn count(tokens: i64) -> u64 {
    u64::try_from(tokens).unwrap_or_default()
}

/// The context the last request filled: its prompt and what it wrote,
/// which the next request reads back.
pub fn context(usage: &ThreadTokenUsage) -> Option<ContextUsage> {
    let max_tokens = count(usage.model_context_window?);
    Some(ContextUsage {
        used_tokens: count(usage.last.total_tokens),
        max_tokens,
        percent: None,
        categories: Vec::new(),
    })
}

pub fn spent(usage: &ThreadTokenUsage) -> SpentTokens {
    let total = &usage.total;
    let cached = count(total.cached_input_tokens);
    SpentTokens {
        input: count(total.input_tokens).saturating_sub(cached),
        cached,
        written: total.cache_write_input_tokens.map_or(0, count),
        output: count(total.output_tokens),
    }
}

/// Each listed model's default effort, by its id and by the model it
/// means, as a thread names either.
pub fn default_efforts(listed: &[protocol::Model]) -> HashMap<String, Effort> {
    listed
        .iter()
        .filter_map(|model| Some((model, effort(&model.default_reasoning_effort)?)))
        .flat_map(|(model, effort)| [(model.id.clone(), effort), (model.model.clone(), effort)])
        .collect()
}

/// `None` for an effort Vortex has no name for, such as `ultra`.
pub fn effort(name: &str) -> Option<Effort> {
    Some(match name {
        "low" => Effort::Low,
        "medium" => Effort::Medium,
        "high" => Effort::High,
        "xhigh" => Effort::Xhigh,
        "max" => Effort::Max,
        _ => return None,
    })
}

pub fn effort_name(effort: Effort) -> &'static str {
    match effort {
        Effort::Low => "low",
        Effort::Medium => "medium",
        Effort::High => "high",
        Effort::Xhigh => "xhigh",
        Effort::Max => "max",
    }
}

/// The models the user may pick, the default one first.
pub fn models(listed: &[protocol::Model]) -> Vec<ModelOption> {
    let mut shown: Vec<&protocol::Model> = listed.iter().filter(|model| !model.hidden).collect();
    shown.sort_by_key(|model| !model.is_default);
    shown
        .into_iter()
        .map(|model| ModelOption {
            id: model.id.clone(),
            resolved: model.model.clone(),
            name: model.display_name.clone(),
            description: model.description.clone(),
            efforts: (model.supported_reasoning_efforts.iter())
                .filter_map(|option| effort(&option.reasoning_effort))
                .collect(),
        })
        .collect()
}

pub fn account(account: &protocol::Account) -> Option<Account> {
    let protocol::Account::Chatgpt(chatgpt) = account else {
        return None;
    };
    Some(Account {
        email: chatgpt.email.clone().unwrap_or_default(),
        organization: chatgpt.workspace_name.clone().unwrap_or_default(),
        plan: plan_name(chatgpt.plan_type).to_owned(),
    })
}

fn plan_name(plan: protocol::PlanType) -> &'static str {
    use protocol::PlanType as Plan;
    match plan {
        Plan::Free => "ChatGPT Free",
        Plan::Go => "ChatGPT Go",
        Plan::Plus => "ChatGPT Plus",
        Plan::Pro | Plan::Prolite | Plan::Promax => "ChatGPT Pro",
        Plan::Team => "ChatGPT Team",
        Plan::Business | Plan::SelfServeBusinessProlite | Plan::SelfServeBusinessUsageBased => {
            "ChatGPT Business"
        }
        Plan::Edu | Plan::EduPlus | Plan::EduPro => "ChatGPT Edu",
        Plan::Enterprise
        | Plan::Ent26
        | Plan::EnterpriseCbpAutomation
        | Plan::EnterpriseCbpUsageBased => "ChatGPT Enterprise",
        _ => "ChatGPT",
    }
}

#[cfg(test)]
mod tests {
    use crate::protocol::{RateLimitSnapshot, RateLimitWindow};

    use super::usage;

    fn window(minutes: i64, used_percent: i64) -> RateLimitWindow {
        RateLimitWindow {
            resets_at: Some(1000),
            used_percent,
            window_duration_mins: Some(minutes),
        }
    }

    #[test]
    fn the_short_window_is_the_session_and_the_long_one_the_week() {
        let snapshot = RateLimitSnapshot {
            primary: Some(window(300, 42)),
            secondary: Some(window(10_080, 7)),
            ..RateLimitSnapshot::default()
        };
        let usage = usage(&snapshot);
        assert_eq!(usage.session.map(|w| w.percent), Some(42));
        assert_eq!(usage.weekly.map(|w| w.percent), Some(7));
        assert_eq!(usage.limited, None);
    }

    #[test]
    fn a_month_long_free_window_counts_as_the_week() {
        let snapshot = RateLimitSnapshot {
            primary: Some(window(43_200, 100)),
            rate_limit_reached_type: Some(crate::protocol::RateLimitReachedType::RateLimitReached),
            ..RateLimitSnapshot::default()
        };
        let usage = usage(&snapshot);
        assert_eq!(usage.session, None);
        assert_eq!(usage.weekly.map(|w| w.percent), Some(100));
        assert_eq!(usage.limited.and_then(|l| l.until), Some(super::at(1000)));
    }
}
