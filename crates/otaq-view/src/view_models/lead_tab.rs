//! The epic sheet's **Lead** tab (LEAD-11, LEAD-12; d0083, d0045, d0048; *05 · Ticket side panel → Row · Epic panel
//! · Lead tab (0083)* idle / re-planning / asks you, *Row · Lead tab · ended (0083)*, *16 → Row · Epic lead after a
//! restart* "Before the restart").
//!
//! - **The header:** the Lead Status in its full form, "read-only" and "212k today · on the epic".
//! - **Changes by the lead**, grouped by turn: "after SHOP-40.1 merged · 10:52", "this turn", or "whole epic · 4
//!   changes" once the epic is in review. Each line links to its ticket ("Edited SHOP-40.3 · description", "SHOP-40.3
//!   · waits for 40.4", "Added SHOP-40.5 · Wishlist empty state", "Suggested closing SHOP-40.6 · closed by you").
//!   The lines come from the epic's History: the `lead_changed` events the lead's commands write (T-0280) and the
//!   `lead_suggestion` events of the suggestions the user settled.
//!   - **Turns** aren't stored, so they are worked out from the wake-ups that start them: every child that merged
//!     or was closed (its Done time) opens a group, and a change belongs to the newest one before it. Changes before
//!     the first merge are its first turn, which checks the plan ("checking the plan · 10:12"; the lead no longer
//!     plans an epic from scratch, d0091). While the lead works, the newest group is "this turn" when nothing
//!     finished after it.
//! - **The body:** the live terminal; "Open lead" when the epic has none (**Split with the Project agent** while it
//!   has no children either, LEAD-13, d0091); "Before the restart" (REC-02, from
//!   Claude's saved conversation) with Resume after a restart; the last terminal, read-only, with "Ended · epic in
//!   review" once the epic reached Ready for Review; the last terminal with Resume when its agent tool exited.
//! - **Waiting on its tool** (d0086, *08 → Epic lead · its tool isn't available / is at its limit*): a lead held
//!   before its start says "Codex isn't available · logged out" (Sign in…), "Waiting for Codex's limit reset" or
//!   "Model unavailable" (Change agent); a running or ended lead on a tool that can't run now waits too, since its
//!   conversation never moves tools.

use otaq_core::agents::{Agent, AgentHold, SessionRole as Role};
use otaq_core::board::tickets::{EventKind, HistoryEvent, Ticket};
use otaq_core::domain::{Status, TicketKey};
use otaq_core::project_agents::lead::{Lead, LeadStatus};
use otaq_core::recovery::{BeforeRestart, TranscriptEntry};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::card::tokens_label;
use crate::view_models::clock::local_hhmm;
use crate::view_models::lead_status::{self, LeadStatusView};
use crate::view_models::recovery;
use crate::view_models::tool_problems::{self, Callout, Tools};

pub const READ_ONLY: &str = "read-only";
pub const CHANGES_TITLE: &str = "Changes by the lead";
pub const NO_CHANGES: &str = "No changes yet";
pub const THIS_TURN: &str = "this turn";
pub const RESUME_LEAD: &str = "Resume lead";
/// The changes before the first wake-up: the lead's first turn checks the plan (LEAD-02, LEAD-13).
pub const FIRST_TURN: &str = "checking the plan";

/// One change, linked to its ticket: "Edited" · SHOP-40.3 · "description". `LeadChangeLine` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LeadChangeLine")]
pub struct ChangeLine {
    pub icon: IconName,
    /// A new child: its icon is violet.
    pub added: bool,
    /// "Edited", "Added", "Suggested closing"; `None` when the line starts with the key ("SHOP-40.3 waits for 40.4").
    pub verb: Option<String>,
    /// The ticket the line opens.
    pub key: String,
    pub text: String,
    /// "Open SHOP-40.3".
    pub open_hint: String,
}

/// The changes of one turn. `LeadChangeGroup` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LeadChangeGroup")]
pub struct ChangeGroup {
    /// "after SHOP-40.1 merged · 10:52", "this turn", "whole epic · 4 changes".
    pub label: String,
    pub lines: Vec<ChangeLine>,
}

/// "Changes by the lead", newest turn first. `LeadChanges` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LeadChanges")]
pub struct Changes {
    pub title: &'static str,
    pub groups: Vec<ChangeGroup>,
    /// Shown when there are no groups.
    pub empty: &'static str,
}

/// One line of "Before the restart": time, who or what, text; `action` for a tool. `LeadRecoveryLine` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LeadRecoveryLine")]
pub struct RecoveryLine {
    pub time: String,
    pub who: String,
    pub text: String,
    pub action: bool,
}

/// What the tab shows under the changes. `LeadTabBody` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "LeadTabBody")]
pub enum Body {
    /// The lead's live terminal: type to it as to a ticket's (§9.3).
    Live { session: String, label: String },
    /// The epic has no lead: what Open lead does, and the button; Split with the Project agent with no children
    /// (no text).
    OpenLead {
        title: String,
        text: Option<&'static str>,
        button: lead_status::Button,
    },
    /// The agent tool's saved conversation is still being read, or the session is starting.
    Loading { text: String },
    /// Ended by restart: the last messages and actions from the tool's saved conversation, read-only, with Resume.
    BeforeRestart {
        title: &'static str,
        source: String,
        session: String,
        lines: Vec<RecoveryLine>,
        ended: String,
        resume: &'static str,
    },
    /// Ended by restart, and the saved conversation can't be read: Resume may still continue it.
    NotAvailable {
        label: &'static str,
        text: String,
        resume: &'static str,
    },
    /// Ended (epic in review, or its agent tool exited): the last terminal read-only, and the bar under it.
    Ended {
        /// The ended session, replayed read-only while the service still has it.
        session: Option<String>,
        label: String,
        title: &'static str,
        text: String,
        icon: IconName,
        /// Resume, when the lead can continue (not in review).
        resume: Option<&'static str>,
    },
}

