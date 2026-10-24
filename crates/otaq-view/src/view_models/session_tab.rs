//! The sheet's Session tab (SHT-04; d0022, d0035, d0048, d0063, d0108; canvas `s25-06`): the ticket's terminals under
//! its header, with the switch **Agent · <check's agent> · Run · Shell**. The check's side shows while a check stage
//! runs on the ticket or after one ran (the running check, else the last: "Validator", "Security reviewer"; its
//! terminal attaches with `--stage <id>`); the dots say which side waits for you (turquoise) or runs (green). Over the
//! terminal, the line "Rust developer · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-22" and "Typing goes to Rust
//! developer"; Run's line "pnpm dev · in SHOP-22’s worktree · :4022" with Open URL. Replaces the panel's Terminal, Run
//! and Shell tabs.

use std::collections::BTreeMap;

use otaq_core::agent_tools::Tool;
use otaq_core::board::stage_runs::StageRun;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Status, TicketKey};
use otaq_core::session::{SessionInfo, SessionKind};
use otaq_core::terminals::run::{RunInfo, RunState};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::view_models::agents::TOOL_DEFAULT;
use crate::view_models::sheet::Content;

/// Below this width the terminal's strip drops the path and the port and shortens the attach command.
pub const NARROW_BELOW: f32 = 560.;

/// A part's dot in the switch (SHT-04): turquoise while it waits for you, green while it runs, red when the Run
/// failed to start or its port is taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "SessionDot")]
pub enum Dot {
    Waiting,
    Running,
    Problem,
}

/// One button of the switch: "Agent", "Security reviewer", "Run", "Shell".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SessionPartView")]
pub struct PartView {
    pub content: Content,
    pub label: String,
    pub dot: Option<Dot>,
}

/// The check's side: the ticket's running check session, else its last one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckSide {
    pub session: String,
    /// The check stage's id (`--stage security`) and its name ("Security").
    pub stage_id: Option<String>,
    pub stage: Option<String>,
    /// Its agent's name: the switch's label.
    pub agent: String,
    pub running: bool,
}

/// The switch's label when a check's agent can't be named.
const CHECK: &str = "Check";

/// The ticket's check session: the running one, else the newest.
pub fn check_session(sessions: &[SessionInfo]) -> Option<&SessionInfo> {
    let checks = || sessions.iter().filter(|s| s.kind == SessionKind::Validator);
    checks()
        .find(|s| s.is_running())
        .or_else(|| checks().max_by_key(|s| s.started_at))
}

/// The check's side from the ticket's sessions, its stage runs and the flow's stage names (id → name): the stage the
/// session ran for (the ticket's stage while it runs) and the agent it ran as.
pub fn check_side(
    ticket: &Ticket,
    sessions: &[SessionInfo],
    runs: &[StageRun],
    stages: &BTreeMap<String, String>,
) -> Option<CheckSide> {
    let session = check_session(sessions)?;
    let running = session.is_running();
    let run = runs
        .iter()
        .rev()
        .find(|r| r.session.as_deref() == Some(session.id.as_str()));
    let stage_id = run
        .map(|r| r.stage.clone())
        .or_else(|| ticket.place.stage().filter(|_| running).map(str::to_owned));
    let agent = session
        .agent
        .as_ref()
        .map(|a| a.name.clone())
        .or_else(|| run.and_then(|r| r.agent.clone()))
        .unwrap_or_else(|| CHECK.to_owned());
    Some(CheckSide {
        session: session.id.clone(),
        stage: stage_id.as_ref().and_then(|id| stages.get(id).cloned()),
        stage_id,
        agent,
        running,
    })
}

/// Whether the ticket is in a check round now: its check is the side to look at.
pub fn checking(status: Status) -> bool {
    matches!(status, Status::Checking | Status::CheckerNeedsYou)
}

/// The part Session lands on (opening it, ⌘J, a notification): the check while one runs or needs you, else the agent.
pub fn lands_on(status: Status) -> Content {
    if checking(status) {
        Content::Check
    } else {
        Content::Agent
    }
}

/// The ticket's agent session: the running one, else the newest.
pub fn agent_session(sessions: &[SessionInfo]) -> Option<&SessionInfo> {
    let agents = || sessions.iter().filter(|s| s.kind == SessionKind::Agent);
    agents()
        .find(|s| s.is_running())
        .or_else(|| agents().max_by_key(|s| s.started_at))
}

/// What the switch is made of.
pub struct Facts<'a> {
    pub ticket: &'a Ticket,
    pub sessions: &'a [SessionInfo],
    pub check: Option<&'a CheckSide>,
    pub run: Option<&'a RunInfo>,
}

