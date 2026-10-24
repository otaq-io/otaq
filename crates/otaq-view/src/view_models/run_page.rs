//! Run, Shell and the running processes as the Tauri page draws them (T-0232; RUN-01…RUN-05; d0026, d0035, d0046;
//! *13 · Run & Shell*): [`run`], [`run_command`], [`processes`] and [`shell`] turned into plain, serialized shapes.
//! The page keeps only the typed fields, the click and which overlay is open.

use otaq_core::session::SessionInfo;
use otaq_core::terminals::run::{RunInfo, RunState};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::view_models::menu::MenuItem;
use crate::view_models::processes;
use crate::view_models::run::{self as vm, RunButton, Tone};
use crate::view_models::run_command::{Phase, RunCommandModel};
use crate::view_models::session_tab;
use crate::view_models::shell;
use crate::view_models::toasts::{NewToast, ToastLevel};

/// The colour of the Run's state words on its line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "RunTone")]
pub enum ToneView {
    Neutral,
    Progress,
    Success,
    Danger,
}

impl From<Tone> for ToneView {
    fn from(tone: Tone) -> Self {
        match tone {
            Tone::Neutral => Self::Neutral,
            Tone::Progress => Self::Progress,
            Tone::Success => Self::Success,
            Tone::Danger => Self::Danger,
        }
    }
}

