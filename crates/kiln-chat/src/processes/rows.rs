//! The rows a processes page shows: the processes in scope, matched against
//! the search and sorted as the user chose.

use std::{cmp::Ordering, time::Duration};

use gpui_kit::SharedString;
use kiln_services::ProcessInfo;

use super::AgentProcesses;

/// One process, with the chat whose agent it runs under.
#[derive(Clone, Copy)]
pub struct Row<'a> {
    /// The agent's process, which tells chats of the same name apart.
    pub agent: u32,
    pub chat: &'a SharedString,
    pub process: &'a ProcessInfo,
}

impl Row<'_> {
    /// Whether it is the agent itself, which its chat's tab stops.
    pub fn is_agent(&self) -> bool {
        self.process.depth == 0
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Sort {
    /// Each process under the one that started it.
    #[default]
    Tree,
    Pid,
    Cpu,
    Memory,
}

/// A column to sort by, and which way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Order {
    pub sort: Sort,
    pub descending: bool,
}

impl Order {
    /// The order after a click on `sort`'s header. A column starts the
    /// way it is most often read, busiest and largest first, PIDs from the
    /// lowest; a second click turns it around, and a third goes back to
    /// the tree.
    #[must_use]
    pub fn then_by(self, sort: Sort) -> Self {
        let first = Self {
            sort,
            descending: matches!(sort, Sort::Cpu | Sort::Memory),
        };
        if sort != self.sort || sort == Sort::Tree {
            return first;
        }
        if self == first {
            Self {
                sort,
                descending: !first.descending,
            }
        } else {
            Self::default()
        }
    }
}

/// The processes of `agents` matching `query`, by command, PID or chat,
/// in `order`.
pub fn rows<'a>(agents: &[&'a AgentProcesses], query: &str, order: Order) -> Vec<Row<'a>> {
    let query = query.trim().to_lowercase();
    let mut rows: Vec<Row<'a>> = agents
        .iter()
        .flat_map(|agent| {
            let root = agent.pid().unwrap_or_default();
            agent.processes.iter().map(move |process| Row {
                agent: root,
                chat: &agent.title,
                process,
            })
        })
        .filter(|row| query.is_empty() || matches(*row, &query))
        .collect();
    let ascending = |a: &Row, b: &Row| match order.sort {
        Sort::Tree => Ordering::Equal,
        Sort::Pid => a.process.pid.cmp(&b.process.pid),
        Sort::Cpu => a.process.cpu.total_cmp(&b.process.cpu),
        Sort::Memory => a.process.memory.cmp(&b.process.memory),
    };
    if order.descending {
        rows.sort_by(|a, b| ascending(b, a));
    } else {
        rows.sort_by(ascending);
    }
    rows
}

fn matches(row: Row, query: &str) -> bool {
    row.process.command.to_lowercase().contains(query)
        || row.process.pid.to_string().contains(query)
        || row.chat.to_lowercase().contains(query)
}

/// CPU and memory of `rows` together.
pub fn totals(rows: &[Row]) -> (f32, u64) {
    rows.iter().fold((0.0, 0), |(cpu, memory), row| {
        (cpu + row.process.cpu, memory + row.process.memory)
    })
}

/// Percent of one core, so a process using two reads 200%.
#[must_use]
pub fn cpu(percent: f32) -> String {
    format!("{percent:.0}%")
}

#[must_use]
pub fn memory(bytes: u64) -> String {
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let (mut scale, mut unit) = (1024, 0);
    while unit < UNITS.len() - 1 && bytes >= scale * 1024 {
        scale *= 1024;
        unit += 1;
    }
    // Tenths of the unit, rounded, without going through floats.
    let tenths = (bytes * 10 + scale / 2) / scale;
    format!("{}.{} {}", tenths / 10, tenths % 10, UNITS[unit])
}

