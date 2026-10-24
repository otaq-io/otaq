//! `terminal_tab.*`: the terminal of the sheet's Session (SHT-04, SES-09, SES-10, SES-11, SES-06, SES-07, SES-08,
//! ATT-03, REC-02; d0022, d0042, d0048, d0040, d0045, d0063; canvas `s25-06`). One view per open ticket: the agent
//! session the Agent side attaches to, the line over it ("Rust developer · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/
//! SHOP-22", "Typing goes to Rust developer"), the strip (state, worktree, `$PORT`, "1 message waiting · Deliver now"
//! or the attach command, font size, find), the hint line under the terminal and its "Instructions: shop-web" link,
//! the end marker of an ended session, and "Before the restart" for an Interrupted ticket whose session is gone. The
//! state label and the hint depend on the terminal's phase and focus, which only the page knows, so the view carries
//! every combination and the page picks one (`terminal_strip.rs` words them all). Once a check stage ran, the view
//! also carries the check's side (its running or last session, `session_tab::check_side`), which Session's switch
//! names after its agent (VAL-11, d0063, d0108).

use otaq_core::board::tickets::Ticket;
use otaq_core::board::validation::ValidationRecord;
use otaq_core::domain::TicketKey;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::protocol::{BeforeRestartParams, KeyParams, Reply, Request, SessionListParams};
use otaq_core::recovery::BeforeRestart;
use otaq_core::session::deliveries::Delivery;
use otaq_core::session::{SessionInfo, SessionState};
use otaq_view::key_labels::KeyLabels;
use otaq_view::keymap::KeyAction;
use otaq_view::terminal::Phase;
use otaq_view::view_models::session_tab::{self, CheckSide};
use otaq_view::view_models::recovery::{self, RecoveryView};
use otaq_view::view_models::terminal_strip::{
    self as strip, Choice, EndMarker, Hint, StripState, UnsentPrompt,
};
use otaq_view::view_models::toasts::{NewToast, ToastLevel};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::panel::{Hint as Tooltip, Sent, Unsent};
use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "terminal_tab.view" => reply(&view(app, parse(params)?).await?),
        "terminal_tab.deliver_now" => reply(&deliver_now(app, parse(params)?).await?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

fn unexpected(what: &str, other: &Reply) -> RequestError {
    RequestError::plain(format!("unexpected answer to {what}: {other:?}"))
}

/// What `terminal_tab.view` takes: the open ticket.
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "TerminalTabParams")]
pub struct Params {
    pub key: String,
}

/// What `terminal_tab.deliver_now` takes: `clear` after the user chose "Clear it and send".
#[derive(Debug, Clone, Deserialize, TS)]
#[ts(rename = "TerminalTabDeliverParams")]
pub struct DeliverParams {
    pub key: String,
    #[serde(default)]
    pub clear: bool,
}

/// The state label and hint for one phase and focus of the terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TerminalTabStrip")]
pub struct Strip {
    pub phase: Phase,
    pub focused: bool,
    pub state: StripState,
    pub hint: Option<Hint>,
}

/// "1 message waiting · Deliver now" (SES-06).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TerminalTabWaiting")]
pub struct Waiting {
    pub label: String,
    pub deliver: &'static str,
}

/// "Instructions: shop-web": the project's settings on Project notes (d0040, AGM-02).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TerminalTabInstructions")]
pub struct Instructions {
    pub label: String,
    pub project: String,
}

/// One line of "Before the restart": time, who or what, text; `action` for a tool (accent colour).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TerminalTabRecoveryLine")]
pub struct RecoveryLine {
    pub time: String,
    pub who: String,
    pub text: String,
    pub action: bool,
}

/// Session's terminal for an Interrupted (or Resume failed) ticket whose session is gone (REC-02, REC-03; *06 →
/// Interrupted · Before the restart*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "TerminalTabRecovery")]
pub enum Recovery {
    /// The tool's saved conversation is still being read.
    Loading { text: String },
    /// The last messages and actions with Resume.
    BeforeRestart {
        title: &'static str,
        source: String,
        session: String,
        lines: Vec<RecoveryLine>,
        ended: String,
        resume: &'static str,
    },
    /// "Conversation no longer available", with Retry and Start from scratch.
    NotAvailable {
        label: &'static str,
        text: String,
        retry: &'static str,
        scratch: &'static str,
    },
}

