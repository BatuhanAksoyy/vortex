//! Where each commit of a log sits in a graph of lanes, and the lines that
//! join it to its parents, row by row as a history list draws them.

use crate::Commit;

/// A line in one half of a row, from a lane at the half's top to a lane at
/// its bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Edge {
    pub from: usize,
    pub to: usize,
}

/// A commit's row: the lane its dot is in, the lines from the row's top
/// edge to the middle, and those from the middle to the bottom edge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphRow {
    pub lane: usize,
    pub above: Vec<Edge>,
    pub below: Vec<Edge>,
}

impl GraphRow {
    /// How many lanes the row reaches across.
    #[must_use]
    pub fn width(&self) -> usize {
        let edges = self.above.iter().chain(&self.below);
        let widest = edges.map(|edge| edge.from.max(edge.to)).max();
        widest.unwrap_or(0).max(self.lane) + 1
    }
}

/// The rows of `commits`, which come children before parents, as
/// `git log --date-order` lists them. A parent past the last commit leaves
/// its line running off the bottom.
#[must_use]
pub fn graph(commits: &[Commit]) -> Vec<GraphRow> {
    // Each lane holds the commit it leads down to, or nothing once ended.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    commits
        .iter()
        .map(|commit| {
            let row = lay_out(&mut lanes, commit);
            while lanes.last() == Some(&None) {
                lanes.pop();
            }
            row
        })
        .collect()
}

fn lay_out<'a>(lanes: &mut Vec<Option<&'a str>>, commit: &'a Commit) -> GraphRow {
    let id = commit.id.as_str();
    let lane = (lanes.iter().position(|led| *led == Some(id))).unwrap_or_else(|| free(lanes));
    // Every lane led here ends in the dot: its children, one per branch.
    let above = (lanes.iter().enumerate())
        .filter_map(|(at, led)| {
            let to = if (*led)? == id { lane } else { at };
            Some(Edge { from: at, to })
        })
        .collect();
    for led in lanes.iter_mut().filter(|led| **led == Some(id)) {
        *led = None;
    }
    let joined = join_parents(lanes, lane, &commit.parents);
    let below = (lanes.iter().enumerate())
        .filter(|(_, led)| led.is_some())
        .map(|(at, _)| {
            let from = if joined.contains(&at) { lane } else { at };
            Edge { from, to: at }
        })
        .collect();
    GraphRow { lane, above, below }
}

/// Leads a lane to each parent: the commit's own to the first, so a
/// branch keeps its lane, and one already led to a merged parent, or a
/// free one, to the others. Returns the lanes the dot joins.
fn join_parents<'a>(
    lanes: &mut Vec<Option<&'a str>>,
    lane: usize,
    parents: &'a [String],
) -> Vec<usize> {
    let mut joined = Vec::with_capacity(parents.len());
    let mut parents = parents.iter().map(String::as_str);
    if let Some(first) = parents.next() {
        lanes[lane] = Some(first);
        joined.push(lane);
    }
    for parent in parents {
        let led = lanes.iter().position(|led| *led == Some(parent));
        let at = led.unwrap_or_else(|| free(lanes));
        lanes[at] = Some(parent);
        joined.push(at);
    }
    joined
}

/// The first lane that has ended, or a new one.
fn free(lanes: &mut Vec<Option<&str>>) -> usize {
    lanes.iter().position(Option::is_none).unwrap_or_else(|| {
        lanes.push(None);
        lanes.len() - 1
    })
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;

    fn commit(id: &str, parents: &[&str]) -> Commit {
        Commit {
            id: id.to_owned(),
            parents: parents.iter().map(|&parent| parent.to_owned()).collect(),
            refs: Vec::new(),
            subject: String::new(),
            author: String::new(),
            time: SystemTime::UNIX_EPOCH,
        }
    }

    fn edge(from: usize, to: usize) -> Edge {
        Edge { from, to }
    }

    #[test]
    fn a_straight_history_keeps_to_one_lane() {
        let rows = graph(&[commit("c", &["b"]), commit("b", &["a"]), commit("a", &[])]);
        assert!(rows.iter().all(|row| row.lane == 0 && row.width() == 1));
        assert!(rows[0].above.is_empty(), "nothing above the newest");
        assert_eq!(rows[1].above, [edge(0, 0)]);
        assert!(rows[2].below.is_empty(), "nothing below the first");
    }

    #[test]
    fn a_merged_branch_runs_beside_and_joins_back() {
        // m merges f into main; f and b both grew from a.
        let rows = graph(&[
            commit("m", &["b", "f"]),
            commit("f", &["a"]),
            commit("b", &["a"]),
            commit("a", &[]),
        ]);
        assert_eq!(rows[0].below, [edge(0, 0), edge(0, 1)], "out to f");
        assert_eq!(rows[1].lane, 1, "f beside main");
        assert_eq!(rows[1].above, [edge(0, 0), edge(1, 1)]);
        assert_eq!(rows[2].lane, 0);
        assert_eq!(rows[2].below, [edge(0, 0), edge(1, 1)]);
        assert_eq!(rows[3].lane, 0);
        assert_eq!(rows[3].above, [edge(0, 0), edge(1, 0)], "in from f");
        assert_eq!(rows[3].width(), 2);
    }

    #[test]
    fn a_parent_past_the_log_leads_off_the_bottom() {
        let rows = graph(&[commit("c", &["b"]), commit("x", &["w"])]);
        assert_eq!(rows[1].lane, 1, "another branch, beside");
        assert_eq!(rows[1].below, [edge(0, 0), edge(1, 1)]);
    }

    #[test]
    fn an_ended_lane_is_taken_again() {
        let rows = graph(&[
            commit("x", &["w"]),
            commit("c", &["b"]),
            commit("w", &[]),
            commit("y", &["v"]),
        ]);
        assert_eq!(rows[1].lane, 1);
        assert_eq!(rows[3].lane, 0, "w's lane ended with it");
    }
}
