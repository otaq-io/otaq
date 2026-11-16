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
        return format!("{RUN_COMMAND}{command}");
    }
    match text(payload, "tool_name") {
        Some(tool) => format!("Use {tool}"),
        None => "Codex asks for approval".to_owned(),
    }
}

/// How a command's prompt starts: "Run command: curl …".
const RUN_COMMAND: &str = "Run command: ";

/// Codex's approval dialog on screen (the hands-on screens of re-test 5, Codex CLI 0.166):
///
/// ```text
///   Would you like to run the following command?
///
///   Environment: local
///
///   $ git diff -- README.md
///
/// › 1. Yes, proceed (y)
///   2. Yes, and don't ask again for commands that start with `git diff -- README.md` (p)
///   3. No, and tell Codex what to do differently (esc)
///
///   Press enter to confirm or esc to cancel
/// ```
///
/// Once answered, Codex replaces it with "✓ You approved codex to run … this time" and the title is gone. Commands it
/// asks for at once show as "• Running …" lines above the dialog, so only the dialog itself says which is drawn.
pub(super) struct Dialog;

/// The title every Codex approval dialog starts with ("… run the following command?", "… make the following edits?").
const DIALOG_TITLE: &str = "Would you like to ";

/// How much of a command is compared: a long one may wrap or be cut on screen.
const COMPARED: usize = 40;

impl crate::agent_tools::PromptScreen for Dialog {
    fn dialog<'s>(&self, screen: &'s str) -> Option<&'s str> {
        let mut at = None;
        let mut offset = 0;
        for line in screen.split_inclusive('\n') {
            if line.trim_start().starts_with(DIALOG_TITLE) {
                at = Some(offset);
            }
            offset += line.len();
        }
        at.map(|at| &screen[at..])
    }

    fn recognises(&self, prompt: &str) -> bool {
        prompt.starts_with(RUN_COMMAND)
    }

    fn shows(&self, dialog: &str, prompt: &str) -> bool {
        let Some(command) = prompt.strip_prefix(RUN_COMMAND) else {
            return false;
        };
        let asked = squashed(command);
        // The dialog's command: from "$ " to the blank line after it (a long one wraps over several lines).
        let lines: Vec<&str> = dialog.lines().map(str::trim).collect();
        let shown = match lines.iter().position(|l| l.starts_with("$ ")) {
            Some(at) => squashed(
                &lines[at..]
                    .iter()
                    .take_while(|l| !l.is_empty())
                    .map(|l| l.strip_prefix("$ ").unwrap_or(l))
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            None => return false,
        };
        let head = |s: &str| s.chars().take(COMPARED).collect::<String>();
        !asked.is_empty()
            && !shown.is_empty()
            && (shown.contains(&head(&asked)) || asked.contains(&head(&shown)))
    }
}

/// Codex's sign-in screen (decision 118; captured on Windows with Codex CLI 0.166 and an expired ChatGPT sign-in,
/// `tests/fixtures/input-line/codex-sign-in.bin`), drawn instead of the conversation, with no hook sent:
///
/// ```text
///   Welcome to Codex, OpenAI's command-line coding agent
///
///   Choose how you want to use Codex.
///
/// > 1. Sign in with ChatGPT
///      Usage included with Plus, Pro, Business, and Enterprise plans
///
///   2. Sign in with Device Code
///   …
///   Press enter to continue
/// ```
///
/// Read off a session's screen only before its first hook, so an agent's own words can't match it.
pub(super) fn sign_in_screen(screen: &str) -> bool {
    let has = |words: &str| screen.lines().any(|line| line.contains(words));
    has(SIGN_IN_OPTION) && has(SIGN_IN_CONTINUE)
}

/// The sign-in screen's first option, pointed at or not.
const SIGN_IN_OPTION: &str = "1. Sign in with ChatGPT";
/// The sign-in screen's last line.
const SIGN_IN_CONTINUE: &str = "Press enter to continue";

/// `text` without any whitespace, so a command wrapped over lines compares with the one-line command.
fn squashed(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
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

    /// The dialog drawn is the last one on screen, and it shows the command it asks for, wrapped or not; a command only
    /// named in a "• Running" line above it isn't the one it asks for.
    #[test]
    fn the_dialog_on_screen_names_its_command() {
        use crate::agent_tools::PromptScreen;
        let screen = "\
• Running Get-Content package.json; Get-Content docs/one.txt
• Running git show --stat --oneline 718a765

  Would you like to run the following command?

  Environment: local

  $ Get-Content package.json; Get-Content docs/one.txt; Get-Content
  docs/two.txt

› 1. Yes, proceed (y)
  2. Yes, and don't ask again for commands that start with `Get-Content docs/one.txt` (p)
  3. No, and tell Codex what to do differently (esc)

  Press enter to confirm or esc to cancel";
        let dialog = Dialog.dialog(screen).expect("a dialog");
        assert!(dialog.trim_start().starts_with("Would you like to run"));
        let first = "Run command: Get-Content package.json; Get-Content docs/one.txt; Get-Content docs/two.txt";
        assert!(Dialog.shows(dialog, first));
        assert!(!Dialog.shows(dialog, "Run command: git show --stat --oneline 718a765"));
        assert!(!Dialog.shows(dialog, "Use apply_patch"));

        let answered = "✓ You approved codex to run git log -1 --oneline this time\n\n• Ran git log -1 --oneline\n› ";
        assert_eq!(Dialog.dialog(answered), None);
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

    /// A capture of the session's screen (`tests/fixtures/input-line/<name>.bin`, 120 × 30, Windows' ConPTY) as
    /// [`crate::session::Sessions::visible_screen`] gives it.
    fn screen_of(name: &str) -> String {
        let path = format!(
            "{}/tests/fixtures/input-line/{name}.bin",
            env!("CARGO_MANIFEST_DIR")
        );
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let size = crate::session::TerminalSize {
            cols: 120,
            rows: 30,
        };
        crate::session::screen::render_above(&bytes, size, 0)
    }

    /// Decision 118 (F-E2-4): Codex 0.166 with an expired ChatGPT sign-in, captured in the Windows VM, opens its
    /// onboarding menu; its idle input line, a typed line and its approval dialog are no sign-in screen.
    #[test]
    fn the_captured_sign_in_screen_is_codexs_sign_in_screen() {
        let screen = screen_of("codex-sign-in");
        assert!(screen.contains("Welcome to Codex"), "{screen}");
        assert!(sign_in_screen(&screen), "{screen}");
        for name in [
            "codex-empty",
            "codex-typed",
            "codex-approval",
            "codex-after-approval",
        ] {
            assert!(!sign_in_screen(&screen_of(name)), "{name}");
        }
        // An agent's echo naming one of the two isn't it either.
        assert!(!sign_in_screen(
            "› 1. Sign in with ChatGPT is the first option"
        ));
        // It reads as a dialog of the tool's own: Otaq types no message into it.
        let path = format!(
            "{}/tests/fixtures/input-line/codex-sign-in.bin",
            env!("CARGO_MANIFEST_DIR")
        );
        let reading = crate::session::input_line::read_all(
            &std::fs::read(path).unwrap(),
            crate::session::TerminalSize {
                cols: 120,
                rows: 30,
            },
            crate::board::settings::TerminalColors {
                fg: 0xD6D9E0,
                bg: 0x14161C,
            },
        );
        assert!(reading.dialog, "{reading:?}");
    }
}
