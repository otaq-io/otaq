//! Connected tools (ADP-10, d0086): each agent tool's state on Settings → Agent tools, kept in memory by the
//! service, the tools that count, and what connecting, hiding, signing in and disconnecting a tool does. Only a
//! **Connected** tool counts: for the default model, for "one tool connected", for avatars and for launching.
//!
//! The states come from each adapter's detect ([`AgentToolState::from_detection`]), run by
//! [`ConnectedTools::recheck`] at service start, on `agent_tools.recheck` (the page opening, Re-check, the
//! onboarding Tools step), after a Sign in… terminal ends and when a launch fails because of its tool; no timer
//! beyond those (d0086). A found tool the user set **Hidden** shows as Hidden whatever detect found; the hidden
//! tools are kept in `settings` ([`AGENT_TOOLS_HIDDEN`]). Every change of what the page would show is sent on a
//! watch channel ([`ConnectedTools::subscribe`]), once per change.
//!
//! **A tool becomes Connected** (from any state, un-hiding included) when it counts now and didn't at the last
//! check; the tools that counted are kept in `settings` ([`AGENT_TOOLS_CONNECTED`]), so a restart connects nothing
//! again. Then, once: the adapter's one-time setup ([`super::AgentTool::set_up_tool`], its report kept for the
//! page) and its folder trust for every existing project with the one-time note naming it
//! ([`Trust::record_existing`], d0085). While no default model for new agents is set, the first connected tool's
//! default model becomes it ([`crate::agents::shipped::ensure_default_model`], AGM-02).

use std::collections::{BTreeSet, HashMap};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use super::setup::Trust;
use super::{Adapters, Detection, Os, SetupReport, Tool, WindowsSource};
use crate::board::{BoardError, Refusal};
use crate::data_dir::DataDir;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::{SessionKind, Sessions, SpawnSpec, Start, TerminalSize};
use crate::store::settings::{AGENT_TOOLS_CONNECTED, AGENT_TOOLS_HIDDEN};
use crate::store::{Store, StoreError, now_ms};
use crate::terminals::TerminalEnv;
use crate::terminals::shell::ProjectTerminal;

/// A tool's state (d0086, Q-G).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AgentToolState {
    /// Installed, runs, and its own sign-in answer says signed in. Counts.
    Connected {
        version: Option<String>,
        /// How it is signed in, where the tool says ("signed in with ChatGPT").
        account: Option<String>,
    },
    /// Found, but signed out: **Sign in…**.
    FoundNotSignedIn { version: Option<String> },
    /// Found, but its sign-in answer couldn't be read (the status command failed, took too long or printed
    /// something else): **Re-check**. Doesn't count, as a tool that may be signed out must not start agents.
    CouldNotCheck { version: Option<String> },
    /// Found, but older than Otaq can run (Copilot before 1.0.98): never launched.
    UpdateNeeded { version: Option<String> },
    /// Not on PATH: **How to install**.
    NotInstalled,
    /// Found, and set by the user not to count.
    Hidden { version: Option<String> },
}

impl AgentToolState {
    /// The state detect's answer gives, before Hidden: not found → Not installed; update needed; signed in
    /// `Some(false)` → Found · not signed in; no answer (`None`) → Couldn't check; signed in → Connected.
    pub fn from_detection(detection: &Detection) -> Self {
        let version = detection.version.clone();
        if !detection.installed {
            Self::NotInstalled
        } else if detection.update_needed {
            Self::UpdateNeeded { version }
        } else {
            match detection.signed_in {
                Some(false) => Self::FoundNotSignedIn { version },
                None => Self::CouldNotCheck { version },
                Some(true) => Self::Connected {
                    version,
                    account: detection.account.clone(),
                },
            }
        }
    }

    /// The version found, if any.
    pub fn version(&self) -> Option<&str> {
        match self {
            Self::Connected { version, .. }
            | Self::FoundNotSignedIn { version }
            | Self::CouldNotCheck { version }
            | Self::UpdateNeeded { version }
            | Self::Hidden { version } => version.as_deref(),
            Self::NotInstalled => None,
        }
    }

    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected { .. })
    }

    /// Why a tool in this state can't start an agent ("Codex isn't available · logged out"); `None` when
    /// Connected.
    pub fn unavailable_reason(&self) -> Option<&'static str> {
        match self {
            Self::Connected { .. } => None,
            Self::FoundNotSignedIn { .. } => Some("signed out"),
            Self::CouldNotCheck { .. } => Some("couldn\u{2019}t check its sign-in"),
            Self::UpdateNeeded { .. } => Some("update needed"),
            Self::NotInstalled => Some("not installed"),
            Self::Hidden { .. } => Some("hidden in Settings"),
        }
    }
}

