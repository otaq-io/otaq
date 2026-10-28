//! Copilot's model list and Auto tiers (AGT-02, ADP-08; d0086, d0088; T-0353).
//!
//! `copilot --acp` speaks the Agent Client Protocol on stdin and stdout, one JSON-RPC message per line. Otaq sends
//! `initialize`, then `session/new` in a temporary folder; the answer's `models.availableModels` lists what this
//! account can run (`modelId`, `name`, sometimes `description` and `_meta.copilotUsage`), with no model call. Then
//! the process is ended. Copilot Free lists only `auto`, twice (once plain, once with its `_meta`), so the list is
//! de-duplicated by `modelId` (the models report, row 2; the Copilot real-account check, §2).
//!
//! It needs sign-in and the network: signed out `session/new` answers `{"code":-32000,"message":"Authentication
//! required"}` (checked with 1.0.98 in a scratch `COPILOT_HOME` on 2026-10-18, as the `initialize` answer is);
//! offline, `{"code":-32603,"message":"error sending request for url (https://api.individual.githubcopilot.com/models)
//! …"}`. Either is a [`ModelListError`]: the copy keeps the last list and the picker says "Couldn't load Copilot's
//! models · Retry".
//!
//! The entries: `auto` becomes three follow entries, one per Auto tier (`auto@efficiency`, `auto@balance`,
//! `auto@intelligence`, "Auto · efficiency" …, [`FollowChoice::Auto`]); plain `auto` is not a fourth (Q107). Every
//! other id is an exact model, passed to `--model` as it is. "Tool default" (Copilot's own default, `auto` on Free)
//! is the picker's own first row on every tool, so the list has no entry for it.
//!
//! Started only by the service ([`crate::agent_tools::models::ModelLists`]); tests parse saved answers
//! (`tests/fixtures/copilot/acp/`).

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use super::{COMMAND_NAME, CopilotContext, events_file::AUTO};
use crate::agent_tools::follow::{CopilotTier, FollowChoice};
use crate::agent_tools::models::ModelEntry;
use crate::agent_tools::{ModelKind, ModelList, ModelListError, Tool};

/// The longest the list may take (it answers in one to two seconds signed in).
pub const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// The option that starts Copilot's Agent Client Protocol server.
const ACP_FLAG: &str = "--acp";

/// The ids of Otaq's two requests.
const INITIALIZE_ID: u64 = 1;
const SESSION_ID: u64 = 2;

/// The ACP version Otaq speaks.
const PROTOCOL_VERSION: u64 = 1;

/// Each Auto tier's description, as Copilot's own `/model` picker words it (1.0.98).
fn tier_description(tier: CopilotTier) -> &'static str {
    match tier {
        CopilotTier::Efficiency => {
            "Favors faster models while preserving the quality needed for the task."
        }
        CopilotTier::Balance => "Balances model quality and speed while adapting to each task.",
        CopilotTier::Intelligence => {
            "Favors stronger models for complex work while remaining adaptive for simpler tasks."
        }
    }
}

/// What the list request runs: the arguments and the two lines written to stdin, for the session in `cwd`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListCommand {
    pub program: String,
    pub args: Vec<String>,
    pub stdin: Vec<String>,
}

/// The list request for a session in `cwd`.
pub fn list_command(cwd: &Path) -> ListCommand {
    let initialize = json!({
        "jsonrpc": "2.0",
        "id": INITIALIZE_ID,
        "method": "initialize",
        "params": {"protocolVersion": PROTOCOL_VERSION, "clientCapabilities": {}},
    });
    let session = json!({
        "jsonrpc": "2.0",
        "id": SESSION_ID,
        "method": "session/new",
        "params": {"cwd": cwd.display().to_string(), "mcpServers": []},
    });
    ListCommand {
        program: COMMAND_NAME.to_owned(),
        args: vec![ACP_FLAG.to_owned()],
        stdin: [initialize, session]
            .iter()
            .map(|line| format!("{line}\n"))
            .collect(),
    }
}

/// Why the list wasn't read, as the error people see.
fn failed(reason: impl Into<String>) -> ModelListError {
    ModelListError {
        message: format!("{} didn't list its models", Tool::Copilot.display_name()),
        reason: reason.into(),
    }
}

/// Whether `line` answers the request `id`.
fn answers(line: &Value, id: u64) -> bool {
    line.get("id").and_then(Value::as_u64) == Some(id)
        && (line.get("result").is_some() || line.get("error").is_some())
}

/// The JSON-RPC error of an answer, as its reason: "Authentication required".
fn error_of(answer: &Value) -> Option<String> {
    let error = answer.get("error")?;
    Some(
        error
            .get("message")
            .and_then(Value::as_str)
            .map_or_else(|| error.to_string(), str::to_owned),
    )
}

