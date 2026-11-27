//! Fixture access shared by the replay tests.

use std::path::{Path, PathBuf};

pub fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(format!("{name}.jsonl"))
}

pub fn output_lines(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(fixture_path(name)).unwrap();
    text.lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|record| record["dir"] == "out")
        .map(|record| record["msg"].to_string())
        .collect()
}
