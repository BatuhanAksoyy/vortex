//! The prompt cache button in the composer strip and the panel it opens:
//! until when the cache stays warm, and how the last request read its
//! prompt. A message after the cache expires writes the whole prompt to it
//! again, which costs more than reading it.

use std::time::{Duration, SystemTime};

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, App, AsyncApp, Context, SharedString, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        v_flex,
    },
    div, px,
};
use kiln_agent::PromptTokens;

use super::{ChatView, StripPanel};
use crate::{
    agent_state::tokens,
    clock,
    scale::{ScaledButton as _, ScaledText as _},
};

const PANEL_WIDTH: f32 = 300.;

const EMPTY: &str = "Nothing cached yet in this session. The cache shows after the next message.";

/// The prompt cache as the button and panel show it, copied out of the
/// view each render.
struct CacheShown {
    last: PromptTokens,
    hit_percent: Option<u64>,
    kept_for: Option<Duration>,
    warm_until: Option<SystemTime>,
}

impl ChatView {
    pub(super) fn cache_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let cache = self.agent.prompt_cache();
        let shown = cache.last().map(|last| CacheShown {
            last,
            hit_percent: cache.hit_percent(),
            kept_for: cache.kept_for(),
            warm_until: cache.warm_until(),
        });
        let now = SystemTime::now();
        let expired = shown
            .as_ref()
            .and_then(|shown| shown.warm_until)
            .is_some_and(|until| until <= now);
        let trigger = Button::new("chat-cache")
            .ghost()
            .control()
            .px_1p5()
            .label(button_label(shown.as_ref(), now))
            .text_color(if expired {
                cx.theme().warning
            } else {
                cx.theme().muted_foreground
            })
            .tooltip("Prompt cache");
        let view = cx.entity().downgrade();
        Popover::new("chat-cache-popover")
            .anchor(Anchor::BottomRight)
            .trigger(trigger)
            .open(self.is_open(StripPanel::Cache))
            .on_open_change(move |open, _, cx| {
                let _ = view.update(cx, |view, cx| view.set_cache_open(*open, cx));
            })
            .content(move |_, _, cx| panel(shown.as_ref(), cx))
    }

    pub(super) fn set_cache_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.show_panel(StripPanel::Cache, open);
        cx.notify();
    }

    /// Redraws each time the minutes left change, and once the cache
    /// expires, so the button says so.
    pub(super) fn sync_cache_timer(&mut self, cx: &mut Context<Self>) {
        let until = self.agent.prompt_cache().warm_until();
        self.cache_timer =
            until.map(|until| cx.spawn(async move |this, cx| count_down(this, until, cx).await));
    }
}

async fn count_down(this: WeakEntity<ChatView>, until: SystemTime, cx: &mut AsyncApp) {
    let mut alive = true;
    while alive && let Ok(left) = until.duration_since(SystemTime::now()) {
        cx.background_executor().timer(next_change(left)).await;
        alive = this.update(cx, |_, cx| cx.notify()).is_ok();
    }
}

/// `Cache 30m` while warm, with the time left, `Cache expired` after,
/// `Cache` until known.
fn button_label(shown: Option<&CacheShown>, now: SystemTime) -> String {
    match shown
        .and_then(|shown| shown.warm_until)
        .map(|until| until.duration_since(now))
    {
        Some(Ok(left)) if !left.is_zero() => format!("Cache {}", time_left(left)),
        Some(_) => "Cache expired".to_owned(),
        None => "Cache".to_owned(),
    }
}

/// Whole minutes left, rounded up so the last one reads `1m`, not `0m`;
/// `1h` for the hour a long-kept cache starts with.
fn time_left(left: Duration) -> String {
    let minutes = left.as_secs().div_ceil(60);
    if minutes >= 60 && minutes.is_multiple_of(60) {
        format!("{}h", minutes / 60)
    } else if minutes >= 60 {
        format!("{}h {}m", minutes / 60, minutes % 60)
    } else {
        format!("{minutes}m")
    }
}

