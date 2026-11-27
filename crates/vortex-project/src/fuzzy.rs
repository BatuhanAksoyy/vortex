use std::path::Path;

use nucleo_matcher::{
    Config, Matcher, Utf32Str,
    pattern::{CaseMatching, Normalization, Pattern},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathMatch {
    /// Index into the candidate slice.
    pub index: usize,
    pub score: u32,
}

/// Ranks `candidates` against `query` with path-aware fuzzy matching and
/// returns at most `limit` matches, best first. An empty query keeps the
/// original order.
#[must_use]
pub fn fuzzy_match(query: &str, candidates: &[&Path], limit: usize) -> Vec<PathMatch> {
    if query.trim().is_empty() {
        return (0..candidates.len().min(limit))
            .map(|index| PathMatch { index, score: 0 })
            .collect();
    }
    let mut scorer = Matcher::new(Config::DEFAULT.match_paths());
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buffer = Vec::new();
    let mut matches: Vec<PathMatch> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            let text = path.to_string_lossy();
            let haystack = Utf32Str::new(&text, &mut buffer);
            pattern
                .score(haystack, &mut scorer)
                .map(|score| PathMatch { index, score })
        })
        .collect();
    matches.sort_by(|a, b| b.score.cmp(&a.score).then(a.index.cmp(&b.index)));
    matches.truncate(limit);
    matches
}

#[cfg(test)]
mod tests {
    use super::fuzzy_match;
    use std::path::Path;

    #[test]
    fn ranks_closer_matches_first_and_respects_limit() {
        let paths = [
            Path::new("docs/ARCHITECTURE.md"),
            Path::new("crates/vortex-settings/src/load.rs"),
            Path::new("crates/vortex-project/src/lib.rs"),
        ];
        let matches = fuzzy_match("settload", &paths, 10);
        assert_eq!(matches[0].index, 1);
        assert_eq!(fuzzy_match("rs", &paths, 1).len(), 1);
    }

    #[test]
    fn empty_query_keeps_order() {
        let paths = [Path::new("b"), Path::new("a")];
        let indices: Vec<usize> = fuzzy_match("", &paths, 10)
            .iter()
            .map(|m| m.index)
            .collect();
        assert_eq!(indices, [0, 1]);
    }

    #[test]
    fn no_match_returns_empty() {
        assert_eq!(fuzzy_match("zzz", &[Path::new("abc")], 10), []);
    }
}