/// Run's dot: green while it runs, red when it failed to start or its port is taken.
pub fn run_dot(run: Option<&RunInfo>) -> Option<Dot> {
    match run?.state {
        state if state.is_live() => Some(Dot::Running),
        RunState::FailedToStart | RunState::PortInUse => Some(Dot::Problem),
        _ => None,
    }
}

/// The switch (SHT-04): Agent, the check's agent once a check ran, Run, Shell, each with its dot.
pub fn parts(f: &Facts) -> Vec<PartView> {
    let ticket = f.ticket;
    let checking = checking(ticket.status);
    let agent_dot = agent_session(f.sessions)
        .filter(|s| s.is_running() && !checking)
        .map(|_| {
            if ticket.shows_needs_you() {
                Dot::Waiting
            } else {
                Dot::Running
            }
        });
    let check = f.check.map(|c| PartView {
        content: Content::Check,
        label: c.agent.clone(),
        dot: c
            .running
            .then_some(if ticket.status == Status::CheckerNeedsYou {
                Dot::Waiting
            } else {
                Dot::Running
            }),
    });
    let shell_dot = f
        .sessions
        .iter()
        .any(|s| s.kind == SessionKind::Shell && s.is_running())
        .then_some(Dot::Running);
    [PartView {
        content: Content::Agent,
        label: "Agent".to_owned(),
        dot: agent_dot,
    }]
    .into_iter()
    .chain(check)
    .chain([
        PartView {
            content: Content::Run,
            label: "Run".to_owned(),
            dot: run_dot(f.run),
        },
        PartView {
            content: Content::Shell,
            label: "Shell".to_owned(),
            dot: shell_dot,
        },
    ])
    .collect()
}

/// The Session tab's own dot (`s25-03`): turquoise when a side waits for you, green while one works.
pub fn tab_dot(parts: &[PartView]) -> Option<Dot> {
    let dots = || {
        parts
            .iter()
            .filter(|p| p.content != Content::Run)
            .filter_map(|p| p.dot)
    };
    dots()
        .find(|d| *d == Dot::Waiting)
        .or_else(|| dots().find(|d| *d == Dot::Running))
}

