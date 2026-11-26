//! The strip over a chat whose turns changed files not committed yet: how
//! many, whether Kiln commits them once the work is done, and the button
//! that commits and pushes them now, for when the user has checked them.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, SharedString, TestSupportExt, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Disableable as _, Icon,
        button::{Button, ButtonVariants as _},
        h_flex,
    },
    div,
};
use kiln_chat::scale::{ScaledButton as _, ScaledIcon as _, ScaledText as _};
use kiln_settings::{CommitTrigger, PushPolicy, Scope};

use super::ChatPanel;
use crate::commit_settings::{self, Opened};

impl ChatPanel {
    /// The commit settings, on this chat's tab.
    pub(super) fn open_commit_settings(&self, window: &mut Window, cx: &mut Context<Self>) {
        let opened = self.commit_settings(Scope::Chat, &cx.entity(), cx);
        commit_settings::open(opened, window, cx);
    }

    /// The commit settings of `this` chat and its project, on `scope`'s tab.
    pub fn commit_settings(&self, scope: Scope, this: &Entity<Self>, cx: &App) -> Opened {
        let chat = self.chat.read(cx);
        let mut models: Vec<String> = chat.models().iter().map(|model| model.id.clone()).collect();
        models.dedup();
        Opened {
            scope,
            project: Some(self.project.read(cx).root().to_path_buf()),
            chat: Some(this.downgrade()),
            own: self.session_settings().clone(),
            agents: vec![chat.agent_name().to_owned()],
            models,
        }
    }

    /// `None` while the chat changed nothing that waits for a commit.
    pub(super) fn render_work(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let touched = self.touched();
        let settings = self.settings(cx);
        if !settings.git.commit.enabled || touched == 0 && !self.is_writing() {
            return None;
        }
        let theme = cx.theme();
        let files: SharedString = match touched {
            1 => "1 file changed".into(),
            count => format!("{count} files changed").into(),
        };
        let auto = match settings.git.commit.trigger {
            CommitTrigger::TaskDone => "Commits when the work is done",
            CommitTrigger::Agent => "Commits when asked",
        };
        let label = if settings.git.push.policy == PushPolicy::Never {
            "Commit"
        } else {
            "Commit & push"
        };
        let strip = h_flex()
            .id("chat-work")
            .test_support()
            .flex_none()
            .gap_2()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(theme.border)
            .text_caption()
            .text_color(theme.muted_foreground)
            .child(Icon::new(Lucide::GitCommitHorizontal).in_row())
            .child(div().text_color(theme.foreground).child(files))
            .child(div().min_w_0().truncate().child(auto))
            .child(div().flex_1())
            .child(
                Button::new("chat-work-settings")
                    .ghost()
                    .control_icon(Lucide::Settings)
                    .tooltip("Commit settings")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_commit_settings(window, cx);
                    })),
            )
            .child(
                Button::new("chat-work-commit")
                    .control()
                    .label(label)
                    .tooltip("Commit what this chat changed, with a message Kiln writes")
                    .loading(self.is_writing())
                    .disabled(self.review.is_some())
                    .on_click(cx.listener(|this, _, window, cx| this.commit_asked(window, cx))),
            );
        Some(strip.into_any_element())
    }
}
