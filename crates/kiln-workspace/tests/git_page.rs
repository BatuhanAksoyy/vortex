//! The Git page: listing changes, staging and committing, and branches.

// Inside a `cfg(test)` module so helpers may `expect`, like unit tests.
#[cfg(test)]
#[path = "common/project.rs"]
mod project;

#[cfg(test)]
mod git_page {
    use std::time::Duration;

    use gpui_kit::test::{TestAppContextExt, TestQueryExt, TestWindowExt};
    use gpui_kit::{AppContext as _, TestAppContext};

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
}
