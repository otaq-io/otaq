//! What the app says when it connects (REC-06; d0045, d0022; *16 → Opening after a reboot*, *11 → After the service
//! restart*): after a reboot, a crash or a service restart the service started again and its startup reconcile
//! interrupted sessions, so the app opens with "4 sessions were interrupted" (Resume all, Review & resume). After a
//! normal reopen, with the service still running, only a quiet "Reconnected".
//!
//! [`summary`] is the summary sheet beside the banner (*16 → Opening after a reboot*, T-0235): the interrupted
//! tickets with a checkbox and Resume each, Resume selected, what each Resume did (resumed, or Couldn't resume with
//! Retry and Start from scratch), and the tickets the restart left alone. Nothing resumes without a click (d0045).
//! A ticket that was validating says Resume starts a fresh validation; the Project agents the restart ended have their
//! own rows with Resume (same conversation), outside Resume selected (T-0244; d0063, d0068).
//!
//! **Per tool** (d0086, *16 → Row · Recovery per tool (0086)*): each row resumes on the tool it started on, named
//! with several tools connected ("Strong · Claude Code"); a row whose tool isn't available now says so ("Standard ·
//! Codex · Codex isn't available (logged out)") with Sign in… in place of Resume, and Resume selected skips it.

use std::collections::{BTreeMap, BTreeSet};

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::project_agents::{AgentState, ProjectAgent};
use otaq_core::protocol::ServiceStatus;
use otaq_core::recovery::StartupReport;
use serde::Serialize;
use ts_rs::TS;

use crate::shell::tilde;
use crate::view_models::banners::Situation;
use crate::view_models::card::tokens_label;
use crate::view_models::clock::local_hhmm;
use crate::view_models::status::pill;
use crate::view_models::tool_problems::{self, Button, Tools, tool_word};

/// What an interrupted ticket's session ran on: its tool and named agent (`None` before named agents).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RanOn {
    pub tool: otaq_core::agent_tools::Tool,
    pub agent: Option<String>,
}

/// The tools and what each interrupted ticket ran on, keyed by ticket key ("SHOP-19").
#[derive(Debug, Clone, Default)]
pub struct Runs {
    pub tools: Tools,
    pub tickets: BTreeMap<String, RanOn>,
}

impl Runs {
    /// "Strong · Claude Code" with several tools connected; `None` with one (as M22).
    fn line(&self, ran: Option<&RanOn>) -> Option<String> {
        let ran = ran.filter(|_| self.tools.several())?;
        let tool = tool_word(ran.tool);
        Some(match &ran.agent {
            Some(name) if !name.eq_ignore_ascii_case(tool) => format!("{name} \u{b7} {tool}"),
            _ => tool.to_owned(),
        })
    }
}

/// A service that started this long before the app opened was already running: a normal reopen.
pub const ALREADY_RUNNING_MS: i64 = 15_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnConnect {
    /// Raise the summary banner (once per service start).
    Summary(Situation),
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
    OnConnect::Summary(Situation::SessionsInterrupted {
        count: status.interrupted_at_start,
        unclean: status.unclean_start,
        at: local_hhmm(status.started_at),
        // The startup report adds them once it is read (`with_agents`).
        agents: 0,
        leads: 0,
    })
}

/// The startup report without the paused projects' tickets (PAU-07, d0112): a pause never lifts on its own, so the
/// reboot summary doesn't open for them; their banner says they are still paused.
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

/// Rows the sheet lists one by one; the rest fold into one "+3" row (*Opening after a reboot*).
pub const LISTED: usize = 5;

/// What one row of the sheet shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AfterRestartRowState")]
pub enum RowState {
    /// Still Interrupted: the checkbox and Resume.
    Interrupted,
    /// Resumed (or queued to resume): no button.
    Resumed,
    /// Resume failed: Retry and Start from scratch (REC-03).
    Failed,
}