/// The check's side (VAL-11, d0063, d0108): the ticket's running check session, else its last one, read-only until
/// the next round starts. `TerminalTabCheck` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "TerminalTabCheck")]
pub struct Check {
    pub session: String,
    /// Its agent's name, Session's switch's label: "Validator", "Security reviewer".
    pub label: String,
    /// The terminal's accessible name: "SHOP-19 · Security reviewer terminal".
    pub terminal_label: String,
    /// "Security reviewer · Claude Code · sonnet · ~/.otaq/wt/shop-web/SHOP-19".
    pub line: String,
    /// "Typing goes to Security reviewer · SHOP-19" (or "Click to type to …") while it runs.
    pub typing: Option<session_tab::Typing>,
    /// It ended: the terminal takes no input.
    pub read_only: bool,
    /// The state label and hint for every phase and focus, as [`View::strips`].
    pub strips: Vec<Strip>,
    /// `otaq attach SHOP-19 --stage security` while it runs.
    pub attach_command: String,
}

/// Session's terminal for one ticket. `TerminalTabView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "TerminalTabView")]
pub struct View {
    pub key: String,
    /// The agent session the terminal attaches to (the running one, else the newest); `None` before the first.
    pub session: Option<String>,
    /// The terminal's accessible name: "SHOP-19 · agent terminal".
    pub label: String,
    /// What the tab says while there is no session.
    pub no_session: String,
    /// The state label and hint for every phase and focus; the page picks the terminal's.
    pub strips: Vec<Strip>,
    /// `…/wt/shop-web/SHOP-19`.
    pub path: Option<String>,
    /// `agent :4019`.
    pub port: Option<String>,
    /// `otaq attach SHOP-19`, and its short form in a narrow panel.
    pub attach_command: String,
    pub attach_short: &'static str,
    pub copy_attach: &'static str,
    pub waiting: Option<Waiting>,
    pub instructions: Instructions,
    /// "leave terminal" with the user's key for Terminals (⌃`), while the terminal has the keyboard.
    pub leave: Tooltip,
    pub smaller: &'static str,
    pub bigger: &'static str,
    pub find: Tooltip,
    /// Working in background: what runs, on hover of the state label.
    pub background: Option<String>,
    /// Under this width the strip drops the path and port (*Narrow panel · 480 px*).
    pub narrow_below: f32,
    pub ended: Option<EndMarker>,
    pub recovery: Option<Recovery>,
    /// "Rust developer · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-22": the agent session's, once there is one.
    pub line: Option<String>,
    /// "Typing goes to Rust developer · SHOP-22" (or "Click to type to …") while its session runs.
    pub typing: Option<session_tab::Typing>,
    /// Set once a check stage ran: Session's switch shows its side.
    pub check: Option<Check>,
}

/// What `terminal_tab.view` answers: the view, or "SHOP-25 is gone." once the ticket was deleted.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "TerminalTabLoad")]
pub struct Load {
    pub view: Option<View>,
    pub gone: String,
}

/// The check's side from its session and the ticket's rounds (the one it ran, else the newest).
pub fn check_of(
    ticket: &Ticket,
    side: &CheckSide,
    session: &SessionInfo,
    rounds: &[ValidationRecord],
) -> Check {
    let record = rounds
        .iter()
        .find(|r| r.session.as_deref() == Some(session.id.as_str()))
        .or_else(|| rounds.iter().max_by_key(|r| r.started_at));
    let outcome = record.and_then(|r| r.outcome);
    let strips = PHASES
        .iter()
        .flat_map(|phase| [false, true].map(|focused| (phase.clone(), focused)))
        .map(|(phase, focused)| Strip {
            state: strip::validator_state(&phase, ticket.status, outcome),
            hint: strip::validator_hint(&phase, focused, outcome),
            phase,
            focused,
        })
        .collect();
    let worktree = ticket.work.as_ref().and_then(|w| w.worktree.as_deref());
    Check {
        session: session.id.clone(),
        label: side.agent.clone(),
        terminal_label: format!("{} \u{b7} {} terminal", ticket.key, side.agent),
        line: session_tab::agent_line(&side.agent, session.tool, model_of(session), worktree),
        typing: side
            .running
            .then(|| session_tab::typing_goes_to(&side.agent, &ticket.key)),
        read_only: !side.running,
        strips,
        attach_command: strip::validator_attach_command(
            &ticket.key,
            side.stage_id.as_deref().filter(|_| side.running),
        ),
    }
}

