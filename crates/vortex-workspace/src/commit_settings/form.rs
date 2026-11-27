//! The page's form: a tab for each scope, then one row per setting, each
//! with the scope its value comes from and, where the tab sets it, a way
//! back to the value below.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FontWeight, SharedString, TestSupportExt, Window,
    component::{
        ActiveTheme, Sizable as _,
        button::{Button, ButtonVariants as _},
        h_flex,
        input::{Input, Textarea},
        select::Select,
        switch::Switch,
        tab::{Tab, TabBar},
        v_flex,
    },
    div,
};
use serde_json::Value;
use vortex_chat::scale::ScaledText as _;
use vortex_settings::Scope;

use super::{
    CONVENTIONAL, CommitSettingsPage, DIRECTIVES, ENABLED, FORBID, Field, MODEL, PUSH, REQUIRE,
    REVIEW, SUBJECT_MAX, TRIGGER,
};

fn scope_title(scope: Scope) -> &'static str {
    match scope {
        Scope::Chat => "This chat",
        Scope::Project => "This project",
        Scope::Global => "All projects",
    }
}

fn scope_note(scope: Scope) -> &'static str {
    match scope {
        Scope::Chat => "Kept with this chat, over the project's settings.",
        Scope::Project => "Saved in the project's .vortex/settings.json, over yours.",
        Scope::Global => "Saved in ~/.config/vortex/settings.json, for every project.",
    }
}

impl CommitSettingsPage {
    /// A setting: its label, where its value comes from, and its control.
    fn row(
        &self,
        field: &'static Field,
        control: impl IntoElement,
        cx: &Context<Self>,
    ) -> AnyElement {
        let theme = cx.theme();
        let source = self.source(field);
        let here = source == Some(self.scope);
        let from: SharedString = match source {
            Some(_) if here => "Set here".into(),
            Some(scope) => format!("From {}", scope_title(scope).to_lowercase()).into(),
            None => "Default".into(),
        };
        let reset = here.then(|| {
            Button::new(SharedString::from(format!(
                "commit-settings-reset-{}",
                field.name
            )))
            .ghost()
            .xsmall()
            .label("Reset")
            .tooltip("Use the value from below again")
            .on_click(cx.listener(move |this, _, window, cx| this.save(field, None, window, cx)))
        });
        v_flex()
            .gap_1()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::MEDIUM)
                            .child(field.label),
                    )
                    .child(
                        div()
                            .text_caption()
                            .text_color(theme.muted_foreground)
                            .child(from),
                    )
                    .children(reset),
            )
            .child(control)
            .into_any_element()
    }

    fn switch(&self, field: &'static Field, cx: &Context<Self>) -> Switch {
        Switch::new(SharedString::from(format!(
            "commit-settings-{}",
            field.name
        )))
        .checked(self.flag(field))
        .on_click(cx.listener(move |this, checked: &bool, window, cx| {
            this.save(field, Some(Value::Bool(*checked)), window, cx);
        }))
    }

    fn render_scopes(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let scopes = self.scopes();
        let picked = scopes.iter().position(|scope| *scope == self.scope);
        let tabs: Vec<Tab> = (scopes.iter())
            .map(|scope| Tab::new().label(scope_title(*scope)))
            .collect();
        TabBar::new("commit-settings-scopes")
            .segmented()
            .small()
            .when_some(picked, TabBar::selected_index)
            .on_click(cx.listener(move |this, at: &usize, window, cx| {
                if let Some(scope) = scopes.get(*at) {
                    this.pick_scope(*scope, window, cx);
                }
            }))
            .children(tabs)
    }

    fn render_committing(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let writer = h_flex()
            .gap_2()
            .child(
                div()
                    .w_1_3()
                    .child(Select::new(&self.controls.agent).id("commit-settings-agent")),
            )
            .child(
                div()
                    .flex_1()
                    .child(Select::new(&self.controls.model).id("commit-settings-model")),
            );
        section("Committing", cx)
            .child(self.row(&ENABLED, self.switch(&ENABLED, cx), cx))
            .child(self.row(
                &TRIGGER,
                Select::new(&self.controls.trigger).id("commit-settings-trigger"),
                cx,
            ))
            .child(self.row(&REVIEW, self.switch(&REVIEW, cx), cx))
            .child(self.row(
                &PUSH,
                Select::new(&self.controls.push).id("commit-settings-push"),
                cx,
            ))
            .child(self.row(&MODEL, writer, cx))
    }

    fn render_message(&self, cx: &Context<Self>) -> impl IntoElement + use<> {
        let lines = |input| Textarea::new(input);
        section("Message", cx)
            .child(self.row(
                &DIRECTIVES,
                lines(&self.controls.directives).aria_label("Directives"),
                cx,
            ))
            .child(self.row(
                &REQUIRE,
                lines(&self.controls.require).aria_label("Must match"),
                cx,
            ))
            .child(self.row(
                &FORBID,
                lines(&self.controls.forbid).aria_label("Must not match"),
                cx,
            ))
            .child(self.row(&SUBJECT_MAX, Input::new(&self.controls.subject_max), cx))
            .child(self.row(&CONVENTIONAL, self.switch(&CONVENTIONAL, cx), cx))
    }
}

fn section(title: &'static str, cx: &Context<CommitSettingsPage>) -> gpui_kit::Div {
    v_flex().gap_3().child(
        div()
            .text_caption()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(cx.theme().muted_foreground)
            .child(title),
    )
}

impl Render for CommitSettingsPage {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let error = self.error.clone().map(|error| {
            div()
                .id("commit-settings-error")
                .test_support()
                .text_caption()
                .text_color(theme.danger)
                .child(error)
        });
        v_flex()
            .id("commit-settings")
            .test_support()
            .gap_5()
            .child(
                v_flex().gap_2().child(self.render_scopes(cx)).child(
                    div()
                        .text_caption()
                        .text_color(theme.muted_foreground)
                        .child(scope_note(self.scope)),
                ),
            )
            .children(error)
            .child(self.render_committing(cx))
            .child(self.render_message(cx))
    }
}