/// What a button on the Run's line does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "RunButtonKind")]
pub enum ButtonKind {
    Run,
    Stop,
    Retry,
    CopyLsof,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RunButtonView")]
pub struct ButtonView {
    pub kind: ButtonKind,
    pub label: String,
    pub icon: IconName,
    pub primary: bool,
    /// Copy lsof command's text.
    pub copy: Option<String>,
}

/// The Run's state in the line's words (Q433): "Serving", "Stopped by you at 11:48", "Failed to start · exit 1".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RunStateView")]
pub struct StateView {
    pub text: String,
    pub tone: ToneView,
    pub icon: IconName,
}

/// Session's "Open http://localhost:4022" (SHT-04), while the Run is up and the project sets an Open URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RunOpenUrl")]
pub struct OpenUrlView {
    pub label: String,
    pub url: String,
}

/// The ⋯ menu's Edit command (RUN-02's dialog).
pub const MENU_EDIT_COMMAND: &str = "edit_command";

/// What `run.tab` answers: Session's Run line and the Run's output session. No state card (Q433): the line says
/// the state, and Run / Stop sit on it, Edit command under its ⋯; the terminal gets the space.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "RunTabView")]
pub struct TabView {
    pub key: String,
    pub project: String,
    /// "pnpm dev · in SHOP-22’s worktree · :4022" (SHT-04); none before a run command is set.
    pub line: Option<String>,
    pub state: StateView,
    /// Why, under the line: a failure's reason, the port's owner, "No run command yet…".
    pub message: Option<String>,
    pub open_url: Option<OpenUrlView>,
    /// Run, Stop, Retry, Copy lsof command: the state's buttons after Open.
    pub buttons: Vec<ButtonView>,
    /// "No Open URL set for billing-api, so there’s no Open button." (*Run · backend*): the line's tooltip.
    pub note: Option<String>,
    /// The line's ⋯: Edit command.
    pub menu: Vec<MenuItem>,
    pub menu_label: &'static str,
    /// The Run's session, attached read-only; `None` before the first Run.
    pub session: Option<String>,
    pub idle: &'static str,
}

pub fn tab_view(run: &RunInfo, hhmm: impl Fn(i64) -> String) -> TabView {
    let card = vm::card(run, hhmm);
    let port = run.port.map(|p| p.to_string()).unwrap_or_default();
    // Retry leads unless the command is what needs fixing (it's under ⋯ now).
    let fix_first = card.buttons.contains(&RunButton::EditCommand);
    let buttons = card
        .buttons
        .iter()
        .filter_map(|b| {
            let (kind, label, icon, primary, copy) = match b {
                RunButton::Run => (ButtonKind::Run, "Run", IconName::Play, true, None),
                RunButton::Stop => (ButtonKind::Stop, "Stop", IconName::Square, false, None),
                RunButton::Retry => (
                    ButtonKind::Retry,
                    "Retry",
                    IconName::RotateCcw,
                    !fix_first,
                    None,
                ),
                RunButton::CopyLsof => (
                    ButtonKind::CopyLsof,
                    "Copy lsof command",
                    IconName::Copy,
                    false,
                    Some(vm::lsof(&port)),
                ),
                // Open is the line's "Open http://localhost:4022"; Edit command is under ⋯.
                RunButton::Open { .. } | RunButton::EditCommand => return None,
            };
            Some(ButtonView {
                kind,
                label: label.to_owned(),
                icon,
                primary,
                copy,
            })
        })
        .collect();
    let icon = match (run.state, card.tone) {
        (RunState::NotStarted, _) => IconName::Play,
        (_, Tone::Neutral) => IconName::Square,
        (_, Tone::Progress) => IconName::LoaderCircle,
        (_, Tone::Success) => IconName::Dot,
        (_, Tone::Danger) => IconName::TriangleAlert,
    };
    // A set command needs no "Runs the project's run command…": the line says it.
    let message = match run.state {
        RunState::NotStarted if !run.command.is_empty() => None,
        _ => card.message,
    };
    TabView {
        key: run.key.to_string(),
        project: run.project.clone(),
        line: session_tab::run_line(&run.command, &run.key, run.port),
        state: StateView {
            text: card.title,
            tone: card.tone.into(),
            icon,
        },
        message,
        open_url: session_tab::open_url(run).map(|(label, url)| OpenUrlView { label, url }),
        buttons,
        note: (run.state.is_live() && run.open_url.as_deref().unwrap_or("").is_empty()).then(
            || {
                format!(
                    "No Open URL set for {}, so there\u{2019}s no Open button.",
                    run.project_name
                )
            },
        ),
        menu: vec![MenuItem::new(MENU_EDIT_COMMAND, "Edit command").icon(IconName::Pencil)],
        menu_label: "Run options",
        session: run.session.clone(),
        idle: "(not running)",
    }
}

/// One row of *All running processes · header menu*.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ProcessRowView")]
pub struct ProcessRowView {
    pub project_name: String,
    pub key: String,
    pub title: String,
    pub port: Option<String>,
    pub uptime: Option<String>,
    pub url: Option<String>,
    /// "Open :4020"
    pub open: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ProcessesView")]
pub struct ProcessesView {
    pub title: &'static str,
    pub stop_all: &'static str,
    pub stop: &'static str,
    pub empty: &'static str,
    pub rows: Vec<ProcessRowView>,
}

pub fn processes_view(runs: &[RunInfo], now_ms: i64) -> ProcessesView {
    ProcessesView {
        title: "Run processes",
        stop_all: "Stop all",
        stop: "Stop",
        empty: "Nothing runs.",
        rows: processes::rows(runs, now_ms)
            .into_iter()
            .map(|r| ProcessRowView {
                open: r
                    .url
                    .as_ref()
                    .and(r.port.as_ref())
                    .map(|port| format!("Open {port}")),
                project_name: r.project_name,
                key: r.key.to_string(),
                title: r.title,
                port: r.port,
                uptime: r.uptime,
                url: r.url,
            })
            .collect(),
    }
}

/// "The agent is reading the repo…" and what it looked at (*2 · Claude thinking*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RunCommandThinking")]
pub struct ThinkingView {
    pub title: &'static str,
    pub lines: Vec<String>,
}

/// The label of Ask the agent, here and on project settings' Run command row (d0086; was "Ask Claude", d0035).
pub const ASK_LABEL: &str = "Ask the agent";

/// Ask the agent's tooltip: who is asked, the project's Project agent `agent` ("Strong").
pub fn ask_tooltip(agent: &str) -> String {
    format!("Asks {agent} (the Project agent\u{2019}s) to suggest a run command")
}

/// Ask the agent and the line next to it (*1 · Empty*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RunCommandAsk")]
pub struct AskView {
    pub label: &'static str,
    /// "Asks Strong (the Project agent's) to suggest a run command".
    pub tooltip: String,
    pub note: &'static str,
}

/// The fields' new values after a suggestion arrived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RunCommandFill")]
pub struct FillView {
    pub command: String,
    pub open_url: String,
}

/// The run-command dialog (RUN-02; *13 → 1 · Empty … 5 · Saved*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "RunCommandView")]
pub struct CommandView {
    pub project: String,
    pub title: String,
    pub intro: &'static str,
    pub command_placeholder: &'static str,
    pub url_label: &'static str,
    pub optional: &'static str,
    pub url_placeholder: &'static str,
    pub url_hint: &'static str,
    pub ask: Option<AskView>,
    pub thinking: Option<ThinkingView>,
    /// The agent's why, after a suggestion (*3 · Suggestion*).
    pub why: Option<String>,
    /// "Saved for shop-web. Running SHOP-19 on :4020… Change it later in project settings."
    pub saved: Option<String>,
    pub error: Option<String>,
    pub save: &'static str,
    pub can_save: bool,
    pub cancel: &'static str,
    pub done: &'static str,
    /// Set once: the fields take the agent's suggestion.
    pub fill: Option<FillView>,
}

pub fn command_view(model: &RunCommandModel, command: &str, fill: Option<FillView>) -> CommandView {
    CommandView {
        project: model.project.clone(),
        title: model.title(),
        intro: "Runs in the ticket\u{2019}s worktree when you press Run. Use $PORT if it listens on a port. Saved in \
                project settings \u{2014} nothing is written to the repo.",
        command_placeholder: "e.g. pnpm dev \u{b7} dotnet run \u{b7} cargo run -- serve \u{b7} python -m app",
        url_label: "Open URL",
        optional: "optional",
        url_placeholder: "e.g. http://localhost:$PORT",
        url_hint: "Leave empty for backends, CLIs and libraries \u{2014} then there\u{2019}s no Open button.",
        ask: model.offers_ask(command).then(|| AskView {
            label: ASK_LABEL,
            tooltip: ask_tooltip(&model.agent),
            note: "The agent looks at the repo and suggests a command.",
        }),
        thinking: match &model.phase {
            Phase::Thinking { lines } => Some(ThinkingView {
                title: "The agent is reading the repo\u{2026}",
                lines: lines.iter().map(|l| format!("\u{b7} {l}")).collect(),
            }),
            _ => None,
        },
        why: match &model.phase {
            Phase::Suggested { why } if !why.is_empty() => Some(why.clone()),
            _ => None,
        },
        saved: match &model.phase {
            Phase::Saved { message } => Some(message.clone()),
            _ => None,
        },
        error: model.error.clone(),
        save: model.save_label(),
        can_save: model.can_save(command),
        cancel: "Cancel",
        done: "Done",
        fill,
    }
}

/// "⌃` leave terminal" while the shell has the keyboard: the user's key for Terminals inside a terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShellLeave")]
pub struct LeaveView {
    pub text: &'static str,
    pub keys: Option<String>,
}

/// A shell a restart stopped (d0045): it stays stopped until Reopen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShellStopped")]
pub struct StoppedView {
    pub title: String,
    pub note: String,
}

/// What `run.shell` answers (*13 → Shell tab · BILL-4*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ShellTabView")]
pub struct ShellView {
    pub key: String,
    pub label: &'static str,
    /// "~/.otaq/wt/billing-api/BILL-4"
    pub path: Option<String>,
    /// "Run port · $PORT=4004"
    pub port: Option<String>,
    pub session: Option<String>,
    pub live: bool,
    /// Why there is no shell (no worktree yet), as the service said.
    pub problem: Option<String>,
    pub stopped: Option<StoppedView>,
    pub reopen: &'static str,
    pub hint: &'static str,
    pub leave: LeaveView,
    /// "BILL-4 · shell"
    pub terminal_label: String,
}

pub struct ShellFacts<'a> {
    pub key: &'a str,
    pub worktree: Option<&'a str>,
    pub port: Option<u16>,
    pub session: Option<&'a SessionInfo>,
    pub problem: Option<String>,
    pub keys: &'a KeyLabels,
}

pub fn shell_view(f: ShellFacts, hhmm: impl Fn(i64) -> String) -> ShellView {
    ShellView {
        key: f.key.to_owned(),
        label: shell::SHELL_LABEL,
        path: f
            .worktree
            .map(|w| crate::view_models::terminal_strip::short_path(&crate::shell::tilde(w))),
        port: f.port.map(shell::port_label),
        session: f.session.map(|s| s.id.clone()),
        live: f.session.is_some_and(SessionInfo::is_running),
        problem: f.problem,
        stopped: f
            .session
            .and_then(|s| shell::stopped_note(s, hhmm))
            .map(|(title, note)| StoppedView { title, note }),
        reopen: "Reopen",
        hint: "Typing goes to the shell, not the agent",
        leave: LeaveView {
            text: "leave terminal",
            keys: f.keys.in_terminal(KeyAction::Terminals),
        },
        terminal_label: format!("{} \u{b7} shell", f.key),
    }
}

/// "Run stopped · SHOP-19 is done" when a Run stops because its ticket reached Done or was closed (RUN-04).
pub fn stopped_toast(event: &otaq_core::protocol::Event) -> Option<NewToast> {
    let otaq_core::protocol::Event::RunStopped { key, message, .. } = event else {
        return None;
    };
    let mut toast = NewToast::new(ToastLevel::Success, message.clone(), None);
    toast.ticket = Some(key.to_string());
    Some(toast)
}

#[cfg(test)]
mod tests {
    use otaq_core::session::{END_STOPPED_BY_RESTART, SessionKind, SessionState};
    use otaq_core::terminals::run::StopCause;
    use otaq_core::terminals::suggest::Suggestion;

