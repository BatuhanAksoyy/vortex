//! Subscription usage limits, from the CLI's two sources: the `get_usage`
//! reply (percent, ISO reset times) and the `rate_limit_event` line sent
//! during turns (fractions, Unix reset times).

use std::{
    collections::HashMap,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use kiln_agent::{Limited, SpentTokens, Usage, UsageWindow};
use serde::Deserialize;
use serde_json::Value;

use crate::wire;

#[derive(Deserialize)]
struct Reply {
    rate_limits: Option<ReplyLimits>,
}

#[derive(Deserialize)]
struct ReplyLimits {
    five_hour: Option<ReplyWindow>,
    seven_day: Option<ReplyWindow>,
}

#[derive(Deserialize)]
struct ReplyWindow {
    utilization: f64,
    resets_at: Option<String>,
    locked_reason: Option<Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    status: Option<String>,
    resets_at: Option<u64>,
    error_code: Option<String>,
    #[serde(default)]
    unified_windows: EventWindows,
}

#[derive(Default, Deserialize)]
struct EventWindows {
    five_hour: Option<EventWindow>,
    seven_day: Option<EventWindow>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EventWindow {
    utilization: f64,
    resets_at: Option<u64>,
}

pub fn from_reply(response: &Value) -> Option<Usage> {
    let reply: Reply = decode(response)?;
    let limits = reply.rate_limits?;
    let window = |window: &ReplyWindow| UsageWindow {
        percent: percent(window.utilization),
        resets_at: window.resets_at.as_deref().and_then(parse_time),
    };
    let windows = [limits.five_hour.as_ref(), limits.seven_day.as_ref()];
    let locked = windows
        .into_iter()
        .flatten()
        .filter(|w| {
            w.locked_reason.as_ref().is_some_and(|r| !r.is_null()) || percent(w.utilization) >= 100
        })
        .map(|w| window(w).resets_at);
    Some(Usage {
        session: limits.five_hour.as_ref().map(window),
        weekly: limits.seven_day.as_ref().map(window),
        limited: latest(locked),
    })
}

pub fn from_event(info: &Value) -> Option<Usage> {
    let event: Event = decode(info)?;
    let window = |window: &EventWindow| UsageWindow {
        percent: percent(window.utilization * 100.),
        resets_at: window.resets_at.map(unix_time),
    };
    // A model the plan does not include is refused the same way, but only
    // that model is out of reach; sending with another one still works.
    let refused_model = event.error_code.as_deref() == Some("credits_required");
    let limited =
        (event.status.as_deref() == Some("rejected") && !refused_model).then(|| Limited {
            until: event.resets_at.map(unix_time),
        });
    Some(Usage {
        session: event.unified_windows.five_hour.as_ref().map(window),
        weekly: event.unified_windows.seven_day.as_ref().map(window),
        limited,
    })
}

/// The limit that lasts longest among `resets`, if any limit is reached.
fn latest(resets: impl Iterator<Item = Option<SystemTime>>) -> Option<Limited> {
    resets.fold(None, |limited, until| {
        let until = match limited {
            Some(Limited {
                until: Some(before),
            }) => until.map(|until| until.max(before)).or(Some(before)),
            _ => until,
        };
        Some(Limited { until })
    })
}

/// Whole percent, clamped to 0–100, without lossy casts.
fn percent(value: f64) -> u8 {
    let value = value.round();
    (0..=100u8)
        .rev()
        .find(|n| f64::from(*n) <= value)
        .unwrap_or(0)
}

fn unix_time(seconds: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(seconds)
}

fn parse_time(text: &str) -> Option<SystemTime> {
    text.parse::<jiff::Timestamp>().ok().map(SystemTime::from)
}

fn decode<T: for<'de> Deserialize<'de>>(value: &Value) -> Option<T> {
    T::deserialize(value)
        .map_err(|error| log::warn!("unreadable usage report: {error}"))
        .ok()
}

/// What every model together spent; `None` when the CLI does not say, as
/// after a command it ran itself.
pub fn spent(models: &HashMap<String, wire::ModelUsage>) -> Option<SpentTokens> {
    let mut total = SpentTokens::default();
    for usage in models.values() {
        total += SpentTokens {
            input: usage.input,
            cached: usage.cached,
            written: usage.written,
            output: usage.output,
        };
    }
    (total.total() > 0).then_some(total)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn percent_rounds_and_clamps() {
        assert_eq!(percent(4.4), 4);
        assert_eq!(percent(55.5), 56);
        assert_eq!(percent(130.), 100);
        assert_eq!(percent(-1.), 0);
    }

    #[test]
    fn a_rejected_event_limits_until_its_reset() {
        let usage = from_event(&json!({
            "status": "rejected",
            "resetsAt": 1_800_000_000,
            "unifiedWindows": { "five_hour": { "utilization": 1.0, "resetsAt": 1_800_000_000 } }
        }))
        .unwrap();
        assert_eq!(usage.session.unwrap().percent, 100);
        assert_eq!(
            usage.limited,
            Some(Limited {
                until: Some(unix_time(1_800_000_000))
            })
        );
    }

    #[test]
    fn a_full_window_limits_until_the_latest_reset() {
        let usage = from_reply(&json!({ "rate_limits": {
            "five_hour": { "utilization": 100, "resets_at": "2026-11-20T08:00:00+00:00", "locked_reason": null },
            "seven_day": { "utilization": 100, "resets_at": "2026-11-24T11:00:00+00:00", "locked_reason": null }
        }}))
        .unwrap();
        let weekly = usage.weekly.unwrap().resets_at;
        assert_eq!(usage.limited, Some(Limited { until: weekly }));
    }
}
