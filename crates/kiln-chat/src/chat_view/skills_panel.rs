//! The skills button in the composer strip and the panel it opens: each
//! skill under its source, with its cost and a menu for how it is offered.
//! Drawn like the context panel; the state menu is the mode menu's kind.

use gpui_kit::prelude::*;
use gpui_kit::{
    Anchor, App, Context, Hsla, SharedString, TestSupportExt, WeakEntity,
    component::{
        ActiveTheme, Disableable, Icon, Sizable,
        button::{Button, ButtonVariants},
        h_flex,
        menu::{DropdownMenu, PopupMenuItem},
        popover::Popover,
        tooltip::Tooltip,
        v_flex,
    },
    div, px,
};
use kiln_agent::{Skill, SkillLock, SkillState};

use super::{ChatView, StripPanel, composer::option_row};
use crate::agent_state::tokens;

const PANEL_WIDTH: f32 = 320.;
const LIST_MAX_HEIGHT: f32 = 360.;
const STATE_MENU_WIDTH: f32 = 260.;

/// What the panel shows, copied out of the view each render.
struct Shown {
    groups: Vec<(String, Vec<Skill>)>,
    saving: Option<String>,
    error: Option<String>,
    notice: Option<String>,
    loaded: bool,
}

impl ChatView {
    pub(super) fn skills_button(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let view = cx.entity().downgrade();
        let menu = &self.skill_menu;
        let shown = Shown {
            groups: menu
                .by_source()
                .into_iter()
                .map(|(source, skills)| (source.to_owned(), skills.into_iter().cloned().collect()))
                .collect(),
            saving: menu.saving().map(str::to_owned),
            error: menu.error().map(str::to_owned),
            notice: menu.notice().map(str::to_owned),
            loaded: menu.is_loaded(),
        };
        let trigger = Button::new("chat-skills")
            .ghost()
            .xsmall()
            .icon(Icon::new(gpui_kit::assets::IconName::Puzzle))
            .tooltip("Skills");
        let opener = view.clone();
        Popover::new("chat-skills-popover")
            .anchor(Anchor::BottomLeft)
            .trigger(trigger)
            .open(self.is_open(StripPanel::Skills))
            .on_open_change(move |open, window, cx| {
                let _ = opener.update(cx, |view, cx| view.set_skills_open(*open, window, cx));
            })
            .content(move |_, _, cx| panel(&shown, &view, cx))
    }
}

fn panel(shown: &Shown, view: &WeakEntity<ChatView>, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let count: usize = shown.groups.iter().map(|(_, skills)| skills.len()).sum();
    let muted = theme.muted_foreground;
    let groups = shown
        .groups
        .iter()
        .map(|(source, skills)| source_group(source, skills, shown.saving.as_deref(), view, cx));
    v_flex()
        .id("chat-skills-panel")
        .test_support()
        .w(px(PANEL_WIDTH))
        .gap_1()
        .text_sm()
        .child(
            h_flex()
                .justify_between()
                .child("Skills")
                .when(count > 0, |this| {
                    this.child(div().text_xs().text_color(muted).child(count.to_string()))
                }),
        )
        .children(
            shown
                .error
                .as_deref()
                .map(|text| message("chat-skills-error", text, theme.danger)),
        )
        .children(
            shown
                .notice
                .as_deref()
                .map(|text| message("chat-skills-notice", text, muted)),
        )
        .when(!shown.loaded, |this| {
            this.child(message("chat-skills-loading", "Loading skills…", muted))
        })
        .when(shown.loaded && count == 0, |this| {
            this.child(message("chat-skills-empty", "No skills found.", muted))
        })
        .child(
            v_flex()
                .id("chat-skills-list")
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

fn source_group(
    source: &str,
    skills: &[Skill],
    saving: Option<&str>,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    v_flex()
        .gap_0p5()
        .child(
            div()
                .id(SharedString::from(format!("skills-source-{source}")))
                .test_support()
                .aria_label(SharedString::from(source_label(source)))
                .px_1()
                .pt_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(source_label(source)),
        )
        .children(
            skills
                .iter()
                .map(|skill| skill_row(skill, saving, view, cx)),
        )
}

/// The skill's name over its cost and description, and its state.
fn skill_row(
    skill: &Skill,
    saving: Option<&str>,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let off = skill.state == SkillState::Off;
    let detail = if skill.description.is_empty() {
        format!("{} tokens", tokens(skill.tokens))
    } else {
        format!("{} tokens · {}", tokens(skill.tokens), skill.description)
    };
    h_flex()
        .id(SharedString::from(format!("skill-{}", skill.name)))
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
                        .when(off, |this| this.text_color(theme.muted_foreground))
                        .child(skill.display_name.clone()),
                )
                .child(
                    div()
                        .truncate()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(detail),
                ),
        )
        .child(state_control(skill, saving, view, cx))
}

