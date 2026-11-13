//! Codex's session file, watched while its agent runs (ADP-03, ADP-07; d0086, d0088): what no hook reports. The
//! file is the conversation's `<CODEX_HOME>/sessions/YYYY/MM/DD/rollout-…-<id>.jsonl`
//! ([`super::launch::session_file`]), one JSON record per line, `{"timestamp", "ordinal", "type", "payload"}`.
//! Samples (captured with Codex CLI 0.157.1): `tests/fixtures/codex/session_file/`.
//!
//! - `event_msg` `task_complete` with an `error` (a failed turn fires no hook): the model can't be used (`The
//!   '<model>' model is not supported when using Codex with a ChatGPT account.`, HTTP 400, inside `error.message`)
//!   → *model refused*; a usage limit (`codex_error_info` `usage_limit_exceeded` / `usage_limit_reached`, or the
//!   limit message) → *limit reached*; anything else → *failed*.
//! - `event_msg` `token_count` → the limit windows in its `rate_limits` ([`super::usage::windows_of`]). Its tokens
//!   are read by the usage reader ([`super::usage`]), not here.
//!
//! - `event_msg` `turn_aborted` with the reason `interrupted` (Esc at the approval dialog or during a turn) → *turn
//!   ended*. Its `Interrupt` hook comes as the Esc is pressed, before Codex has stopped the turn, so it isn't the end
//!   ([`super::hooks`]): only now is Codex idle at an empty input line, ready for a message Otaq types.
//!
//! Every other record (the messages, `task_started`, a turn aborted for another reason, such as a new turn replacing
//! it…) gives nothing. A line that isn't such a record is *unrecognised* (logged raw), so the watcher can tell that Codex
//! changed its file.

use std::path::{Path, PathBuf};

use serde_json::Value;

use super::CodexContext;
use super::launch::session_file;
use super::usage::{limit_hit, windows_of};
use crate::agent_tools::{SignalContext, Tool, ToolEvent};

/// The event name an unreadable line is reported under.
pub(super) const UNREADABLE_LINE: &str = "session file line";

/// The conversation's session file in Codex's home, once Codex has written it.
pub(super) fn watched_file(context: &CodexContext, tool_id: &str, _cwd: &Path) -> Option<PathBuf> {
    session_file(context.codex_home.as_deref()?, tool_id)
}

/// The events of whole lines newly added to the file.
pub(super) fn map_lines(lines: &str, _context: SignalContext<'_>) -> Vec<ToolEvent> {
    lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .flat_map(map_line)
        .collect()
}

