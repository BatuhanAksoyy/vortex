//! The processes behind a session: its agent's CLI and everything that
//! started under it, with what each uses, and stopping them. No UI
//! dependencies.

mod processes;

pub use processes::{Monitor, ProcessInfo};

/// Asks `pid` and every process under it to end; whether any was asked.
/// Reads the process table first, so it takes a while.
#[must_use]
pub fn stop(pid: u32) -> bool {
    let Some(mut monitor) = Monitor::new() else {
        return false;
    };
    monitor.refresh();
    monitor.stop(pid)
}