/// The model a session ran on: the exact one the tool reported, else the one passed.
fn model_of(session: &SessionInfo) -> Option<&str> {
    session.model_ran.as_deref().or(session.model.as_deref())
}

const PHASES: [Phase; 5] = [
    Phase::NoSession,
    Phase::Attaching,
    Phase::Live,
    Phase::Reconnecting,
    Phase::Ended,
];

/// `tool` is the agent session's, when known: "Reading Codex's saved conversation…".
fn recovery_of(
    ticket: &Ticket,
    tool: Option<otaq_core::agent_tools::Tool>,
    before: Option<&BeforeRestart>,
) -> Recovery {
    let Some(before) = before else {
        return Recovery::Loading {
            text: format!(
                "Reading {}\u{2019}s saved conversation\u{2026}",
                tool.map_or("the agent", |t| t.short_name())
            ),
        };
    };
    let tool = match before {
        BeforeRestart::Available { tool, .. } => Some(*tool),
        BeforeRestart::NotAvailable { tool, .. } => *tool,
    }
    .or(tool);
    match recovery::view(ticket, before) {
        RecoveryView::BeforeRestart {
            session,
            lines,
            ended,
        } => Recovery::BeforeRestart {
            title: "Before the restart",
            source: format!(
                "from {}\u{2019}s saved conversation \u{b7} read-only",
                tool.map_or("the agent", |t| t.short_name())
            ),
            session,
            lines: lines
                .into_iter()
                .map(|l| RecoveryLine {
                    time: l.time,
                    who: l.who,
                    text: l.text,
                    action: l.action,
                })
                .collect(),
            ended,
            resume: "Resume",
        },
        RecoveryView::NotAvailable { text } => Recovery::NotAvailable {
            label: "Conversation no longer available",
            text,
            retry: "Retry",
            scratch: "Start from scratch…",
        },
    }
}

/// The view from what the service said: the ticket, its sessions, its validation rounds, its check's side
/// ([`session_tab::check_side`]), its project's name and, for an Interrupted ticket whose session is gone, its tool's
/// saved conversation. `keys`: the user's keys, read as they work inside a terminal (KEY-08), for "leave terminal"
/// and Find.
pub fn view_with(
    ticket: &Ticket,
    sessions: &[SessionInfo],
    rounds: &[ValidationRecord],
    check: Option<&CheckSide>,
    project_name: &str,
    before: Option<&BeforeRestart>,
    keys: &KeyLabels,
) -> View {
    let session = session_tab::agent_session(sessions);
    let running = session.is_some_and(SessionInfo::is_running);
    let recovering = recovery::shows_recovery(ticket, running);
    let flags = &ticket.flags;
    let strips = PHASES
        .iter()
        .flat_map(|phase| [false, true].map(|focused| (phase.clone(), focused)))
        .map(|(phase, focused)| Strip {
            state: if recovering {
                strip::INTERRUPTED
            } else {
                strip::tab_state(&phase, ticket.status, flags.your_turn)
            },
            hint: if recovering {
                strip::hint(
                    &Phase::Ended,
                    false,
                    ticket.status,
                    false,
                    flags.messages_waiting,
                )
            } else {
                strip::hint(
                    &phase,
                    focused,
                    ticket.status,
                    flags.your_turn,
                    flags.messages_waiting,
                )
            },
            phase,
            focused,
        })
        .collect();
    let work = ticket.work.clone().unwrap_or_default();
    let ended = session
        .filter(|s| s.state == SessionState::Ended)
        .and_then(|s| {
            s.ended_at
                .map(|at| strip::end_marker(s.started_at, at, s.end_reason.as_deref()))
        });
    let background = ticket
        .signal
        .as_ref()
        .filter(|_| ticket.status == otaq_core::domain::Status::WorkingInBackground)
        .and_then(|s| s.text.as_deref())
        .map(|t| t.strip_prefix("background: ").unwrap_or(t).to_owned());
    let check = check.and_then(|side| {
        let session = sessions.iter().find(|s| s.id == side.session)?;
        Some(check_of(ticket, side, session, rounds))
    });
    let agent = session.map(|s| {
        s.agent
            .as_ref()
            .map_or_else(|| "Agent".to_owned(), |a| a.name.clone())
    });
    View {
        key: ticket.key.to_string(),
        session: session.map(|s| s.id.clone()),
        label: format!("{} · agent terminal", ticket.key),
        no_session: strip::no_session(&ticket.key),
        strips,
        path: work.worktree.as_deref().map(strip::short_path),
        port: work.agent_port.map(strip::agent_port),
        attach_command: strip::attach_command(&ticket.key),
        attach_short: "attach",
        copy_attach: "Copy attach command",
        waiting: strip::waiting_label(flags.messages_waiting).map(|label| Waiting {
            label,
            deliver: "Deliver now",
        }),
        instructions: Instructions {
            label: strip::instructions_link(project_name),
            project: ticket.key.project.to_string(),
        },
        leave: Tooltip {
            text: "leave terminal".to_owned(),
            keys: keys.in_terminal(KeyAction::Terminals),
        },
        smaller: "Smaller text",
        bigger: "Larger text",
        find: Tooltip {
            text: "Find in scrollback".to_owned(),
            keys: keys.in_terminal(KeyAction::FindInTerminal),
        },
        background,
        narrow_below: session_tab::NARROW_BELOW,
        ended,
        recovery: recovering.then(|| recovery_of(ticket, session.and_then(|s| s.tool), before)),
        line: session.zip(agent.as_deref()).map(|(s, agent)| {
            session_tab::agent_line(agent, s.tool, model_of(s), work.worktree.as_deref())
        }),
        typing: agent
            .filter(|_| running && !recovering)
            .map(|agent| session_tab::typing_goes_to(&agent, &ticket.key)),
        check,
    }
}

