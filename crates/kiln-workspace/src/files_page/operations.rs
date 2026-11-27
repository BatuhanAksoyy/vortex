//! Creating, renaming and deleting files from the tree.

use std::path::{Path, PathBuf};

use gpui_kit::{
    AppContext as _, Context, Window,
    component::{WindowExt as _, button::ButtonVariant, notification::Notification},
};

use super::{FilesChanged, FilesPage};
use crate::name_prompt::{self, Question};

impl FilesPage {
    /// Where a new file goes: the selected folder, or the selected file's.
    pub(super) fn target(&self, cx: &Context<Self>) -> PathBuf {
        let selected = self.selected(cx);
        selected.map_or_else(PathBuf::new, |selected| self.folder_of(&selected))
    }

    pub(super) fn folder_of(&self, relative: &Path) -> PathBuf {
        if self.is_folder(&relative.to_string_lossy()) {
            return relative.to_path_buf();
        }
        relative.parent().map_or_default(Path::to_path_buf)
    }

    pub(super) fn ask_new_file(
        &mut self,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page = cx.entity().downgrade();
        let folder = self.root.join(folder);
        let question = Question {
            title: self.place("New File", &folder),
            action: "Create",
            initial: String::new(),
        };
        let submit = move |name: &str, window: &mut Window, cx: &mut gpui_kit::App| {
            let path = kiln_project::create_file(&folder, name)?;
            page.update(cx, |page, cx| page.created(&path, window, cx))
        };
        name_prompt::ask(question, submit, window, cx);
    }

    pub(super) fn ask_new_folder(
        &mut self,
        folder: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page = cx.entity().downgrade();
        let folder = self.root.join(folder);
        let question = Question {
            title: self.place("New Folder", &folder),
            action: "Create",
            initial: String::new(),
        };
        let submit = move |name: &str, _: &mut Window, cx: &mut gpui_kit::App| {
            let path = kiln_project::create_folder(&folder, name)?;
            page.update(cx, |page, cx| page.reveal_new(&path, cx))
        };
        name_prompt::ask(question, submit, window, cx);
    }

    /// `New File in src/ui`
    fn place(&self, what: &str, folder: &Path) -> gpui_kit::SharedString {
        let relative = folder.strip_prefix(&self.root).unwrap_or(folder);
        if relative.as_os_str().is_empty() {
            return what.to_owned().into();
        }
        format!("{what} in {}", relative.display()).into()
    }

    fn created(&mut self, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        self.reveal_new(path, cx);
        self.open_in_editor(path.to_path_buf(), None, window, cx);
    }

    fn reveal_new(&mut self, path: &Path, cx: &mut Context<Self>) {
        if let Ok(relative) = path.strip_prefix(&self.root) {
            self.reveal(relative, cx);
        }
        cx.emit(FilesChanged);
    }

    pub(super) fn ask_rename(
        &mut self,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(name) = relative.file_name() else {
            return;
        };
        let name = name.to_string_lossy().into_owned();
        let page = cx.entity().downgrade();
        let path = self.root.join(relative);
        let question = Question {
            title: format!("Rename “{name}”").into(),
            action: "Rename",
            initial: name,
        };
        let submit = move |name: &str, _: &mut Window, cx: &mut gpui_kit::App| {
            let to = kiln_project::rename(&path, name)?;
            page.update(cx, |page, cx| {
                page.renamed_file(&path, &to, cx);
                page.reveal_new(&to, cx);
            })
        };
        name_prompt::ask(question, submit, window, cx);
    }

    pub(super) fn ask_delete(
        &mut self,
        relative: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let page = cx.entity().downgrade();
        let path = self.root.join(relative);
        let shown = relative.display().to_string();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (page, path) = (page.clone(), path.clone());
            alert
                .title(format!("Delete “{shown}”?"))
                .description("It moves to the trash, where you can restore it from.")
                .confirm()
                .ok_text("Move to Trash")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    let _ = page.update(cx, |_, cx| Self::delete(path.clone(), window, cx));
                    true
                })
        });
    }

    fn delete(path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let delete = cx.background_spawn(async move { kiln_project::trash(&path).map(|()| path) });
        cx.spawn_in(window, async move |this, cx| {
            let result = delete.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(path) => {
                    this.deleted_file(&path, window, cx);
                    cx.emit(FilesChanged);
                }
                Err(error) => {
                    let message = format!("{error:#}");
                    window.push_notification(Notification::error(message), cx);
                }
            });
        })
        .detach();
    }
}
