//! The ticket sheet (SHT-01, SHT-02; d0087, d0095, d0097; canvas `s25-02`, `s25-03`, `s25-10`): a 1040 px sheet over
//! the board (920 px beside the agent panel), or the whole content area at full page (⤢, "← Board"). Its header is
//! the breadcrumb (project / key), the title and status, **Move…** and ⋯; under it one line (where it is and who works
//! on it) and the flow path; then the tabs **Overview · Session · Changes ·
//! History**, `1`–`4` and `g o` / `g s` / `g f` / `g a`. Changes carries its count ("Changes 3", Q421, [`FilesCounts`]).
//!
//! What the sheet shows is a [`Content`]: a tab, or one side of Session's switch (Agent, the check's agent, Run, Shell;
//! `session_tab.rs`, T-0466). Each tab's screen registers its body: Overview (`overview.rs`, T-0465), Session, Files
//! (`files_tab.rs`, T-0468) and Activity (`history.rs`, T-0466).

use otaq_core::agents::SessionRole;
use otaq_core::agents::facts::{PathKind, PathStage, TicketAgents};
use otaq_core::agents::store::SessionAgentRun;
use std::collections::{HashMap, HashSet};

use otaq_core::board::files::{FilesChanges, FilesFrom};
use otaq_core::board::merge::passed_latest;
use otaq_core::board::order::sort_column;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, Status, StatusSet, TicketKey};
use otaq_core::flow::engine::RunResult;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::agents::TOOL_DEFAULT;
use crate::view_models::board_page::{DragAnswer, Verdict};
use crate::view_models::flow_columns::Columns;
use crate::view_models::menu::MenuItem;
use crate::view_models::tool_problems::Tools;

/// The sheet's width over the board (SHT-01).
pub const WIDTH: f32 = 1040.;
/// Its width with the Project agent's panel open beside it.
pub const WIDTH_BESIDE_AGENT: f32 = 920.;
/// What it keeps clear on each side of it; a narrow window narrows the sheet.
pub const MARGIN: f32 = 18.;

/// The sheet's width over a main area `main_width` px wide: the width its left edge was dragged to (`dragged`), else
/// 1040 px, 920 px beside the agent panel; never wider than the main area less its margins.
pub fn width(beside_agent: bool, main_width: f32, dragged: Option<f32>) -> f32 {
    let wanted = dragged.unwrap_or(if beside_agent {
        WIDTH_BESIDE_AGENT
    } else {
        WIDTH
    });
    wanted.min(main_width - 2. * MARGIN).max(0.)
}

/// The sheet's tabs (SHT-02). `SheetTab` on the page, where each tab's screen registers its body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "SheetTab")]
pub enum SheetTab {
    Overview,
    Session,
    Files,
    Activity,
}

impl SheetTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Session => "Session",
            Self::Files => "Changes",
            Self::Activity => "History",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Overview => IconName::FileText,
            Self::Session => IconName::SquareTerminal,
            Self::Files => IconName::FolderOpen,
            Self::Activity => IconName::History,
        }
    }

    /// Its key's number: `1`–`4`, and `g o` / `g s` / `g f` / `g a` (KeyAction::SwitchTab).
    pub fn number(self) -> Option<u8> {
        match self {
            Self::Overview => Some(1),
            Self::Session => Some(2),
            Self::Files => Some(3),
            Self::Activity => Some(4),
        }
    }

    /// What it can show, the first by default: Session's switch is Agent, the check's agent, Run and Shell (the
    /// check only once one ran on the ticket, `session_tab::check_side`); every other tab shows itself.
    pub fn contents(self) -> &'static [Content] {
        match self {
            Self::Overview => &[Content::Overview],
            Self::Session => &[Content::Agent, Content::Check, Content::Run, Content::Shell],
            Self::Files => &[Content::Files],
            Self::Activity => &[Content::Activity],
        }
    }
}

/// What the sheet shows (SHT-02, SHT-04): a tab, or one side of Session's switch. `SheetContent` on the page, where
/// the selection names it (`run`: Session on Run) and links ask for one (a toast's "Send back to resolve →", a
/// needs-you chip).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "SheetContent")]
pub enum Content {
    Overview,
    /// Session: the ticket's agent's terminal.
    Agent,
    /// Session: the running or last check stage's terminal (Validator, Security reviewer).
    Check,
    Run,
    Shell,
    Files,
    Activity,
}

/// The sheet's tabs for `ticket`, in order: Overview · Session · Files · Activity.
pub fn tabs(_ticket: &Ticket) -> Vec<SheetTab> {
    vec![
        SheetTab::Overview,
        SheetTab::Session,
        SheetTab::Files,
        SheetTab::Activity,
    ]
}

/// The shown tab whose key is `n`.
pub fn numbered(shown: &[SheetTab], n: u8) -> Option<SheetTab> {
    shown.iter().copied().find(|t| t.number() == Some(n))
}

/// The tab the sheet opens on (SHT-02): Overview, in Review too, where it says what the agent did, what the checker
/// found, the decisions it made and the ticket; the review bar is under every tab.
pub fn opens_on(_ticket: &Ticket) -> SheetTab {
    SheetTab::Overview
}

/// One crumb of the breadcrumb: the project, the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetCrumb")]
pub struct Crumb {
    pub text: String,
    /// The key, in the mono font.
    pub key: bool,
}

