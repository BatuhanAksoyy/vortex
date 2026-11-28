//! The folders the sessions work in: choosing one, and following the one
//! the active session works in.

use std::path::{Path, PathBuf};

use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, PathPromptOptions, TestSupportExt, Window,
    component::{WindowExt, notification::Notification, v_flex},
};

use super::{
    Workspace,
    chats::{self, Place},
};
use crate::{
    chat_panel::ChatPanel,
    project::{Project, ProjectEvent},
    project_picker::{ProjectPicker, ProjectPickerEvent},
};

impl Workspace {
    /// The project the active session works in.
    pub(super) fn project<'a>(&self, cx: &'a App) -> Option<&'a Entity<Project>> {
        let panel = self.active_chat.as_ref()?;
        Some(panel.read(cx).project())
    }

    /// The project of `folder`, kept as long as the window is: its editor
    /// may hold unsaved changes.
    pub(super) fn project_at(
        &mut self,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<Project> {
        let root = folder
            .canonicalize()
            .unwrap_or_else(|_| folder.to_path_buf());
        let known = self.projects.iter().map(|(project, _)| project);
        if let Some(project) = known.into_iter().find(|p| p.read(cx).root() == root) {
            return project.clone();
        }
        crate::config::load_project(&root, cx);
        // Its settings files are watched too.
        let watch = Self::watch_config(window, cx);
        let _previous = self.config_watch.replace(watch);
        let project = cx.new(|cx| Project::new(root, window, cx));
        let subscription = cx.subscribe_in(&project, window, Self::on_project);
        self.projects.push((project.clone(), subscription));
        let backends = self.backends.clone();
        project.update(cx, |project, cx| project.reload_sessions(&backends, cx));
        self.lay_out_pages(cx);
        project
    }

    fn on_project(
        &mut self,
        project: &Entity<Project>,
        event: &ProjectEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ProjectEvent::SessionsListed => {
                self.restore_layout(window, cx);
                self.sync_titles(project, cx);
            }
            ProjectEvent::OpenFile(path) => self.open_absolute(path.clone(), window, cx),
            ProjectEvent::CommitSettings => {
                self.commit_settings(vortex_settings::Scope::Project, window, cx);
            }
        }
    }

    /// Makes `panel` the chat the user works in; the side panel shows its
    /// project.
    pub(super) fn activate(
        &mut self,
        panel: Option<Entity<ChatPanel>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_chat = panel;
        self.sync_brand(cx);
        self.sync_browser(window, cx);
        let root = self.project(cx).map(|p| p.read(cx).root().to_owned());
        if self.shown_root == root {
            return;
        }
        self.shown_root = root;
        self.load_project(window, cx);
        cx.notify();
    }

    /// Colors Vortex after the active chat's harness, and in its own
    /// neutral colors without one.
    pub(super) fn sync_brand(&self, cx: &mut Context<Self>) {
        let brand = self
            .active_chat
            .as_ref()
            .map_or(vortex_chat::Brand::Vortex, |panel| {
                vortex_chat::Brand::of(panel.read(cx).chat().read(cx).agent_name())
            });
        crate::config::set_brand(brand, cx);
    }

    /// Starts working in `folder`: the active chat moves there when nothing
    /// was said in it, otherwise a new one opens beside it.
    pub(super) fn open_folder(
        &mut self,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let project = self.project_at(folder, window, cx);
        if let Err(error) = self.recent.add(project.read(cx).root()) {
            log::warn!("cannot keep the project list: {error:#}");
        }
        let active = self.active_chat.clone();
        let blank = active.filter(|panel| panel.read(cx).chat().read(cx).is_blank());
        let chat = match blank {
            Some(panel) => {
                panel.update(cx, |panel, cx| panel.move_to(project, window, cx));
                self.activate(Some(panel.clone()), window, cx);
                panel.read(cx).chat().clone()
            }
            None => self.add_chat(Place::Tab, &project, window, cx),
        };
        chats::focus(&chat, window, cx);
        cx.notify();
    }

    pub fn prompt_open_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: None,
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            if let Some(folder) = paths.into_iter().next() {
                let _ =
                    this.update_in(cx, |this, window, cx| this.open_folder(&folder, window, cx));
            }
        })
        .detach();
    }

    /// Shows the folders to work in over the active chat.
    pub(super) fn pick_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.active_chat.clone() else {
            return;
        };
        let folders = self.recent.folders();
        panel.update(cx, |panel, cx| panel.open_projects(folders, window, cx));
    }

    /// Opens the folder named when Vortex was started, or shows the folders
    /// to choose from while no chat is open.
    pub(super) fn start_in_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(folder) = self.first_folder.take() {
            let project = self.project_at(&folder, window, cx);
            let open = self.chats_in(&project, cx);
            match open {
                Some(panel) => self.show_chat_panel(&panel, window, cx),
                None => self.open_folder(&folder, window, cx),
            }
        }
        if self.active_chat.is_none() {
            self.show_start_page(window, cx);
        }
    }

    pub(super) fn show_start_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folders = self.recent.folders();
        let page = cx.new(|cx| ProjectPicker::new(folders, None, cx));
        let subscription =
            cx.subscribe_in(&page, window, |this, _, event, window, cx| match event {
                ProjectPickerEvent::Pick(folder) => this.open_folder(folder, window, cx),
                ProjectPickerEvent::Browse => this.prompt_open_folder(window, cx),
                ProjectPickerEvent::Close => {}
            });
        ProjectPicker::focus(&page, window, cx);
        self.start_page = Some((page, subscription));
        self.sync_brand(cx);
        // With no project, only the user's own settings apply.
        self.load_project(window, cx);
        cx.notify();
    }

    /// The folders to choose from, in place of the chats while none is open.
    pub(super) fn render_start_page(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let (page, _) = self.start_page.as_ref()?;
        self.active_chat.is_none().then(|| {
            v_flex()
                .id("start-page")
                .test_support()
                .size_full()
                .p_6()
                .items_center()
                .justify_center()
                .child(page.clone())
                .child(Self::render_agent_check(cx))
                .into_any_element()
        })
    }

    /// Opens `path` in the active session's editor and shows it there.
    pub(super) fn open_absolute(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.project(cx).cloned() else {
            // Without a session there is no editor to show it in.
            cx.open_with_system(&path);
            return;
        };
        project.update(cx, |project, cx| project.open(path, window, cx));
        self.show_page(crate::side_panel::Page::Files, window, cx);
    }

    pub(super) fn needs_project(window: &mut Window, cx: &mut Context<Self>) {
        window.push_notification(Notification::info("Open a project folder first."), cx);
    }
}
