//! OpenCode's sessions (REC-02, REC-03; d0045, d0088): the resume check, its text for an unknown `--session`, and
//! "Before the restart". Every command runs with `--standalone` in the session's folder, so none of them starts
//! OpenCode's shared background service (the default mode does, and leaves it running).
//!
//! Checked with OpenCode 2.0.17, its folders in a scratch folder (no provider, no model call;
//! `tests/fixtures/opencode/sessions/`):
//! - `opencode api --standalone GET /api/session/<id>` answers `{"data":{"id":…,"title":…,"model":…}}` and exits 0;
//!   an unknown id answers `{"_tag":"SessionNotFoundError",…,"message":"Session not found: <id>"}`, `HTTP 404 Not
//!   Found` and exits 1. (`opencode session list` shows only the newest 100 of the project's sessions, so the one
//!   session is asked for instead.)
//! - `opencode --standalone --session <unknown id>` shows "Session not found: <id>" and opens a new session (the
//!   capture `tui-session-not-found.bin`), so Otaq reads that text as Resume failed (REC-03).
//! - `opencode session export --standalone <id>` prints `{"info": {…}, "messages": [ … ]}`: `user` (`text`),
//!   `assistant` (`content`: `text`, `reasoning` and `tool` parts, a tool with its `name` and `state.input`),
//!   `shell` (a command the user ran, `command`), `skill`, `synthetic`, `model-switched`, `idle`…, each with
//!   `time.created` in Unix milliseconds. Written from the export schema in OpenCode's own API description and
//!   round-tripped through `opencode session import` / `export`; T-0369 replaces it with a capture.

use std::collections::VecDeque;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::{COMMAND_NAME, OpenCodeContext};
use crate::agent_tools::TranscriptEntry;

/// How long the resume check may take (ticket: 5 s).
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// How long the export may take: a long conversation is a large file.
const EXPORT_TIMEOUT: Duration = Duration::from_secs(15);

/// What OpenCode shows for a session id it doesn't have, lower-cased (the id follows).
const CONVERSATION_NOT_FOUND: &str = "session not found: ses";

/// The longest message kept, in characters (as Claude Code's).
const MAX_TEXT: usize = 2000;

/// The longest action summary, in characters.
const MAX_SUMMARY: usize = 120;

/// The arguments that ask for one session.
pub(super) fn session_args(tool_id: &str) -> Vec<String> {
    ["api", "--standalone", "GET"]
        .map(str::to_owned)
        .into_iter()
        .chain([format!("/api/session/{tool_id}")])
        .collect()
}

/// The arguments that export one session.
pub(super) fn export_args(tool_id: &str) -> Vec<String> {
    ["session", "export", "--standalone", tool_id]
        .map(str::to_owned)
        .to_vec()
}

/// Whether OpenCode still has session `tool_id` (it ran in `cwd`).
pub(super) fn can_resume(context: &OpenCodeContext, tool_id: &str, cwd: &Path) -> bool {
    if !tool_id.starts_with("ses") {
        return false;
    }
    run(context, &session_args(tool_id), cwd, CHECK_TIMEOUT)
        .is_some_and(|output| is_session(&output, tool_id))
}