/// `shop-web / SHOP-22` (SHT-01). At full page the project's crumb is the page's "← Board".
pub fn breadcrumb(ticket: &Ticket, project: &str) -> Vec<Crumb> {
    vec![
        Crumb {
            text: project.to_owned(),
            key: false,
        },
        Crumb {
            text: ticket.key.to_string(),
            key: true,
        },
    ]
}

/// What the details line is made of.
pub struct DetailFacts<'a> {
    pub ticket: &'a Ticket,
    /// The ticket's agents (`ticket.agents`); `None` when they couldn't be read.
    pub agents: Option<&'a TicketAgents>,
    pub tools: &'a Tools,
    /// The project's tickets.
    pub tickets: &'a [Ticket],
    pub now: i64,
    /// The ticket's helpers (d0133): "In charge", "Pieces 1 of 4 merged" and the tokens split.
    pub helpers: &'a [otaq_core::helpers::Helper],
    /// The ticket's tokens with the helpers' and the checks' shares (`usage.summary`).
    pub usage: Option<&'a otaq_core::usage::TicketUsage>,
}

/// "3 min", "1 h 5 min": how long, in whole minutes (at least one).
pub fn minutes_label(ms: i64) -> String {
    let minutes = (ms / 60_000).max(1);
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// "Codex · gpt-5.5", "Claude Code · tool default".
pub(crate) fn model_text(tool: otaq_core::agent_tools::Tool, model: Option<&str>) -> String {
    format!(
        "{} \u{b7} {}",
        tool.display_name(),
        model.filter(|m| !m.is_empty()).unwrap_or(TOOL_DEFAULT)
    )
}

/// The ticket's running session (the newest one running).
fn running(agents: &TicketAgents) -> Option<&SessionAgentRun> {
    agents
        .sessions
        .iter()
        .rev()
        .find(|s| s.running && s.role != SessionRole::ProjectAgent)
}

/// The stage whose agent the ticket has next: the one it is in or waits before; from Backlog the first it runs.
fn next_stage<'a>(ticket: &Ticket, path: &'a [PathStage]) -> Option<&'a PathStage> {
    let runs = |s: &&PathStage| s.agent.is_some() && !s.skipped;
    match &ticket.place {
        Place::InStage(id) | Place::Waiting(Some(id)) => path.iter().find(|s| &s.stage_id == id),
        Place::Backlog | Place::Waiting(None) => path.iter().find(runs),
        Place::Done => None,
    }
}

/// Who is on the ticket: the agent on it now (its running session), else the agent its next stage runs, else the last
/// one that ran; `None` when no one can be named.
fn assignee(f: &DetailFacts) -> Option<String> {
    let agents = f.agents?;
    if let Some(session) = running(agents) {
        return session.agent_name.clone();
    }
    if let Some(stage) = next_stage(f.ticket, &agents.path)
        && stage.kind != PathKind::HumanCheck
        && let Some(agent) = &stage.agent
    {
        return Some(agent.name.clone());
    }
    agents
        .sessions
        .last()
        .and_then(|last| last.agent_name.clone())
}

/// "Dev and Validate by Claude Code" for a ticket in your check: the stages its agents did, and their tools.
fn done_by(f: &DetailFacts) -> Option<String> {
    let agents = f.agents?;
    let stages: Vec<&str> = steps(f.ticket, Some(agents))
        .iter()
        .zip(agents.path.iter().filter(|s| s.kind != PathKind::Delivery))
        .filter(|(step, stage)| {
            step.state == StepState::Done
                && matches!(stage.kind, PathKind::Agent | PathKind::AgentCheck)
        })
        .map(|(_, stage)| stage.stage.as_str())
        .collect();
    let mut tools: Vec<&str> = Vec::new();
    for tool in agents
        .sessions
        .iter()
        .filter(|s| s.role != SessionRole::ProjectAgent)
        .filter_map(|s| s.tool)
    {
        let name = tool.display_name();
        if !tools.contains(&name) {
            tools.push(name);
        }
    }
    let list = |items: &[&str]| match items {
        [] => None,
        [one] => Some((*one).to_owned()),
        [rest @ .., last] => Some(format!("{} and {last}", rest.join(", "))),
    };
    let stages = list(&stages)?;
    Some(match list(&tools) {
        Some(tools) => format!("{stages} by {tools}"),
        None => stages,
    })
}

/// The one line under the title: where the ticket is and who works on it, in the board's words. In your check: "Dev
/// and Validate by Claude Code · 14 min · started by you"; while a stage works: "In Dev · Strong · started by you 12
/// min ago"; "Not started" in Backlog; "Done" once finished. Priority, the branch and the stage's agents are changed in
/// Edit ticket… and the ⋯ menu, not here.
pub fn subtitle(f: &DetailFacts) -> String {
    let ticket = f.ticket;
    let started = ticket.started_at;
    let ago = |at: i64| format!("{} ago", minutes_label(f.now - at));
    if ticket.place == Place::Done {
        return ticket.status.label().to_owned();
    }
    if ticket.place.is_backlog() || started.is_none() {
        return "Not started".to_owned();
    }
    let stage = ticket.place.stage().and_then(|id| {
        f.agents?
            .path
            .iter()
            .find(|s| s.stage_id == id)
            .map(|s| s.stage.clone())
    });
    let mut parts: Vec<String> = Vec::new();
    if ticket.status.set() == StatusSet::HumanCheck {
        if let Some(by) = done_by(f) {
            parts.push(by);
        }
        if let Some(at) = started {
            let worked = ticket.status_since.unwrap_or(f.now) - at;
            parts.push(minutes_label(worked));
        }
        parts.push("started by you".to_owned());
        return parts.join(" \u{b7} ");
    }
    if let Some(stage) = stage {
        parts.push(format!("In {stage}"));
    }
    if let Some(who) = assignee(f) {
        parts.push(who);
    }
    parts.push(match started {
        Some(at) => format!("started by you {}", ago(at)),
        None => "started by you".to_owned(),
    });
    parts.join(" \u{b7} ")
}

