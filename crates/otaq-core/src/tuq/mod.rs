//! tuq, the user's own (tuq decisions 100, 102, 103, 104, 106; tech spec §3, §5): a stage naming `tuq:<name>` runs
//! the tuq the user has installed in the stage's tool, and Otaq installs or updates it when that tuq doesn't have the
//! skill.
//!
//! - **Your tuq, per tool** ([`installed`], decision 100): tuq installed as a plugin named `tuq` in Claude Code, Codex
//!   or Copilot, and on OpenCode the `tuq-<name>` folders tuq's installer copies into OpenCode's personal skills
//!   folder, told apart from the user's own skills by the `metadata.tuq-release` stamp every tuq `SKILL.md` carries.
//!   It is read through the skill finder ([`crate::skills::found`]), which already reads every tool's plugins and
//!   skills folders. Loose or repository copies of tuq's skills are the user's own skills, by their bare names.
//! - **Its version** is that stamp (`2026.11.14`, tech spec §5), the same on every tool ([`TuqInstalls::versions`]).
//! - **Each tool's names** (decision 105): Claude Code and Codex run tuq's plugin skills in its namespace
//!   (`/tuq:product-spec`, `$tuq:product-spec`); on Copilot and OpenCode each skill carries the prefix in its own name,
//!   `tuq-product-spec` ([`installed_name`]), which Copilot types under the plugin's name (`/tuq:tuq-product-spec`)
//!   and OpenCode loads by name.
//! - **Installed when missing** ([`TuqInstalls::ensure`], decisions 102, 103): before a stage starts, and before a
//!   `tuq:` skill is typed into a running session, the tool's tuq is asked for the skill. Missing (no tuq, or an older
//!   one), tuq's own installer runs for that tool ([`Installer::Published`]: `install.sh` / `install.ps1 --tool
//!   <tool>` from tuq's published repository, decision 106), one install at a time, and the tool's tuq is asked
//!   again. The ticket's History says "tuq updated from … to …" ([`Updated::text`]). Still missing, or the installer
//!   failed (offline, the tool refused): the stage waits in Needs you, "Your tuq doesn't have `tuq:<name>`"
//!   ([`crate::flow::proposal::missing_skill_text`], decision 083). A failed or fruitless install isn't tried again
//!   for that tool for a while ([`RETRY_AFTER`]), so a held stage that is checked again on every lift doesn't run the
//!   installer each time; **Check again** forgets that ([`TuqInstalls::forget_failures`]).
//! - **Its commands** (`tuq`, `tuq-plan`) are in `~/.tuq/bin` (`%LOCALAPPDATA%\tuq\bin`), where tuq's installer puts
//!   them: every launch puts that folder on the session's PATH ([`launch_path`]; the installer's own PATH change
//!   reaches new terminals, not the running service) and pre-approves [`COMMANDS`] as it does `otaq` (decision 077).
//!
//! Without a home folder (the service's default, and in tests) Otaq reads no tool's files, so it can't look for tuq:
//! a `tuq:` skill then counts as there, as the tool will say when it isn't.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::agent_tools::{self, Tool};
use crate::skills::found::{self, Homes};

/// tuq's namespace, its plugin's name: `tuq:product-spec` (decision 063).
pub const NAMESPACE: &str = "tuq";

/// tuq's commands, pre-approved in every session like `otaq` (decision 077).
pub const COMMANDS: [&str; 2] = ["tuq", "tuq-plan"];

/// Where tuq's installer is published, without its extension (`.sh`, `.ps1`).
const INSTALLER_URL: &str = crate::naming::TUQ_INSTALLER_URL;

/// The skills tuq has (release 2026.11.14), listed as Otaq's for a flow before any tool has tuq installed: what Otaq's
/// presets and ready stages name.
pub const KNOWN: &[&str] = &["milestone-plan", "product-spec", "step-back", "tech-spec"];

/// How long a failed or fruitless install keeps the installer from running again for the same tool, unless the user
/// clicks Check again.
pub const RETRY_AFTER: Duration = Duration::from_secs(10 * 60);

/// How long tuq's installer may take.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// The skill `name` names in tuq's namespace (`tuq:product-spec` → `product-spec`); `None` for any other name.
pub fn skill_of(name: &str) -> Option<&str> {
    let skill = name.trim().strip_prefix(NAMESPACE)?.strip_prefix(':')?;
    let plain = !skill.is_empty()
        && skill
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    plain.then_some(skill)
}