/// One thing Otaq's setup writes into the tool's own config, for "Otaq added…" ([`super::SetupEntry`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupLine {
    /// A plain phrase: "trust for your projects".
    pub text: String,
    /// Removed by Disconnect (Otaq-only entries); folder trust stays (d0086).
    pub removed_on_disconnect: bool,
}

/// What the tool's last one-time setup or removal did: the entries done and the ones that failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupRun {
    /// `set_up` when the tool became Connected, `removed` by Disconnect.
    pub action: SetupAction,
    /// Unix milliseconds.
    pub at: i64,
    pub done: Vec<SetupLine>,
    pub failed: Vec<SetupFailure>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupAction {
    SetUp,
    Removed,
}

/// An entry the setup or removal couldn't write, with its short reason shown as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetupFailure {
    pub text: String,
    pub reason: String,
}

impl SetupRun {
    fn new(action: SetupAction, report: SetupReport) -> Self {
        Self {
            action,
            at: now_ms(),
            done: report.done.into_iter().map(SetupLine::from).collect(),
            failed: report
                .failed
                .into_iter()
                .map(|(entry, reason)| SetupFailure {
                    text: entry.text,
                    reason,
                })
                .collect(),
        }
    }
}

impl From<super::SetupEntry> for SetupLine {
    fn from(entry: super::SetupEntry) -> Self {
        Self {
            text: entry.text,
            removed_on_disconnect: entry.removed_on_disconnect,
        }
    }
}

/// One tool's row: its state, when it was last checked, the notes its adapter's readers set ("limits not
/// reported", T-0350), and what the Agent tools page shows with it (T-0348; empty before the service wires the
/// adapters in).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolStatus {
    pub tool: Tool,
    #[serde(flatten)]
    pub state: AgentToolState,
    /// Unix milliseconds; `None` before the first check.
    pub checked_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
    /// **How to install** on this OS, for copying, and what **Install** runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_command: Option<String>,
    /// **Install** applies: not installed, and the install command's package manager (`npm`) is on PATH.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub can_install: bool,
    /// The package manager the install command needs and PATH lacks (`npm`): no Install, a note instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_needs: Option<String>,
    /// What Otaq's setup writes into the tool's config ("Otaq added…"), with which stay on Disconnect.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub setup_entries: Vec<SetupLine>,
    /// The last setup or removal, with its failures; `None` while neither ran since the service started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<SetupRun>,
    /// The permission warning's title for a found tool whose own settings stop new sessions on permission
    /// prompts (SET-02, d0011).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_warning: Option<String>,
    /// **Sign in…** applies: found, not signed in, and the tool has a sign-in command.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub can_sign_in: bool,
    /// **Disconnect…** applies: the tool is Connected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub can_disconnect: bool,
    /// Where the tool's usage windows come from, for "Otaq can see · limits" (`not_reported`: "limits not
    /// reported"); `None` before the service wires the adapters in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<WindowsSource>,
}

/// Why a tool can't start an agent now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotAvailable {
    /// "logged out", "not installed", "update needed", "hidden in Settings".
    pub reason: String,
}

impl NotAvailable {
    /// "Codex isn't available · logged out".
    pub fn message(&self, tool: Tool) -> String {
        format!(
            "{} isn\u{2019}t available \u{b7} {}",
            tool.display_name(),
            self.reason
        )
    }
}

/// The answer to `agent_tools.list` and `agent_tools.set_hidden`: every tool, in [`Tool::ALL`] order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentToolsList {
    pub tools: Vec<ToolStatus>,
}

#[derive(Debug, Default)]
struct Found {
    /// What the last detect gave (never `Hidden`), and when.
    states: HashMap<Tool, (AgentToolState, i64)>,
    hidden: BTreeSet<Tool>,
    notes: HashMap<Tool, Vec<String>>,
    setups: HashMap<Tool, SetupRun>,
    /// Connected tools whose detect still offers Sign in… ([`Detection::sign_in_offered`]).
    sign_in_offered: BTreeSet<Tool>,
}

impl Found {
    fn status(&self, tool: Tool) -> ToolStatus {
        let (found, checked_at) = match self.states.get(&tool) {
            Some((state, at)) => (state.clone(), Some(*at)),
            None => (AgentToolState::NotInstalled, None),
        };
        let state = match found {
            AgentToolState::NotInstalled => AgentToolState::NotInstalled,
            found if self.hidden.contains(&tool) => AgentToolState::Hidden {
                version: found.version().map(str::to_owned),
            },
            found => found,
        };
        ToolStatus {
            tool,
            state,
            checked_at,
            notes: self.notes.get(&tool).cloned().unwrap_or_default(),
            install_command: None,
            can_install: false,
            install_needs: None,
            setup_entries: Vec::new(),
            setup: self.setups.get(&tool).cloned(),
            permission_warning: None,
            can_sign_in: false,
            can_disconnect: false,
            limits: None,
        }
    }