/// The Lead tab of one epic. `LeadTabView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LeadTabView")]
pub struct LeadTabView {
    pub epic: String,
    pub status: LeadStatusView,
    pub read_only: &'static str,
    /// "212k today · on the epic"; `None` without a lead.
    pub tokens: Option<String>,
    /// `None` without a lead.
    pub changes: Option<Changes>,
    pub body: Body,
    /// The lead asks you: the sheet header shows its question bar (as for a ticket's Asks you).
    pub asks_you: bool,
    /// The lead waits on its tool or its model (M23): the notice above the body.
    pub waiting: Option<Callout>,
}

/// The unsent-text question before an answer to the lead's own question (d0048): (title, body).
pub fn unsent(name: &str, choice: &otaq_core::board::questions::Choice) -> (String, String) {
    (
        format!("{name} has unsent text in its terminal"),
        format!(
            "{} {}.",
            crate::view_models::terminal_strip::UnsentPrompt::EXPLANATION,
            crate::view_models::answer::sending(choice)
        ),
    )
}

/// The lead's statuses with a running session.
pub fn is_live(status: &LeadStatus) -> bool {
    matches!(
        status,
        LeadStatus::Idle
            | LeadStatus::CheckingPlan
            | LeadStatus::Replanning
            | LeadStatus::Answering { .. }
            | LeadStatus::AsksYou
    )
}

fn is_working(status: &LeadStatus) -> bool {
    matches!(
        status,
        LeadStatus::CheckingPlan | LeadStatus::Replanning | LeadStatus::Answering { .. }
    )
}

/// What the tab is made of: the epic, its project's tickets, its lead, the epic's History and, after a restart,
/// the lead's saved conversation (`None` while it is read).
pub struct Facts<'a> {
    pub epic: &'a Ticket,
    pub tickets: &'a [Ticket],
    pub lead: Option<&'a Lead>,
    pub events: &'a [HistoryEvent],
    pub before: Option<&'a BeforeRestart>,
    /// The epic's holds (`ticket.agents`): its lead's, held before its start.
    pub holds: &'a [AgentHold],
    /// Every named agent (the held one's name).
    pub agent_list: &'a [Agent],
    pub tools: &'a Tools,
}

pub fn view(f: &Facts) -> LeadTabView {
    let epic = f.epic;
    let lead = f.lead.filter(|l| l.status != LeadStatus::None);
    LeadTabView {
        epic: epic.key.to_string(),
        status: LeadStatusView::of_epic(epic, f.tickets, lead),
        read_only: READ_ONLY,
        tokens: lead.map(|l| {
            format!(
                "{} today \u{b7} on the epic",
                tokens_label(l.tokens_today.shown())
            )
        }),
        changes: lead.map(|l| changes(epic, f.tickets, l, f.events)),
        body: match lead {
            None => open_lead(epic, f.tickets),
            Some(l) => body(epic, l, f.before),
        },
        asks_you: lead.is_some_and(|l| l.status == LeadStatus::AsksYou),
        waiting: tool_problems::lead_notice(
            f.holds.iter().find(|h| h.role == Role::EpicLead),
            lead.filter(|l| l.status != LeadStatus::EndedInReview)
                .map(|l| l.agent.tool),
            f.agent_list,
            f.tools,
        ),
    }
}

/// Whether the tab needs the lead's saved conversation ("Before the restart").
pub fn needs_before_restart(lead: Option<&Lead>) -> bool {
    lead.is_some_and(|l| l.status == LeadStatus::EndedByRestart)
}

fn open_lead(epic: &Ticket, tickets: &[Ticket]) -> Body {
    if !lead_status::has_children(epic, tickets) {
        // LEAD-13 (d0091): the Project agent splits it; the lead orchestrates once it has children.
        return Body::OpenLead {
            title: format!("{} has no lead", epic.key),
            text: None,
            button: lead_status::Button::split(epic),
        };
    }
    Body::OpenLead {
        title: format!("{} has no lead", epic.key),
        text: Some(
            "Open lead starts a read-only agent that plans this epic with you, answers its children\u{2019}s \
             questions first and re-plans after each merge. It uses tokens; it never edits files or moves cards.",
        ),
        button: lead_status::Button::open_lead(),
    }
}

fn terminal_label(lead: &Lead) -> String {
    format!("{} \u{b7} terminal", lead.name)
}

