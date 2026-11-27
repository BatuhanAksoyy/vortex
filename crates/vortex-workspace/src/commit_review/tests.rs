use std::{
    path::{Path, PathBuf},
    process::Command,
};

use gpui_kit::test::{TestAppContextExt as _, TestWindowExt as _};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, TestAppContext, WindowBounds, WindowOptions, px, size,
};
use vortex_bridge::{Content, Reply};

use super::{CommitReview, Pending, files};

fn git(root: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .expect("git");
    assert!(out.status.success(), "git {args:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository with one commit, `a.txt` changed since and `b.txt` new.
fn repository() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path();
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "user.name", "Vortex"]);
    git(root, &["config", "user.email", "vortex@example.com"]);
    std::fs::create_dir(root.join("docs")).expect("mkdir");
    std::fs::write(root.join("a.txt"), "a\n").expect("write");
    git(root, &["add", "."]);
    git(root, &["commit", "-q", "-m", "start"]);
    std::fs::write(root.join("a.txt"), "a\nmore\n").expect("write");
    std::fs::write(root.join("docs/b.txt"), "b\n").expect("write");
    dir
}

#[test]
fn a_call_takes_the_files_under_its_paths() {
    let repo = repository();
    let status = vortex_git::status(repo.path()).expect("status");
    let names = |paths: &[&str]| -> Vec<String> {
        let paths: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        files(&status, &paths)
            .into_iter()
            .map(|file| file.path)
            .collect()
    };
    assert_eq!(names(&[]), ["a.txt", "docs/b.txt"]);
    assert_eq!(names(&["docs"]), ["docs/b.txt"]);
    assert_eq!(names(&["nowhere"]), Vec::<String>::new());
    let a = &files(&status, &[PathBuf::from("a.txt")])[0];
    assert_eq!((a.letter, a.added, a.removed), ('M', 1, 0));
}

/// The review of a call to commit `paths` with `message`, and where the
/// agent's answer arrives.
fn review(
    cx: &mut TestAppContext,
    root: &Path,
    message: &str,
    paths: &[&str],
) -> (AnyWindowHandle, async_channel::Receiver<Reply>) {
    cx.update(gpui_kit::init);
    cx.update(crate::config::init);
    let (reply, answer) = async_channel::bounded(1);
    let paths: Vec<String> = paths.iter().map(|path| (*path).to_owned()).collect();
    let status = vortex_git::status(root).expect("status");
    let commit = cx.update(|cx| crate::config::settings(cx).git.commit.clone());
    let rules = super::message_rules(&commit).expect("rules");
    let pending = Pending {
        project: root.to_path_buf(),
        files: files(
            &status,
            &paths.iter().map(PathBuf::from).collect::<Vec<_>>(),
        ),
        paths,
        sent: message.to_owned(),
        cleaned: rules.clean(message),
        reply,
        rules,
        push: None,
    };
    let (window, _) = cx.update(|cx| {
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                gpui_kit::Point::default(),
                size(px(800.), px(600.)),
            ))),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            cx.new(|cx| CommitReview::new(pending, window, cx))
        })
        .expect("window")
    });
    cx.run_until_parked();
    (window, answer)
}

fn text(reply: &Reply) -> &str {
    match reply.content.first() {
        Some(Content::Text(text)) => text,
        _ => "",
    }
}

fn click(cx: &mut TestAppContext, window: AnyWindowHandle, id: &'static str) {
    cx.update_window(window, |_, window, cx| window.click(id, cx))
        .expect("window");
    cx.run_until_parked();
}

#[gpui_kit::test]
async fn the_user_commits_the_files_asked_for(cx: &mut TestAppContext) {
    let repo = repository();
    let message = "Add b\n\nCo-Authored-By: Claude <noreply@anthropic.com>";
    let (window, answer) = review(cx, repo.path(), message, &["docs"]);
    click(cx, window, "commit-review-commit");
    cx.wait_for(window, std::time::Duration::from_secs(5), |_, _| {
        !answer.is_empty()
    })
    .await;
    let reply = answer.try_recv().expect("answered");
    assert!(!reply.error, "{}", text(&reply));
    assert!(
        text(&reply).contains("with this message:\n\nAdd b"),
        "{}",
        text(&reply)
    );
    assert_eq!(git(repo.path(), &["log", "-1", "--format=%B"]), "Add b");
    assert_eq!(git(repo.path(), &["status", "--porcelain"]), "M a.txt");
}

#[gpui_kit::test]
fn a_rejected_commit_tells_the_agent(cx: &mut TestAppContext) {
    let repo = repository();
    let (window, answer) = review(cx, repo.path(), "Change a", &[]);
    click(cx, window, "commit-review-reject");
    let reply = answer.try_recv().expect("answered");
    assert!(reply.error && text(&reply).contains("rejected"));
    assert_eq!(git(repo.path(), &["rev-list", "--count", "HEAD"]), "1");
}
