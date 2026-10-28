//! The one-time setup in Codex's home (ADP-07, d0085, d0086; T-0351): folder trust per project, trust for Otaq's
//! hooks, `rules/otaq.rules`, Otaq's local plugin; removal on disconnect (folder trust stays).
//!
//! Codex has no launch option for any of these (the hands-on report, rows 3–6), so they go into its home folder
//! (`$CODEX_HOME`, else `~/.codex`), each written carefully and only Otaq's own (d0086):
//! - **folder trust** ([`record_project`]): `[projects."<repo>"] trust_level = "trusted"` in `config.toml`, edited by
//!   [`config_toml`] (a worktree inherits its repo's trust). Stays when Codex is disconnected;
//! - **Otaq's plugin** (its hooks only, AGM-07: Codex finds a plugin's skills in its root `skills/` folder even
//!   without a `skills` entry, developers.openai.com/codex/plugins/build: "Portable packages discover skills from the
//!   root `skills/` directory", so the plugin is a folder without one, and the skills reach Codex as project skills
//!   copied into the session's folder): `plugin/codex/.codex-plugin/plugin.json`, offered by the local marketplace
//!   `plugin/codex/.agents/plugins/marketplace.json`, installed by Codex itself: `codex plugin marketplace add
//!   <data folder>/runtime/codex-plugin`, then `codex plugin add otaq@otaq-local` (which also refreshes Codex's cached
//!   copy). The marketplace is machine-wide, so it points at that one fixed folder, which every service start copies
//!   `plugin/codex` over (d0140, gap review R4, [`refresh_plugin`]): a new build changes nothing
//!   in Codex's config. A marketplace added from another folder (another data folder, or an older install) is
//!   removed first;
//! - **hook trust**, before the first launch: any untrusted hook stops Codex at startup ("Hooks need review"). A
//!   short-lived `codex app-server` lists the hooks (`hooks/list`: each one's `key`, `currentHash`,
//!   `trustStatus`), and Otaq's five (`otaq@otaq-local:plugin.json#hooks[0]:<event>:0:0`) are recorded as trusted
//!   with `config/batchWrite` (`hooks.state."<key>".trusted_hash`). The hook command lines never change (per-ticket
//!   data travels in the environment), so the trust holds;
//! - **`rules/otaq.rules`**: `prefix_rule(pattern=["otaq"], decision="allow")`, so `otaq` commands run without a
//!   prompt and every other command still asks.
//!
//! A `config.toml` that doesn't parse is never written, by Otaq or by the Codex commands it runs: the setup stops
//! and the page shows why. Codex's own commands write their entries the same careful way (checked: the user's
//! comments and other tables stay). Tests run a stand-in `codex` on a temporary `CODEX_HOME`.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use serde_json::{Value, json};

use super::app_server::AppServer;
use super::config_toml::{self, ConfigError};
use super::{COMMAND_NAME, CodexContext, DISPLAY_NAME};
use crate::agent_tools::{LaunchError, SetupEntry, SetupOutcome, SetupReport};
use crate::naming;

/// Otaq's hooks in its Codex plugin, by Codex's event names (the manifest's) and the names its trust keys use.
pub(super) const HOOK_EVENTS: [(&str, &str); 5] = [
    ("SessionStart", "session_start"),
    ("PermissionRequest", "permission_request"),
    ("Stop", "stop"),
    ("Interrupt", "interrupt"),
    ("UserPromptSubmit", "user_prompt_submit"),
];

/// The folder of Otaq's Codex plugin (its hooks) inside Otaq's plugin folder.
const PLUGIN_FOLDER: &str = "codex";

/// How long one Codex command (a plugin install) may take.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(60);

/// How long recording the hook trust may take (the app server answers in well under a second).
const TRUST_TIMEOUT: Duration = Duration::from_secs(20);

/// The local marketplace's name (`plugin/.agents/plugins/marketplace.json`).
pub(super) fn marketplace() -> String {
    format!("{}-local", naming::PLUGIN_NAMESPACE)
}

/// The installed plugin's id: `otaq@otaq-local`.
pub(super) fn plugin_id() -> String {
    format!("{}@{}", naming::PLUGIN_NAMESPACE, marketplace())
}

/// The start of every trust key of Otaq's plugin hooks.
fn hook_prefix() -> String {
    format!("{}:", plugin_id())
}

/// The trust key Codex gives Otaq's hook for `event` (snake case).
pub(super) fn hook_key(event: &str) -> String {
    format!("{}plugin.json#hooks[0]:{event}:0:0", hook_prefix())
}

/// `rules/otaq.rules` in Codex's home.
fn rules_file(home: &Path) -> PathBuf {
    home.join("rules")
        .join(format!("{}.rules", naming::COMMAND_NAME))
}

/// The rule that pre-approves only `otaq` commands.
pub(super) fn rules_text() -> String {
    format!(
        "prefix_rule(pattern=[\"{}\"], decision=\"allow\")\n",
        naming::COMMAND_NAME
    )
}

fn folder_trust() -> SetupEntry {
    SetupEntry {
        text: "trust for your projects".to_owned(),
        removed_on_disconnect: false,
    }
}

fn hooks_entry() -> SetupEntry {
    SetupEntry {
        text: "its hooks".to_owned(),
        removed_on_disconnect: true,
    }
}

fn rules_entry() -> SetupEntry {
    SetupEntry {
        text: format!("{} pre-approved", naming::COMMAND_NAME),
        removed_on_disconnect: true,
    }
}

