//! A snapshot of the process table, read as trees: a session's CLI and the
//! processes started under it, such as a shell the agent left running.

use std::{
    collections::HashMap,
    path::PathBuf,
    time::{Duration, SystemTime},
};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, Signal, System, UpdateKind};

/// One process of a tree, as last refreshed.
#[derive(Clone, Debug, PartialEq)]
pub struct ProcessInfo {
    pub pid: u32,
    /// How far below the tree's root it is; the root is 0.
    pub depth: usize,
    /// Its command line, or its name when the command line is unreadable.
    pub command: String,
    /// Percent of one core over the time since the last refresh, so more
    /// than 100 when it uses several.
    pub cpu: f32,
    /// Resident memory, in bytes.
    pub memory: u64,
    pub parent: Option<u32>,
    /// Running, sleeping, stopped…, as the system names it.
    pub status: String,
    pub started_at: SystemTime,
    pub cwd: Option<PathBuf>,
    pub exe: Option<PathBuf>,
}

pub struct Monitor {
    system: System,
}

impl Monitor {
    /// `None` when the platform offers no process table.
    #[must_use]
    pub fn new() -> Option<Self> {
        System::new().ok().map(|system| Self { system })
    }

    /// Reads the process table again. CPU use is measured between two
    /// refreshes, so the first one reports none.
    pub fn refresh(&mut self) {
        let kind = ProcessRefreshKind::nothing()
            .with_memory()
            .with_cpu()
            .with_cmd(UpdateKind::OnlyIfNotSet)
            .with_exe(UpdateKind::OnlyIfNotSet)
            .with_cwd(UpdateKind::OnlyIfNotSet)
            // On Linux threads are listed as processes of their own.
            .without_tasks();
        self.system
            .refresh_processes_specifics(ProcessesToUpdate::All, true, kind);
    }

    /// Process `root` and every process under it, each after its parent and
    /// children in the order they started; empty if `root` is gone.
    #[must_use]
    pub fn tree(&self, root: u32) -> Vec<ProcessInfo> {
        let parents: Vec<(u32, Option<u32>)> = self
            .system
            .processes()
            .values()
            .map(|process| (process.pid().as_u32(), process.parent().map(Pid::as_u32)))
            .collect();
        let Some(_) = self.system.process(Pid::from_u32(root)) else {
            return Vec::new();
        };
        descendants(root, &parents)
            .into_iter()
            .filter_map(|(pid, depth)| self.info(pid, depth))
            .collect()
    }

    /// Asks `pid` and every process under it to end, the deepest first, so
    /// none is left behind reparented; whether any was asked.
    #[must_use]
    pub fn stop(&self, pid: u32) -> bool {
        let mut tree = self.tree(pid);
        tree.reverse();
        // Every one is asked, so no short-circuiting `any`.
        tree.iter()
            .filter_map(|process| self.system.process(Pid::from_u32(process.pid)))
            .filter(|process| process.kill_with(Signal::Term).unwrap_or(false))
            .count()
            > 0
    }

    fn info(&self, pid: u32, depth: usize) -> Option<ProcessInfo> {
        let process = self.system.process(Pid::from_u32(pid))?;
        let command = process
            .cmd()
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        let command = if command.is_empty() {
            process.name().to_string_lossy().into_owned()
        } else {
            command
        };
        Some(ProcessInfo {
            pid,
            depth,
            command,
            cpu: process.cpu_usage(),
            memory: process.memory(),
            parent: process.parent().map(Pid::as_u32),
            status: process.status().to_string(),
            started_at: SystemTime::UNIX_EPOCH + Duration::from_secs(process.start_time()),
            cwd: process.cwd().map(PathBuf::from),
            exe: process.exe().map(PathBuf::from),
        })
    }
}

/// `root` and the processes under it with their depth, depth first, given
/// each process's parent.
fn descendants(root: u32, parents: &[(u32, Option<u32>)]) -> Vec<(u32, usize)> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for &(pid, parent) in parents {
        if let Some(parent) = parent.filter(|parent| *parent != pid) {
            children.entry(parent).or_default().push(pid);
        }
    }
    for list in children.values_mut() {
        list.sort_unstable();
    }
    let mut tree = Vec::new();
    let mut stack = vec![(root, 0)];
    while let Some((pid, depth)) = stack.pop() {
        // A cycle in a racy snapshot must not loop forever.
        if tree.iter().any(|(seen, _)| *seen == pid) {
            continue;
        }
        tree.push((pid, depth));
        if let Some(list) = children.get(&pid) {
            stack.extend(list.iter().rev().map(|child| (*child, depth + 1)));
        }
    }
    tree
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tree_lists_each_process_under_its_parent() {
        let parents = [
            (1, None),
            (10, Some(1)),
            (12, Some(10)),
            (11, Some(10)),
            (20, Some(1)),
            (13, Some(11)),
        ];
        assert_eq!(
            descendants(10, &parents),
            [(10, 0), (11, 1), (13, 2), (12, 1)]
        );
        assert_eq!(descendants(99, &parents), [(99, 0)]);
    }

    #[test]
    fn a_cycle_in_the_snapshot_ends() {
        let parents = [(1, Some(2)), (2, Some(1))];
        assert_eq!(descendants(1, &parents), [(1, 0), (2, 1)]);
    }

    #[test]
    fn a_child_shows_under_this_process_until_stopped() {
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let mut monitor = Monitor::new().unwrap();
        monitor.refresh();
        let tree = monitor.tree(std::process::id());
        let shown = tree
            .iter()
            .find(|process| process.pid == child.id())
            .unwrap();
        assert_eq!((shown.depth, shown.command.as_str()), (1, "sleep 30"));
        assert!(tree[0].memory > 0, "{tree:?}");
        assert_eq!(shown.parent, Some(std::process::id()));
        assert!(shown.started_at <= SystemTime::now());
        assert!(monitor.stop(child.id()));
        let status = child.wait().unwrap();
        assert!(!status.success(), "ended by the signal");
    }
}