/// Whether an answer to [`session_args`] is that session.
pub(super) fn is_session(output: &str, tool_id: &str) -> bool {
    serde_json::from_str::<Value>(output.trim())
        .ok()
        .and_then(|v| {
            v.pointer("/data/id")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .is_some_and(|id| id == tool_id)
}

pub(super) fn conversation_not_found(output: &[u8]) -> bool {
    String::from_utf8_lossy(output)
        .to_lowercase()
        .contains(CONVERSATION_NOT_FOUND)
}

/// The last `limit` messages and actions of session `tool_id`; `None` when it can't be exported.
pub(super) fn last_entries(
    context: &OpenCodeContext,
    tool_id: &str,
    cwd: &Path,
    limit: usize,
) -> Option<Vec<TranscriptEntry>> {
    if !tool_id.starts_with("ses") {
        return None;
    }
    let output = run(context, &export_args(tool_id), cwd, EXPORT_TIMEOUT)?;
    entries(&output, limit)
}

/// Runs OpenCode with `args` in `cwd` (the folder itself, or the user's home when it is gone) and returns its
/// stdout when it exits 0 within `timeout`; it is killed otherwise.
fn run(
    context: &OpenCodeContext,
    args: &[String],
    cwd: &Path,
    timeout: Duration,
) -> Option<String> {
    let program = crate::tools::find_on_path(COMMAND_NAME, &context.tool.path)?;
    let mut command = crate::tools::cmd_shim::command(&program, &context.tool.path).ok()?;
    command
        .args(args)
        .env("PATH", &context.tool.path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if cwd.is_dir() {
        command.current_dir(cwd);
    } else if let Some(home) = &context.tool.home {
        command.current_dir(home);
    }
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    // Read on a thread so a large answer can't fill the pipe while the time limit is watched.
    let reader = std::thread::spawn(move || {
        let mut text = Vec::new();
        std::io::Read::read_to_end(&mut stdout, &mut text).map(|_| text)
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(25));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let text = reader.join().ok()?.ok()?;
    let text = String::from_utf8_lossy(&text).into_owned();
    tracing::debug!(?args, output = %text, "opencode's answer");
    status.filter(|s| s.success()).map(|_| text)
}

/// The last `limit` entries of an export, oldest first; `None` when it isn't one.
pub(super) fn entries(export: &str, limit: usize) -> Option<Vec<TranscriptEntry>> {
    let value: Value = serde_json::from_str(export.trim()).ok()?;
    let messages = value.get("messages")?.as_array()?;
    let mut kept: VecDeque<TranscriptEntry> = VecDeque::with_capacity(limit + 1);
    for entry in messages.iter().flat_map(entries_of) {
        kept.push_back(entry);
        if kept.len() > limit {
            kept.pop_front();
        }
    }
    Some(kept.into())
}

/// One message's entries: a user's message, the agent's text and the tools it ran (in order), a command the user
/// ran. OpenCode's own records (skills loaded, models switched, idle, synthetic notes) are skipped.
fn entries_of(message: &Value) -> Vec<TranscriptEntry> {
    let at = message.pointer("/time/created").and_then(Value::as_i64);
    let text = |field: &str| message.get(field).and_then(Value::as_str).map(str::trim);
    match text("type") {
        Some("user") => text("text")
            .filter(|t| !t.is_empty())
            .map(|t| TranscriptEntry::User {
                text: cut(t, MAX_TEXT),
                at,
            })
            .into_iter()
            .collect(),
        Some("shell") => text("command")
            .map(|command| TranscriptEntry::Action {
                tool: "shell".to_owned(),
                summary: cut(command, MAX_SUMMARY),
                at,
            })
            .into_iter()
            .collect(),
        Some("assistant") => assistant(message, at),
        _ => Vec::new(),
    }
}

/// The agent's text parts (consecutive ones joined) and its tools, in order.
fn assistant(message: &Value, at: Option<i64>) -> Vec<TranscriptEntry> {
    let mut out = Vec::new();
    let mut said = String::new();
    let flush = |said: &mut String, out: &mut Vec<TranscriptEntry>| {
        let text = said.trim();
        if !text.is_empty() {
            out.push(TranscriptEntry::Assistant {
                text: cut(text, MAX_TEXT),
                at,
            });
        }
        said.clear();
    };
    let parts = message.get("content").and_then(Value::as_array);
    for part in parts.into_iter().flatten() {
        match part.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    if !said.is_empty() {
                        said.push('\n');
                    }
                    said.push_str(text);
                }
            }
            Some("tool") => {
                flush(&mut said, &mut out);
                let tool = part
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_owned();
                out.push(TranscriptEntry::Action {
                    summary: cut(&tool_summary(part), MAX_SUMMARY),
                    tool,
                    at: part.pointer("/time/created").and_then(Value::as_i64).or(at),
                });
            }
            _ => {}
        }
    }
    flush(&mut said, &mut out);
    out
}

/// What a tool looked at or ran: its command, path, pattern or URL, else nothing.
fn tool_summary(part: &Value) -> String {
    let input = part.pointer("/state/input");
    ["command", "path", "filePath", "pattern", "url", "query"]
        .iter()
        .find_map(|field| input?.get(*field)?.as_str())
        .map(|s| s.lines().next().unwrap_or("").trim().to_owned())
        .unwrap_or_default()
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max - 1).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    // The checks' tests are Unix only: the stand-in `opencode` is an `sh` script, which Windows can't run (Q598).
    // On Windows their helpers go unused.
    #![cfg_attr(not(unix), allow(dead_code, unused_imports))]

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::agent_tools::test_support::context;

    fn fixture(name: &str) -> String {
        std::fs::read_to_string(format!(
            "{}/tests/fixtures/opencode/sessions/{name}",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap()
    }

    const ID: &str = "ses_0192f0a1b2c3d4e5f6a7b8c9e1";

    #[test]
    fn the_commands() {
        assert_eq!(
            session_args(ID),
            [
                "api",
                "--standalone",
                "GET",
                "/api/session/ses_0192f0a1b2c3d4e5f6a7b8c9e1"
            ]
        );
        assert_eq!(export_args(ID), ["session", "export", "--standalone", ID]);
    }

    #[test]
    fn the_session_answer() {
        assert!(is_session(&fixture("session-get.json"), ID));
        assert!(!is_session(&fixture("session-get.json"), "ses_other"));
        assert!(!is_session(&fixture("session-get-not-found.txt"), ID));
    }

    /// The text OpenCode's terminal shows for an unknown `--session` (captured), and plain output that doesn't.
    #[test]
    fn an_unknown_session() {
        let capture = std::fs::read(format!(
            "{}/tests/fixtures/opencode/sessions/tui-session-not-found.bin",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap();
        assert!(conversation_not_found(&capture));
        assert!(conversation_not_found(
            fixture("session-get-not-found.txt").as_bytes()
        ));
        assert!(!conversation_not_found(
            b"Build \xc2\xb7 Big Pickle\r\n> work-ticket"
        ));
    }

    /// The export: the user's message, the agent's text before and after its tools, each tool with what it looked
    /// at, a command the user ran; OpenCode's own records skipped; the last `limit` only.
    #[test]
    fn before_the_restart() {
        let all = entries(&fixture("export.json"), 50).unwrap();
        assert_eq!(
            all,
            [
                TranscriptEntry::User {
                    text: "work-ticket".into(),
                    at: Some(1_792_273_771_500)
                },
                TranscriptEntry::Assistant {
                    text: "I'll read the pricing code first.".into(),
                    at: Some(1_792_273_772_000)
                },
                TranscriptEntry::Action {
                    tool: "shell".into(),
                    summary: format!("{} prime", crate::naming::COMMAND_NAME),
                    at: Some(1_792_273_773_000)
                },
                TranscriptEntry::Action {
                    tool: "read".into(),
                    summary: "lib/pricing.ts".into(),
                    at: Some(1_792_273_774_100)
                },
                TranscriptEntry::Action {
                    tool: "edit".into(),
                    summary: "lib/pricing.ts".into(),
                    at: Some(1_792_273_774_200)
                },
                TranscriptEntry::Action {
                    tool: "shell".into(),
                    summary: "pnpm test".into(),
                    at: Some(1_792_273_777_000)
                },
                TranscriptEntry::Assistant {
                    text: "Rounding fixed; tests pass.".into(),
                    at: Some(1_792_273_779_000)
                },
            ]
        );
        assert_eq!(entries(&fixture("export.json"), 2).unwrap(), all[5..]);
        assert_eq!(entries("not json", 5), None);
        assert_eq!(
            entries(r#"{"info": {}, "messages": []}"#, 5),
            Some(Vec::new())
        );
    }

    /// A stand-in `opencode` on the test PATH that prints `stdout` and exits with `code`, recording its arguments
    /// and folder.
    #[cfg(unix)]
    fn stand_in(bin: &Path, stdout: &str, code: i32) {
        let file = bin.join("opencode");
        let body = stdout.replace('\'', "'\\''");
        std::fs::write(
            &file,
            format!(
                "#!/bin/sh\necho \"$@\" > {dir}/args\npwd > {dir}/cwd\nprintf '%s' '{body}'\nexit {code}\n",
                dir = bin.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn the_resume_check_and_the_export_run_opencode_in_the_folder() {
        let bin = tempfile::tempdir().unwrap();
        let worktree = tempfile::tempdir().unwrap();
        let context = OpenCodeContext::new(context(bin.path(), None));
        assert!(!can_resume(&context, ID, worktree.path()), "not on PATH");

        stand_in(bin.path(), &fixture("session-get.json"), 0);
        assert!(can_resume(&context, ID, worktree.path()));
        assert_eq!(
            std::fs::read_to_string(bin.path().join("args"))
                .unwrap()
                .trim(),
            format!("api --standalone GET /api/session/{ID}")
        );
        assert_eq!(
            std::fs::canonicalize(
                std::fs::read_to_string(bin.path().join("cwd"))
                    .unwrap()
                    .trim()
            )
            .unwrap(),
            std::fs::canonicalize(worktree.path()).unwrap()
        );
        assert!(!can_resume(&context, "not-a-session", worktree.path()));

        stand_in(bin.path(), &fixture("session-get-not-found.txt"), 1);
        assert!(!can_resume(&context, ID, worktree.path()));

        stand_in(bin.path(), &fixture("export.json"), 0);
        let last = last_entries(&context, ID, worktree.path(), 1).unwrap();
        assert_eq!(
            last,
            [TranscriptEntry::Assistant {
                text: "Rounding fixed; tests pass.".into(),
                at: Some(1_792_273_779_000)
            }]
        );
        assert_eq!(
            std::fs::read_to_string(bin.path().join("args"))
                .unwrap()
                .trim(),
            format!("session export --standalone {ID}")
        );
        stand_in(bin.path(), "Session not found", 1);
        assert_eq!(last_entries(&context, ID, worktree.path(), 5), None);
    }

    /// A check that hangs is ended at its time limit.
    #[cfg(unix)]
    #[test]
    fn a_hanging_check_is_ended() {
        let bin = tempfile::tempdir().unwrap();
        let file = bin.path().join("opencode");
        std::fs::write(&file, "#!/bin/sh\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        let context = OpenCodeContext::new(context(bin.path(), None));
        let started = Instant::now();
        let args = session_args(ID);
        assert_eq!(
            run(&context, &args, bin.path(), Duration::from_millis(300)),
            None
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }
}