async fn ticket(app: &App, key: &str) -> Result<Option<Ticket>, RequestError> {
    match app
        .service
        .send(Request::TicketGet(KeyParams {
            key: key.to_owned(),
        }))
        .await
    {
        Ok(Reply::Ticket(t)) => Ok(Some(*t)),
        Ok(other) => Err(unexpected("ticket.get", &other)),
        Err(e) if e.not_found => Ok(None),
        Err(e) => Err(e),
    }
}

async fn sessions(app: &App, key: &str) -> Result<Vec<SessionInfo>, RequestError> {
    match app
        .service
        .send(Request::SessionList(SessionListParams {
            key: Some(key.to_owned()),
        }))
        .await?
    {
        Reply::Sessions(list) => Ok(list),
        other => Err(unexpected("session.list", &other)),
    }
}

async fn rounds(app: &App, key: &str) -> Result<Vec<ValidationRecord>, RequestError> {
    match app
        .service
        .send(Request::TicketValidations(KeyParams {
            key: key.to_owned(),
        }))
        .await?
    {
        Reply::Validations(list) => Ok(list),
        other => Err(unexpected("ticket.validations", &other)),
    }
}

async fn project_name(app: &App, key: &TicketKey) -> Result<String, RequestError> {
    let project = key.project.to_string();
    match app.service.send(Request::ProjectList).await? {
        Reply::Projects(list) => Ok(list
            .into_iter()
            .find(|p: &ProjectInfo| p.key == project)
            .map_or(project, |p| p.name)),
        other => Err(unexpected("project.list", &other)),
    }
}

