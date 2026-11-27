//! Putting a downloaded app in place of the running one. macOS lets an app's
//! bundle be replaced while it runs; the new one takes over once it starts.

use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context as _, bail};

/// The archive is always unpacked to this name; the release workflow
/// packs it so.
const APP: &str = "Kiln.app";

/// The app bundle the running Kiln is, if it runs from one rather than
/// from a build folder.
#[must_use]
pub fn running_bundle() -> Option<PathBuf> {
    let program = std::env::current_exe().ok()?.canonicalize().ok()?;
    let mut folders = program.ancestors();
    let bundle = folders.find(|folder| folder.extension().is_some_and(|it| it == "app"))?;
    Some(bundle.to_owned())
}

/// Downloads the archive at `url` and puts the app in it in place of
/// `bundle`. Until the new app is in place the old one is kept, and it
/// comes back if moving the new one in fails.
pub fn install(url: &str, bundle: &Path) -> anyhow::Result<()> {
    let Some(folder) = bundle.parent() else {
        bail!("{} is not in a folder", bundle.display());
    };
    // Beside the app, so moving it in is a rename on the same disk.
    let work = folder.join(format!(".kiln-update-{}", std::process::id()));
    let installed = unpack_and_swap(url, bundle, &work);
    let _ = std::fs::remove_dir_all(&work);
    installed
}

fn unpack_and_swap(url: &str, bundle: &Path, work: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(work).with_context(|| format!("cannot create {}", work.display()))?;
    let archive = work.join("update.zip");
    run(
        Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--location"])
            .arg("--output")
            .arg(&archive)
            .arg(url),
        "download the update",
    )?;
    let unpacked = work.join("unpacked");
    run(
        Command::new("ditto")
            .args(["-x", "-k"])
            .arg(&archive)
            .arg(&unpacked),
        "unpack the update",
    )?;
    let new = unpacked.join(APP);
    if !new.join("Contents").is_dir() {
        bail!("the update holds no {APP}");
    }
    let old = work.join("old.app");
    std::fs::rename(bundle, &old)
        .with_context(|| format!("cannot move {} aside", bundle.display()))?;
    if let Err(error) = std::fs::rename(&new, bundle) {
        let _ = std::fs::rename(&old, bundle);
        return Err(error)
            .with_context(|| format!("cannot put the update at {}", bundle.display()));
    }
    Ok(())
}

fn run(command: &mut Command, doing: &str) -> anyhow::Result<()> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("cannot {doing}"))?;
    if !output.status.success() {
        bail!(
            "cannot {doing}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

/// Starts the app at `bundle` as a new process; the running one is to quit
/// after.
pub fn relaunch(bundle: &Path) -> anyhow::Result<()> {
    Command::new("open")
        .arg("-n")
        .arg(bundle)
        .stdin(Stdio::null())
        .spawn()
        .context("cannot start the updated Kiln")?;
    Ok(())
}

// `ditto` packs and unpacks the archives.
#[cfg(test)]
#[cfg(target_os = "macos")]
mod tests {
    use std::{path::Path, process::Command};

    use super::install;

    fn app(at: &Path, program: &str) {
        let folder = at.join("Contents/MacOS");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("kiln"), program).unwrap();
    }

    #[test]
    fn the_downloaded_app_takes_the_running_ones_place() {
        let dir = tempfile::tempdir().unwrap();
        let built = dir.path().join("built");
        app(&built.join("Kiln.app"), "new");
        let archive = dir.path().join("Kiln.zip");
        let packed = Command::new("ditto")
            .args(["-c", "-k", "--keepParent"])
            .arg(built.join("Kiln.app"))
            .arg(&archive)
            .status()
            .unwrap();
        assert!(packed.success());
        let applications = dir.path().join("Applications");
        let bundle = applications.join("Kiln.app");
        app(&bundle, "old");

        let url = format!("file://{}", archive.display());
        install(&url, &bundle).unwrap();
        let program = std::fs::read_to_string(bundle.join("Contents/MacOS/kiln")).unwrap();
        assert_eq!(program, "new");
        let left: Vec<_> = std::fs::read_dir(&applications).unwrap().collect();
        assert_eq!(left.len(), 1, "nothing is left beside it");

        let missing = format!("file://{}", dir.path().join("none.zip").display());
        assert!(install(&missing, &bundle).is_err());
        let program = std::fs::read_to_string(bundle.join("Contents/MacOS/kiln")).unwrap();
        assert_eq!(program, "new", "a failed update leaves the app as it was");
    }
}
