//! "Before the restart" (REC-02; d0045, d0088): the last messages and commands of a conversation, read read-only
//! from its session file (`<CODEX_HOME>/sessions/…/rollout-…-<id>.jsonl`, [`super::launch::session_file`]).
//!
//! Each line is `{"timestamp": "…Z", "type": …, "payload": …}`. What is shown comes from the `response_item` lines,
//! the conversation as the model saw it: a `message` from the `user` (not Codex's own context, which it wraps in
//! tags: `<environment_context>`, AGENTS.md) or the `assistant`, and the agent's tool calls (`function_call`,
//! `custom_tool_call`, `local_shell_call`), whose command is the summary. `event_msg` lines repeat the same messages
//! and are skipped; so is everything else (`session_meta`, `turn_context`, the developer instructions).

use std::collections::VecDeque;
use std::path::Path;

use serde_json::Value;

use super::CodexContext;
use super::launch::session_file;
use crate::agent_tools::TranscriptEntry;
use crate::agent_tools::claude::transcript::iso_ms;

/// The longest message kept, in characters (as Claude Code's).
const MAX_TEXT: usize = 2000;

/// The longest action summary, in characters.
const MAX_SUMMARY: usize = 120;

pub(super) fn last_entries(
    context: &CodexContext,
    tool_id: &str,
    _cwd: &Path,
    limit: usize,
) -> Option<Vec<TranscriptEntry>> {
    let file = session_file(context.codex_home.as_deref()?, tool_id)?;
    let text = std::fs::read(file).ok()?;
    Some(entries(&String::from_utf8_lossy(&text), limit))
}

/// The last `limit` entries of a session file's lines, oldest first. Lines that aren't understood are skipped.
pub(super) fn entries(jsonl: &str, limit: usize) -> Vec<TranscriptEntry> {
    let mut kept: VecDeque<TranscriptEntry> = VecDeque::with_capacity(limit + 1);
    for entry in jsonl.lines().filter_map(entry_of) {
        kept.push_back(entry);
        if kept.len() > limit {
            kept.pop_front();
        }
    }
    kept.into()
}

fn entry_of(line: &str) -> Option<TranscriptEntry> {
    let record: Value = serde_json::from_str(line).ok()?;
    if record.get("type").and_then(Value::as_str) != Some("response_item") {
        return None;
    }
    let at = record
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(iso_ms);
    let item = record.get("payload")?;
    let field = |name: &str| item.get(name).and_then(Value::as_str);
    match field("type")? {
        "message" => {
            let text = message_text(item.get("content")?)?;
            match field("role")? {
                "user" if !text.starts_with('<') && !text.starts_with("# AGENTS.md") => {
                    Some(TranscriptEntry::User {
                        text: cut(&text, MAX_TEXT),
                        at,
                    })
                }
                "assistant" => Some(TranscriptEntry::Assistant {
                    text: cut(&text, MAX_TEXT),
                    at,
                }),
                _ => None,
            }
        }
        "function_call" | "custom_tool_call" | "local_shell_call" => {
            let tool = field("name").unwrap_or("shell").to_owned();
            let summary = action_summary(item);
            Some(TranscriptEntry::Action { tool, summary, at })
        }
        _ => None,
    }
}

