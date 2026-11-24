//! The conversations stored for the project, most recently active first,
//! which each chat's conversations page lists under day headings, like
//! Claude Desktop's, and how a row tells when each was active.

use std::{
    collections::HashSet,
    time::{Duration, SystemTime},
};

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, EventEmitter, FontWeight, SharedString, Task, TestSupportExt,
    component::{ActiveTheme, h_flex, tooltip::Tooltip},
    div,
};
use kiln_agent::{PastSession, Pending};

pub enum SessionListEvent {
    /// A listing finished, or failed.
    Loaded,
}

#[derive(Default)]
pub struct SessionList {
    sessions: Vec<PastSession>,
    /// Conversations with a turn running in one of the chats.
    running: HashSet<String>,
    loaded: bool,
    error: Option<String>,
    load: Option<Task<()>>,
}

impl EventEmitter<SessionListEvent> for SessionList {}

impl SessionList {
    pub fn set_running(&mut self, running: HashSet<String>, cx: &mut Context<Self>) {
        if self.running != running {
            self.running = running;
            cx.notify();
        }
    }

    /// The listed conversations, most recently active first.
    pub fn sessions(&self) -> &[PastSession] {
        &self.sessions
    }

    /// Whether a chat here runs a turn in the conversation `id`.
    pub fn is_running(&self, id: &str) -> bool {
        self.running.contains(id)
    }

    /// What a list says in place of rows: an error, or that there are
    /// none yet.
    pub fn note(&self) -> Option<String> {
        if let Some(error) = &self.error {
            return Some(error.clone());
        }
        (self.loaded && self.sessions.is_empty())
            .then(|| "No sessions in this folder yet.".to_owned())
    }

    /// Whether the conversation `id` is open in another process; `None`
    /// when it is not listed.
    pub fn open_elsewhere(&self, id: &str) -> Option<bool> {
        self.sessions
            .iter()
            .find(|session| session.id == id)
            .map(|session| session.open_elsewhere)
    }

    /// Lists the conversations `sessions` reads, replacing a listing still
    /// under way.
    pub fn reload(&mut self, sessions: Pending<Vec<PastSession>>, cx: &mut Context<Self>) {
        self.load = Some(cx.spawn(async move |this, cx| {
            let listed = cx.background_spawn(sessions).await;
            let _ = this.update(cx, |this, cx| this.show(listed, cx));
        }));
    }

    fn show(&mut self, listed: anyhow::Result<Vec<PastSession>>, cx: &mut Context<Self>) {
        self.loaded = true;
        match listed {
            Ok(sessions) => {
                self.sessions = sessions;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("Cannot list sessions: {error:#}")),
        }
        cx.emit(SessionListEvent::Loaded);
        cx.notify();
    }
}

/// When the conversation was last active, whether a chat here is working on
/// it, and whether another window has it open.
pub fn stamp(
    session: &PastSession,
    running: bool,
    now: SystemTime,
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    h_flex()
        .flex_none()
        .gap_1()
        .text_xs()
        .text_color(theme.muted_foreground)
        .when(running, |this| {
            this.child(
                div()
                    .id(SharedString::from(format!(
                        "session-running-{}",
                        session.id
                    )))
                    .test_support()
                    .aria_label("Running")
                    .size_1p5()
                    .rounded_full()
                    .bg(theme.primary),
            )
        })
        .when(session.open_elsewhere, |this| {
            this.child(
                div()
                    .id(SharedString::from(format!("session-open-{}", session.id)))
                    .test_support()
                    .aria_label("Open in another window")
                    .text_color(theme.success)
                    .child("Open")
                    .tooltip(|window, cx| {
                        Tooltip::new("Open in another window. Continuing here makes a copy.")
                            .build(window, cx)
                    }),
            )
        })
        .child(ago(session.updated_at, now))
}

pub fn heading(text: &'static str, cx: &App) -> impl IntoElement + use<> {
    div()
        .px_2()
        .pt_3()
        .pb_1()
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(cx.theme().muted_foreground)
        .child(text)
}

/// Which heading a conversation last active at `then` falls under, by the
/// local calendar.
pub fn day_group(then: SystemTime, today: jiff::civil::Date) -> &'static str {
    let day = jiff::Timestamp::try_from(then).map_or(today, |stamp| {
        stamp.to_zoned(jiff::tz::TimeZone::system()).date()
    });
    let days = today.since(day).map_or(0, |span| span.get_days()).max(0);
    match days {
        0 => "Today",
        1 => "Yesterday",
        2..7 => "Previous 7 days",
        7..30 => "Previous 30 days",
        _ => "Older",
    }
}

/// How long ago `then` was, in the largest whole unit: "now", "5m", "3h",
/// "2d", "6w", "1y".
pub fn ago(then: SystemTime, now: SystemTime) -> String {
    const UNITS: [(u64, &str); 5] = [
        (365 * 24 * 3600, "y"),
        (7 * 24 * 3600, "w"),
        (24 * 3600, "d"),
        (3600, "h"),
        (60, "m"),
    ];
    let seconds = now.duration_since(then).unwrap_or(Duration::ZERO).as_secs();
    UNITS.iter().find(|(unit, _)| seconds >= *unit).map_or_else(
        || "now".to_owned(),
        |(unit, name)| format!("{}{name}", seconds / unit),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_read_in_their_largest_unit() {
        let now = SystemTime::UNIX_EPOCH + Duration::from_hours(400 * 24);
        let before = |seconds| now - Duration::from_secs(seconds);
        assert_eq!(ago(before(30), now), "now");
        assert_eq!(ago(before(5 * 60), now), "5m");
        assert_eq!(ago(before(3 * 3600 + 59), now), "3h");
        assert_eq!(ago(before(2 * 24 * 3600), now), "2d");
        assert_eq!(ago(before(6 * 7 * 24 * 3600), now), "6w");
        assert_eq!(ago(before(366 * 24 * 3600), now), "1y");
        assert_eq!(
            ago(now + Duration::from_secs(9), now),
            "now",
            "a clock ahead"
        );
    }

    #[test]
    fn conversations_fall_under_their_day() {
        let today = jiff::Zoned::now().date();
        let days_ago = |days: i64| {
            let day = today.checked_sub(jiff::Span::new().days(days)).unwrap();
            let noon = day
                .at(12, 0, 0, 0)
                .to_zoned(jiff::tz::TimeZone::system())
                .unwrap();
            SystemTime::from(noon.timestamp())
        };
        assert_eq!(day_group(days_ago(0), today), "Today");
        assert_eq!(day_group(days_ago(1), today), "Yesterday");
        assert_eq!(day_group(days_ago(6), today), "Previous 7 days");
        assert_eq!(day_group(days_ago(7), today), "Previous 30 days");
        assert_eq!(day_group(days_ago(30), today), "Older");
    }
}
