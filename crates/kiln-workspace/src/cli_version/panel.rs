//! The status bar button and the panel it opens: the version installed and
//! the newest, or that the CLI is missing, and what can be done about it.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, FontWeight, Hsla, SharedString, TestSupportExt, WeakEntity, Window,
    component::{
        ActiveTheme, Disableable as _, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        spinner::Spinner,
        v_flex,
    },
    div, px,
};
use kiln_agent::{Install, Method};
use kiln_chat::scale::{ScaledButton as _, ScaledText as _};

use super::{CliVersion, Shown, Update};

const PANEL_WIDTH: f32 = 300.;

pub(super) fn trigger(shown: &Shown, cx: &App) -> Button {
    let theme = cx.theme();
    let (dot, tooltip): (Hsla, SharedString) = match (&shown.install, &shown.latest) {
        _ if shown.missing => (
            theme.warning,
            format!("{} is not installed", shown.title).into(),
        ),
        (Some(_), Some(latest)) if shown.outdated() => (
            theme.warning,
            format!("{} {latest} is available", shown.title).into(),
        ),
        (Some(_), Some(_)) => (
            theme.success,
            format!("{} is up to date", shown.title).into(),
        ),
        _ => (theme.muted_foreground, shown.title.into()),
    };
    let label = shown.install.as_ref().map_or_else(
        || {
            let missing = if shown.missing { " not installed" } else { "" };
            format!("{}{missing}", shown.title)
        },
        |install| format!("{} {}", shown.title, install.version),
    );
    Button::new("cli-version")
        .ghost()
        .small()
        .px_1p5()
        .accessibility_label(label.clone())
        .text_color(theme.muted_foreground)
        .tooltip(tooltip)
        .child(
            h_flex()
                .gap_1p5()
                .child(div().size_2().rounded_full().bg(dot))
                .child(div().text_caption().child(label)),
        )
}

pub(super) fn panel(
    shown: &Shown,
    view: &WeakEntity<CliVersion>,
    cx: &App,
) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let installed = shown.install.as_ref();
    let fact = |name: &'static str, value: String| {
        h_flex()
            .gap_2()
            .child(div().w_24().text_color(theme.muted_foreground).child(name))
            .child(div().min_w_0().truncate().child(value))
    };
    v_flex()
        .id("cli-version-panel")
        .test_support()
        .w(px(PANEL_WIDTH))
        .gap_3()
        // The status bar's small type would carry over into the panel.
        .text_sm()
        .text_color(theme.popover_foreground)
        .child(div().font_weight(FontWeight::SEMIBOLD).child(shown.title))
        .child(
            v_flex()
                .gap_1()
                .children(installed.map(|it| fact("Installed", it.version.clone())))
                .when(!shown.missing, |facts| {
                    facts.child(fact(
                        "Latest",
                        shown.latest.clone().unwrap_or_else(|| "Unknown".into()),
                    ))
                })
                .children(installed.map(|it| fact("Installed with", method_name(&it.method)))),
        )
        .child(status(shown, view, cx))
        .children(restart_offer(shown, view, cx))
        .child(
            h_flex().justify_end().child(
                Button::new("cli-version-check")
                    .ghost()
                    .control()
                    .label("Check again")
                    .disabled(matches!(shown.update, Update::Running))
                    .on_click(listener(view, CliVersion::check_again)),
            ),
        )
}

/// Where the panel says what can be done, or what happened.
fn status(shown: &Shown, view: &WeakEntity<CliVersion>, cx: &App) -> gpui_kit::AnyElement {
    let theme = cx.theme();
    match &shown.update {
        Update::Running => running(shown),
        Update::Done if shown.missing => note(
            "Installed, but it is not on PATH yet. Restart Kiln.".into(),
            theme.warning,
        ),
        Update::Done => note("Done.".into(), theme.success),
        Update::Failed(error) => note(error.clone(), theme.danger),
        Update::Idle if shown.missing => install_offer(shown, view, cx),
        Update::Idle => idle(shown, view, cx),
    }
}

