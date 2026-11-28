//! What changed since the Vortex that last ran with the user's data, from
//! the changelog built into this one: the text the release notes on GitHub
//! carry too, at hand without asking GitHub.

use std::path::Path;

use anyhow::Context as _;

use crate::{CURRENT, Version};

const CHANGELOG: &str = include_str!("../../../CHANGELOG.md");
/// Beside the user's data: the newest Vortex that ran with it, so each
/// release is introduced once.
const SEEN_FILE: &str = "last-version";

/// The notes of each release after the newest Vortex that ran with
/// `data_dir`, up to the running one, as Markdown; `None` when there is
/// none. With no Vortex known to have run, `ran_before` says whether one
/// did, before the version was kept: it is shown the running release's
/// notes, and a first run none. The running version is kept for the next.
///
/// # Errors
/// When the version cannot be kept, so that the notes are not shown on
/// every start.
pub fn since_last_run(data_dir: &Path, ran_before: bool) -> anyhow::Result<Option<String>> {
    let Some(current) = Version::parse(CURRENT) else {
        return Ok(None);
    };
    let path = data_dir.join(SEEN_FILE);
    let seen = std::fs::read_to_string(&path).ok();
    let seen = seen.as_deref().and_then(Version::parse);
    if seen.is_none_or(|seen| seen < current) {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("cannot create {}", data_dir.display()))?;
        std::fs::write(&path, current.to_string())
            .with_context(|| format!("cannot write {}", path.display()))?;
    }
    let shown = match seen {
        Some(seen) => notes(CHANGELOG, |version| seen < version && version <= current),
        None if ran_before => notes(CHANGELOG, |version| version == current),
        None => None,
    };
    Ok(shown)
}

/// The sections of `changelog` whose release is `wanted`, newest first as it
/// lists them. A lone one is given without its heading, which the dialog's
/// title says; several keep theirs to tell them apart.
fn notes(changelog: &str, wanted: impl Fn(Version) -> bool) -> Option<String> {
    let sections: Vec<(&str, &str)> = sections(changelog)
        .filter(|(heading, _)| release(heading).is_some_and(&wanted))
        .collect();
    match sections.as_slice() {
        [] => None,
        [(_, body)] => Some((*body).to_owned()),
        several => Some(
            several
                .iter()
                .map(|(heading, body)| format!("## {heading}\n\n{body}"))
                .collect::<Vec<_>>()
                .join("\n\n"),
        ),
    }
}

/// Each `## ` heading's text, e.g. "1.1.0 — 2026-11-27", and what is under
/// it, trimmed.
fn sections(changelog: &str) -> impl Iterator<Item = (&str, &str)> {
    let starts = changelog.match_indices("\n## ").map(|(at, _)| at + 1);
    let starts: Vec<usize> = changelog
        .starts_with("## ")
        .then_some(0)
        .into_iter()
        .chain(starts)
        .collect();
    let ends: Vec<usize> = (starts.iter().skip(1).copied())
        .chain([changelog.len()])
        .collect();
    starts.into_iter().zip(ends).map(move |(start, end)| {
        let section = &changelog[start + "## ".len()..end];
        let (heading, body) = section.split_once('\n').unwrap_or((section, ""));
        (heading.trim(), body.trim())
    })
}

/// The release a heading names, its first word.
fn release(heading: &str) -> Option<Version> {
    Version::parse(heading.split_whitespace().next()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "# Changelog\n\n\
        ## 1.2.0 — 2026-12-01\n\n### Added\n\n- Two\n\n\
        ## 1.1.0 — 2026-11-27\n\n### Fixed\n\n- One\n\n\
        ## 1.0.0 — 2026-11-20\n\nFirst.\n";

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn a_lone_release_comes_without_its_heading_and_several_with_theirs() {
        let one = notes(LOG, |it| it == version("1.1.0"));
        assert_eq!(one.as_deref(), Some("### Fixed\n\n- One"));

        let after = version("1.0.0");
        let several = notes(LOG, |it| after < it).unwrap();
        assert_eq!(
            several,
            "## 1.2.0 — 2026-12-01\n\n### Added\n\n- Two\n\n\
             ## 1.1.0 — 2026-11-27\n\n### Fixed\n\n- One"
        );
        assert_eq!(notes(LOG, |it| it == version("9.0.0")), None);
    }

    #[test]
    fn the_running_release_is_introduced_once() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        assert_eq!(since_last_run(&data, false).unwrap(), None, "a first run");
        assert_eq!(since_last_run(&data, true).unwrap(), None, "kept by then");

        let before = Version([0, 0, 1]);
        std::fs::write(data.join(SEEN_FILE), before.to_string()).unwrap();
        let shown = since_last_run(&data, true).unwrap();
        assert!(shown.is_some_and(|notes| !notes.is_empty()));
        assert_eq!(since_last_run(&data, true).unwrap(), None, "shown once");
    }

    #[test]
    fn a_vortex_that_ran_before_the_version_was_kept_hears_of_this_one() {
        let dir = tempfile::tempdir().unwrap();
        let shown = since_last_run(dir.path(), true).unwrap();
        let current = notes(CHANGELOG, |it| it == version(CURRENT));
        assert_eq!(shown, current);
    }

    #[test]
    fn a_newer_vortex_that_ran_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SEEN_FILE), "99.0.0").unwrap();
        assert_eq!(since_last_run(dir.path(), true).unwrap(), None);
        let kept = std::fs::read_to_string(dir.path().join(SEEN_FILE)).unwrap();
        assert_eq!(kept, "99.0.0", "an older one does not bring its notes back");
    }
}
