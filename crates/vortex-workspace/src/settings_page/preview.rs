//! A small sample of the window at the top of the Appearance and Editor
//! pages: a message, the agent's reply and a few lines of code, drawn with
//! the theme and fonts as they are set now. The settings sit over the work
//! in a dialog, so a change shows here at once.

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Hsla, SharedString, TestSupportExt as _,
    component::{ActiveTheme as _, h_flex, setting::SettingItem, v_flex},
    div, px,
};

/// The lines of code, each as parts with whether the part is a keyword.
const CODE: [&[(&str, Part)]; 3] = [
    &[
        ("fn ", Part::Keyword),
        ("build", Part::Name),
        ("() {", Part::Plain),
    ],
    &[
        ("    let ", Part::Keyword),
        ("ok = ", Part::Plain),
        ("true", Part::Keyword),
        (";", Part::Plain),
    ],
    &[("}", Part::Plain)],
];

#[derive(Clone, Copy)]
enum Part {
    Plain,
    Keyword,
    Name,
}

pub(super) fn item(id: &'static str) -> SettingItem {
    SettingItem::render(move |_, _, cx| sample(id, cx))
}

fn sample(id: &'static str, cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    v_flex()
        .id(id)
        .test_support()
        .w_full()
        .overflow_hidden()
        .rounded(theme.radius_lg)
        .border_1()
        .border_color(theme.border)
        .bg(theme.background)
        .child(
            h_flex()
                .px_3()
                .py_1()
                .gap_2()
                .border_b_1()
                .border_color(theme.border)
                .bg(theme.title_bar)
                .text_xs()
                .text_color(theme.muted_foreground)
                .child("Preview"),
        )
        .child(
            v_flex()
                .p_3()
                .gap_2()
                .child(
                    h_flex().justify_end().child(
                        div()
                            .px_3()
                            .py_1()
                            .rounded(theme.radius_lg)
                            .bg(theme.secondary)
                            .child("Fix the build"),
                    ),
                )
                .child(div().child("The build fails in main.rs; the fix is below."))
                .child(code(cx)),
        )
}

fn code(cx: &App) -> impl IntoElement + use<> {
    let theme = cx.theme();
    let color = |part: Part| -> Hsla {
        match part {
            Part::Plain => theme.foreground,
            Part::Keyword => theme.primary,
            Part::Name => theme.info,
        }
    };
    let lines = CODE.iter().map(|parts| {
        h_flex().children(parts.iter().map(|(text, part)| {
            div()
                .whitespace_nowrap()
                .text_color(color(*part))
                .child(SharedString::from(*text))
        }))
    });
    v_flex()
        .px_3()
        .py_2()
        .rounded(theme.radius)
        .border_1()
        .border_color(theme.border)
        .bg(theme.secondary.opacity(0.5))
        .font_family(theme.mono_font_family.clone())
        .text_size(theme.mono_font_size)
        .line_height(theme.mono_font_size * 1.5)
        .min_h(px(0.))
        .children(lines)
}