fn body(epic: &Ticket, lead: &Lead, before: Option<&BeforeRestart>) -> Body {
    let session = lead.agent.session.as_ref().map(|s| s.id.clone());
    match &lead.status {
        status if is_live(status) => match session {
            Some(session) => Body::Live {
                session,
                label: terminal_label(lead),
            },
            None => Body::Loading {
                text: "The lead\u{2019}s session is starting\u{2026}".to_owned(),
            },
        },
        LeadStatus::EndedByRestart => before_restart(epic, lead, before),
        LeadStatus::EndedInReview => Body::Ended {
            session,
            label: terminal_label(lead),
            title: "Ended \u{b7} epic in review",
            text: "Reopening a child or adding one resumes this conversation.".to_owned(),
            icon: IconName::Eye,
            resume: None,
        },
        _ => Body::Ended {
            session,
            label: terminal_label(lead),
            title: "Ended",
            text: format!(
                "{}\u{2019}s {} exited. Resume continues this conversation. Until then, {}\u{2019}s children \
                 ask you directly.",
                lead.name,
                lead.agent.tool.command_name(),
                epic.key
            ),
            icon: IconName::ZapOff,
            resume: Some(lead_status::RESUME),
        },
    }
}

fn before_restart(epic: &Ticket, lead: &Lead, before: Option<&BeforeRestart>) -> Body {
    let tool = lead.agent.tool;
    let Some(before) = before else {
        return Body::Loading {
            text: format!(
                "Reading {}\u{2019}s saved conversation\u{2026}",
                tool.short_name()
            ),
        };
    };
    match before {
        BeforeRestart::Available {
            tool,
            tool_session_id,
            entries,
        } => {
            let short = recovery::short_id(tool_session_id);
            let at = lead
                .agent
                .ended_at
                .map_or_else(String::new, |at| format!(" at {}", local_hhmm(at)));
            Body::BeforeRestart {
                title: "Before the restart",
                source: format!(
                    "from {}\u{2019}s saved conversation \u{b7} read-only",
                    tool.short_name()
                ),
                session: format!("session {short}"),
                lines: entries
                    .iter()
                    .map(|e| {
                        let l = recovery::entry_line(e, *tool);
                        RecoveryLine {
                            // The assistant is the lead here.
                            who: if matches!(e, TranscriptEntry::Assistant { .. }) {
                                "Lead".to_owned()
                            } else {
                                l.who
                            },
                            time: l.time,
                            text: l.text,
                            action: l.action,
                        }
                    })
                    .collect(),
                ended: format!(
                    "The lead ended with a restart{at}. Resume continues this conversation ({}). Until then, \
                     {}\u{2019}s children ask you directly.",
                    tool.resume_hint(short),
                    epic.key
                ),
                resume: lead_status::RESUME,
            }
        }
        BeforeRestart::NotAvailable {
            tool: stored,
            tool_session_id,
        } => Body::NotAvailable {
            label: "Conversation not readable",
            text: format!(
                "{}\u{2019}s saved conversation{} can\u{2019}t be read, so there is nothing to show. Resume \
                 still tries to continue it. Until then, {}\u{2019}s children ask you directly.",
                stored.unwrap_or(tool).short_name(),
                tool_session_id
                    .as_deref()
                    .map_or_else(String::new, |id| format!(
                        " for session {}",
                        recovery::short_id(id)
                    )),
                epic.key
            ),
            resume: lead_status::RESUME,
        },
    }
}

/// "40.4" for a child of the same epic; other keys stay whole.
fn short(key: &str, epic: &TicketKey) -> String {
    match key.parse::<TicketKey>() {
        Ok(k) if k.project == epic.project && k.number == epic.number && k.child.is_some() => {
            format!("{}.{}", k.number, k.child.unwrap_or_default())
        }
        _ => key.to_owned(),
    }
}

fn text(payload: &serde_json::Value, field: &str) -> String {
    payload
        .get(field)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_owned()
}

/// One History event of the epic as a change line; `None` for events that aren't the lead's changes.
pub fn change_line(event: &HistoryEvent, epic: &TicketKey) -> Option<ChangeLine> {
    let p = &event.payload;
    let key = text(p, "child");
    if key.is_empty() {
        return None;
    }
    let (icon, added, verb, line) = match event.kind {
        EventKind::LeadChanged => match text(p, "change").as_str() {
            "updated" => {
                let fields: Vec<&str> = p
                    .get("fields")
                    .and_then(|f| f.as_array())
                    .map(|f| f.iter().filter_map(|v| v.as_str()).collect())
                    .unwrap_or_default();
                (
                    IconName::Pencil,
                    false,
                    Some("Edited"),
                    fields.join(" \u{b7} "),
                )
            }
            "dep_added" => (
                IconName::Link2,
                false,
                None,
                format!("waits for {}", short(&text(p, "parent"), epic)),
            ),
            "dep_removed" => (
                IconName::Unlink,
                false,
                None,
                format!("no longer waits for {}", short(&text(p, "parent"), epic)),
            ),
            "created" => (IconName::Plus, true, Some("Added"), text(p, "title")),
            _ => return None,
        },
        EventKind::LeadSuggestion => {
            let (icon, verb) = match text(p, "kind").as_str() {
                "delete" => (IconName::Trash2, "Suggested deleting"),
                _ => (IconName::CircleX, "Suggested closing"),
            };
            let outcome = text(p, "outcome");
            let line = if outcome.is_empty() {
                String::new()
            } else {
                format!("{outcome} by you")
            };
            (icon, false, Some(verb), line)
        }
        _ => return None,
    };
    Some(ChangeLine {
        icon,
        added,
        verb: verb.map(str::to_owned),
        open_hint: format!("Open {key}"),
        key,
        text: line,
    })
}

