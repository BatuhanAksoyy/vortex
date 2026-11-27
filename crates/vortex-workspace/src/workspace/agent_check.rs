//! Under the folders of the start page, which harnesses' CLIs Vortex runs
//! and from where, so a missing one is seen before a chat needs it, with a
//! way to install it or to say where it is; then what the activity bar's
//! gear offers, which is not shown without a project.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, TestSupportExt as _,
    assets::IconName as Lucide,
    component::{
        ActiveTheme as _, Disableable as _, Icon, Sizable as _,
        button::{Button, ButtonVariants},
        h_flex, v_flex,
    },
    div, rems,
};
use vortex_chat::{AgentSetup, Setup};

use super::Workspace;

/// As wide as the folders above.
const WIDTH_REMS: f32 = 36.0;

impl Workspace {
    pub(super) fn render_agent_check(cx: &Context<Self>) -> AnyElement {
        let programs = crate::programs::programs(cx);
        let rows = [
            ("claude", "Claude Code", programs.claude.path()),
            ("codex", "Codex", programs.codex.path()),
        ];
        v_flex()
            .id("start-agents")
            .test_support()
            .w(rems(WIDTH_REMS))
            .max_w_full()
            .mt_4()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Coding agents"),
            )
            .children(
                rows.into_iter()
                    .map(|(agent, name, path)| agent_row(agent, name, &path, cx)),
            )
            .child(Self::render_start_tools(cx))
            .into_any_element()
    }

    /// Settings, shortcuts and updates, as the gear's menu has them.
    fn render_start_tools(cx: &Context<Self>) -> impl IntoElement + use<> {
        let updates = div()
            .relative()
            .child(
                Button::new("start-updates")
                    .ghost()
                    .small()
                    .icon(Lucide::CircleArrowUp)
                    .label(Self::update_label(cx))
                    .when(Self::update_waiting(cx), ButtonVariants::primary)
                    .on_click(
                        cx.listener(|this, _, window, cx| this.check_for_updates(window, cx)),
                    ),
            )
            .children(Self::render_update_panel(gpui_kit::Anchor::TopLeft, cx));
        h_flex()
            .mt_3()
            .gap_1()
            .child(
                Button::new("start-settings")
                    .ghost()
                    .small()
                    .icon(Lucide::Settings)
                    .label("Settings")
                    .on_click(
                        cx.listener(|this, _, window, cx| this.open_settings_page(window, cx)),
                    ),
            )
            .child(
                Button::new("start-shortcuts")
                    .ghost()
                    .small()
                    .icon(Lucide::Keyboard)
                    .label("Keyboard Shortcuts")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_shortcuts_page(window, cx);
                    })),
            )
            .child(updates)
    }
}

fn agent_row(
    agent: &'static str,
    name: &'static str,
    path: &std::path::Path,
    cx: &Context<Workspace>,
) -> impl IntoElement + use<> {
    let setup = AgentSetup::of(agent, cx);
    let theme = cx.theme();
    let (icon, color, detail) = match &setup {
        None if path.is_absolute() => (
            Lucide::CircleCheck,
            theme.success,
            path.display().to_string(),
        ),
        // Not checked yet: the status bar's check has not come back.
        None => (
            Lucide::Circle,
            theme.muted_foreground,
            "Looking…".to_owned(),
        ),
        Some(Setup::Installing) => (
            Lucide::Download,
            theme.muted_foreground,
            "Installing…".to_owned(),
        ),
        Some(Setup::Missing) => (
            Lucide::CircleAlert,
            theme.warning,
            "Not installed".to_owned(),
        ),
        Some(Setup::Failed(why)) => (Lucide::CircleAlert, theme.danger, why.clone()),
    };
    h_flex()
        .id(format!("start-agent-{agent}"))
        .test_support()
        .gap_2()
        .px_2()
        .py_1()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .child(Icon::new(icon).small().text_color(color))
        .child(
            v_flex().flex_1().min_w_0().text_sm().child(name).child(
                div()
                    .truncate()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(detail),
            ),
        )
        .children(
            setup
                .as_ref()
                .map(|setup| actions(agent, *setup == Setup::Installing, cx)),
        )
}

fn actions(
    agent: &'static str,
    installing: bool,
    cx: &Context<Workspace>,
) -> impl IntoElement + use<> {
    h_flex()
        .flex_none()
        .gap_1()
        .child(
            Button::new(format!("start-install-{agent}"))
                .primary()
                .small()
                .label("Install")
                .disabled(installing)
                .on_click(cx.listener(move |this, _, _, cx| this.install_cli(agent, cx))),
        )
        .child(
            Button::new(format!("start-choose-{agent}"))
                .ghost()
                .small()
                .label("Choose Path…")
                .on_click(cx.listener(move |_, _, window, cx| {
                    Workspace::choose_cli_path(agent, window, cx);
                })),
        )
}
