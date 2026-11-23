//! The prompt cache button in the composer strip and the panel it opens:
//! until when the cache stays warm, and how the last request read its
//! prompt. A message after the cache expires writes the whole prompt to it
//! again, which costs more than reading it.

use std::time::{Duration, SystemTime};

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, App, Context, SharedString, TestSupportExt,
    component::{
        ActiveTheme, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        v_flex,
    },
    div, px,
};
use kiln_agent::PromptTokens;

use super::{ChatView, StripPanel, header::reset_label};
use crate::agent_state::tokens;

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
            .xsmall()
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

    /// Redraws once the cache expires, so the button says so.
    pub(super) fn sync_cache_timer(&mut self, cx: &mut Context<Self>) {
        let wait = self
            .agent
            .prompt_cache()
            .warm_until()
            .and_then(|until| until.duration_since(SystemTime::now()).ok());
        self.cache_timer = wait.map(|wait| {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
        });
    }
}

/// `Cache 18:05` while warm, `Cache expired` after, `Cache` until known.
fn button_label(shown: Option<&CacheShown>, now: SystemTime) -> String {
    match shown.and_then(|shown| shown.warm_until) {
        Some(until) if until > now => format!("Cache {}", reset_label(until, now)),
        Some(_) => "Cache expired".to_owned(),
        None => "Cache".to_owned(),
    }
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
                .text_xs()
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
            .text_xs()
            .text_color(muted)
            .child(status)
    }))
    .child(div().h_px().bg(cx.theme().border))
    .child(div().text_xs().text_color(muted).child("Last request"))
    .child(row("Read from cache", shown.last.cached))
    .child(row("Written to cache", shown.last.written))
    .child(row("Not cached", shown.last.uncached))
}

fn status(shown: &CacheShown, now: SystemTime) -> Option<String> {
    let until = shown.warm_until?;
    let at = reset_label(until, now);
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
        assert!(button_label(Some(&warm), now).starts_with("Cache "));
        assert!(status(&warm, now).is_some_and(|text| text.contains("5 minutes")));
        let cold = shown(now - Duration::from_mins(1));
        assert_eq!(button_label(Some(&cold), now), "Cache expired");
        assert!(status(&cold, now).is_some_and(|text| text.starts_with("Expired at")));
    }
}