/// How long until [`time_left`] reads differently, with `left` to go: just
/// past the next whole minute, or when it runs out.
fn next_change(left: Duration) -> Duration {
    const MINUTE: Duration = Duration::from_mins(1);
    let into_minute = Duration::from_millis(
        u64::try_from(left.as_millis() % MINUTE.as_millis()).unwrap_or_default(),
    );
    let wait = if into_minute.is_zero() {
        MINUTE
    } else {
        into_minute
    };
    // Past the boundary, so the minutes are already one fewer.
    wait + Duration::from_millis(50)
}

fn panel(shown: Option<&CacheShown>, cx: &App) -> impl IntoElement + use<> {
    let muted = cx.theme().muted_foreground;
    let body = v_flex()
        .id("chat-cache-panel")
        .test_support()
        .w(px(PANEL_WIDTH))
        .gap_2()
        .text_sm();
    let Some(shown) = shown else {
        return body.child(h_flex().child("Prompt cache")).child(
            div()
                .id("chat-cache-empty")
                .test_support()
                .aria_label(SharedString::from(EMPTY))
                .text_caption()
                .text_color(muted)
                .child(EMPTY),
        );
    };
    let row = |label: &'static str, count: u64| {
        h_flex().justify_between().child(label).child(tokens(count))
    };
    body.child(
        h_flex()
            .justify_between()
            .child("Prompt cache")
            .children(shown.hit_percent.map(|hit| {
                div()
                    .text_color(muted)
                    .child(format!("{hit}% read from cache"))
            })),
    )
    .children(status(shown, SystemTime::now()).map(|status| {
        div()
            .id("chat-cache-status")
            .test_support()
            .aria_label(SharedString::from(status.clone()))
            .text_caption()
            .text_color(muted)
            .child(status)
    }))
    .child(div().h_px().bg(cx.theme().border))
    .child(div().text_caption().text_color(muted).child("Last request"))
    .child(row("Read from cache", shown.last.cached))
    .child(row("Written to cache", shown.last.written))
    .child(row("Not cached", shown.last.uncached))
}

fn status(shown: &CacheShown, now: SystemTime) -> Option<String> {
    let until = shown.warm_until?;
    let at = clock::reset_label(until, now);
    if until <= now {
        return Some(format!(
            "Expired at {at} · the next message writes the prompt to the cache again"
        ));
    }
    Some(match shown.kept_for.map(|kept| kept.as_secs() / 60) {
        Some(60) => format!("Warm until {at} · kept for an hour after each use"),
        Some(minutes) => format!("Warm until {at} · kept for {minutes} minutes after each use"),
        None => format!("Warm until {at}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_time_left_reads_in_whole_minutes_rounded_up() {
        assert_eq!(time_left(Duration::from_secs(29 * 60 + 1)), "30m");
        assert_eq!(time_left(Duration::from_secs(20)), "1m");
        assert_eq!(time_left(Duration::from_hours(1)), "1h");
        assert_eq!(time_left(Duration::from_mins(75)), "1h 15m");
        let left = Duration::from_secs(4 * 60 + 10);
        assert_eq!(next_change(left), Duration::from_millis(10_050));
        assert_eq!(
            time_left(left.saturating_sub(next_change(left))),
            "4m",
            "one fewer once it fires"
        );
    }

    fn shown(warm_until: SystemTime) -> CacheShown {
        CacheShown {
            last: PromptTokens::default(),
            hit_percent: None,
            kept_for: Some(Duration::from_mins(5)),
            warm_until: Some(warm_until),
        }
    }

    #[test]
    fn the_button_says_when_the_cache_expires_and_once_it_has() {
        let now = SystemTime::now();
        assert_eq!(button_label(None, now), "Cache");
        let warm = shown(now + Duration::from_mins(5));
        assert_eq!(button_label(Some(&warm), now), "Cache 5m");
        assert!(status(&warm, now).is_some_and(|text| text.contains("5 minutes")));
        let cold = shown(now - Duration::from_mins(1));
        assert_eq!(button_label(Some(&cold), now), "Cache expired");
        assert!(status(&cold, now).is_some_and(|text| text.starts_with("Expired at")));
    }
}
