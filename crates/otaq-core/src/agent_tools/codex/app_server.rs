//! A short-lived `codex app-server` (T-0351): Codex's documented JSON-RPC over stdio, one JSON object per line.
//! `initialize` (with Otaq's client name), the `initialized` notification, then requests answered by id; the
//! server's own notifications are skipped. Closing its stdin ends it; a server that hasn't ended by then is killed
//! with everything it started (the `codex` on PATH is often npm's launcher of the native program).
//!
//! Used for the model list (`model/list`) and the hook trust (`hooks/list`, `config/batchWrite`). Started only by
//! the service; tests run a stand-in program.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use super::{COMMAND_NAME, CodexContext};
use crate::naming;

/// How long the server may take to end once its stdin is closed.
const END_WAIT: Duration = Duration::from_secs(2);

/// A running app server.
pub(super) struct AppServer {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Lines<BufReader<ChildStdout>>,
    next_id: u64,
}

impl AppServer {
    /// Starts `codex app-server` from the context's PATH in `cwd`, with `CODEX_HOME` set to Codex's home when it
    /// is known, and initializes it. Its error output is dropped.
    pub(super) async fn start(context: &CodexContext, cwd: &Path) -> Result<Self, String> {
        let program = crate::tools::find_on_path(COMMAND_NAME, &context.tool.path)
            .ok_or_else(|| format!("{COMMAND_NAME} isn't on PATH"))?;
        let mut command = Command::from(
            crate::tools::cmd_shim::command(&program, &context.tool.path)
                .map_err(|e| e.message(Some(super::DISPLAY_NAME)))?,
        );
        command
            .arg("app-server")
            .env("PATH", &context.tool.path)
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        if let Some(home) = &context.codex_home {
            command.env("CODEX_HOME", home);
        }
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .map_err(|e| format!("can't start {}: {e}", program.display()))?;
        let stdin = child.stdin.take().ok_or("no stdin")?;
        let stdout = child.stdout.take().ok_or("no stdout")?;
        let mut server = Self {
            child,
            stdin: Some(stdin),
            lines: BufReader::new(stdout).lines(),
            next_id: 0,
        };
        server
            .request(
                "initialize",
                json!({
                    "clientInfo": {
                        "name": naming::COMMAND_NAME,
                        "title": naming::PRODUCT_NAME,
                        "version": env!("CARGO_PKG_VERSION"),
                    }
                }),
            )
            .await?;
        server
            .send(&json!({ "method": "initialized" }))
            .await
            .map_err(|e| format!("can't send initialized: {e}"))?;
        Ok(server)
    }

    /// Sends `method` and waits for its answer: the `result`, or the `error`'s message.
    pub(super) async fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "id": id, "method": method, "params": params }))
            .await
            .map_err(|e| format!("can't send {method}: {e}"))?;
        loop {
            let line = self
                .lines
                .next_line()
                .await
                .map_err(|e| format!("can't read its answer to {method}: {e}"))?
                .ok_or_else(|| format!("it ended without answering {method}"))?;
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(answer) = answer_to(&message, id) {
                tracing::debug!(method, answer = %line, "codex app-server answer");
                return answer.map_err(|e| format!("it answered {method} with: {e}"));
            }
        }
    }

    async fn send(&mut self, message: &Value) -> std::io::Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| std::io::Error::other("stdin closed"))?;
        stdin.write_all(format!("{message}\n").as_bytes()).await?;
        stdin.flush().await
    }

    /// Closes its stdin and waits briefly for it to end; then ends it and everything it started.
    pub(super) async fn stop(mut self) {
        drop(self.stdin.take());
        if tokio::time::timeout(END_WAIT, self.child.wait())
            .await
            .is_err()
        {
            kill(&mut self.child).await;
        }
    }
}

impl Drop for AppServer {
    /// A server dropped mid-request (a timeout) is ended with its children.
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            crate::process::Group::of_leader(self.child.id()).kill();
            let _ = self.child.start_kill();
        }
    }
}

async fn kill(child: &mut Child) {
    crate::process::Group::of_leader(child.id()).kill();
    let _ = child.start_kill();
    let _ = child.wait().await;
}

/// The answer in `message` to request `id`: `Ok(result)` or `Err(error message)`; `None` for anything else (a
/// notification, a request from the server, another id).
pub(super) fn answer_to(message: &Value, id: u64) -> Option<Result<Value, String>> {
    if message.get("method").is_some() || message.get("id").and_then(Value::as_u64) != Some(id) {
        return None;
    }
    if let Some(error) = message.get("error") {
        let text = error
            .get("message")
            .and_then(Value::as_str)
            .map_or_else(|| error.to_string(), str::to_owned);
        return Some(Err(text));
    }
    Some(Ok(message.get("result").cloned().unwrap_or(Value::Null)))
}

