//! Repository rules that Clippy cannot express.

use std::{collections::BTreeMap, fmt::Write as _, path::Path};

use anyhow::{Context as _, bail};

use crate::{attributes, complexity, workspace};

pub const MAX_CYCLOMATIC_COMPLEXITY: usize = 10;
pub const MAX_FILE_LINES: usize = 400;
pub const EXPECT_BASELINE: &str = "quality/expect-baseline.txt";
const UNSAFE_CRATE: &str = "crates/kiln-sys/";

pub fn check(root: &Path) -> anyhow::Result<()> {
    let mut violations = Vec::new();
    violations.extend(check_manifests(root)?);
    for path in workspace::rust_files(root) {
        let source = std::fs::read_to_string(&path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        violations.extend(check_source(&workspace::relative(root, &path), &source));
    }
    violations.extend(check_expect_baseline(root)?);
    if violations.is_empty() {
        return Ok(());
    }
    bail!(
        "{} violation(s):\n  {}",
        violations.len(),
        violations.join("\n  ")
    )
}

/// Rules for one source file. `path` is repository-relative.
pub fn check_source(path: &str, source: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let lines = source.lines().count();
    if lines > MAX_FILE_LINES {
        violations.push(format!(
            "{path}: {lines} lines (max {MAX_FILE_LINES}); split the module"
        ));
    }
    let file = match syn::parse_file(source) {
        Ok(file) => file,
        Err(error) => {
            violations.push(format!("{path}: cannot parse: {error}"));
            return violations;
        }
    };
    let facts = attributes::collect(&file);
    let at = |lines: &[usize], what: &str| -> Vec<String> {
        lines
            .iter()
            .map(|line| format!("{path}:{line}: {what}"))
            .collect()
    };
    violations.extend(at(
        &facts.inner_suppressions,
        "crate/module-level lint suppression",
    ));
    violations.extend(at(&facts.bare_ignores, "`#[ignore]` needs `= \"reason\"`"));
    if !path.starts_with(UNSAFE_CRATE) {
        violations.extend(at(
            &facts.unsafe_suppressions,
            "unsafe code outside crates/kiln-sys",
        ));
    }
    violations.extend(
        complexity::analyze(&file)
            .into_iter()
            .filter(|f| f.complexity > MAX_CYCLOMATIC_COMPLEXITY)
            .map(|f| {
                format!(
                    "{path}:{}: `{}` has cyclomatic complexity {} (max {MAX_CYCLOMATIC_COMPLEXITY})",
                    f.line, f.name, f.complexity
                )
            }),
    );
    violations
}

fn check_manifests(root: &Path) -> anyhow::Result<Vec<String>> {
    let mut violations = Vec::new();
    for manifest in workspace::files_with_extension(root, "toml") {
        if manifest.file_name().is_none_or(|name| name != "Cargo.toml")
            || manifest == root.join("Cargo.toml")
        {
            continue;
        }
        let text = std::fs::read_to_string(&manifest)?;
        let value: toml::Table = toml::from_str(&text)
            .with_context(|| format!("cannot parse {}", manifest.display()))?;
        let inherits = value
            .get("lints")
            .and_then(|lints| lints.get("workspace"))
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);
        if !inherits {
            let path = workspace::relative(root, &manifest);
            violations.push(format!("{path}: must contain `[lints] workspace = true`"));
        }
    }
    Ok(violations)
}

/// Counts `#[expect]` attributes per file. A file may not exceed its baseline; every
/// increase must come with a reviewed baseline update (`cargo xtask lock-config`).
pub fn expect_counts(root: &Path) -> anyhow::Result<BTreeMap<String, usize>> {
    let mut counts = BTreeMap::new();
    for path in workspace::rust_files(root) {
        let source = std::fs::read_to_string(&path)?;
        let Ok(file) = syn::parse_file(&source) else {
            continue;
        };
        let count = attributes::collect(&file).expects;
        if count > 0 {
            counts.insert(workspace::relative(root, &path), count);
        }
    }
    Ok(counts)
}

pub fn render_baseline(counts: &BTreeMap<String, usize>) -> String {
    let mut out = String::new();
    for (path, count) in counts {
        let _ = writeln!(out, "{count} {path}");
    }
    out
}

fn check_expect_baseline(root: &Path) -> anyhow::Result<Vec<String>> {
    let baseline_text = std::fs::read_to_string(root.join(EXPECT_BASELINE)).unwrap_or_default();
    let baseline: BTreeMap<&str, usize> = baseline_text
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter_map(|(count, path)| Some((path, count.parse().ok()?)))
        .collect();
    Ok(expect_counts(root)?
        .into_iter()
        .filter(|(path, count)| *count > baseline.get(path.as_str()).copied().unwrap_or(0))
        .map(|(path, count)| {
            format!(
                "{path}: {count} `#[expect]` above baseline; fix the code instead of suppressing"
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::check_source;

    #[test]
    fn flags_crate_level_suppression_and_bare_ignore() {
        let violations = check_source(
            "crates/a/src/lib.rs",
            "#![allow(dead_code)]\n#[ignore]\nfn t() {}\n",
        );
        assert_eq!(violations.len(), 2, "{violations:?}");
    }

    #[test]
    fn unsafe_expectation_is_only_allowed_in_sys_crate() {
        let line = "#[expect(unsafe_code, reason = \"ffi\")]\nfn f() {}\n";
        assert_eq!(
            check_source("crates/kiln-sys/src/lib.rs", line),
            Vec::<String>::new()
        );
        assert_eq!(check_source("crates/kiln/src/main.rs", line).len(), 1);
    }

    #[test]
    fn flags_complex_functions() {
        let branches = "if a {} ".repeat(super::MAX_CYCLOMATIC_COMPLEXITY);
        let source = format!("fn f(a: bool) {{ {branches} }}");
        let violations = check_source("x.rs", &source);
        assert_eq!(violations.len(), 1);
        assert!(violations[0].contains("cyclomatic complexity 11"));
    }

    #[test]
    fn flags_long_files() {
        let source = "// line\n".repeat(super::MAX_FILE_LINES + 1);
        assert_eq!(check_source("x.rs", &source).len(), 1);
    }
}