    fn statuses(&self) -> Vec<ToolStatus> {
        Tool::ALL.iter().map(|&tool| self.status(tool)).collect()
    }
}

/// What connecting, signing in and disconnecting need from the service, handed over once at service start
/// ([`ConnectedTools::wire`]).
pub struct Wiring {
    pub adapters: Adapters,
    pub store: Store,
    /// Folder trust for the existing projects when a tool connects (d0085).
    pub trust: Trust,
    /// Runs the Sign in… terminal.
    pub sessions: Sessions,
    pub events: EventBus,
    /// The Sign in… terminal's environment (the `otaq` folder on PATH), as a Project terminal's.
    pub terminals: TerminalEnv,
    pub data: DataDir,
    /// The PATH detect searches; the sign-in program is found on it.
    pub path: OsString,
    /// The user's home folder, where the Sign in… terminal starts; `None` (tests) starts it in the data folder.
    pub home: Option<PathBuf>,
}

struct Wired {
    wiring: Wiring,
    /// One recheck (and what connecting does) at a time, so a tool is set up once.
    rechecking: tokio::sync::Mutex<()>,
}

/// The tools' states in memory, shared by the service. Cheap to clone.
#[derive(Clone)]
pub struct ConnectedTools {
    found: Arc<Mutex<Found>>,
    changes: Arc<watch::Sender<Vec<ToolStatus>>>,
    wired: Arc<OnceLock<Wired>>,
}

impl ConnectedTools {
    /// Nothing checked yet (every tool Not installed), with `hidden` set Hidden.
    pub fn new(hidden: impl IntoIterator<Item = Tool>) -> Self {
        let found = Found {
            hidden: hidden.into_iter().collect(),
            ..Found::default()
        };
        let (changes, _) = watch::channel(found.statuses());
        Self {
            found: Arc::new(Mutex::new(found)),
            changes: Arc::new(changes),
            wired: Arc::default(),
        }
    }

    /// Nothing checked yet, with the hidden tools stored in `settings`.
    pub async fn load(store: &Store) -> Result<Self, StoreError> {
        Ok(Self::new(hidden_tools(store).await?))
    }

    /// Hands over what rechecks, Sign in… and Disconnect need. Set once, at service start; a second call changes
    /// nothing. Before it, a recheck checks nothing and the rows carry only their states.
    pub fn wire(&self, wiring: Wiring) {
        let _ = self.wired.set(Wired {
            wiring,
            rechecking: tokio::sync::Mutex::new(()),
        });
        self.change(|_| {});
    }

    /// Every tool's row, in [`Tool::ALL`] order.
    pub fn list(&self) -> AgentToolsList {
        let statuses = self.lock().statuses();
        AgentToolsList {
            tools: self.with_page(statuses),
        }
    }

    pub fn status(&self, tool: Tool) -> ToolStatus {
        self.lock().status(tool)
    }

    /// The tools that count, in [`Tool::ALL`] order.
    pub fn connected(&self) -> Vec<Tool> {
        let found = self.lock();
        Tool::ALL
            .iter()
            .copied()
            .filter(|&tool| found.status(tool).state.is_connected())
            .collect()
    }

    /// Whether `tool` can start an agent now, or why not.
    pub fn is_available(&self, tool: Tool) -> Result<(), NotAvailable> {
        match self.status(tool).state.unavailable_reason() {
            None => Ok(()),
            Some(reason) => Err(NotAvailable {
                reason: reason.to_owned(),
            }),
        }
    }

    /// Records what detect found for `tool` at `at` (Unix milliseconds). A `Hidden` state is taken as the found
    /// tool it hides; hiding is [`Self::set_hidden`]'s.
    pub fn set_detected(&self, tool: Tool, state: AgentToolState, at: i64) {
        let state = match state {
            AgentToolState::Hidden { version } => AgentToolState::Connected {
                version,
                account: None,
            },
            state => state,
        };
        self.change(|found| {
            found.states.insert(tool, (state, at));
        });
    }

    /// Records what detect answered for `tool` at `at`: its state and whether Sign in… is still offered.
    pub fn set_detection(&self, tool: Tool, detection: &Detection, at: i64) {
        self.set_detected(tool, AgentToolState::from_detection(detection), at);
        self.change(|found| {
            if detection.sign_in_offered {
                found.sign_in_offered.insert(tool);
            } else {
                found.sign_in_offered.remove(&tool);
            }
        });
    }

