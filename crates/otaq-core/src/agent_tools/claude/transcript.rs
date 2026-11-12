//! Claude Code's saved conversations (USG-01, USG-09, REC-02; d0020, d0045, d0088), read **read-only**: the tokens
//! of every response, and the last messages and actions of a conversation for "Before the restart". Otaq never
//! copies terminal output to disk, and nothing here writes, moves or deletes anything.
//!
//! Claude Code keeps each conversation as JSON lines in `<config>/projects/<folder>/<session-id>.jsonl`, where
//! `<config>` is `$CLAUDE_CONFIG_DIR` or `~/.claude` and `<folder>` is the session's working directory with every
//! character other than a letter or digit replaced by `-` (checked against Claude Code 2.1.296; very long paths
//! get a hash suffix, so the session's file is also looked for in every project folder). The conversations of its
//! helper agents are in `<folder>/<session-id>/subagents/agent-<id>.jsonl`. A record has a `type` (`user`,
//! `assistant`, and others) and a `message` whose `content` is a string or a list of blocks (`text`, `thinking`,
//! `tool_use`, `tool_result`).
//!
//! **Tokens.** Every API response is an `assistant` record whose `message` carries the response `id`, the `model`
//! and a `usage` object with `input_tokens`, `output_tokens`, `cache_creation_input_tokens` and
//! `cache_read_input_tokens`. They map onto Otaq's [`Tokens`] without counting anything twice: `input` is
//! `input_tokens + cache_creation_input_tokens` (input not read from a cache, cache writes included), `cached` is
//! `cache_read_input_tokens`, `output` is `output_tokens`, and `reasoning` is `None` (Claude Code counts thinking
//! in its output). A response with several content blocks is written as several records with the same
//! `message.id` and the same `usage`, so counts are kept **per response id**: a repeated id replaces its earlier
//! counts instead of adding to them. Files only grow, so a [`TranscriptFile`] remembers how far it has read and
//! reads only the lines added since; a line that isn't complete yet is left for the next read. A line that isn't
//! JSON is skipped. A file none of whose lines is a Claude Code record (an object with a `type`) is
//! **unrecognised**: like an unreadable file it makes the conversation's tokens unknown.
//!
//! **Before the restart.** Records of helper agents (`isSidechain`) and meta records are skipped. Like usage
//! (d0020), a change of this format may break this view, never a workflow: unknown lines and shapes are skipped,
//! and a missing or unreadable file is "not available".

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::agent_tools::{TokenReader, TokenStep, TokensUnknown, TranscriptEntry};
use crate::usage::Tokens;

/// Messages are cut after this many characters.
const MAX_TEXT: usize = 2000;
/// Action summaries are cut after this many characters.
const MAX_SUMMARY: usize = 120;

/// Claude Code's folder name for a working directory.
pub fn project_folder(cwd: &Path) -> String {
    cwd.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

/// Where Claude Code writes the conversation `session_id` of a session that ran in `cwd` (tests build
/// transcripts there).
pub fn transcript_path(config_dir: &Path, cwd: &Path, session_id: &str) -> PathBuf {
    config_dir
        .join("projects")
        .join(project_folder(cwd))
        .join(format!("{session_id}.jsonl"))
}

/// Where Claude Code keeps the conversation `session_id` of a session that ran in `cwd`: the expected folder
/// first, else any project folder holding that file. `None` when there is none.
pub fn find_transcript(config_dir: &Path, cwd: &Path, session_id: &str) -> Option<PathBuf> {
    // A session ID is a UUID; anything else never becomes a path.
    if session_id.is_empty()
        || !session_id
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-')
    {
        return None;
    }
    let projects = config_dir.join("projects");
    let file = format!("{session_id}.jsonl");
    let mut candidates = vec![transcript_path(config_dir, cwd, session_id)];
    if let Ok(canonical) = crate::paths::canonical(cwd) {
        candidates.push(transcript_path(config_dir, &canonical, session_id));
    }
    if let Some(found) = candidates.into_iter().find(|p| p.is_file()) {
        return Some(found);
    }
    std::fs::read_dir(&projects)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join(&file))
        .find(|p| p.is_file())
}

/// Reads the last `limit` entries of a transcript file. `None` when it can't be read.
pub fn read_transcript(path: &Path, limit: usize) -> Option<Vec<TranscriptEntry>> {
    let text = std::fs::read(path).ok()?;
    Some(last_entries(&String::from_utf8_lossy(&text), limit))
}