/// The list in the `session/new` answer. An error answer, or one without models, is an error.
pub fn parse_answer(answer: &Value) -> Result<ModelList, ModelListError> {
    if let Some(error) = error_of(answer) {
        return Err(failed(error));
    }
    let models = answer
        .pointer("/result/models/availableModels")
        .and_then(Value::as_array)
        .ok_or_else(|| failed("its answer has no models"))?;
    let mut entries: Vec<ModelEntry> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for model in models {
        let Some(id) = text(model, "modelId") else {
            continue;
        };
        if seen.contains(&id) {
            continue;
        }
        seen.push(id);
        if id == AUTO {
            entries.extend(CopilotTier::ALL.iter().map(|&tier| auto_entry(tier)));
        } else {
            entries.push(ModelEntry {
                id: id.to_owned(),
                display_name: text(model, "name").unwrap_or(id).to_owned(),
                description: text(model, "description").map(str::to_owned),
                resolves_to: None,
                kind: ModelKind::Exact,
                family: None,
                released_at: None,
            });
        }
    }
    if entries.is_empty() {
        return Err(failed("its answer lists no models"));
    }
    Ok(ModelList { entries })
}

fn text<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Auto with `tier`: stored `auto@<tier>`, "Auto · <tier>"; Auto picks a model per task, so it resolves to none.
fn auto_entry(tier: CopilotTier) -> ModelEntry {
    ModelEntry {
        id: FollowChoice::Auto { tier }.stored(),
        display_name: tier.label(),
        description: Some(tier_description(tier).to_owned()),
        resolves_to: None,
        kind: ModelKind::Follow,
        family: None,
        released_at: None,
    }
}

/// The list from Copilot on `context`'s PATH, its session in a temporary folder (removed afterwards).
pub(super) async fn read_list(context: &CopilotContext) -> Result<ModelList, ModelListError> {
    let program = crate::tools::find_on_path(COMMAND_NAME, &context.tool.path)
        .ok_or_else(|| failed(format!("{COMMAND_NAME} isn't on the PATH")))?;
    let folder =
        tempfile::tempdir().map_err(|e| failed(format!("can't make a temporary folder: {e}")))?;
    run(program.as_os_str(), &context.tool.path, folder.path()).await
}

async fn send(stdin: &mut tokio::process::ChildStdin, line: &str) -> Result<(), ModelListError> {
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| failed(format!("can't send the request: {e}")))
}