    use super::*;

    fn run(state: RunState) -> RunInfo {
        RunInfo {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            key: "SHOP-19".parse().unwrap(),
            title: "Price table".into(),
            state,
            port: Some(4020),
            command: "pnpm dev --port $PORT".into(),
            open_url: Some("http://localhost:$PORT".into()),
            session: Some("s1".into()),
            started_at: Some(1_000),
            ended_at: Some(4_000),
            exit_code: Some(1),
            stopped_by: None,
        }
    }

    const HHMM: fn(i64) -> String = |_| "11:48".to_owned();

    fn kinds(v: &TabView) -> Vec<ButtonKind> {
        v.buttons.iter().map(|b| b.kind).collect()
    }

    /// Q433: no state card. Each state reads in the line's words, with its buttons on the line and Edit command
    /// always under ⋯.
    #[test]
    fn the_run_line_in_each_state() {
        let line = Some("pnpm dev --port $PORT \u{b7} in SHOP-19\u{2019}s worktree \u{b7} :4020");
        let words = |v: &TabView| (v.state.text.clone(), v.state.tone);

        let idle = tab_view(&run(RunState::NotStarted), HHMM);
        assert_eq!(idle.line.as_deref(), line);
        assert_eq!(words(&idle), ("Not running".into(), ToneView::Neutral));
        assert_eq!(idle.state.icon, IconName::Play);
        assert_eq!(idle.message, None, "the line already says what runs");
        assert_eq!(kinds(&idle), [ButtonKind::Run]);
        assert!(idle.buttons[0].primary);
        assert_eq!(idle.open_url, None);
        assert_eq!(
            idle.menu
                .iter()
                .map(|m| (m.id.as_str(), m.label.as_str()))
                .collect::<Vec<_>>(),
            [(MENU_EDIT_COMMAND, "Edit command")]
        );

        let mut unset = run(RunState::NotStarted);
        unset.command = String::new();
        let unset = tab_view(&unset, HHMM);
        assert_eq!(unset.line, None);
        assert_eq!(
            unset.message.as_deref(),
            Some("No run command yet. Run asks for the command that starts this project.")
        );
        assert_eq!(kinds(&unset), [ButtonKind::Run]);

        let starting = tab_view(&run(RunState::Starting), HHMM);
        assert_eq!(
            words(&starting),
            ("Starting\u{2026}".into(), ToneView::Progress)
        );
        assert_eq!(starting.open_url, None, "Open waits for the port");
        assert_eq!(kinds(&starting), [ButtonKind::Stop]);

        let up = tab_view(&run(RunState::Running), HHMM);
        assert_eq!(words(&up), ("Serving".into(), ToneView::Success));
        assert_eq!(
            up.open_url,
            Some(OpenUrlView {
                label: "Open http://localhost:4020".into(),
                url: "http://localhost:4020".into()
            })
        );
        assert_eq!(kinds(&up), [ButtonKind::Stop]);
        assert!(!up.buttons[0].primary);
        assert_eq!(up.note, None);

        let mut stopped = run(RunState::Stopped);
        stopped.stopped_by = Some(StopCause::You);
        let stopped = tab_view(&stopped, HHMM);
        assert_eq!(
            words(&stopped),
            ("Stopped by you at 11:48".into(), ToneView::Neutral)
        );
        assert_eq!(kinds(&stopped), [ButtonKind::Run]);
        assert_eq!(
            tab_view(&run(RunState::Stopped), HHMM).state.text,
            "Stopped \u{b7} exited 1 at 11:48"
        );

        let failed = tab_view(&run(RunState::FailedToStart), HHMM);
        assert_eq!(
            words(&failed),
            ("Failed to start \u{b7} exit 1".into(), ToneView::Danger)
        );
        assert_eq!(
            failed.message.as_deref(),
            Some(
                "pnpm exited after 3s. The command may be wrong for this project, or setup didn\u{2019}t finish."
            )
        );
        assert_eq!(kinds(&failed), [ButtonKind::Retry]);
        assert!(!failed.buttons[0].primary, "fixing the command comes first");
        assert_eq!(failed.menu[0].id, MENU_EDIT_COMMAND);

        let taken = tab_view(&run(RunState::PortInUse), HHMM);
        assert_eq!(
            words(&taken),
            ("Port 4020 is in use".into(), ToneView::Danger)
        );
        assert!(
            taken
                .message
                .as_deref()
                .unwrap()
                .starts_with("Another process is listening on 4020.")
        );
        assert_eq!(kinds(&taken), [ButtonKind::Retry, ButtonKind::CopyLsof]);
        assert!(taken.buttons[0].primary);
        assert_eq!(taken.buttons[1].copy.as_deref(), Some("lsof -i :4020"));

        let restarted = tab_view(&run(RunState::StoppedByRestart), HHMM);
        assert_eq!(
            restarted.state.text,
            "Stopped by restart \u{b7} was running at 11:48"
        );
        assert_eq!(kinds(&restarted), [ButtonKind::Run]);

        let mut backend = run(RunState::Running);
        backend.project_name = "billing-api".into();
        backend.open_url = None;
        let backend = tab_view(&backend, HHMM);
        assert_eq!(backend.open_url, None);
        assert_eq!(kinds(&backend), [ButtonKind::Stop]);
        assert_eq!(
            backend.note.as_deref(),
            Some("No Open URL set for billing-api, so there\u{2019}s no Open button.")
        );
    }

