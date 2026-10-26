//! The ticket sheet (SHT-01, SHT-02; d0087, d0095, d0097; canvas `s25-02`, `s25-03`, `s25-10`): a 1040 px sheet over
//! the board (920 px beside the agent panel), or the whole content area at full page (⤢, "← Board"). Its header is
//! the breadcrumb (project / key), the title and status, **Move…** and ⋯; under it the **details line** (Assignee,
//! Model, Working, Priority, Branch, External ID) and the flow path; then the tabs **Overview · Session · Files ·
//! Activity**, `1`–`4` and `g o` / `g s` / `g f` / `g a`. Files carries its count ("Files 3", Q421, [`FilesCounts`]).
//!
//! What the sheet shows is a [`Content`]: a tab, or one side of Session's switch (Agent, the check's agent, Run, Shell;
//! `session_tab.rs`, T-0466). Each tab's screen registers its body: Overview (`overview.rs`, T-0465), Session, Files
//! (`files_tab.rs`, T-0468) and Activity (`history.rs`, T-0466).

use otaq_core::agents::facts::{PathKind, PathStage, TicketAgents};
use otaq_core::agents::store::SessionAgentRun;
use otaq_core::agents::{Agent, SessionRole};
use std::collections::HashMap;

use otaq_core::board::files::{FilesChanges, FilesFrom};
use otaq_core::board::order::sort_column;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, Status, StatusSet, TicketKey};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::agents::TOOL_DEFAULT;
use crate::view_models::board_page::{DragAnswer, Verdict};
use crate::view_models::card::tokens_label;
use crate::view_models::flow_columns::Columns;
use crate::view_models::menu::MenuItem;
use crate::view_models::priority;
use crate::view_models::stage_choices;
use crate::view_models::tool_avatar::ToolAvatarView;
use crate::view_models::tool_problems::Tools;

/// The sheet's width over the board (SHT-01).
pub const WIDTH: f32 = 1040.;
/// Its width with the Project agent's panel open beside it.
pub const WIDTH_BESIDE_AGENT: f32 = 920.;
/// What it keeps clear on each side of it; a narrow window narrows the sheet.
pub const MARGIN: f32 = 18.;

/// The sheet's width over a main area `main_width` px wide: 1040 px, 920 px beside the agent panel, never wider than
/// the main area less its margins.
pub fn width(beside_agent: bool, main_width: f32) -> f32 {
    let wanted = if beside_agent {
        WIDTH_BESIDE_AGENT
    } else {
        WIDTH
    };
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
            Self::Files => "Files",
            Self::Activity => "Activity",
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

/// The tab the sheet opens on (SHT-02): Overview, and Files (on All changes) while the ticket waits in your check
/// (SHT-06) and has the tab.
pub fn opens_on(ticket: &Ticket) -> SheetTab {
    if super::files_tab::waits_for_you(ticket) && tabs(ticket).contains(&SheetTab::Files) {
        SheetTab::Files
    } else {
        SheetTab::Overview
    }
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

/// What a click on a detail does.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "SheetDetailChange")]
pub enum DetailChange {
    /// Priority's menu (`sheet.priority`).
    Priority { items: Vec<MenuItem> },
    /// The stage's agent before it starts (`sheet.set_choice`, AGM-08).
    Choice { stage: String, items: Vec<MenuItem> },
    /// The External ID (Q416): a click copies `text`; the tooltip then says `copied`.
    Copy {
        text: String,
        hint: &'static str,
        copied: &'static str,
    },
}

/// One item of the details line: "Assignee  CX Rust developer", "Branch  otaq/SHOP-22 from main".
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "SheetDetail")]
pub struct Detail {
    pub label: &'static str,
    pub value: String,
    /// The tool's avatar, with several tools connected (SHL-08).
    pub avatar: Option<ToolAvatarView>,
    /// The working dot before the value.
    pub working: bool,
    /// The value in the mono font (the branch).
    pub mono: bool,
    /// Muted text after the value: "from main".
    pub note: Option<String>,
    /// Clickable where the value may be changed.
    pub change: Option<DetailChange>,
}

