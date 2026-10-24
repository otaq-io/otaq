//! The ticket sheet (SHT-01, SHT-02; d0087, d0095, d0097; canvas `s25-02`, `s25-03`, `s25-10`): a 1040 px sheet over
//! the board (920 px beside the agent panel), or the whole content area at full page (⤢, "← Board"). Its header is
//! the breadcrumb (project / [epic /] key), the title and status, **Move…** and ⋯; under it the **details line**
//! (Assignee, Model, Working, Priority, Branch, Epic) and the flow path; then the tabs **Overview · Session · Files ·
//! Activity**, `1`–`4` and `g o` / `g s` / `g f` / `g a` (an epic: Overview · Tickets · Lead · Files · Activity,
//! SHT-07, `epic_sheet.rs`).
//!
//! What the sheet shows is a [`Content`]: a tab, or one side of Session's switch (Agent, the check's agent, Run, Shell;
//! `session_tab.rs`, T-0466). Each tab's screen registers its body: Overview (`overview.rs`, T-0465), Session, Files
//! (`files_tab.rs`, T-0468), Activity (`history.rs`, T-0466) and an epic's Lead.

use otaq_core::agents::facts::{PathKind, PathStage, TicketAgents};
use otaq_core::agents::store::SessionAgentRun;
use otaq_core::agents::{Agent, SessionRole};
use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::{Place, StatusSet};
use otaq_core::project_agents::lead::{Lead, LeadStatus};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::agents::TOOL_DEFAULT;
use crate::view_models::board_page::{DragAnswer, Verdict};
use crate::view_models::card::tokens_label;
use crate::view_models::flow_columns::Columns;
use crate::view_models::menu::MenuItem;
use crate::view_models::panel;
use crate::view_models::priority;
use crate::view_models::session_tab::Dot;
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
    /// An epic's lead (d0083), in the place of a ticket's Session.
    Lead,
    /// An epic's children as List rows (SHT-07).
    Tickets,
}