    /// An agent of `tool` stopped because the tool's sign-in ran out or is missing ("Login expired"): a Connected
    /// tool reads Found · not signed in at once, so Sign in… shows wherever the tool's problem does (the stopped
    /// ticket, the agent panel, the strip) without waiting for a check of the tools. Its last check's time stays; the
    /// next check takes the tool's own answer. Any other state is left as it is.
    pub fn signed_out(&self, tool: Tool) {
        self.change(|found| {
            if let Some((state, _)) = found.states.get_mut(&tool)
                && let AgentToolState::Connected { version, .. } = state
            {
                *state = AgentToolState::FoundNotSignedIn {
                    version: version.take(),
                };
                found.sign_in_offered.remove(&tool);
            }
        });
    }

    /// Sets `tool` Hidden or not, in memory only.
    pub fn hide(&self, tool: Tool, hidden: bool) {
        self.change(|found| {
            if hidden {
                found.hidden.insert(tool);
            } else {
                found.hidden.remove(&tool);
            }
        });
    }

    /// Sets `tool` Hidden or not and stores it in `settings` (`agent_tools.set_hidden`). Writes and removes
    /// nothing in the tool's own config (d0086). Shown again and signed in, the tool becomes Connected (its setup,
    /// its entry, its folder trust).
    pub async fn set_hidden(
        &self,
        store: &Store,
        tool: Tool,
        hidden: bool,
    ) -> Result<(), StoreError> {
        let mut tools: BTreeSet<Tool> = self.lock().hidden.clone();
        if hidden {
            tools.insert(tool);
        } else {
            tools.remove(&tool);
        }
        let names: Vec<&str> = tools.iter().map(|t| t.as_str()).collect();
        let json = serde_json::to_string(&names).expect("names serialise");
        store.set_setting(AGENT_TOOLS_HIDDEN, &json).await?;
        self.hide(tool, hidden);
        if let Some(wired) = self.wired.get() {
            let _one = wired.rechecking.lock().await;
            self.settle(&wired.wiring).await;
        }
        Ok(())
    }

    /// Adds (`on`) or removes a note on `tool`'s row ("limits not reported").
    pub fn set_note(&self, tool: Tool, note: &str, on: bool) {
        self.change(|found| {
            let notes = found.notes.entry(tool).or_default();
            let has = notes.iter().any(|n| n == note);
            if on && !has {
                notes.push(note.to_owned());
            } else if !on && has {
                notes.retain(|n| n != note);
            }
        });
    }

    /// Detects `tool` (or every tool, in parallel) again and records the states; a tool that becomes Connected
    /// is set up, gets its entry and its folder trust ([`Self::settle`]). Called at service start, on
    /// `agent_tools.recheck`, after a Sign in… terminal ends, and with the tool when a launch fails because of it
    /// (T-0357). Does nothing before the service wires it.
    pub async fn recheck(&self, tool: Option<Tool>) -> Result<(), BoardError> {
        let Some(wired) = self.wired.get() else {
            return Ok(());
        };
        let _one = wired.rechecking.lock().await;
        let adapters = &wired.wiring.adapters;
        let tools = tool.map_or_else(|| Tool::ALL.to_vec(), |tool| vec![tool]);
        let mut detecting = tokio::task::JoinSet::new();
        for tool in tools {
            let adapters = adapters.clone();
            detecting.spawn(async move { (tool, adapters.get(tool).detect().await) });
        }
        while let Some(done) = detecting.join_next().await {
            let Ok((tool, detection)) = done.inspect_err(|e| {
                tracing::warn!(error = %e, "detecting an agent tool stopped");
            }) else {
                continue;
            };
            let state = AgentToolState::from_detection(&detection);
            tracing::info!(tool = %tool, state = ?state, "agent tool checked");
            self.set_detection(tool, &detection, now_ms());
        }
        self.settle(&wired.wiring).await;
        Ok(())
    }

