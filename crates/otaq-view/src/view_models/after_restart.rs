//! What the app says when it connects (REC-06; d0045, d0022): after a reboot, a crash or a service restart the service
//! started again and its startup reconcile stopped the agents that were working, so the app opens with one dialog,
//! "3 agents were stopped by a restart". After a normal reopen, with the service still running, only a quiet
//! "Reconnected".
//!
//! [`summary`] is that dialog: one row per stopped ticket and per conversation with Noyan, ticked; a row whose tool
//! can't run now (signed out, not installed, at its limit) says why and stays unticked, so nothing ever resumes on a
//! tool that can't run (there is no fallback to another tool). "Resume 3" resumes the ticked rows; each row then says
//! what its Resume did (resumed, or couldn't, with Retry and Start from scratch). Nothing resumes without a click
//! (d0045). A ticket that was checking starts its check again; a ticket that was asking you keeps its question, which
//! the dialog names below the rows. Closed, the stopped tickets wait in Needs you, red with Resume.

use std::collections::{BTreeMap, BTreeSet};

use otaq_core::agent_tools::connected::AgentToolState;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::project_agents::{AgentState, ProjectAgent};
use otaq_core::protocol::ServiceStatus;
use otaq_core::recovery::StartupReport;
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::tool_problems::{self, Problem, Tools, tool_word};

/// What an interrupted ticket's session ran on: its tool and named agent (`None` before named agents).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RanOn {
    pub tool: otaq_core::agent_tools::Tool,
    pub agent: Option<String>,
}

/// The tools, what each stopped ticket ran on and the name of the stage it was in, keyed by ticket key ("SHOP-19").
#[derive(Debug, Clone, Default)]
pub struct Runs {
    pub tools: Tools,
    pub tickets: BTreeMap<String, RanOn>,
    /// "SHOP-27" → "Dev".
    pub stages: BTreeMap<String, String>,
}

impl Runs {
    /// The tool a ticket's stopped session ran on: its newest session's, else the first tool.
    pub fn tool(&self, key: &str) -> otaq_core::agent_tools::Tool {
        self.tickets
            .get(key)
            .map_or(otaq_core::agent_tools::Tool::ALL[0], |r| r.tool)
    }
}

/// Why `tool` can't resume anything now, as a row says it: "Codex isn’t signed in · sign in first", "Couldn’t check
/// Claude Code’s sign-in · Re-check first", "Codex isn’t available (not installed)", "Claude Code limit reached ·
/// resets 3:45 pm". `None` while it can run.
pub fn cannot_resume(tool: otaq_core::agent_tools::Tool, tools: &Tools) -> Option<String> {
    let problem = tools.problem(tool)?;
    // The tool's own sign-in answer decides: no answer is not signed in either, so nothing resumes on it until a
    // Re-check reads one.
    if let Some(AgentToolState::CouldNotCheck { .. }) = tools.status(tool).map(|s| &s.state) {
        return Some(format!(
            "Couldn\u{2019}t check {}\u{2019}s sign-in \u{b7} Re-check first",
            tool_word(tool)
        ));
    }
    let signed_out = matches!(
        tools.status(tool).map(|s| &s.state),
        Some(AgentToolState::FoundNotSignedIn { .. })
    );
    match problem {
        Problem::Unavailable { sign_in, .. } if sign_in || signed_out => Some(format!(
            "{} isn\u{2019}t signed in \u{b7} sign in first",
            tool_word(tool)
        )),
        _ => tool_problems::recovery_problem(tool, tools).map(|(text, _)| text),
    }
}

/// A service that started this long before the app opened was already running: a normal reopen.
pub const ALREADY_RUNNING_MS: i64 = 15_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnConnect {
    /// Open the after-restart dialog (once per service start).
    Summary,
    /// "Reconnected": the app reopened while the service kept running.
    Reconnected,
    Nothing,
}