/// A wake-up that starts a turn: a child that merged or was closed, and when.
struct Wake {
    at: i64,
    label: String,
}

fn wakes(epic: &Ticket, tickets: &[Ticket]) -> Vec<Wake> {
    let mut out: Vec<Wake> = tickets
        .iter()
        .filter(|t| t.epic.as_ref() == Some(&epic.key))
        .filter_map(|t| {
            let at = t.status_since?;
            let label = match t.status {
                Status::Merged => format!("after {} merged", t.key),
                Status::ClosedNotMerged => format!("after {} was closed", t.key),
                _ => return None,
            };
            Some(Wake { at, label })
        })
        .collect();
    out.sort_by_key(|w| w.at);
    out
}

/// "Changes by the lead" of `epic`, newest turn first, from its History `events`.
pub fn changes(epic: &Ticket, tickets: &[Ticket], lead: &Lead, events: &[HistoryEvent]) -> Changes {
    let mut lines: Vec<(i64, ChangeLine)> = events
        .iter()
        .filter_map(|e| change_line(e, &epic.key).map(|l| (e.at, l)))
        .collect();
    lines.sort_by_key(|(at, _)| *at);
    let groups = if lead.status == LeadStatus::EndedInReview {
        if lines.is_empty() {
            Vec::new()
        } else {
            let n = lines.len();
            vec![ChangeGroup {
                label: format!(
                    "whole epic \u{b7} {n} {}",
                    if n == 1 { "change" } else { "changes" }
                ),
                lines: lines.into_iter().map(|(_, l)| l).collect(),
            }]
        }
    } else {
        by_turn(lines, &wakes(epic, tickets), is_working(&lead.status))
    };
    Changes {
        title: CHANGES_TITLE,
        groups,
        empty: NO_CHANGES,
    }
}

fn by_turn(lines: Vec<(i64, ChangeLine)>, wakes: &[Wake], working: bool) -> Vec<ChangeGroup> {
    // (the wake's index or none for the first turn, the first change's time, the lines), oldest first.
    let mut groups: Vec<(Option<usize>, i64, Vec<ChangeLine>)> = Vec::new();
    for (at, line) in lines {
        let wake = wakes.iter().rposition(|w| w.at <= at);
        match groups.last_mut() {
            Some((w, _, group)) if *w == wake => group.push(line),
            _ => groups.push((wake, at, vec![line])),
        }
    }
    let newest_wake = wakes.len().checked_sub(1);
    let count = groups.len();
    groups
        .into_iter()
        .enumerate()
        .map(|(i, (wake, first, lines))| {
            let label = if working && i + 1 == count && wake == newest_wake {
                THIS_TURN.to_owned()
            } else {
                match wake {
                    Some(w) => format!("{} \u{b7} {}", wakes[w].label, local_hhmm(wakes[w].at)),
                    None => format!("{FIRST_TURN} \u{b7} {}", local_hhmm(first)),
                }
            };
            ChangeGroup { label, lines }
        })
        .rev()
        .collect()
}

/// The Lead tab's states for the gallery and the page tests: SHOP-40 Wishlist with its lead idle, re-planning,
/// asking you, ended by restart and ended in review, and an epic with none. `LeadTabGallery` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LeadTabGallery")]
pub struct Gallery {
    pub tabs: Vec<GalleryTab>,
}

/// One state of the gallery. `LeadTabGalleryTab` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LeadTabGalleryTab")]
pub struct GalleryTab {
    pub name: String,
    pub view: LeadTabView,
}

pub mod fixtures {
    //! SHOP-40 Wishlist with the lead's changes of the frames: an edit of 40.3, 40.3 waiting for 40.4 and a new
    //! 40.5 after 40.1 merged, and a closing suggestion of 40.6.
    use otaq_core::board::tickets::{EventKind, HistoryEvent, Ticket};
    use otaq_core::domain::{Actor, Kind, Status};
    use otaq_core::project_agents::lead::{Lead, LeadStatus};
    use otaq_core::recovery::{BeforeRestart, TranscriptEntry};
    use serde_json::json;

    use crate::view_models::card_fixtures::ticket;
    use crate::view_models::lead_status::sample;

    /// 10:00 on the day of the samples, in ms (times are shown in local time, so tests compare against
    /// `local_hhmm`).
    pub const T0: i64 = 1_790_000_000_000;
    pub const MIN: i64 = 60_000;

    pub fn epic() -> Ticket {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        epic.external_id = Some("PAY-123".into());
        epic
    }

    pub fn tickets() -> Vec<Ticket> {
        let child = |key: &str, title: &str, status: Status, since: Option<i64>| {
            let mut t = ticket(key, title, status);
            t.epic = Some("SHOP-40".parse().expect("a key"));
            t.status_since = since;
            t
        };
        vec![
            epic(),
            child(
                "SHOP-40.1",
                "Wishlist API",
                Status::Merged,
                Some(T0 + 52 * MIN),
            ),
            child("SHOP-40.2", "Wishlist button", Status::Working, None),
            child("SHOP-40.3", "Wishlist page", Status::Backlog, None),
            child("SHOP-40.4", "Wishlist share link", Status::Working, None),
            child("SHOP-40.5", "Wishlist empty state", Status::Backlog, None),
        ]
    }