/// The last `limit` entries of a transcript's lines, oldest first. Lines that aren't understood are skipped.
pub fn last_entries(jsonl: &str, limit: usize) -> Vec<TranscriptEntry> {
    let mut kept: VecDeque<TranscriptEntry> = VecDeque::with_capacity(limit + 1);
    for line in jsonl.lines() {
        for entry in entries_of(line) {
            kept.push_back(entry);
            if kept.len() > limit {
                kept.pop_front();
            }
        }
    }
    kept.into()
}

/// The entries of one record.
fn entries_of(line: &str) -> Vec<TranscriptEntry> {
    let Ok(record) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let flag = |name: &str| record.get(name).and_then(Value::as_bool) == Some(true);
    if flag("isSidechain") || flag("isMeta") {
        return Vec::new();
    }
    let kind = record.get("type").and_then(Value::as_str);
    let at = record
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(iso_ms);
    let Some(content) = record.get("message").and_then(|m| m.get("content")) else {
        return Vec::new();
    };
    match kind {
        Some("user") => user_text(content)
            .map(|text| vec![TranscriptEntry::User { text, at }])
            .unwrap_or_default(),
        Some("assistant") => assistant_entries(content, at),
        _ => Vec::new(),
    }
}

/// A user record's text; tool results and Claude Code's own wrappers (reminders, command output) are skipped,
/// a slash command shows as its name.
fn user_text(content: &Value) -> Option<String> {
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let text = without_paste_tags(&text);
    let text = text.trim();
    if let Some(name) = between(text, "<command-name>", "</command-name>") {
        let args = between(text, "<command-args>", "</command-args>").unwrap_or_default();
        return Some(cut(format!("{name} {args}").trim(), MAX_TEXT));
    }
    if text.is_empty() || text.starts_with('<') {
        return None;
    }
    Some(cut(text, MAX_TEXT))
}

fn assistant_entries(content: &Value, at: Option<i64>) -> Vec<TranscriptEntry> {
    let blocks: Vec<Value> = match content {
        Value::String(s) => vec![serde_json::json!({ "type": "text", "text": s })],
        Value::Array(blocks) => blocks.clone(),
        _ => return Vec::new(),
    };
    blocks
        .iter()
        .filter_map(|block| match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                let text = block.get("text").and_then(Value::as_str)?.trim();
                (!text.is_empty()).then(|| TranscriptEntry::Assistant {
                    text: cut(text, MAX_TEXT),
                    at,
                })
            }
            Some("tool_use") => {
                let tool = block.get("name").and_then(Value::as_str)?.to_owned();
                let summary = action_summary(&tool, block.get("input").unwrap_or(&Value::Null));
                Some(TranscriptEntry::Action { tool, summary, at })
            }
            _ => None,
        })
        .collect()
}

/// A tool call in a few words: the command, the file, the pattern or the task.
fn action_summary(tool: &str, input: &Value) -> String {
    let field = |name: &str| input.get(name).and_then(Value::as_str).map(str::trim);
    let preferred = match tool {
        "Bash" => field("command"),
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            field("file_path").or(field("notebook_path"))
        }
        "Grep" | "Glob" => field("pattern"),
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        "Task" | "Agent" => field("description"),
        _ => None,
    };
    let any = || {
        input
            .as_object()?
            .values()
            .find_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
    };
    let summary = preferred.or_else(any).unwrap_or_default();
    cut(summary.lines().next().unwrap_or_default(), MAX_SUMMARY)
}

/// The opening of the tag Claude Code wraps pasted text in.
const PASTE_OPEN: &str = "<pasted_content";
/// The opening of its closing tag.
const PASTE_CLOSE: &str = "</pasted_content";

