//! Codex's hooks (ADP-07, ADP-02; d0088). The payloads are the hands-on check's (row 3, Codex CLI 0.157.1; saved
//! under `tests/fixtures/codex/hooks/`): every event has `session_id`, `transcript_path`, `cwd`, `hook_event_name`,
//! `model` and `permission_mode`; turn events add `turn_id`.
//!
//! - `SessionStart` (`source` `startup` or `resume`) → the tool id (`session_id`) and the model;
//! - `PermissionRequest` (the approval dialog shows) → needs you, with the command (`tool_input.command`);
//! - `Stop` → the turn ended, with `last_assistant_message` (no background work reported: Otaq checks the process
//!   tree, ADP-12);
//! - `Interrupt` (Esc at the approval dialog or during a turn) → nothing: Codex sends it as the Esc is pressed, while
//!   it is still stopping the turn, and text typed then is taken in as the next turn *and* put back in its input line
//!   (re-tests 2 and 3: Otaq's stage reminder sent once and typed a second time while Codex worked). The turn ends
//!   when Codex writes `turn_aborted` to its session file ([`super::session_file`]);
//! - `UserPromptSubmit` → a new turn, with its `prompt`;
//! - `SessionEnd` → nothing (the session's own end says it).
//!
//! Codex has no hook for a failed turn, a limit or a question: those come from its session file
//! ([`super::session_file`]). Anything else is unrecognised and logged raw (ADP-11).

use serde_json::Value;

use crate::agent_tools::{SignalContext, Tool, ToolEvent, unrecognised};

pub(super) fn map_hook(
    event: &str,
    payload: &Value,
    _context: SignalContext<'_>,
) -> Vec<ToolEvent> {
    match event {
        "SessionStart" => {
            let Some(tool_id) = text(payload, "session_id") else {
                return unrecognised(Tool::Codex, event, payload);
            };
            let mut events = vec![ToolEvent::ToolIdKnown { tool_id }];
            if let Some(model) = text(payload, "model") {
                events.push(ToolEvent::ModelKnown { model });
            }
            events
        }
        "PermissionRequest" => vec![ToolEvent::NeedsYou {
            prompt: permission_prompt(payload),
            helper_agent: false,
        }],
        "Stop" => vec![ToolEvent::TurnEnded {
            background: Default::default(),
            last_message: text(payload, "last_assistant_message"),
        }],
        // Not the turn's end yet: see the module's notes.
        "Interrupt" => Vec::new(),
        "UserPromptSubmit" => vec![ToolEvent::NewTurn {
            prompt: text(payload, "prompt"),
        }],
        "SessionEnd" => Vec::new(),
        _ => unrecognised(Tool::Codex, event, payload),
    }
}

/// What the approval dialog asks: "Run command: curl …" for a command, else "Use <tool>".
fn permission_prompt(payload: &Value) -> String {
    let input = payload.get("tool_input");
    if let Some(command) = input
        .and_then(|i| i.get("command"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        return format!("Run command: {command}");
    }
    match text(payload, "tool_name") {
        Some(tool) => format!("Use {tool}"),
        None => "Codex asks for approval".to_owned(),
    }
}

/// A non-empty string field.
fn text(payload: &Value, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_tools::codex::tests::fixture;

    fn map(name: &str) -> Vec<ToolEvent> {
        let payload = fixture(&format!("hooks/{name}"));
        let event = payload["hook_event_name"].as_str().unwrap().to_owned();
        map_hook(&event, &payload, SignalContext::default())
    }

    fn turn_ended(last_message: Option<&str>) -> ToolEvent {
        ToolEvent::TurnEnded {
            background: Default::default(),
            last_message: last_message.map(str::to_owned),
        }
    }

    /// Each saved payload of the hands-on check gives its events.
    #[test]
    fn each_hook_gives_its_events() {
        let id = "01a1479b-da76-7702-a546-a8847b2bcc57";
        for name in ["SessionStart-startup.json", "SessionStart-resume.json"] {
            assert_eq!(
                map(name),
                [
                    ToolEvent::ToolIdKnown { tool_id: id.into() },
                    ToolEvent::ModelKnown {
                        model: "gpt-6-luna".into()
                    }
                ],
                "{name}"
            );
        }
        assert_eq!(
            map("PermissionRequest.json"),
            [ToolEvent::NeedsYou {
                prompt: "Run command: curl -sI https://example.com | head -1".into(),
                helper_agent: false
            }]
        );
        assert_eq!(map("Stop.json"), [turn_ended(Some("PONG"))]);
        assert_eq!(map("Interrupt.json"), []);
        assert_eq!(
            map("UserPromptSubmit.json"),
            [ToolEvent::NewTurn {
                prompt: Some("Reply with the single word PONG.".into())
            }]
        );
        assert_eq!(map("SessionEnd.json"), []);
        assert_eq!(
            map("PreToolUse.json"),
            [ToolEvent::Unrecognised {
                event: "PreToolUse".into()
            }]
        );
    }

    #[test]
    fn a_session_start_without_an_id_is_unrecognised_and_one_without_a_model_has_only_the_id() {
        let start = |payload: Value| map_hook("SessionStart", &payload, SignalContext::default());
        assert_eq!(
            start(serde_json::json!({"model": "gpt-6-luna"})),
            [ToolEvent::Unrecognised {
                event: "SessionStart".into()
            }]
        );
        assert_eq!(
            start(serde_json::json!({"session_id": "s-1"})),
            [ToolEvent::ToolIdKnown {
                tool_id: "s-1".into()
            }]
        );
    }

    #[test]
    fn a_permission_request_without_a_command_names_the_tool() {
        let ask = |payload: Value| permission_prompt(&payload);
        assert_eq!(
            ask(serde_json::json!({"tool_name": "apply_patch", "tool_input": {}})),
            "Use apply_patch"
        );
        assert_eq!(ask(serde_json::json!({})), "Codex asks for approval");
    }

    /// An event Otaq doesn't map is logged raw at debug level (ADP-11), so a hand check can capture it.
    #[test]
    fn an_unknown_event_is_logged_raw() {
        #[derive(Clone, Default)]
        struct Log(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
        impl std::io::Write for Log {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let log = Log::default();
        let writer = log.clone();
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::DEBUG)
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        let events = tracing::subscriber::with_default(subscriber, || map("PreToolUse.json"));
        assert_eq!(
            events,
            [ToolEvent::Unrecognised {
                event: "PreToolUse".into()
            }]
        );
        let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
        assert!(text.contains("DEBUG"), "{text}");
        assert!(
            text.contains("a hook the adapter doesn't recognise"),
            "{text}"
        );
        assert!(
            text.contains("\"tool_input\":{\"command\":\"sleep 20\"}"),
            "{text}"
        );
    }
}