/// Whether `name` is a `tuq:` skill.
pub fn is_tuq(name: &str) -> bool {
    skill_of(name).is_some()
}

/// Whether tuq names its skills with the prefix on `tool` (decision 105): Copilot and OpenCode.
fn prefixed(tool: Tool) -> bool {
    tool.tuq_prefixed()
}

/// The name tuq's skill `skill` (`product-spec`) has in `tool`'s install: `product-spec` in the `tuq` plugin of
/// Claude Code and Codex, `tuq-product-spec` on Copilot and OpenCode (decision 105).
pub fn installed_name(tool: Tool, skill: &str) -> String {
    if prefixed(tool) {
        format!("{NAMESPACE}-{skill}")
    } else {
        skill.to_owned()
    }
}

/// The folder tuq's installer puts `tuq` and `tuq-plan` in: `%LOCALAPPDATA%\tuq\bin` on Windows, else
/// `~/.tuq/bin`. `None` when the variable it is under isn't set.
pub fn bin_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        let local = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?;
        Some(PathBuf::from(local).join(NAMESPACE).join("bin"))
    } else {
        Some(std::env::home_dir()?.join(".tuq").join("bin"))
    }
}

/// A session's PATH: the `otaq` folder first, then tuq's commands' folder ([`bin_dir`]), then `path`.
pub fn launch_path(otaq_bin: &Path, path: &OsStr) -> OsString {
    let mut parts = vec![otaq_bin.to_path_buf()];
    parts.extend(bin_dir());
    if !path.is_empty() {
        parts.extend(std::env::split_paths(path));
    }
    std::env::join_paths(parts).unwrap_or_else(|_| path.to_owned())
}

/// A tool's tuq, as installed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TuqInstall {
    /// The release its skills were stamped with ([`found::FrontMatter::tuq_release`]); `None` for a tuq from before
    /// the stamp.
    pub version: Option<String>,
    /// Its skills, by their names in tuq's namespace (`product-spec`, for `tuq:product-spec`), with their
    /// descriptions. On Copilot and OpenCode only the prefixed ones count (`tuq-product-spec`): an older tuq with
    /// plain names there is updated (decision 105).
    pub skills: Vec<(String, Option<String>)>,
}

impl TuqInstall {
    /// Whether it has `skill` (`product-spec`).
    pub fn has(&self, skill: &str) -> bool {
        self.skills.iter().any(|(name, _)| name == skill)
    }
}

/// `tool`'s tuq under `homes` (decision 100): tuq's plugin in Claude Code, Codex and Copilot, tuq's stamped
/// `tuq-<name>` folders in OpenCode's personal skills folder ([`Tool::tuq_as_plugin`]). `None` when the tool has none. Blocking file reads.
pub fn installed(homes: &Homes, tool: Tool) -> Option<TuqInstall> {
    let folders = homes.tools.as_ref()?;
    let reads: Vec<found::Read> = if !tool.tuq_as_plugin() {
        found::read_folder(folders.of(tool))
            .into_iter()
            .filter(is_tuqs_folder)
            .collect()
    } else {
        let plugins: Vec<_> = agent_tools::plugin_skill_folders(folders, None)
            .into_iter()
            .filter(|(on, plugin)| *on == tool && plugin.plugin == NAMESPACE)
            .collect();
        if plugins.is_empty() {
            return None;
        }
        plugins
            .iter()
            .flat_map(|(_, plugin)| found::read_folder(&plugin.folder))
            .collect()
    };
    if !tool.tuq_as_plugin() && reads.is_empty() {
        return None;
    }
    let version = reads.iter().filter_map(|r| r.tuq_release.clone()).max();
    let skills = reads
        .into_iter()
        .filter_map(|read| {
            let name = if prefixed(tool) {
                read.name.strip_prefix(&format!("{NAMESPACE}-"))?.to_owned()
            } else {
                read.name
            };
            Some((name, read.description))
        })
        .collect();
    Some(TuqInstall { version, skills })
}

/// Whether a skill read from a tool's personal skills folder is one of tuq's (a tool whose tuq isn't a plugin): a
/// `tuq-<name>` folder carrying tuq's stamp. Without the stamp it is the user's own.
pub(crate) fn is_tuqs_folder(read: &found::Read) -> bool {
    read.tuq_release.is_some() && read.name.starts_with(&format!("{NAMESPACE}-"))
}

