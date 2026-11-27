//! What the chat knows of the agent's prompt cache: how the last request
//! read its prompt, how much of the conversation's prompts came from the
//! cache, and until when the cache stays warm.

use std::time::{Duration, SystemTime};

use vortex_agent::PromptTokens;

#[derive(Default)]
pub struct PromptCache {
    last: Option<(PromptTokens, SystemTime)>,
    /// How long the agent keeps what it caches; requests that only read
    /// from the cache do not say, so the last one that wrote does.
    kept_for: Option<Duration>,
    total: PromptTokens,
    requests: u64,
}

impl PromptCache {
    /// Takes the request that started at `at`.
    pub fn record(&mut self, tokens: PromptTokens, at: SystemTime) {
        self.kept_for = tokens.kept_for.or(self.kept_for);
        self.total.cached += tokens.cached;
        self.total.written += tokens.written;
        self.total.uncached += tokens.uncached;
        self.requests += 1;
        self.last = Some((tokens, at));
    }

    pub fn last(&self) -> Option<PromptTokens> {
        self.last.map(|(tokens, _)| tokens)
    }

    pub fn kept_for(&self) -> Option<Duration> {
        self.kept_for
    }

    /// When the cache entry expires: each request that uses it keeps it
    /// for the whole time again.
    pub fn warm_until(&self) -> Option<SystemTime> {
        let (_, at) = self.last?;
        Some(at + self.kept_for?)
    }

    /// How many requests the conversation's own turns made, subagents' not
    /// counted.
    pub fn requests(&self) -> u64 {
        self.requests
    }

    /// The prompt an average request sent, once one was made.
    pub fn average_prompt(&self) -> Option<u64> {
        let total = self.total.cached + self.total.written + self.total.uncached;
        (self.requests > 0).then(|| total / self.requests)
    }

    /// Whole percent of every prompt token so far read from the cache.
    pub fn hit_percent(&self) -> Option<u64> {
        let total = self.total.cached + self.total.written + self.total.uncached;
        (total > 0).then(|| self.total.cached * 100 / total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(cached: u64, written: u64, kept_for: Option<Duration>) -> PromptTokens {
        PromptTokens {
            cached,
            written,
            uncached: 0,
            kept_for,
        }
    }

    #[test]
    fn the_cache_stays_warm_for_its_time_after_each_request() {
        let start = SystemTime::UNIX_EPOCH;
        let hour = Duration::from_hours(1);
        let mut cache = PromptCache::default();
        assert_eq!(cache.warm_until(), None);
        cache.record(tokens(0, 1_000, Some(hour)), start);
        assert_eq!(cache.warm_until(), Some(start + hour));
        let later = start + Duration::from_mins(20);
        cache.record(tokens(1_000, 0, None), later);
        assert_eq!(cache.kept_for(), Some(hour), "a read keeps the known time");
        assert_eq!(cache.warm_until(), Some(later + hour));
        assert_eq!(cache.hit_percent(), Some(50));
        assert_eq!(cache.last(), Some(tokens(1_000, 0, None)));
    }

    #[test]
    fn without_a_write_seen_the_expiry_is_unknown() {
        let mut cache = PromptCache::default();
        assert_eq!(cache.hit_percent(), None);
        cache.record(tokens(3_000, 0, None), SystemTime::UNIX_EPOCH);
        assert_eq!(cache.warm_until(), None);
        assert_eq!(cache.hit_percent(), Some(100));
    }
}