pub async fn view(app: &App, params: Params) -> Result<Load, RequestError> {
    let gone = format!("{} is gone.", params.key);
    let Some(ticket) = ticket(app, &params.key).await? else {
        return Ok(Load { view: None, gone });
    };
    let sessions = sessions(app, &params.key).await?;
    let name = project_name(app, &ticket.key).await?;
    // The check's side, its rounds (its round's outcome) and its stage (its runs, the flow's names), once one ran.
    let (rounds, check) = if session_tab::check_session(&sessions).is_some() {
        let agents =
            super::editor::agent_facts(app, super::editor::of_ticket(&params.key)).await;
        let stages = agents
            .as_ref()
            .map(|a| {
                a.path
                    .iter()
                    .map(|s| (s.stage_id.clone(), s.stage.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let runs = agents.as_ref().map_or(&[][..], |a| a.runs.as_slice());
        (
            rounds(app, &params.key).await?,
            session_tab::check_side(&ticket, &sessions, runs, &stages),
        )
    } else {
        (Vec::new(), None)
    };
    let running = session_tab::agent_session(&sessions).is_some_and(SessionInfo::is_running);
    let before = if recovery::shows_recovery(&ticket, running) {
        match app
            .service
            .send(Request::SessionBeforeRestart(BeforeRestartParams {
                key: params.key.clone(),
                limit: None,
            }))
            .await?
        {
            Reply::BeforeRestart(before) => Some(*before),
            other => return Err(unexpected("session.before_restart", &other)),
        }
    } else {
        None
    };
    let keys = super::keymap::labels(app).await;
    Ok(Load {
        view: Some(view_with(
            &ticket,
            &sessions,
            &rounds,
            check.as_ref(),
            &name,
            before.as_ref(),
            &keys,
        )),
        gone,
    })
}

/// Deliver now (SES-06): the waiting messages are typed into the session; with unsent text in its input line the
/// user is asked first (SES-07, d0048), and "Clear it and send" asks again with `clear`.
pub async fn deliver_now(app: &App, params: DeliverParams) -> Result<Sent, RequestError> {
    let key: TicketKey = params
        .key
        .parse()
        .map_err(|_| RequestError::plain(format!("not a ticket key: {}", params.key)))?;
    let choice = params.clear.then_some(Choice::Clear);
    let Some(request) = strip::deliver_now_request(&key, choice) else {
        return Ok(Sent::Done);
    };
    match app.service.send(request).await {
        Ok(Reply::Delivery(Delivery::NeedsConfirmation { unclearable_by, .. })) => {
            let waiting = ticket(app, &params.key)
                .await?
                .map_or(1, |t| t.flags.messages_waiting);
            Ok(Sent::Ask(Unsent::of(
                &UnsentPrompt::deliver_now(key, None, waiting).unclearable_by(unclearable_by),
            )))
        }
        Ok(_) => Ok(Sent::Done),
        Err(e) => {
            app.notices.toast(NewToast::new(
                ToastLevel::Error,
                format!("{key}\u{2019}s waiting messages weren\u{2019}t delivered"),
                Some(e.message.clone()),
            ));
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::tickets::{AgentSignal, Work};
    use otaq_core::domain::{ValidationOutcome, ValidationTrigger};
    use otaq_core::board::projects::AddProject;
    use otaq_core::board::tickets::NewTicketInput;
    use otaq_core::git::testing::TempRepos;
    use otaq_core::recovery::TranscriptEntry;
    use serde_json::json;

    use otaq_view::key_labels::fixtures as keys;
    use otaq_view::keymap::Platform;

    use super::super::request as table;
    use super::*;
    use otaq_core::domain::Status;
    use crate::service::testing::{Harness, connected};

    /// [`view_with`] on the base map's macOS keys.
    fn view_of(
        ticket: &Ticket,
        sessions: &[SessionInfo],
        rounds: &[ValidationRecord],
        project_name: &str,
        before: Option<&BeforeRestart>,
    ) -> View {
        let check = session_tab::check_side(ticket, sessions, &[], &Default::default());
        view_with(
            ticket,
            sessions,
            rounds,
            check.as_ref(),
            project_name,
            before,
            &keys::base(),
        )
    }

    /// KEY-08: "leave terminal" and Find name the user's keys as they work inside a terminal: ⌃` and ⌘F on macOS,
    /// Ctrl+` and Ctrl+Shift+F in a Windows/Linux terminal, a recorded key, none once removed.
    #[test]
    fn leave_and_find_show_the_users_keys_in_a_terminal() {
        let t = ticket(Status::Working);
        let tips = |labels: &KeyLabels| {
            let v = view_with(&t, &[], &[], None, "shop-web", None, labels);
            (v.leave.keys, v.find.keys)
        };
        let own = |a: &str, b: &str| (Some(a.to_owned()), Some(b.to_owned()));
        assert_eq!(tips(&keys::base()), own("⌃`", "⌘F"));
        assert_eq!(
            tips(&KeyLabels::base(Platform::WinLinux)),
            own("Ctrl+`", "Ctrl+Shift+F")
        );
        assert_eq!(
            tips(&keys::replaced("find_in_terminal", None, 0, "⌘G")),
            own("⌃`", "⌘G")
        );
        let removed = keys::with(true, &[("terminals", None, 0, None), ("find_in_terminal", None, 0, None)]);
        assert_eq!(tips(&removed), (None, None));
        assert_eq!(tips(&keys::letters_off()), own("⌃`", "⌘F"));
    }

    fn session(id: &str, state: SessionState, started_at: i64) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            kind: otaq_core::session::SessionKind::Agent,
            project: "SHOP".into(),
            ticket: Some("SHOP-19".parse().unwrap()),
            state,
            started_at,
            ended_at: (state == SessionState::Ended).then_some(started_at + 41 * 60_000),
            end_reason: (state == SessionState::Ended).then(|| "exit 0".to_owned()),
            tool: None,
            tool_session_id: None,
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            model_ran: None,
            size: None,
        }
    }

    fn ticket(status: Status) -> Ticket {
        let mut t = otaq_view::view_models::card_fixtures::ticket("SHOP-19", "Price table", status);
        t.work = Some(Work {
            worktree: Some("/Users/me/.data/wt/shop-web/SHOP-19".into()),
            agent_port: Some(4019),
            ..Work::default()
        });
        t
    }

    fn strip(view: &View, phase: Phase, focused: bool) -> &Strip {
        view.strips
            .iter()
            .find(|s| s.phase == phase && s.focused == focused)
            .unwrap()
    }

    #[test]
    fn the_tab_follows_the_running_agent_session() {
        let mut shell = session("shell", SessionState::Running, 30);
        shell.kind = otaq_core::session::SessionKind::Shell;
        let list = [
            session("old", SessionState::Ended, 10),
            shell,
            session("new", SessionState::Ended, 20),
        ];
        assert_eq!(session_tab::agent_session(&list).unwrap().id, "new", "the newest");
        let mut list = list.to_vec();
        list.push(session("live", SessionState::Running, 5));
        assert_eq!(session_tab::agent_session(&list).unwrap().id, "live", "the running one");
        assert_eq!(session_tab::agent_session(&[]), None);
    }

    #[test]
    fn the_strip_has_every_phase_and_focus() {
        let mut t = ticket(Status::Working);
        t.flags.messages_waiting = 1;
        let view = view_of(
            &t,
            &[session("s1", SessionState::Running, 0)],
            &[],
            "shop-web",
            None,
        );
        assert_eq!(view.session.as_deref(), Some("s1"));
        assert_eq!(view.label, "SHOP-19 · agent terminal");
        // The line over the terminal (SHT-04): who, on what, where; typing goes to it while it runs.
        assert_eq!(
            view.line.as_deref(),
            Some("Agent \u{b7} /Users/me/.data/wt/shop-web/SHOP-19")
        );
        let typing = view.typing.as_ref().unwrap();
        assert_eq!(typing.focused, "Typing goes to Agent \u{b7} SHOP-19");
        assert_eq!(typing.unfocused, "Click to type to Agent \u{b7} SHOP-19");
        assert_eq!(view.strips.len(), 10);
        assert_eq!(strip(&view, Phase::Live, false).state.label, "Live");
        assert_eq!(
            strip(&view, Phase::Reconnecting, true).state.label,
            "Reconnecting…"
        );
        assert_eq!(view.path.as_deref(), Some("…/wt/shop-web/SHOP-19"));
        assert_eq!(view.port.as_deref(), Some("agent :4019"));
        assert_eq!(view.attach_command, strip::attach_command(&t.key));
        assert_eq!(view.waiting.unwrap().label, "1 message waiting");
        assert_eq!(view.instructions.label, "Instructions: shop-web");
        assert_eq!(view.instructions.project, "SHOP");
        assert_eq!(view.ended, None);
        assert_eq!(view.recovery, None);
        assert_eq!(view.background, None);
    }

    #[test]
    fn an_ended_session_has_its_marker_and_background_work_its_tasks() {
        let view = view_of(
            &ticket(Status::WaitingForYou),
            &[session("s1", SessionState::Ended, 0)],
            &[],
            "shop-web",
            None,
        );
        let ended = view.ended.unwrap();
        assert!(ended.title.ends_with("41 min"), "{}", ended.title);
        assert_eq!(ended.text, "The agent’s session finished.");

        let mut t = ticket(Status::WorkingInBackground);
        t.signal = Some(AgentSignal {
            text: Some("background: test run, 1 helper agent".into()),
            ..AgentSignal::default()
        });
        let view = view_of(
            &t,
            &[session("s1", SessionState::Running, 0)],
            &[],
            "shop-web",
            None,
        );
        assert_eq!(view.background.as_deref(), Some("test run, 1 helper agent"));
        assert_eq!(
            strip(&view, Phase::Live, false).state.label,
            "Waiting on its background work"
        );
    }

    #[test]
    fn an_interrupted_ticket_shows_before_the_restart() {
        let t = ticket(Status::Interrupted);
        let ended = [session("s1", SessionState::Ended, 0)];
        let loading = view_of(&t, &ended, &[], "shop-web", None);
        assert!(matches!(loading.recovery, Some(Recovery::Loading { .. })));
        assert_eq!(strip(&loading, Phase::Ended, false).state.label, "Interrupted");
        let before = BeforeRestart::Available {
            tool: otaq_core::agent_tools::Tool::Claude,
            tool_session_id: "3be94c10-aaaa".into(),
            entries: vec![TranscriptEntry::Action {
                tool: "Bash".into(),
                summary: "pnpm build".into(),
                at: None,
            }],
        };
        let view = view_of(&t, &ended, &[], "shop-web", Some(&before));
        let Some(Recovery::BeforeRestart {
            session: id, lines, ..
        }) = view.recovery
        else {
            panic!("{:?}", view.recovery);
        };
        assert_eq!(id, "session 3be94c10");
        assert_eq!(lines[0].who, "Bash");
        assert!(lines[0].action);
        let gone = BeforeRestart::NotAvailable {
            tool: None,
            tool_session_id: None,
        };
        assert!(matches!(
            view_of(&t, &ended, &[], "shop-web", Some(&gone)).recovery,
            Some(Recovery::NotAvailable { .. })
        ));
        // Resumed: the live session shows again.
        let running = [session("s2", SessionState::Running, 5)];
        assert_eq!(view_of(&t, &running, &[], "shop-web", None).recovery, None);
    }

    fn validator(id: &str, state: SessionState, started_at: i64) -> SessionInfo {
        SessionInfo {
            kind: otaq_core::session::SessionKind::Validator,
            tool: Some(otaq_core::agent_tools::Tool::Codex),
            model: Some("gpt-5.5".into()),
            agent: Some(otaq_core::session::SessionAgent {
                id: "validator".into(),
                name: "Validator".into(),
            }),
            ..session(id, state, started_at)
        }
    }

    fn round(round: u32, session: &str, outcome: Option<ValidationOutcome>) -> ValidationRecord {
        ValidationRecord {
            round,
            trigger: ValidationTrigger::Agent,
            outcome,
            started_at: i64::from(round),
            ended_at: None,
            duration_ms: None,
            session: Some(session.into()),
            report: None,
            notes: Vec::new(),
            tokens: None,
        }
    }

    #[test]
    fn the_check_side_shows_once_a_check_ran() {
        let agent = session("a1", SessionState::Running, 0);
        let before = view_of(&ticket(Status::Working), std::slice::from_ref(&agent), &[], "shop-web", None);
        assert_eq!(before.check, None);

        // Validating: the validator runs, takes typing and copies its own attach command.
        let sessions = [agent.clone(), validator("v1", SessionState::Running, 5)];
        let rounds = [round(1, "v1", None)];
        let view = view_of(&ticket(Status::Checking), &sessions, &rounds, "shop-web", None);
        let v = view.check.as_ref().unwrap();
        assert_eq!(v.session, "v1");
        assert_eq!(v.label, "Validator");
        assert_eq!(v.terminal_label, "SHOP-19 \u{b7} Validator terminal");
        assert_eq!(
            v.line,
            "Validator \u{b7} Codex \u{b7} gpt-5.5 \u{b7} /Users/me/.data/wt/shop-web/SHOP-19"
        );
        assert_eq!(
            v.typing.as_ref().map(|t| t.focused.as_str()),
            Some("Typing goes to Validator \u{b7} SHOP-19")
        );
        assert!(!v.read_only);
        assert_eq!(v.attach_command, format!("{} --stage validate", strip::attach_command(&view_key())));
        assert_eq!(strip(&view, Phase::Live, false).state.label, "Live");
        let live = v.strips.iter().find(|s| s.phase == Phase::Live && s.focused).unwrap();
        assert_eq!(live.hint.as_ref().unwrap().text, "Typing goes to the validator");

        // Needs you · validator: its strip says it waits for you.
        let view = view_of(&ticket(Status::CheckerNeedsYou), &sessions, &rounds, "shop-web", None);
        let v = view.check.unwrap();
        let live = v.strips.iter().find(|s| s.phase == Phase::Live && !s.focused).unwrap();
        assert_eq!(live.state.label, "Waiting for you");
    }

    #[test]
    fn an_ended_check_stays_readable_until_the_next_round() {
        let sessions = [
            session("a1", SessionState::Running, 0),
            validator("v1", SessionState::Ended, 5),
            validator("v2", SessionState::Ended, 9),
        ];
        let rounds = [
            round(1, "v1", Some(ValidationOutcome::Fail)),
            round(2, "v2", Some(ValidationOutcome::Fail)),
        ];
        let view = view_of(&ticket(Status::CheckFailed), &sessions, &rounds, "shop-web", None);
        let v = view.check.unwrap();
        assert_eq!(v.session, "v2", "the newest");
        assert!(v.read_only);
        assert_eq!(v.typing, None);
        assert_eq!(v.attach_command, strip::attach_command(&view_key()), "only a running one attaches");
        let ended = v.strips.iter().find(|s| s.phase == Phase::Ended && !s.focused).unwrap();
        assert_eq!(ended.state.label, "Ended · failed");
        assert_eq!(ended.hint.as_ref().unwrap().text, "Read-only · this validator has ended");

        // The next round started: the new validator shows, live again.
        let mut next = sessions.to_vec();
        next.push(validator("v3", SessionState::Running, 12));
        let mut rounds = rounds.to_vec();
        rounds.push(round(3, "v3", None));
        let v = view_of(&ticket(Status::Checking), &next, &rounds, "shop-web", None)
            .check
            .unwrap();
        assert_eq!((v.session.as_str(), v.read_only), ("v3", false));

        // A restart ended it: its own state.
        let rounds = [round(1, "v1", Some(ValidationOutcome::Interrupted))];
        let v = view_of(&ticket(Status::Interrupted), &sessions[..2], &rounds, "shop-web", None)
            .check
            .unwrap();
        let ended = v.strips.iter().find(|s| s.phase == Phase::Ended && !s.focused).unwrap();
        assert_eq!(ended.state.label, "Ended by restart");
    }

    fn view_key() -> TicketKey {
        "SHOP-19".parse().unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_tab_reads_a_real_ticket() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        let path = repos.repo("shop-web", 1);
        let project = match h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }))
            .await
            .unwrap()
        {
            Reply::ProjectAdded { project: p, .. } => p,
            other => panic!("{other:?}"),
        };
        let created = match h
            .app
            .service
            .send(Request::TicketCreate(NewTicketInput {
                project: project.key.clone(),
                kind: Default::default(),
                title: "Price table".into(),
                body: String::new(),
                priority: None,
                epic: None,
                base_branch: None,
                external_id: None,
                first_comment: None,
            }))
            .await
            .unwrap()
        {
            Reply::Ticket(t) => t,
            other => panic!("{other:?}"),
        };
        let key = created.key.to_string();

        let load = table(&h.app, "terminal_tab.view", json!({ "key": key }))
            .await
            .unwrap();
        assert_eq!(load["view"]["session"], Value::Null);
        assert_eq!(
            load["view"]["instructions"]["label"],
            format!("Instructions: {}", project.name)
        );
        assert_eq!(
            load["view"]["no_session"],
            format!("{key} has no agent session yet. It starts when the ticket goes to Dev.")
        );
        // No session to type into: refused (and shown as a toast).
        let sent = table(&h.app, "terminal_tab.deliver_now", json!({ "key": key })).await;
        assert!(sent.unwrap_err().not_found);
        let gone = table(&h.app, "terminal_tab.view", json!({ "key": "SHOP-99" }))
            .await
            .unwrap();
        assert_eq!(gone["view"], Value::Null);
        assert_eq!(gone["gone"], "SHOP-99 is gone.");
        h.service.stop_all().await;
    }
}