impl Detail {
    fn new(label: &'static str, value: impl Into<String>) -> Self {
        Self {
            label,
            value: value.into(),
            avatar: None,
            working: false,
            mono: false,
            note: None,
            change: None,
        }
    }
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

const DASH: &str = "\u{2014}";

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

/// Assignee and Model: the agent on it now (its running session), you in your check, else the agent its next stage
/// runs (changeable until that stage starts), else the last one that ran.
fn assignee(f: &DetailFacts) -> (Detail, Detail) {
    let ticket = f.ticket;
    let empty = || (Detail::new("Assignee", DASH), Detail::new("Model", DASH));
    if ticket.status.set() == StatusSet::HumanCheck {
        return (Detail::new("Assignee", "You"), Detail::new("Model", DASH));
    }
    let Some(agents) = f.agents else {
        return empty();
    };
    if let Some(session) = running(agents) {
        let model = session.model_ran.as_deref().or(session.model.as_deref());
        return (
            Detail {
                avatar: session.tool.and_then(|t| f.tools.avatar(t, model)),
                ..Detail::new(
                    "Assignee",
                    session
                        .agent_name
                        .clone()
                        .unwrap_or_else(|| DASH.to_owned()),
                )
            },
            Detail::new(
                "Model",
                session
                    .tool
                    .map_or_else(|| DASH.to_owned(), |t| model_text(t, model)),
            ),
        );
    }
    let of_agent = |agent: &Agent, change: Option<DetailChange>| {
        let on = agent.model.on(f.tools.default_model.as_ref());
        (
            Detail {
                avatar: on
                    .as_ref()
                    .and_then(|m| f.tools.avatar(m.tool, m.model.as_deref())),
                change,
                ..Detail::new("Assignee", agent.name.clone())
            },
            Detail::new(
                "Model",
                on.map_or_else(
                    || DASH.to_owned(),
                    |m| model_text(m.tool, m.model.as_deref()),
                ),
            ),
        )
    };
    if let Some(stage) = next_stage(ticket, &agents.path)
        && stage.kind != PathKind::HumanCheck
        && let Some(agent) = &stage.agent
    {
        // Its agent may be changed until the stage starts (AGM-08): the stage's choice, as the Choices picker.
        let change = stage_choices::sheet_choices(agents)
            .and_then(|c| c.rows.into_iter().find(|r| r.stage_id == stage.stage_id))
            .filter(|r| !r.items.is_empty() && r.lock.is_none())
            .map(|r| DetailChange::Choice {
                stage: r.stage_id,
                items: r.items,
            });
        return of_agent(agent, change);
    }
    match agents.sessions.last() {
        Some(last) => {
            let model = last.model_ran.as_deref().or(last.model.as_deref());
            (
                Detail {
                    avatar: last.tool.and_then(|t| f.tools.avatar(t, model)),
                    ..Detail::new(
                        "Assignee",
                        last.agent_name.clone().unwrap_or_else(|| DASH.to_owned()),
                    )
                },
                Detail::new(
                    "Model",
                    last.tool
                        .map_or_else(|| DASH.to_owned(), |t| model_text(t, model)),
                ),
            )
        }
        None => empty(),
    }
}

/// Working: "3 min · 64k tokens" while its agent runs, "Sent back · waiting for the agent to pick it up" while it hasn't
/// picked up a hand-off (d0118), "waiting for you · 12 min" in your check; nothing otherwise.
fn working(f: &DetailFacts) -> Option<Detail> {
    let ticket = f.ticket;
    if ticket.status.set() == StatusSet::HumanCheck {
        let since = ticket.status_since.unwrap_or(ticket.updated_at);
        return Some(Detail::new(
            "Working",
            format!("waiting for you \u{b7} {}", minutes_label(f.now - since)),
        ));
    }
    // A hand-off the agent hasn't picked up: the card's words, without the working dot (d0118).
    if let Some(kind) = super::card::pending_hand_off(ticket, ticket.status) {
        return Some(Detail::new("Working", super::card::hand_off_line(kind)));
    }
    let session = f.agents.and_then(running)?;
    let time = minutes_label(f.now - session.started_at);
    let value = match ticket.flags.tokens {
        Some(tokens) => format!("{time} \u{b7} {} tokens", tokens_label(tokens)),
        None => time,
    };
    Some(Detail {
        working: true,
        ..Detail::new("Working", value)
    })
}

/// Branch: "otaq/SHOP-22 from main".
fn branch(ticket: &Ticket) -> Detail {
    Detail {
        mono: true,
        note: Some(format!("from {}", ticket.base.branch())),
        ..Detail::new("Branch", branch_name(ticket).unwrap_or_default())
    }
}

/// The External ID detail's words (Q416).
pub const EXTERNAL_ID: &str = "External ID";
pub const COPY_EXTERNAL_ID: &str = "Copy External ID";
pub const COPIED: &str = "Copied";

/// External ID "PAY-123" (Q416), only when the ticket has one: a click copies it.
fn external_id(ticket: &Ticket) -> Option<Detail> {
    let id = ticket
        .external_id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())?;
    Some(Detail {
        mono: true,
        change: Some(DetailChange::Copy {
            text: id.to_owned(),
            hint: COPY_EXTERNAL_ID,
            copied: COPIED,
        }),
        ..Detail::new(EXTERNAL_ID, id)
    })
}

