//! A chat as a tab in the dock. Each one runs its own agent process, which
//! keeps working while another tab is shown.

mod commit;
mod pickers;
mod settings;
mod turn_commit;
mod work_strip;
mod worktree;

use std::path::PathBuf;

use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EventEmitter, FocusHandle, Focusable, SharedString, Subscription,
    TestSupportExt as _, Window,
    component::{
        ActiveTheme, Icon,
        button::{Button, ButtonVariants as _},
        dock::{BasePanel, Panel, PanelEvent},
        h_flex, v_flex,
    },
    div,
};
use vortex_chat::{ChatEvent, ChatView, Status, scale::ScaledButton as _};

use crate::{
    commit_review::CommitReview, conversation_picker::ConversationPicker, project::Project,
    project_picker::ProjectPicker,
};

/// Tab titles past this many characters are cut, so a few tabs fit.
const TITLE_CHARS: usize = 28;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatPanelEvent {
    /// The user clicked in the tab or focused it: a new chat goes beside
    /// it.
    Activated,
    /// The tab's "New session" button: another chat in the same group.
    NewTab,
    /// The tab's "Split right" button: another chat in a group beside it.
    SplitRight,
    /// The tab was closed; its process ends with it.
    Removed,
    /// The tab's own close button, which only the last tab shows: the dock
    /// never closes its last tab itself.
    Close,
    /// A conversation picked on the chat's conversations page, to show in
    /// this chat.
    Open { id: String, open_elsewhere: bool },
    /// A folder picked on the projects page.
    OpenProject(PathBuf),
    /// A folder that is not on the projects page is wanted.
    BrowseProjects,
    /// The tab's "New session in a worktree" button: another chat in the
    /// same group, on a branch and worktree of its own.
    NewWorktree,
    /// The chat's own settings changed, to keep with its layout.
    SettingsChanged,
}

pub struct ChatPanel {
    chat: Entity<ChatView>,
    project: Entity<Project>,
    /// The conversations page over the chat, while open.
    picker: Option<(Entity<ConversationPicker>, Subscription)>,
    /// The projects page over the chat, while open.
    projects: Option<(Entity<ProjectPicker>, Subscription)>,
    /// A commit the agent asked for, while it waits for the user.
    review: Option<(Entity<CommitReview>, Subscription)>,
    turn: turn_commit::TurnWatch,
    /// Vortex committed for this chat, so it may push as the chat ends.
    made_commit: bool,
    worktree: worktree::WorktreeState,
    /// The settings chosen for this chat alone, over its project's.
    session: serde_json::Value,
    /// Whether it is the only tab, which then offers its own close button.
    alone: bool,
    /// The dock tracks the active panel's handle on the group around it, so
    /// it must not be the chat's own, which the composer tracks too: one
    /// focus reported by two elements panics GPUI's accessibility tree.
    focus: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl ChatPanel {
    pub fn new(
        chat: Entity<ChatView>,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        let subscriptions = vec![
            // The tab title and its running mark come from the chat.
            cx.observe(&chat, |_, _, cx| cx.notify()),
            cx.subscribe_in(&chat, window, |this, _, event: &ChatEvent, window, cx| {
                if matches!(event, ChatEvent::PickConversation) {
                    this.open_picker(window, cx);
                }
                this.follow_turn(event, window, cx);
            }),
            cx.on_focus_in(&focus, window, |_, _, cx| {
                cx.emit(ChatPanelEvent::Activated);
            }),
            // The dock focuses the panel when its tab is chosen.
            cx.on_focus(&focus, window, |this, window, cx| {
                this.chat.read(cx).focus_handle(cx).focus(window, cx);
            }),
        ];
        let mut this = Self {
            chat,
            project,
            picker: None,
            review: None,
            turn: turn_commit::TurnWatch::default(),
            made_commit: false,
            worktree: worktree::WorktreeState::default(),
            session: serde_json::Value::Object(serde_json::Map::new()),
            projects: None,
            alone: false,
            focus,
            _subscriptions: subscriptions,
        };
        this.read_worktree(cx);
        this.settings_changed(cx);
        this.follow_repo(cx);
        this
    }

    pub fn set_alone(&mut self, alone: bool, cx: &mut Context<Self>) {
        if self.alone != alone {
            self.alone = alone;
            cx.notify();
        }
    }

    pub fn chat(&self) -> &Entity<ChatView> {
        &self.chat
    }

    /// The project the chat works in.
    pub fn project(&self) -> &Entity<Project> {
        &self.project
    }

    /// Works in `project` from now on, with a new conversation.
    pub fn move_to(
        &mut self,
        project: Entity<Project>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project != project {
            let root = project.read(cx).root().to_owned();
            self.project = project;
            self.chat
                .update(cx, |chat, cx| chat.move_to(root, window, cx));
            self.read_worktree(cx);
            self.settings_changed(cx);
            self.follow_repo(cx);
        }
    }
}

impl EventEmitter<PanelEvent> for ChatPanel {}
impl EventEmitter<ChatPanelEvent> for ChatPanel {}

impl BasePanel for ChatPanel {
    fn panel_name(&self) -> &'static str {
        "ChatPanel"
    }

