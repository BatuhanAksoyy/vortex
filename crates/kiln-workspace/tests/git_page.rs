//! Git in the workspace: the Git page's changes, commits, branches and
//! pushes, and chats on worktrees of their own.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
#[path = "common/project.rs"]
mod project;

#[cfg(test)]
mod git_page {
    use std::{path::Path, time::Duration};

    use gpui_kit::test::{TestAppContextExt, TestQueryExt, TestWindowExt};
    use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};

    use super::project::*;

    #[gpui_kit::test]
    async fn the_git_page_lists_changes_and_shows_their_diff(cx: &mut TestAppContext) {
        let dir = project();
        let root = dir.path();
        git(root, &["init", "-q", "-b", "main"]);
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "start"]);
        std::fs::write(root.join("README.md"), "# changed\n").expect("write");
        std::fs::write(root.join("notes.txt"), "new\n").expect("write");
        let (window, _) = open_workspace(cx, root.to_path_buf());
        let window = window.into();

        click(cx, window, "activity-git");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("git-diff").is_some()
        })
        .await;
        assert!(has(cx, window, "git-change-unstaged-README.md"));
        assert!(has(cx, window, "git-change-untracked-notes.txt"));
        let branch = cx
            .update_window(window, |_, window, cx| {
                window.render_frame(cx);
                window.try_find_by_label("main").is_some()
            })
            .expect("window");
        assert!(branch, "the branch is shown");
        click(cx, window, "git-change-untracked-notes.txt");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("git-diff").is_some()
        })
        .await;
    }

    #[gpui_kit::test]
    async fn the_git_page_is_walked_by_keyboard_into_a_file(cx: &mut TestAppContext) {
        let dir = changed_project();
        let root = std::fs::canonicalize(dir.path()).expect("canonical root");
        let (window, workspace) = open_workspace(cx, root.clone());
        let window = window.into();
        click(cx, window, "activity-git");
        let second = "src/main.rs: Modified";
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find_by_label("README.md: Modified").is_some()
        })
        .await;
        click(cx, window, "git-section-unstaged");
        assert!(!has(cx, window, "git-change-unstaged-README.md"), "folded");
        click(cx, window, "git-section-unstaged");
        press(cx, window, "down");
        cx.wait_for(window, Duration::from_secs(5), move |window, _| {
            window.try_find_by_label(second).is_some()
        })
        .await;
        press(cx, window, "enter");
        let active = cx.update(|cx| workspace.read(cx).active_file(cx));
        assert_eq!(active, Some(root.join("src/main.rs")));
        assert!(has(cx, window, "files-tree"));
    }

    #[gpui_kit::test]
    async fn the_git_page_stages_and_commits(cx: &mut TestAppContext) {
        let dir = changed_project();
        let root = dir.path();
        git(root, &["config", "user.name", "Kiln"]);
        git(root, &["config", "user.email", "kiln@example.com"]);
        let (window, _) = open_workspace(cx, root.to_path_buf());
        let window = window.into();
        click(cx, window, "activity-git");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("git-change-unstaged-README.md").is_some()
        })
        .await;
        click(cx, window, "git-stage-unstaged-README.md");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("git-change-staged-README.md").is_some()
        })
        .await;
        click_label(cx, window, "Commit message");
        type_text(cx, window, &"x".repeat(80));
        press(cx, window, &primary("enter"));
        assert!(
            has(cx, window, "git-write-error"),
            "the subject is too long"
        );
        press(cx, window, &primary("a"));
        type_text(cx, window, "Change the readme");
        press(cx, window, "enter");
        press(cx, window, "enter");
        type_text(cx, window, "Co-Authored-By: Claude <noreply@anthropic.com>");
        press(cx, window, &primary("enter"));
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("git-change-staged-README.md").is_none()
        })
        .await;
        let log = std::process::Command::new("git")
            .args(["log", "-1", "--format=%B"])
            .current_dir(root)
            .output()
            .expect("git log");
        assert_eq!(
            String::from_utf8_lossy(&log.stdout).trim(),
            "Change the readme"
        );
        assert!(
            has(cx, window, "git-change-unstaged-src/main.rs"),
            "left unstaged"
        );
    }

    /// Clicks the open menu's item labelled `label`.
    fn pick(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle, label: &'static str) {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            let at = window.find_by_label(label).bounds().center();
            window.drag(at, at, cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    fn head(root: &std::path::Path) -> String {
        let out = std::process::Command::new("git")
            .args(["branch", "--show-current"])
            .current_dir(root)
            .output()
            .expect("git branch");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    #[gpui_kit::test]
    async fn the_branch_menu_creates_and_switches_branches(cx: &mut TestAppContext) {
        let dir = changed_project();
        let root = dir.path().to_path_buf();
        let (window, _) = open_workspace(cx, root.clone());
        let window = window.into();
        click(cx, window, "activity-git");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("git-branch").is_some()
        })
        .await;
        click(cx, window, "git-branch");
        pick(cx, window, "Create Branch…");
        type_text(cx, window, "feature/menu");
        press(cx, window, "enter");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("git-new-branch").is_none()
        })
        .await;
        assert_eq!(head(&root), "feature/menu");
        // The status is read again, and the branches with it.
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find_by_label("feature/menu").is_some()
        })
        .await;
        click(cx, window, "git-branch");
        pick(cx, window, "main");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find_by_label("main").is_some()
        })
        .await;
        assert_eq!(head(&root), "main");
        assert!(
            has(cx, window, "git-change-unstaged-README.md"),
            "changes kept"
        );
    }

    #[gpui_kit::test]
    async fn the_git_page_says_when_there_is_no_repository(cx: &mut TestAppContext) {
        let dir = project();
        let (window, _) = open_workspace(cx, dir.path().to_path_buf());
        let window = window.into();
        click(cx, window, "activity-git");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window
                .try_find_by_label("This folder is not in a git repository.")
                .is_some()
        })
        .await;
    }

    fn remote_branches(remote: &std::path::Path) -> String {
        let out = std::process::Command::new("git")
            .args(["branch", "--list", "--format=%(refname:short)"])
            .current_dir(remote)
            .output()
            .expect("git branch");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    #[gpui_kit::test]
    async fn pushing_main_asks_first(cx: &mut TestAppContext) {
        let dir = changed_project();
        let root = dir.path();
        let remote = tempfile::tempdir().expect("tempdir");
        git(remote.path(), &["init", "-q", "--bare"]);
        git(
            root,
            &["remote", "add", "origin", &remote.path().to_string_lossy()],
        );
        let (window, _) = open_workspace(cx, root.to_path_buf());
        let window = window.into();
        click(cx, window, "activity-git");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("git-push").is_some()
        })
        .await;
        click(cx, window, "git-push");
        assert_eq!(
            remote_branches(remote.path()),
            "",
            "not before the user confirms"
        );
        click_label(cx, window, "Push");
        cx.wait_for(window, Duration::from_secs(5), |_, _| {
            remote_branches(remote.path()) == "main"
        })
        .await;
    }

    /// The only worktree Kiln made under `data`.
    fn made(data: &Path) -> std::path::PathBuf {
        let repos = std::fs::read_dir(data.join("worktrees")).expect("worktrees");
        let repo = repos.flatten().next().expect("one repository").path();
        let trees = std::fs::read_dir(repo).expect("its worktrees");
        trees.flatten().next().expect("one worktree").path()
    }

    async fn open_worktree_chat(
        cx: &mut TestAppContext,
        root: &Path,
        data: &Path,
    ) -> AnyWindowHandle {
        let (window, _) = open_workspace_keeping(cx, root.to_path_buf(), Some(data));
        let window = window.into();
        click(cx, window, "chat-new-worktree");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("chat-worktree").is_some()
        })
        .await;
        window
    }

    fn branches(root: &Path) -> String {
        let out = std::process::Command::new("git")
            .args(["branch", "--format=%(refname:short)"])
            .current_dir(root)
            .output()
            .expect("git branch");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    #[gpui_kit::test]
    async fn a_worktree_chat_merges_into_its_base(cx: &mut TestAppContext) {
        let (dir, data) = (changed_project(), tempfile::tempdir().expect("tempdir"));
        let root = dir.path();
        let window = open_worktree_chat(cx, root, data.path()).await;
        let tree = made(data.path());
        assert!(branches(root).contains("kiln/"), "{}", branches(root));
        std::fs::write(tree.join("agent.txt"), "work\n").expect("write");
        git(&tree, &["add", "agent.txt"]);
        git(&tree, &["commit", "-q", "-m", "Add the agent's work"]);
        click(cx, window, "chat-worktree-merge");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("chat-worktree").is_none()
        })
        .await;
        assert!(root.join("agent.txt").exists(), "merged");
        assert!(!tree.exists(), "the worktree is gone");
        assert_eq!(branches(root), "main");
        assert!(root.join("README.md").exists());
    }

    #[gpui_kit::test]
    async fn a_discarded_worktree_takes_its_work_along(cx: &mut TestAppContext) {
        let (dir, data) = (changed_project(), tempfile::tempdir().expect("tempdir"));
        let root = dir.path();
        let window = open_worktree_chat(cx, root, data.path()).await;
        let tree = made(data.path());
        std::fs::write(tree.join("agent.txt"), "work\n").expect("write");
        click(cx, window, "chat-worktree-discard");
        click(cx, window, "ok");
        cx.wait_for(window, Duration::from_secs(5), |window, _| {
            window.try_find("chat-worktree").is_none()
        })
        .await;
        assert!(!tree.exists());
        assert_eq!(branches(root), "main");
    }

    /// Closes the worktree chat's tab, the second in the group.
    fn close_worktree_tab(cx: &mut TestAppContext, window: AnyWindowHandle) {
        cx.update_window(window, |_, window, cx| {
            window.render_frame(cx);
            window.within("tab-bar").click(("close-tab", 1_usize), cx);
        })
        .expect("window");
        cx.run_until_parked();
    }

    #[gpui_kit::test]
    async fn closing_an_unused_worktree_chat_takes_its_worktree(cx: &mut TestAppContext) {
        let (dir, data) = (changed_project(), tempfile::tempdir().expect("tempdir"));
        let root = dir.path();
        let window = open_worktree_chat(cx, root, data.path()).await;
        let tree = made(data.path());
        close_worktree_tab(cx, window);
        cx.wait_for(window, Duration::from_secs(5), |_, _| !tree.exists())
            .await;
        assert_eq!(branches(root), "main");
    }

    #[gpui_kit::test]
    async fn a_closed_worktree_chat_keeps_its_work(cx: &mut TestAppContext) {
        let (dir, data) = (changed_project(), tempfile::tempdir().expect("tempdir"));
        let root = dir.path();
        let window = open_worktree_chat(cx, root, data.path()).await;
        let tree = made(data.path());
        std::fs::write(tree.join("agent.txt"), "work\n").expect("write");
        close_worktree_tab(cx, window);
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert!(tree.join("agent.txt").exists(), "the work stays");
        assert!(branches(root).contains("kiln/"));
    }
}