    /// `agent_tools.sign_in`: a plain terminal in the home folder, titled "Codex sign in", running the tool's own
    /// login (`codex login`); when it ends, the tool is checked again. Otaq never reads what is typed (d0086).
    pub async fn sign_in(&self, tool: Tool) -> Result<ProjectTerminal, BoardError> {
        let Some(wired) = self.wired.get() else {
            return Err(not_built("agent_tools.sign_in"));
        };
        let w = &wired.wiring;
        let adapter = w.adapters.get(tool);
        let name = adapter.display_name();
        let Some((program, args)) = adapter
            .login_command()
            .and_then(|command| command.split_first().map(|(p, a)| (p.clone(), a.to_vec())))
        else {
            return Err(BoardError::refused(
                Refusal::NotAvailableYet,
                format!("{name} has no sign-in command of its own"),
            ));
        };
        let Some(program) = crate::tools::find_on_path(&program, &w.path) else {
            return Err(BoardError::refused(
                Refusal::NotAvailableYet,
                format!("{name} isn\u{2019}t installed"),
            ));
        };
        let session = self
            .open_terminal(w, tool, program.display().to_string(), args)
            .await?;
        tracing::info!(tool = %tool, session = %session.id, "sign-in terminal opened");
        Ok(ProjectTerminal {
            number: 0,
            name: sign_in_title(tool),
            session,
        })
    }

    /// `agent_tools.install`: a plain terminal in the home folder, titled "Installing Codex", running the row's
    /// **How to install** command (`npm install -g @openai/codex`) after a `$ …` line, on the user's click only;
    /// when it ends, the tool is checked again. Refused unless the tool is not installed and the command's
    /// package manager is on PATH.
    pub async fn install(&self, tool: Tool) -> Result<ProjectTerminal, BoardError> {
        let Some(wired) = self.wired.get() else {
            return Err(not_built("agent_tools.install"));
        };
        let w = &wired.wiring;
        // Its own statement: a guard taken inside `with_page(…)`'s argument would live until the end of the
        // statement, and `with_page` takes the same lock (a deadlock that hung every check run).
        let found = self.lock().status(tool);
        let status = self.with_page(vec![found]).pop().expect("one status");
        let name = tool.display_name();
        if status.state != AgentToolState::NotInstalled {
            return Err(BoardError::refused(
                Refusal::NotAvailableYet,
                format!("{name} is installed already"),
            ));
        }
        if let Some(manager) = &status.install_needs {
            return Err(BoardError::refused(
                Refusal::NotAvailableYet,
                format!("{name}\u{2019}s install command needs {manager}"),
            ));
        }
        let command = w.adapters.get(tool).install_command(Os::current());
        let run = crate::terminals::tool::command_line(command, &w.terminals);
        let session = self
            .open_terminal_with(w, tool, run.program, run.args, run.env)
            .await?;
        tracing::info!(tool = %tool, session = %session.id, "install terminal opened");
        Ok(ProjectTerminal {
            number: 0,
            name: install_title(tool),
            session,
        })
    }

    /// A terminal of no project in the home folder, running `program`; the tool is checked again when it ends.
    async fn open_terminal(
        &self,
        w: &Wiring,
        tool: Tool,
        program: String,
        args: Vec<String>,
    ) -> Result<crate::session::SessionInfo, BoardError> {
        self.open_terminal_with(w, tool, program, args, Vec::new())
            .await
    }

    async fn open_terminal_with(
        &self,
        w: &Wiring,
        tool: Tool,
        program: String,
        args: Vec<String>,
        extra_env: Vec<(String, String)>,
    ) -> Result<crate::session::SessionInfo, BoardError> {
        let mut env = w.terminals.env(&w.data, None, None);
        env.extend(extra_env);
        let session = w
            .sessions
            .spawn(SpawnSpec {
                kind: SessionKind::Project,
                start: Start::User,
                // No project: the terminal belongs to the tool, and isn't stored.
                project_id: String::new(),
                project_key: String::new(),
                ticket: None,
                program,
                args,
                cwd: w
                    .home
                    .clone()
                    .unwrap_or_else(|| w.data.root().to_path_buf()),
                env,
                size: SIGN_IN_SIZE,
                tool: None,
                tool_session_id: None,
                model: None,
                agent: None,
                effort: None,
                theme: None,
            })
            .await
            .map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        let (this, sessions, id) = (self.clone(), w.sessions.clone(), session.id.clone());
        tokio::spawn(async move {
            sessions.wait_ended(&id).await;
            if let Err(e) = this.recheck(Some(tool)).await {
                tracing::warn!(tool = %tool, error = %e, "can't check the tool after its terminal ended");
            }
        });
        Ok(session)
    }

