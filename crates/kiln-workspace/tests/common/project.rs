//! A Kiln window over a project of its own, with no agent behind it.

use std::path::PathBuf;

use gpui_kit::test::{TestQueryExt, TestWindowExt};
use gpui_kit::{
    AppContext, Bounds, Entity, TestAppContext, WindowBounds, WindowHandle, WindowOptions, px, size,
};
use kiln_workspace::Workspace;

/// The chat starts its agent when it opens; these tests need none.
struct NoAgent;

impl kiln_agent::AgentBackend for NoAgent {
    fn name(&self) -> &'static str {
        "claude"
    }

    fn spawn(&self, _: kiln_agent::SessionSpec) -> anyhow::Result<kiln_agent::Session> {
        anyhow::bail!("no agent in workspace tests")
    }

    fn sign_in(&self, _: Vec<(String, String)>) -> anyhow::Result<kiln_agent::SignIn> {
        anyhow::bail!("no agent in workspace tests")
    }

    fn past_sessions(
        &self,
        _: &std::path::Path,
    ) -> kiln_agent::Pending<Vec<kiln_agent::PastSession>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn past_messages(
        &self,
        _: &std::path::Path,
        _: &str,
    ) -> kiln_agent::Pending<Vec<kiln_agent::PastMessage>> {
        Box::pin(async { anyhow::bail!("no stored conversations in workspace tests") })
    }
}

/// Workspace tests add no account, so the list is never written.
fn no_accounts(cx: &mut gpui_kit::App) -> Entity<kiln_accounts::Accounts> {
    let dir = std::env::temp_dir().join("kiln-workspace-tests");
    cx.new(|_| kiln_accounts::Accounts::empty(dir))
}

pub fn open_workspace(
    cx: &mut TestAppContext,
    root: PathBuf,
) -> (WindowHandle<gpui_kit::base::Root>, Entity<Workspace>) {
    open_workspace_keeping(cx, root, None)
}

/// A workspace in `root` that keeps its data, worktrees too, in `data`.
pub fn open_workspace_keeping(
    cx: &mut TestAppContext,
    root: PathBuf,
    data: Option<&std::path::Path>,
) -> (WindowHandle<gpui_kit::base::Root>, Entity<Workspace>) {
    let layout = data.map(|data| data.join("layout.json"));
    cx.update(gpui_kit::init);
    cx.update(kiln_workspace::init);
    let (window, workspace) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    gpui_kit::Point::default(),
                    size(px(1280.), px(800.)),
                ))),
                ..Default::default()
            },
            cx,
            |window, cx| {
                let backend = std::rc::Rc::new(NoAgent);
                let accounts = no_accounts(cx);
                cx.new(|cx| {
                    let workspace = Workspace::new(Some(root), backend, accounts, window, cx);
                    match &layout {
                        Some(layout) => workspace.keeping_data_beside(layout),
                        None => workspace,
                    }
                })
            },
        )
        .expect("open test window")
    });
    cx.run_until_parked();
    (window.downcast().expect("kit window has a Root"), workspace)
}

/// The production keymap binds `cmd` on macOS where it binds `ctrl`
/// elsewhere.
pub fn primary(key: &str) -> String {
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    format!("{modifier}-{key}")
}

pub fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::create_dir_all(dir.path().join("src")).expect("mkdir");
    std::fs::write(dir.path().join("src/main.rs"), "fn main() {}\n").expect("write");
    std::fs::write(dir.path().join("README.md"), "# demo\n").expect("write");
    dir
}

pub fn click_label(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, label: &str) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        let at = window.find_by_label(label).bounds().center();
        window.drag(at, at, cx);
    })
    .expect("window");
    cx.run_until_parked();
}

pub fn click(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, id: &'static str) {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    })
    .expect("window");
    cx.run_until_parked();
}

pub fn git(root: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args(["-c", "user.name=Kiln", "-c", "user.email=kiln@example.com"])
        .args(args)
        .current_dir(root)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?}");
}

pub fn press(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, window, cx| window.press(key, cx))
        .expect("window");
    cx.run_until_parked();
}

pub fn type_text(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, text: &str) {
    cx.update_window(window, |_, window, cx| window.input(text, cx))
        .expect("window");
    cx.run_until_parked();
}

pub fn has(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, id: &'static str) -> bool {
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.try_find(id).is_some()
    })
    .expect("window")
}

/// A repository with `README.md` and `src/main.rs` changed.
pub fn changed_project() -> tempfile::TempDir {
    let dir = project();
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "start"]);
    std::fs::write(root.join("README.md"), "# changed\n").expect("write");
    std::fs::write(root.join("src/main.rs"), "// changed\n").expect("write");
    dir
}
