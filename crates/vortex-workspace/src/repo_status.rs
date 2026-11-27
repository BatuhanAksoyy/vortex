//! The project's git status, which the Git page lists and the Files page
//! marks its tree with.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use gpui_kit::{AppContext as _, Context, EventEmitter, Hsla, Task, component::Theme};
use vortex_git::{Kind, Status};

/// The status was read again. Files may differ though the status does not.
pub struct StatusRead;

pub struct RepoStatus {
    project: PathBuf,
    /// `None` until the first reading; an error when the folder is not a
    /// repository or `git` cannot run.
    reading: Option<Result<Status, String>>,
    marks: HashMap<PathBuf, Kind>,
    load: Option<Task<()>>,
}

impl EventEmitter<StatusRead> for RepoStatus {}

impl RepoStatus {
    pub fn new(project: PathBuf) -> Self {
        Self {
            project,
            reading: None,
            marks: HashMap::new(),
            load: None,
        }
    }

    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let project = self.project.clone();
        let read = cx.background_spawn(async move {
            vortex_git::status(&project).map(|status| (status.marks(), status))
        });
        self.load = Some(cx.spawn(async move |this, cx| {
            let reading = read.await;
            let _ = this.update(cx, |this, cx| this.show(reading, cx));
        }));
    }

    fn show(
        &mut self,
        reading: anyhow::Result<(HashMap<PathBuf, Kind>, Status)>,
        cx: &mut Context<Self>,
    ) {
        let (marks, reading) = match reading {
            Ok((marks, status)) => (marks, Ok(status)),
            Err(error) => (HashMap::new(), Err(format!("{error:#}"))),
        };
        self.marks = marks;
        self.reading = Some(reading);
        cx.emit(StatusRead);
        cx.notify();
    }

    /// The project folder it reads.
    pub fn project(&self) -> &Path {
        &self.project
    }

    pub fn reading(&self) -> Option<&Result<Status, String>> {
        self.reading.as_ref()
    }

    /// The last reading, when it succeeded.
    pub fn status(&self) -> Option<&Status> {
        self.reading.as_ref()?.as_ref().ok()
    }

    /// How the file or folder at `path`, in the project, changed.
    pub fn mark(&self, path: &Path) -> Option<Kind> {
        self.marks.get(path).copied()
    }
}

pub fn kind_color(kind: Kind, theme: &Theme) -> Hsla {
    match kind {
        Kind::Added | Kind::Untracked => theme.success,
        Kind::Deleted | Kind::Conflicted => theme.danger,
        Kind::Modified | Kind::TypeChanged => theme.warning,
        Kind::Renamed | Kind::Copied => theme.info,
    }
}
