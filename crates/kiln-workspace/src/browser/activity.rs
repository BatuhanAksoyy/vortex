//! What the agent does on the page, shown as it does it: the element it is
//! about to use, where it clicks, and a line saying what it is doing.

use std::time::Duration;

use gpui_kit::prelude::*;
use gpui_kit::{
    Animation, AnimationExt as _, AnyElement, Context, SharedString, Task, TestSupportExt,
    component::{ActiveTheme, Sizable as _, h_flex, spinner::Spinner},
    div, ease_out_quint, px,
};
use kiln_bridge::Browse;
use kiln_browser::Target;

use super::Browser;

/// How long the element shows before it is used, so the user sees what is
/// coming.
pub(super) const LOOK: Duration = Duration::from_millis(350);
/// How long what was done stays on the page after.
const LINGER: Duration = Duration::from_millis(1500);
const RING: f32 = 36.;
const RING_TIME: Duration = Duration::from_millis(600);
/// Names longer than this are cut in the line.
const MAX_NAME: usize = 40;

#[derive(Default)]
pub(super) struct Activity {
    doing: Option<Doing>,
    /// Counts the calls, so a call ending late leaves the next one shown.
    serial: u64,
    clearing: Option<Task<()>>,
}

struct Doing {
    serial: u64,
    verb: &'static str,
    what: String,
    target: Option<Target>,
    clicked: bool,
    done: bool,
}

impl Browser {
    /// Shows that the agent began `request`; the number names the call.
    pub(super) fn begin(&mut self, request: &Browse, cx: &mut Context<Self>) -> u64 {
        let activity = &mut self.activity;
        activity.serial += 1;
        activity.clearing = None;
        let (verb, what) = describe(request);
        activity.doing = Some(Doing {
            serial: activity.serial,
            verb,
            what,
            target: None,
            clicked: false,
            done: false,
        });
        cx.notify();
        activity.serial
    }

    /// Marks the element the call is about to use.
    pub(super) fn aim(&mut self, serial: u64, target: Target, cx: &mut Context<Self>) {
        if let Some(doing) = self.doing_now(serial) {
            if !target.name.is_empty() {
                doing.what = format!("{} “{}”", doing.verb, short(&target.name));
            }
            doing.target = Some(target);
            cx.notify();
        }
    }

    pub(super) fn clicked(&mut self, serial: u64, cx: &mut Context<Self>) {
        if let Some(doing) = self.doing_now(serial) {
            doing.clicked = true;
            cx.notify();
        }
    }

    /// Leaves what was done on the page a moment, then takes it away.
    pub(super) fn end(&mut self, serial: u64, cx: &mut Context<Self>) {
        let Some(doing) = self.doing_now(serial) else {
            return;
        };
        doing.done = true;
        cx.notify();
        self.activity.clearing = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(LINGER).await;
            let _ = this.update(cx, |this, cx| {
                this.activity.doing = None;
                cx.notify();
            });
        }));
    }

    fn doing_now(&mut self, serial: u64) -> Option<&mut Doing> {
        self.activity
            .doing
            .as_mut()
            .filter(|doing| doing.serial == serial)
    }

    /// The element being used, and the ring where it was clicked, laid over
    /// the page in its own pixels.
    pub(super) fn render_marks(&self, cx: &Context<Self>) -> Vec<AnyElement> {
        let Some(doing) = &self.activity.doing else {
            return Vec::new();
        };
        let Some(target) = &doing.target else {
            return Vec::new();
        };
        let theme = cx.theme();
        let mut marks = vec![
            div()
                .id("browser-agent-target")
                .test_support()
                .absolute()
                .left(px(target.x - 2.))
                .top(px(target.y - 2.))
                .w(px(target.width + 4.))
                .h(px(target.height + 4.))
                .rounded(theme.radius)
                .border_2()
                .border_color(theme.primary)
                .bg(theme.primary.opacity(0.12))
                .into_any_element(),
        ];
        if doing.clicked {
            let (x, y) = target.middle();
            let color = theme.primary;
            let ring = div()
                .absolute()
                .rounded_full()
                .border_2()
                .border_color(color)
                .bg(color.opacity(0.25))
                .with_animation(
                    ("browser-agent-click", doing.serial),
                    Animation::new(RING_TIME).with_easing(ease_out_quint()),
                    move |ring, delta| {
                        let size = RING * (0.3 + 0.7 * delta);
                        ring.left(px(x - size / 2.))
                            .top(px(y - size / 2.))
                            .size(px(size))
                            .opacity(1. - delta * 0.8)
                    },
                );
            marks.push(ring.into_any_element());
        }
        marks
    }

    /// A line over the page's foot saying what the agent is doing.
    pub(super) fn render_activity(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let doing = self.activity.doing.as_ref()?;
        let theme = cx.theme();
        let line = h_flex()
            .id("browser-agent-activity")
            .test_support()
            .aria_label(SharedString::from(doing.what.clone()))
            .gap_2()
            .px_3()
            .py_1p5()
            .max_w_full()
            .rounded_full()
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .shadow_md()
            .text_sm()
            .when(!doing.done, |line| line.child(Spinner::new().small()))
            .child(div().flex_none().text_color(theme.primary).child("Claude"))
            .child(div().min_w_0().truncate().child(doing.what.clone()));
        let foot = div()
            .absolute()
            .bottom_3()
            .left_3()
            .right_3()
            .flex()
            .justify_center()
            .child(line);
        Some(foot.into_any_element())
    }
}

/// The verb to name the element with, and what the call does without it.
fn describe(request: &Browse) -> (&'static str, String) {
    let simple = |text: &str| ("", text.to_owned());
    match request {
        Browse::Navigate { url } => ("", format!("Opening {}", short(url))),
        Browse::Snapshot => simple("Reading the page"),
        Browse::Click { .. } => ("Clicking", "Clicking".to_owned()),
        Browse::Type { .. } => ("Typing in", "Typing".to_owned()),
        Browse::PressKey { key } => ("", format!("Pressing {key}")),
        Browse::Screenshot => simple("Taking a screenshot"),
        Browse::Console => simple("Reading the console"),
        Browse::Evaluate { .. } => simple("Running a script"),
        Browse::Questions => simple("Reading what the page asks"),
        Browse::Answer(_) => simple("Answering the page"),
    }
}

fn short(text: &str) -> String {
    let text = text.trim();
    match text.char_indices().nth(MAX_NAME) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_names_are_cut() {
        assert_eq!(short(" Save "), "Save");
        let long = "x".repeat(50);
        assert_eq!(short(&long).chars().count(), MAX_NAME + 1);
    }
}
