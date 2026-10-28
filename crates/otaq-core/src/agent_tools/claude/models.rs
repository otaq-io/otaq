//! Claude Code's model list (AGT-02; d0086, d0088): the SDK's `initialize` control request.
//!
//! `claude -p --input-format stream-json --output-format stream-json --verbose` with
//! `{"type":"control_request","request_id":"r1","request":{"subtype":"initialize"}}` on stdin answers with a
//! `control_response` whose `response.response.models[]` lists what Claude Code can run: `value` (what is
//! passed), `displayName`, `description`, `resolvedModel` (the exact model today) and `supportedEffortLevels`.
//! It makes no model call and needs no sign-in or network (`reports/Models across the four tools (2026-10-17).md`,
//! Claude Code row 2; the field names checked in the installed binary 2.1.302's SDK schema). No `--model` is ever
//! passed to it: Claude Code would add that id to the list as an entry of its own.
//!
//! Started only by the service ([`super::super::models::ModelLists`]); tests parse a saved response
//! (`tests/fixtures/claude/models/`).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::agent_tools::models::{ModelEntry, ModelKind, ModelList, ModelListError};

/// The longest the list may take (it answers in about half a second).
pub const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// The id of Otaq's one request.
const REQUEST_ID: &str = "r1";

/// Claude Code's name for its own default model: shown as "Tool default", passed as nothing.
const TOOL_DEFAULT: &str = "default";

/// What the list request runs: `claude`'s arguments and the line written to its stdin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListCommand {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: String,
}

/// The list request for `program` (`["claude"]`, or what stands for it).
pub fn list_command(program: &[String]) -> ListCommand {
    let (program, leading) = program.split_first().map_or_else(
        || (super::command_name().to_owned(), &[][..]),
        |(p, rest)| (p.clone(), rest),
    );
    let mut args = leading.to_vec();
    args.extend(
        [
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
        ]
        .map(str::to_owned),
    );
    ListCommand {
        program,
        args,
        stdin: format!(
            "{{\"type\":\"control_request\",\"request_id\":\"{REQUEST_ID}\",\"request\":{{\"subtype\":\"initialize\"}}}}\n"
        ),
    }
}

/// Why the list wasn't read, as the error people see.
fn failed(reason: impl Into<String>) -> ModelListError {
    ModelListError {
        message: "Claude Code didn't list its models".to_owned(),
        reason: reason.into(),
    }
}

/// Whether `line` is the answer to Otaq's request.
fn is_answer(line: &Value) -> bool {
    line.get("type").and_then(Value::as_str) == Some("control_response")
        && line.pointer("/response/request_id").and_then(Value::as_str) == Some(REQUEST_ID)
}

/// The list in the `control_response` line `response`. An error answer, or one without `models`, is an error.
pub fn parse_response(response: &Value) -> Result<ModelList, ModelListError> {
    let body = response.get("response").unwrap_or(&Value::Null);
    if body.get("subtype").and_then(Value::as_str) == Some("error") {
        let error = body
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("an error");
        return Err(failed(format!("it answered: {error}")));
    }
    let models = body
        .pointer("/response/models")
        .and_then(Value::as_array)
        .ok_or_else(|| failed("its answer has no models"))?;
    let entries: Vec<ModelEntry> = models.iter().filter_map(entry).collect();
    if entries.is_empty() {
        return Err(failed("its answer lists no models"));
    }
    Ok(ModelList { entries })
}