/// What a connection to the service shows. `app_started_ms`: when this app opened; `first_connect`: the app's
/// first connection; `shown_for`: the service start a summary was already shown for.
pub fn on_connect(
    status: &ServiceStatus,
    app_started_ms: i64,
    first_connect: bool,
    shown_for: Option<i64>,
) -> OnConnect {
    if shown_for == Some(status.started_at) {
        return OnConnect::Nothing;
    }
    let already_running = status.started_at + ALREADY_RUNNING_MS < app_started_ms;
    if already_running {
        return if first_connect {
            OnConnect::Reconnected
        } else {
            OnConnect::Nothing
        };
    }
    if status.interrupted_at_start == 0 {
        return OnConnect::Nothing;
    }
    OnConnect::Summary
}

/// The startup report without the paused projects' tickets (PAU-07, d0112): a pause never lifts on its own, so the
/// dialog doesn't list them; their banner says they are still paused.
pub fn outside_pause(report: &StartupReport) -> StartupReport {
    let kept = |keys: &[otaq_core::domain::TicketKey]| -> Vec<otaq_core::domain::TicketKey> {
        keys.iter()
            .filter(|k| !report.paused.contains(&k.project.to_string()))
            .cloned()
            .collect()
    };
    StartupReport {
        interrupted: kept(&report.interrupted),
        session_ended: kept(&report.session_ended),
        was_validating: kept(&report.was_validating),
        ..report.clone()
    }
}

/// What one row shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AfterRestartRowState")]
pub enum RowState {
    /// Still stopped: its checkbox.
    Stopped,
    /// Resumed (or queued to resume).
    Resumed,
    /// Resume failed: Retry and Start from scratch (REC-03).
    Failed,
}

/// What a row stands for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AfterRestartRowKind")]
pub enum RowKind {
    Ticket,
    /// A conversation with Noyan.
    Conversation,
}

/// One stopped agent: a ticket's or a conversation with Noyan's. `AfterRestartRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AfterRestartRow")]
pub struct SummaryRow {
    /// The ticket's key, or the conversation's id: what its checkbox and Resume send.
    pub id: String,
    pub kind: RowKind,
    /// What the row starts with: the ticket's key, or "Noyan".
    pub tag: String,
    pub title: String,
    pub state: RowState,
    /// "was working in Dev", "was checking · starts the check again", "conversation"; "Resumed"; or why Resume
    /// failed.
    pub detail: String,
    /// "Codex isn’t signed in · sign in first": its tool can't run now, so it is unticked and can't be ticked.
    pub problem: Option<String>,
    pub checked: bool,
}

/// The after-restart dialog. `AfterRestartSummary` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AfterRestartSummary")]
pub struct SummaryPage {
    /// "3 agents were stopped by a restart".
    pub title: String,
    pub text: &'static str,
    pub rows: Vec<SummaryRow>,
    /// "SHOP-22 still waits for your answer; answering resumes it."
    pub notes: Vec<String>,
    pub retry: &'static str,
    pub scratch: &'static str,
    /// "Resuming…", on a row while the page waits for its Resume.
    pub resuming: &'static str,
    pub foot: &'static str,
    pub later: &'static str,
    /// "Resume 3".
    pub resume_selected: String,
    /// The ticked tickets, in order.
    pub selected: Vec<String>,
    /// The ticked conversations with Noyan.
    pub selected_agents: Vec<String>,
}

/// "3 agents were stopped by a restart".
pub fn title(n: usize) -> String {
    if n == 1 {
        "1 agent was stopped by a restart".to_owned()
    } else {
        format!("{n} agents were stopped by a restart")
    }
}

