//! Pushing a project's branch as `git.push` allows: by hand from the Git
//! page, and on Kiln's own after a commit or as a chat ends when the policy
//! asks for it. A protected branch is pushed only once the user confirms,
//! so never on Kiln's own.

use std::path::{Path, PathBuf};

use gpui_kit::{App, AppContext as _};
use kiln_settings::{PushPolicy, PushSettings};

pub(crate) fn settings(cx: &App) -> &PushSettings {
    &crate::config::settings(cx).git.push
}

pub(crate) fn is_protected(branch: &str, cx: &App) -> bool {
    settings(cx).protected.iter().any(|name| name == branch)
}

/// The settings to push with when the policy is `when`.
pub(crate) fn wanted(when: PushPolicy, cx: &App) -> Option<PushSettings> {
    let push = settings(cx);
    (push.policy == when).then(|| push.clone())
}

/// Pushes the branch `project` is on, unless it is protected or has
/// nothing to push. Blocks; run it off the main thread.
///
/// # Errors
/// When `git` cannot read the repository or the push fails.
pub(crate) fn push_unattended(project: &Path, push: &PushSettings) -> anyhow::Result<()> {
    let status = kiln_git::status(project)?;
    let Some(branch) = status.branch.as_deref() else {
        return Ok(());
    };
    let tracked = status.upstream.is_some();
    if push.protected.iter().any(|name| name == branch) || (tracked && status.ahead == 0) {
        return Ok(());
    }
    kiln_git::push(
        &status.root,
        branch,
        status.upstream.as_deref(),
        &push.remote,
    )
}

/// Pushes in the background when the policy is `when`.
pub(crate) fn push_on(when: PushPolicy, project: PathBuf, cx: &App) {
    let Some(push) = wanted(when, cx) else {
        return;
    };
    cx.background_spawn(async move {
        if let Err(error) = push_unattended(&project, &push) {
            log::warn!("cannot push {}: {error:#}", project.display());
        }
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use std::{path::Path, process::Command};

    use kiln_settings::{PushPolicy, PushSettings};

    use super::push_unattended;

    fn git(root: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .args(args)
            .current_dir(root)
            .output()
            .expect("git");
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    #[test]
    fn kiln_pushes_on_its_own_only_to_unprotected_branches() {
        let (repo, remote) = (
            tempfile::tempdir().expect("dir"),
            tempfile::tempdir().expect("dir"),
        );
        let root = repo.path();
        git(remote.path(), &["init", "-q", "--bare"]);
        git(root, &["init", "-q", "-b", "main"]);
        git(
            root,
            &[
                "-c",
                "user.name=K",
                "-c",
                "user.email=k@example.com",
                "commit",
                "-q",
                "--allow-empty",
                "-m",
                "start",
            ],
        );
        git(
            root,
            &["remote", "add", "origin", &remote.path().to_string_lossy()],
        );
        let push = PushSettings {
            policy: PushPolicy::AfterCommit,
            protected: vec!["main".into()],
            remote: "origin".into(),
        };
        push_unattended(root, &push).expect("skipped");
        assert_eq!(
            git(remote.path(), &["branch", "--list"]),
            "",
            "main is protected"
        );
        git(root, &["switch", "-q", "-c", "work"]);
        push_unattended(root, &push).expect("pushed");
        assert_eq!(git(remote.path(), &["branch", "--list"]), "work");
    }
}