    fn event(kind: EventKind, at: i64, payload: serde_json::Value) -> HistoryEvent {
        HistoryEvent {
            kind,
            actor: Actor::ProjectAgent("lead-SHOP-40".into()),
            at,
            payload,
        }
    }

    /// The lead's changes after 40.1 merged (10:52–10:53).
    pub fn events() -> Vec<HistoryEvent> {
        let lead = "Wishlist lead";
        vec![
            event(
                EventKind::LeadChanged,
                T0 + 53 * MIN,
                json!({"lead": lead, "child": "SHOP-40.3", "change": "updated", "fields": ["description"]}),
            ),
            event(
                EventKind::LeadChanged,
                T0 + 53 * MIN + 1,
                json!({"lead": lead, "child": "SHOP-40.3", "change": "dep_added", "parent": "SHOP-40.4"}),
            ),
            event(
                EventKind::LeadChanged,
                T0 + 53 * MIN + 2,
                json!({"lead": lead, "child": "SHOP-40.5", "change": "created",
                       "title": "Wishlist empty state"}),
            ),
        ]
    }

    /// The closing suggestion of 40.6, settled by the user.
    pub fn suggestion(at: i64) -> HistoryEvent {
        HistoryEvent {
            kind: EventKind::LeadSuggestion,
            actor: Actor::User("me".into()),
            at,
            payload: json!({"lead": "Wishlist lead", "child": "SHOP-40.6", "kind": "close",
                            "outcome": "closed", "reason": "PAY-123 doesn't ask for an export."}),
        }
    }

    pub fn lead(status: LeadStatus) -> Lead {
        let mut lead = sample(&epic(), status, 212_000);
        lead.agent.tool_session_id = Some("7c21a9e4-0000-0000-0000-000000000000".into());
        lead.agent.session = Some(otaq_core::session::SessionInfo {
            id: "lead-session".into(),
            kind: otaq_core::session::SessionKind::ProjectAgent,
            project: "SHOP".into(),
            ticket: None,
            state: otaq_core::session::SessionState::Running,
            started_at: T0,
            ended_at: None,
            end_reason: None,
            tool: Some(lead.agent.tool),
            tool_session_id: lead.agent.tool_session_id.clone(),
            model: None,
            agent: None,
            effort: None,
            fallback: None,
            model_ran: None,
            size: None,
        });
        lead
    }

    /// The lead's saved conversation after a restart (*16 → "Before the restart"*).
    pub fn before() -> BeforeRestart {
        BeforeRestart::Available {
            tool: otaq_core::agent_tools::Tool::Claude,
            tool_session_id: "7c21a9e4-0000-0000-0000-000000000000".into(),
            entries: vec![
                TranscriptEntry::User {
                    text: "Automatic message: SHOP-40.1 merged: Wishlist API".into(),
                    at: Some(T0 + 52 * MIN),
                },
                TranscriptEntry::Action {
                    tool: otaq_core::naming::COMMAND_NAME.into(),
                    summary: "update SHOP-40.3 \u{b7} description; depends on SHOP-40.4".into(),
                    at: Some(T0 + 53 * MIN),
                },
                TranscriptEntry::Assistant {
                    text: "Plan updated. Idle until the next merge or question.".into(),
                    at: Some(T0 + 54 * MIN),
                },
            ],
        }
    }
}

/// The gallery's Lead tabs (*05 → Row · Epic sheet · Lead tab (0083)*, *Row · Lead tab · ended*, *16 → Before the
/// restart*).
pub fn gallery() -> Gallery {
    use fixtures::*;
    let tickets = tickets();
    let events = events();
    let mut review_events = events.clone();
    review_events.push(suggestion(T0 + 4 * 60 * MIN));
    let mut review_epic = epic();
    review_epic.status = Status::WaitingForYou;
    review_epic.place = otaq_core::domain::Place::stage_of("review");
    let mut asking = lead(LeadStatus::AsksYou);
    asking.agent.question = Some(otaq_core::board::questions::HumanQuestion {
        id: "q-lead".into(),
        text: "Should the share link (40.4) expire? PAY-123 doesn\u{2019}t say.".into(),
        options: vec!["Never".into(), "After 30 days".into()],
        recommended: None,
        notes: Vec::new(),
        asked_at: T0,
        via_lead: None,
    });
    let mut restarted = lead(LeadStatus::EndedByRestart);
    restarted.agent.ended_at = Some(T0 - 2 * 60 * MIN);
    let before = before();
    let epic = epic();
    let tab = |name: &str, epic: &Ticket, lead: Option<&Lead>, events: &[HistoryEvent], before| {
        GalleryTab {
            name: name.to_owned(),
            view: view(&Facts {
                epic,
                tickets: &tickets,
                lead,
                events,
                before,
                holds: &[],
                agent_list: &[],
                tools: &Tools::default(),
            }),
        }
    };
    Gallery {
        tabs: vec![
            tab(
                "Lead tab · idle",
                &epic,
                Some(&lead(LeadStatus::Idle)),
                &events,
                None,
            ),
            tab(
                "Lead tab · working (re-planning)",
                &epic,
                Some(&lead(LeadStatus::Replanning)),
                &events[..1],
                None,
            ),
            tab("Lead tab · asks you", &epic, Some(&asking), &events, None),
            tab(
                "Lead tab · ended by restart · Before the restart",
                &epic,
                Some(&restarted),
                &events,
                Some(&before),
            ),
            tab(
                "Lead tab · ended · epic in review",
                &review_epic,
                Some(&lead(LeadStatus::EndedInReview)),
                &review_events,
                None,
            ),
            tab("Lead tab · no lead", &epic, None, &[], None),
        ]
        .into_iter()
        .chain(waiting_tabs(&epic, &tickets, &events))
        .chain([GalleryTab {
            name: "Lead tab · no lead, no children (0091)".to_owned(),
            view: view(&Facts {
                epic: &epic,
                tickets: std::slice::from_ref(&epic),
                lead: None,
                events: &[],
                before: None,
                holds: &[],
                agent_list: &[],
                tools: &Tools::default(),
            }),
        }])
        .collect(),
    }
}