/// "Rust developer · Codex · gpt-5.5 · ~/.otaq/wt/shop-web/SHOP-22": who the terminal is, on what, where.
pub fn agent_line(
    agent: &str,
    tool: Option<Tool>,
    model: Option<&str>,
    worktree: Option<&str>,
) -> String {
    let model = model.filter(|m| !m.is_empty()).unwrap_or(TOOL_DEFAULT);
    let tool = tool.map(|t| format!("{} \u{b7} {model}", t.display_name()));
    [
        Some(agent.to_owned()),
        tool,
        worktree.map(crate::shell::tilde),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
}

/// Who typing reaches, with the ticket's key so a wrong ticket shows (finding 55 of the 2026-10-24 dogfood report). The
/// page shows `focused` only while that terminal has the keyboard, else `unfocused`. `SessionTyping` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SessionTyping")]
pub struct Typing {
    /// "Typing goes to Griller · DEMO-15".
    pub focused: String,
    /// "Click to type to Griller · DEMO-15".
    pub unfocused: String,
}

/// Who typing reaches while the agent's terminal takes input.
pub fn typing_goes_to(agent: &str, key: &TicketKey) -> Typing {
    Typing {
        focused: format!("Typing goes to {agent} \u{b7} {key}"),
        unfocused: format!("Click to type to {agent} \u{b7} {key}"),
    }
}

/// Run's line: "pnpm dev · in SHOP-22’s worktree · :4022"; none before a run command is set.
pub fn run_line(command: &str, key: &TicketKey, port: Option<u16>) -> Option<String> {
    let command = command.trim();
    if command.is_empty() {
        return None;
    }
    let port = port.map(|p| format!(" \u{b7} :{p}")).unwrap_or_default();
    Some(format!("{command} \u{b7} in {key}\u{2019}s worktree{port}"))
}

/// "Open http://localhost:4022": Run's Open URL once its port answers, only when the project sets one (RUN-03).
pub fn open_url(run: &RunInfo) -> Option<(String, String)> {
    let url = run.url().filter(|_| run.state == RunState::Running)?;
    Some((format!("Open {url}"), url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::Place;
    use otaq_core::session::{SessionAgent, SessionState};

    fn session(id: &str, kind: SessionKind, running: bool, agent: Option<&str>) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            kind,
            project: "SHOP".into(),
            ticket: Some("SHOP-30".parse().unwrap()),
            state: if running {
                SessionState::Running
            } else {
                SessionState::Ended
            },
            started_at: 10,
            ended_at: (!running).then_some(20),
            end_reason: None,
            tool: Some(Tool::Codex),
            tool_session_id: None,
            model: Some("gpt-5.5".into()),
            agent: agent.map(|name| SessionAgent {
                id: name.to_lowercase(),
                name: name.into(),
            }),
            effort: None,
            fallback: None,
            model_ran: None,
            size: None,
        }
    }

    fn run_of(stage: &str, session: &str, agent: &str) -> StageRun {
        StageRun {
            stage: stage.into(),
            kind: "check".into(),
            session: Some(session.into()),
            agent: Some(agent.into()),
            model: None,
            round: Some(1),
            result: None,
            summary: None,
            started_at: 10,
            ended_at: None,
        }
    }

    fn stages() -> BTreeMap<String, String> {
        [
            ("validate", "Validate"),
            ("security", "Security"),
            ("dev", "Dev"),
        ]
        .into_iter()
        .map(|(id, name)| (id.to_owned(), name.to_owned()))
        .collect()
    }

    fn labels(parts: &[PartView]) -> Vec<(&str, Option<Dot>)> {
        parts.iter().map(|p| (p.label.as_str(), p.dot)).collect()
    }

    #[test]
    fn shop_30_validating_shows_the_validator_running_and_lands_on_it() {
        let mut t = ticket("SHOP-30", "Order history page", Status::Checking);
        t.place = Place::InStage("validate".into());
        let sessions = [
            session("dev", SessionKind::Agent, false, Some("Rust developer")),
            session("val", SessionKind::Validator, true, Some("Validator")),
        ];
        let check = check_side(
            &t,
            &sessions,
            &[run_of("validate", "val", "Validator")],
            &stages(),
        )
        .expect("a check ran");
        assert_eq!(check.stage_id.as_deref(), Some("validate"));
        assert_eq!(check.stage.as_deref(), Some("Validate"));
        assert!(check.running);
        let parts = parts(&Facts {
            ticket: &t,
            sessions: &sessions,
            check: Some(&check),
            run: None,
        });
        assert_eq!(
            labels(&parts),
            [
                ("Agent", None),
                ("Validator", Some(Dot::Running)),
                ("Run", None),
                ("Shell", None)
            ]
        );
        assert_eq!(parts[1].content, Content::Check);
        assert_eq!(lands_on(t.status), Content::Check);
        assert_eq!(tab_dot(&parts), Some(Dot::Running));
        // The validator asks for you: turquoise.
        t.status = Status::CheckerNeedsYou;
        let waiting = super::parts(&Facts {
            ticket: &t,
            sessions: &sessions,
            check: Some(&check),
            run: None,
        });
        assert_eq!(waiting[1].dot, Some(Dot::Waiting));
        assert_eq!(tab_dot(&waiting), Some(Dot::Waiting));
    }

    #[test]
    fn shop_19_in_security_names_the_security_reviewer() {
        let mut t = ticket("SHOP-19", "Price table", Status::Checking);
        t.place = Place::InStage("security".into());
        // Its session started before the stage run was read: the ticket's stage names it.
        let sessions = [session(
            "sec",
            SessionKind::Validator,
            true,
            Some("Security reviewer"),
        )];
        let check = check_side(&t, &sessions, &[], &stages()).expect("a check runs");
        assert_eq!(check.agent, "Security reviewer");
        assert_eq!(check.stage.as_deref(), Some("Security"));
        let parts = parts(&Facts {
            ticket: &t,
            sessions: &sessions,
            check: Some(&check),
            run: None,
        });
        assert_eq!(
            parts.iter().map(|p| p.label.as_str()).collect::<Vec<_>>(),
            ["Agent", "Security reviewer", "Run", "Shell"]
        );
    }

    #[test]
    fn the_last_check_stays_after_it_ended_and_the_agent_side_is_back() {
        let mut t = ticket("SHOP-29", "Coupon field", Status::Working);
        t.place = Place::InStage("dev".into());
        let sessions = [
            session("sec", SessionKind::Validator, false, None),
            session("dev2", SessionKind::Agent, true, Some("Rust developer")),
        ];
        let runs = [run_of("security", "sec", "Security reviewer")];
        let check = check_side(&t, &sessions, &runs, &stages()).expect("a check ran");
        assert_eq!(check.agent, "Security reviewer", "named by its stage run");
        assert_eq!(check.stage_id.as_deref(), Some("security"));
        assert!(!check.running);
        let parts = parts(&Facts {
            ticket: &t,
            sessions: &sessions,
            check: Some(&check),
            run: None,
        });
        assert_eq!(parts[0].dot, Some(Dot::Running));
        assert_eq!(parts[1].dot, None);
        assert_eq!(lands_on(t.status), Content::Agent);
    }

    #[test]
    fn no_check_no_check_side_and_waiting_agents_are_turquoise() {
        let t = ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou);
        let sessions = [session(
            "dev",
            SessionKind::Agent,
            true,
            Some("Rust developer"),
        )];
        assert_eq!(check_side(&t, &sessions, &[], &stages()), None);
        let parts = parts(&Facts {
            ticket: &t,
            sessions: &sessions,
            check: None,
            run: None,
        });
        assert_eq!(
            labels(&parts),
            [
                ("Agent", Some(Dot::Waiting)),
                ("Run", None),
                ("Shell", None)
            ]
        );
    }

    fn run(state: RunState) -> RunInfo {
        RunInfo {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            key: "SHOP-22".parse().unwrap(),
            title: "Fix flaky login redirect".into(),
            state,
            port: Some(4022),
            command: "pnpm dev".into(),
            open_url: Some("http://localhost:$PORT".into()),
            session: Some("run".into()),
            started_at: Some(1),
            ended_at: None,
            exit_code: None,
            stopped_by: None,
        }
    }

    #[test]
    fn runs_line_its_open_url_and_its_dot() {
        let key: TicketKey = "SHOP-22".parse().unwrap();
        assert_eq!(
            run_line("pnpm dev", &key, Some(4022)).as_deref(),
            Some("pnpm dev \u{b7} in SHOP-22\u{2019}s worktree \u{b7} :4022")
        );
        assert_eq!(
            run_line("cargo run", &key, None).as_deref(),
            Some("cargo run \u{b7} in SHOP-22\u{2019}s worktree")
        );
        assert_eq!(run_line("  ", &key, Some(4022)), None);
        let running = run(RunState::Running);
        assert_eq!(
            open_url(&running),
            Some((
                "Open http://localhost:4022".to_owned(),
                "http://localhost:4022".to_owned()
            ))
        );
        assert_eq!(open_url(&run(RunState::Stopped)), None);
        assert_eq!(
            open_url(&run(RunState::Starting)),
            None,
            "it waits for the port"
        );
        assert_eq!(
            open_url(&RunInfo {
                open_url: None,
                ..running.clone()
            }),
            None
        );
        assert_eq!(run_dot(Some(&running)), Some(Dot::Running));
        assert_eq!(run_dot(Some(&run(RunState::PortInUse))), Some(Dot::Problem));
        assert_eq!(run_dot(Some(&run(RunState::Stopped))), None);
        assert_eq!(run_dot(None), None);
        // Run's dot is its own: the tab's dot is the agents'.
        let t = ticket("SHOP-22", "Fix flaky login redirect", Status::Working);
        let parts = parts(&Facts {
            ticket: &t,
            sessions: &[session("sh", SessionKind::Shell, true, None)],
            check: None,
            run: Some(&running),
        });
        assert_eq!(
            labels(&parts),
            [
                ("Agent", None),
                ("Run", Some(Dot::Running)),
                ("Shell", Some(Dot::Running))
            ]
        );
    }

    #[test]
    fn the_line_names_the_agent_its_tool_model_and_worktree() {
        assert_eq!(
            agent_line(
                "Rust developer",
                Some(Tool::Codex),
                Some("gpt-5.5"),
                Some("/data/wt/shop-web/SHOP-22")
            ),
            "Rust developer \u{b7} Codex \u{b7} gpt-5.5 \u{b7} /data/wt/shop-web/SHOP-22"
        );
        assert_eq!(
            agent_line("Validator", Some(Tool::Claude), None, None),
            "Validator \u{b7} Claude Code \u{b7} tool default"
        );
        assert_eq!(agent_line("Griller", None, None, None), "Griller");
    }

    /// Finding 55: the label names the ticket, and says "Typing goes to" only while that terminal has the keyboard.
    #[test]
    fn typing_names_the_agent_and_the_ticket_focused_or_not() {
        let typing = typing_goes_to("Griller", &"DEMO-15".parse().unwrap());
        assert_eq!(typing.focused, "Typing goes to Griller \u{b7} DEMO-15");
        assert_eq!(typing.unfocused, "Click to type to Griller \u{b7} DEMO-15");
    }
}