fn map_line(line: &str) -> Vec<ToolEvent> {
    let record = serde_json::from_str::<Value>(line).ok();
    let Some((kind, payload)) = record.as_ref().and_then(|r| {
        Some((
            r.get("type")?.as_str()?,
            r.get("payload").filter(|p| p.is_object())?,
        ))
    }) else {
        tracing::debug!(tool = %Tool::Codex, line, "a session file line the adapter can't read");
        return vec![ToolEvent::Unrecognised {
            event: UNREADABLE_LINE.to_owned(),
        }];
    };
    if kind != "event_msg" {
        return Vec::new();
    }
    match payload.get("type").and_then(Value::as_str) {
        Some("task_complete") => {
            let Some(error) = payload.get("error").filter(|e| !e.is_null()) else {
                return Vec::new();
            };
            tracing::debug!(tool = %Tool::Codex, line, "Codex turn failed");
            vec![failed_turn(error)]
        }
        Some("turn_aborted")
            if payload.get("reason").and_then(Value::as_str) == Some("interrupted") =>
        {
            tracing::debug!(tool = %Tool::Codex, line, "Codex turn interrupted");
            vec![ToolEvent::TurnEnded {
                background: Default::default(),
                last_message: None,
            }]
        }
        Some("token_count") => {
            tracing::debug!(tool = %Tool::Codex, line, "Codex token count");
            payload
                .get("rate_limits")
                .and_then(windows_of)
                .map(|windows| vec![ToolEvent::LimitWindows { windows }])
                .unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

/// A `task_complete` error: `{"message": "…", "codex_error_info": "other"}`, where the message may itself be the
/// API's JSON error (`{"type":"error","status":400,"error":{"message":"The '…' model is not supported…"}}`).
fn failed_turn(error: &Value) -> ToolEvent {
    let raw = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let message = api_message(raw).unwrap_or_else(|| raw.trim().to_owned());
    let info = error_info(error.get("codex_error_info"));
    if let Some(hit) = limit_hit(info.as_deref(), &message) {
        return ToolEvent::LimitReached {
            hit,
            resets_at: error.get("resets_at").and_then(seconds_ms),
        };
    }
    if let Some(model) = refused_model(&message) {
        return ToolEvent::ModelRefused {
            model: Some(model),
            message,
        };
    }
    ToolEvent::Failed {
        error: info.unwrap_or_else(|| "unknown".to_owned()),
        message: (!message.is_empty()).then_some(message),
    }
}

/// The message inside an API error passed on as JSON text.
fn api_message(raw: &str) -> Option<String> {
    let value: Value = serde_json::from_str(raw.trim()).ok()?;
    value
        .pointer("/error/message")
        .or_else(|| value.get("message"))
        .and_then(Value::as_str)
        .map(|m| m.trim().to_owned())
}

/// `codex_error_info` as a word: a plain string (`"other"`), or the key of an object
/// (`{"usage_limit_exceeded": {…}}`).
fn error_info(info: Option<&Value>) -> Option<String> {
    match info? {
        Value::String(s) => Some(s.clone()),
        Value::Object(map) => map.keys().next().cloned(),
        _ => None,
    }
}

/// The model a refusal names: "The 'foo-bar-9' model is not supported when using Codex with a ChatGPT account."
/// (the TUI and `codex exec` alike), or `codex exec`'s "Model metadata for `foo-bar-9` not found".
fn refused_model(message: &str) -> Option<String> {
    let quoted = |open: char, close: char, rest: &str| {
        let start = rest.find(open)? + 1;
        let end = start + rest[start..].find(close)?;
        Some(rest[start..end].to_owned()).filter(|m| !m.is_empty())
    };
    if message.contains("model is not supported") {
        return quoted('\'', '\'', message);
    }
    let after = &message[message.find("Model metadata for")?..];
    after
        .contains("not found")
        .then(|| quoted('`', '`', after))
        .flatten()
}

/// Unix seconds as milliseconds.
pub(super) fn seconds_ms(value: &Value) -> Option<i64> {
    let seconds = value.as_f64().filter(|s| *s > 0.0)?;
    Some((seconds * 1000.0) as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_tools::LimitWindow;
    use crate::agent_tools::codex::tests::session_lines;
    use crate::usage::limit::{LimitHit, LimitKind};

    fn map(text: &str) -> Vec<ToolEvent> {
        map_lines(text, SignalContext::default())
    }

    fn weekly(used_percent: f64) -> LimitWindow {
        LimitWindow {
            key: "weekly".into(),
            length_minutes: Some(10_080),
            used_percent,
            resets_at: Some(1_792_807_562_000),
            credits: None,
        }
    }

    /// A TUI session of several turns: each `token_count` reports the Plus account's one weekly window, and the one
    /// interrupted turn (Esc) ends; nothing else gives an event (the other turn ends come from hooks).
    #[test]
    fn token_counts_report_the_windows() {
        let events = map(&session_lines("turns.jsonl"));
        let interrupted = ToolEvent::TurnEnded {
            background: Default::default(),
            last_message: None,
        };
        assert_eq!(
            events.iter().filter(|e| **e == interrupted).count(),
            1,
            "the interrupted turn"
        );
        let reported: Vec<(String, f64, Option<i64>)> = events
            .into_iter()
            .filter(|e| *e != interrupted)
            .map(|event| match event {
                ToolEvent::LimitWindows { windows } if windows.len() == 1 => (
                    windows[0].key.clone(),
                    windows[0].used_percent,
                    windows[0].resets_at,
                ),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(reported.len(), 10, "one per token_count");
        assert_eq!(
            reported[0],
            ("weekly".to_owned(), 0.0, Some(1_792_807_560_000))
        );
        let later = weekly(0.0);
        assert!(
            reported[1..]
                .iter()
                .all(|r| *r == (later.key.clone(), 0.0, later.resets_at))
        );
    }

    /// The TUI's first turn on a model the account can't use (captured): *model refused*, with the API's message.
    #[test]
    fn a_model_the_account_cant_use_is_refused() {
        assert_eq!(
            map(&session_lines("model_refused.jsonl")),
            [ToolEvent::ModelRefused {
                model: Some("no-such-model-xyz".into()),
                message:
                    "The 'no-such-model-xyz' model is not supported when using Codex with a ChatGPT account."
                        .into()
            }]
        );
        assert_eq!(
            refused_model(
                "Model metadata for `foo-bar-9` not found. Defaulting to fallback metadata"
            ),
            Some("foo-bar-9".into())
        );
    }

    /// Any other error is a failed turn with Codex's own message.
    #[test]
    fn another_error_is_a_failed_turn() {
        let events = map(&session_lines("failed_signed_out.jsonl"));
        let [ToolEvent::Failed { error, message }] = events.as_slice() else {
            panic!("{events:?}");
        };
        assert_eq!(error, "other");
        assert!(
            message
                .as_deref()
                .unwrap()
                .starts_with("unexpected status 401 Unauthorized"),
            "{message:?}"
        );
        assert_eq!(
            failed_turn(&serde_json::json!({})),
            ToolEvent::Failed {
                error: "unknown".into(),
                message: None
            }
        );
    }

    /// The usage limit (written from the report): the window at 100 %, then *limit reached*.
    #[test]
    fn the_usage_limit_is_limit_reached() {
        let text = "You've hit your usage limit. Visit https://chatgpt.com/codex/settings/usage to purchase more \
                    credits or try again at 3:46 PM.";
        assert_eq!(
            map(&session_lines("limit.jsonl")),
            [
                ToolEvent::LimitWindows {
                    windows: vec![weekly(100.0)]
                },
                ToolEvent::LimitReached {
                    hit: LimitHit {
                        kind: LimitKind::Other,
                        text: text.into()
                    },
                    resets_at: None
                }
            ]
        );
        // The kind as an object, with the reset on the error.
        let error = serde_json::json!({
            "message": "You've hit your weekly usage limit.",
            "codex_error_info": {"usage_limit_reached": {}},
            "resets_at": 1_792_807_562
        });
        assert_eq!(
            failed_turn(&error),
            ToolEvent::LimitReached {
                hit: LimitHit {
                    kind: LimitKind::Weekly,
                    text: "You've hit your weekly usage limit.".into()
                },
                resets_at: Some(1_792_807_562_000)
            }
        );
    }

    /// Esc in Codex: its turn ends once Codex wrote `turn_aborted` (the line as re-test 3 captured it), not at its
    /// `Interrupt` hook. A turn a new one replaced isn't idle.
    #[test]
    fn an_interrupted_turn_ends_when_codex_wrote_it() {
        let aborted = |reason: &str| {
            serde_json::json!({"timestamp": "2026-11-10T23:00:21.197Z", "ordinal": 109, "type": "event_msg",
                "payload": {"type": "turn_aborted", "root_turn_id": "01a1c7b0-3c67-74e2-ba3c-0402a4187a6b",
                    "turn_id": "01a1c7b0-3c67-74e2-ba3c-0402a4187a6b", "reason": reason,
                    "started_at": 1_794_351_578, "completed_at": 1_794_351_621, "duration_ms": 42_962}})
            .to_string()
        };
        assert_eq!(
            map(&aborted("interrupted")),
            [ToolEvent::TurnEnded {
                background: Default::default(),
                last_message: None
            }]
        );
        assert_eq!(map(&aborted("replaced")), []);
    }

    /// A line that isn't a record (a partial write is never handed over; this is a changed format) is
    /// unrecognised; records of other types and empty lines give nothing.
    #[test]
    fn lines_that_arent_records_are_unrecognised() {
        let unreadable = ToolEvent::Unrecognised {
            event: UNREADABLE_LINE.into(),
        };
        assert_eq!(map("not json\n"), std::slice::from_ref(&unreadable));
        assert_eq!(
            map("{\"kind\":\"token_count\"}\n"),
            std::slice::from_ref(&unreadable)
        );
        assert_eq!(map("[1,2]\n"), [unreadable]);
        assert_eq!(
            map("\n{\"type\":\"response_item\",\"payload\":{\"type\":\"message\"}}\n"),
            []
        );
        assert_eq!(
            map("{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\",\"error\":null}}"),
            []
        );
    }
}