/// One interrupted ticket. `AfterRestartRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AfterRestartRow")]
pub struct SummaryRow {
    pub key: String,
    /// The project's name ("shop-web").
    pub project: String,
    pub title: String,
    pub state: RowState,
    /// "SHOP-19 · Resumed", "BILL-4 · Couldn’t resume"; `None` while Interrupted.
    pub headline: Option<String>,
    /// "was Agent Working · round 2 · 88k", "Agent Working again · same conversation", or why Resume failed.
    pub detail: String,
    pub checked: bool,
    /// While the page waits for its Resume: "SHOP-25 · Resuming…" and what it does.
    pub resuming: String,
    pub resuming_detail: String,
    /// "Strong · Claude Code", with several tools connected.
    pub agent: Option<String>,
    /// "Codex isn't available (logged out)": its tool can't resume it now; Resume selected skips it.
    pub problem: Option<String>,
    /// Sign in… in place of Resume, while its tool is logged out.
    pub sign_in: Option<Button>,
}

/// The rows after the first [`LISTED`], folded into one. `AfterRestartMore` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AfterRestartMore")]
pub struct SummaryMore {
    /// "+3".
    pub count: String,
    /// "SHOP-30, SHOP-40.4, SHOP-40.2".
    pub keys: String,
    /// "was working · expand to resume one by one": a click unfolds the row.
    pub detail: &'static str,
    /// Every folded ticket still to resume is selected.
    pub checked: bool,
    /// The folded tickets still to resume, for its checkbox and its Resume.
    pub resumable: Vec<String>,
}

/// One Project agent the restart ended (*16 → Opening after a reboot*, PROJECT AGENTS). `AfterRestartAgent` on the
/// page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AfterRestartAgent")]
pub struct AgentRow {
    pub id: String,
    pub project: String,
    /// Where a ticket key sits: "agent".
    pub tag: &'static str,
    pub name: String,
    /// "Ended by restart · same conversation", "Ended by restart · was asking you"; "Resumed" once it runs again.
    pub detail: String,
    /// Still ended: Resume continues the same conversation.
    pub resumable: bool,
    /// "Standard · Codex", with several tools connected.
    pub agent: Option<String>,
    /// Its tool can't resume it now (Sign in… in place of Resume).
    pub problem: Option<String>,
    pub sign_in: Option<Button>,
}

/// The Project agents part of the sheet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AfterRestartAgents")]
pub struct AgentsPart {
    /// "PROJECT AGENTS · NOT IN RESUME SELECTED · TERMINALS ARE GONE".
    pub title: &'static str,
    pub rows: Vec<AgentRow>,
    /// "A “New agent” that had no request yet is discarded, not listed."
    pub discarded: Option<String>,
}

/// The summary sheet (*16 → Opening after a reboot*). `AfterRestartSummary` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AfterRestartSummary")]
pub struct SummaryPage {
    pub title: String,
    pub text: &'static str,
    pub rows: Vec<SummaryRow>,
    pub more: Option<SummaryMore>,
    /// The Project agents the restart ended; `None` when it ended none and discarded none.
    pub agents: Option<AgentsPart>,
    pub notes_title: &'static str,
    /// The tickets the restart left alone or that Resume selected leaves to Needs you.
    pub notes: Vec<String>,
    pub resume: &'static str,
    pub retry: &'static str,
    pub scratch: &'static str,
    pub foot: &'static str,
    pub later: &'static str,
    /// "Resume selected (4)".
    pub resume_selected: String,
    /// What Resume selected resumes, in order.
    pub selected: Vec<String>,
    /// "Resume selected skips the 2 Codex sessions until you sign in".
    pub skipped: Vec<String>,
}