/// The Files tab's count (SHT-02, Q421): how many files the ticket changed against its base, as All changes counts
/// them; `None` when there are none or it never started ("Files").
pub fn files_count(changes: &FilesChanges) -> Option<u32> {
    if changes.source.from == FilesFrom::Base {
        return None;
    }
    u32::try_from(changes.files.len()).ok().filter(|n| *n > 0)
}

/// Whether there is anything to count: a ticket that never started has no branch of its own ("Files", no git call).
pub fn counts_files(ticket: &Ticket) -> bool {
    ticket
        .work
        .as_ref()
        .is_some_and(|w| w.branch.is_some() && w.base_sha.is_some())
}

/// The Changes tab's note: "3 files", "1 file"; none without changes.
pub fn files_count_label(count: Option<u32>) -> Option<String> {
    count.map(|n| match n {
        1 => "1 file".to_owned(),
        n => format!("{n} files"),
    })
}

/// The Files tab's counts by ticket (Q421), kept between the sheet's refreshes so a plain refresh never asks git
/// again. A count is taken when the sheet opens on the ticket, and again once its status moved on since the count
/// was taken: its agent's turn ended (Working → Idle, Needs you, the next stage), which is also when its commits land
/// (no event says "it committed" on its own). Nothing waits on a timer (d0086). The sheet never waits for a count
/// (reading a big worktree's changes takes seconds): it shows the last one while the next is taken.
#[derive(Debug, Default)]
pub struct FilesCounts {
    taken: HashMap<String, (Status, Option<u32>)>,
    /// The tickets whose count is being taken now.
    taking: HashSet<String>,
}

impl FilesCounts {
    /// The count to show without taking it again; `None` when it must be taken (the sheet just opened, never taken,
    /// or the status changed since).
    pub fn kept(&self, ticket: &Ticket, opened: bool) -> Option<Option<u32>> {
        if opened {
            return None;
        }
        self.taken
            .get(&ticket.key.to_string())
            .filter(|(status, _)| *status == ticket.status)
            .map(|(_, count)| *count)
    }

    /// The count last taken for the ticket in any status, to show while the next one is taken.
    pub fn last(&self, ticket: &Ticket) -> Option<u32> {
        self.taken
            .get(&ticket.key.to_string())
            .and_then(|(_, count)| *count)
    }

    /// Marks the ticket's count as being taken; false when it already is.
    pub fn start(&mut self, ticket: &Ticket) -> bool {
        self.taking.insert(ticket.key.to_string())
    }

    /// Keeps a count just taken for the ticket in its status now.
    pub fn keep(&mut self, ticket: &Ticket, count: Option<u32>) {
        let key = ticket.key.to_string();
        self.taking.remove(&key);
        self.taken.insert(key, (ticket.status, count));
    }
}

/// Where a stage of the flow path stands for the ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "SheetStepState")]
pub enum StepState {
    Done,
    /// The stage it is in now; `attention` when it needs you there.
    Current,
    ToCome,
    /// The ticket went past it without doing it: it skips it by its choice (AGM-08), or it is a check before where
    /// the ticket is that didn't pass in its latest run (skipped, moved past, never ran).
    Skipped,
}

/// One stage of the flow path under the details line (`s25-03`: Product spec ✓ — Dev ● — Validate — …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetStep")]
pub struct Step {
    pub stage: String,
    pub state: StepState,
    /// It needs you in the stage it is in (the turquoise ring).
    pub attention: bool,
}