fn state_control(
    skill: &Skill,
    saving: Option<&str>,
    view: &WeakEntity<ChatView>,
    cx: &App,
) -> gpui_kit::AnyElement {
    let muted = cx.theme().muted_foreground;
    if let Some(lock) = skill.locked_by {
        return h_flex()
            .id(SharedString::from(format!("skill-lock-{}", skill.name)))
            .test_support()
            .flex_none()
            .gap_1()
            .px_1()
            .text_xs()
            .text_color(muted)
            .child(Icon::new(gpui_kit::assets::IconName::Lock).xsmall())
            .child(state_label(skill.state))
            .tooltip(move |window, cx| Tooltip::new(lock_reason(lock)).build(window, cx))
            .into_any_element();
    }
    let this_saving = saving == Some(skill.name.as_str());
    let (name, current, view) = (skill.name.clone(), skill.state, view.clone());
    Button::new(SharedString::from(format!("skill-state-{}", skill.name)))
        .ghost()
        .xsmall()
        .flex_none()
        .label(if this_saving {
            "Saving…"
        } else {
            state_label(skill.state)
        })
        .when(skill.state == SkillState::Off, |this| {
            this.text_color(muted)
        })
        .disabled(saving.is_some())
        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _, _| {
            SkillState::ALL
                .into_iter()
                .fold(menu.min_w(px(STATE_MENU_WIDTH)), |menu, state| {
                    menu.item(state_item(&name, state, state == current, &view))
                })
        })
        .into_any_element()
}

fn state_item(
    name: &str,
    state: SkillState,
    checked: bool,
    view: &WeakEntity<ChatView>,
) -> PopupMenuItem {
    let (view, name) = (view.clone(), name.to_owned());
    let id = SharedString::from(format!("skill-state-{name}-{state:?}"));
    PopupMenuItem::element(move |_, cx| {
        option_row(
            id.clone(),
            state_label(state).into(),
            state_meaning(state).into(),
            cx.theme().muted_foreground,
        )
    })
    .checked(checked)
    .on_click(move |_, _, cx| {
        let _ = view.update(cx, |view, cx| view.set_skill_state(&name, state, cx));
    })
}

/// `project` reads `Project`.
fn source_label(source: &str) -> String {
    let mut chars = source.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

fn state_label(state: SkillState) -> &'static str {
    match state {
        SkillState::On => "On",
        SkillState::NameOnly => "Name only",
        SkillState::UserOnly => "User only",
        SkillState::Off => "Off",
    }
}

fn state_meaning(state: SkillState) -> &'static str {
    match state {
        SkillState::On => "Claude sees it, and you can run it",
        SkillState::NameOnly => "Claude sees its name only, not its description",
        SkillState::UserOnly => "You can run it; Claude does not see it",
        SkillState::Off => "Hidden from Claude and from the command list",
    }
}

fn lock_reason(lock: SkillLock) -> &'static str {
    match lock {
        SkillLock::Plugin => "Managed with its plugin",
        SkillLock::Author => "Set in the skill's own file",
        SkillLock::Policy => "Set by a higher-priority configuration",
        SkillLock::ReservedName => {
            "Settings can't store an entry with this name; rename the skill's folder or file to \
             configure it"
        }
    }
}
