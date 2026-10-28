//! OpenCode's model list (AGT-02; d0086, d0088): `GET /api/model` from a short-lived `opencode serve`.
//!
//! `opencode serve --hostname 127.0.0.1 --port 0` in a temporary folder, with the user's own data and config
//! folders (their providers), prints `server listening on http://127.0.0.1:<port>`; every `/api/*` request needs
//! HTTP Basic `opencode:<password>`, and `OPENCODE_SERVER_PASSWORD` presets the password (`OPENCODE_PASSWORD` does
//! too; without either the server prints a random one). Its catalog loads a moment after it starts: the first
//! answers are `{"data": []}`, then the whole list (about 1 s with only the free models, 1.5 s with a provider), so
//! Otaq asks every 500 ms until two answers in a row list the same models, 10 s at most, and then ends the server
//! and everything it started. `opencode models` and `opencode api --standalone` always answer empty (their private
//! server exits before its catalog loads; models report row 2), and `/provider` is the web page, never asked.
//!
//! Each model is `{id, providerID, name, family?, time: {released}, enabled, …}` in release order, newest first;
//! `family` is missing on some. Offline, OpenCode answers its stale bundled list: accepted as its own answer. Checked
//! with OpenCode 2.0.17 and its folders in a scratch folder (no provider, then a stored test key; no model call):
//! `tests/fixtures/opencode/models/`.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::Instant;

use super::{COMMAND_NAME, OpenCodeContext};
use crate::agent_tools::catalog::FamilySource;
use crate::agent_tools::models::ModelEntry;
use crate::agent_tools::{ModelKind, ModelList, ModelListError, Tool};

/// The longest the list may take, the server's start included.
pub(super) const LIST_TIMEOUT: Duration = Duration::from_secs(10);

/// How often the list is asked for while the server loads its catalog.
const POLL_EVERY: Duration = Duration::from_millis(500);

/// How long one request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(2);

/// How long the server has to end after a hang-up before it and its children are killed.
const END_WAIT: Duration = Duration::from_secs(2);

/// The variable that presets the server's password.
const PASSWORD_VAR: &str = "OPENCODE_SERVER_PASSWORD";

/// The Basic auth user name the server expects.
const USER: &str = "opencode";

/// What the server prints once it listens.
const LISTENING: &str = "server listening on ";

/// The tool default's id in the list: no model is passed (d0088). OpenCode's own ids always have a provider
/// (`opencode/big-pickle`), so this never names a model.
pub(super) const TOOL_DEFAULT: &str = "default";

/// OpenCode's list says each model's family and release date; models.dev names a family for the same
/// `provider/id` when the list has none (d0086).
pub(super) fn family_source() -> FamilySource {
    FamilySource::ListCarriesFamily {
        models_dev_fallback: true,
    }
}

/// What starts the server: the arguments and the password it is given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ServeCommand {
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub password: String,
}

/// The server on a free local port, with a password Otaq picks.
pub(super) fn serve_command(password: &str) -> ServeCommand {
    ServeCommand {
        args: ["serve", "--hostname", "127.0.0.1", "--port", "0"]
            .map(str::to_owned)
            .to_vec(),
        env: vec![(PASSWORD_VAR.to_owned(), password.to_owned())],
        password: password.to_owned(),
    }
}