/// The text blocks of a message (`input_text`, `output_text`), joined; `None` when empty.
fn message_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| {
                matches!(
                    b.get("type").and_then(Value::as_str),
                    Some("input_text" | "output_text" | "text")
                )
            })
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// A tool call in a few words: its command (`cmd` as a string, or `command` as a string or a list of words, in the
/// arguments, which `function_call` carries as JSON text), else its first text argument.
fn action_summary(item: &Value) -> String {
    let arguments = match item.get("arguments").or_else(|| item.get("input")) {
        Some(Value::String(text)) => {
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.clone()))
        }
        Some(value) => value.clone(),
        None => item.get("action").cloned().unwrap_or(Value::Null),
    };
    let command = |value: &Value| match value {
        Value::String(text) => Some(text.trim().to_owned()),
        Value::Array(words) => {
            let words: Vec<&str> = words.iter().filter_map(Value::as_str).collect();
            // `["bash", "-lc", "pnpm test"]`: the script is what was run.
            match words.as_slice() {
                [_, flag, script] if flag.starts_with('-') && flag.ends_with('c') => {
                    Some((*script).to_owned())
                }
                _ => Some(words.join(" ")),
            }
        }
        _ => None,
    };
    let summary = match &arguments {
        Value::String(text) => Some(text.trim().to_owned()),
        Value::Object(fields) => ["cmd", "command"]
            .iter()
            .find_map(|name| fields.get(*name).and_then(command))
            .or_else(|| {
                fields
                    .values()
                    .find_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
                    .map(str::to_owned)
            }),
        _ => None,
    }
    .unwrap_or_default();
    cut(summary.lines().next().unwrap_or_default(), MAX_SUMMARY)
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_tools::AgentTool;
    use crate::agent_tools::codex::Codex;
    use crate::naming;

    /// A session file in the shape of the one `codex exec` wrote with `CODEX_HOME` in a temporary folder (0.157.1:
    /// `session_meta`, `event_msg`, the developer instructions, the `<environment_context>` and the user's message as
    /// `response_item` lines, then `task_complete`), shortened; the assistant's message and the three kinds of tool
    /// call are written from Codex's response items (T-0369 replaces them with a capture).
    const SESSION: &str = r#"{"timestamp":"2026-10-17T21:05:13.642Z","ordinal":0,"type":"session_meta","payload":{"session_id":"01a14bae-dd63-7c03-b96e-8f7557586707","id":"01a14bae-dd63-7c03-b96e-8f7557586707","cwd":"/wt/shop-web/SHOP-18","originator":"codex_tui","cli_version":"0.157.1","source":"cli"}}
{"timestamp":"2026-10-17T21:05:13.643Z","ordinal":1,"type":"event_msg","payload":{"type":"task_started","turn_id":"01a14bae-dda3-7832-a0a2-3a72ce2ec854","model_context_window":258400}}
{"timestamp":"2026-10-17T21:05:13.963Z","ordinal":2,"type":"response_item","payload":{"type":"message","id":"msg_1","role":"developer","content":[{"type":"input_text","text":"<skills_instructions>\n## Skills\n</skills_instructions>"}]}}
{"timestamp":"2026-10-17T21:05:13.963Z","ordinal":5,"type":"response_item","payload":{"type":"message","id":"msg_2","role":"user","content":[{"type":"input_text","text":"<environment_context>\n  <cwd>/wt/shop-web/SHOP-18</cwd>\n</environment_context>"}]}}
{"timestamp":"2026-10-17T21:05:13.974Z","ordinal":8,"type":"response_item","payload":{"type":"message","id":"msg_3","role":"user","content":[{"type":"input_text","text":"@SKILL@"}]},"metadata":{"client_authored":false,"user_input_order":0}}
{"timestamp":"2026-10-17T21:05:13.975Z","ordinal":9,"type":"event_msg","payload":{"type":"item_completed","item":{"type":"UserMessage","content":[{"type":"text","text":"@SKILL@"}]}}}
{"timestamp":"2026-10-17T21:05:15.100Z","ordinal":10,"type":"response_item","payload":{"type":"message","id":"msg_4","role":"assistant","content":[{"type":"output_text","text":"I'll read the ticket first."}]}}
{"timestamp":"2026-10-17T21:05:16.000Z","ordinal":11,"type":"response_item","payload":{"type":"function_call","name":"exec_command","arguments":"{\"cmd\":\"@COMMAND@ show SHOP-18\",\"workdir\":\"/wt/shop-web/SHOP-18\"}","call_id":"call_1"}}
{"timestamp":"2026-10-17T21:05:17.000Z","ordinal":12,"type":"response_item","payload":{"type":"function_call_output","call_id":"call_1","output":"SHOP-18 Coupon codes"}}
{"timestamp":"2026-10-17T21:05:18.000Z","ordinal":13,"type":"response_item","payload":{"type":"function_call","name":"shell","arguments":"{\"command\":[\"bash\",\"-lc\",\"pnpm test\"]}","call_id":"call_2"}}
{"timestamp":"2026-10-17T21:05:19.000Z","ordinal":14,"type":"response_item","payload":{"type":"local_shell_call","call_id":"call_3","status":"completed","action":{"type":"exec","command":["git","status"]}}}
{"timestamp":"2026-10-17T21:05:20.000Z","ordinal":15,"type":"response_item","payload":{"type":"custom_tool_call","name":"apply_patch","input":"*** Begin Patch\n*** Update File: lib/pricing.ts\n","call_id":"call_4"}}
not json
{"timestamp":"2026-10-17T21:05:29.629Z","ordinal":16,"type":"event_msg","payload":{"type":"task_complete","turn_id":"01a14bae-dda3-7832-a0a2-3a72ce2ec854","last_agent_message":null,"error":{"message":"unexpected status 401 Unauthorized","codex_error_info":"other"}}}
"#;

    const ID: &str = "01a14bae-dd63-7c03-b96e-8f7557586707";

    /// [`SESSION`] with Otaq's names filled in (they are spelled out only in `naming`).
    fn session() -> String {
        SESSION
            .replace(
                "@SKILL@",
                &format!("${}:work-ticket", naming::PLUGIN_NAMESPACE),
            )
            .replace("@COMMAND@", naming::COMMAND_NAME)
    }

    fn at(s: &str) -> Option<i64> {
        iso_ms(s)
    }

    #[test]
    fn the_messages_and_commands_of_a_session_file() {
        let skill = format!("${}:work-ticket", naming::PLUGIN_NAMESPACE);
        assert_eq!(
            entries(&session(), 40),
            [
                TranscriptEntry::User {
                    text: skill,
                    at: at("2026-10-17T21:05:13.974Z")
                },
                TranscriptEntry::Assistant {
                    text: "I'll read the ticket first.".into(),
                    at: at("2026-10-17T21:05:15.100Z")
                },
                TranscriptEntry::Action {
                    tool: "exec_command".into(),
                    summary: format!("{} show SHOP-18", naming::COMMAND_NAME),
                    at: at("2026-10-17T21:05:16.000Z")
                },
                TranscriptEntry::Action {
                    tool: "shell".into(),
                    summary: "pnpm test".into(),
                    at: at("2026-10-17T21:05:18.000Z")
                },
                TranscriptEntry::Action {
                    tool: "shell".into(),
                    summary: "git status".into(),
                    at: at("2026-10-17T21:05:19.000Z")
                },
                TranscriptEntry::Action {
                    tool: "apply_patch".into(),
                    summary: "*** Begin Patch".into(),
                    at: at("2026-10-17T21:05:20.000Z")
                },
            ]
        );
        let last = entries(&session(), 2);
        assert_eq!(last.len(), 2);
        assert!(matches!(&last[1], TranscriptEntry::Action { tool, .. } if tool == "apply_patch"));
    }

    /// Read from the session file in Codex's home folder; `None` when there is none.
    #[test]
    fn last_entries_from_the_saved_session_file() {
        let home = tempfile::tempdir().unwrap();
        let tool = crate::agent_tools::test_support::context(home.path(), None);
        let adapter = Codex::with_context(CodexContext {
            tool,
            codex_home: Some(home.path().to_path_buf()),
        });
        let cwd = Path::new("/wt/shop-web/SHOP-18");
        assert_eq!(adapter.last_entries(ID, cwd, 40), None);
        let day = home.path().join("sessions/2026/10/18");
        std::fs::create_dir_all(&day).unwrap();
        std::fs::write(
            day.join(format!("rollout-2026-10-18T00-05-13-{ID}.jsonl")),
            session(),
        )
        .unwrap();
        let read = adapter.last_entries(ID, cwd, 3).unwrap();
        assert_eq!(read, entries(&session(), 3));
    }
}
