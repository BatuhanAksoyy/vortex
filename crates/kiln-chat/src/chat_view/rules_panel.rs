//! The permission rules button in the composer strip and the panel it
//! opens: each rule under what it does, where it is kept, and a button to
//! remove the ones kept in a settings file. Drawn like the skills panel.

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, App, Context, Hsla, SharedString, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme, Disableable, Icon, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        popover::Popover,
        v_flex,
    },
    div, px,
};
use kiln_agent::{PermissionRule, RuleBehavior, RuleSource};

use super::{ChatView, StripPanel};

const PANEL_WIDTH: f32 = 360.;
const LIST_MAX_HEIGHT: f32 = 360.;

const EMPTY: &str = "No permission rules yet. Choose \"don't ask again\" on a permission prompt to \
                     add one.";

/// What the panel shows, copied out of the view each render.
struct Shown {
    groups: Vec<(RuleBehavior, Vec<PermissionRule>)>,
    removing: bool,
    error: Option<String>,
    loaded: bool,
}

impl ChatView {
    pub(super) fn rules_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let view = cx.entity().downgrade();
        let list = &self.rule_list;
        let shown = Shown {
            groups: list
                .by_behavior()
                .into_iter()
                .map(|(behavior, rules)| (behavior, rules.into_iter().cloned().collect()))
                .collect(),
            removing: list.is_removing(),
            error: list.error().map(str::to_owned),
            loaded: list.is_loaded(),
        };
        let trigger = Button::new("chat-rules")
            .ghost()
            .xsmall()
            .icon(Icon::new(gpui_kit::assets::IconName::ShieldCheck))
            .tooltip("Permission rules");
        let opener = view.clone();
        Popover::new("chat-rules-popover")
            .anchor(Anchor::BottomLeft)
            .trigger(trigger)
            .open(self.is_open(StripPanel::Rules))
            .on_open_change(move |open, window, cx| {
                let _ = opener.update(cx, |view, cx| view.set_rules_open(*open, window, cx));
            })
            .content(move |_, _, cx| panel(&shown, &view, cx))
    }
}

fn panel(shown: &Shown, view: &WeakEntity<ChatView>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let count: usize = shown.groups.iter().map(|(_, rules)| rules.len()).sum();
    let muted = theme.muted_foreground;
    let groups = shown
        .groups
        .iter()
        .map(|(behavior, rules)| group(*behavior, rules, shown.removing, view, cx));
    v_flex()
        .id("chat-rules-panel")
        .test_support()
        .w(px(PANEL_WIDTH))
        .gap_1()
        .text_sm()
        .child(
            h_flex()
                .justify_between()
                .child("Permission rules")
                .when(count > 0, |this| {
                    this.child(div().text_xs().text_color(muted).child(count.to_string()))
                }),
        )
        .children(
            shown
                .error
                .as_deref()
                .map(|text| message("chat-rules-error", text, theme.danger)),
        )
        .when(!shown.loaded, |this| {
            this.child(message("chat-rules-loading", "Loading rules…", muted))
        })
        .when(shown.loaded && count == 0, |this| {
            this.child(message("chat-rules-empty", EMPTY, muted))
        })
        .child(
            v_flex()
                .id("chat-rules-list")
                .max_h(px(LIST_MAX_HEIGHT))
                .overflow_y_scroll()
                .gap_1()
                .children(groups),
        )
}

/// A status line, found in tests by its text.
fn message(id: &'static str, text: &str, color: Hsla) -> impl IntoElement + use<> {
    div()
        .id(id)
        .test_support()
        .aria_label(SharedString::from(text.to_owned()))
        .text_xs()
        .text_color(color)
        .child(text.to_owned())
}

fn group(
    behavior: RuleBehavior,
    rules: &[PermissionRule],
    removing: bool,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .id(SharedString::from(format!("rules-group-{behavior:?}")))
                .test_support()
                .aria_label(behavior_label(behavior))
                .px_1()
                .pt_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(behavior_label(behavior)),
        )
        .children(rules.iter().map(|rule| rule_row(rule, removing, view, cx)))
}

/// The rule as written over its reading and where it is kept.
fn rule_row(
    rule: &PermissionRule,
    removing: bool,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let key = rule_key(rule);
    let detail = match &rule.description {
        Some(reading) => format!("{reading} · {}", source_label(rule.source)),
        None => source_label(rule.source).to_owned(),
    };
    h_flex()
        .id(SharedString::from(format!("rule-{key}")))
        .test_support()
        .gap_2()
        .px_1()
        .py_0p5()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .truncate()
                        .font_family(theme.mono_font_family.clone())
                        .child(rule.rule.clone()),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("rule-detail-{key}")))
                        .test_support()
                        .aria_label(SharedString::from(detail.clone()))
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(detail),
                ),
        )
        .when(rule.source.is_removable(), |this| {
            this.child(remove_button(rule, &key, removing, view))
        })
}

fn remove_button(
    rule: &PermissionRule,
    key: &str,
    removing: bool,
    view: &WeakEntity<ChatView>,
) -> impl IntoElement + use<> {
    let (rule, view) = (rule.clone(), view.clone());
    let tooltip = if rule.source == RuleSource::SharedProject {
        "Remove from the project settings shared with the repository"
    } else {
        "Remove rule"
    };
    Button::new(SharedString::from(format!("rule-remove-{key}")))
        .ghost()
        .xsmall()
        .flex_none()
        .icon(Icon::new(gpui_kit::assets::IconName::Trash))
        .tooltip(tooltip)
        .disabled(removing)
        .on_click(move |_, _, cx| {
            let _ = view.update(cx, |view, cx| view.remove_rule(&rule, cx));
        })
}

/// Names a row by what makes the rule unique: the same rule may be kept in
/// several places, or both allowed and denied.
fn rule_key(rule: &PermissionRule) -> String {
    format!("{:?}-{:?}-{}", rule.behavior, rule.source, rule.rule).to_lowercase()
}

fn behavior_label(behavior: RuleBehavior) -> &'static str {
    match behavior {
        RuleBehavior::Allow => "Allowed",
        RuleBehavior::Ask => "Always asks",
        RuleBehavior::Deny => "Denied",
    }
}

fn source_label(source: RuleSource) -> &'static str {
    match source {
        RuleSource::Session => "This session",
        RuleSource::Project => "This project",
        RuleSource::SharedProject => "Shared project settings",
        RuleSource::User => "All projects",
        RuleSource::Fixed => "Managed",
    }
}