    /// `agent_tools.disconnect`: the adapter removes Otaq's own entries (folder trust stays, d0086), then the tool
    /// is set Hidden so it doesn't count again at the next check (Q-F). Refused while an agent of the tool runs.
    /// Named agents on the tool stay.
    pub async fn disconnect(&self, tool: Tool) -> Result<(), BoardError> {
        let Some(wired) = self.wired.get() else {
            return Err(not_built("agent_tools.disconnect"));
        };
        let w = &wired.wiring;
        let running: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM sessions WHERE tool = ? AND ended_at IS NULL")
                .bind(tool.as_str())
                .fetch_one(w.store.pool())
                .await
                .map_err(|e| BoardError::Store(e.into()))?;
        if running > 0 {
            return Err(BoardError::refused(
                Refusal::AgentInUse,
                running_message(tool, running),
            ));
        }
        let adapters = w.adapters.clone();
        let report = tokio::task::spawn_blocking(move || adapters.get(tool).remove_setup())
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(tool = %tool, error = %e, "removing the tool's setup stopped");
                SetupReport::default()
            });
        tracing::info!(tool = %tool, removed = report.done.len(), failed = report.failed.len(), "agent tool disconnected");
        self.change(|found| {
            found
                .setups
                .insert(tool, SetupRun::new(SetupAction::Removed, report));
        });
        self.set_hidden(&w.store, tool, true).await?;
        Ok(())
    }

    /// Every row, sent again whenever one changes (once per change).
    pub fn subscribe(&self) -> watch::Receiver<Vec<ToolStatus>> {
        self.changes.subscribe()
    }

    /// What follows a check: a tool that counts now and didn't before becomes Connected, once; with no default
    /// model yet, the first connected tool's becomes it. The tools that count are stored for the next check, keeping
    /// the earlier answer for a tool not checked yet or that couldn't be checked.
    async fn settle(&self, w: &Wiring) {
        let known = match stored_tools(&w.store, AGENT_TOOLS_CONNECTED).await {
            Ok(known) => known,
            Err(e) => {
                tracing::warn!(error = %e, "can't read the connected agent tools");
                return;
            }
        };
        let connected = self.connected();
        // Not checked yet, or its sign-in answer couldn't be read: the earlier answer stands, so a tool that only
        // couldn't be checked this time isn't set up again once it can be.
        let unchecked: Vec<Tool> = {
            let found = self.lock();
            Tool::ALL
                .iter()
                .copied()
                .filter(|tool| {
                    !matches!(
                        found.states.get(tool),
                        Some((state, _)) if !matches!(state, AgentToolState::CouldNotCheck { .. })
                    )
                })
                .collect()
        };
        if let Err(e) = crate::agents::shipped::ensure_default_model(&w.store, &connected).await {
            tracing::warn!(error = %e, "can't set the default model for new agents");
        }
        for &tool in connected.iter().filter(|tool| !known.contains(tool)) {
            self.connect(w, tool).await;
        }
        // A tool that stayed Connected but whose setup is gone (or never got written): only the setup again.
        for &tool in connected.iter().filter(|tool| known.contains(tool)) {
            if w.adapters.get(tool).setup_needed() {
                self.set_up(w, tool).await;
            }
        }
        let now: Vec<Tool> = Tool::ALL
            .iter()
            .copied()
            .filter(|tool| {
                connected.contains(tool) || (unchecked.contains(tool) && known.contains(tool))
            })
            .collect();
        if now != known
            && let Err(e) = store_tools(&w.store, AGENT_TOOLS_CONNECTED, &now).await
        {
            tracing::warn!(error = %e, "can't store the connected agent tools");
        }
    }

    /// What connecting `tool` does (d0086, d0085): its one-time setup and its folder trust.
    async fn connect(&self, w: &Wiring, tool: Tool) {
        let adapter = w.adapters.get(tool);
        let name = adapter.display_name();
        tracing::info!(tool = %tool, "{name} connected: running its one-time setup");
        self.set_up(w, tool).await;
        if w.trust.record_existing(&w.store, tool).await {
            w.events.publish(Event::TrustChanged);
        }
    }

    /// The adapter's one-time setup, its report kept for the page.
    async fn set_up(&self, w: &Wiring, tool: Tool) {
        let adapter = w.adapters.get(tool);
        let name = adapter.display_name();
        let report = adapter.set_up_tool().await;
        for (entry, reason) in &report.failed {
            tracing::warn!(tool = %tool, entry = %entry.text, reason = %reason, "{name}'s setup couldn't write an entry");
        }
        self.change(|found| {
            found
                .setups
                .insert(tool, SetupRun::new(SetupAction::SetUp, report));
        });
    }

    /// The rows with what the page shows beside each state, from the adapters once wired.
    fn with_page(&self, mut statuses: Vec<ToolStatus>) -> Vec<ToolStatus> {
        let Some(wired) = self.wired.get() else {
            return statuses;
        };
        let offered = self.lock().sign_in_offered.clone();
        for status in &mut statuses {
            let adapter = wired.wiring.adapters.get(status.tool);
            let install = adapter.install_command(Os::current());
            status.install_command = Some(install.to_owned());
            if status.state == AgentToolState::NotInstalled {
                status.install_needs = crate::tools::package_manager_of(install)
                    .filter(|manager| {
                        crate::tools::find_on_path(manager, &wired.wiring.path).is_none()
                    })
                    .map(str::to_owned);
                status.can_install = status.install_needs.is_none();
            }
            status.setup_entries = adapter
                .setup_entries()
                .into_iter()
                .map(SetupLine::from)
                .collect();
            if status.state != AgentToolState::NotInstalled {
                status.permission_warning = adapter.permission_warning();
            }
            status.can_sign_in = sign_in_applies(&status.state, offered.contains(&status.tool))
                && adapter.login_command().is_some();
            status.can_disconnect = status.state.is_connected();
            status.limits = Some(adapter.windows_source());
        }
        statuses
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Found> {
        self.found.lock().expect("connected tools")
    }

    fn change(&self, edit: impl FnOnce(&mut Found)) {
        let statuses = {
            let mut found = self.lock();
            edit(&mut found);
            found.statuses()
        };
        let statuses = self.with_page(statuses);
        let changed = self.changes.send_if_modified(|sent| {
            let changed = *sent != statuses;
            if changed {
                *sent = statuses;
            }
            changed
        });
        if changed && let Some(wired) = self.wired.get() {
            wired.wiring.events.publish(Event::AgentToolsChanged);
        }
    }
}