/// Runs the list request with `program` (PATH `path`, its session in `cwd`), reads the answer and ends the process.
pub async fn run(
    program: &std::ffi::OsStr,
    path: &std::ffi::OsStr,
    cwd: &Path,
) -> Result<ModelList, ModelListError> {
    let command = list_command(cwd);
    let launch = crate::tools::cmd_shim::command(program, path)
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
        .map_err(|e| failed(format!("can't start {}: {e}", program.to_string_lossy())))?;
    let answer = tokio::time::timeout(LIST_TIMEOUT, async {
        let mut stdin = child.stdin.take().ok_or_else(|| failed("no stdin"))?;
        let stdout = child.stdout.take().ok_or_else(|| failed("no stdout"))?;
        let mut lines = BufReader::new(stdout).lines();
        send(&mut stdin, &command.stdin[0]).await?;
        let mut sent_session = false;
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|e| failed(format!("can't read its answer: {e}")))?
        {
            let Ok(line) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if answers(&line, INITIALIZE_ID) && !sent_session {
                if let Some(error) = error_of(&line) {
                    return Err(failed(error));
                }
                send(&mut stdin, &command.stdin[1]).await?;
                sent_session = true;
            } else if answers(&line, SESSION_ID) {
                tracing::debug!(answer = %line, "copilot session/new answer");
                return parse_answer(&line);
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

    use super::*;

    fn fixture(name: &str) -> Value {
        let path: PathBuf = [
            env!("CARGO_MANIFEST_DIR"),
            "tests",
            "fixtures",
            "copilot",
            "acp",
            name,
        ]
        .iter()
        .collect();
        let text =
            std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn rows(list: &ModelList) -> Vec<(&str, &str, ModelKind)> {
        list.entries
            .iter()
            .map(|e| (e.id.as_str(), e.display_name.as_str(), e.kind))
            .collect()
    }

    /// Copilot Free: `auto` (listed twice) gives the three Auto tiers once, and nothing else.
    #[test]
    fn free_lists_the_three_auto_tiers() {
        let list = parse_answer(&fixture("session_new-free.json")).unwrap();
        assert_eq!(
            rows(&list),
            [
                (
                    "auto@efficiency",
                    "Auto \u{b7} efficiency",
                    ModelKind::Follow
                ),
                ("auto@balance", "Auto \u{b7} balance", ModelKind::Follow),
                (
                    "auto@intelligence",
                    "Auto \u{b7} intelligence",
                    ModelKind::Follow
                ),
            ]
        );
        assert!(!list.contains("auto"));
        assert_eq!(list.tool_default(), None);
        assert_eq!(
            list.entry("auto@balance").unwrap().description.as_deref(),
            Some("Balances model quality and speed while adapting to each task.")
        );
        assert_eq!(
            list.entry("auto@balance").unwrap().passed(),
            Some("auto@balance")
        );
    }

    /// A paid plan: the Auto tiers where `auto` is listed, then every other id as an exact model, each once.
    #[test]
    fn other_models_are_exact() {
        let list = parse_answer(&fixture("session_new-paid.json")).unwrap();
        assert_eq!(
            rows(&list),
            [
                (
                    "auto@efficiency",
                    "Auto \u{b7} efficiency",
                    ModelKind::Follow
                ),
                ("auto@balance", "Auto \u{b7} balance", ModelKind::Follow),
                (
                    "auto@intelligence",
                    "Auto \u{b7} intelligence",
                    ModelKind::Follow
                ),
                ("gpt-5.5", "GPT-5.5", ModelKind::Exact),
                ("claude-sonnet-5.5", "Claude Sonnet 5.5", ModelKind::Exact),
                ("gpt-6-luna", "gpt-6-luna", ModelKind::Exact),
            ]
        );
        assert_eq!(list.entry("gpt-5.5").unwrap().passed(), Some("gpt-5.5"));
    }

    /// Signed out and offline: the error with Copilot's reason; an answer without models is an error too.
    #[test]
    fn signed_out_and_offline_are_errors_with_their_reason() {
        let signed_out = parse_answer(&fixture("session_new-signed_out.json")).unwrap_err();
        assert_eq!(signed_out.message, "GitHub Copilot didn't list its models");
        assert_eq!(signed_out.reason, "Authentication required");
        let offline = parse_answer(&fixture("session_new-offline.json")).unwrap_err();
        assert!(
            offline.reason.starts_with(
                "error sending request for url (https://api.individual.githubcopilot.com/models)"
            ),
            "{offline}"
        );
        let empty = json!({"jsonrpc": "2.0", "id": 2, "result": {"sessionId": "s",
            "models": {"availableModels": [], "currentModelId": "auto"}}});
        assert!(parse_answer(&empty).is_err());
        let none = json!({"jsonrpc": "2.0", "id": 2, "result": {"sessionId": "s"}});
        assert_eq!(
            parse_answer(&none).unwrap_err().reason,
            "its answer has no models"
        );
    }

    /// The exact request: `copilot --acp`, `initialize`, then `session/new` in the given folder; never a model.
    #[test]
    fn the_request_bytes() {
        let command = list_command(Path::new("/var/folders/x/T/.tmpA1b2"));
        assert_eq!(command.program, "copilot");
        assert_eq!(command.args, ["--acp"]);
        assert_eq!(
            command.stdin,
            [
                "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"initialize\",\"params\":{\"clientCapabilities\":{},\"protocolVersion\":1}}\n",
                "{\"id\":2,\"jsonrpc\":\"2.0\",\"method\":\"session/new\",\"params\":{\"cwd\":\"/var/folders/x/T/.tmpA1b2\",\"mcpServers\":[]}}\n",
            ]
        );
    }

    /// The process plumbing, with `sh` in place of `copilot`: it answers `initialize`, prints a notification, then
    /// the saved `session/new` answer once the second request came. One that ends without answering, or answers
    /// `initialize` with an error, is an error.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_answer_is_read_from_the_process() {
        let dir = tempfile::tempdir().unwrap();
        let answer = dir.path().join("answer.json");
        std::fs::write(&answer, fixture("session_new-free.json").to_string()).unwrap();
        let initialize = dir.path().join("initialize.json");
        std::fs::write(&initialize, fixture("initialize.json").to_string()).unwrap();
        let requests = dir.path().join("requests");
        let script = |body: &str| {
            let file = dir.path().join(format!("copilot-{}", body.len()));
            std::fs::write(&file, format!("#!/bin/sh\n{body}\n")).unwrap();
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
            file
        };
        let good = script(&format!(
            "read a; printf '%s\\n' \"$a\" > '{r}'; cat '{i}'; echo; echo '{{\"jsonrpc\":\"2.0\",\"method\":\"session/update\"}}'; read b; printf '%s\\n' \"$b\" >> '{r}'; cat '{s}'; echo; exec sleep 30",
            r = requests.display(),
            i = initialize.display(),
            s = answer.display()
        ));
        let path = std::env::var_os("PATH").unwrap_or_default();
        let list = run(good.as_os_str(), &path, dir.path()).await.unwrap();
        assert_eq!(list.entries.len(), 3);
        assert_eq!(
            std::fs::read_to_string(&requests).unwrap(),
            list_command(dir.path()).stdin.concat()
        );

        let silent = script("read a");
        let error = run(silent.as_os_str(), &path, dir.path())
            .await
            .unwrap_err();
        assert!(error.reason.contains("without answering"), "{error}");

        let refused = script(
            "read a; echo '{\"jsonrpc\":\"2.0\",\"id\":1,\"error\":{\"code\":-32602,\"message\":\"Invalid params\"}}'; exec sleep 30",
        );
        let error = run(refused.as_os_str(), &path, dir.path())
            .await
            .unwrap_err();
        assert_eq!(error.reason, "Invalid params");
    }
}
