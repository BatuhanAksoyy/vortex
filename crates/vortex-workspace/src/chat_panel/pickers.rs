//! What a chat shows over itself to pick from: the project to work in and
//! the conversation to go on with.

use std::path::PathBuf;

use gpui_kit::{AppContext as _, Context, Focusable as _, Window};

use super::{ChatPanel, ChatPanelEvent};
use crate::{
    conversation_picker::{ConversationPicker, ConversationPickerEvent},
    project_picker::{ProjectPicker, ProjectPickerEvent},
};

impl ChatPanel {
    /// Shows `folders` over the chat, to pick the one to work in.
    pub fn open_projects(
        &mut self,
        folders: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self.project.read(cx).root().to_owned();
        let page = cx.new(|cx| ProjectPicker::new(folders, Some(current), cx));
        let subscription = cx.subscribe_in(&page, window, |this, _, event, window, cx| {
            this.close_projects(window, cx);
            match event {
                ProjectPickerEvent::Pick(folder) if folder != this.project.read(cx).root() => {
                    cx.emit(ChatPanelEvent::OpenProject(folder.clone()));
                }
                ProjectPickerEvent::Browse => cx.emit(ChatPanelEvent::BrowseProjects),
                ProjectPickerEvent::Pick(_) | ProjectPickerEvent::Close => {}
            }
        });
        ProjectPicker::focus(&page, window, cx);
        self.projects = Some((page, subscription));
        cx.notify();
    }

    fn close_projects(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.projects.take().is_some() {
            self.chat.read(cx).focus_handle(cx).focus(window, cx);
            cx.notify();
        }
    }

    pub(super) fn open_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.chat.read(cx).session_id().map(str::to_owned);
        let sessions = self.project.read(cx).sessions().clone();
        let picker = cx.new(|cx| ConversationPicker::new(sessions, current, window, cx));
        let subscription = cx.subscribe_in(&picker, window, |this, _, event, window, cx| {
            this.close_picker(window, cx);
            if let ConversationPickerEvent::Pick { id, open_elsewhere } = event {
                cx.emit(ChatPanelEvent::Open {
                    id: id.clone(),
                    open_elsewhere: *open_elsewhere,
                });
            }
        });
        ConversationPicker::focus(&picker, window, cx);
        self.picker = Some((picker, subscription));
        cx.notify();
    }

    fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.picker.take().is_some() {
            self.chat.read(cx).focus_handle(cx).focus(window, cx);
            cx.notify();
        }
    }
}