/// `text` with Claude Code's paste tags taken out, the pasted text kept. Claude Code (2.1.322) records a paste of
/// over 800 characters, which every long message Otaq types is, as `\n\n<pasted_content id="…">\n` + the text +
/// `\n</pasted_content id="…">\n` (checked against what the real tool saved on Windows and macOS). Text without
/// the tags is returned as it is.
pub fn without_paste_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let open = rest.find(PASTE_OPEN);
        let close = rest.find(PASTE_CLOSE);
        let (at, closing) = match (open, close) {
            (Some(o), Some(c)) if c < o => (c, true),
            (Some(o), _) => (o, false),
            (None, Some(c)) => (c, true),
            (None, None) => break,
        };
        let Some(end) = rest[at..].find('>').map(|e| at + e + 1) else {
            break;
        };
        if closing {
            out.push_str(rest[..at].strip_suffix('\n').unwrap_or(&rest[..at]));
            rest = rest[end..].trim_start_matches('\n');
        } else {
            out.push_str(rest[..at].trim_end_matches('\n'));
            rest = rest[end..].strip_prefix('\n').unwrap_or(&rest[end..]);
        }
    }
    out.push_str(rest);
    out
}

fn between<'a>(text: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = text.find(open)? + open.len();
    let end = text[start..].find(close)? + start;
    Some(text[start..end].trim())
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut cut: String = text.chars().take(max).collect();
    cut.push('…');
    cut
}

/// `2026-10-09T12:34:56.789Z` (UTC, as Claude Code writes it) in Unix milliseconds.
pub fn iso_ms(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let mut d = date.splitn(3, '-').map(|p| p.parse::<i64>().ok());
    let (year, month, day) = (d.next()??, d.next()??, d.next()??);
    let (hms, frac) = time.split_once('.').unwrap_or((time, "0"));
    let mut t = hms.splitn(3, ':').map(|p| p.parse::<i64>().ok());
    let (hour, minute, second) = (t.next()??, t.next()??, t.next()??);
    let millis: i64 = format!("{frac:0<3}").get(..3)?.parse().ok()?;
    // Days from 1970-01-01 in the proleptic Gregorian calendar (Howard Hinnant's days_from_civil).
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

/// One API response's counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageUsage {
    /// `message.id`, else the record's `uuid`.
    pub id: String,
    /// When the record was written (Unix milliseconds).
    pub at: Option<i64>,
    pub model: Option<String>,
    pub tokens: Tokens,
}

/// What one line of a transcript is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    /// An `assistant` record with usage.
    Usage(MessageUsage),
    /// Another Claude Code record (user message, bookkeeping, …).
    Record,
    /// Not JSON, or not a record.
    Unrecognised,
}

/// Reads one transcript line.
pub fn parse_line(line: &str) -> Line {
    let Ok(record) = serde_json::from_str::<Value>(line) else {
        return Line::Unrecognised;
    };
    let Some(kind) = record.get("type").and_then(Value::as_str) else {
        return Line::Unrecognised;
    };
    if kind != "assistant" {
        return Line::Record;
    }
    let message = record.get("message");
    let Some(usage) = message
        .and_then(|m| m.get("usage"))
        .filter(|u| u.is_object())
    else {
        return Line::Record;
    };
    let count = |field: &str| usage.get(field).and_then(Value::as_u64).unwrap_or(0);
    let text = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_owned);
    let Some(id) = text(message.and_then(|m| m.get("id"))).or_else(|| text(record.get("uuid")))
    else {
        return Line::Record;
    };
    Line::Usage(MessageUsage {
        id,
        at: record
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(iso_ms),
        model: text(message.and_then(|m| m.get("model"))),
        tokens: Tokens {
            input: count("input_tokens") + count("cache_creation_input_tokens"),
            cached: count("cache_read_input_tokens"),
            output: count("output_tokens"),
            reasoning: None,
        },
    })
}

/// One transcript file and how far it has been read.
#[derive(Debug, Clone, Default)]
pub struct TranscriptFile {
    /// Bytes read so far (always the end of a complete line).
    offset: u64,
    /// At least one line was a Claude Code record.
    recognised: bool,
    /// At least one complete line was read.
    any_line: bool,
}