/// The flow path as steps: the stages before the one it is in done, that one current, the rest to come; from
/// Backlog every stage is to come, in Done every one is done. A stage the ticket went past without doing it is
/// skipped: one it skips by its choice, and a check before it that didn't pass in its latest run (the same fact that
/// makes an automatic Delivery say "Delivered without all checks"). None for an unread flow.
pub fn steps(ticket: &Ticket, agents: Option<&TicketAgents>) -> Vec<Step> {
    let Some(agents) = agents else {
        return Vec::new();
    };
    // Delivery is how Review ends (the Approve button), not a step: the path shows the flow's stages only. A ticket
    // being delivered (To deliver, PR open) waits in the last of them.
    let path: Vec<&PathStage> = agents
        .path
        .iter()
        .filter(|s| s.kind != PathKind::Delivery)
        .collect();
    let index = |id: &str| path.iter().position(|s| s.stage_id == id);
    // Stages before `done_before` are done; `current` is the one it is in.
    let (done_before, current) = match &ticket.place {
        Place::InStage(id) => {
            index(id).map_or((path.len(), path.len().checked_sub(1)), |i| (i, Some(i)))
        }
        Place::Waiting(Some(id)) => (index(id).unwrap_or(0), None),
        // Closed without merging, it got only as far as the furthest stage it ran.
        Place::Done if ticket.status == Status::ClosedNotMerged => (
            path.iter()
                .rposition(|s| agents.runs.iter().any(|r| r.stage == s.stage_id))
                .map_or(0, |i| i + 1),
            None,
        ),
        Place::Done => (path.len(), None),
        Place::Backlog | Place::Waiting(None) => (0, None),
    };
    let closed = ticket.status == Status::ClosedNotMerged;
    path.into_iter()
        .enumerate()
        .map(|(i, stage)| {
            let check = matches!(stage.kind, PathKind::AgentCheck | PathKind::HumanCheck);
            // A closed ticket ticks only the stages it passed: an Agent stage one of whose runs finished, a check
            // whose latest run passed. The stage it was closed in is not ticked.
            let passed = || match check {
                true => passed_latest(&agents.runs, &stage.stage_id),
                false => agents
                    .runs
                    .iter()
                    .any(|r| r.stage == stage.stage_id && r.result == Some(RunResult::Done)),
            };
            let state = match () {
                _ if stage.skipped => StepState::Skipped,
                _ if Some(i) == current => StepState::Current,
                _ if closed && passed() => StepState::Done,
                _ if closed && i + 1 < done_before => StepState::Skipped,
                _ if closed => StepState::ToCome,
                _ if i < done_before && check && !passed_latest(&agents.runs, &stage.stage_id) => {
                    StepState::Skipped
                }
                _ if i < done_before => StepState::Done,
                _ => StepState::ToCome,
            };
            Step {
                stage: stage.stage.clone(),
                state,
                attention: state == StepState::Current && ticket.shows_needs_you(),
            }
        })
        .collect()
}

/// **Move…** (SHT-01): the board's columns as the drag sees them, each with the drag's verdict: its own column
/// checked, an allowed one with what the drop does, a refused one disabled with why. A pick is a drop
/// (`board.drop`), with the same dialogs.
pub fn move_menu(columns: &Columns, answer: &DragAnswer) -> Vec<MenuItem> {
    let title = |target: &str| {
        columns
            .get(target)
            .map_or_else(|| target.to_owned(), |c| c.title.clone())
    };
    answer
        .targets
        .iter()
        .map(|t| {
            let item = MenuItem::new(t.target.0.to_string(), title(&t.target.0));
            match &t.verdict {
                Verdict::Home => item.checked(true).disabled(true),
                Verdict::Allowed { hint, .. } => item.hint(hint.clone()),
                Verdict::Forbidden { title, .. } => item.disabled(true).hint(title.clone()),
            }
        })
        .collect()
}

/// The board column id the ticket shows in (the drag's own column).
pub fn column_of(columns: &Columns, ticket: &Ticket) -> String {
    columns.column_of(ticket).id.clone()
}

/// The header's and the tab bar's words.
pub const MOVE: &str = "Move\u{2026}";
pub const FULL_PAGE: &str = "Full page";
pub const BACK_TO_SHEET: &str = "Back to the sheet";
/// At full page, the breadcrumb's first crumb goes back to the sheet over the board (`s25-10`).
pub const BOARD: &str = "Board";

// What the sheet keeps of the old side panel (UI-04; d0058): previous / next ticket in the column's order (j / k,
// ⌘[ / ⌘]), the Start interrupted callout and the branch the header shows.

/// The ticket before (`-1`) or after (`+1`) `current` in its column's order (both Dev sections, top to bottom).
pub fn neighbour(tickets: &[Ticket], current: &TicketKey, step: i32) -> Option<TicketKey> {
    let ticket = tickets.iter().find(|t| &t.key == current)?;
    let order = sort_column(ticket.status.set(), tickets);
    let keys: Vec<&TicketKey> = order.cards.iter().collect();
    let at = keys.iter().position(|k| *k == current)? as i32;
    let next = at + step;
    (next >= 0 && (next as usize) < keys.len()).then(|| keys[next as usize].clone())
}

/// "The start didn’t finish" above a Start interrupted ticket's tab (*16 → Start interrupted · SHOP-28 sheet*,
/// T-0063): the failed step and its message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartProblem {
    pub heading: String,
    pub message: String,
}

/// What Retry start does, under the callout.
pub const RETRY_START_DOES: [&str; 3] = [
    "Runs the whole start again, the same way as dragging to Dev",
    "Starts a new agent session, so it uses tokens",
    "Leaves the ticket and its dependencies as they are",
];

pub fn start_problem(ticket: &Ticket) -> Option<StartProblem> {
    if ticket.status != otaq_core::domain::Status::StartInterrupted {
        return None;
    }
    Some(match &ticket.start_problem {
        Some(p) => StartProblem {
            heading: format!("Start failed: {}", p.step.failure_label()),
            message: p.message.clone(),
        },
        None => StartProblem {
            heading: "Start failed".to_owned(),
            message: format!(
                "{} stopped before its agent session started, so there is nothing to resume.",
                ticket.key
            ),
        },
    })
}