/// Every tool's tuq under `homes`, in [`Tool::ALL`]'s order, for the ones that have one. Blocking file reads.
pub fn installs(homes: &Homes) -> Vec<(Tool, TuqInstall)> {
    Tool::ALL
        .iter()
        .filter_map(|&tool| installed(homes, tool).map(|install| (tool, install)))
        .collect()
}

/// tuq installed or updated for a stage: from which release to which.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Updated {
    pub tool: Tool,
    /// The release before; `None` when the tool had no tuq (or one from before the stamp).
    pub from: Option<String>,
    /// The release now.
    pub to: Option<String>,
    /// Whether the tool had a tuq before.
    pub had: bool,
}

impl Updated {
    /// The ticket's History line: "tuq updated from 2026.11.10 to 2026.11.14", "tuq 2026.11.14 installed".
    pub fn text(&self) -> String {
        let to = self.to.as_deref();
        match (self.had, self.from.as_deref(), to) {
            (false, _, Some(to)) => format!("{NAMESPACE} {to} installed"),
            (false, _, None) => format!("{NAMESPACE} installed"),
            (true, Some(from), Some(to)) if from != to => {
                format!("{NAMESPACE} updated from {from} to {to}")
            }
            (true, _, Some(to)) => format!("{NAMESPACE} updated to {to}"),
            (true, _, None) => format!("{NAMESPACE} updated"),
        }
    }
}

/// What [`TuqInstalls::ensure`] answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tuq {
    /// The tool's tuq has the skill; `version` is its release, `updated` says when Otaq just installed or updated it.
    Ready {
        version: Option<String>,
        updated: Option<Updated>,
    },
    /// It doesn't, even after tuq's installer ran (or the installer failed): why, for the log.
    Missing { reason: String },
}

/// How tuq gets installed or updated on a tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Installer {
    /// tuq's published installer for this system, as tuq's README says to run it (decision 106): `install.sh` piped
    /// from `curl` on macOS and Linux, `install.ps1` through `irm` in Windows PowerShell; `--tool <tool>`.
    Published,
    /// A program run as `<program> <args…> --tool <tool>` (tests' stand-in installer).
    Program { program: PathBuf, args: Vec<String> },
}

impl Installer {
    /// The program and arguments that install tuq on `tool`.
    fn command(&self, tool: Tool) -> (OsString, Vec<OsString>) {
        match self {
            Self::Published if cfg!(windows) => (
                "powershell.exe".into(),
                [
                    "-NoProfile",
                    "-NonInteractive",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                ]
                .into_iter()
                .map(OsString::from)
                .chain([OsString::from(format!(
                    "$ErrorActionPreference = 'Stop'; try {{ & ([scriptblock]::Create((irm '{INSTALLER_URL}.ps1'))) \
                     --tool {} }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}",
                    tool.as_str()
                ))])
                .collect(),
            ),
            // `curl … | sh` would hide a failed download (sh reads nothing and succeeds), so the script is fetched
            // first and run with its arguments.
            Self::Published => (
                "sh".into(),
                [
                    "-c",
                    "script=$(curl -fsSL --retry 2 \"$0\") || exit 1; exec sh -c \"$script\" install.sh --tool \"$1\"",
                ]
                .into_iter()
                .map(OsString::from)
                .chain([format!("{INSTALLER_URL}.sh").into(), tool.as_str().into()])
                .collect(),
            ),
            Self::Program { program, args } => (
                program.clone().into_os_string(),
                args.iter()
                    .map(OsString::from)
                    .chain(["--tool".into(), tool.as_str().into()])
                    .collect(),
            ),
        }
    }
}

/// The tuq of each tool, and installing it when a stage needs a skill it doesn't have. One per service (the board
/// holds it); one install runs at a time.
pub struct TuqInstalls {
    /// What installs tuq; `None` installs nothing (the default, and in tests unless they set a stand-in).
    installer: Mutex<Option<Installer>>,
    /// The PATH the installer runs with (the one the tools are found on); empty: the service's own.
    path: Mutex<OsString>,
    /// Held while a tool's tuq is checked and installed: one install at a time, and a second stage asking meanwhile
    /// finds the first one's install.
    one: tokio::sync::Mutex<()>,
    /// The last failed or fruitless install per tool, and why ([`RETRY_AFTER`]).
    failed: Mutex<HashMap<Tool, (Instant, String)>>,
}

impl Default for TuqInstalls {
    fn default() -> Self {
        Self::new(None)
    }
}