impl TranscriptFile {
    /// Reads the lines added since the last read and hands every response's counts to `found`. A file that got
    /// shorter (rewritten) is read again from the start; repeated response ids replace, so nothing counts twice.
    pub fn read_new(
        &mut self,
        path: &Path,
        found: &mut impl FnMut(MessageUsage),
    ) -> io::Result<()> {
        let mut file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        if len < self.offset {
            *self = Self::default();
        }
        if len == self.offset {
            return Ok(());
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut added = Vec::new();
        file.take(len - self.offset).read_to_end(&mut added)?;
        // Only complete lines: Claude Code may be writing the last one right now.
        let Some(end) = added.iter().rposition(|b| *b == b'\n') else {
            return Ok(());
        };
        for line in String::from_utf8_lossy(&added[..end]).lines() {
            if line.trim().is_empty() {
                continue;
            }
            self.any_line = true;
            match parse_line(line) {
                Line::Usage(usage) => {
                    self.recognised = true;
                    found(usage);
                }
                Line::Record => self.recognised = true,
                Line::Unrecognised => {}
            }
        }
        self.offset += end as u64 + 1;
        Ok(())
    }

    /// Lines were read and none was a Claude Code record.
    pub fn unrecognised(&self) -> bool {
        self.any_line && !self.recognised
    }
}

/// One Claude conversation (a stored Claude session ID): its files and every response's counts.
#[derive(Debug, Clone, Default)]
pub struct Conversation {
    /// The main transcript and its helper agents' transcripts.
    files: BTreeMap<PathBuf, TranscriptFile>,
    /// Counts per response id.
    messages: HashMap<String, MessageUsage>,
}

impl Conversation {
    /// Reads what was added to `main` and its helper agents' files since the last read. Returns whether any
    /// counts changed, or why the usage is unknown.
    pub fn read(&mut self, main: &Path) -> Result<bool, TokensUnknown> {
        let mut paths = vec![main.to_owned()];
        if let (Some(dir), Some(stem)) = (main.parent(), main.file_stem())
            && let Ok(entries) = std::fs::read_dir(dir.join(stem).join("subagents"))
        {
            let mut helpers: Vec<PathBuf> = entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
                .collect();
            helpers.sort();
            paths.extend(helpers);
        }
        let mut changed = false;
        let messages = &mut self.messages;
        let mut found = |usage: MessageUsage| {
            if messages.get(&usage.id) != Some(&usage) {
                changed = true;
                messages.insert(usage.id.clone(), usage);
            }
        };
        for path in &paths {
            let file = self.files.entry(path.clone()).or_default();
            let read = file.read_new(path, &mut found);
            if path == main {
                if let Err(e) = read {
                    return Err(TokensUnknown::Unreadable(e.to_string()));
                }
                if file.unrecognised() {
                    return Err(TokensUnknown::Unrecognised);
                }
            } else if let Err(e) = read {
                tracing::debug!(path = %path.display(), error = %e, "helper agent transcript unreadable");
            }
        }
        Ok(changed)
    }

    /// Every response's counts.
    pub fn messages(&self) -> impl Iterator<Item = &MessageUsage> {
        self.messages.values()
    }
}

/// One Claude conversation's token counts, from its transcript and its helper agents' files: the adapter's
/// [`TokenReader`]. The transcript is looked for until it is found, and again if it disappears.
pub(super) struct Transcript {
    claude_dir: Option<PathBuf>,
    cwd: PathBuf,
    tool_id: String,
    /// The transcript, once found.
    path: Option<PathBuf>,
    conversation: Conversation,
}

impl Transcript {
    pub(super) fn new(claude_dir: Option<PathBuf>, cwd: PathBuf, tool_id: String) -> Self {
        Self {
            claude_dir,
            cwd,
            tool_id,
            path: None,
            conversation: Conversation::default(),
        }
    }
}

impl TokenReader for Transcript {
    fn read(&mut self) -> Result<bool, TokensUnknown> {
        let Some(claude_dir) = &self.claude_dir else {
            return Err(TokensUnknown::Missing);
        };
        if self.path.as_ref().is_none_or(|p| !p.is_file()) {
            self.path = find_transcript(claude_dir, &self.cwd, &self.tool_id);
        }
        let Some(path) = &self.path else {
            return Err(TokensUnknown::Missing);
        };
        self.conversation.read(path)
    }