    #[test]
    fn the_processes_menu_lists_every_live_run() {
        let mut other = run(RunState::Running);
        other.project_name = "billing-api".into();
        other.key = "BILL-4".parse().unwrap();
        other.open_url = None;
        let view = processes_view(
            &[run(RunState::Running), other, run(RunState::Stopped)],
            61_000,
        );
        assert_eq!(view.title, "Run processes");
        assert_eq!(view.rows.len(), 2);
        assert_eq!(view.rows[0].key, "BILL-4");
        assert_eq!(view.rows[0].open, None, "no Open URL, no Open");
        assert_eq!(view.rows[1].open.as_deref(), Some("Open :4020"));
    }

    #[test]
    fn the_run_command_dialog_walks_its_five_steps() {
        let r = run(RunState::NotStarted);
        let mut m = RunCommandModel::new("SHOP", "shop-web", "Strong", Some(&r), "");
        let v = command_view(&m, "", None);
        assert_eq!(v.title, "What command runs shop-web?");
        let ask = v.ask.as_ref().unwrap();
        assert_eq!(
            (ask.label, ask.tooltip.as_str(), ask.note),
            (
                "Ask the agent",
                "Asks Strong (the Project agent\u{2019}s) to suggest a run command",
                "The agent looks at the repo and suggests a command."
            )
        );
        assert!(!v.can_save);
        assert_eq!(v.save, "Save & start");
        assert!(m.ask());
        m.progress("Read(package.json)");
        let v = command_view(&m, "", None);
        assert!(v.ask.is_none());
        let thinking = v.thinking.unwrap();
        assert_eq!(thinking.title, "The agent is reading the repo\u{2026}");
        assert_eq!(thinking.lines, ["\u{b7} Read(package.json)"]);
        let fill = m.suggested(&Suggestion {
            command: "pnpm dev --port $PORT".into(),
            open_url: "http://localhost:$PORT".into(),
            why: "package.json has \"dev\".".into(),
        });
        let v = command_view(
            &m,
            "pnpm dev --port $PORT",
            fill.map(|(command, open_url)| FillView { command, open_url }),
        );
        assert_eq!(v.why.as_deref(), Some("package.json has \"dev\"."));
        assert_eq!(v.fill.unwrap().command, "pnpm dev --port $PORT");
        assert!(v.can_save);
        m.edited();
        m.saved();
        let v = command_view(&m, "pnpm dev --port $PORT --turbo", None);
        assert_eq!(
            v.saved.as_deref(),
            Some(
                "Saved for shop-web. Running SHOP-19 on :4020\u{2026} Change it later in project settings."
            )
        );
        assert!(!v.can_save);
    }

