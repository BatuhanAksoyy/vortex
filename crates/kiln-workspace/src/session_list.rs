//! The sidebar: a new session, and the conversations stored for the
//! project, most recently active first and grouped by day, like Claude
//! Desktop's.

use std::time::{Duration, SystemTime};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, EventEmitter, FontWeight, SharedString, Task, TestSupportExt, Window,
    component::{
        ActiveTheme, Icon, IconName, Sizable as _, button::Button, h_flex, tooltip::Tooltip, v_flex,
    },
    div,
};
use kiln_agent::{PastSession, Pending};

pub enum SessionListEvent {
    New,
    Open { id: String, open_elsewhere: bool },
}

#[derive(Default)]
pub struct SessionList {
    sessions: Vec<PastSession>,
    /// The conversation the chat shows.
    current: Option<String>,
    /// A turn is running; opening another conversation would cut it off.
    busy: bool,
    loaded: bool,
    error: Option<String>,
    load: Option<Task<()>>,
}

impl EventEmitter<SessionListEvent> for SessionList {}

impl SessionList {
    pub fn set_current(&mut self, current: Option<String>, busy: bool, cx: &mut Context<Self>) {
        self.current = current;
        self.busy = busy;
        cx.notify();
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
        cx.notify();
    }

    fn row(&self, session: &PastSession, now: SystemTime, cx: &Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let current = self.current.as_deref() == Some(session.id.as_str());
        let enabled = !current && !self.busy;
        let hover = theme.secondary.opacity(0.5);
        let id = session.id.clone();
        let open_elsewhere = session.open_elsewhere;
        h_flex()
            .id(SharedString::from(format!("session-{}", session.id)))
            .test_support()
            .aria_label(SharedString::from(session.title.clone()))
            .gap_2()
            .px_2()
            .py_1p5()
            .rounded(theme.radius)
            .when(current, |this| {
                this.bg(theme.secondary)
                    .text_color(theme.foreground)
                    .font_weight(FontWeight::MEDIUM)
            })
            .when(!current && self.busy, |this| {
                this.text_color(theme.muted_foreground)
            })
            .when(enabled, |this| {
                this.hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(SessionListEvent::Open {
                            id: id.clone(),
                            open_elsewhere,
                        });
                    }))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(session.title.clone()),
            )
            .child(stamp(session, now, cx))
    }

    /// The rows under a heading per day group, newest first.
    fn grouped(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let now = SystemTime::now();
        let today = jiff::Zoned::now().date();
        let mut shown = Vec::new();
        let mut last_group = None;
        for session in &self.sessions {
            let group = day_group(session.updated_at, today);
            if last_group != Some(group) {
                last_group = Some(group);
                shown.push(heading(group, cx).into_any_element());
            }
            shown.push(self.row(session, now, cx).into_any_element());
        }
        shown
    }

    fn note(&self) -> Option<String> {
        if let Some(error) = &self.error {
            return Some(error.clone());
        }
        (self.loaded && self.sessions.is_empty())
            .then(|| "No sessions in this folder yet.".to_owned())
    }
}

/// When the conversation was last active, and whether another window has
/// it open.
fn stamp(session: &PastSession, now: SystemTime, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    h_flex()
        .flex_none()
        .gap_1()
        .text_xs()
        .text_color(theme.muted_foreground)
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

fn heading(text: &'static str, cx: &App) -> impl IntoElement + use<> {
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

impl Render for SessionList {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let rows = self.grouped(cx);
        v_flex()
            .id("session-list")
            .test_support()
            .size_full()
            .p_2()
            .gap_0p5()
            .text_sm()
            .bg(theme.sidebar)
            .text_color(theme.sidebar_foreground)
            .child(
                Button::new("session-new")
                    .outline()
                    .small()
                    .w_full()
                    .icon(Icon::new(IconName::Plus))
                    .label("New session")
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(SessionListEvent::New))),
            )
            .children(self.note().map(|note| {
                div()
                    .id("session-note")
                    .test_support()
                    .aria_label(SharedString::from(note.clone()))
                    .px_2()
                    .pt_3()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(note)
            }))
            .child(
                v_flex()
                    .id("session-rows")
                    .flex_1()
                    .min_h_0()
                    .gap_0p5()
                    .overflow_y_scroll()
                    .children(rows),
            )
    }
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