fn ticket_row(
    ticket: &Ticket,
    validating: bool,
    unchecked: &BTreeSet<String>,
    runs: &Runs,
) -> SummaryRow {
    let key = ticket.key.to_string();
    let tool = runs.tool(&key);
    let (state, detail) = match ticket.status {
        // d0063: no round is counted and the old checker conversation isn't resumed.
        Status::Interrupted if validating => (
            RowState::Stopped,
            "was checking \u{b7} starts the check again".to_owned(),
        ),
        Status::Interrupted => (
            RowState::Stopped,
            match runs.stages.get(&key) {
                Some(stage) => format!("was working in {stage}"),
                None => "was working".to_owned(),
            },
        ),
        Status::ResumeFailed => (
            RowState::Failed,
            format!(
                "Couldn\u{2019}t resume: {} has no saved conversation for this session. Retry tries again; Start \
                 from scratch begins a new session in its own copy.",
                tool.short_name()
            ),
        ),
        _ => (RowState::Resumed, "Resumed".to_owned()),
    };
    let problem = (state == RowState::Stopped)
        .then(|| cannot_resume(tool, &runs.tools))
        .flatten();
    SummaryRow {
        checked: state == RowState::Stopped && problem.is_none() && !unchecked.contains(&key),
        id: key.clone(),
        kind: RowKind::Ticket,
        tag: key,
        title: ticket.title.clone(),
        state,
        detail,
        problem,
    }
}

/// A ticket that was asking you, on a tool that can't run now: unticked with why.
fn asking_row(ticket: &Ticket, runs: &Runs) -> SummaryRow {
    let key = ticket.key.to_string();
    SummaryRow {
        problem: cannot_resume(runs.tool(&key), &runs.tools),
        id: key.clone(),
        kind: RowKind::Ticket,
        tag: key,
        title: ticket.title.clone(),
        state: RowState::Stopped,
        detail: "was waiting for you".to_owned(),
        checked: false,
    }
}

fn conversation_row(agent: &ProjectAgent, unchecked: &BTreeSet<String>, runs: &Runs) -> SummaryRow {
    let stopped = agent.state == AgentState::EndedByRestart;
    let problem = stopped
        .then(|| cannot_resume(agent.tool, &runs.tools))
        .flatten();
    let asked = agent.question.is_some() || agent.needs_you.is_some();
    SummaryRow {
        id: agent.id.clone(),
        kind: RowKind::Conversation,
        tag: "Noyan".to_owned(),
        title: agent.name.clone(),
        state: if stopped {
            RowState::Stopped
        } else {
            RowState::Resumed
        },
        detail: match (stopped, asked) {
            (false, _) => "Resumed".to_owned(),
            (true, true) => "conversation \u{b7} was asking you".to_owned(),
            (true, false) => "conversation".to_owned(),
        },
        checked: stopped && problem.is_none() && !unchecked.contains(&agent.id),
        problem,
    }
}