/// The folder a short-lived server runs in: Codex's home (not a repo, so no repo hooks or config join in), else
/// the system's temporary folder.
pub(super) fn neutral_cwd(context: &CodexContext) -> PathBuf {
    context
        .codex_home
        .clone()
        .filter(|home| home.is_dir())
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;

    /// A stand-in `codex` on `bin` whose `app-server` answers each request with the result of the first of
    /// `answers` whose text is in the request's line (`"method":"model/list"`, `"cursor":"3"`), logs every line it
    /// reads to `log`, and answers `initialize` with an empty result. `codex plugin …` logs its command line and
    /// runs the `sh` code `plugin` (its arguments in `$@`).
    pub(in crate::agent_tools::codex) fn stand_in(
        bin: &Path,
        log: &Path,
        answers: &[(&str, Value)],
        plugin: &str,
    ) {
        let mut cases = String::new();
        for (n, (text, result)) in answers.iter().enumerate() {
            let file = bin.join(format!("answer-{n}.json"));
            std::fs::write(&file, result.to_string()).unwrap();
            cases.push_str(&format!(
                "      *'{text}'*) printf '{{\"id\":%s,\"result\":%s}}\\n' \"$id\" \"$(/bin/cat '{}')\" ;;\n",
                file.display()
            ));
        }
        let script = format!(
            r#"echo "$*" >> '{log}'
if [ "$1" = plugin ]; then
  {plugin}
  exit 0
fi
if [ "$1" = app-server ]; then
  while IFS= read -r line; do
    printf '%s\n' "$line" >> '{log}'
    id=$(printf '%s' "$line" | /usr/bin/sed -n 's/^{{"id":\([0-9]*\),.*/\1/p')
    [ -z "$id" ] && continue
    printf '{{"method":"remoteControl/status/changed","params":{{}}}}\n'
    case "$line" in
      *'"method":"initialize"'*) printf '{{"id":%s,"result":{{}}}}\n' "$id" ;;
{cases}      *) printf '{{"id":%s,"error":{{"code":-32601,"message":"unknown method"}}}}\n' "$id" ;;
    esac
  done
  exit 0
fi
exit 2"#,
            log = log.display()
        );
        crate::process::testing::stub_command(bin, COMMAND_NAME, &script);
    }

    /// A saved app-server answer, `tests/fixtures/codex/app_server/<name>`, on one line.
    pub(in crate::agent_tools::codex) fn saved(name: &str) -> Value {
        let path = format!(
            "{}/tests/fixtures/codex/app_server/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    #[test]
    fn answers_are_matched_by_id() {
        assert_eq!(
            answer_to(&json!({"id": 3, "result": {"data": []}}), 3),
            Some(Ok(json!({"data": []})))
        );
        assert_eq!(answer_to(&json!({"id": 2, "result": {}}), 3), None);
        assert_eq!(
            answer_to(&json!({"method": "x", "params": {}}), 3),
            None,
            "a notification"
        );
        assert_eq!(
            answer_to(&json!({"id": 3, "method": "item/approve", "params": {}}), 3),
            None,
            "a request from the server"
        );
        assert_eq!(
            answer_to(&json!({"id": 3, "error": {"code": -1, "message": "no"}}), 3),
            Some(Err("no".into()))
        );
    }

    /// The stand-in answers like the real server (initialize, then the request), skipping its notifications.
    #[tokio::test]
    async fn a_request_through_a_stand_in() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        stand_in(
            dir.path(),
            &log,
            &[(r#""method":"model/list""#, json!({"data": []}))],
            "",
        );
        let context = CodexContext {
            tool: crate::agent_tools::test_support::context(dir.path(), None),
            codex_home: Some(dir.path().to_path_buf()),
        };
        let mut server = AppServer::start(&context, dir.path()).await.unwrap();
        assert_eq!(
            server.request("model/list", json!({})).await,
            Ok(json!({"data": []}))
        );
        assert_eq!(
            server.request("nope", json!({})).await,
            Err("it answered nope with: unknown method".into())
        );
        server.stop().await;
        let log = std::fs::read_to_string(&log).unwrap();
        assert!(log.starts_with("app-server\n"), "{log}");
        assert!(log.contains(r#""method":"initialized""#), "{log}");
        assert!(
            log.contains(&format!(r#""name":"{}""#, naming::COMMAND_NAME)),
            "{log}"
        );
    }
}