    #[test]
    fn the_shell_strip_and_its_stopped_state() {
        let session = SessionInfo {
            id: "sh1".into(),
            kind: SessionKind::Shell,
            project: "BILL".into(),
            ticket: Some("BILL-4".parse().unwrap()),
            state: SessionState::Ended,
            started_at: 0,
            ended_at: None,
            end_reason: Some(END_STOPPED_BY_RESTART.into()),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            model_ran: None,
            size: None,
        };
        let v = shell_view(
            ShellFacts {
                key: "BILL-4",
                worktree: Some("/w/billing-api/BILL-4"),
                port: Some(4004),
                session: Some(&session),
                problem: None,
                keys: &crate::key_labels::fixtures::base(),
            },
            |_| "08:24".to_owned(),
        );
        assert_eq!(v.label, "Shell \u{b7} not the agent");
        assert_eq!(v.port.as_deref(), Some("Run port \u{b7} $PORT=4004"));
        assert!(!v.live);
        assert_eq!(
            v.stopped.unwrap().title,
            "Stopped by restart \u{b7} was open at 08:24"
        );
        assert_eq!(v.hint, "Typing goes to the shell, not the agent");
        assert_eq!(v.leave.keys.as_deref(), Some("⌃`"));
    }

    /// KEY-08: "leave terminal" names the user's key for Terminals as it works inside a terminal: a recorded key,
    /// none once removed (its `g` `t` doesn't work there), the Windows/Linux terminal column.
    #[test]
    fn leave_terminal_shows_the_users_key_in_a_terminal() {
        use crate::key_labels::fixtures;
        let leave = |keys: &KeyLabels| {
            shell_view(
                ShellFacts {
                    key: "BILL-4",
                    worktree: None,
                    port: None,
                    session: None,
                    problem: None,
                    keys,
                },
                |_| String::new(),
            )
            .leave
            .keys
        };
        assert_eq!(
            leave(&fixtures::replaced("terminals", None, 0, "⌘T")).as_deref(),
            Some("⌘T")
        );
        assert_eq!(leave(&fixtures::removed("terminals", None, 0)), None);
        assert_eq!(leave(&fixtures::letters_off()).as_deref(), Some("⌃`"));
        assert_eq!(
            leave(&KeyLabels::base(crate::keymap::Platform::WinLinux)).as_deref(),
            Some("Ctrl+`")
        );
    }

    #[test]
    fn a_run_stopped_at_done_shows_a_toast() {
        let toast = stopped_toast(&otaq_core::protocol::Event::RunStopped {
            project: "SHOP".into(),
            key: "SHOP-19".parse().unwrap(),
            message: "Run stopped \u{b7} SHOP-19 is done".into(),
        })
        .unwrap();
        assert_eq!(toast.title, "Run stopped \u{b7} SHOP-19 is done");
        assert_eq!(toast.ticket.as_deref(), Some("SHOP-19"));
    }
}