    fn on_removed(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let push = self.settings(cx).git.push;
        if let Some(push) = (self.made_commit)
            .then(|| crate::pushing::wanted(vortex_settings::PushPolicy::SessionEnd, &push))
            .flatten()
        {
            let project = self.project.read(cx).root().to_path_buf();
            crate::pushing::push_in_background(project, push, cx);
        }
        self.drop_unused_worktree(cx);
        cx.emit(ChatPanelEvent::Removed);
    }
}

impl Panel for ChatPanel {
    /// The project and conversation, with a dot while a turn runs, and the
    /// last tab's own close button.
    fn title(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let chat = self.chat.read(cx);
        let running = chat.status() == Status::Running;
        let title = format!(
            "{} · {}",
            self.project.read(cx).name(),
            tab_title(chat.title())
        );
        h_flex()
            .gap_1p5()
            .when(running, |this| {
                this.child(div().size_1p5().rounded_full().bg(cx.theme().primary))
            })
            .child(title)
            .when(self.alone, |this| this.child(Self::close_button(cx)))
    }

    /// None, so the tab draws [`Panel::title`], which holds the last tab's
    /// close button.
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        None
    }

    /// Drawn here rather than as toolbar buttons, which the dock shrinks to
    /// a size where the icons blur on a standard-density display.
    fn title_suffix(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        Some(
            h_flex()
                .gap_1()
                .child(
                    Button::new("chat-new-tab")
                        .ghost()
                        .control_icon(Icon::new(gpui_kit::assets::IconName::Plus))
                        .tab_stop(false)
                        .tooltip("New session")
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatPanelEvent::NewTab))),
                )
                .child(
                    Button::new("chat-new-worktree")
                        .ghost()
                        .control_icon(Icon::new(gpui_kit::assets::IconName::GitBranch))
                        .tab_stop(false)
                        .tooltip("New session in a worktree")
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatPanelEvent::NewWorktree))),
                )
                .child(
                    Button::new("chat-commit-settings")
                        .ghost()
                        .control_icon(Icon::new(gpui_kit::assets::IconName::GitCommitHorizontal))
                        .tab_stop(false)
                        .tooltip("Commit settings")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.open_commit_settings(window, cx);
                        })),
                )
                .child(
                    Button::new("chat-split-right")
                        .ghost()
                        .control_icon(Icon::new(gpui_kit::assets::IconName::Columns2))
                        .tab_stop(false)
                        .tooltip("Split right")
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatPanelEvent::SplitRight))),
                ),
        )
    }

    /// The chat draws its own margins.
    fn inner_padding(&self, _: &App) -> bool {
        false
    }
}

impl ChatPanel {
    fn close_button(cx: &Context<Self>) -> Button {
        Button::new("chat-close")
            .ghost()
            .row_icon(Icon::new(gpui_kit::assets::IconName::Close))
            .tab_stop(false)
            .tooltip("Close session")
            .on_click(cx.listener(|_, _, _, cx| {
                cx.stop_propagation();
                cx.emit(ChatPanelEvent::Close);
            }))
    }
}

fn tab_title(title: Option<&str>) -> SharedString {
    let Some(title) = title else {
        return "New session".into();
    };
    if title.chars().count() <= TITLE_CHARS {
        return title.to_owned().into();
    }
    let cut: String = title.chars().take(TITLE_CHARS - 1).collect();
    format!("{}…", cut.trim_end()).into()
}

impl Focusable for ChatPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for ChatPanel {
    /// A click anywhere in the chat makes it the one a new chat goes
    /// beside, also on text or buttons that take no focus. The dock also
    /// shows each group's tab when it lays them out, which says nothing of
    /// where the user works, so being shown does not.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // The backdrop keeps clicks from reaching the chat under it.
        let picker = self.picker.as_ref().map(|(picker, _)| {
            div()
                .id("conversations-overlay")
                .test_support()
                .absolute()
                .inset_0()
                .p_3()
                .occlude()
                .bg(cx.theme().overlay)
                .child(picker.clone())
        });
        let projects = self.projects.as_ref().map(|(page, _)| {
            v_flex()
                .id("projects-overlay")
                .test_support()
                .absolute()
                .inset_0()
                .p_3()
                .items_center()
                .occlude()
                .bg(cx.theme().overlay)
                .child(page.clone())
        });
        let review = self.review.as_ref().map(|(review, _)| {
            v_flex()
                .absolute()
                .inset_0()
                .p_4()
                .items_center()
                .justify_end()
                .occlude()
                .bg(cx.theme().overlay)
                .child(review.clone())
        });
        div()
            .relative()
            .size_full()
            .capture_any_mouse_down(cx.listener(|_, _, _, cx| cx.emit(ChatPanelEvent::Activated)))
            .child(
                v_flex()
                    .size_full()
                    .children(self.render_worktree(cx))
                    .children(self.render_work(cx))
                    .child(div().flex_1().min_h_0().child(self.chat.clone())),
            )
            .children(review)
            .children(picker)
            .children(projects)
    }
}

#[cfg(test)]
mod tests {
    use super::tab_title;

    #[test]
    fn long_titles_are_cut_to_fit_a_tab() {
        assert_eq!(tab_title(None), "New session");
        assert_eq!(tab_title(Some("Fix the build")), "Fix the build");
        let long = tab_title(Some("Replace the file tree with the session history"));
        assert!(long.chars().count() <= 28, "{long}");
        assert!(long.ends_with('…'));
    }
}
