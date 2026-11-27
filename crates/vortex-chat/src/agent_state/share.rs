//! How much of the plan's session window this chat used: what the window
//! rose by while the chat's own requests ran, up to the first report after
//! its turn. The plan reports one number for every session of the account,
//! so a rise while another chat ran at the same time counts here too; the
//! panel says "about".

use std::time::{Duration, SystemTime};

use vortex_agent::UsageWindow;

/// Reset times this close are the same window, reported twice.
const SAME_WINDOW: Duration = Duration::from_mins(1);

#[derive(Default)]
pub struct SessionShare {
    /// The window counted in, by when it resets, and its last percent.
    last: Option<(Option<SystemTime>, u8)>,
    /// A request of the chat's ran since the last report.
    counting: bool,
    /// The turn ended: the next report closes the count.
    ending: bool,
    here: u8,
}

impl SessionShare {
    /// A request of the chat's started.
    pub fn request_started(&mut self) {
        self.counting = true;
        self.ending = false;
    }

    pub fn turn_ended(&mut self) {
        self.ending = self.counting;
    }

    /// The chat moved to another account, whose window this is not.
    pub fn forget(&mut self) {
        *self = Self::default();
    }

    pub fn reported(&mut self, window: Option<UsageWindow>) {
        let Some(window) = window else {
            return;
        };
        let same = self.last.is_some_and(|(resets, percent)| {
            window.percent >= percent && near(resets, window.resets_at)
        });
        if !same {
            self.here = 0;
        } else if self.counting {
            let (_, percent) = self.last.unwrap_or((None, window.percent));
            self.here = self.here.saturating_add(window.percent - percent);
        }
        self.last = Some((window.resets_at, window.percent));
        if self.ending {
            (self.counting, self.ending) = (false, false);
        }
    }

    /// The percent of the session window this chat used, once it used any.
    pub fn here(&self) -> Option<u8> {
        (self.here > 0).then_some(self.here)
    }
}

fn near(a: Option<SystemTime>, b: Option<SystemTime>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            let gap = a.duration_since(b).or_else(|_| b.duration_since(a));
            gap.is_ok_and(|gap| gap < SAME_WINDOW)
        }
        (None, None) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(percent: u8) -> UsageWindow {
        UsageWindow {
            percent,
            resets_at: None,
        }
    }

    #[test]
    fn only_the_rise_during_the_chat_s_turns_counts() {
        let mut share = SessionShare::default();
        share.reported(Some(at(30)));
        share.request_started();
        share.reported(Some(at(36)));
        share.turn_ended();
        share.reported(Some(at(40)));
        assert_eq!(share.here(), Some(10), "its turn and the report after");
        share.reported(Some(at(45)));
        assert_eq!(share.here(), Some(10), "another chat's work");
        share.reported(Some(at(2)));
        assert_eq!(share.here(), None, "a new window");
    }
}
