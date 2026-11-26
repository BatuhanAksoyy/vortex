//! A new chat on a worktree of its own, beside the user's checkout, kept
//! with Kiln's data: `worktrees/<repository>/<name>`, on branch
//! `kiln/<name>`.

use std::{
    hash::{DefaultHasher, Hash as _, Hasher as _},
    path::{Path, PathBuf},
};

use gpui_kit::{
    AppContext as _, Context, Entity, Window,
    component::{WindowExt as _, notification::Notification},
};

use super::{
    Workspace,
    chats::{Place, focus},
};
use crate::chat_panel::ChatPanel;

impl Workspace {
    pub(super) fn new_worktree_chat(
        &mut self,
        from: &Entity<ChatPanel>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(data) = self.data_dir.clone() else {
            let missing = "Worktrees are kept with Kiln's data, which this window has none of.";
            window.push_notification(Notification::error(missing), cx);
            return;
        };
        let project = from.read(cx).project().read(cx).root().to_path_buf();
        let name = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
        let made = cx.background_spawn(async move { make(&project, &data, &name) });
        cx.spawn_in(window, async move |this, cx| {
            let made = made.await;
            let _ = this.update_in(cx, |this, window, cx| match made {
                Ok(folder) => {
                    let project = this.project_at(&folder, window, cx);
                    let chat = this.add_chat(Place::Tab, &project, window, cx);
                    focus(&chat, window, cx);
                }
                Err(error) => {
                    let error = format!("No worktree: {error:#}");
                    window.push_notification(Notification::error(error), cx);
                }
            });
        })
        .detach();
    }
}

/// Makes the worktree, returning the folder in it that matches `project`.
fn make(project: &Path, data: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let main = kiln_git::status(project)?.root;
    let mut hasher = DefaultHasher::new();
    main.hash(&mut hasher);
    let parent = data
        .join("worktrees")
        .join(format!("{:016x}", hasher.finish()));
    std::fs::create_dir_all(&parent)?;
    let (_, folder) = kiln_git::create_worktree(project, &parent.join(name), name)?;
    Ok(folder)
}