/// A password nobody else knows, for one server: the random halves of two v7 UUIDs.
fn new_password() -> String {
    (0..2)
        .flat_map(|_| uuid::Uuid::now_v7().as_bytes()[8..].to_vec())
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn failed(reason: impl Into<String>) -> ModelListError {
    ModelListError {
        message: format!("{} didn't list its models", Tool::OpenCode.display_name()),
        reason: reason.into(),
    }
}

/// The address in the server's `server listening on http://127.0.0.1:52681` line.
pub(super) fn listening_url(line: &str) -> Option<&str> {
    let url = line.trim().strip_prefix(LISTENING)?.trim();
    url.starts_with("http://").then_some(url)
}

/// The list in an `/api/model` answer, the tool default first (resolving to `default`, the `/api/model/default`
/// answer, when known). Disabled models are left out. `None` when the answer isn't a list.
pub(super) fn parse_list(answer: &Value, default: Option<&Value>) -> Option<ModelList> {
    let models = answer.get("data")?.as_array()?;
    let mut entries = vec![ModelEntry {
        id: TOOL_DEFAULT.to_owned(),
        display_name: "Tool default".to_owned(),
        description: None,
        resolves_to: default.and_then(|d| d.get("data")).and_then(model_path),
        kind: ModelKind::ToolDefault,
        family: None,
        released_at: None,
    }];
    entries.extend(models.iter().filter_map(entry));
    Some(ModelList { entries })
}

/// `opencode/big-pickle` for a model of the list.
fn model_path(model: &Value) -> Option<String> {
    let text = |field: &str| {
        model
            .get(field)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
    };
    Some(format!("{}/{}", text("providerID")?, text("id")?))
}

fn entry(model: &Value) -> Option<ModelEntry> {
    if model.get("enabled").and_then(Value::as_bool) == Some(false) {
        return None;
    }
    let id = model_path(model)?;
    let text = |field: &str| {
        model
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    Some(ModelEntry {
        display_name: text("name").unwrap_or_else(|| id.clone()),
        description: None,
        resolves_to: None,
        kind: ModelKind::Exact,
        family: text("family"),
        released_at: model.pointer("/time/released").and_then(Value::as_i64),
        id,
    })
}

/// The models in the list (the tool default aside), to tell a loaded list from an empty one.
fn models_in(list: &ModelList) -> usize {
    list.entries
        .iter()
        .filter(|e| e.kind != ModelKind::ToolDefault)
        .count()
}

/// Starts the server, asks for the list until it has loaded, and ends the server and its children.
pub(super) async fn read_list(context: &OpenCodeContext) -> Result<ModelList, ModelListError> {
    let program = crate::tools::find_on_path(COMMAND_NAME, &context.tool.path)
        .ok_or_else(|| failed(format!("{COMMAND_NAME} isn't on PATH")))?;
    let folder = tempfile::tempdir().map_err(|e| failed(format!("no temporary folder: {e}")))?;
    read_list_with(
        &program,
        &context.tool.path,
        folder.path(),
        &new_password(),
        LIST_TIMEOUT,
    )
    .await
}

/// [`read_list`] with `program` (OpenCode, or what stands for it) on `path`, in `cwd`, `limit` at most.
pub(super) async fn read_list_with(
    program: &Path,
    path: &std::ffi::OsStr,
    cwd: &Path,
    password: &str,
    limit: Duration,
) -> Result<ModelList, ModelListError> {
    let serve = serve_command(password);
    let mut command = tokio::process::Command::from(
        crate::tools::cmd_shim::command(program, path)
            .map_err(|e| failed(e.message(Some(super::DISPLAY_NAME))))?,
    );
    command
        .args(&serve.args)
        .envs(serve.env.iter().map(|(k, v)| (k, v)))
        .env("PATH", path)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    // Its own process group, so ending it ends what it started too.
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|e| failed(format!("can't start {}: {e}", program.display())))?;
    let group = Server(crate::process::Group::of_leader(child.id()));
    let deadline = Instant::now() + limit;
    let stdout = child.stdout.take();
    // Only the wait for its address times out here: `poll` keeps to the same deadline itself and says why it stopped
    // (a server that answers with no models isn't one that took too long).
    let answer = async {
        let url = tokio::time::timeout_at(deadline, listening(stdout))
            .await
            .unwrap_or_else(|_| Err(failed(format!("it took longer than {}s", limit.as_secs()))))?;
        poll(&url, &serve.password, deadline).await
    }
    .await;
    group.0.hang_up();
    if tokio::time::timeout(END_WAIT, child.wait()).await.is_err() {
        let _ = child.start_kill();
    }
    drop(group);
    answer
}

/// Kills the server's whole group when dropped: after the list, or when the caller gives up waiting.
struct Server(crate::process::Group);

impl Drop for Server {
    fn drop(&mut self) {
        self.0.kill();
    }
}

/// The server's address, from its first lines.
async fn listening(stdout: Option<tokio::process::ChildStdout>) -> Result<String, ModelListError> {
    let stdout = stdout.ok_or_else(|| failed("no output"))?;
    let mut lines = BufReader::new(stdout).lines();
    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| failed(format!("can't read its output: {e}")))?
    {
        tracing::debug!(line, "a line of opencode serve's output");
        if let Some(url) = listening_url(&line) {
            return Ok(url.trim_end_matches('/').to_owned());
        }
    }
    Err(failed("it ended before it listened"))
}

