//! OpenCode's tokens (USG-09; T-0354): its plugin's `session.usage.updated` events ([`super::hooks`]); no limit
//! windows ("Limits not reported", d0086): OpenCode has no plan or remaining-limit data, and a limit shows only as
//! its rate-limit retry, which carries its own reset, or as a failed execution on a limit (F10), whose reset is the
//! error's retry time or the "retry in" of its message.

use std::path::Path;

use serde_json::Value;

use super::OpenCodeContext;
use crate::agent_tools::events::LimitWindow;
use crate::agent_tools::{TokenSource, WindowsSource};
use crate::usage::limit::LimitKind;

pub(super) fn token_source(_context: &OpenCodeContext, _tool_id: &str, _cwd: &Path) -> TokenSource {
    TokenSource::Events
}

pub(super) fn windows_source() -> WindowsSource {
    WindowsSource::NotReported
}

pub(super) fn windows_from_report(_report: &Value) -> Option<Vec<LimitWindow>> {
    None
}

pub(super) fn limit_window_key(_kind: LimitKind) -> Option<&'static str> {
    None
}

/// A provider's "retry in" in a limit message (F10; Gemini's quota: "… Please retry in 8h25m12.5s."), after `now`:
/// the duration after "retry in" or "retry after", in hours, minutes, seconds or milliseconds ("8h25m",
/// "41.7s", "2 hours 5 minutes").
pub(super) fn limit_reset_from_text(text: &str, now: i64) -> Option<i64> {
    let lower = text.to_lowercase();
    let start = ["retry in ", "retry after "]
        .iter()
        .filter_map(|p| lower.find(p).map(|at| at + p.len()))
        .min()?;
    let ms = duration_ms(&lower[start..])?;
    (ms > 0).then_some(now + ms)
}

/// The duration at the start of `text`: number and unit pairs ("8h25m12.5s", "2 hours 5 minutes"), up to the first
/// word that isn't one.
fn duration_ms(text: &str) -> Option<i64> {
    let mut rest = text.trim_start();
    let mut total = 0.0_f64;
    let mut any = false;
    loop {
        let digits = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.'))
            .unwrap_or(rest.len());
        let Ok(n) = rest[..digits].parse::<f64>() else {
            break;
        };
        let after = rest[digits..].trim_start();
        let unit_len = after
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(after.len());
        let factor = match &after[..unit_len] {
            "h" | "hr" | "hrs" | "hour" | "hours" => 3_600_000.0,
            "m" | "min" | "mins" | "minute" | "minutes" => 60_000.0,
            "s" | "sec" | "secs" | "second" | "seconds" => 1_000.0,
            "ms" => 1.0,
            _ => break,
        };
        total += n * factor;
        any = true;
        rest = after[unit_len..].trim_start_matches([' ', ',']);
        rest = rest.strip_prefix("and ").unwrap_or(rest);
    }
    any.then_some(total.round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_retry_in_the_limit_message_is_the_reset() {
        const H: i64 = 3_600_000;
        const M: i64 = 60_000;
        let quota = "You exceeded your current quota. * Quota exceeded for metric: \
                     generate_content_free_tier_requests, limit: 20 per day. Please retry in 8h25m12.5s.";
        assert_eq!(
            limit_reset_from_text(quota, 1_000),
            Some(1_000 + 8 * H + 25 * M + 12_500)
        );
        assert_eq!(
            limit_reset_from_text("Please retry in 41.7s.", 0),
            Some(41_700)
        );
        assert_eq!(
            limit_reset_from_text("Retry after 2 hours and 5 minutes", 0),
            Some(2 * H + 5 * M)
        );
        assert_eq!(limit_reset_from_text("Retry in 250ms", 0), Some(250));
        for none in [
            "Rate limit exceeded",
            "Please retry in a while",
            "retry in 0s",
        ] {
            assert_eq!(limit_reset_from_text(none, 0), None, "{none}");
        }
    }
}