/// `45s`, `12m 5s`, `3h 20m`, `2d 4h`: the two largest units.
pub fn duration(elapsed: Duration) -> String {
    let seconds = elapsed.as_secs();
    let (days, hours, minutes) = (seconds / 86_400, seconds / 3600 % 24, seconds / 60 % 60);
    match (days, seconds / 3600, seconds / 60) {
        (1.., _, _) => format!("{days}d {hours}h"),
        (_, 1.., _) => format!("{}h {minutes}m", seconds / 3600),
        (_, _, 1..) => format!("{minutes}m {}s", seconds % 60),
        _ => format!("{seconds}s"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;

    fn process(pid: u32, depth: usize, command: &str, cpu: f32, memory: u64) -> ProcessInfo {
        ProcessInfo {
            pid,
            depth,
            command: command.into(),
            cpu,
            memory,
            parent: None,
            status: "Run".into(),
            started_at: SystemTime::UNIX_EPOCH,
            cwd: None,
            exe: None,
        }
    }

    fn agents() -> [AgentProcesses; 2] {
        [
            AgentProcesses {
                title: "Fix the build".into(),
                processes: vec![
                    process(10, 0, "claude", 2.0, 300),
                    process(12, 1, "npm run dev", 40.0, 900),
                ],
            },
            AgentProcesses {
                title: "Plan the release".into(),
                processes: vec![process(20, 0, "claude", 1.0, 200)],
            },
        ]
    }

    fn pids(rows: &[Row]) -> Vec<u32> {
        rows.iter().map(|row| row.process.pid).collect()
    }

    #[test]
    fn rows_keep_the_tree_until_sorted() {
        let agents = agents();
        let agents: Vec<_> = agents.iter().collect();
        let tree = Order::default();
        assert_eq!(pids(&rows(&agents, "", tree)), [10, 12, 20]);
        let cpu = tree.then_by(Sort::Cpu);
        assert_eq!(pids(&rows(&agents, "", cpu)), [12, 10, 20], "busiest first");
        let memory = cpu.then_by(Sort::Memory);
        assert_eq!(pids(&rows(&agents, "", memory)), [12, 10, 20]);
        let pid = memory.then_by(Sort::Pid);
        assert_eq!(pids(&rows(&agents, "", pid)), [10, 12, 20], "lowest first");
    }

    #[test]
    fn a_second_click_on_a_column_turns_it_around() {
        let agents = agents();
        let agents: Vec<_> = agents.iter().collect();
        let memory = Order::default().then_by(Sort::Memory);
        let smallest = memory.then_by(Sort::Memory);
        assert_eq!(pids(&rows(&agents, "", smallest)), [20, 10, 12]);
        assert_eq!(
            smallest.then_by(Sort::Memory),
            Order::default(),
            "a third click goes back to the tree"
        );
        let highest = Order::default().then_by(Sort::Pid).then_by(Sort::Pid);
        assert!(highest.descending);
        assert_eq!(highest.then_by(Sort::Pid), Order::default());
        assert_eq!(
            smallest.then_by(Sort::Tree).then_by(Sort::Tree),
            Order::default()
        );
    }

    #[test]
    fn the_search_matches_a_command_a_pid_or_a_chat() {
        let agents = agents();
        let agents: Vec<_> = agents.iter().collect();
        assert_eq!(pids(&rows(&agents, " NPM ", Order::default())), [12]);
        assert_eq!(pids(&rows(&agents, "20", Order::default())), [20]);
        assert_eq!(pids(&rows(&agents, "release", Order::default())), [20]);
        assert_eq!(
            totals(&rows(&agents, "build", Order::default())),
            (42.0, 1200)
        );
    }

    #[test]
    fn sizes_and_times_read_in_their_largest_units() {
        assert_eq!(memory(512), "512 B");
        assert_eq!(memory(2048), "2.0 KB");
        assert_eq!(memory(84 * 1024 * 1024 + 300 * 1024), "84.3 MB");
        assert_eq!(memory(3 * 1024 * 1024 * 1024), "3.0 GB");
        assert_eq!(cpu(0.4), "0%");
        assert_eq!(cpu(143.6), "144%");
        assert_eq!(duration(Duration::from_secs(45)), "45s");
        assert_eq!(
            duration(Duration::from_mins(12) + Duration::from_secs(5)),
            "12m 5s"
        );
        assert_eq!(
            duration(Duration::from_hours(3) + Duration::from_mins(20)),
            "3h 20m"
        );
        assert_eq!(duration(Duration::from_hours(2 * 24 + 4)), "2d 4h");
    }
}
