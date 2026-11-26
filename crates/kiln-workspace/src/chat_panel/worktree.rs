//! A chat working in a worktree Kiln made for it: a strip over the chat
//! names its branch and the one it started from, and ends it by merging the
//! work back or throwing it away. Either way the tab closes with it.

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, Context, FontWeight, SharedString, Task, TestSupportExt, Window,
    assets::IconName as Lucide,
    component::{
        ActiveTheme, Disableable as _, Icon, WindowExt as _,
        button::{Button, ButtonVariant, ButtonVariants as _},
        h_flex,
        notification::Notification,
    },
    div,
};
use kiln_chat::scale::{ScaledButton as _, ScaledIcon as _, ScaledText as _};
use kiln_git::Worktree;

use super::{ChatPanel, ChatPanelEvent};

/// The worktree the chat's folder lies in, once read, and an end running.
#[derive(Default)]
pub(super) struct WorktreeState {
    worktree: Option<Worktree>,
    read: Option<Task<()>>,
    ending: Option<Task<()>>,
}

#[derive(Clone, Copy)]
enum End {
    Merge,
    Discard,
}

impl ChatPanel {
    /// Reads whether the chat's folder is a Kiln worktree.
    pub(super) fn read_worktree(&mut self, cx: &mut Context<Self>) {
        let folder = self.project.read(cx).root().to_path_buf();
        let read = cx.background_spawn(async move { kiln_git::worktree_of(&folder) });
        self.worktree.read = Some(cx.spawn(async move |this, cx| {
            let worktree = read.await.ok().flatten();
            let _ = this.update(cx, |this, cx| {
                this.worktree.worktree = worktree;
                cx.notify();
            });
        }));
    }

    fn ask_discard(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(worktree) = &self.worktree.worktree else {
            return;
        };
        let view = cx.entity().downgrade();
        let title = format!("Discard {}?", worktree.branch);
        window.open_alert_dialog(cx, move |dialog, _, _| {
            let view = view.clone();
            dialog
                .confirm()
                .title(title.clone())
                .description(
                    "Its changes and commits are deleted, and the worktree's folder with them.",
                )
                .ok_text("Discard")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    let _ = view.update(cx, |this, cx| this.end(End::Discard, window, cx));
                    true
                })
        });
    }

    fn end(&mut self, end: End, window: &mut Window, cx: &mut Context<Self>) {
        let Some(worktree) = self.worktree.worktree.clone() else {
            return;
        };
        if self.worktree.ending.is_some() {
            return;
        }
        let done = matches!(end, End::Merge)
            .then(|| format!("Merged {} into {}.", worktree.branch, worktree.base));
        let ending = cx.background_spawn(async move {
            match end {
                End::Merge => kiln_git::merge_worktree(&worktree),
                End::Discard => kiln_git::discard_worktree(&worktree),
            }
        });
        self.worktree.ending = Some(cx.spawn_in(window, async move |this, cx| {
            let result = ending.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.ended(result.map(|()| done), window, cx);
            });
        }));
        cx.notify();
    }

    /// Closes the tab once the worktree is gone; says why when it is not.
    fn ended(
        &mut self,
        result: anyhow::Result<Option<String>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.worktree.ending = None;
        match result {
            Ok(said) => {
                if let Some(said) = said {
                    window.push_notification(Notification::success(said), cx);
                }
                cx.emit(ChatPanelEvent::Close);
            }
            Err(error) => window.push_notification(Notification::error(format!("{error:#}")), cx),
        }
        cx.notify();
    }

    /// `None` unless the chat works in a Kiln worktree.
    pub(super) fn render_worktree(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let worktree = self.worktree.worktree.as_ref()?;
        let theme = cx.theme();
        let ending = self.worktree.ending.is_some();
        let merge: SharedString = format!("Merge into {}", worktree.base).into();
        let strip = h_flex()
            .id("chat-worktree")
            .test_support()
            .flex_none()
            .gap_2()
            .px_3()
            .py_1()
            .border_b_1()
            .border_color(theme.border)
            .text_caption()
            .text_color(theme.muted_foreground)
            .child(Icon::new(Lucide::GitBranch).in_row())
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.foreground)
                    .child(worktree.branch.clone()),
            )
            .child(format!("from {}", worktree.base))
            .child(div().flex_1())
            .child(
                Button::new("chat-worktree-discard")
                    .ghost()
                    .control()
                    .label("Discard")
                    .disabled(ending)
                    .on_click(cx.listener(|this, _, window, cx| this.ask_discard(window, cx))),
            )
            .child(
                Button::new("chat-worktree-merge")
                    .control()
                    .label(merge)
                    .loading(ending)
                    .on_click(cx.listener(|this, _, window, cx| this.end(End::Merge, window, cx))),
            );
        Some(strip.into_any_element())
    }
}