/// A long branch name shortened in the middle, keeping its start (with the ticket number) and its end:
/// `otaq/SHOP-25-upgr…-app-router`.
pub fn middle_truncate(text: &str, max_chars: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max_chars || max_chars < 5 {
        return text.to_owned();
    }
    // Two thirds for the start (prefix and ticket number), one third for the end.
    let keep = max_chars - 1;
    let tail = keep / 3;
    let head = keep - tail;
    let mut out: String = chars[..head].iter().collect();
    out.push('…');
    out.extend(&chars[chars.len() - tail..]);
    out
}

/// The branch the header shows: the stored one once started, else the temporary name it will get.
pub fn branch_name(ticket: &Ticket) -> Option<String> {
    let started = ticket.has_started() || !ticket.place.is_backlog();
    ticket
        .work
        .as_ref()
        .and_then(|w| w.branch.clone())
        .or_else(|| started.then(|| otaq_core::naming::branch(&ticket.key.to_string())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use crate::view_models::stage_choices::fixtures::{
        CODEX, session, shop_options, shop_path, tools,
    };
    use otaq_core::domain::{Priority, Status};

    const MIN: i64 = 60_000;

    fn agents_of(t: &Ticket, sessions: Vec<SessionAgentRun>) -> TicketAgents {
        crate::view_models::stage_choices::fixtures::ticket(
            &t.key.to_string(),
            t.status,
            t.place.clone(),
            shop_path("rust"),
            shop_options(),
            Vec::new(),
            sessions,
        )
    }

    /// SHOP-22 "Fix flaky login redirect" asks to run a command in Dev, Rust developer on Codex · gpt-5.5 for 3
    /// minutes and 64k tokens (`s25-03`).
    fn shop_22() -> Ticket {
        let mut t = ticket("SHOP-22", "Fix flaky login redirect", Status::NeedsYou);
        t.place = Place::InStage("dev".into());
        t.priority = Priority::High;
        t.flags.tokens = Some(64_000);
        t.work = Some(otaq_core::board::tickets::Work {
            branch: Some(otaq_core::naming::branch("SHOP-22")),
            ..Default::default()
        });
        t
    }

    #[test]
    fn the_breadcrumb_for_shop_22() {
        let t = shop_22();
        let crumbs = breadcrumb(&t, "shop-web");
        let texts: Vec<_> = crumbs.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["shop-web", "SHOP-22"]);
        assert!(crumbs[1].key && !crumbs[0].key);
    }

    /// The one line under the title, in the board's words: where it is, who works on it, since when; in your check,
    /// which stages its agents did and on which tool.
    #[test]
    fn the_line_under_the_title() {
        let mut t = shop_22();
        t.started_at = Some(100 * MIN);
        let mut dev = session(
            "s1",
            SessionRole::Developer,
            "Rust developer",
            CODEX,
            None,
            Some("gpt-5.5"),
        );
        dev.running = true;
        let agents = agents_of(&t, vec![dev.clone()]);
        let tools = tools();
        let facts = |t: &Ticket, agents: &TicketAgents| {
            subtitle(&DetailFacts {
                ticket: t,
                agents: Some(agents),
                tools: &tools,
                tickets: &[],
                now: 112 * MIN,
                helpers: &[],
                usage: None,
            })
        };
        assert_eq!(
            facts(&t, &agents),
            "In Dev \u{b7} Rust developer \u{b7} started by you 12 min ago"
        );
        let mut review = t.clone();
        review.status = Status::WaitingForYou;
        review.place = Place::InStage("review".into());
        review.status_since = Some(114 * MIN);
        dev.running = false;
        let agents = agents_of(&review, vec![dev]);
        let line = facts(&review, &agents);
        assert!(
            line.ends_with("\u{b7} 14 min \u{b7} started by you"),
            "{line}"
        );
        assert!(line.contains(" by Codex"), "{line}");
        let backlog = ticket("SHOP-41", "Fix README typo", Status::Backlog);
        assert_eq!(
            facts(&backlog, &agents_of(&backlog, Vec::new())),
            "Not started"
        );
    }

    /// SHOP-22's path (`s25-03`): Product spec done, Dev current and needing you, the rest to come.
    #[test]
    fn the_flow_path_as_steps() {
        let t = shop_22();
        let agents = agents_of(&t, Vec::new());
        let steps = steps(&t, Some(&agents));
        let shown: Vec<(&str, StepState, bool)> = steps
            .iter()
            .map(|s| (s.stage.as_str(), s.state, s.attention))
            .collect();
        assert_eq!(
            shown,
            [
                ("Product spec", StepState::Done, false),
                ("Dev", StepState::Current, true),
                ("Validate", StepState::ToCome, false),
                ("Security", StepState::ToCome, false),
                ("Review", StepState::ToCome, false),
            ]
        );
        let backlog = ticket("SHOP-41", "Fix README typo", Status::Backlog);
        let agents = agents_of(&backlog, Vec::new());
        assert!(
            super::steps(&backlog, Some(&agents))
                .iter()
                .all(|s| s.state == StepState::ToCome)
        );
        assert!(super::steps(&backlog, None).is_empty());
    }

    /// A stage the ticket went past without doing it is skipped: Product spec it skips by its choice, Validate it
    /// was moved past after a fail, Security it skipped while it ran. Delivery is no step: a ticket to deliver waits
    /// in Review, the last stage.
    #[test]
    fn stages_gone_past_without_doing_them_are_skipped() {
        use crate::view_models::stage_choices::fixtures::{round, run};
        use otaq_core::flow::engine::RunResult;
        // To deliver in Review, the last stage: every other stage is behind it.
        let mut t = ticket("SHOP-30", "Saved carts", Status::ToDeliver);
        t.place = Place::InStage("review".into());
        let check = |stage: &str, n: u32, result: RunResult| {
            round(run(stage, None, None, None, Some(result)), n, None)
        };
        let runs = vec![
            run("dev", None, None, None, Some(RunResult::Done)),
            check("validate", 1, RunResult::Failed),
            check("security", 1, RunResult::Skipped),
            check("review", 1, RunResult::Failed),
            check("review", 2, RunResult::Passed),
        ];
        let mut path = shop_path("rust");
        path[0].skipped = true;
        let agents = crate::view_models::stage_choices::fixtures::ticket(
            "SHOP-30",
            t.status,
            t.place.clone(),
            path,
            shop_options(),
            runs,
            Vec::new(),
        );
        let steps = steps(&t, Some(&agents));
        let shown: Vec<(&str, StepState)> =
            steps.iter().map(|s| (s.stage.as_str(), s.state)).collect();
        assert_eq!(
            shown,
            [
                ("Product spec", StepState::Skipped),
                ("Dev", StepState::Done),
                ("Validate", StepState::Skipped),
                ("Security", StepState::Skipped),
                ("Review", StepState::Current),
            ]
        );
    }

    /// A closed ticket ticks only the stages it passed: closed from Product spec, nothing is ticked; closed in
    /// Security after Dev finished and Validate passed, those two are, and the stages it never reached are to come.
    #[test]
    fn a_closed_ticket_ticks_only_the_stages_it_passed() {
        use crate::view_models::stage_choices::fixtures::{round, run};
        let mut t = ticket(
            "NOTE-15",
            "Retest3 codex spec park",
            Status::ClosedNotMerged,
        );
        t.place = Place::Done;
        let shown = |t: &Ticket, runs: Vec<_>, skip_first: bool| {
            let mut path = shop_path("rust");
            path[0].skipped = skip_first;
            let agents = crate::view_models::stage_choices::fixtures::ticket(
                "NOTE-15",
                t.status,
                t.place.clone(),
                path,
                shop_options(),
                runs,
                Vec::new(),
            );
            steps(t, Some(&agents))
                .into_iter()
                .map(|s| (s.stage, s.state))
                .collect::<Vec<_>>()
        };
        let all = |state| {
            ["Product spec", "Dev", "Validate", "Security", "Review"]
                .map(|s| (s.to_owned(), state))
                .to_vec()
        };
        // Closed from Product spec: its run was stopped.
        let spec = vec![run(
            "product-spec",
            None,
            None,
            None,
            Some(RunResult::Cancelled),
        )];
        assert_eq!(shown(&t, spec, false), all(StepState::ToCome));
        // Never started.
        assert_eq!(shown(&t, Vec::new(), false), all(StepState::ToCome));
        // Closed in Security, past Dev and a passed Validate; Product spec it skipped by its choice.
        let check = |stage: &str, n: u32, result: RunResult| {
            round(run(stage, None, None, None, Some(result)), n, None)
        };
        let runs = vec![
            run("dev", None, None, None, Some(RunResult::Done)),
            check("validate", 1, RunResult::Passed),
            check("security", 1, RunResult::Cancelled),
        ];
        assert_eq!(
            shown(&t, runs, true),
            [
                ("Product spec".to_owned(), StepState::Skipped),
                ("Dev".to_owned(), StepState::Done),
                ("Validate".to_owned(), StepState::Done),
                ("Security".to_owned(), StepState::ToCome),
                ("Review".to_owned(), StepState::ToCome),
            ]
        );
        // Merged, every stage it went through is done.
        let mut merged = t.clone();
        merged.status = Status::Merged;
        let runs = vec![
            run("dev", None, None, None, Some(RunResult::Done)),
            check("validate", 1, RunResult::Passed),
            check("security", 1, RunResult::Passed),
            check("review", 1, RunResult::Passed),
        ];
        assert_eq!(shown(&merged, runs, false)[1..], all(StepState::Done)[1..]);
    }

    #[test]
    fn minutes() {
        assert_eq!(minutes_label(10_000), "1 min");
        assert_eq!(minutes_label(3 * MIN + 59_000), "3 min");
        assert_eq!(minutes_label(60 * MIN), "1 h");
        assert_eq!(minutes_label(65 * MIN), "1 h 5 min");
    }

    /// Tabs: a ticket's four, numbered 1–4; Overview on open; what each one shows.
    #[test]
    fn tabs_their_numbers_and_the_tab_on_open() {
        let t = ticket("SHOP-22", "Fix flaky login redirect", Status::Working);
        let shown = tabs(&t);
        assert_eq!(
            shown,
            [
                SheetTab::Overview,
                SheetTab::Session,
                SheetTab::Files,
                SheetTab::Activity
            ]
        );
        let numbers: Vec<Option<u8>> = shown.iter().map(|t| t.number()).collect();
        assert_eq!(numbers, [Some(1), Some(2), Some(3), Some(4)]);
        assert_eq!(numbered(&shown, 4), Some(SheetTab::Activity));
        assert_eq!(numbered(&shown, 5), None);
        assert_eq!(opens_on(&t), SheetTab::Overview);
        // In Review too: Overview says what the agent did; the review bar is under every tab.
        assert_eq!(
            opens_on(&ticket("SHOP-18", "Checkout API", Status::WaitingForYou)),
            SheetTab::Overview
        );
        let labels: Vec<_> = shown.iter().map(|t| t.label()).collect();
        assert_eq!(labels, ["Overview", "Session", "Changes", "History"]);
        assert_eq!(
            SheetTab::Session.contents(),
            [Content::Agent, Content::Check, Content::Run, Content::Shell]
        );
        assert_eq!(SheetTab::Files.contents(), [Content::Files]);
        assert_eq!(SheetTab::Activity.contents(), [Content::Activity]);
    }

    /// j / k follow the column's order (pinned first), as the panel's did (`neighbour`).
    #[test]
    fn j_and_k_follow_the_column_order() {
        let mut a = ticket("SHOP-1", "a", Status::Backlog);
        a.created_at = 1;
        let mut b = ticket("SHOP-2", "b", Status::Backlog);
        b.created_at = 2;
        b.flags.pinned_at = Some(10);
        let tickets = vec![a, b];
        let key = |s: &str| s.parse::<otaq_core::domain::TicketKey>().unwrap();
        assert_eq!(neighbour(&tickets, &key("SHOP-2"), 1), Some(key("SHOP-1")));
        assert_eq!(neighbour(&tickets, &key("SHOP-1"), -1), Some(key("SHOP-2")));
    }

    fn changed(path: &str) -> otaq_core::board::files::ChangedEntry {
        otaq_core::board::files::ChangedEntry {
            file: otaq_core::git::diff::ChangedFile {
                status: otaq_core::git::diff::FileStatus::Modified,
                path: path.into(),
                old_path: None,
                added: 1,
                removed: 0,
                binary: false,
                size: None,
                old_size: None,
                similarity: None,
            },
            uncommitted: false,
            tags: Vec::new(),
        }
    }

    fn changes(from: FilesFrom, files: Vec<otaq_core::board::files::ChangedEntry>) -> FilesChanges {
        FilesChanges {
            source: otaq_core::board::files::FilesSource {
                key: "SHOP-22".parse().unwrap(),
                from,
                branch: otaq_core::naming::branch("SHOP-22"),
                base_branch: "main".into(),
                base_sha: Some("a1b2c3d4e5".into()),
                base_short: Some("a1b2c3d".into()),
            },
            files,
            added: 0,
            removed: 0,
        }
    }

    /// Q421: "Files 3" counts the ticket's own changed files; none changed, or never started, reads "Files".
    #[test]
    fn the_files_count_and_its_label() {
        let three = changes(
            FilesFrom::Worktree,
            vec![
                changed("e2e/login.spec.ts"),
                changed("src/app/login/page.tsx"),
                changed("src/middleware.ts"),
            ],
        );
        assert_eq!(files_count(&three), Some(3));
        assert_eq!(
            files_count_label(files_count(&three)).as_deref(),
            Some("3 files")
        );
        assert_eq!(files_count_label(Some(1)).as_deref(), Some("1 file"));
        assert_eq!(files_count(&changes(FilesFrom::Branch, Vec::new())), None);
        assert_eq!(files_count_label(None), None);
        assert_eq!(files_count(&changes(FilesFrom::Base, Vec::new())), None);
        // Never started: nothing to count, so no git call at all.
        let backlog = ticket("SHOP-41", "Fix README typo", Status::Backlog);
        assert!(!counts_files(&backlog));
        let mut started = shop_22();
        started.work = Some(otaq_core::board::tickets::Work {
            branch: Some(otaq_core::naming::branch("SHOP-22")),
            base_sha: Some("a1b2c3d4e5".into()),
            ..Default::default()
        });
        assert!(counts_files(&started));
    }

    /// Q421: the count is taken when the sheet opens and again once the agent's turn ended (its status moved on), never
    /// on a plain refresh. `taken` counts the diffs computed, as `sheet.view` takes them.
    #[test]
    fn the_files_count_is_taken_on_open_and_after_a_turn_ends_only() {
        let mut counts = FilesCounts::default();
        let mut taken = 0;
        let mut on_disk = 3;
        let mut refresh = |counts: &mut FilesCounts, t: &Ticket, opened: bool, on_disk: u32| {
            counts.kept(t, opened).unwrap_or_else(|| {
                taken += 1;
                let count = Some(on_disk);
                counts.keep(t, count);
                count
            })
        };
        let mut t = shop_22();
        t.status = Status::Working;
        assert_eq!(refresh(&mut counts, &t, true, on_disk), Some(3), "on open");
        // The agent edits two more files while it works: plain refreshes (tokens, sessions) keep the count.
        on_disk = 5;
        for _ in 0..4 {
            assert_eq!(refresh(&mut counts, &t, false, on_disk), Some(3));
        }
        // Its turn ends: Working → Idle.
        t.status = Status::Idle;
        assert_eq!(
            refresh(&mut counts, &t, false, on_disk),
            Some(5),
            "after the turn"
        );
        assert_eq!(refresh(&mut counts, &t, false, on_disk), Some(5));
        // Opening the sheet again takes it again.
        on_disk = 6;
        assert_eq!(
            refresh(&mut counts, &t, true, on_disk),
            Some(6),
            "opened again"
        );
        drop(refresh);
        assert_eq!(taken, 3, "on open, after the turn ended, on open again");
    }

    #[test]
    fn widths() {
        assert_eq!(width(false, 1376., None), WIDTH);
        assert_eq!(width(true, 956., None), WIDTH_BESIDE_AGENT);
        assert_eq!(
            width(true, 896., None),
            896. - 36.,
            "a narrow main area narrows it"
        );
        // A dragged width replaces both defaults, inside the same margins.
        assert_eq!(width(false, 1376., Some(700.)), 700.);
        assert_eq!(width(true, 956., Some(700.)), 700.);
        assert_eq!(width(false, 1376., Some(1600.)), 1376. - 36.);
    }

    /// Move… lists the drag's targets with their verdicts.
    #[test]
    fn move_lists_the_columns_with_the_drags_verdicts() {
        let flow = &otaq_core::flow::default_file().flow;
        let columns = Columns::of(flow);
        let t = ticket("SHOP-41", "Fix README typo", Status::Backlog);
        let answer = crate::view_models::board_page::drag_answer(&t, &[]);
        let menu = move_menu(&columns, &answer);
        let labels: Vec<_> = menu.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels.first(), Some(&"Backlog"));
        assert!(
            !labels.contains(&"Ready for Dev"),
            "no Ready for Dev column"
        );
        assert!(menu[0].checked && menu[0].disabled, "its own column");
        let dev = menu.iter().find(|i| i.id == "dev").unwrap();
        assert!(!dev.disabled, "Backlog → Dev starts it");
        assert_eq!(column_of(&columns, &t), "backlog");
    }

    #[test]
    fn previous_and_next_follow_the_column_order() {
        let mut a = ticket("SHOP-1", "a", Status::Backlog);
        a.created_at = 1;
        let mut b = ticket("SHOP-2", "b", Status::Backlog);
        b.created_at = 2;
        let mut c = ticket("SHOP-3", "c", Status::Backlog);
        c.created_at = 3;
        c.flags.pinned_at = Some(10);
        let other = ticket("SHOP-4", "d", Status::Working);
        let tickets = vec![a, b, c, other];
        let key = |s: &str| s.parse::<TicketKey>().unwrap();
        // Pinned SHOP-3 is first, then SHOP-1, SHOP-2.
        assert_eq!(neighbour(&tickets, &key("SHOP-1"), -1), Some(key("SHOP-3")));
        assert_eq!(neighbour(&tickets, &key("SHOP-1"), 1), Some(key("SHOP-2")));
        assert_eq!(
            neighbour(&tickets, &key("SHOP-2"), 1),
            None,
            "stays in its column"
        );
        assert_eq!(neighbour(&tickets, &key("SHOP-3"), -1), None);
        assert_eq!(neighbour(&tickets, &key("SHOP-4"), 1), None);
    }

    #[test]
    fn neighbours_stay_in_the_column() {
        let mut working = ticket("SHOP-1", "w", Status::Working);
        working.created_at = 5;
        let mut queued = ticket("SHOP-2", "q", Status::Queued);
        queued.created_at = 1;
        let mut waiting = ticket("SHOP-3", "s", Status::WaitingForSlot);
        waiting.created_at = 2;
        let tickets = vec![working, queued, waiting];
        let key = |s: &str| s.parse::<TicketKey>().unwrap();
        assert_eq!(
            neighbour(&tickets, &key("SHOP-1"), 1),
            None,
            "the tickets waiting to begin are their own group"
        );
        assert_eq!(neighbour(&tickets, &key("SHOP-2"), 1), Some(key("SHOP-3")));
        assert_eq!(neighbour(&tickets, &key("SHOP-3"), -1), Some(key("SHOP-2")));
    }

    #[test]
    fn a_start_interrupted_ticket_says_which_step_failed() {
        let mut t = ticket("SHOP-28", "Cart badge count", Status::StartInterrupted);
        let p = start_problem(&t).unwrap();
        assert_eq!(p.heading, "Start failed");
        assert!(p.message.starts_with("SHOP-28 stopped before"));
        t.start_problem = Some(otaq_core::board::tickets::StartProblem {
            step: otaq_core::domain::StartStep::SyncBase,
            message: "main has diverged".into(),
        });
        let p = start_problem(&t).unwrap();
        assert!(p.heading.starts_with("Start failed: "));
        assert_eq!(p.message, "main has diverged");
        assert_eq!(start_problem(&ticket("SHOP-1", "t", Status::Working)), None);
    }

    #[test]
    fn long_branches_shorten_in_the_middle() {
        let name = "feature/SHOP-25-upgrade-next-15-app-router";
        let short = middle_truncate(name, 28);
        assert_eq!(short.chars().count(), 28);
        assert!(short.starts_with("feature/SHOP-25"));
        assert!(short.ends_with("router"));
        assert!(short.contains('…'));
        assert_eq!(middle_truncate("feature/SHOP-1", 28), "feature/SHOP-1");
    }

    #[test]
    fn branch_shown_in_the_header() {
        let backlog = ticket("SHOP-30", "New", Status::Backlog);
        assert_eq!(branch_name(&backlog), None);
        let queued = ticket("SHOP-18", "Checkout API", Status::Queued);
        assert_eq!(
            branch_name(&queued),
            Some(otaq_core::naming::branch("SHOP-18"))
        );
    }
}
