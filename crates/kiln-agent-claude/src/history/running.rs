//! Which stored conversations a CLI process has open right now. While it
//! runs, each process keeps `<config>/sessions/<pid>.json` naming its
//! conversation; another writer would interleave messages in the same file.

use std::{
    collections::HashSet,
    path::Path,
    process::{Command, Stdio},
};

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Running {
    pid: u32,
    session_id: String,
}

/// The ids of conversations open in a live process other than `own`.
pub fn open_elsewhere(config: &Path, own: &HashSet<u32>) -> HashSet<String> {
    let Ok(entries) = std::fs::read_dir(config.join("sessions")) else {
        return HashSet::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .filter_map(|text| serde_json::from_str::<Running>(&text).ok())
        .filter(|running| !own.contains(&running.pid) && alive(running.pid))
        .map(|running| running.session_id)
        .collect()
}

/// A process that crashed leaves its record behind. `kill -0` asks whether
/// the pid is live without signalling it, and keeps `unsafe` out of here.
fn alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}