/// The details line (SHT-01): Assignee, Model (not once it is Done), Working (while it works or waits for you),
/// Priority, Branch and the External ID when set.
pub fn details(f: &DetailFacts) -> Vec<Detail> {
    let ticket = f.ticket;
    let priority = Detail {
        change: Some(DetailChange::Priority {
            items: priority::menu_items(ticket.priority),
        }),
        ..Detail::new("Priority", priority::view(ticket.priority).short)
    };
    // A finished ticket has no one on it: no Assignee (nor its Model), not the last agent that ran (#36).
    let (who, model) = if ticket.place == Place::Done {
        (None, None)
    } else {
        let (who, model) = assignee(f);
        (Some(who), Some(model))
    };
    // A ticket whose build stage split its work (d0133, `s30-03`): "In charge", its pieces and its tokens split.
    let pieces = crate::view_models::pieces::Facts::new(ticket, f.helpers);
    let who = who.map(|who| match pieces.split() {
        true => Detail {
            label: crate::view_models::pieces::IN_CHARGE_LABEL,
            ..who
        },
        false => who,
    });
    let split = crate::view_models::pieces::pieces_detail(&pieces)
        .map(|merged| Detail::new("Pieces", merged))
        .into_iter()
        .chain(
            f.usage
                .filter(|_| pieces.split())
                .and_then(|u| Some((u.tokens?, crate::view_models::pieces::tokens_split(u)?)))
                .map(|(total, split)| Detail {
                    note: Some(split),
                    ..Detail::new("Tokens", tokens_label(total.shown()))
                }),
        );
    who.into_iter()
        .chain(model)
        .chain(working(f))
        .chain([priority, branch(ticket)])
        .chain(split)
        .chain(external_id(ticket))
        .collect()
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

/// The Files tab's note: "3"; none without changes.
pub fn files_count_label(count: Option<u32>) -> Option<String> {
    count.map(|n| n.to_string())
}

/// The Files tab's counts by ticket (Q421), kept between the sheet's refreshes so a plain refresh never asks git
/// again. A count is taken when the sheet opens on the ticket, and again once its status moved on since the count
/// was taken: its agent's turn ended (Working → Idle, Needs you, the next stage), which is also when its commits land
/// (no event says "it committed" on its own). Nothing waits on a timer (d0086).
#[derive(Debug, Default)]
pub struct FilesCounts {
    taken: HashMap<String, (Status, Option<u32>)>,
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

    /// Keeps a count just taken for the ticket in its status now.
    pub fn keep(&mut self, ticket: &Ticket, count: Option<u32>) {
        self.taken
            .insert(ticket.key.to_string(), (ticket.status, count));
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
    /// The ticket skips it (its choice, AGM-08).
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
/// Backlog every stage is to come, in Done every one is done. None for an unread flow.
pub fn steps(ticket: &Ticket, agents: Option<&TicketAgents>) -> Vec<Step> {
    let Some(agents) = agents else {
        return Vec::new();
    };
    let index = |id: &str| agents.path.iter().position(|s| s.stage_id == id);
    // Stages before `done_before` are done; `current` is the one it is in.
    let (done_before, current) = match &ticket.place {
        Place::InStage(id) => index(id).map_or((0, None), |i| (i, Some(i))),
        Place::Waiting(Some(id)) => (index(id).unwrap_or(0), None),
        Place::Done => (agents.path.len(), None),
        Place::Backlog | Place::Waiting(None) => (0, None),
    };
    agents
        .path
        .iter()
        .enumerate()
        .map(|(i, stage)| {
            let state = match () {
                _ if stage.skipped => StepState::Skipped,
                _ if Some(i) == current => StepState::Current,
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
            .iter()
            .find_map(|c| {
                if c.id == target {
                    Some(c.title.clone())
                } else {
                    c.parts
                        .iter()
                        .skip(1)
                        .find(|p| c.has_sections() && p.id == target)
                        .map(|p| p.stage.name.clone())
                }
            })
            .unwrap_or_else(|| target.to_owned())
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
        CLAUDE, CODEX, session, shop_options, shop_path, tools,
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

    fn several_tools() -> Tools {
        let mut tools = tools();
        tools.connected = vec![CLAUDE, CODEX];
        tools
    }

    fn shown(details: &[Detail]) -> Vec<(&str, String, Option<&str>)> {
        details
            .iter()
            .map(|d| (d.label, d.value.clone(), d.note.as_deref()))
            .collect()
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
    fn the_header_and_details_line_for_shop_22() {
        let t = shop_22();
        let mut dev = session(
            "s1",
            SessionRole::Developer,
            "Rust developer",
            CODEX,
            None,
            Some("gpt-5.5"),
        );
        dev.running = true;
        dev.started_at = 100 * MIN;
        dev.ended_at = None;
        let agents = agents_of(&t, vec![dev]);
        let several = several_tools();
        let details = details(&DetailFacts {
            ticket: &t,
            agents: Some(&agents),
            tools: &several,
            tickets: &[],
            now: 103 * MIN + 20_000,
            helpers: &[],
            usage: None,
        });
        assert_eq!(
            shown(&details),
            [
                ("Assignee", "Rust developer".into(), None),
                ("Model", "Codex \u{b7} gpt-5.5".into(), None),
                ("Working", "3 min \u{b7} 64k tokens".into(), None),
                ("Priority", "P2".into(), None),
                (
                    "Branch",
                    otaq_core::naming::branch("SHOP-22"),
                    Some("from main")
                ),
            ]
        );
        assert_eq!(
            details[0].avatar.as_ref().map(|a| a.letters.as_str()),
            Some("CX")
        );
        assert!(details[2].working);
        assert!(details[4].mono);
        assert!(matches!(
            details[3].change,
            Some(DetailChange::Priority { .. })
        ));
        assert_eq!(details[0].change, None, "Dev has started");
        let crumbs = breadcrumb(&t, "shop-web");
        let texts: Vec<_> = crumbs.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["shop-web", "SHOP-22"]);
        assert!(crumbs[1].key && !crumbs[0].key);
        // One tool connected: no avatar (SHL-08).
        let one = tools();
        let plain = super::details(&DetailFacts {
            ticket: &t,
            agents: Some(&agents),
            tools: &one,
            tickets: &[],
            now: 103 * MIN,
            helpers: &[],
            usage: None,
        });
        assert_eq!(plain[0].avatar, None);
    }

    /// #36: after the merge nobody is on the ticket, so the line has no Assignee (it said "Validator", the last agent
    /// that ran).
    #[test]
    fn a_done_ticket_has_no_assignee() {
        let mut t = shop_22();
        t.status = Status::Merged;
        t.place = Place::Done;
        let mut validator = session(
            "s2",
            SessionRole::Validator,
            "Validator",
            CODEX,
            None,
            Some("gpt-5.5"),
        );
        validator.running = false;
        let agents = agents_of(&t, vec![validator]);
        let tools = tools();
        let details = details(&DetailFacts {
            ticket: &t,
            agents: Some(&agents),
            tools: &tools,
            tickets: &[],
            now: 0,
            helpers: &[],
            usage: None,
        });
        let labels: Vec<&str> = details.iter().map(|d| d.label).collect();
        assert_eq!(labels, ["Priority", "Branch"]);
    }

    /// While a hand-off waits for the agent (d0118) the Working detail says so, without the working dot.
    #[test]
    fn working_says_a_hand_off_waits_for_the_agent() {
        let mut t = ticket("SHOP-18", "Login redirect", Status::Working);
        t.flags.hand_off = Some(otaq_core::domain::HandOffKind::SendBack);
        let tools = tools();
        let details = details(&DetailFacts {
            ticket: &t,
            agents: None,
            tools: &tools,
            tickets: &[],
            now: 0,
            helpers: &[],
            usage: None,
        });
        let working = details.iter().find(|d| d.label == "Working").unwrap();
        assert_eq!(
            working.value,
            "Sent back \u{b7} waiting for the agent to pick it up"
        );
        assert!(!working.working);
    }

    /// A ticket in Backlog: its first stage's agent, changeable before it starts; no working time, no branch yet.
    #[test]
    fn a_ticket_in_backlog() {
        let t = ticket("SHOP-41", "Fix README typo", Status::Backlog);
        let agents = agents_of(&t, Vec::new());
        let tools = tools();
        let details = details(&DetailFacts {
            ticket: &t,
            agents: Some(&agents),
            tools: &tools,
            tickets: &[],
            now: 0,
            helpers: &[],
            usage: None,
        });
        assert_eq!(
            shown(&details),
            [
                ("Assignee", "Spec writer".into(), None),
                ("Model", "Claude Code \u{b7} sonnet".into(), None),
                ("Priority", "P3".into(), None),
                ("Branch", String::new(), Some("from main")),
            ]
        );
        let Some(DetailChange::Choice { stage, items }) = &details[0].change else {
            panic!("Product spec's agent can change: {:?}", details[0].change);
        };
        assert_eq!(stage, "product-spec");
        assert!(items.iter().any(|i| i.checked));
    }

    /// In your check (`s25-09`): Assignee You, no model, and how long it waits for you.
    #[test]
    fn in_your_check() {
        let mut t = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        t.status_since = Some(10 * MIN);
        let tools = tools();
        let details = details(&DetailFacts {
            ticket: &t,
            agents: None,
            tools: &tools,
            tickets: &[],
            now: 22 * MIN,
            helpers: &[],
            usage: None,
        });
        assert_eq!(
            &shown(&details)[..3],
            [
                ("Assignee", "You".into(), None),
                ("Model", DASH.into(), None),
                ("Working", "waiting for you \u{b7} 12 min".into(), None),
            ]
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
                ("Done", StepState::ToCome, false),
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
        // In your check it opens on Files, where All changes and the review bar are (SHT-06).
        assert_eq!(
            opens_on(&ticket("SHOP-18", "Checkout API", Status::WaitingForYou)),
            SheetTab::Files
        );
        let labels: Vec<_> = shown.iter().map(|t| t.label()).collect();
        assert_eq!(labels, ["Overview", "Session", "Files", "Activity"]);
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

    /// Q416: "External ID  PAY-123" at the end of the details line when set, a click copying it; not shown when
    /// empty.
    #[test]
    fn the_external_id_shows_when_set_and_a_click_copies_it() {
        let tools = tools();
        let line = |t: &Ticket| {
            details(&DetailFacts {
                ticket: t,
                agents: None,
                tools: &tools,
                tickets: &[],
                now: 0,
                helpers: &[],
                usage: None,
            })
        };
        let mut t = shop_22();
        t.external_id = Some("PAY-123".into());
        let shown = line(&t);
        let id = shown.last().unwrap();
        assert_eq!((id.label, id.value.as_str()), ("External ID", "PAY-123"));
        assert!(id.mono);
        assert_eq!(
            id.change,
            Some(DetailChange::Copy {
                text: "PAY-123".into(),
                hint: "Copy External ID",
                copied: "Copied",
            })
        );
        for none in [None, Some(String::new()), Some("  ".into())] {
            t.external_id = none;
            assert!(
                line(&t).iter().all(|d| d.label != EXTERNAL_ID),
                "no External ID: {:?}",
                t.external_id
            );
        }
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
        assert_eq!(files_count_label(files_count(&three)).as_deref(), Some("3"));
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
        assert_eq!(width(false, 1376.), WIDTH);
        assert_eq!(width(true, 956.), WIDTH_BESIDE_AGENT);
        assert_eq!(
            width(true, 896.),
            896. - 36.,
            "a narrow main area narrows it"
        );
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
        assert!(labels.contains(&"Ready for Dev"));
        assert!(menu[0].checked && menu[0].disabled, "its own column");
        let ready = menu.iter().find(|i| i.id == "ready_for_dev").unwrap();
        assert!(!ready.disabled);
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
            "Ready for Dev is its own column"
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