/// The gallery's waiting leads (M23, *08 → Epic lead · its tool isn't available / is at its limit*).
fn waiting_tabs(epic: &Ticket, tickets: &[Ticket], events: &[HistoryEvent]) -> Vec<GalleryTab> {
    use otaq_core::agents::HoldReason;
    use tool_problems::fixtures::{self as tf, CLAUDE, CODEX};
    let idle = fixtures::lead(LeadStatus::Idle);
    let mut logged_out = tf::several();
    logged_out.statuses[0] = tf::status(
        CLAUDE,
        otaq_core::agent_tools::connected::AgentToolState::FoundNotSignedIn { version: None },
    );
    let held = |reason| {
        [tf::hold(
            Role::EpicLead,
            reason,
            CODEX,
            "standard",
            Some("gpt-5.4"),
        )]
    };
    let tab = |name: &str, lead: Option<&Lead>, holds: &[AgentHold], tools: &Tools| GalleryTab {
        name: name.to_owned(),
        view: view(&Facts {
            epic,
            tickets,
            lead,
            events,
            before: None,
            holds,
            agent_list: &[],
            tools,
        }),
    };
    vec![
        tab(
            "Lead tab \u{b7} its tool isn't available",
            Some(&idle),
            &[],
            &logged_out,
        ),
        tab(
            "Lead tab \u{b7} its tool is at its limit",
            Some(&idle),
            &[],
            &tf::at_limit(CLAUDE, None),
        ),
        tab(
            "No lead \u{b7} held: Codex isn't available",
            None,
            &held(HoldReason::ToolUnavailable),
            &tf::codex_logged_out(),
        ),
        tab(
            "No lead \u{b7} held: Codex's limit",
            None,
            &held(HoldReason::ToolLimit),
            &tf::at_limit(CODEX, None),
        ),
        tab(
            "No lead \u{b7} held: Model unavailable",
            None,
            &held(HoldReason::ModelUnavailable),
            &tf::several(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::fixtures::*;
    use super::*;
    use crate::view_models::lead_status::ActionKind;

    fn tab(
        lead: Option<&Lead>,
        events: &[HistoryEvent],
        before: Option<&BeforeRestart>,
    ) -> LeadTabView {
        view(&Facts {
            epic: &epic(),
            tickets: &tickets(),
            lead,
            events,
            before,
            holds: &[],
            agent_list: &[],
            tools: &Tools::default(),
        })
    }

    /// *Lead tab · idle*: the header, the group "after SHOP-40.1 merged · 10:52" and the three lines.
    #[test]
    fn idle_shows_the_header_the_changes_and_the_live_terminal() {
        let t = tab(Some(&lead(LeadStatus::Idle)), &events(), None);
        assert_eq!(t.status.full, "Wishlist lead \u{b7} idle");
        assert_eq!(t.read_only, "read-only");
        assert_eq!(t.tokens.as_deref(), Some("212k today \u{b7} on the epic"));
        assert!(!t.asks_you);
        let changes = t.changes.unwrap();
        assert_eq!(changes.title, "Changes by the lead");
        assert_eq!(changes.groups.len(), 1);
        let group = &changes.groups[0];
        assert_eq!(
            group.label,
            format!(
                "after SHOP-40.1 merged \u{b7} {}",
                local_hhmm(T0 + 52 * MIN)
            )
        );
        let lines: Vec<(Option<&str>, &str, &str)> = group
            .lines
            .iter()
            .map(|l| (l.verb.as_deref(), l.key.as_str(), l.text.as_str()))
            .collect();
        assert_eq!(
            lines,
            [
                (Some("Edited"), "SHOP-40.3", "description"),
                (None, "SHOP-40.3", "waits for 40.4"),
                (Some("Added"), "SHOP-40.5", "Wishlist empty state"),
            ]
        );
        assert_eq!(group.lines[0].icon, IconName::Pencil);
        assert_eq!(group.lines[1].icon, IconName::Link2);
        assert!(group.lines[2].added);
        assert_eq!(group.lines[2].open_hint, "Open SHOP-40.5");
        assert_eq!(
            t.body,
            Body::Live {
                session: "lead-session".into(),
                label: "Wishlist lead \u{b7} terminal".into()
            }
        );
    }

    /// *Lead tab · working (re-planning)*: the newest group is "this turn".
    #[test]
    fn a_working_lead_labels_its_turn() {
        let t = tab(Some(&lead(LeadStatus::Replanning)), &events()[..1], None);
        assert_eq!(t.status.full, "Wishlist lead \u{b7} re-planning");
        let groups = t.changes.unwrap().groups;
        assert_eq!(groups[0].label, "this turn");
        assert!(matches!(t.body, Body::Live { .. }));
    }

    /// Turns follow the wake-ups: the first turn (checking the plan), then each merge; newest first. An older turn of a working lead keeps
    /// its label.
    #[test]
    fn changes_group_by_turn_newest_first() {
        let mut events = events();
        // A change in its first turn, before 40.1 merged.
        events.insert(
            0,
            HistoryEvent {
                kind: EventKind::LeadChanged,
                actor: otaq_core::domain::Actor::ProjectAgent("lead-SHOP-40".into()),
                at: T0 + 10 * MIN,
                payload: serde_json::json!({"child": "SHOP-40.4", "change": "dep_removed",
                                            "parent": "SHOP-40.2"}),
            },
        );
        let mut tickets = tickets();
        // 40.6 was closed after the lead's 10:53 changes: a newer turn with nothing yet.
        let mut closed = crate::view_models::card_fixtures::ticket(
            "SHOP-40.6",
            "Wishlist export (CSV)",
            Status::ClosedNotMerged,
        );
        closed.epic = Some("SHOP-40".parse().unwrap());
        closed.status_since = Some(T0 + 90 * MIN);
        tickets.push(closed);
        let groups = changes(&epic(), &tickets, &lead(LeadStatus::Replanning), &events).groups;
        let labels: Vec<&str> = groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                format!(
                    "after SHOP-40.1 merged \u{b7} {}",
                    local_hhmm(T0 + 52 * MIN)
                )
                .as_str(),
                format!("checking the plan \u{b7} {}", local_hhmm(T0 + 10 * MIN)).as_str(),
            ],
            "the 10:53 turn isn't \u{201c}this turn\u{201d}: 40.6 closed after it"
        );
        assert_eq!(groups[1].lines[0].text, "no longer waits for 40.2");
        assert_eq!(groups[1].lines[0].icon, IconName::Unlink);
        assert!(groups[0].lines.len() == 3);
    }

    /// *Lead tab · ended · epic in review*: one "whole epic · 4 changes" group, the last terminal read-only and the
    /// bar; no Resume.
    #[test]
    fn ended_in_review_lists_the_whole_epic() {
        let mut events = events();
        events.push(suggestion(T0 + 240 * MIN));
        let t = tab(Some(&lead(LeadStatus::EndedInReview)), &events, None);
        assert_eq!(
            t.status.full,
            "Wishlist lead \u{b7} ended \u{b7} epic in review"
        );
        let groups = t.changes.unwrap().groups;
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].label, "whole epic \u{b7} 4 changes");
        let last = &groups[0].lines[3];
        assert_eq!(last.verb.as_deref(), Some("Suggested closing"));
        assert_eq!(last.key, "SHOP-40.6");
        assert_eq!(last.text, "closed by you");
        assert_eq!(last.icon, IconName::CircleX);
        let Body::Ended {
            session,
            title,
            text,
            resume,
            ..
        } = t.body
        else {
            panic!("ended body");
        };
        assert_eq!(session.as_deref(), Some("lead-session"));
        assert_eq!(title, "Ended \u{b7} epic in review");
        assert_eq!(
            text,
            "Reopening a child or adding one resumes this conversation."
        );
        assert_eq!(resume, None);
    }

    /// *16 → Before the restart*: loading, then the saved conversation with Resume; unreadable says so.
    #[test]
    fn ended_by_restart_shows_before_the_restart() {
        let mut restarted = lead(LeadStatus::EndedByRestart);
        restarted.agent.ended_at = Some(T0 - 120 * MIN);
        assert!(needs_before_restart(Some(&restarted)));
        assert!(!needs_before_restart(Some(&lead(LeadStatus::Idle))));
        assert!(matches!(
            tab(Some(&restarted), &events(), None).body,
            Body::Loading { .. }
        ));
        let t = tab(Some(&restarted), &events(), Some(&before()));
        assert_eq!(
            t.status.action.as_ref().map(|a| a.label.as_str()),
            Some("Resume")
        );
        assert_eq!(t.changes.unwrap().groups.len(), 1, "the changes as idle");
        let Body::BeforeRestart {
            title,
            session,
            lines,
            ended,
            resume,
            ..
        } = t.body
        else {
            panic!("before the restart");
        };
        assert_eq!(title, "Before the restart");
        assert_eq!(session, "session 7c21a9e4");
        assert_eq!(lines[1].who, otaq_core::naming::COMMAND_NAME);
        assert!(lines[1].action);
        assert_eq!(lines[2].who, "Lead");
        assert_eq!(
            ended,
            format!(
                "The lead ended with a restart at {}. Resume continues this conversation (claude --resume \
                 7c21a9e4). Until then, SHOP-40\u{2019}s children ask you directly.",
                local_hhmm(T0 - 120 * MIN)
            )
        );
        assert_eq!(resume, "Resume");
        let gone = tab(
            Some(&restarted),
            &[],
            Some(&BeforeRestart::NotAvailable {
                tool: Some(otaq_core::agent_tools::Tool::Claude),
                tool_session_id: Some("7c21a9e4-1".into()),
            }),
        );
        let Body::NotAvailable { text, .. } = gone.body else {
            panic!("not available");
        };
        assert!(text.contains("session 7c21a9e4"));
    }

    /// A lead whose agent tool exited: the last terminal with Resume. Asks you: the header's question bar.
    #[test]
    fn ended_and_asks_you() {
        let t = tab(Some(&lead(LeadStatus::Ended)), &[], None);
        let Body::Ended { resume, title, .. } = &t.body else {
            panic!("ended");
        };
        assert_eq!(*title, "Ended");
        assert_eq!(*resume, Some("Resume"));
        assert_eq!(t.changes.unwrap().groups, Vec::new());
        let asks = tab(Some(&lead(LeadStatus::AsksYou)), &[], None);
        assert!(asks.asks_you);
        assert!(matches!(asks.body, Body::Live { .. }));
    }

    /// No lead and no children (LEAD-13, d0091): Split with the Project agent instead of Open lead, no text.
    #[test]
    fn an_empty_epic_offers_split_with_the_project_agent() {
        let epic = epic();
        let t = view(&Facts {
            epic: &epic,
            tickets: std::slice::from_ref(&epic),
            lead: None,
            events: &[],
            before: None,
            holds: &[],
            agent_list: &[],
            tools: &Tools::default(),
        });
        assert_eq!(t.status.full, "Split with the Project agent");
        assert_eq!(t.changes, None);
        let Body::OpenLead {
            title,
            text,
            button,
        } = t.body
        else {
            panic!("the button");
        };
        assert_eq!(title, "SHOP-40 has no lead");
        assert_eq!(text, None);
        assert_eq!(button.label, "Split with the Project agent");
        assert_eq!(button.kind, ActionKind::Split);
        assert_eq!(
            button.split.map(|s| (s.project, s.message)),
            Some(("SHOP".into(), "Split SHOP-40 into tickets".into()))
        );
    }

    /// No lead: Open lead, no tokens and no changes.
    #[test]
    fn no_lead_offers_open_lead() {
        let t = tab(None, &[], None);
        assert_eq!(t.status.full, "Open lead");
        assert_eq!(t.tokens, None);
        assert_eq!(t.changes, None);
        let Body::OpenLead { title, button, .. } = t.body else {
            panic!("open lead");
        };
        assert_eq!(title, "SHOP-40 has no lead");
        assert_eq!(button.label, "Open lead");
        assert_eq!(button.kind, ActionKind::Open);
    }

    #[test]
    fn unsent_text_names_the_lead() {
        let (title, body) = unsent(
            "Wishlist lead",
            &otaq_core::board::questions::Choice::Option(1),
        );
        assert_eq!(title, "Wishlist lead has unsent text in its terminal");
        assert!(!body.is_empty());
    }

    #[test]
    fn the_gallery_has_every_state() {
        let g = gallery();
        assert_eq!(g.tabs.len(), 12);
        assert_eq!(g.tabs[11].view.status.full, "Split with the Project agent");
        assert!(g.tabs[2].view.asks_you);
        assert!(matches!(g.tabs[3].view.body, Body::BeforeRestart { .. }));
    }

    /// *08 → Epic lead*: the three waiting states of a held lead, a running lead's tool, and none by default.
    #[test]
    fn a_waiting_lead_says_what_it_waits_for() {
        let g = gallery();
        let waiting: Vec<(String, Option<String>)> = g
            .tabs
            .iter()
            .map(|t| {
                (
                    t.name.clone(),
                    t.view.waiting.as_ref().map(|w| w.heading.clone()),
                )
            })
            .collect();
        assert!(waiting[..6].iter().all(|(_, w)| w.is_none()), "{waiting:?}");
        assert_eq!(
            waiting[6].1.as_deref(),
            Some(
                "Claude Code isn\u{2019}t available \u{b7} signed out. The lead waits; children on Claude Code wait too."
            )
        );
        assert_eq!(
            waiting[7].1.as_deref(),
            Some("Claude Code limit reached. The lead continues after the reset.")
        );
        assert_eq!(
            waiting[8].1.as_deref(),
            Some("Codex isn\u{2019}t available \u{b7} signed out")
        );
        assert_eq!(
            waiting[9].1.as_deref(),
            Some("Waiting for Codex\u{2019}s limit reset")
        );
        assert_eq!(waiting[10].1.as_deref(), Some("Model unavailable"));
        let labels = |i: usize| -> Vec<String> {
            g.tabs[i]
                .view
                .waiting
                .as_ref()
                .unwrap()
                .buttons
                .iter()
                .map(|b| b.label.clone())
                .collect()
        };
        assert_eq!(labels(8), ["Sign in\u{2026}"]);
        assert_eq!(labels(10), ["Open agent\u{2026}"]);
        // An ended-in-review lead doesn't wait on its tool.
        let tools = tool_problems::fixtures::at_limit(tool_problems::fixtures::CLAUDE, None);
        let t = view(&Facts {
            epic: &epic(),
            tickets: &tickets(),
            lead: Some(&lead(LeadStatus::EndedInReview)),
            events: &[],
            before: None,
            holds: &[],
            agent_list: &[],
            tools: &tools,
        });
        assert_eq!(t.waiting, None);
    }
}
