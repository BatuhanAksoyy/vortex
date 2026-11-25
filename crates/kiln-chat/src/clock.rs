//! Times of day and lengths of time, as the chat words them.

use std::time::{Duration, SystemTime};

/// "15:00" today, "Mon 11:00" on another day, in local time.
pub fn reset_label(at: SystemTime, now: SystemTime) -> String {
    let local = |time: SystemTime| {
        jiff::Timestamp::try_from(time)
            .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
            .to_zoned(jiff::tz::TimeZone::system())
    };
    let (at, now) = (local(at), local(now));
    if at.date() == now.date() {
        at.strftime("%H:%M").to_string()
    } else {
        at.strftime("%a %H:%M").to_string()
    }
}

/// `4m 36s`, or `1.2s` for a short time.
pub fn took(time: Duration) -> String {
    let secs = time.as_secs();
    match secs {
        0..10 => format!("{:.1}s", time.as_secs_f64()),
        10..60 => format!("{secs}s"),
        60..3_600 => format!("{}m {}s", secs / 60, secs % 60),
        _ => format!("{}h {}m", secs / 3_600, secs / 60 % 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reset_on_another_day_names_the_day() {
        let now = SystemTime::now();
        assert_eq!(reset_label(now, now).len(), "15:00".len());
        let next_week = now + Duration::from_hours(72);
        assert!(reset_label(next_week, now).contains(' '));
    }

    #[test]
    fn a_time_taken_reads_in_its_two_largest_units() {
        assert_eq!(took(Duration::from_millis(1154)), "1.2s");
        assert_eq!(took(Duration::from_secs(16)), "16s");
        assert_eq!(took(Duration::from_secs(276)), "4m 36s");
        assert_eq!(took(Duration::from_secs(3_725)), "1h 2m");
    }
}