/// "SHOP-22 still waits for your answer; answering resumes it."
fn kept_note(kept: &[&Ticket]) -> Option<String> {
    match kept {
        [] => None,
        [one] => Some(format!(
            "{} still waits for your answer; answering resumes it.",
            one.key
        )),
        several => Some(format!(
            "{} still wait for your answers; answering resumes each.",
            several
                .iter()
                .map(|t| t.key.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

/// The dialog for this service start, or `None` once it stopped nothing. `tickets`: the ones `report` names, as they
/// are now; `agents`: the conversations with Noyan it names; `unchecked`: the rows the user unticked (every row that
/// can resume starts ticked); `runs`: what each ticket ran on and which tools can run now.
pub fn summary(
    report: &StartupReport,
    tickets: &[Ticket],
    agents: &[ProjectAgent],
    unchecked: &BTreeSet<String>,
    runs: &Runs,
) -> Option<SummaryPage> {
    let find = |key: &otaq_core::domain::TicketKey| tickets.iter().find(|t| &t.key == key);
    let stopped: Vec<&Ticket> = report.interrupted.iter().filter_map(find).collect();
    // A ticket that was asking kept its question: answering it from Needs you resumes and delivers it (d0045). A
    // stopped check is a row: its Resume starts the check again (d0063).
    let (asking, listed): (Vec<&Ticket>, Vec<&Ticket>) = stopped.iter().partition(|t| {
        t.status == Status::Interrupted
            && t.interrupted_from.is_some_and(|s| s.is_needs_you_family())
    });
    // Every stopped agent is in the dialog: one that was asking on a tool that can't run now can't be resumed by an
    // answer either, so it is a row, unticked with why, rather than the note.
    let (blocked, kept): (Vec<&Ticket>, Vec<&Ticket>) = asking
        .into_iter()
        .partition(|t| cannot_resume(runs.tool(&t.key.to_string()), &runs.tools).is_some());
    let mut rows: Vec<SummaryRow> = listed
        .iter()
        .map(|t| ticket_row(t, report.was_validating.contains(&t.key), unchecked, runs))
        .chain(blocked.iter().map(|t| asking_row(t, runs)))
        .chain(
            report
                .project_agents_ended
                .iter()
                .filter_map(|id| agents.iter().find(|a| &a.id == id))
                .map(|a| conversation_row(a, unchecked, runs)),
        )
        .collect();
    // The rows that can't resume now go last.
    rows.sort_by_key(|r| r.problem.is_some());
    let notes: Vec<String> = kept_note(&kept).into_iter().collect();
    if rows.is_empty() && notes.is_empty() {
        return None;
    }
    let ticked = |kind: RowKind| -> Vec<String> {
        rows.iter()
            .filter(|r| r.checked && r.kind == kind)
            .map(|r| r.id.clone())
            .collect()
    };
    let selected = ticked(RowKind::Ticket);
    let selected_agents = ticked(RowKind::Conversation);
    // The title counts the agents that can resume, as the button counts the ticked ones; the ones that can't are
    // counted only when none can, so it never says "0 agents were stopped".
    let can_resume = rows.iter().filter(|r| r.problem.is_none()).count();
    Some(SummaryPage {
        title: title(if can_resume == 0 {
            rows.len() + kept.len()
        } else {
            can_resume
        }),
        text: "Each can continue its same conversation, in its own copy. Resuming uses tokens.",
        notes,
        retry: "Retry",
        scratch: "Start from scratch",
        resuming: "Resuming\u{2026}",
        foot: "They wait in Needs you if you close this.",
        later: "Not now",
        resume_selected: format!("Resume {}", selected.len() + selected_agents.len()),
        selected,
        selected_agents,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures;
    use crate::view_models::tool_problems::fixtures::{self as tf, CLAUDE, CODEX};

    const APP: i64 = 1_791_600_000_000;

    fn status(started_at: i64, interrupted: u32, unclean: bool) -> ServiceStatus {
        ServiceStatus {
            started_at,
            interrupted_at_start: interrupted,
            unclean_start: unclean,
            ..Default::default()
        }
    }

    #[test]
    fn after_a_reboot_the_app_opens_the_dialog_once() {
        let s = status(APP + 2_000, 4, true);
        assert_eq!(on_connect(&s, APP, true, None), OnConnect::Summary);
        assert_eq!(
            on_connect(&s, APP, false, Some(s.started_at)),
            OnConnect::Nothing
        );
    }

    #[test]
    fn a_restart_while_the_app_runs_opens_it_too() {
        let s = status(APP + 600_000, 3, false);
        assert_eq!(
            on_connect(&s, APP, false, Some(APP - 1)),
            OnConnect::Summary
        );
        assert_eq!(
            on_connect(&status(APP + 600_000, 0, false), APP, false, None),
            OnConnect::Nothing
        );
    }

    #[test]
    fn a_normal_reopen_is_a_quiet_reconnected() {
        let s = status(APP - 3_600_000, 4, true);
        assert_eq!(on_connect(&s, APP, true, None), OnConnect::Reconnected);
        assert_eq!(on_connect(&s, APP, false, None), OnConnect::Nothing);
    }

    fn report(interrupted: &[&str]) -> StartupReport {
        StartupReport {
            unclean: true,
            interrupted: interrupted.iter().map(|k| k.parse().unwrap()).collect(),
            ..Default::default()
        }
    }

    fn ended_agent(id: &str, name: &str, tool: otaq_core::agent_tools::Tool) -> ProjectAgent {
        ProjectAgent {
            id: id.into(),
            project: "SHOP".into(),
            name: name.into(),
            name_source: None,
            state: AgentState::EndedByRestart,
            tool,
            tool_session_id: Some("s".into()),
            model: None,
            got_request: true,
            mid_turn: false,
            limit_stopped: false,
            created_at: 0,
            ended_at: Some(0),
            closed: false,
            question: None,
            needs_you: None,
            focused: false,
            tokens: Default::default(),
            session: None,
            turn_ended_at: None,
        }
    }

    fn ran(tool: otaq_core::agent_tools::Tool) -> RanOn {
        RanOn { tool, agent: None }
    }

    /// The dialog as drawn: a ticket that was working, a check and a conversation with Noyan, ticked; a ticket on a
    /// signed-out tool unticked with why, last; the ticket that was asking named below.
    #[test]
    fn the_dialog_lists_what_stopped_ticked_and_what_cant_resume_unticked() {
        let working = card_fixtures::ticket("SHOP-27", "Wishlist share link", Status::Interrupted);
        let mut checking =
            card_fixtures::ticket("SHOP-26", "Price table rounding", Status::Interrupted);
        checking.interrupted_from = Some(Status::Checking);
        let signed_out = card_fixtures::ticket("BILL-4", "Invoice PDF export", Status::Interrupted);
        let mut asking =
            card_fixtures::ticket("SHOP-22", "Fix flaky login redirect", Status::Interrupted);
        asking.interrupted_from = Some(Status::AsksYou);
        let tickets = vec![signed_out, working, checking, asking];
        let mut r = report(&["BILL-4", "SHOP-27", "SHOP-26", "SHOP-22"]);
        r.was_validating = vec!["SHOP-26".parse().unwrap()];
        r.project_agents_ended = vec!["a1".into()];
        let runs = Runs {
            tools: tf::codex_logged_out(),
            tickets: BTreeMap::from([
                ("BILL-4".to_owned(), ran(CODEX)),
                ("SHOP-27".to_owned(), ran(CLAUDE)),
            ]),
            stages: BTreeMap::from([("SHOP-27".to_owned(), "Dev".to_owned())]),
        };
        let agents = [ended_agent("a1", "Checkout and footer", CLAUDE)];
        let page = summary(&r, &tickets, &agents, &BTreeSet::new(), &runs).unwrap();
        let rows: Vec<(&str, &str, &str, Option<&str>, bool)> = page
            .rows
            .iter()
            .map(|r| {
                (
                    r.tag.as_str(),
                    r.title.as_str(),
                    r.detail.as_str(),
                    r.problem.as_deref(),
                    r.checked,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "SHOP-27",
                    "Wishlist share link",
                    "was working in Dev",
                    None,
                    true
                ),
                (
                    "SHOP-26",
                    "Price table rounding",
                    "was checking \u{b7} starts the check again",
                    None,
                    true
                ),
                ("Noyan", "Checkout and footer", "conversation", None, true),
                (
                    "BILL-4",
                    "Invoice PDF export",
                    "was working",
                    Some("Codex isn\u{2019}t signed in \u{b7} sign in first"),
                    false
                ),
            ]
        );
        // As drawn: the title counts the three that can resume, not the row that can't nor the asking note.
        assert_eq!(page.title, "3 agents were stopped by a restart");
        assert_eq!(page.resume_selected, "Resume 3");
        assert_eq!(page.selected, ["SHOP-27", "SHOP-26"]);
        assert_eq!(page.selected_agents, ["a1"]);
        assert_eq!(
            page.notes,
            ["SHOP-22 still waits for your answer; answering resumes it."]
        );
        assert_eq!(page.foot, "They wait in Needs you if you close this.");
        // Unticked rows leave Resume.
        let page = summary(
            &r,
            &tickets,
            &agents,
            &BTreeSet::from(["SHOP-27".to_owned(), "a1".to_owned()]),
            &runs,
        )
        .unwrap();
        assert_eq!(page.selected, ["SHOP-26"]);
        assert!(page.selected_agents.is_empty());
        assert_eq!(page.resume_selected, "Resume 1");
    }

    fn claude_as(state: AgentToolState) -> Tools {
        let mut tools = tf::several();
        tools.statuses[0] = tf::status(CLAUDE, state);
        tools.connected.retain(|t| *t != CLAUDE);
        tools
    }

    fn rows_of(page: &SummaryPage) -> Vec<(&str, Option<&str>, bool)> {
        page.rows
            .iter()
            .map(|r| (r.tag.as_str(), r.problem.as_deref(), r.checked))
            .collect()
    }

    /// The release test: one agent that can resume and one that was waiting for you on a signed-out Claude Code. Each
    /// has a row (the signed-out one unticked with why, not hidden in the "still waits for your answer" note), and
    /// the title and the button both count the one that can resume.
    #[test]
    fn an_agent_on_a_signed_out_tool_gets_an_unticked_row_and_isnt_counted() {
        let working = card_fixtures::ticket("SHOP-9", "Reltest wait", Status::Interrupted);
        let mut idle = card_fixtures::ticket("NOTE-4", "Reltest cc", Status::Interrupted);
        idle.interrupted_from = Some(Status::Idle);
        let r = report(&["SHOP-9", "NOTE-4"]);
        let runs = Runs {
            tools: claude_as(AgentToolState::FoundNotSignedIn { version: None }),
            tickets: BTreeMap::from([
                ("SHOP-9".to_owned(), ran(CODEX)),
                ("NOTE-4".to_owned(), ran(CLAUDE)),
            ]),
            ..Runs::default()
        };
        let page = summary(&r, &[working, idle], &[], &BTreeSet::new(), &runs).unwrap();
        assert_eq!(
            rows_of(&page),
            [
                ("SHOP-9", None, true),
                (
                    "NOTE-4",
                    Some("Claude Code isn\u{2019}t signed in \u{b7} sign in first"),
                    false
                ),
            ]
        );
        assert_eq!(page.rows[1].detail, "was waiting for you");
        assert!(page.notes.is_empty());
        assert_eq!(page.title, "1 agent was stopped by a restart");
        assert_eq!(page.resume_selected, "Resume 1");
        assert_eq!(page.selected, ["SHOP-9"]);

        // The same with a ticket that was working on the signed-out tool.
        let mut working_cc = card_fixtures::ticket("NOTE-4", "Reltest cc", Status::Interrupted);
        working_cc.interrupted_from = Some(Status::Working);
        let working = card_fixtures::ticket("SHOP-9", "Reltest wait", Status::Interrupted);
        let page = summary(&r, &[working, working_cc], &[], &BTreeSet::new(), &runs).unwrap();
        assert_eq!(rows_of(&page).len(), 2);
        assert!(!page.rows[1].checked && page.rows[1].problem.is_some());
        assert_eq!(page.title, "1 agent was stopped by a restart");
        assert_eq!(page.resume_selected, "Resume 1");
    }

    /// A tool whose sign-in answer couldn't be read can't resume anything either: its row says so and points at
    /// Re-check.
    #[test]
    fn a_tool_whose_sign_in_couldnt_be_checked_gets_an_unticked_row() {
        let mut asking = card_fixtures::ticket("NOTE-4", "Reltest cc", Status::Interrupted);
        asking.interrupted_from = Some(Status::AsksYou);
        let mut r = report(&["NOTE-4"]);
        r.project_agents_ended = vec!["a1".into()];
        let runs = Runs {
            tools: claude_as(AgentToolState::CouldNotCheck { version: None }),
            tickets: BTreeMap::from([("NOTE-4".to_owned(), ran(CLAUDE))]),
            ..Runs::default()
        };
        let agents = [ended_agent("a1", "Import issues", CLAUDE)];
        let page = summary(&r, &[asking], &agents, &BTreeSet::new(), &runs).unwrap();
        let why = "Couldn\u{2019}t check Claude Code\u{2019}s sign-in \u{b7} Re-check first";
        assert_eq!(
            rows_of(&page),
            [("NOTE-4", Some(why), false), ("Noyan", Some(why), false)]
        );
        // None can resume: the title counts every stopped agent, never "0 agents".
        assert_eq!(page.title, "2 agents were stopped by a restart");
        assert_eq!(page.resume_selected, "Resume 0");
    }

    /// A ticket that was asking on a tool that can run keeps the note, as drawn, and isn't counted as a row.
    #[test]
    fn a_ticket_asking_on_a_tool_that_can_run_stays_the_note() {
        let working = card_fixtures::ticket("SHOP-27", "Wishlist share link", Status::Interrupted);
        let mut asking = card_fixtures::ticket("SHOP-22", "Fix flaky login", Status::Interrupted);
        asking.interrupted_from = Some(Status::AsksYou);
        let runs = Runs {
            tools: tf::several(),
            ..Runs::default()
        };
        let page = summary(
            &report(&["SHOP-27", "SHOP-22"]),
            &[working, asking],
            &[],
            &BTreeSet::new(),
            &runs,
        )
        .unwrap();
        assert_eq!(rows_of(&page), [("SHOP-27", None, true)]);
        assert_eq!(
            page.notes,
            ["SHOP-22 still waits for your answer; answering resumes it."]
        );
        assert_eq!(page.title, "1 agent was stopped by a restart");
    }

    /// A conversation with Noyan on a tool at its limit stays unticked too: nothing resumes on a tool that can't run.
    #[test]
    fn a_conversation_on_a_tool_that_cant_run_is_unticked() {
        let mut r = report(&[]);
        r.project_agents_ended = vec!["a1".into()];
        let runs = Runs {
            tools: tf::at_limit(CODEX, None),
            ..Runs::default()
        };
        let agents = [ended_agent("a1", "Import issues", CODEX)];
        let page = summary(&r, &[], &agents, &BTreeSet::new(), &runs).unwrap();
        assert_eq!(page.title, "1 agent was stopped by a restart");
        let row = &page.rows[0];
        assert!(!row.checked);
        assert_eq!(row.problem.as_deref(), Some("Codex limit reached"));
        assert_eq!(page.resume_selected, "Resume 0");
    }

    #[test]
    fn rows_say_what_resume_did() {
        let resumed = card_fixtures::ticket("SHOP-19", "Price table", Status::Working);
        let failed = card_fixtures::ticket("BILL-4", "Invoice PDF export", Status::ResumeFailed);
        let mut agent = ended_agent("a1", "Checkout slowness", CLAUDE);
        agent.state = AgentState::Running;
        let mut r = report(&["SHOP-19", "BILL-4"]);
        r.project_agents_ended = vec!["a1".into()];
        let page = summary(
            &r,
            &[resumed, failed],
            &[agent],
            &BTreeSet::new(),
            &Runs::default(),
        )
        .unwrap();
        let states: Vec<(&str, RowState, bool)> = page
            .rows
            .iter()
            .map(|r| (r.id.as_str(), r.state, r.checked))
            .collect();
        assert_eq!(
            states,
            [
                ("SHOP-19", RowState::Resumed, false),
                ("BILL-4", RowState::Failed, false),
                ("a1", RowState::Resumed, false),
            ]
        );
        assert!(
            page.rows[1]
                .detail
                .starts_with("Couldn\u{2019}t resume: Claude")
        );
        assert!(page.selected.is_empty() && page.selected_agents.is_empty());
        // Nothing stopped: no dialog.
        assert_eq!(
            summary(&report(&[]), &[], &[], &BTreeSet::new(), &Runs::default()),
            None
        );
    }

    /// PAU-07: a paused project's tickets stay out of the dialog.
    #[test]
    fn the_dialog_leaves_paused_projects_out() {
        let key = |k: &str| k.parse::<otaq_core::domain::TicketKey>().unwrap();
        let report = StartupReport {
            interrupted: vec![key("BILL-4"), key("SHOP-19")],
            was_validating: vec![key("SHOP-19")],
            session_ended: vec![key("SHOP-23")],
            paused: vec!["SHOP".into()],
            ..StartupReport::default()
        };
        let out = outside_pause(&report);
        assert_eq!(out.interrupted, [key("BILL-4")]);
        assert!(out.was_validating.is_empty() && out.session_ended.is_empty());
        assert_eq!(out.paused, ["SHOP"]);
    }
}