/// One `models[]` item; `None` without a `value`.
fn entry(model: &Value) -> Option<ModelEntry> {
    let text = |field: &str| {
        model
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    let id = text("value")?;
    let resolves_to = text("resolvedModel");
    // An alias resolves to another id; an exact model resolves to itself (or says nothing).
    let kind = if id == TOOL_DEFAULT {
        ModelKind::ToolDefault
    } else if resolves_to.as_ref().is_some_and(|r| *r != id) {
        ModelKind::Follow
    } else {
        ModelKind::Exact
    };
    Some(ModelEntry {
        display_name: text("displayName").unwrap_or_else(|| id.clone()),
        description: text("description"),
        resolves_to,
        kind,
        family: None,
        released_at: None,
        id,
    })
}

/// Runs the list request with `program` (PATH `path`, in `cwd`), reads the answer and ends the process.
pub async fn read_list(
    program: &[String],
    path: &std::ffi::OsStr,
    cwd: &Path,
) -> Result<ModelList, ModelListError> {
    let command = list_command(program);
    let launch = crate::tools::cmd_shim::command(&command.program, path)
        .map_err(|e| failed(e.message(Some(super::DISPLAY_NAME))))?;
    let mut child = tokio::process::Command::from(launch)
        .args(&command.args)
        .env("PATH", path)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| failed(format!("can't start {}: {e}", command.program)))?;
    let answer = tokio::time::timeout(LIST_TIMEOUT, async {
        // Stdin stays open until the answer: at its end Claude Code would stop reading requests.
        let mut stdin = child.stdin.take().ok_or_else(|| failed("no stdin"))?;
        stdin
            .write_all(command.stdin.as_bytes())
            .await
            .map_err(|e| failed(format!("can't send the request: {e}")))?;
        let stdout = child.stdout.take().ok_or_else(|| failed("no stdout"))?;
        let mut lines = BufReader::new(stdout).lines();
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| failed(format!("can't read its answer: {e}")))?
        {
            if let Ok(line) = serde_json::from_str::<Value>(&line)
                && is_answer(&line)
            {
                drop(stdin);
                return parse_response(&line);
            }
        }
        Err(failed("it ended without answering"))
    })
    .await
    .unwrap_or_else(|_| {
        Err(failed(format!(
            "no answer within {}s",
            LIST_TIMEOUT.as_secs()
        )))
    });
    let _ = child.start_kill();
    let _ = child.wait().await;
    answer
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use serde_json::json;

    use super::*;

    fn fixture(name: &str) -> Value {
        let path: PathBuf = [
            env!("CARGO_MANIFEST_DIR"),
            "tests",
            "fixtures",
            "claude",
            "models",
            name,
        ]
        .iter()
        .collect();
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    /// The saved answer gives the five entries with their kinds, names and what they resolve to.
    #[test]
    fn the_saved_answer_gives_five_entries() {
        let response = fixture("initialize.json");
        assert!(is_answer(&response));
        let list = parse_response(&response).unwrap();
        let rows: Vec<_> = list
            .entries
            .iter()
            .map(|e| {
                (
                    e.id.as_str(),
                    e.display_name.as_str(),
                    e.resolves_to.as_deref(),
                    e.kind,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "default",
                    "Default (recommended)",
                    Some("claude-opus-5-5"),
                    ModelKind::ToolDefault
                ),
                ("opus", "Opus", Some("claude-opus-5-5"), ModelKind::Follow),
                (
                    "fable",
                    "Fable",
                    Some("claude-fable-5-1"),
                    ModelKind::Follow
                ),
                (
                    "sonnet",
                    "Sonnet",
                    Some("claude-sonnet-5-5"),
                    ModelKind::Follow
                ),
                (
                    "haiku",
                    "Haiku",
                    Some("claude-haiku-5-5"),
                    ModelKind::Follow
                ),
            ]
        );
        assert_eq!(list.tool_default().unwrap().passed(), None);
        assert_eq!(list.entry("opus").unwrap().passed(), Some("opus"));
        assert!(list.entry("opus").unwrap().description.is_some());
    }

    #[test]
    fn an_answer_without_models_or_with_an_error_is_an_error() {
        let mut response = fixture("initialize.json");
        response["response"]["response"]
            .as_object_mut()
            .unwrap()
            .remove("models");
        let error = parse_response(&response).unwrap_err();
        assert_eq!(error.message, "Claude Code didn't list its models");

        let refused = json!({"type": "control_response", "response": {"subtype": "error",
            "request_id": "r1", "error": "Already initialized"}});
        assert!(
            parse_response(&refused)
                .unwrap_err()
                .reason
                .contains("Already initialized")
        );
        assert!(
            parse_response(
                &json!({"type": "control_response", "response": {"subtype": "success",
                "request_id": "r1", "response": {"models": []}}})
            )
            .is_err()
        );
    }

    /// An exact id resolves to itself; an entry without a value is skipped.
    #[test]
    fn exact_models_and_odd_entries() {
        let response = json!({"type": "control_response", "response": {"subtype": "success",
        "request_id": "r1", "response": {"models": [
            {"value": "claude-opus-4-6", "displayName": "Opus 4.6", "resolvedModel": "claude-opus-4-6"},
            {"value": "claude-sonnet-4-6", "displayName": "Sonnet 4.6"},
            {"displayName": "no value"}
        ]}}});
        let list = parse_response(&response).unwrap();
        assert_eq!(list.entries.len(), 2);
        assert!(list.entries.iter().all(|e| e.kind == ModelKind::Exact));
    }

    /// The exact command and stdin of the request, without running anything; never a `--model`.
    #[test]
    fn the_request_command_and_stdin() {
        let command = list_command(&["claude".to_owned()]);
        assert_eq!(command.program, "claude");
        assert_eq!(
            command.args,
            [
                "-p",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose"
            ]
        );
        assert!(!command.args.iter().any(|a| a == "--model"));
        assert_eq!(
            command.stdin,
            "{\"type\":\"control_request\",\"request_id\":\"r1\",\"request\":{\"subtype\":\"initialize\"}}\n"
        );
        let stdin: Value = serde_json::from_str(&command.stdin).unwrap();
        assert_eq!(
            stdin,
            json!({"type": "control_request", "request_id": "r1", "request": {"subtype": "initialize"}})
        );
        // A program that stands for `claude` keeps its leading arguments.
        let wrapped = list_command(&["sh".to_owned(), "-c".to_owned()]);
        assert_eq!(
            (wrapped.program.as_str(), &wrapped.args[..2]),
            ("sh", &["-c".to_owned(), "-p".to_owned()][..])
        );
    }

    /// The process plumbing, with an ordinary `sh` in place of `claude`: it reads the request line, prints a
    /// line that isn't the answer, then the saved answer on one line; one that ends without answering is an
    /// error.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_answer_is_read_from_the_process() {
        let dir = tempfile::tempdir().unwrap();
        let answer = dir.path().join("answer.json");
        std::fs::write(&answer, fixture("initialize.json").to_string()).unwrap();
        let request = dir.path().join("request");
        let script = format!(
            "read line; printf '%s' \"$line\" > '{}'; echo '{{\"type\":\"system\"}}'; cat '{}'; echo; exec sleep 30",
            request.display(),
            answer.display()
        );
        let program = ["sh".to_owned(), "-c".to_owned(), script];
        let path = std::env::var_os("PATH").unwrap_or_default();
        let list = read_list(&program, &path, dir.path()).await.unwrap();
        assert_eq!(list.entries.len(), 5);
        assert_eq!(
            std::fs::read_to_string(&request).unwrap(),
            list_command(&["claude".to_owned()]).stdin.trim_end()
        );

        let silent = ["sh".to_owned(), "-c".to_owned(), "read line".to_owned()];
        let error = read_list(&silent, &path, dir.path()).await.unwrap_err();
        assert_eq!(error.message, "Claude Code didn't list its models");
        assert!(error.reason.contains("without answering"), "{error}");
    }
}