/// Whether **Sign in…** applies to a tool in `state` (before asking whether it has a login command): found and
/// signed out, or Connected while its detect still offers it (OpenCode with free models only, Q-K).
fn sign_in_applies(state: &AgentToolState, offered: bool) -> bool {
    match state {
        AgentToolState::FoundNotSignedIn { .. } => true,
        AgentToolState::Connected { .. } => offered,
        _ => false,
    }
}

/// The Sign in… terminal's starting size; the app resizes it.
const SIGN_IN_SIZE: TerminalSize = TerminalSize {
    cols: 120,
    rows: 32,
};

/// "Codex sign in".
pub fn sign_in_title(tool: Tool) -> String {
    format!("{} sign in", tool.display_name())
}

/// "Installing Codex".
pub fn install_title(tool: Tool) -> String {
    format!("Installing {}", tool.display_name())
}

/// "Codex has 2 agents running. End them first."
fn running_message(tool: Tool, running: i64) -> String {
    let name = tool.display_name();
    if running == 1 {
        format!("{name} has 1 agent running. End it first.")
    } else {
        format!("{name} has {running} agents running. End them first.")
    }
}

fn not_built(request: &str) -> BoardError {
    BoardError::refused(
        Refusal::NotAvailableYet,
        format!("{request} isn't built yet"),
    )
}

/// The hidden tools stored in `settings`; names this build doesn't know are left out.
pub async fn hidden_tools(store: &Store) -> Result<Vec<Tool>, StoreError> {
    stored_tools(store, AGENT_TOOLS_HIDDEN).await
}

/// A list of tools stored in `settings` under `key`; names this build doesn't know are left out.
pub(super) async fn stored_tools(store: &Store, key: &str) -> Result<Vec<Tool>, StoreError> {
    let names: Vec<String> = store
        .setting(key)
        .await?
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    Ok(names.iter().filter_map(|name| name.parse().ok()).collect())
}

