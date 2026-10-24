//! Times and durations as the sheet shows them: "11:02" in local time, "41 min", "2 h 5 min"; and as the usage
//! limits show them: "3:45 pm", "Mon", "Nov 1", "in 1 h 20 min".

/// The local time of day of a Unix-milliseconds timestamp, as `HH:MM`.
pub fn local_hhmm(at_ms: i64) -> String {
    match otaq_core::local_time::local(at_ms) {
        Some(t) => format!("{:02}:{:02}", t.hour, t.minute),
        None => "--:--".to_owned(),
    }
}

/// A duration in whole minutes: "0 min", "41 min", "72 min" (as the History frame), then "2 h 5 min", "3 h"
/// from two hours on.
pub fn duration(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    if minutes < 120 {
        return format!("{minutes} min");
    }
    match (minutes / 60, minutes % 60) {
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// "3:45 pm", "12:00 am".
pub fn clock_12h(hour: u32, minute: u32) -> String {
    let (h, suffix) = match hour {
        0 => (12, "am"),
        1..=11 => (hour, "am"),
        12 => (12, "pm"),
        _ => (hour - 12, "pm"),
    };
    format!("{h}:{minute:02} {suffix}")
}

/// A time still to come, rounded up to the minute: "42 min", "1 h 20 min", "3 h".
pub fn in_label(ms: i64) -> String {
    let minutes = (ms.max(0) + 59_999) / 60_000;
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// Day names, from Sunday (`LocalTime::weekday`).
pub const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A day as the frames write it: "Oct 12" (local time).
pub fn month_day(t: &otaq_core::local_time::LocalTime) -> String {
    format!(
        "{} {}",
        MONTHS[(t.month as usize).saturating_sub(1) % 12],
        t.day
    )
}

/// When a limit window resets, as short as the frames say it: "3:45 pm" within a day, "Mon" within a week,
/// else "Nov 1".
pub fn reset_label(at: i64, now: i64) -> String {
    let Some(t) = otaq_core::local_time::local(at) else {
        return "at an unknown time".to_owned();
    };
    let ahead = at - now;
    if ahead < 24 * 3_600_000 {
        clock_12h(t.hour, t.minute)
    } else if ahead < 7 * 24 * 3_600_000 {
        WEEKDAYS[t.weekday as usize].to_owned()
    } else {
        month_day(&t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(duration(0), "0 min");
        assert_eq!(duration(59_999), "0 min");
        assert_eq!(duration(41 * 60_000 + 5_000), "41 min");
        assert_eq!(duration(72 * 60_000), "72 min");
        assert_eq!(duration(125 * 60_000), "2 h 5 min");
        assert_eq!(duration(180 * 60_000), "3 h");
        assert_eq!(duration(-5), "0 min");
    }

    #[test]
    fn time_of_day_has_two_digit_fields() {
        let t = local_hhmm(1_791_582_000_000);
        assert_eq!(t.len(), 5);
        assert_eq!(&t[2..3], ":");
    }

    #[test]
    fn limit_times_read_like_the_frames() {
        assert_eq!(clock_12h(15, 45), "3:45 pm");
        assert_eq!(clock_12h(0, 0), "12:00 am");
        assert_eq!(clock_12h(12, 5), "12:05 pm");
        assert_eq!(in_label(42 * 60_000), "42 min");
        assert_eq!(in_label(80 * 60_000), "1 h 20 min");
        assert_eq!(in_label(3 * 3_600_000), "3 h");

        let now = 1_791_547_200_000;
        let soon = now + 80 * 60_000;
        let t = otaq_core::local_time::local(soon).unwrap();
        assert_eq!(reset_label(soon, now), clock_12h(t.hour, t.minute));
        let days = now + 3 * 86_400_000;
        let t = otaq_core::local_time::local(days).unwrap();
        assert_eq!(reset_label(days, now), WEEKDAYS[t.weekday as usize]);
        let later = now + 20 * 86_400_000;
        let t = otaq_core::local_time::local(later).unwrap();
        assert_eq!(
            reset_label(later, now),
            format!("{} {}", MONTHS[t.month as usize - 1], t.day)
        );
    }
}