fn joined(keys: &[&Ticket]) -> String {
    keys.iter()
        .map(|t| t.key.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn was(ticket: &Ticket) -> String {
    let mut parts = vec![format!("was {}", pill(Status::Working).1)];
    if let Some(back) = &ticket.flags.sent_back {
        parts.push(format!("round {}", back.round));
    }
    if let Some(tokens) = ticket.flags.tokens {
        parts.push(tokens_label(tokens));
    }
    parts.join(" · ")
}

fn row(
    ticket: &Ticket,
    projects: &BTreeMap<String, String>,
    unchecked: &BTreeSet<String>,
    validating: bool,
    runs: &Runs,
) -> SummaryRow {
    let key = ticket.key.to_string();
    let ran = runs.tickets.get(&key);
    let tool = ran.map_or(otaq_core::agent_tools::Tool::ALL[0], |r| r.tool);
    let problem = tool_problems::recovery_problem(tool, &runs.tools);
    let (state, headline, detail) = match ticket.status {
        // d0063: no round is counted and the old validator conversation isn't resumed.
        Status::Interrupted if validating => (
            RowState::Interrupted,
            None,
            "was Validating \u{b7} Resume = fresh validation".to_owned(),
        ),
        Status::Interrupted => (RowState::Interrupted, None, was(ticket)),
        Status::ResumeFailed => (
            RowState::Failed,
            Some(format!("{key} · Couldn’t resume")),
            format!(
                "{} has no saved conversation for this session, so it can’t continue where it left off. Retry \
                 tries again; Start from scratch begins a new session in the same worktree.",
                tool.short_name()
            ),
        ),
        other => (
            RowState::Resumed,
            Some(format!("{key} · Resumed")),
            format!("{} again · same conversation", pill(other).1),
        ),
    };
    let worktree = ticket
        .work
        .as_ref()
        .and_then(|w| w.worktree.as_deref())
        .map(tilde);
    SummaryRow {
        project: projects
            .get(ticket.key.project.as_str())
            .cloned()
            .unwrap_or_else(|| ticket.key.project.to_string()),
        title: ticket.title.clone(),
        state,
        headline,
        detail,
        checked: state == RowState::Interrupted && !unchecked.contains(&key) && problem.is_none(),
        resuming: format!("{key} · Resuming…"),
        resuming_detail: {
            // "claude --resume", "codex resume": the tool's own resume, without the id.
            let command = tool.resume_hint("");
            match worktree {
                Some(w) => format!("Reattaching {} in {w}", command.trim()),
                None => format!("Reattaching {}", command.trim()),
            }
        },
        agent: runs.line(ran),
        sign_in: problem.as_ref().and_then(|(_, b)| b.clone()),
        problem: problem.map(|(p, _)| p),
        key,
    }
}

fn agents_part(
    report: &StartupReport,
    agents: &[ProjectAgent],
    projects: &BTreeMap<String, String>,
    runs: &Runs,
) -> Option<AgentsPart> {
    let rows: Vec<AgentRow> = report
        .project_agents_ended
        .iter()
        .filter_map(|id| agents.iter().find(|a| &a.id == id))
        .map(|a| {
            let ended = a.state == AgentState::EndedByRestart;
            let problem = tool_problems::recovery_problem(a.tool, &runs.tools);
            let ran = RanOn {
                tool: a.tool,
                agent: a
                    .session
                    .as_ref()
                    .and_then(|s| s.agent.as_ref())
                    .map(|n| n.name.clone()),
            };
            AgentRow {
                agent: runs.line(Some(&ran)),
                sign_in: problem.as_ref().and_then(|(_, b)| b.clone()),
                problem: problem.as_ref().map(|(p, _)| p.clone()),
                id: a.id.clone(),
                project: projects
                    .get(&a.project)
                    .cloned()
                    .unwrap_or_else(|| a.project.clone()),
                tag: "agent",
                name: a.name.clone(),
                detail: match (ended, a.question.is_some() || a.needs_you.is_some()) {
                    (false, _) => "Resumed \u{b7} same conversation".to_owned(),
                    (true, true) => "Ended by restart \u{b7} was asking you".to_owned(),
                    (true, false) => "Ended by restart \u{b7} same conversation".to_owned(),
                },
                resumable: ended && problem.is_none(),
            }
        })
        .collect();
    let discarded = match report.new_agents_discarded {
        0 => None,
        1 => Some(
            "A \u{201c}New agent\u{201d} that had no request yet is discarded, not listed."
                .to_owned(),
        ),
        n => Some(format!(
            "{n} \u{201c}New agent\u{201d}s that had no request yet are discarded, not listed."
        )),
    };
    (!rows.is_empty() || discarded.is_some()).then_some(AgentsPart {
        title: "PROJECT AGENTS \u{b7} NOT IN RESUME SELECTED \u{b7} TERMINALS ARE GONE",
        rows,
        discarded,
    })
}

/// "Resume selected skips the 2 Codex sessions until you sign in", one line per tool that can't resume now.
fn skipped(tickets: &[&Ticket], runs: &Runs) -> Vec<String> {
    let mut lines = Vec::new();
    for &tool in otaq_core::agent_tools::Tool::ALL {
        let count = tickets
            .iter()
            .filter(|t| t.status == Status::Interrupted)
            .filter(|t| {
                runs.tickets
                    .get(&t.key.to_string())
                    .is_some_and(|r| r.tool == tool)
            })
            .count();
        let Some(problem) = runs.tools.problem(tool).filter(|_| count > 0) else {
            continue;
        };
        let sessions = if count == 1 {
            format!("the {} session", tool_word(tool))
        } else {
            format!("the {count} {} sessions", tool_word(tool))
        };
        let until = match problem {
            tool_problems::Problem::Unavailable { sign_in: true, .. } => "you sign in".to_owned(),
            tool_problems::Problem::Unavailable { .. } => {
                format!("{} is available again", tool_word(tool))
            }
            tool_problems::Problem::Limit { .. } => {
                format!("{}\u{2019}s limit resets", tool_word(tool))
            }
        };
        lines.push(format!("Resume selected skips {sessions} until {until}"));
    }
    lines
}

/// "8 sessions were interrupted · 2 Project agents ended · 1 epic lead ended" (*16 → Epic lead after a restart*).
pub fn title(sessions: usize, agents: usize, leads: usize) -> String {
    let sessions = match sessions {
        0 => None,
        1 => Some("1 session was interrupted".to_owned()),
        n => Some(format!("{n} sessions were interrupted")),
    };
    sessions
        .into_iter()
        .chain(ended(agents, leads))
        .collect::<Vec<_>>()
        .join(" \u{b7} ")
}

/// What the restart ended besides tickets, each counted on its own (d0068, d0083): "2 Project agents ended", "1
/// epic lead ended". Epic leads aren't Project agents in the count, and aren't in Resume selected.
pub fn ended(agents: usize, leads: usize) -> Vec<String> {
    let agents = match agents {
        0 => None,
        1 => Some("1 Project agent ended".to_owned()),
        n => Some(format!("{n} Project agents ended")),
    };
    let leads = match leads {
        0 => None,
        1 => Some("1 epic lead ended".to_owned()),
        n => Some(format!("{n} epic leads ended")),
    };
    [agents, leads].into_iter().flatten().collect()
}

/// The sheet for this service start, or `None` once it interrupted nothing. `tickets`: the ones `report` names,
/// as they are now; `agents`: the Project agents it names; `projects`: key → name; `unchecked`: the rows the user
/// unticked (every row starts ticked); `unfolded`: the "+3" row was opened.
pub fn summary(
    report: &StartupReport,
    tickets: &[Ticket],
    agents: &[ProjectAgent],
    projects: &BTreeMap<String, String>,
    unchecked: &BTreeSet<String>,
    unfolded: bool,
) -> Option<SummaryPage> {
    summary_with(
        report,
        tickets,
        agents,
        projects,
        unchecked,
        unfolded,
        &Runs::default(),
    )
}

/// [`summary`] per tool: `runs` says what each ticket ran on and which tools can resume now.
pub fn summary_with(
    report: &StartupReport,
    tickets: &[Ticket],
    agents: &[ProjectAgent],
    projects: &BTreeMap<String, String>,
    unchecked: &BTreeSet<String>,
    unfolded: bool,
    runs: &Runs,
) -> Option<SummaryPage> {
    let find = |key: &otaq_core::domain::TicketKey| tickets.iter().find(|t| &t.key == key);
    let interrupted: Vec<&Ticket> = report.interrupted.iter().filter_map(find).collect();
    let agents_part = agents_part(report, agents, projects, runs);
    if interrupted.is_empty() && agents_part.is_none() && report.leads_ended.is_empty() {
        return None;
    }
    // Needs-you tickets kept their question: answering from Needs you resumes and delivers it (d0045). An
    // interrupted check is a row: its Resume starts a fresh check (d0063).
    let (kept, listed): (Vec<&Ticket>, Vec<&Ticket>) = interrupted.iter().partition(|t| {
        t.status == Status::Interrupted
            && t.interrupted_from.is_some_and(|s| s.is_needs_you_family())
    });
    let mut rows: Vec<SummaryRow> = listed
        .iter()
        .map(|t| {
            row(
                t,
                projects,
                unchecked,
                report.was_validating.contains(&t.key),
                runs,
            )
        })
        .collect();
    let more = (!unfolded && rows.len() > LISTED + 1).then(|| {
        let folded = rows.split_off(LISTED);
        let resumable: Vec<String> = folded
            .iter()
            .filter(|r| r.state == RowState::Interrupted && r.problem.is_none())
            .map(|r| r.key.clone())
            .collect();
        SummaryMore {
            count: format!("+{}", folded.len()),
            keys: folded
                .iter()
                .map(|r| r.key.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            detail: "was working · expand to resume one by one",
            checked: !resumable.is_empty() && resumable.iter().all(|k| !unchecked.contains(k)),
            resumable,
        }
    });
    let mut selected: Vec<String> = rows
        .iter()
        .filter(|r| r.checked)
        .map(|r| r.key.clone())
        .collect();
    if let Some(m) = &more {
        selected.extend(
            m.resumable
                .iter()
                .filter(|k| !unchecked.contains(*k))
                .cloned(),
        );
    }
    let mut notes = Vec::new();
    if !kept.is_empty() {
        notes.push(format!(
            "Turquoise tickets keep their question — {}. Answering it from Needs you resumes and delivers it.",
            joined(&kept)
        ));
    }
    let untouched: Vec<&Ticket> = report.session_ended.iter().filter_map(find).collect();
    if !untouched.is_empty() {
        notes.push(format!(
            "Review and Approved are untouched — {}. Run and Shell show “stopped by restart”.",
            joined(&untouched)
        ));
    }
    Some(SummaryPage {
        title: title(
            report.interrupted.len(),
            report.project_agents_ended.len(),
            report.leads_ended.len(),
        ),
        text: "Only Dev tickets that were working become Interrupted. Each one can resume its session on the tool it \
               started on in the same worktree; resuming uses tokens, so it needs your click.",
        rows,
        more,
        agents: agents_part,
        // Q44: the frame says "NOT IN RESUME ALL", but Resume all includes the
        // needs-you tickets (d0045); the sheet's own button leaves them out.
        notes_title: "NOT IN RESUME SELECTED",
        notes,
        resume: "Resume",
        retry: "Retry",
        scratch: "Start from scratch",
        foot: "They stay in Needs you if you close this.",
        later: "Not now",
        resume_selected: format!("Resume selected ({})", selected.len()),
        selected,
        skipped: skipped(&listed, runs),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures;

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
    fn after_a_reboot_the_app_opens_with_the_summary_once() {
        let s = status(APP + 2_000, 4, true);
        let OnConnect::Summary(Situation::SessionsInterrupted {
            count, unclean, at, ..
        }) = on_connect(&s, APP, true, None)
        else {
            panic!("summary")
        };
        assert_eq!((count, unclean, at.len()), (4, true, 5));
        assert_eq!(
            on_connect(&s, APP, false, Some(s.started_at)),
            OnConnect::Nothing
        );
    }

    #[test]
    fn a_restart_while_the_app_runs_shows_it_too() {
        let s = status(APP + 600_000, 3, false);
        assert!(matches!(
            on_connect(&s, APP, false, Some(APP - 1)),
            OnConnect::Summary(Situation::SessionsInterrupted { unclean: false, .. })
        ));
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

    fn report(interrupted: &[&str], session_ended: &[&str]) -> StartupReport {
        StartupReport {
            unclean: true,
            interrupted: interrupted.iter().map(|k| k.parse().unwrap()).collect(),
            session_ended: session_ended.iter().map(|k| k.parse().unwrap()).collect(),
            ..Default::default()
        }
    }

    fn projects() -> BTreeMap<String, String> {
        BTreeMap::from([("SHOP".to_owned(), "shop-web".to_owned())])
    }

    #[test]
    fn the_sheet_lists_the_interrupted_tickets_all_ticked() {
        let mut working =
            card_fixtures::ticket("SHOP-27", "Search results paging", Status::Interrupted);
        working.flags.tokens = Some(88_000);
        working.flags.sent_back = Some(otaq_core::domain::flags::SentBack {
            comments: 1,
            round: 2,
            decisions: 0,
        });
        let mut needs_you = card_fixtures::ticket("SHOP-22", "Login", Status::Interrupted);
        needs_you.interrupted_from = Some(Status::AsksYou);
        let review = card_fixtures::ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        let tickets = vec![working, needs_you, review];
        let page = summary(
            &report(&["SHOP-27", "SHOP-22"], &["SHOP-18"]),
            &tickets,
            &[],
            &projects(),
            &BTreeSet::new(),
            false,
        )
        .unwrap();
        assert_eq!(page.title, "2 sessions were interrupted");
        assert_eq!(
            page.rows.len(),
            1,
            "the needs-you ticket is a note, not a row"
        );
        let r = &page.rows[0];
        assert_eq!(
            (r.project.as_str(), r.state, r.checked),
            ("shop-web", RowState::Interrupted, true)
        );
        assert_eq!(r.detail, "was Working · round 2 · 88k");
        assert_eq!(page.resume_selected, "Resume selected (1)");
        assert_eq!(page.selected, ["SHOP-27"]);
        assert_eq!(
            page.notes,
            [
                "Turquoise tickets keep their question — SHOP-22. Answering it from Needs you resumes and delivers it.",
                "Review and Approved are untouched — SHOP-18. Run and Shell show “stopped by restart”."
            ]
        );
        // Unticked: Resume selected leaves it out; nothing resumed it.
        let page = summary(
            &report(&["SHOP-27"], &[]),
            &tickets,
            &[],
            &projects(),
            &BTreeSet::from(["SHOP-27".to_owned()]),
            false,
        )
        .unwrap();
        assert!(!page.rows[0].checked);
        assert_eq!(page.resume_selected, "Resume selected (0)");
    }

    #[test]
    fn rows_say_what_resume_did() {
        let resumed = card_fixtures::ticket("SHOP-19", "Price table", Status::Working);
        let failed = card_fixtures::ticket("BILL-4", "Invoice PDF export", Status::ResumeFailed);
        let page = summary(
            &report(&["SHOP-19", "BILL-4"], &[]),
            &[resumed, failed],
            &[],
            &projects(),
            &BTreeSet::new(),
            false,
        )
        .unwrap();
        assert_eq!(page.rows[0].headline.as_deref(), Some("SHOP-19 · Resumed"));
        assert_eq!(page.rows[0].detail, "Working again · same conversation");
        assert_eq!(page.rows[1].state, RowState::Failed);
        assert_eq!(
            page.rows[1].headline.as_deref(),
            Some("BILL-4 · Couldn’t resume")
        );
        assert_eq!(
            page.rows[1].project, "BILL",
            "an unknown project shows its key"
        );
        assert!(page.selected.is_empty());
        assert_eq!(page.rows[0].resuming, "SHOP-19 · Resuming…");
    }

    #[test]
    fn a_long_list_folds_into_one_row_and_unfolds() {
        let keys: Vec<String> = (1..=8).map(|i| format!("SHOP-{i}")).collect();
        let tickets: Vec<Ticket> = keys
            .iter()
            .map(|k| card_fixtures::ticket(k, "t", Status::Interrupted))
            .collect();
        let refs: Vec<&str> = keys.iter().map(String::as_str).collect();
        let r = report(&refs, &[]);
        let page = summary(
            &r,
            &tickets,
            &[],
            &projects(),
            &BTreeSet::from(["SHOP-8".to_owned()]),
            false,
        )
        .unwrap();
        assert_eq!(page.rows.len(), LISTED);
        let more = page.more.unwrap();
        assert_eq!(
            (more.count.as_str(), more.keys.as_str()),
            ("+3", "SHOP-6, SHOP-7, SHOP-8")
        );
        assert!(!more.checked);
        assert_eq!(page.selected.len(), 7);
        assert_eq!(page.title, "8 sessions were interrupted");
        let open = summary(&r, &tickets, &[], &projects(), &BTreeSet::new(), true).unwrap();
        assert_eq!((open.rows.len(), open.more), (8, None));
        // Nothing interrupted: no sheet.
        assert_eq!(
            summary(
                &report(&[], &[]),
                &tickets,
                &[],
                &projects(),
                &BTreeSet::new(),
                false
            ),
            None
        );
    }

    fn ended_agent(id: &str, name: &str, asked: bool) -> ProjectAgent {
        ProjectAgent {
            id: id.into(),
            project: "SHOP".into(),
            name: name.into(),
            name_source: None,
            state: AgentState::EndedByRestart,
            tool: otaq_core::agent_tools::Tool::Claude,
            tool_session_id: Some("s".into()),
            model: None,
            got_request: true,
            mid_turn: false,
            limit_stopped: false,
            created_at: 0,
            ended_at: Some(0),
            closed: false,
            question: None,
            needs_you: asked.then(|| "Allow gh issue list?".to_owned()),
            focused: false,
            tokens: Default::default(),
            session: None,
            epic: None,
            turn_ended_at: None,
        }
    }

    #[test]
    fn validating_tickets_and_project_agents_are_counted_and_listed() {
        let tickets = vec![card_fixtures::ticket(
            "SHOP-29",
            "Coupon field",
            Status::Interrupted,
        )];
        let agents = vec![
            ended_agent("a1", "Checkout slowness", false),
            ended_agent("a2", "Import GitHub issues", true),
        ];
        let mut r = report(&["SHOP-29"], &[]);
        r.was_validating = vec!["SHOP-29".parse().unwrap()];
        r.project_agents_ended = vec!["a1".into(), "a2".into()];
        r.new_agents_discarded = 1;
        let page = summary(&r, &tickets, &agents, &projects(), &BTreeSet::new(), false).unwrap();
        assert_eq!(
            page.title,
            "1 session was interrupted \u{b7} 2 Project agents ended"
        );
        assert_eq!(
            page.rows[0].detail,
            "was Validating \u{b7} Resume = fresh validation"
        );
        assert_eq!(
            page.selected,
            ["SHOP-29"],
            "agents aren't in Resume selected"
        );
        // *16 → Epic lead after a restart*: the leads are counted on their own, never in Resume selected (T-0289).
        r.leads_ended = vec!["lead-1".into()];
        let page = summary(&r, &tickets, &agents, &projects(), &BTreeSet::new(), false).unwrap();
        assert_eq!(
            page.title,
            "1 session was interrupted \u{b7} 2 Project agents ended \u{b7} 1 epic lead ended"
        );
        assert_eq!(page.selected, ["SHOP-29"]);
        assert_eq!(
            title(4, 0, 1),
            "4 sessions were interrupted \u{b7} 1 epic lead ended"
        );
        assert_eq!(
            title(4, 0, 2),
            "4 sessions were interrupted \u{b7} 2 epic leads ended"
        );
        let part = page.agents.unwrap();
        assert_eq!(
            part.title,
            "PROJECT AGENTS \u{b7} NOT IN RESUME SELECTED \u{b7} TERMINALS ARE GONE"
        );
        let rows: Vec<(&str, &str, &str, bool)> = part
            .rows
            .iter()
            .map(|a| {
                (
                    a.project.as_str(),
                    a.name.as_str(),
                    a.detail.as_str(),
                    a.resumable,
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "shop-web",
                    "Checkout slowness",
                    "Ended by restart \u{b7} same conversation",
                    true
                ),
                (
                    "shop-web",
                    "Import GitHub issues",
                    "Ended by restart \u{b7} was asking you",
                    true
                ),
            ]
        );
        assert_eq!(
            part.discarded.as_deref(),
            Some("A \u{201c}New agent\u{201d} that had no request yet is discarded, not listed.")
        );

        // Only agents ended: the sheet still opens; a resumed one says so.
        let mut only = report(&[], &[]);
        only.project_agents_ended = vec!["a1".into()];
        let mut running = ended_agent("a1", "Checkout slowness", false);
        running.state = AgentState::Running;
        let page = summary(&only, &[], &[running], &projects(), &BTreeSet::new(), false).unwrap();
        assert_eq!(page.title, "1 Project agent ended");
        let row = &page.agents.unwrap().rows[0];
        assert!(!row.resumable);
        assert_eq!(row.detail, "Resumed \u{b7} same conversation");
    }

    /// T-0490 (#58; d0045, d0063): a check the restart interrupted is a Resume row, ticked, never "keeps its
    /// question"; a ticket interrupted while it asked is in that note, its question kept for Needs you.
    #[test]
    fn an_interrupted_check_is_a_resume_row_not_a_kept_question() {
        let mut checking = card_fixtures::ticket("SHOP-15", "Coupon field", Status::Interrupted);
        checking.interrupted_from = Some(Status::Checking);
        let mut asked = card_fixtures::ticket("SHOP-22", "Login", Status::Interrupted);
        asked.interrupted_from = Some(Status::AsksYou);
        asked.question = Some(otaq_core::board::questions::HumanQuestion {
            id: "q1".into(),
            text: "Which email provider should I use?".into(),
            options: Vec::new(),
            recommended: None,
            notes: Vec::new(),
            asked_at: 0,
            via_lead: None,
        });
        let tickets = vec![checking, asked];
        let mut r = report(&["SHOP-15", "SHOP-22"], &[]);
        r.was_validating = vec!["SHOP-15".parse().unwrap()];
        let page = summary(&r, &tickets, &[], &projects(), &BTreeSet::new(), false).unwrap();
        let rows: Vec<(&str, RowState, &str, bool)> = page
            .rows
            .iter()
            .map(|r| (r.key.as_str(), r.state, r.detail.as_str(), r.checked))
            .collect();
        assert_eq!(
            rows,
            [(
                "SHOP-15",
                RowState::Interrupted,
                "was Validating \u{b7} Resume = fresh validation",
                true
            )]
        );
        assert_eq!(page.selected, ["SHOP-15"]);
        assert_eq!(
            page.notes,
            [
                "Turquoise tickets keep their question \u{2014} SHOP-22. Answering it from Needs you resumes and \
                 delivers it."
            ]
        );
        assert_eq!(
            tickets[1].question.as_ref().map(|q| q.text.as_str()),
            Some("Which email provider should I use?"),
            "kept for Needs you"
        );
    }

    /// *16 → Recovery per tool*: each row names its agent and tool with several tools; a row whose tool is logged
    /// out has Sign in… and isn't selected; Resume failed and Reattaching name the tool.
    #[test]
    fn the_sheet_resumes_per_tool() {
        use tool_problems::fixtures::{self as tf, CLAUDE, CODEX};
        let tickets = vec![
            card_fixtures::ticket("SHOP-25", "Upgrade to Next.js 15", Status::Interrupted),
            card_fixtures::ticket("SHOP-19", "Price table", Status::Interrupted),
            card_fixtures::ticket("SHOP-30", "Order history page", Status::ResumeFailed),
        ];
        let runs = Runs {
            tools: tf::codex_logged_out(),
            tickets: BTreeMap::from([
                (
                    "SHOP-25".to_owned(),
                    RanOn {
                        tool: CLAUDE,
                        agent: Some("Strong".into()),
                    },
                ),
                (
                    "SHOP-19".to_owned(),
                    RanOn {
                        tool: CODEX,
                        agent: Some("Standard".into()),
                    },
                ),
                (
                    "SHOP-30".to_owned(),
                    RanOn {
                        tool: CODEX,
                        agent: Some("Codex".into()),
                    },
                ),
            ]),
        };
        let page = summary_with(
            &report(&["SHOP-25", "SHOP-19", "SHOP-30"], &[]),
            &tickets,
            &[],
            &projects(),
            &BTreeSet::new(),
            false,
            &runs,
        )
        .unwrap();
        let [cc, cx, failed] = &page.rows[..] else {
            panic!("three rows")
        };
        assert_eq!(cc.agent.as_deref(), Some("Strong \u{b7} Claude Code"));
        assert_eq!((cc.problem.as_deref(), cc.checked), (None, true));
        assert_eq!(cx.agent.as_deref(), Some("Standard \u{b7} Codex"));
        assert_eq!(
            cx.problem.as_deref(),
            Some("Codex isn\u{2019}t available (signed out)")
        );
        assert_eq!(cx.sign_in.as_ref().unwrap().label, "Sign in\u{2026}");
        assert!(!cx.checked);
        assert_eq!(page.selected, ["SHOP-25"]);
        assert_eq!(
            page.skipped,
            ["Resume selected skips the Codex session until you sign in"]
        );
        assert_eq!(
            failed.agent.as_deref(),
            Some("Codex"),
            "the tool is the name"
        );
        assert!(failed.detail.starts_with("Codex has no saved conversation"));
        assert!(
            cx.resuming_detail
                .starts_with(&format!("Reattaching {}", CODEX.resume_hint("").trim())),
            "{}",
            cx.resuming_detail
        );
        assert!(page.text.contains("on the tool it started on"));
        // One tool connected: no agent line, as before.
        let page = summary(
            &report(&["SHOP-25"], &[]),
            &tickets,
            &[],
            &projects(),
            &BTreeSet::new(),
            false,
        )
        .unwrap();
        assert_eq!(
            (page.rows[0].agent.as_ref(), page.rows[0].problem.as_ref()),
            (None, None)
        );
        assert!(page.skipped.is_empty());
    }

    /// PAU-07: a paused project's tickets stay out of the reboot summary.
    #[test]
    fn the_summary_leaves_paused_projects_out() {
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