pub(super) async fn store_tools(
    store: &Store,
    key: &str,
    tools: &[Tool],
) -> Result<(), StoreError> {
    let names: Vec<&str> = tools.iter().map(|t| t.as_str()).collect();
    let json = serde_json::to_string(&names).expect("names serialise");
    store.set_setting(key, &json).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected(version: &str) -> AgentToolState {
        AgentToolState::Connected {
            version: Some(version.into()),
            account: None,
        }
    }

    #[test]
    fn states_from_detection() {
        let found = |signed_in, update_needed| Detection {
            installed: true,
            signed_in,
            version: Some("0.157.1".into()),
            update_needed,
            account: Some("signed in with ChatGPT".into()),
            sign_in_offered: false,
        };
        assert_eq!(
            AgentToolState::from_detection(&Detection::default()),
            AgentToolState::NotInstalled
        );
        assert_eq!(
            AgentToolState::from_detection(&found(Some(true), true)),
            AgentToolState::UpdateNeeded {
                version: Some("0.157.1".into())
            }
        );
        assert_eq!(
            AgentToolState::from_detection(&found(Some(false), false)),
            AgentToolState::FoundNotSignedIn {
                version: Some("0.157.1".into())
            }
        );
        assert_eq!(
            AgentToolState::from_detection(&found(Some(true), false)),
            AgentToolState::Connected {
                version: Some("0.157.1".into()),
                account: Some("signed in with ChatGPT".into())
            }
        );
        // No sign-in answer (a slow or failing status command) is never signed in, and doesn't count.
        let unchecked = AgentToolState::from_detection(&found(None, false));
        assert_eq!(
            unchecked,
            AgentToolState::CouldNotCheck {
                version: Some("0.157.1".into())
            }
        );
        assert!(!unchecked.is_connected());
        assert_eq!(
            unchecked.unavailable_reason(),
            Some("couldn\u{2019}t check its sign-in")
        );
    }

    #[test]
    fn hidden_wins_over_a_found_tool_and_is_cleared() {
        let tools = ConnectedTools::new([Tool::Copilot]);
        for tool in Tool::ALL.iter().copied() {
            tools.set_detected(tool, connected("1.0.98"), 10);
        }
        assert_eq!(
            tools.connected(),
            [Tool::Claude, Tool::Codex, Tool::OpenCode]
        );
        assert_eq!(
            tools.status(Tool::Copilot).state,
            AgentToolState::Hidden {
                version: Some("1.0.98".into())
            }
        );
        let refused = tools.is_available(Tool::Copilot).unwrap_err();
        assert_eq!(
            refused.message(Tool::Copilot),
            "GitHub Copilot isn\u{2019}t available \u{b7} hidden in Settings"
        );

        tools.hide(Tool::Copilot, false);
        assert_eq!(tools.connected(), Tool::ALL);
        assert_eq!(tools.is_available(Tool::Copilot), Ok(()));
        tools.hide(Tool::Codex, true);
        assert_eq!(
            tools.connected(),
            [Tool::Claude, Tool::Copilot, Tool::OpenCode]
        );

        // A hidden tool that isn't installed reads as not installed.
        tools.set_detected(Tool::Codex, AgentToolState::NotInstalled, 11);
        assert_eq!(
            tools.status(Tool::Codex).state,
            AgentToolState::NotInstalled
        );
        assert_eq!(
            tools
                .is_available(Tool::Codex)
                .unwrap_err()
                .message(Tool::Codex),
            "Codex isn\u{2019}t available \u{b7} not installed"
        );
    }

    #[test]
    fn connected_keeps_the_tools_order_whatever_the_order_found() {
        let tools = ConnectedTools::new([]);
        assert!(tools.connected().is_empty(), "nothing checked yet");
        for tool in [Tool::OpenCode, Tool::Claude, Tool::Codex] {
            tools.set_detected(tool, connected("1"), 1);
        }
        assert_eq!(
            tools.connected(),
            [Tool::Claude, Tool::Codex, Tool::OpenCode]
        );
        let list = tools.list();
        let order: Vec<Tool> = list.tools.iter().map(|s| s.tool).collect();
        assert_eq!(order, Tool::ALL);
        assert_eq!(list.tools[2].checked_at, None, "Copilot not checked");
    }

    #[test]
    fn the_watch_fires_once_per_change() {
        let tools = ConnectedTools::new([]);
        let mut changes = tools.subscribe();
        assert!(!changes.has_changed().unwrap());

        tools.set_detected(Tool::Codex, connected("0.157.1"), 5);
        assert!(changes.has_changed().unwrap());
        let rows = changes.borrow_and_update().clone();
        assert!(rows[1].state.is_connected());
        assert!(!changes.has_changed().unwrap());

        tools.hide(Tool::Codex, true);
        assert!(changes.has_changed().unwrap());
        changes.mark_unchanged();
        // The same again changes nothing, so nothing is sent.
        tools.hide(Tool::Codex, true);
        tools.set_detected(Tool::Codex, connected("0.157.1"), 5);
        assert!(!changes.has_changed().unwrap());

        tools.hide(Tool::Codex, false);
        assert!(changes.has_changed().unwrap());
        changes.mark_unchanged();
        tools.set_note(Tool::Codex, "limits not reported", true);
        assert!(changes.has_changed().unwrap());
        assert_eq!(
            changes.borrow_and_update()[1].notes,
            ["limits not reported"]
        );
        tools.set_note(Tool::Codex, "limits not reported", true);
        assert!(!changes.has_changed().unwrap());
    }

    #[test]
    fn rows_serialise_with_their_state() {
        let tools = ConnectedTools::new([]);
        tools.set_detected(Tool::Codex, connected("0.157.1"), 5);
        let json = serde_json::to_value(tools.status(Tool::Codex)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"tool": "codex", "state": "connected", "version": "0.157.1", "account": null,
                               "checked_at": 5})
        );
    }
}