    fn steps(&self) -> Vec<TokenStep> {
        self.conversation
            .messages()
            .map(|m| TokenStep {
                id: m.id.clone(),
                at: m.at,
                model: m.model.clone(),
                tokens: m.tokens,
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_follow_claude_code() {
        assert_eq!(
            project_folder(Path::new("/Users/nevzat/.data/wt/shop_web/SHOP-25")),
            "-Users-nevzat--data-wt-shop-web-SHOP-25"
        );
    }

    /// A long message Otaq typed is saved inside Claude Code's paste tags (one block, or several when the terminal's
    /// reads were each taken as a paste): the user's turn shows the message alone. The shapes are what the real
    /// Claude Code 2.1.322 saved.
    #[test]
    fn a_pasted_message_shows_without_the_paste_tags() {
        let turn = |content: &str| user_text(&Value::String(content.to_owned()));
        assert_eq!(
            turn("<pasted_content id=\"34fa\">\nDo nothing; reply OK.\n</pasted_content id=\"34fa\">\n").as_deref(),
            Some("Do nothing; reply OK.")
        );
        assert_eq!(
            turn(
                "\n\n<pasted_content id=\"0f28\">\nDo noth\n</pasted_content id=\"0f28\">\n\n\n\
                 <pasted_content id=\"0f28\">\ning; reply\nOK.\n</pasted_content id=\"0f28\">\n"
            )
            .as_deref(),
            Some("Do nothing; reply\nOK.")
        );
        assert_eq!(turn("plain").as_deref(), Some("plain"));
        assert_eq!(turn("<system-reminder>x</system-reminder>"), None);
    }

    #[test]
    fn timestamps() {
        assert_eq!(iso_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(iso_ms("2026-10-09T12:34:56.789Z"), Some(1_791_549_296_789));
        assert_eq!(iso_ms("2000-02-29T00:00:01Z"), Some(951_782_401_000));
        assert_eq!(iso_ms("yesterday"), None);
    }

    #[test]
    fn summaries_name_the_command_file_or_task() {
        let s = |tool: &str, input: Value| action_summary(tool, &input);
        assert_eq!(
            s("Bash", serde_json::json!({"command": "pnpm test\nmore"})),
            "pnpm test"
        );
        assert_eq!(
            s(
                "Edit",
                serde_json::json!({"file_path": "/wt/lib/pricing.ts", "old_string": "a"})
            ),
            "/wt/lib/pricing.ts"
        );
        assert_eq!(s("Grep", serde_json::json!({"pattern": "TODO"})), "TODO");
        assert_eq!(s("mcp__x__y", serde_json::json!({"q": "hello"})), "hello");
        assert_eq!(s("Unknown", serde_json::json!({})), "");
    }

    #[test]
    fn assistant_records_carry_usage_and_others_are_records() {
        let line = r#"{"type":"assistant","timestamp":"1970-01-01T00:00:01.000Z","uuid":"u1","message":{"id":"msg_1","model":"claude-opus-5-5","role":"assistant","content":[],"usage":{"input_tokens":2,"cache_creation_input_tokens":11524,"cache_read_input_tokens":28134,"output_tokens":226,"service_tier":"standard","cache_creation":{"ephemeral_1h_input_tokens":11524,"ephemeral_5m_input_tokens":0}}}}"#;
        assert_eq!(
            parse_line(line),
            Line::Usage(MessageUsage {
                id: "msg_1".into(),
                at: Some(1000),
                model: Some("claude-opus-5-5".into()),
                tokens: Tokens {
                    input: 2 + 11524,
                    cached: 28134,
                    output: 226,
                    reasoning: None
                },
            })
        );
        assert_eq!(
            parse_line(r#"{"type":"user","message":{"content":"hi"}}"#),
            Line::Record
        );
        assert_eq!(
            parse_line(r#"{"type":"assistant","message":{"content":[]}}"#),
            Line::Record
        );
        assert_eq!(parse_line("not json"), Line::Unrecognised);
        assert_eq!(parse_line("[1, 2]"), Line::Unrecognised);
    }

    /// The shown number is today's (Q26): input + output + cache writes, cache reads left out. Before T-0312 the
    /// response below showed 2 + 226 + 11,524 = 11,752 (`input + output + cache_write`); folding the cache writes
    /// into `input` gives the same number.
    #[test]
    fn cache_writes_fold_into_input_and_the_shown_number_stays() {
        let line = r#"{"type":"assistant","timestamp":"2026-10-17T09:00:00.000Z","message":{"id":"msg_2","model":"claude-opus-5-5","usage":{"input_tokens":2,"cache_creation_input_tokens":11524,"cache_read_input_tokens":28134,"output_tokens":226}}}"#;
        let Line::Usage(usage) = parse_line(line) else {
            panic!("an assistant record with usage");
        };
        let (input, cache_write, output) = (2, 11_524, 226);
        assert_eq!(usage.tokens.shown(), input + output + cache_write);
        assert_eq!(usage.tokens.shown(), 11_752);
        assert_eq!(usage.tokens.cached, 28_134);
    }
}