impl SheetTab {
    pub fn label(self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Session => "Session",
            Self::Files => "Files",
            Self::Activity => "Activity",
            Self::Lead => "Lead",
            Self::Tickets => "Tickets",
        }
    }

    pub fn icon(self) -> IconName {
        match self {
            Self::Overview => IconName::FileText,
            Self::Session => IconName::SquareTerminal,
            Self::Files => IconName::FolderOpen,
            Self::Activity => IconName::History,
            Self::Lead => IconName::Bot,
            Self::Tickets => IconName::List,
        }
    }

    /// Its key's number: `1`–`4`, and `g o` / `g s` / `g f` / `g a` (KeyAction::SwitchTab). An epic's Lead takes
    /// Session's; its Tickets has no key of its own (Q-A: `SwitchTab` goes to 4).
    pub fn number(self) -> Option<u8> {
        match self {
            Self::Overview => Some(1),
            Self::Session | Self::Lead => Some(2),
            Self::Files => Some(3),
            Self::Activity => Some(4),
            Self::Tickets => None,
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
            Self::Lead => &[Content::Lead],
            Self::Tickets => &[Content::Tickets],
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
    /// An epic's lead (d0083).
    Lead,
    /// An epic's children (SHT-07).
    Tickets,
}

/// The sheet's tabs for `ticket`, in order: Overview · Session · Files · Activity; an epic shows Overview · Tickets ·
/// Lead · Files (its branch with its children's tags, FTB-06) · Activity (SHT-07).
pub fn tabs(ticket: &Ticket) -> Vec<SheetTab> {
    if ticket.is_epic() {
        vec![
            SheetTab::Overview,
            SheetTab::Tickets,
            SheetTab::Lead,
            SheetTab::Files,
            SheetTab::Activity,
        ]
    } else {
        vec![
            SheetTab::Overview,
            SheetTab::Session,
            SheetTab::Files,
            SheetTab::Activity,
        ]
    }
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

/// The Lead tab's dot: violet while the lead runs (the page draws a live lead's dot in its own colour), turquoise
/// while it asks you, none once it ended.
pub fn lead_dot(status: Option<&LeadStatus>) -> Option<Dot> {
    let status = status.filter(|s| super::lead_tab::is_live(s))?;
    Some(if *status == LeadStatus::AsksYou {
        Dot::Waiting
    } else {
        Dot::Running
    })
}

/// One crumb of the breadcrumb: the project, the epic (opens it), the key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetCrumb")]
pub struct Crumb {
    pub text: String,
    /// The ticket a click opens (the epic's crumb).
    pub opens: Option<String>,
    /// The key, in the mono font.
    pub key: bool,
}

/// "40.2" for an epic child (its epic is the crumb before it), an epic's External ID ("PAY-123"); other keys whole.
fn short_key(ticket: &Ticket) -> String {
    match &ticket.external_id {
        Some(external) if ticket.is_epic() && !external.is_empty() => external.clone(),
        _ => super::epic_sheet::short_key(ticket),
    }
}

/// `shop-web / SHOP-22`, `shop-web / Wishlist / 40.2`, an epic's `shop-web / PAY-123` (SHT-01, SHT-07). At full page
/// the project's crumb is the page's "← Board".
pub fn breadcrumb(ticket: &Ticket, project: &str, epic_title: Option<&str>) -> Vec<Crumb> {
    let epic = ticket.epic.as_ref().map(|epic| Crumb {
        text: epic_title.map_or_else(|| epic.to_string(), str::to_owned),
        opens: Some(epic.to_string()),
        key: epic_title.is_none(),
    });
    [Crumb {
        text: project.to_owned(),
        opens: None,
        key: false,
    }]
    .into_iter()
    .chain(epic)
    .chain([Crumb {
        text: short_key(ticket),
        opens: None,
        key: true,
    }])
    .collect()
}

/// What a click on a detail does.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "SheetDetailChange")]
pub enum DetailChange {
    /// Priority's menu (`panel.priority`).
    Priority { items: Vec<MenuItem> },
    /// The stage's agent before it starts (`panel.set_choice`, AGM-08).
    Choice { stage: String, items: Vec<MenuItem> },
    /// The epic: opens its sheet.
    Open { key: String },
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
    /// The epic of a child, by title.
    pub epic_title: Option<&'a str>,
    /// An epic's lead and its project's tickets (its children): Lead, Model and Tickets (SHT-07).
    pub lead: Option<&'a Lead>,
    pub tickets: &'a [Ticket],
    pub now: i64,
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
        let change = stage_choices::panel_choices(agents)
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

/// The note after an epic's branch: its children merge into it (`s25-11`).
pub const EPIC_BRANCH: &str = "the epic\u{2019}s branch";

/// Branch: "otaq/SHOP-22 from main", "otaq/PAY-123/40.2 from the epic’s branch"; an epic's own is "the epic’s branch",
/// shared by its children.
fn branch(ticket: &Ticket) -> Detail {
    if ticket.is_epic() {
        let name = ticket
            .work
            .as_ref()
            .and_then(|w| w.branch.clone())
            .unwrap_or_else(|| otaq_core::naming::branch(&ticket.key.to_string()));
        return Detail {
            mono: true,
            note: Some(EPIC_BRANCH.to_owned()),
            ..Detail::new("Branch", name)
        };
    }
    Detail {
        mono: true,
        note: Some(match &ticket.base {
            Base::Branch(b) => format!("from {b}"),
            Base::Epic(_) => "from the epic\u{2019}s branch".to_owned(),
        }),
        ..Detail::new("Branch", panel::branch_name(ticket).unwrap_or_default())
    }
}

/// An epic's Lead and Model (SHT-07): its lead's name and the tool and model it runs on; before it has one, the lead
/// agent its flow names.
fn lead_details(f: &DetailFacts) -> (Detail, Detail) {
    let (mut who, model) = match f.lead {
        Some(lead) => {
            let agent = &lead.agent;
            (
                Detail {
                    avatar: f.tools.avatar(agent.tool, agent.model.as_deref()),
                    ..Detail::new("Lead", lead.name.clone())
                },
                Detail::new("Model", model_text(agent.tool, agent.model.as_deref())),
            )
        }
        None => match f.agents.and_then(|a| a.lead.as_ref()) {
            Some(agent) => {
                let on = agent.model.on(f.tools.default_model.as_ref());
                (
                    Detail {
                        avatar: on
                            .as_ref()
                            .and_then(|m| f.tools.avatar(m.tool, m.model.as_deref())),
                        ..Detail::new(
                            "Lead",
                            otaq_core::project_agents::lead::lead_name(&f.ticket.title),
                        )
                    },
                    Detail::new(
                        "Model",
                        on.map_or_else(
                            || DASH.to_owned(),
                            |m| model_text(m.tool, m.model.as_deref()),
                        ),
                    ),
                )
            }
            None => (Detail::new("Lead", DASH), Detail::new("Model", DASH)),
        },
    };
    who.label = "Lead";
    (who, model)
}

/// The details line (SHT-01): Assignee, Model (not once it is Done), Working (while it works or waits for you),
/// Priority, Branch, and an epic child's Epic; an epic's (SHT-07) Lead, Model, Tickets "5 · 1 merged · 1 working", Priority and its branch.
pub fn details(f: &DetailFacts) -> Vec<Detail> {
    let ticket = f.ticket;
    let priority = Detail {
        change: Some(DetailChange::Priority {
            items: priority::menu_items(ticket.priority),
        }),
        ..Detail::new("Priority", priority::view(ticket.priority).short)
    };
    if ticket.is_epic() {
        let (lead, model) = lead_details(f);
        let tickets = Detail::new(
            "Tickets",
            super::epic_sheet::tickets_detail(ticket, f.tickets),
        );
        return vec![lead, model, tickets, priority, branch(ticket)];
    }
    // A finished ticket has no one on it: no Assignee (nor its Model), not the last agent that ran (#36).
    let (who, model) = if ticket.place == Place::Done {
        (None, None)
    } else {
        let (who, model) = assignee(f);
        (Some(who), Some(model))
    };
    let epic = ticket.epic.as_ref().map(|epic| Detail {
        change: Some(DetailChange::Open {
            key: epic.to_string(),
        }),
        ..Detail::new(
            "Epic",
            f.epic_title.map_or_else(|| epic.to_string(), str::to_owned),
        )
    });
    who.into_iter()
        .chain(model)
        .chain(working(f))
        .chain([priority, branch(ticket)])
        .chain(epic)
        .collect()
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

/// One stage of the flow path under the details line (`s25-03`: Grill ✓ — Dev ● — Validate — …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SheetStep")]
pub struct Step {
    pub stage: String,
    pub state: StepState,
    /// It needs you in the stage it is in (the turquoise ring).
    pub attention: bool,
}

/// The flow path as steps: the stages before the one it is in done, that one current, the rest to come; from
/// Backlog every stage is to come, in Done every one is done. None for an epic or an unread flow.
pub fn steps(ticket: &Ticket, agents: Option<&TicketAgents>) -> Vec<Step> {
    let Some(agents) = agents.filter(|_| !ticket.is_epic()) else {
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
            epic_title: None,
            lead: None,
            tickets: &[],
            now: 103 * MIN + 20_000,
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
        let crumbs = breadcrumb(&t, "shop-web", None);
        let texts: Vec<_> = crumbs.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["shop-web", "SHOP-22"]);
        assert!(crumbs[1].key && !crumbs[0].key);
        // One tool connected: no avatar (SHL-08).
        let one = tools();
        let plain = super::details(&DetailFacts {
            ticket: &t,
            agents: Some(&agents),
            tools: &one,
            epic_title: None,
            lead: None,
            tickets: &[],
            now: 103 * MIN,
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
            epic_title: None,
            lead: None,
            tickets: &[],
            now: 0,
        });
        let labels: Vec<&str> = details.iter().map(|d| d.label).collect();
        assert_eq!(labels, ["Priority", "Branch"]);
    }

    /// A child of the Wishlist epic: the epic in the breadcrumb (key shortened) and its Epic detail linking back,
    /// the branch from the epic's.
    #[test]
    fn a_child_of_an_epic() {
        let mut t = ticket("SHOP-40.2", "Wishlist button", Status::Working);
        t.epic = Some("SHOP-40".parse().unwrap());
        t.base = Base::Epic("SHOP-40".parse().unwrap());
        t.place = Place::InStage("dev".into());
        let crumbs = breadcrumb(&t, "shop-web", Some("Wishlist"));
        let texts: Vec<_> = crumbs.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["shop-web", "Wishlist", "40.2"]);
        assert_eq!(crumbs[1].opens.as_deref(), Some("SHOP-40"));
        let tools = tools();
        let details = details(&DetailFacts {
            ticket: &t,
            agents: None,
            tools: &tools,
            epic_title: Some("Wishlist"),
            lead: None,
            tickets: &[],
            now: 0,
        });
        let epic = details.last().unwrap();
        assert_eq!((epic.label, epic.value.as_str()), ("Epic", "Wishlist"));
        assert_eq!(
            epic.change,
            Some(DetailChange::Open {
                key: "SHOP-40".into()
            })
        );
        let branch = details.iter().find(|d| d.label == "Branch").unwrap();
        assert_eq!(
            branch.note.as_deref(),
            Some("from the epic\u{2019}s branch")
        );
        assert_eq!(details[0].value, DASH, "agents unread");
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
            epic_title: None,
            lead: None,
            tickets: &[],
            now: 0,
        });
        let working = details.iter().find(|d| d.label == "Working").unwrap();
        assert_eq!(
            working.value,
            "Sent back \u{b7} waiting for the agent to pick it up"
        );
        assert!(!working.working);
    }

    /// The Wishlist epic (`s25-11`): `shop-web / PAY-123`; Lead "Wishlist lead" on Claude Code · opus, Tickets "5 · 1
    /// merged · 1 working", Priority, and the epic's branch.
    #[test]
    fn the_wishlist_epics_header_and_details() {
        let mut tickets = crate::view_models::epic_sheet::samples::wishlist();
        tickets[0].priority = Priority::Urgent;
        tickets[0].work = Some(otaq_core::board::tickets::Work {
            branch: Some(otaq_core::naming::branch("PAY-123")),
            ..Default::default()
        });
        let epic = &tickets[0];
        let crumbs = breadcrumb(epic, "shop-web", None);
        let texts: Vec<_> = crumbs.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(texts, ["shop-web", "PAY-123"]);
        let mut lead = crate::view_models::lead_status::sample(epic, LeadStatus::AsksYou, 0);
        lead.agent.model = Some("opus".into());
        let several = several_tools();
        let details = details(&DetailFacts {
            ticket: epic,
            agents: None,
            tools: &several,
            epic_title: None,
            lead: Some(&lead),
            tickets: &tickets,
            now: 0,
        });
        assert_eq!(
            shown(&details),
            [
                ("Lead", "Wishlist lead".into(), None),
                ("Model", "Claude Code \u{b7} opus".into(), None),
                ("Tickets", "5 \u{b7} 1 merged \u{b7} 1 working".into(), None),
                ("Priority", "P1".into(), None),
                (
                    "Branch",
                    otaq_core::naming::branch("PAY-123"),
                    Some(EPIC_BRANCH)
                ),
            ]
        );
        assert_eq!(
            details[0].avatar.as_ref().map(|a| a.letters.as_str()),
            Some("CC")
        );
        // No lead yet: dashes until it starts.
        let tools = tools();
        let none = super::details(&DetailFacts {
            ticket: epic,
            agents: None,
            tools: &tools,
            epic_title: None,
            lead: None,
            tickets: &tickets,
            now: 0,
        });
        assert_eq!((none[0].label, none[0].value.as_str()), ("Lead", DASH));
        assert!(steps(epic, None).is_empty());
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
            epic_title: None,
            lead: None,
            tickets: &[],
            now: 0,
        });
        assert_eq!(
            shown(&details),
            [
                ("Assignee", "Griller".into(), None),
                ("Model", "Claude Code \u{b7} sonnet".into(), None),
                ("Priority", "P3".into(), None),
                ("Branch", String::new(), Some("from main")),
            ]
        );
        let Some(DetailChange::Choice { stage, items }) = &details[0].change else {
            panic!("Grill's agent can change: {:?}", details[0].change);
        };
        assert_eq!(stage, "grill");
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
            epic_title: None,
            lead: None,
            tickets: &[],
            now: 22 * MIN,
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

    /// SHOP-22's path (`s25-03`): Grill done, Dev current and needing you, the rest to come.
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
                ("Grill", StepState::Done, false),
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

    /// Tabs: a ticket's four, numbered 1–4; an epic's Overview · Lead · Activity with Files in review; Overview on
    /// open; what each one shows.
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

        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = otaq_core::domain::Kind::Epic;
        let epic_tabs = tabs(&epic);
        assert_eq!(
            epic_tabs,
            [
                SheetTab::Overview,
                SheetTab::Tickets,
                SheetTab::Lead,
                SheetTab::Files,
                SheetTab::Activity
            ]
        );
        let numbers: Vec<Option<u8>> = epic_tabs.iter().map(|t| t.number()).collect();
        assert_eq!(numbers, [Some(1), None, Some(2), Some(3), Some(4)]);
        assert_eq!(numbered(&epic_tabs, 2), Some(SheetTab::Lead));
        assert_eq!(SheetTab::Tickets.contents(), [Content::Tickets]);
        assert_eq!(opens_on(&epic), SheetTab::Overview);
    }

    #[test]
    fn the_lead_tab_has_its_dot() {
        assert_eq!(lead_dot(Some(&LeadStatus::Replanning)), Some(Dot::Running));
        assert_eq!(lead_dot(Some(&LeadStatus::AsksYou)), Some(Dot::Waiting));
        assert_eq!(lead_dot(Some(&LeadStatus::EndedByRestart)), None);
        assert_eq!(lead_dot(None), None);
    }

    /// j / k follow the column's order (pinned first), as the panel's did (`panel::neighbour`).
    #[test]
    fn j_and_k_follow_the_column_order() {
        let mut a = ticket("SHOP-1", "a", Status::Backlog);
        a.created_at = 1;
        let mut b = ticket("SHOP-2", "b", Status::Backlog);
        b.created_at = 2;
        b.flags.pinned_at = Some(10);
        let tickets = vec![a, b];
        let key = |s: &str| s.parse::<otaq_core::domain::TicketKey>().unwrap();
        assert_eq!(
            panel::neighbour(&tickets, &key("SHOP-2"), 1),
            Some(key("SHOP-1"))
        );
        assert_eq!(
            panel::neighbour(&tickets, &key("SHOP-1"), -1),
            Some(key("SHOP-2"))
        );
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
}
