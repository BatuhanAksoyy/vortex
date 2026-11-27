//! Finding text in the project's files.

use std::{
    io::Read as _,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

/// Larger files are mostly data, and slow to look through.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// A NUL this early means the file is not text.
const SNIFF_BYTES: usize = 8 * 1024;
/// Enough to go through by eye; the rest are counted as left out.
pub const MAX_HITS: usize = 2000;
/// A minified line can be very long; the part around the match is enough.
const MAX_SHOWN: usize = 200;
const BEFORE_MATCH: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// As listed, relative to the project.
    pub path: PathBuf,
    /// From 0.
    pub line: u32,
    /// The line, or the part of it around the match.
    pub text: String,
    /// Where the match is in `text`, in bytes.
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    pub hits: Vec<Hit>,
    /// More matched than `MAX_HITS`.
    pub truncated: bool,
}

/// Every line of `files` under `root` that holds `query`; stops early when
/// `cancelled` is set.
pub fn search(
    root: &Path,
    files: &[PathBuf],
    query: &str,
    match_case: bool,
    cancelled: &AtomicBool,
) -> Found {
    let mut found = Found::default();
    if query.is_empty() {
        return found;
    }
    let needle = if match_case {
        query.to_owned()
    } else {
        query.to_lowercase()
    };
    for path in files {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let Some(text) = read_text(&root.join(path)) else {
            continue;
        };
        for (line, content) in (0..).zip(text.lines()) {
            let Some((start, end)) = find(content, &needle, match_case) else {
                continue;
            };
            if found.hits.len() == MAX_HITS {
                found.truncated = true;
                return found;
            }
            found.hits.push(hit(path, line, content, start, end));
        }
    }
    found
}

fn read_text(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES).read_to_end(&mut bytes).ok()?;
    if bytes.iter().take(SNIFF_BYTES).any(|byte| *byte == 0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Where `needle` is in `line`, in bytes of `line`.
fn find(line: &str, needle: &str, match_case: bool) -> Option<(usize, usize)> {
    if match_case {
        let start = line.find(needle)?;
        return Some((start, start + needle.len()));
    }
    // Lowercasing can change a character's length, so the match is mapped
    // back through the characters it came from.
    let mut lowered = String::with_capacity(line.len());
    let mut origins = Vec::with_capacity(line.len());
    for (at, character) in line.char_indices() {
        for low in character.to_lowercase() {
            lowered.push(low);
            origins.extend(std::iter::repeat_n(at, low.len_utf8()));
        }
    }
    let start = lowered.find(needle)?;
    let end = start + needle.len();
    let from = origins[start];
    let to = origins.get(end).copied().unwrap_or(line.len());
    Some((from, to.max(from + 1).min(line.len())))
}

fn hit(path: &Path, line: u32, content: &str, start: usize, end: usize) -> Hit {
    let from = if content.len() > MAX_SHOWN {
        boundary(content, start.saturating_sub(BEFORE_MATCH))
    } else {
        0
    };
    let to = boundary(content, (from + MAX_SHOWN).max(end)).max(end);
    let shown = &content[from..to];
    let trimmed = shown.trim_start();
    let cut = from + (shown.len() - trimmed.len());
    Hit {
        path: path.to_path_buf(),
        line,
        text: trimmed.trim_end().to_owned(),
        start: start.saturating_sub(cut),
        end: end.saturating_sub(cut).min(trimmed.trim_end().len()),
    }
}

/// The nearest character boundary at or before `at`.
fn boundary(text: &str, at: usize) -> usize {
    let mut at = at.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_with_the_text_are_found_in_text_files_only() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.rs"),
            "fn main() {\n    Vortex::start();\n}\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("b.bin"), b"vortex\0").unwrap();
        let files = [PathBuf::from("a.rs"), PathBuf::from("b.bin")];
        let never = AtomicBool::new(false);
        let found = search(dir.path(), &files, "vortex", false, &never);
        assert_eq!(found.hits.len(), 1);
        let hit = &found.hits[0];
        assert_eq!((hit.line, hit.text.as_str()), (1, "Vortex::start();"));
        assert_eq!(&hit.text[hit.start..hit.end], "Vortex");
        let exact = search(dir.path(), &files, "vortex", true, &never);
        assert_eq!(exact.hits, []);
    }

    #[test]
    fn case_is_ignored_across_letters_that_change_length() {
        assert_eq!(find("İstanbul ışık", "ışık", false), Some((10, 17)));
        assert_eq!(find("aİb", "b", false), Some((3, 4)));
    }

    #[test]
    fn a_long_line_is_shown_around_the_match() {
        let line = format!("{}needle{}", "x".repeat(500), "y".repeat(500));
        let hit = hit(Path::new("f"), 0, &line, 500, 506);
        assert!(hit.text.len() <= MAX_SHOWN + 10);
        assert_eq!(&hit.text[hit.start..hit.end], "needle");
    }
}