fn plugin_entry() -> SetupEntry {
    SetupEntry {
        text: "its plugin".to_owned(),
        removed_on_disconnect: true,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Folder trust

/// `[projects."<root>"] trust_level = "trusted"` in `config.toml`. `None` without Codex's home folder (Codex
/// never ran here: nothing is created for it).
pub(super) fn record_project(context: &CodexContext, repo: &Path) -> Option<SetupOutcome> {
    let home = context.codex_home.as_deref()?;
    if !home.is_dir() {
        return None;
    }
    let key = config_toml::project_key(repo);
    let file = config_toml::path(home);
    let outcome = match config_toml::edit(&file, &mut |doc| {
        if config_toml::project_trusted(doc, &key) {
            return Ok(false);
        }
        config_toml::trust_project(doc, &key)?;
        Ok(true)
    }) {
        Ok(false) => SetupOutcome::AlreadyTrusted,
        Ok(true) => SetupOutcome::Recorded,
        Err(error) => SetupOutcome::Failed {
            reason: error.reason(),
        },
    };
    let file = file.display();
    match &outcome {
        SetupOutcome::AlreadyTrusted => {}
        SetupOutcome::Recorded => tracing::info!(key, %file, "recorded {DISPLAY_NAME}'s trust"),
        SetupOutcome::Failed { reason } => {
            tracing::warn!(key, %file, reason, "couldn't record {DISPLAY_NAME}'s trust")
        }
    }
    Some(outcome)
}

// ---------------------------------------------------------------------------------------------------------------
// The tool-wide setup

/// The pre-approval, the plugin and its hooks' trust, in that order; a step that fails says why. Idempotent: a
/// second run rewrites nothing that is already there (the plugin install refreshes Codex's copy). Nothing without
/// Codex's home folder.
pub(super) async fn set_up_tool(context: &CodexContext) -> SetupReport {
    let Some(home) = context.codex_home.clone() else {
        return SetupReport::default();
    };
    let rules = write_rules(&home);
    let (plugin, hooks) = match install_plugin(context, &home).await {
        Ok(()) => (Ok(()), trust_hooks(context, &home).await),
        Err(reason) => (Err(reason.clone()), Err(reason)),
    };
    let mut report = SetupReport::default();
    for (entry, result) in [
        (hooks_entry(), hooks),
        (rules_entry(), rules),
        (plugin_entry(), plugin),
    ] {
        match result {
            Ok(()) => report.done.push(entry),
            Err(reason) => report.failed.push((entry, reason)),
        }
    }
    tracing::info!(
        home = %home.display(),
        done = report.done.len(),
        failed = report.failed.len(),
        "{DISPLAY_NAME}'s one-time setup"
    );
    report
}

/// What Otaq's setup writes into Codex's home, for the Agent tools page.
pub(super) fn setup_entries() -> Vec<SetupEntry> {
    vec![folder_trust(), hooks_entry(), rules_entry(), plugin_entry()]
}

/// Writes `rules/otaq.rules` unless it already says exactly that.
fn write_rules(home: &Path) -> Result<(), String> {
    let file = rules_file(home);
    let text = rules_text();
    let failed = || format!("{DISPLAY_NAME}'s rules file couldn't be written");
    let original = config_toml::read_bytes(&file).map_err(|_| failed())?;
    if original.as_deref() == Some(text.as_bytes()) {
        return Ok(());
    }
    std::fs::create_dir_all(file.parent().unwrap_or(home)).map_err(|_| failed())?;
    match config_toml::replace(&file, &original, text.as_bytes(), &mut || {}) {
        Ok(true) => Ok(()),
        Ok(false) | Err(_) => Err(failed()),
    }
}

/// Copies Otaq's Codex plugin (`<plugin>/codex`) over the data folder's fixed `runtime/codex-plugin` (gap review
/// R4), as every service start does: Codex's marketplace keeps pointing at the same folder for every build. `None`
/// when `plugin_dir` has no Codex plugin.
pub(crate) fn refresh_plugin(
    data: &crate::data_dir::DataDir,
    plugin_dir: &Path,
) -> std::io::Result<Option<PathBuf>> {
    let source = plugin_dir.join(PLUGIN_FOLDER);
    if !source.is_dir() {
        return Ok(None);
    }
    let dest = data.codex_plugin_dir();
    crate::runtime::replace_dir(&source, &dest)?;
    Ok(Some(dest))
}

/// The folder Codex's marketplace points at: the data folder's fixed `runtime/codex-plugin` (gap review R4), when
/// Otaq's folders are known.
fn plugin_folder(context: &CodexContext) -> Option<PathBuf> {
    context
        .tool
        .launch
        .as_ref()
        .map(|_| context.tool.data.codex_plugin_dir())
}

/// Otaq's Codex plugin folder, with its manifest and local marketplace.
fn plugin_dir(context: &CodexContext) -> Result<PathBuf, String> {
    plugin_folder(context)
        .filter(|dir| {
            dir.join(".codex-plugin").join("plugin.json").is_file()
                && dir
                    .join(".agents")
                    .join("plugins")
                    .join("marketplace.json")
                    .is_file()
        })
        .ok_or_else(|| {
            format!(
                "{}'s plugin for {DISPLAY_NAME} wasn't found",
                naming::PRODUCT_NAME
            )
        })
}

/// The config file as a document, for the steps that make Codex write it; a file that doesn't parse stops them.
fn readable_config(home: &Path) -> Result<Option<toml_edit::DocumentMut>, String> {
    config_toml::read(&config_toml::path(home)).map_err(ConfigError::reason)
}

/// `codex plugin marketplace add <plugin folder>` (after removing a marketplace of the same name added from
/// another folder), then `codex plugin add otaq@otaq-local`.
async fn install_plugin(context: &CodexContext, home: &Path) -> Result<(), String> {
    let dir = plugin_dir(context)?;
    let doc = readable_config(home)?;
    let marketplace = marketplace();
    let dir_text = dir.to_string_lossy().into_owned();
    if let Some(source) = doc
        .as_ref()
        .and_then(|doc| config_toml::marketplace_source(doc, &marketplace))
        && !same_folder(Path::new(&source), &dir)
    {
        tracing::info!(
            old = source,
            new = dir_text,
            "{DISPLAY_NAME}'s marketplace moved"
        );
        run(
            context,
            home,
            &["plugin", "marketplace", "remove", &marketplace],
        )
        .await?;
    }
    run(context, home, &["plugin", "marketplace", "add", &dir_text]).await?;
    run(context, home, &["plugin", "add", &plugin_id()]).await
}

fn same_folder(a: &Path, b: &Path) -> bool {
    let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    real(a) == real(b)
}

/// Records Otaq's hooks as trusted through a short-lived app server.
async fn trust_hooks(context: &CodexContext, home: &Path) -> Result<(), String> {
    readable_config(home)?;
    let failed = |reason: String| {
        tracing::warn!(reason, "{DISPLAY_NAME}'s hook trust");
        format!("{DISPLAY_NAME} couldn't record the trust for its hooks: {reason}")
    };
    tokio::time::timeout(TRUST_TIMEOUT, async {
        let mut server = AppServer::start(context, home).await?;
        let result = async {
            let list = server
                .request("hooks/list", json!({ "cwds": [home] }))
                .await?;
            let edits = trust_edits(&list)?;
            if !edits.is_empty() {
                server
                    .request("config/batchWrite", json!({ "edits": edits }))
                    .await?;
            }
            tracing::info!(
                written = edits.len(),
                "{DISPLAY_NAME}'s hook trust recorded"
            );
            Ok(())
        }
        .await;
        server.stop().await;
        result
    })
    .await
    .unwrap_or_else(|_| Err(format!("no answer within {}s", TRUST_TIMEOUT.as_secs())))
    .map_err(failed)
}

/// The `config/batchWrite` edits that trust Otaq's hooks in a `hooks/list` answer: one per hook of Otaq's plugin
/// that isn't trusted yet (untrusted, or modified since), with the hash Codex lists. An error when one of Otaq's
/// five hooks isn't listed. Other hooks are never touched.
pub(super) fn trust_edits(list: &Value) -> Result<Vec<Value>, String> {
    let id = plugin_id();
    let ours: Vec<&Value> = list
        .get("data")
        .and_then(Value::as_array)
        .ok_or("its answer has no data")?
        .iter()
        .filter_map(|entry| entry.get("hooks").and_then(Value::as_array))
        .flatten()
        .filter(|hook| hook.get("pluginId").and_then(Value::as_str) == Some(id.as_str()))
        .collect();
    let mut edits = Vec::new();
    for (event, snake) in HOOK_EVENTS {
        let key = hook_key(snake);
        let hook = ours
            .iter()
            .find(|hook| hook.get("key").and_then(Value::as_str) == Some(key.as_str()))
            .ok_or_else(|| format!("its {event} hook isn't listed"))?;
        if hook.get("trustStatus").and_then(Value::as_str) == Some("trusted") {
            continue;
        }
        let hash = hook
            .get("currentHash")
            .and_then(Value::as_str)
            .filter(|h| !h.is_empty())
            .ok_or_else(|| format!("its {event} hook has no hash"))?;
        edits.push(json!({
            "keyPath": format!("hooks.state.\"{key}\".trusted_hash"),
            "value": hash,
            "mergeStrategy": "upsert",
        }));
    }
    Ok(edits)
}

/// Runs `codex <args>` from the context's PATH with Codex's home; its own message when it fails.
async fn run(context: &CodexContext, home: &Path, args: &[&str]) -> Result<(), String> {
    let program = crate::tools::find_on_path(COMMAND_NAME, &context.tool.path)
        .ok_or_else(|| format!("{DISPLAY_NAME} isn't on PATH"))?;
    let mut command = tokio::process::Command::from(
        crate::tools::cmd_shim::command(&program, &context.tool.path)
            .map_err(|e| e.message(Some(DISPLAY_NAME)))?,
    );
    command
        .args(args)
        .env("PATH", &context.tool.path)
        .env("CODEX_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let line = args.join(" ");
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .map_err(|_| format!("`{COMMAND_NAME} {line}` took too long"))?
        .map_err(|e| format!("`{COMMAND_NAME} {line}` couldn't start: {e}"))?;
    tracing::debug!(
        command = line,
        status = ?output.status.code(),
        stdout = %String::from_utf8_lossy(&output.stdout).trim(),
        stderr = %String::from_utf8_lossy(&output.stderr).trim(),
        "{DISPLAY_NAME} command"
    );
    if output.status.success() {
        Ok(())
    } else {
        Err(command_failed(&line, &output.stdout, &output.stderr))
    }
}

/// "`codex plugin add otaq@otaq-local` failed: <its last line>".
fn command_failed(line: &str, stdout: &[u8], stderr: &[u8]) -> String {
    let said = [stderr, stdout]
        .iter()
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .find_map(|text| {
            text.lines()
                .map(str::trim)
                .rfind(|l| !l.is_empty())
                .map(|l| l.trim_start_matches("Error: ").to_owned())
        });
    match said {
        Some(said) => format!("`{COMMAND_NAME} {line}` failed: {said}"),
        None => format!("`{COMMAND_NAME} {line}` failed"),
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The launch check

/// Why no Codex agent can start yet: the setup isn't (or no longer) in Codex's home, and Codex would stop at
/// "Hooks need review" or lack Otaq's hooks. Read from `config.toml` only (cheap): Otaq's plugin installed from
/// this plugin folder, and Otaq's five hooks with a recorded trust. `None` without Codex's home (tests).
pub(super) fn launch_refusal(context: &CodexContext) -> Option<LaunchError> {
    let home = context.codex_home.as_deref()?;
    let doc = match config_toml::read(&config_toml::path(home)) {
        Ok(doc) => doc.unwrap_or_default(),
        Err(error) => return Some(LaunchError(format!("{} · Re-check", error.reason()))),
    };
    let moved = match (
        config_toml::marketplace_source(&doc, &marketplace()),
        plugin_folder(context),
    ) {
        (Some(source), Some(folder)) => !same_folder(Path::new(&source), &folder),
        (source, _) => source.is_none(),
    };
    if moved || !config_toml::plugin_installed(&doc, &plugin_id()) {
        return Some(LaunchError(format!(
            "{}'s plugin isn't installed in {DISPLAY_NAME} yet · Re-check",
            naming::PRODUCT_NAME
        )));
    }
    let trusted = config_toml::trusted_hooks(&doc, &hook_prefix());
    if HOOK_EVENTS
        .iter()
        .any(|(_, snake)| !trusted.contains(&hook_key(snake)))
    {
        return Some(LaunchError(format!(
            "{DISPLAY_NAME}'s hooks aren't trusted yet · Re-check"
        )));
    }
    None
}

/// Whether the next check should run the setup again: Otaq's folders are known and a launch would be refused
/// for a missing setup ([`launch_refusal`]).
pub(super) fn setup_needed(context: &CodexContext) -> bool {
    context.tool.launch.is_some() && launch_refusal(context).is_some()
}

// ---------------------------------------------------------------------------------------------------------------
// Disconnect

/// Removes Otaq's own entries (d0086): its plugin (`codex plugin remove`, then the marketplace), its hooks' trust,
/// `rules/otaq.rules`. Folder trust stays. Blocking (the service runs it off the async threads). Without Codex on
/// PATH, the plugin's two tables are removed from `config.toml` directly.
pub(super) fn remove_setup(context: &CodexContext) -> SetupReport {
    let Some(home) = context.codex_home.clone() else {
        return SetupReport::default();
    };
    let file = config_toml::path(&home);
    let plugin = remove_plugin(context, &home, &file);
    let hooks = config_toml::edit(&file, &mut |doc| {
        Ok(config_toml::remove_hooks(doc, &hook_prefix()))
    })
    .map(|_| ())
    .map_err(ConfigError::reason);
    let rules = match std::fs::remove_file(rules_file(&home)) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            Err(format!("{DISPLAY_NAME}'s rules file couldn't be removed"))
        }
        _ => Ok(()),
    };
    let mut report = SetupReport::default();
    for (entry, result) in [
        (hooks_entry(), hooks),
        (rules_entry(), rules),
        (plugin_entry(), plugin),
    ] {
        match result {
            Ok(()) => report.done.push(entry),
            Err(reason) => report.failed.push((entry, reason)),
        }
    }
    tracing::info!(
        home = %home.display(),
        removed = report.done.len(),
        failed = report.failed.len(),
        "{DISPLAY_NAME}'s setup removed"
    );
    report
}

fn remove_plugin(context: &CodexContext, home: &Path, file: &Path) -> Result<(), String> {
    let doc = config_toml::read(file)
        .map_err(ConfigError::reason)?
        .unwrap_or_default();
    let (id, marketplace) = (plugin_id(), marketplace());
    let installed = config_toml::plugin_installed(&doc, &id);
    let added = config_toml::marketplace_source(&doc, &marketplace).is_some();
    if crate::tools::find_on_path(COMMAND_NAME, &context.tool.path).is_some() {
        if installed {
            run_blocking(context, home, &["plugin", "remove", &id])?;
        }
        if added {
            run_blocking(
                context,
                home,
                &["plugin", "marketplace", "remove", &marketplace],
            )?;
        }
    }
    // Whatever Codex left (or couldn't remove without its command).
    config_toml::edit(file, &mut |doc| {
        Ok(config_toml::remove_plugin(doc, &id, &marketplace))
    })
    .map(|_| ())
    .map_err(ConfigError::reason)
}

/// [`run`] for the blocking removal.
fn run_blocking(context: &CodexContext, home: &Path, args: &[&str]) -> Result<(), String> {
    let program = crate::tools::find_on_path(COMMAND_NAME, &context.tool.path)
        .ok_or_else(|| format!("{DISPLAY_NAME} isn't on PATH"))?;
    let line = args.join(" ");
    let mut child = crate::tools::cmd_shim::command(&program, &context.tool.path)
        .map_err(|e| e.message(Some(DISPLAY_NAME)))?
        .args(args)
        .env("PATH", &context.tool.path)
        .env("CODEX_HOME", home)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("`{COMMAND_NAME} {line}` couldn't start: {e}"))?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < COMMAND_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("`{COMMAND_NAME} {line}` took too long"));
            }
        }
    }
    let output = child
        .wait_with_output()
        .map_err(|e| format!("`{COMMAND_NAME} {line}` failed: {e}"))?;
    tracing::debug!(
        command = line,
        status = ?output.status.code(),
        stderr = %String::from_utf8_lossy(&output.stderr).trim(),
        "{DISPLAY_NAME} command"
    );
    if output.status.success() {
        Ok(())
    } else {
        Err(command_failed(&line, &output.stdout, &output.stderr))
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::fs;

    use super::super::app_server::tests::{saved, stand_in};
    use super::super::config_toml::tests::{USER_CONFIG, with_shop_web};
    use super::*;
    use crate::agent_tools::{LaunchContext, claude};

    /// Otaq's real plugin folder (the repository's).
    fn repo_plugin() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../plugin")
            .canonicalize()
            .unwrap()
    }

    /// A temporary Codex home with the user's realistic `config.toml`, a test PATH, and Otaq's plugin folder.
    struct Home {
        dir: tempfile::TempDir,
    }

    impl Home {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            fs::create_dir_all(dir.path().join("bin")).unwrap();
            fs::create_dir_all(dir.path().join("codex-home")).unwrap();
            fs::write(
                dir.path().join("codex-home").join(config_toml::FILE),
                USER_CONFIG,
            )
            .unwrap();
            Self { dir }
        }

        fn bin(&self) -> PathBuf {
            self.dir.path().join("bin")
        }

        fn home(&self) -> PathBuf {
            self.dir.path().join("codex-home")
        }

        fn log(&self) -> PathBuf {
            self.dir.path().join("log")
        }

        /// The fixed folder Codex's marketplace points at.
        fn codex_folder(&self) -> PathBuf {
            crate::agent_tools::test_support::context(&self.bin(), None)
                .data
                .codex_plugin_dir()
        }

        fn config(&self) -> String {
            fs::read_to_string(config_toml::path(&self.home())).unwrap()
        }

        fn write_config(&self, text: &str) {
            fs::write(config_toml::path(&self.home()), text).unwrap();
        }

        /// A service's context with Otaq's plugin folder `plugin_dir`, whose Codex plugin the service start copied
        /// into the data folder's fixed folder.
        fn context(&self, plugin_dir: &Path) -> CodexContext {
            let mut tool = crate::agent_tools::test_support::context(&self.bin(), None);
            refresh_plugin(&tool.data, plugin_dir).unwrap();
            tool.launch = Some(LaunchContext {
                bin_dir: self.bin(),
                plugin_dir: plugin_dir.to_path_buf(),
                path: OsString::new(),
                claude: claude::ClaudeLaunch {
                    program: vec!["claude".into()],
                    config_dir: None,
                },
            });
            CodexContext {
                tool,
                codex_home: Some(self.home()),
            }
        }

        /// A stand-in `codex` that, like Codex, appends its plugin tables on `plugin marketplace add` and `plugin
        /// add`, and answers `hooks/list` with `hooks` (a saved answer's result).
        fn codex(&self, hooks: Value) {
            let (marketplace, id) = (marketplace(), plugin_id());
            let file = config_toml::path(&self.home());
            let plugin = format!(
                r#"case "$2 $3" in
    "marketplace add") /usr/bin/grep -q '^\[marketplaces.{marketplace}\]' '{f}' || printf '\n[marketplaces.{marketplace}]\nsource_type = "local"\nsource = "%s"\n' "$(printf '%s' "$4" | /usr/bin/sed 's/\\/\\\\/g')" >> '{f}' ;;
    "add {id}") /usr/bin/grep -q '^\[plugins."{id}"\]' '{f}' || printf '\n[plugins."{id}"]\nenabled = true\n' >> '{f}' ;;
  esac"#,
                f = file.display()
            );
            stand_in(
                &self.bin(),
                &self.log(),
                &[
                    (r#""method":"hooks/list""#, hooks),
                    (
                        r#""method":"config/batchWrite""#,
                        saved("config_batch_write.json")["result"].clone(),
                    ),
                ],
                &plugin,
            );
        }

        /// What Codex's `config/batchWrite` would have written: each edit's trusted hash.
        fn apply_batch_write(&self) {
            let log = fs::read_to_string(self.log()).unwrap();
            let line = log
                .lines()
                .find(|l| l.contains("config/batchWrite"))
                .expect("a batchWrite");
            let request: Value = serde_json::from_str(line).unwrap();
            let file = config_toml::path(&self.home());
            config_toml::edit(&file, &mut |doc| {
                for edit in request["params"]["edits"].as_array().unwrap() {
                    let path = edit["keyPath"].as_str().unwrap();
                    let key = path
                        .strip_prefix("hooks.state.\"")
                        .and_then(|p| p.strip_suffix("\".trusted_hash"))
                        .unwrap();
                    let mut table = toml_edit::Table::new();
                    table.insert(
                        "trusted_hash",
                        toml_edit::value(edit["value"].as_str().unwrap()),
                    );
                    let state = doc
                        .as_table_mut()
                        .entry("hooks")
                        .or_insert(toml_edit::table())
                        .as_table_mut()
                        .unwrap()
                        .entry("state")
                        .or_insert(toml_edit::table())
                        .as_table_mut()
                        .unwrap();
                    state.set_implicit(true);
                    state.insert(key, toml_edit::Item::Table(table));
                }
                Ok(true)
            })
            .unwrap();
        }
    }

    fn untrusted() -> Value {
        saved("hooks_list-untrusted.json")["result"].clone()
    }

    fn texts(entries: &[SetupEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.text.as_str()).collect()
    }

    fn pre_approved() -> String {
        format!("{} pre-approved", naming::COMMAND_NAME)
    }

    /// The page's line (15 `bGHVl`): trust for your projects stays on disconnect; the rest is removed.
    #[test]
    fn the_entries_otaq_adds() {
        let entries = setup_entries();
        assert_eq!(
            texts(&entries),
            [
                "trust for your projects",
                "its hooks",
                &pre_approved(),
                "its plugin"
            ]
        );
        assert_eq!(
            entries
                .iter()
                .map(|e| e.removed_on_disconnect)
                .collect::<Vec<_>>(),
            [false, true, true, true]
        );
    }

    /// The manifest beside Claude Code's: no skills (AGM-07), and exactly the five hooks the adapter maps, each with
    /// its fixed command line; the marketplace offers that folder under the name the setup installs.
    #[test]
    fn the_plugin_manifest_and_its_marketplace() {
        let dir = repo_plugin();
        let manifest: Value = serde_json::from_str(
            &fs::read_to_string(dir.join("codex/.codex-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        let claude: Value = serde_json::from_str(
            &fs::read_to_string(dir.join("claude/.claude-plugin/plugin.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(manifest["name"], naming::PLUGIN_NAMESPACE);
        assert_eq!(manifest["version"], claude["version"]);
        assert!(manifest.get("skills").is_none());
        assert!(!dir.join("codex/skills").exists(), "hooks only");
        let hooks = manifest["hooks"]["hooks"].as_object().unwrap();
        let mut names: Vec<&str> = hooks.keys().map(String::as_str).collect();
        names.sort_unstable();
        let mut expected: Vec<&str> = HOOK_EVENTS.iter().map(|(event, _)| *event).collect();
        expected.sort_unstable();
        assert_eq!(names, expected);
        for (event, _) in HOOK_EVENTS {
            let group = &hooks[event];
            assert_eq!(group.as_array().unwrap().len(), 1, "{event}");
            assert_eq!(
                group[0]["hooks"][0]["command"],
                format!(
                    "command -v {c} >/dev/null 2>&1 || exit 0; {c} hook --tool codex {event}",
                    c = naming::COMMAND_NAME
                )
            );
            assert_eq!(group[0]["hooks"][0]["type"], "command");
        }
        let market: Value = serde_json::from_str(
            &fs::read_to_string(dir.join("codex/.agents/plugins/marketplace.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(market["name"], marketplace());
        assert_eq!(market["plugins"][0]["name"], naming::PLUGIN_NAMESPACE);
        assert_eq!(market["plugins"][0]["source"]["source"], "local");
        assert_eq!(market["plugins"][0]["source"]["path"], "./");
    }

    /// The saved `hooks/list` answer (Codex 0.157.1, the plugin just installed) gives one upsert per Otaq hook.
    #[test]
    fn the_trust_edits_from_a_saved_answer() {
        let edits = trust_edits(&untrusted()).unwrap();
        assert_eq!(edits.len(), 5);
        assert_eq!(
            edits[2],
            json!({
                "keyPath": format!("hooks.state.\"{}\".trusted_hash", hook_key("stop")),
                "value": "sha256:a80c83798b2819d6154ebe38a7ebcb666458f0345580d9ae1fa9de2d2b7517a1",
                "mergeStrategy": "upsert",
            })
        );
        let paths: Vec<&str> = edits
            .iter()
            .map(|e| e["keyPath"].as_str().unwrap())
            .collect();
        for (_, snake) in HOOK_EVENTS {
            assert!(
                paths.contains(
                    &format!("hooks.state.\"{}\".trusted_hash", hook_key(snake)).as_str()
                )
            );
        }
    }

    /// With the user's own hook untrusted, Otaq's four trusted and one modified (its hash changed): only the
    /// modified one is written; the user's hook is never touched.
    #[test]
    fn only_otaqs_untrusted_or_modified_hooks_are_written() {
        let edits = trust_edits(&saved("hooks_list-mixed.json")["result"]).unwrap();
        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0]["keyPath"],
            format!("hooks.state.\"{}\".trusted_hash", hook_key("stop"))
        );
        assert!(!edits[0].to_string().contains("hooks.json"));
    }

    #[test]
    fn a_missing_hook_is_a_failure() {
        let mut list = untrusted();
        list["data"][0]["hooks"]
            .as_array_mut()
            .unwrap()
            .retain(|h| h["eventName"] != "interrupt");
        assert_eq!(
            trust_edits(&list),
            Err("its Interrupt hook isn't listed".into())
        );
        assert!(trust_edits(&json!({})).is_err());
    }

    /// d0085: only the new project's table is added; the user's lines stay byte for byte; a second time is
    /// "already trusted"; no Codex home → nothing (nothing created).
    #[test]
    fn folder_trust_writes_only_its_table() {
        let home = Home::new();
        let context = home.context(&repo_plugin());
        let repo = Path::new("/tmp/m23/shop-web");
        assert_eq!(record_project(&context, repo), Some(SetupOutcome::Recorded));
        assert_eq!(home.config(), with_shop_web());
        assert_eq!(
            record_project(&context, repo),
            Some(SetupOutcome::AlreadyTrusted)
        );
        assert_eq!(
            record_project(&context, Path::new("/Users/me/other")),
            Some(SetupOutcome::AlreadyTrusted)
        );

        let gone = CodexContext {
            codex_home: Some(home.dir.path().join("never-ran")),
            ..context.clone()
        };
        assert_eq!(record_project(&gone, repo), None);
        assert!(!home.dir.path().join("never-ran").exists());
        let none = CodexContext {
            codex_home: None,
            ..context
        };
        assert_eq!(record_project(&none, repo), None);
    }

    #[test]
    fn an_unparseable_config_is_never_written() {
        let home = Home::new();
        let broken = "model = \"gpt-5.6-terra\"\n[projects.\"/a\"\ntrust_level = \n";
        home.write_config(broken);
        let context = home.context(&repo_plugin());
        assert_eq!(
            record_project(&context, Path::new("/tmp/m23/shop-web")),
            Some(SetupOutcome::Failed {
                reason: "Codex's config file couldn't be read".into()
            })
        );
        assert_eq!(home.config(), broken);
    }

    /// Connecting Codex (ADP-07): the rules file, the plugin from Otaq's folder, the trust of its five hooks
    /// through the app server; the user's `config.toml` lines are kept; a second run writes nothing new.
    #[tokio::test]
    async fn the_setup_writes_only_otaqs_entries() {
        let home = Home::new();
        home.codex(untrusted());
        let plugin = repo_plugin();
        let context = home.context(&plugin);
        assert_eq!(
            launch_refusal(&context),
            Some(LaunchError(format!(
                "{}'s plugin isn't installed in Codex yet · Re-check",
                naming::PRODUCT_NAME
            )))
        );

        let report = set_up_tool(&context).await;
        assert_eq!(report.failed, []);
        assert_eq!(
            texts(&report.done),
            ["its hooks", &pre_approved(), "its plugin"]
        );
        assert_eq!(
            fs::read_to_string(rules_file(&home.home())).unwrap(),
            format!(
                "prefix_rule(pattern=[\"{}\"], decision=\"allow\")\n",
                naming::COMMAND_NAME
            )
        );
        let log = fs::read_to_string(home.log()).unwrap();
        let commands: Vec<&str> = log.lines().filter(|l| !l.starts_with('{')).collect();
        assert_eq!(
            commands,
            [
                format!("plugin marketplace add {}", home.codex_folder().display()).as_str(),
                &format!("plugin add {}", plugin_id()),
                "app-server",
            ]
        );
        assert!(log.contains(r#""method":"hooks/list""#));
        let batch: Value = serde_json::from_str(
            log.lines()
                .find(|l| l.contains("config/batchWrite"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            batch["params"]["edits"],
            json!(trust_edits(&untrusted()).unwrap())
        );

        // Codex added only its plugin tables after the user's lines (the stand-in does what Codex does).
        let config = home.config();
        assert!(config.starts_with(USER_CONFIG), "{config}");
        assert_eq!(
            launch_refusal(&context),
            Some(LaunchError(
                "Codex's hooks aren't trusted yet · Re-check".into()
            ))
        );
        assert!(setup_needed(&context));
        home.apply_batch_write();
        assert_eq!(launch_refusal(&context), None);
        assert!(!setup_needed(&context));
        let no_folders = CodexContext {
            tool: crate::agent_tools::test_support::context(&home.bin(), None),
            ..context.clone()
        };
        assert!(!setup_needed(&no_folders), "nothing a setup could install");

        // Again: the rules file is left as it is; Codex is asked again (its install refreshes its copy).
        let before = fs::metadata(rules_file(&home.home()))
            .unwrap()
            .modified()
            .unwrap();
        let again = set_up_tool(&context).await;
        assert_eq!(again.failed, []);
        assert_eq!(
            fs::metadata(rules_file(&home.home()))
                .unwrap()
                .modified()
                .unwrap(),
            before
        );
    }

    /// A new build (its plugin in another runtime folder) refreshes the fixed folder: Codex's config doesn't change,
    /// launches aren't refused and no setup runs again (gap review R4).
    #[tokio::test]
    async fn a_new_builds_plugin_keeps_codexs_marketplace() {
        let home = Home::new();
        home.codex(untrusted());
        let context = home.context(&repo_plugin());
        assert_eq!(set_up_tool(&context).await.failed, []);
        home.apply_batch_write();
        assert_eq!(launch_refusal(&context), None);
        let config = home.config();
        assert!(
            config.contains(&format!(
                "source = \"{}\"",
                home.codex_folder()
                    .display()
                    .to_string()
                    .replace('\\', "\\\\")
            )),
            "{config}"
        );

        let next = tempfile::tempdir().unwrap();
        refresh_plugin(&context.tool.data, &next_build_plugin(next.path())).unwrap();
        let mut moved = context.clone();
        moved.tool.launch.as_mut().unwrap().plugin_dir = next.path().join("plugin");
        assert_eq!(launch_refusal(&moved), None);
        assert!(!setup_needed(&moved));
        assert_eq!(home.config(), config);
        assert!(
            home.codex_folder()
                .join(".codex-plugin")
                .join("next-build")
                .is_file()
        );
    }

    /// The repository's plugin copied under `root`, as another build's runtime folder holds it, with a file only
    /// that build has.
    fn next_build_plugin(root: &Path) -> PathBuf {
        fn copy(from: &Path, to: &Path) {
            fs::create_dir_all(to).unwrap();
            for entry in fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    copy(&entry.path(), &to.join(entry.file_name()));
                } else {
                    fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
                }
            }
        }
        let plugin = root.join("plugin");
        copy(&repo_plugin(), &plugin);
        fs::write(
            plugin
                .join("codex")
                .join(".codex-plugin")
                .join("next-build"),
            "",
        )
        .unwrap();
        plugin
    }

    /// Otaq's plugin folder moved (another install): the old marketplace is removed before the new one is added,
    /// and a launch is refused until then.
    #[tokio::test]
    async fn a_moved_plugin_folder_is_added_again() {
        let home = Home::new();
        home.codex(untrusted());
        home.write_config(&format!(
            "{USER_CONFIG}\n[marketplaces.{}]\nsource_type = \"local\"\nsource = \"/old/plugin\"\n\n[plugins.\"{}\"]\nenabled = true\n",
            marketplace(),
            plugin_id()
        ));
        let context = home.context(&repo_plugin());
        assert!(launch_refusal(&context).is_some());
        let report = set_up_tool(&context).await;
        assert_eq!(report.failed, []);
        let log = fs::read_to_string(home.log()).unwrap();
        assert!(
            log.starts_with(&format!("plugin marketplace remove {}\n", marketplace())),
            "{log}"
        );
    }

    /// A trust that can't be recorded is a setup failure for the page and keeps launches refused.
    #[tokio::test]
    async fn a_failed_trust_is_a_setup_failure_and_a_launch_refusal() {
        let home = Home::new();
        let mut list = untrusted();
        list["data"][0]["hooks"].as_array_mut().unwrap().clear();
        home.codex(list);
        let context = home.context(&repo_plugin());
        let report = set_up_tool(&context).await;
        assert_eq!(texts(&report.done), [pre_approved().as_str(), "its plugin"]);
        assert_eq!(
            report.failed,
            [(
                hooks_entry(),
                "Codex couldn't record the trust for its hooks: its SessionStart hook isn't listed"
                    .to_owned()
            )]
        );
        assert_eq!(
            launch_refusal(&context),
            Some(LaunchError(
                "Codex's hooks aren't trusted yet · Re-check".into()
            ))
        );
    }

    /// A failed install fails the plugin and the hooks with Codex's own message; no app server is started.
    #[tokio::test]
    async fn a_failed_install_says_why() {
        let home = Home::new();
        stand_in(
            &home.bin(),
            &home.log(),
            &[],
            r#"echo "Error: marketplace is broken" >&2; exit 1"#,
        );
        let context = home.context(&repo_plugin());
        let report = set_up_tool(&context).await;
        let reason = format!(
            "`codex plugin marketplace add {}` failed: marketplace is broken",
            home.codex_folder().display()
        );
        assert_eq!(
            report.failed,
            [(hooks_entry(), reason.clone()), (plugin_entry(), reason)]
        );
        assert!(
            !fs::read_to_string(home.log())
                .unwrap()
                .contains("app-server")
        );
    }

    /// d0085: with a `config.toml` that doesn't parse, no Codex command runs (they would rewrite it) and the file
    /// stays as it is; only the separate rules file is written.
    #[tokio::test]
    async fn an_unparseable_config_stops_the_setup() {
        let home = Home::new();
        home.codex(untrusted());
        let broken = "model = \"gpt-5.6-terra\"\n[projects\n";
        home.write_config(broken);
        let context = home.context(&repo_plugin());
        let report = set_up_tool(&context).await;
        let reason = "Codex's config file couldn't be read".to_owned();
        assert_eq!(texts(&report.done), [pre_approved().as_str()]);
        assert_eq!(
            report.failed,
            [
                (hooks_entry(), reason.clone()),
                (plugin_entry(), reason.clone())
            ]
        );
        assert!(!home.log().exists(), "no Codex command ran");
        assert_eq!(home.config(), broken);
        assert_eq!(
            launch_refusal(&context),
            Some(LaunchError(format!("{reason} · Re-check")))
        );
        let removed = remove_setup(&context);
        assert_eq!(home.config(), broken);
        assert_eq!(texts(&removed.done), [pre_approved().as_str()]);
    }

    /// Without Otaq's plugin folder the plugin and the hooks can't be set up; nothing without Codex's home.
    #[tokio::test]
    async fn no_plugin_folder_or_no_home() {
        let home = Home::new();
        home.codex(untrusted());
        let empty = tempfile::tempdir().unwrap();
        let report = set_up_tool(&home.context(empty.path())).await;
        assert_eq!(texts(&report.done), [pre_approved().as_str()]);
        assert_eq!(
            report.failed[0].1,
            format!("{}'s plugin for Codex wasn't found", naming::PRODUCT_NAME)
        );
        let none = CodexContext {
            codex_home: None,
            ..home.context(&repo_plugin())
        };
        assert_eq!(set_up_tool(&none).await, SetupReport::default());
        assert_eq!(remove_setup(&none), SetupReport::default());
        assert_eq!(launch_refusal(&none), None);
    }

    /// Disconnect (d0086): Codex removes its plugin, Otaq removes its hooks' trust, the plugin tables Codex left
    /// and the rules file; the folder trust and every user line stay.
    #[tokio::test]
    async fn disconnect_removes_otaqs_entries_and_keeps_folder_trust() {
        let home = Home::new();
        home.codex(untrusted());
        let context = home.context(&repo_plugin());
        assert_eq!(
            record_project(&context, Path::new("/tmp/m23/shop-web")),
            Some(SetupOutcome::Recorded)
        );
        let with_trust = home.config();
        assert!(set_up_tool(&context).await.failed.is_empty());
        home.apply_batch_write();
        assert_ne!(home.config(), with_trust);
        let _ = fs::remove_file(home.log());

        let report = tokio::task::spawn_blocking(move || {
            let report = remove_setup(&context);
            (report, context)
        })
        .await
        .unwrap();
        let (report, context) = report;
        assert_eq!(report.failed, []);
        assert_eq!(
            texts(&report.done),
            ["its hooks", &pre_approved(), "its plugin"]
        );
        assert_eq!(home.config(), with_trust, "only the folder trust is left");
        assert!(!rules_file(&home.home()).exists());
        let log = fs::read_to_string(home.log()).unwrap();
        assert_eq!(
            log,
            format!(
                "plugin remove {}\nplugin marketplace remove {}\n",
                plugin_id(),
                marketplace()
            )
        );
        // Again: nothing left to remove, nothing fails, Codex isn't asked.
        let _ = fs::remove_file(home.log());
        assert_eq!(remove_setup(&context).failed, []);
        assert!(!home.log().exists());
        assert_eq!(home.config(), with_trust);
    }
}