/// Asks for the list every [`POLL_EVERY`] until two answers in a row list the same number of models (a catalog
/// still loading may answer part of it), or until `deadline`, when the last list with models counts. No request
/// outlasts `deadline`.
async fn poll(url: &str, password: &str, deadline: Instant) -> Result<ModelList, ModelListError> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|e| failed(e.to_string()))?;
    let get = |what: &'static str| {
        let client = client.clone();
        let request = client
            .get(format!("{url}{what}"))
            .basic_auth(USER, Some(password));
        async move {
            let response = request.send().await.map_err(|e| e.to_string())?;
            let status = response.status();
            if !status.is_success() {
                return Err(format!("{what} answered {status}"));
            }
            response.json::<Value>().await.map_err(|e| e.to_string())
        }
    };
    let mut last: Option<ModelList> = None;
    loop {
        let Ok(answer) = tokio::time::timeout_at(deadline, get("/api/model")).await else {
            return last.ok_or_else(|| failed("it listed no models"));
        };
        match answer {
            Ok(answer) => {
                tracing::debug!(answer = %answer, "opencode's /api/model");
                let list = parse_list(&answer, None)
                    .ok_or_else(|| failed("its answer has no list of models"))?;
                let loaded = models_in(&list);
                if loaded > 0 && last.as_ref().is_some_and(|l| models_in(l) == loaded) {
                    let default = tokio::time::timeout_at(deadline, get("/api/model/default"))
                        .await
                        .ok()
                        .and_then(Result::ok);
                    return parse_list(&answer, default.as_ref())
                        .ok_or_else(|| failed("its answer has no list of models"));
                }
                if loaded > 0 {
                    last = Some(list);
                }
            }
            Err(e) if e.contains("401") => return Err(failed(e)),
            Err(e) => tracing::debug!(error = %e, "opencode's model list isn't ready"),
        }
        if Instant::now() + POLL_EVERY >= deadline {
            return last.ok_or_else(|| failed("it listed no models"));
        }
        tokio::time::sleep(POLL_EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    // The server's tests are Unix only: the stand-in `opencode` is an `sh` script, which Windows can't run (Q598).
    // On Windows their helpers go unused.
    #![cfg_attr(not(unix), allow(dead_code, unused_imports))]

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    /// A saved answer (`tests/fixtures/opencode/models/`).
    fn fixture(name: &str) -> Value {
        let path = format!(
            "{}/tests/fixtures/opencode/models/{name}.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn the_server_command_and_its_password() {
        let serve = serve_command("pw1");
        assert_eq!(
            serve.args,
            ["serve", "--hostname", "127.0.0.1", "--port", "0"]
        );
        assert_eq!(
            serve.env,
            [("OPENCODE_SERVER_PASSWORD".to_owned(), "pw1".to_owned())]
        );
        assert_ne!(new_password(), new_password());
        assert_eq!(new_password().len(), 32);
        assert_eq!(
            listening_url("server listening on http://127.0.0.1:52681\n"),
            Some("http://127.0.0.1:52681")
        );
        assert_eq!(listening_url("server password BR6Gym"), None);
    }

    /// The free models with no provider signed in (captured): the tool default first, resolving to OpenCode's own
    /// default; each model as `provider/id` with its name, family and release date; Space Bunny has no family.
    #[test]
    fn the_free_list() {
        let list = parse_list(&fixture("free"), Some(&fixture("default"))).unwrap();
        assert_eq!(list.entries.len(), 11);
        let default = &list.entries[0];
        assert_eq!(default.kind, ModelKind::ToolDefault);
        assert_eq!(default.id, "default");
        assert_eq!(default.passed(), None);
        assert_eq!(
            default.resolves_to.as_deref(),
            Some("opencode/nemotron-3.5-super-vl-free")
        );
        let first = &list.entries[1];
        assert_eq!(first.id, "opencode/nemotron-3.5-super-vl-free");
        assert_eq!(first.display_name, "Nemotron 3.5 Super VL Free");
        assert_eq!(first.kind, ModelKind::Exact);
        assert_eq!(first.family.as_deref(), Some("nemotron-free"));
        assert_eq!(first.released_at, Some(1_792_022_400_000));
        let bunny = list.entry("opencode/space-bunny-free").unwrap();
        assert_eq!(bunny.family, None);
        assert_eq!(bunny.released_at, Some(1_790_121_600_000));
        assert_eq!(
            list.entries.last().unwrap().id,
            "opencode/big-pickle",
            "the tool's own order"
        );
    }

    /// With a provider signed in (captured with a stored test key): its models come with the free ones; a disabled
    /// model is left out; an answer without a list is no list.
    #[test]
    fn the_signed_in_list() {
        let mut answer = fixture("signed-in");
        let list = parse_list(&answer, None).unwrap();
        assert_eq!(list.entries.len(), 31);
        assert_eq!(list.tool_default().unwrap().resolves_to, None);
        let opus = list.entry("anthropic/claude-opus-5-5").unwrap();
        assert_eq!(opus.display_name, "Claude Opus 5.5");
        assert_eq!(opus.family.as_deref(), Some("claude-opus"));
        assert!(list.contains("opencode/big-pickle"));

        answer["data"][1]["enabled"] = Value::Bool(false);
        let list = parse_list(&answer, None).unwrap();
        assert!(!list.contains("anthropic/claude-haiku-5-5"));
        assert_eq!(parse_list(&serde_json::json!({"error": "x"}), None), None);
    }

    #[test]
    fn families_come_from_the_list_with_models_dev_for_the_rest() {
        assert_eq!(
            family_source(),
            FamilySource::ListCarriesFamily {
                models_dev_fallback: true
            }
        );
    }

    /// What the stand-in server saw.
    #[derive(Default)]
    struct Seen {
        auth: Vec<String>,
        paths: Vec<String>,
    }

    /// Serves `/api/model` like OpenCode while its catalog loads: `answers` in turn (the last one again after),
    /// `/api/model/default` from the fixture; 401 without the password.
    async fn serve(answers: Vec<Value>, seen: Arc<Mutex<Seen>>) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut asked = 0;
            loop {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut buf = vec![0u8; 8192];
                let n = socket.read(&mut buf).await.unwrap();
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = request.split(' ').nth(1).unwrap_or("").to_owned();
                let auth = request
                    .lines()
                    .find_map(|l| {
                        l.split_once(": ")
                            .filter(|(name, _)| name.eq_ignore_ascii_case("authorization"))
                            .map(|(_, value)| value)
                    })
                    .unwrap_or("")
                    .to_owned();
                let body = if auth != "Basic b3BlbmNvZGU6cHctdGVzdA==" {
                    None
                } else if path == "/api/model" {
                    let answer = answers[asked.min(answers.len() - 1)].clone();
                    asked += 1;
                    Some(answer)
                } else {
                    Some(fixture("default"))
                };
                {
                    let mut seen = seen.lock().unwrap();
                    seen.auth.push(auth);
                    seen.paths.push(path);
                }
                let (status, body) = match body {
                    Some(body) => ("200 OK", body.to_string()),
                    None => ("401 Unauthorized", "{}".to_owned()),
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = socket.write_all(reply.as_bytes()).await;
            }
        });
        port
    }

    /// A stand-in `opencode` that records its arguments, its password and its children's ids, starts a child that
    /// would outlive it, prints the line OpenCode prints and waits.
    #[cfg(unix)]
    fn stand_in(bin: &Path, port: u16) -> std::path::PathBuf {
        let file = bin.join("opencode");
        let out = bin.join("ran");
        std::fs::write(
            &file,
            format!(
                "#!/bin/sh\necho \"$@\" > {out}/args\necho \"$OPENCODE_SERVER_PASSWORD\" > {out}/password\n\
                 sleep 300 &\necho $! > {out}/child\necho $$ > {out}/self\n\
                 echo 'server listening on http://127.0.0.1:{port}'\nwait\n",
                out = out.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::create_dir_all(&out).unwrap();
        file
    }

    fn alive(pid_file: &Path) -> bool {
        let pid: u32 = std::fs::read_to_string(pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        crate::process::alive(pid)
    }

    /// An empty answer, then the whole list twice: the list, read with the password Otaq gave the server; then the
    /// server and its child are gone.
    #[cfg(unix)]
    #[tokio::test]
    async fn the_list_once_the_catalog_has_loaded_then_the_server_is_ended() {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let empty = serde_json::json!({"location": {"directory": "/tmp/x"}, "data": []});
        let port = serve(vec![empty, fixture("free")], seen.clone()).await;
        let bin = tempfile::tempdir().unwrap();
        let program = stand_in(bin.path(), port);
        let cwd = tempfile::tempdir().unwrap();

        let list = read_list_with(
            &program,
            bin.path().as_os_str(),
            cwd.path(),
            "pw-test",
            LIST_TIMEOUT,
        )
        .await
        .unwrap();
        assert_eq!(list.entries.len(), 11);
        assert_eq!(
            list.tool_default().unwrap().resolves_to.as_deref(),
            Some("opencode/nemotron-3.5-super-vl-free")
        );

        let ran = bin.path().join("ran");
        assert_eq!(
            std::fs::read_to_string(ran.join("args")).unwrap().trim(),
            "serve --hostname 127.0.0.1 --port 0"
        );
        assert_eq!(
            std::fs::read_to_string(ran.join("password"))
                .unwrap()
                .trim(),
            "pw-test"
        );
        {
            let seen = seen.lock().unwrap();
            assert_eq!(
                seen.paths,
                [
                    "/api/model",
                    "/api/model",
                    "/api/model",
                    "/api/model/default"
                ],
                "asked until two answers agree; never /provider"
            );
            // opencode:pw-test
            assert!(
                seen.auth
                    .iter()
                    .all(|a| a == "Basic b3BlbmNvZGU6cHctdGVzdA==")
            );
        }
        for _ in 0..50 {
            if !alive(&ran.join("self")) && !alive(&ran.join("child")) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the server or its child is still running");
    }

    /// A server that never lists a model: an error after the time limit, and the server is ended all the same.
    #[cfg(unix)]
    #[tokio::test]
    async fn no_models_is_an_error_and_the_server_still_ends() {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let empty = serde_json::json!({"data": []});
        let port = serve(vec![empty], seen).await;
        let bin = tempfile::tempdir().unwrap();
        let program = stand_in(bin.path(), port);
        let cwd = tempfile::tempdir().unwrap();
        let started = std::time::Instant::now();
        // Room for the stand-in to start: a new script can take over a second to start on macOS while other tests
        // start processes too.
        let limit = Duration::from_secs(4);
        let error = read_list_with(
            &program,
            bin.path().as_os_str(),
            cwd.path(),
            "pw-test",
            limit,
        )
        .await
        .unwrap_err();
        assert_eq!(error.message, "OpenCode didn't list its models");
        assert_eq!(error.reason, "it listed no models");
        assert!(started.elapsed() < limit + END_WAIT);
        let ran = bin.path().join("ran");
        for _ in 0..50 {
            if !alive(&ran.join("self")) && !alive(&ran.join("child")) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the server or its child is still running");
    }

    /// The wrong password is refused at once (401), not waited on.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_refused_password_is_an_error() {
        let seen = Arc::new(Mutex::new(Seen::default()));
        let port = serve(vec![fixture("free")], seen).await;
        let bin = tempfile::tempdir().unwrap();
        let program = stand_in(bin.path(), port);
        let cwd = tempfile::tempdir().unwrap();
        let error = read_list_with(
            &program,
            bin.path().as_os_str(),
            cwd.path(),
            "other",
            LIST_TIMEOUT,
        )
        .await
        .unwrap_err();
        assert!(error.reason.contains("401"), "{}", error.reason);
    }

    #[tokio::test]
    async fn not_on_path() {
        let bin = tempfile::tempdir().unwrap();
        let context =
            OpenCodeContext::new(crate::agent_tools::test_support::context(bin.path(), None));
        let error = read_list(&context).await.unwrap_err();
        assert_eq!(error.reason, "opencode isn't on PATH");
    }
}