impl std::fmt::Debug for TuqInstalls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TuqInstalls").finish_non_exhaustive()
    }
}

impl TuqInstalls {
    pub fn new(installer: Option<Installer>) -> Self {
        Self {
            installer: Mutex::new(installer),
            path: Mutex::new(OsString::new()),
            one: tokio::sync::Mutex::new(()),
            failed: Mutex::new(HashMap::new()),
        }
    }

    /// Sets what installs tuq (the service: [`Installer::Published`]; tests: a stand-in).
    pub fn set_installer(&self, installer: Option<Installer>) {
        *self.installer.lock().unwrap_or_else(|e| e.into_inner()) = installer;
    }

    /// Sets the PATH the installer runs with: the one the agent tools are found on, so it finds `claude`.
    pub fn set_path(&self, path: OsString) {
        *self.path.lock().unwrap_or_else(|e| e.into_inner()) = path;
    }

    /// Forgets the failed installs, so the next [`Self::ensure`] runs the installer again (Check again).
    pub fn forget_failures(&self) {
        self.failed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    /// Each tool's tuq release, in [`Tool::ALL`]'s order: `None` for a tool without tuq (or with one from before the
    /// stamp). For Settings → About and the connected tools. Blocking file reads.
    pub fn versions(&self, homes: &Homes) -> Vec<(Tool, Option<String>)> {
        Tool::ALL
            .iter()
            .map(|&tool| (tool, installed(homes, tool).and_then(|i| i.version)))
            .collect()
    }

    /// `tool`'s tuq release now, for the session record. Blocking file reads.
    pub fn version(&self, homes: &Homes, tool: Tool) -> Option<String> {
        installed(homes, tool).and_then(|i| i.version)
    }

    /// Makes sure `tool`'s tuq has `skill` (`tuq:product-spec`), as tools launched with `homes` find it: it is asked,
    /// and when it doesn't have the skill, tuq's installer runs for the tool (with `env` over the service's
    /// environment, the project's variables) and it is asked again. Without a home folder nothing can be read, and
    /// the skill counts as there.
    pub async fn ensure(
        &self,
        tool: Tool,
        skill: &str,
        homes: &Homes,
        env: &[(String, String)],
    ) -> Tuq {
        let Some(name) = skill_of(skill).map(str::to_owned) else {
            return Tuq::Missing {
                reason: format!("{skill} isn't one of tuq's skills"),
            };
        };
        let Some(home) = homes.home.clone() else {
            return Tuq::Ready {
                version: None,
                updated: None,
            };
        };
        let _one = self.one.lock().await;
        let before = look(homes, tool).await;
        if let Some(install) = before.as_ref().filter(|i| i.has(&name)) {
            return Tuq::Ready {
                version: install.version.clone(),
                updated: None,
            };
        }
        if let Some(reason) = self.recent_failure(tool) {
            return Tuq::Missing { reason };
        }
        let installer = self
            .installer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        let Some(installer) = installer else {
            return Tuq::Missing {
                reason: format!("{}'s tuq doesn't have {name}", tool.display_name()),
            };
        };
        let path = self.path.lock().unwrap_or_else(|e| e.into_inner()).clone();
        tracing::info!(%tool, skill, had = before.is_some(), "running tuq's installer");
        if let Err(reason) = run(&installer, tool, &home, &path, env).await {
            tracing::warn!(%tool, skill, reason, "tuq's installer failed");
            self.failed(tool, &reason);
            return Tuq::Missing { reason };
        }
        let after = look(homes, tool).await;
        match after {
            Some(install) if install.has(&name) => {
                let updated = Updated {
                    tool,
                    from: before.as_ref().and_then(|b| b.version.clone()),
                    to: install.version.clone(),
                    had: before.is_some(),
                };
                tracing::info!(%tool, skill, update = updated.text(), "tuq installed");
                Tuq::Ready {
                    version: install.version,
                    updated: Some(updated),
                }
            }
            after => {
                let reason = match after.and_then(|i| i.version) {
                    Some(version) => format!("tuq {version} has no {name}"),
                    None => format!(
                        "tuq's installer ran, and {}'s tuq still has no {name}",
                        tool.display_name()
                    ),
                };
                tracing::warn!(%tool, skill, reason, "tuq still doesn't have the skill");
                self.failed(tool, &reason);
                Tuq::Missing { reason }
            }
        }
    }

    fn recent_failure(&self, tool: Tool) -> Option<String> {
        let failed = self.failed.lock().unwrap_or_else(|e| e.into_inner());
        failed
            .get(&tool)
            .filter(|(at, _)| at.elapsed() < RETRY_AFTER)
            .map(|(_, reason)| reason.clone())
    }

    fn failed(&self, tool: Tool, reason: &str) {
        self.failed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(tool, (Instant::now(), reason.to_owned()));
    }
}

/// `tool`'s tuq under `homes`, read off the async threads.
async fn look(homes: &Homes, tool: Tool) -> Option<TuqInstall> {
    let homes = homes.clone();
    tokio::task::spawn_blocking(move || installed(&homes, tool))
        .await
        .ok()
        .flatten()
}

/// Runs `installer` for `tool`, with `home` as the home folder, `path` (else the service's) as PATH and `env` over
/// the service's environment; why it failed.
async fn run(
    installer: &Installer,
    tool: Tool,
    home: &Path,
    path: &OsStr,
    env: &[(String, String)],
) -> Result<(), String> {
    let (program, args) = installer.command(tool);
    let mut command = tokio::process::Command::new(&program);
    command
        .args(&args)
        .env("HOME", home)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    if !path.is_empty() {
        command.env("PATH", path);
    }
    for (key, value) in env {
        command.env(key, value);
    }
    #[cfg(windows)]
    command.creation_flags(crate::process::CREATE_NO_WINDOW);
    let output = match tokio::time::timeout(INSTALL_TIMEOUT, command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(e)) => {
            return Err(format!(
                "tuq's installer couldn't start ({}): {e}",
                Path::new(&program).display()
            ));
        }
        Err(_) => return Err("tuq's installer took too long".to_owned()),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    tracing::debug!(%tool, output = %stdout, "tuq's installer");
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let why = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map_or_else(|| format!("it ended with {}", output.status), str::to_owned);
    Err(format!("tuq's installer failed: {why}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The marketplace tuq came from: any name, tuq is known by its plugin's name.
    const MARKETPLACE: &str = "team";

    #[test]
    fn tuq_names() {
        assert_eq!(skill_of("tuq:product-spec"), Some("product-spec"));
        assert_eq!(skill_of(" tuq:step-back "), Some("step-back"));
        assert_eq!(skill_of("tuq:"), None);
        assert_eq!(skill_of(crate::own_skill!("validate")), None);
        assert_eq!(skill_of("product-spec"), None);
        assert_eq!(skill_of("tuq:a b"), None);
        assert_eq!(installed_name(Tool::Claude, "product-spec"), "product-spec");
        assert_eq!(installed_name(Tool::Codex, "product-spec"), "product-spec");
        assert_eq!(
            installed_name(Tool::Copilot, "product-spec"),
            "tuq-product-spec"
        );
        assert_eq!(
            installed_name(Tool::OpenCode, "product-spec"),
            "tuq-product-spec"
        );
    }

    #[test]
    fn the_history_line() {
        let updated = |had, from: Option<&str>, to: Option<&str>| Updated {
            tool: Tool::Claude,
            from: from.map(str::to_owned),
            to: to.map(str::to_owned),
            had,
        };
        assert_eq!(
            updated(true, Some("2026.11.10"), Some("2026.11.14")).text(),
            "tuq updated from 2026.11.10 to 2026.11.14"
        );
        assert_eq!(
            updated(false, None, Some("2026.11.14")).text(),
            "tuq 2026.11.14 installed"
        );
        assert_eq!(
            updated(true, None, Some("2026.11.14")).text(),
            "tuq updated to 2026.11.14"
        );
    }

    #[test]
    fn the_published_installer_is_tuqs() {
        let (program, args) = Installer::Published.command(Tool::Codex);
        let all: Vec<String> = std::iter::once(program)
            .chain(args)
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        if cfg!(windows) {
            assert_eq!(all[0], "powershell.exe");
            assert!(
                all.last()
                    .unwrap()
                    .contains(&format!("{INSTALLER_URL}.ps1"))
            );
            assert!(all.last().unwrap().contains("--tool codex"));
        } else {
            assert_eq!(all[0], "sh");
            assert_eq!(
                &all[all.len() - 2..],
                [format!("{INSTALLER_URL}.sh"), "codex".to_owned()]
            );
        }
    }

    /// A `SKILL.md` named `name`, stamped with `release` when there is one.
    fn skill(folder: &Path, name: &str, release: Option<&str>) {
        let dir = folder.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        let stamp =
            release.map_or_else(String::new, |r| format!("metadata:\n  tuq-release: {r}\n"));
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name}'s.\n{stamp}---\n"),
        )
        .unwrap();
    }

    /// Each tool's tuq is found its own way (decision 100): Claude Code's and Codex's plugin named tuq, Copilot's
    /// prefixed plugin, OpenCode's stamped `tuq-<name>` folders. Loose copies of tuq's skills (`~/.claude/skills`,
    /// an unstamped `tuq-…` folder in OpenCode's) are the user's own, not their tuq.
    #[test]
    fn each_tools_tuq_is_found_by_its_install() {
        let dir = tempfile::TempDir::new().unwrap();
        let h = dir.path();
        // Claude Code: the plugin, and a loose copy that doesn't count.
        let claude = h.join(format!(".claude/plugins/cache/{MARKETPLACE}/tuq/abc"));
        skill(&claude.join("skills"), "product-spec", Some("2026.11.14"));
        skill(&claude.join("skills"), "tech-spec", Some("2026.11.14"));
        std::fs::write(
            h.join(".claude/plugins/installed_plugins.json"),
            serde_json::json!({"version": 2, "plugins": {"tuq@team": [
                {"scope": "user", "installPath": claude.display().to_string()}
            ]}})
            .to_string(),
        )
        .unwrap();
        skill(
            &h.join(".claude/skills"),
            "milestone-plan",
            Some("2026.11.14"),
        );
        // Codex: an older plugin from before the stamp.
        let codex = h.join(format!(".codex/plugins/cache/{MARKETPLACE}/tuq/1"));
        skill(&codex.join("skills"), "product-spec", None);
        std::fs::write(
            h.join(".codex/config.toml"),
            format!("[plugins.\"tuq@{MARKETPLACE}\"]\nenabled = true\n"),
        )
        .unwrap();
        // Copilot: the prefixed plugin, and an older plain name that doesn't count as the skill.
        let copilot = h.join(format!(".copilot/installed-plugins/{MARKETPLACE}/tuq"));
        std::fs::create_dir_all(copilot.join(".claude-plugin")).unwrap();
        std::fs::write(
            copilot.join(".claude-plugin/plugin.json"),
            r#"{"name": "tuq"}"#,
        )
        .unwrap();
        skill(&copilot.join("skills"), "tuq-step-back", Some("2026.11.12"));
        skill(
            &copilot.join("skills"),
            "milestone-plan",
            Some("2026.11.12"),
        );
        // OpenCode: stamped tuq- folders; an unstamped one is the user's.
        let opencode = h.join(".config/opencode/skills");
        skill(&opencode, "tuq-product-spec", Some("2026.11.13"));
        skill(&opencode, "tuq-mine", None);

        let homes = Homes::in_home(h);
        let names = |tool| {
            installed(&homes, tool)
                .unwrap()
                .skills
                .into_iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
        };
        assert_eq!(names(Tool::Claude), ["product-spec", "tech-spec"]);
        assert_eq!(names(Tool::Codex), ["product-spec"]);
        assert_eq!(names(Tool::Copilot), ["step-back"]);
        assert_eq!(names(Tool::OpenCode), ["product-spec"]);
        assert_eq!(
            TuqInstalls::default().versions(&homes),
            [
                (Tool::Claude, Some("2026.11.14".to_owned())),
                (Tool::Codex, None),
                (Tool::Copilot, Some("2026.11.12".to_owned())),
                (Tool::OpenCode, Some("2026.11.13".to_owned())),
            ]
        );

        // No tuq anywhere: none.
        let empty = tempfile::TempDir::new().unwrap();
        let homes = Homes::in_home(empty.path());
        assert!(installs(&homes).is_empty());
    }

    /// A stand-in installer: counts its runs in `$HOME/runs`, takes a moment, then copies a stamped `tuq-<name>`
    /// skill into OpenCode's folder (or fails with `fail`).
    #[cfg(unix)]
    fn stand_in(dir: &Path, fail: bool) -> Installer {
        use std::os::unix::fs::PermissionsExt;
        let program = dir.join(if fail { "failing" } else { "installer" });
        let body = if fail {
            "echo 'tuq install: offline' >&2; exit 1".to_owned()
        } else {
            "d=\"$HOME/.config/opencode/skills/tuq-product-spec\"; mkdir -p \"$d\"; \
             printf -- '---\\nname: tuq-product-spec\\nmetadata:\\n  tuq-release: 2026.11.14\\n---\\n' > \"$d/SKILL.md\""
                .to_owned()
        };
        std::fs::write(
            &program,
            format!("#!/bin/sh\necho \"$*\" >> \"$HOME/runs\"\nsleep 0.3\n{body}\n"),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        Installer::Program {
            program,
            args: Vec::new(),
        }
    }

    fn runs(home: &Path) -> usize {
        std::fs::read_to_string(home.join("runs"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    /// Two stages asking at once get one install; the second finds the first one's tuq. A tuq that has the skill
    /// needs none.
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn two_stages_asking_at_once_get_one_install() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let tuq = std::sync::Arc::new(TuqInstalls::new(Some(stand_in(dir.path(), false))));
        let homes = Homes::in_home(&home);
        let (a, b) = tokio::join!(
            tuq.ensure(Tool::OpenCode, "tuq:product-spec", &homes, &[]),
            tuq.ensure(Tool::OpenCode, "tuq:product-spec", &homes, &[]),
        );
        assert_eq!(runs(&home), 1);
        let updated = |answer: &Tuq| match answer {
            Tuq::Ready { updated, version } => {
                assert_eq!(version.as_deref(), Some("2026.11.14"));
                updated.is_some()
            }
            Tuq::Missing { reason } => panic!("{reason}"),
        };
        assert_ne!(updated(&a), updated(&b), "one of them installed");
        assert_eq!(
            std::fs::read_to_string(home.join("runs")).unwrap().trim(),
            "--tool opencode"
        );
        // Present now: nothing to install.
        assert!(matches!(
            tuq.ensure(Tool::OpenCode, "tuq:product-spec", &homes, &[])
                .await,
            Tuq::Ready { updated: None, .. }
        ));
        assert_eq!(runs(&home), 1);
    }

    /// Missing after the install, or a failing installer: missing, and not tried again for a while unless the failures
    /// are forgotten (Check again).
    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_install_is_not_retried_until_forgotten() {
        let dir = tempfile::TempDir::new().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let homes = Homes::in_home(&home);
        let tuq = TuqInstalls::new(Some(stand_in(dir.path(), true)));
        let missing = tuq
            .ensure(Tool::OpenCode, "tuq:product-spec", &homes, &[])
            .await;
        assert!(
            matches!(&missing, Tuq::Missing { reason } if reason.contains("tuq install: offline")),
            "{missing:?}"
        );
        assert!(matches!(
            tuq.ensure(Tool::OpenCode, "tuq:product-spec", &homes, &[])
                .await,
            Tuq::Missing { .. }
        ));
        assert_eq!(runs(&home), 1, "not run again on its own");
        tuq.forget_failures();
        tuq.ensure(Tool::OpenCode, "tuq:product-spec", &homes, &[])
            .await;
        assert_eq!(runs(&home), 2);

        // Installed, but without the skill: still missing.
        tuq.forget_failures();
        tuq.set_installer(Some(stand_in(dir.path(), false)));
        assert!(matches!(
            tuq.ensure(Tool::OpenCode, "tuq:update-docs", &homes, &[]).await,
            Tuq::Missing { reason } if reason == "tuq 2026.11.14 has no update-docs"
        ));
        // Without an installer nothing runs.
        let none = TuqInstalls::default();
        assert!(matches!(
            none.ensure(Tool::Claude, "tuq:product-spec", &homes, &[])
                .await,
            Tuq::Missing { .. }
        ));
        // Without a home nothing can be read: the skill counts as there.
        assert!(matches!(
            none.ensure(Tool::Claude, "tuq:product-spec", &Homes::default(), &[])
                .await,
            Tuq::Ready { .. }
        ));
    }

    #[test]
    fn a_session_finds_tuqs_commands_after_otaqs() {
        let path = launch_path(Path::new("/opt/app/bin"), OsStr::new("/usr/bin"));
        let parts: Vec<PathBuf> = std::env::split_paths(&path).collect();
        assert_eq!(parts.first().unwrap(), Path::new("/opt/app/bin"));
        assert_eq!(parts.last().unwrap(), Path::new("/usr/bin"));
        if let Some(bin) = bin_dir() {
            assert_eq!(parts[1], bin);
            assert!(bin.ends_with(Path::new("bin")));
        }
    }
}