/// An installation or update under way, and the command it runs.
fn running(shown: &Shown) -> gpui_kit::AnyElement {
    let command = if shown.missing {
        shown.install_command.clone()
    } else {
        shown.install.as_ref().and_then(Install::update_command)
    };
    h_flex()
        .id("cli-version-status")
        .test_support()
        .gap_2()
        .text_sm()
        .child(Spinner::new().small())
        .child(format!("Running {}…", command.unwrap_or_default()))
        .into_any_element()
}

/// An installed CLI: up to date, or what updates it.
fn idle(shown: &Shown, view: &WeakEntity<CliVersion>, cx: &App) -> gpui_kit::AnyElement {
    let muted = cx.theme().muted_foreground;
    let command = shown.install.as_ref().and_then(Install::update_command);
    match command {
        Some(command) if shown.outdated() => v_flex()
            .gap_1()
            .child(
                Button::new("cli-version-update")
                    .primary()
                    .control()
                    .label(format!("Update {}", shown.title))
                    .on_click(listener(view, CliVersion::start_update)),
            )
            .child(
                div()
                    .text_caption()
                    .text_color(muted)
                    .child(format!("Runs {command}")),
            )
            .into_any_element(),
        None if shown.outdated() => note("Update it the way you installed it.".into(), muted),
        _ => note(
            shown
                .problem
                .clone()
                .unwrap_or_else(|| "Up to date.".into()),
            muted,
        ),
    }
}

fn note(text: String, color: Hsla) -> gpui_kit::AnyElement {
    div()
        .id("cli-version-status")
        .test_support()
        .text_sm()
        .text_color(color)
        .child(text)
        .into_any_element()
}

/// What a missing CLI's panel offers: installing it, as Kiln would.
fn install_offer(shown: &Shown, view: &WeakEntity<CliVersion>, cx: &App) -> gpui_kit::AnyElement {
    let theme = cx.theme();
    let caption = |text: String| {
        div()
            .text_caption()
            .text_color(theme.muted_foreground)
            .child(text)
    };
    let Some(command) = shown.install_command.clone() else {
        return caption(format!("Install {} from its website.", shown.title))
            .id("cli-version-status")
            .test_support()
            .into_any_element();
    };
    v_flex()
        .id("cli-version-status")
        .test_support()
        .gap_1()
        .child(caption(format!("{} is not installed.", shown.title)))
        .child(
            Button::new("cli-version-install")
                .primary()
                .control()
                .label(format!("Install {}", shown.title))
                .on_click(listener(view, CliVersion::start_install)),
        )
        .child(caption(format!("Runs {command}")))
        .into_any_element()
}

/// Open chats on an older version restart on the installed one, each
/// resuming its conversation; a chat in a turn restarts when it ends.
fn restart_offer(
    shown: &Shown,
    view: &WeakEntity<CliVersion>,
    cx: &App,
) -> Option<gpui_kit::AnyElement> {
    if shown.behind == 0 || matches!(shown.update, Update::Running) {
        return None;
    }
    let chats = match shown.behind {
        1 => "1 open chat runs".to_owned(),
        n => format!("{n} open chats run"),
    };
    let element = v_flex()
        .gap_1()
        .child(
            div()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{chats} an older version.")),
        )
        .child(
            Button::new("cli-version-restart")
                .outline()
                .control()
                .label("Restart Chats on This Version")
                .tooltip("Each chat resumes its conversation; a running turn finishes first")
                .on_click(listener(view, CliVersion::restart_chats)),
        )
        .into_any_element();
    Some(element)
}

fn listener(
    view: &WeakEntity<CliVersion>,
    act: fn(&mut CliVersion, &mut Context<CliVersion>),
) -> impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static {
    let view = view.clone();
    move |_, _, cx| {
        let _ = view.update(cx, act);
    }
}

fn method_name(method: &Method) -> String {
    match method {
        Method::Homebrew { cask, .. } => format!("Homebrew ({cask})"),
        Method::Native => "Native installer".to_owned(),
        Method::Npm => "npm".to_owned(),
        Method::Unknown => "Unknown".to_owned(),
    }
}
