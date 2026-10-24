//! The Needs you strip (T-0459; NYS-01, NYS-02, NYS-03; d0114, d0016, d0024, d0083, d0093; *24 → Row · The two
//! views* `s24-03`, *Row · Needs you: the next project, and every group* `s24-12`): under the board's toolbar, in both
//! views, the open project's needs-you items as chips, in the queue's order (d0016) and its groups: what needs you
//! ("Needs you · 3": questions, permission prompts, Idle, a lead passing on a child's question, a waiting Project agent
//! or epic lead), the epic leads' suggestions ("Suggestion", Confirm · Not now), Interrupted ("Interrupted · 3" with
//! Resume all), then your checks ("Checks · 1"). "Needs you · 0 here" when the project has none.
//!
//! **Next (⌘J)** opens the item after the open one in this project; after this project's last item it reads "Next ·
//! billing-api, 1" and goes to the next project with items, in the rail's order, opening its first. It never moves on
//! by itself (d0016): only the user's press opens anything. Waiting plain Project agents are items too (NYS-02).
//!
//! It replaces the Inbox overlay (d0114): the items, their order and the back-pressure count are the service's
//! (`attention::inbox`), unchanged.

use otaq_core::attention::inbox::InboxList;
use otaq_core::board::needs_you::{NeedsYouGroup, NeedsYouItem, SuggestionItem};
use otaq_core::domain::{Status, TicketKey};
use otaq_core::epic_lead::suggest::SuggestionKind;
use otaq_core::flow::model::Delivers;
use otaq_core::project_agents::AgentNeedsYou;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::session_tab;
use crate::view_models::sheet::Content;

/// One thing in the strip: a ticket's item, or a Project agent (an epic lead included) waiting for the user.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
    Ticket(Box<NeedsYouItem>),
    Agent(Box<AgentNeedsYou>),
}

impl Item {
    pub fn project(&self) -> &str {
        match self {
            Self::Ticket(i) => &i.project,
            Self::Agent(a) => &a.project,
        }
    }

    pub fn project_name(&self) -> &str {
        match self {
            Self::Ticket(i) => &i.project_name,
            Self::Agent(a) => &a.project_name,
        }
    }

    /// Its group: a waiting agent is one of the needs-you items (d0114).
    pub fn group(&self) -> NeedsYouGroup {
        match self {
            Self::Ticket(i) => i.group,
            Self::Agent(_) => NeedsYouGroup::Family,
        }
    }

    /// What opening it shows: a ticket on its tab, an epic lead on its epic's Lead tab, a Project agent in the panel.
    pub fn target(&self) -> Target {
        match self {
            Self::Ticket(i) => Target::Ticket {
                project: i.project.clone(),
                key: i.key.to_string(),
                tab: if i.suggestion.is_some() {
                    // A suggestion's child never started: its description.
                    Content::Overview
                } else if i.group == NeedsYouGroup::Review {
                    Content::Files
                } else {
                    // Session, on the side that needs you (the check's while it asks).
                    session_tab::lands_on(i.status)
                },
            },
            Self::Agent(a) => match &a.epic {
                Some(epic) => Target::Ticket {
                    project: a.project.clone(),
                    key: epic.to_string(),
                    tab: Content::Lead,
                },
                None => Target::Agent {
                    project: a.project.clone(),
                    agent: a.agent.clone(),
                },
            },
        }
    }

    /// Whether it is what the page shows now.
    fn is(&self, open: &Open) -> bool {
        match (self.target(), open) {
            (Target::Ticket { key, .. }, Open::Ticket { key: shown }) => &key == shown,
            (Target::Agent { agent, .. }, Open::Agent { agent: shown }) => &agent == shown,
            _ => false,
        }
    }
}

/// Every item of every project, in the strip's order: the service's queue order (by group, then the tickets that
/// block more, the oldest first; d0016), with the waiting Project agents and epic leads after the tickets of the
/// first group (LEAD-11, NYS-02).
pub fn items(list: &InboxList) -> Vec<Item> {
    let at = list
        .items
        .iter()
        .position(|i| i.group != NeedsYouGroup::Family)
        .unwrap_or(list.items.len());
    let ticket = |i: &NeedsYouItem| Item::Ticket(Box::new(i.clone()));
    list.items[..at]
        .iter()
        .map(ticket)
        .chain(
            list.project_agents
                .iter()
                .map(|a| Item::Agent(Box::new(a.clone()))),
        )
        .chain(list.items[at..].iter().map(ticket))
        .collect()
}

/// The projects in the rail's order, by key (the service lists every project in its counts).
pub fn project_order(list: &InboxList) -> Vec<String> {
    list.counts
        .projects
        .iter()
        .map(|p| p.project.clone())
        .collect()
}

/// What opening an item shows. `StripTarget` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "StripTarget")]
pub enum Target {
    /// A ticket (or an epic, on its Lead tab) on `tab`.
    Ticket {
        project: String,
        key: String,
        tab: Content,
    },
    /// A plain Project agent in the panel.
    Agent { project: String, agent: String },
}

/// What the page shows now, which Next moves on from. `StripOpen` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "StripOpen")]
pub enum Open {
    Ticket { key: String },
    Agent { agent: String },
}

/// Where Next goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step<'a> {
    /// An item of this project.
    Here(&'a Item),
    /// This project's last item is behind: the next project with items, its count and its first item.
    Elsewhere {
        name: &'a str,
        count: usize,
        first: &'a Item,
    },
}

impl<'a> Step<'a> {
    pub fn item(self) -> &'a Item {
        match self {
            Self::Here(item) | Self::Elsewhere { first: item, .. } => item,
        }
    }
}

/// **Next** (NYS-02) from `open` in `project`: the item after it in this project; after the last (or with none here),
/// the first item of the next project in `order` that has any; with nothing elsewhere, this project's first again.
/// `None` when nothing waits anywhere.
pub fn next<'a>(
    items: &'a [Item],
    project: &str,
    open: Option<&Open>,
    order: &[String],
) -> Option<Step<'a>> {
    let here: Vec<&Item> = items.iter().filter(|i| i.project() == project).collect();
    let at = open.and_then(|o| here.iter().position(|i| i.is(o)));
    let after = match at {
        Some(n) => here.get(n + 1),
        None => here.first(),
    };
    if let Some(item) = after {
        return Some(Step::Here(item));
    }
    elsewhere(items, project, order).or_else(|| here.first().map(|i| Step::Here(i)))
}

/// The next project after `project` (in `order`, wrapping) that has items.
fn elsewhere<'a>(items: &'a [Item], project: &str, order: &[String]) -> Option<Step<'a>> {
    let start = order.iter().position(|p| p == project).map_or(0, |n| n + 1);
    let len = order.len();
    (0..len)
        .map(|n| &order[(start + n) % len])
        .filter(|p| p.as_str() != project)
        .find_map(|p| {
            let mut theirs = items.iter().filter(|i| i.project() == p);
            let first = theirs.next()?;
            Some(Step::Elsewhere {
                name: first.project_name(),
                count: 1 + theirs.count(),
                first,
            })
        })
}

/// A chip's colours, after the status legend: turquoise needs you, red failed, the check's blue, the agents' violet,
/// paused grey. `StripTone` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "StripTone")]
pub enum Tone {
    Attn,
    Error,
    Review,
    Investigate,
    Paused,
    Approved,
}

/// One item's chip (`s24-03`): its key and a short reason ("SHOP-22 allow a command"); the hover says it whole.
/// `StripChip` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "StripChip")]
pub struct Chip {
    /// "SHOP-22", "40.4" (an epic's child by its number), "Agent".
    pub label: String,
    /// "allow a command", "a question", "decide", "your check", "via the lead".
    pub reason: String,
    pub icon: IconName,
    pub tone: Tone,
    /// The hover: the key, the title and the service's reason ("SHOP-22 Fix flaky login redirect · permission: …").
    pub title: String,
    pub target: Target,
}

/// Which confirmation a suggestion's Confirm opens: Close ticket (d0044) or Delete (d0060). `StripSuggestionDialog`
/// on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "StripSuggestionDialog")]
pub enum SuggestionDialog {
    Close,
    Delete,
}

/// An epic lead's suggestion (LEAD-08; `s24-12`): the chip "Close 40.6?" opens "Wishlist lead suggests", the lead's
/// reason, Not now (dismiss) and Confirm (the usual confirmation, with the suggestion's line). `StripSuggestion` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "StripSuggestion")]
pub struct Suggestion {
    pub id: String,
    /// The child it is about.
    pub ticket: String,
    /// "Close 40.6?".
    pub label: String,
    /// "Wishlist lead suggests".
    pub heading: String,
    /// The lead's reason.
    pub text: String,
    pub not_now: &'static str,
    pub confirm: &'static str,
    pub dialog: SuggestionDialog,
    /// "Suggested by Wishlist lead: “PAY-123 doesn't ask for an export.”", added to the confirmation.
    pub line: String,
}

/// The groups of 0016's queue. `StripGroupId` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "StripGroupId")]
pub enum GroupId {
    NeedsYou,
    Suggestions,
    Interrupted,
    Checks,
}

/// One group: its head ("Needs you · 3", "Suggestion", "Interrupted · 3", "Checks · 1") and its chips. `StripGroup` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "StripGroup")]
pub struct Group {
    pub id: GroupId,
    pub icon: IconName,
    pub tone: Tone,
    pub title: String,
    pub chips: Vec<Chip>,
    pub suggestions: Vec<Suggestion>,
    /// "Resume all" on Interrupted while a session can be resumed (d0045): it asks first.
    pub resume_all: Option<&'static str>,
}

/// Next (⌘J): "Next", or "Next · billing-api, 1" when it goes to another project (then the accent button).
/// `StripNext` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "StripNext")]
pub struct NextButton {
    pub label: String,
    pub icon: IconName,
    /// The user's key for it; `None` once it was removed.
    pub keys: Option<String>,
    /// It goes to another project.
    pub elsewhere: bool,
}

/// "Needs you · 0 here" (`s24-12`), and "You’re through shop-web’s items." when another project has some.
/// `StripEmpty` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "StripEmpty")]
pub struct Empty {
    pub icon: IconName,
    pub title: &'static str,
    pub text: Option<String>,
}

/// The strip. `StripView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "StripView")]
pub struct View {
    pub groups: Vec<Group>,
    pub empty: Option<Empty>,
    pub next: Option<NextButton>,
}

/// The strip of `project` (`project_name` for the empty text) with `open` shown, the projects in the rail's order and
/// the user's key for Next.
pub fn view(
    items: &[Item],
    project: &str,
    project_name: &str,
    open: Option<&Open>,
    order: &[String],
    keys: Option<&str>,
) -> View {
    let here: Vec<&Item> = items.iter().filter(|i| i.project() == project).collect();
    let in_group = |g: NeedsYouGroup| here.iter().copied().filter(move |i| i.group() == g);
    let mut groups = Vec::new();
    let needs_you: Vec<Chip> = in_group(NeedsYouGroup::Family).filter_map(chip).collect();
    if !needs_you.is_empty() {
        groups.push(Group {
            id: GroupId::NeedsYou,
            icon: IconName::Hand,
            tone: Tone::Attn,
            title: format!("Needs you \u{b7} {}", needs_you.len()),
            chips: needs_you,
            suggestions: Vec::new(),
            resume_all: None,
        });
    }
    let suggestions: Vec<Suggestion> = in_group(NeedsYouGroup::Suggestions)
        .filter_map(|i| match i {
            Item::Ticket(t) => t.suggestion.as_ref().map(|s| suggestion(&t.key, s)),
            Item::Agent(_) => None,
        })
        .collect();
    if !suggestions.is_empty() {
        groups.push(Group {
            id: GroupId::Suggestions,
            icon: IconName::Lightbulb,
            tone: Tone::Investigate,
            title: "Suggestion".to_owned(),
            chips: Vec::new(),
            suggestions,
            resume_all: None,
        });
    }
    let stopped: Vec<&Item> = in_group(NeedsYouGroup::Stopped).collect();
    if !stopped.is_empty() {
        let resumable = stopped
            .iter()
            .any(|i| matches!(i, Item::Ticket(t) if t.status == Status::Interrupted));
        groups.push(Group {
            id: GroupId::Interrupted,
            icon: IconName::CirclePause,
            tone: Tone::Paused,
            title: format!("Interrupted \u{b7} {}", stopped.len()),
            // Interrupted sessions are counted in the head and resumed together; the rest keep a chip.
            chips: stopped.into_iter().filter_map(chip).collect(),
            suggestions: Vec::new(),
            resume_all: resumable.then_some("Resume all"),
        });
    }
    let checks: Vec<Chip> = in_group(NeedsYouGroup::Review).filter_map(chip).collect();
    if !checks.is_empty() {
        groups.push(Group {
            id: GroupId::Checks,
            icon: IconName::UserCheck,
            tone: Tone::Review,
            title: format!("Checks \u{b7} {}", checks.len()),
            chips: checks,
            suggestions: Vec::new(),
            resume_all: None,
        });
    }
    let step = next(items, project, open, order);
    let next_button = step.map(|step| match step {
        Step::Here(_) => NextButton {
            label: "Next".to_owned(),
            icon: IconName::ArrowRight,
            keys: keys.map(str::to_owned),
            elsewhere: false,
        },
        Step::Elsewhere { name, count, .. } => NextButton {
            label: format!("Next \u{b7} {name}, {count}"),
            icon: IconName::ArrowRight,
            keys: keys.map(str::to_owned),
            elsewhere: true,
        },
    });
    let empty = here.is_empty().then(|| Empty {
        icon: IconName::CircleCheck,
        title: "Needs you \u{b7} 0 here",
        text: next_button
            .as_ref()
            .filter(|n| n.elsewhere)
            .map(|_| format!("You\u{2019}re through {project_name}\u{2019}s items.")),
    });
    View {
        groups,
        empty,
        next: next_button,
    }
}

/// A key as a chip shows it: an epic's child by its number ("40.4"), any other ticket whole ("SHOP-22").
pub fn short_key(key: &TicketKey) -> String {
    let text = key.to_string();
    match text.split_once('-') {
        Some((_, number)) if number.contains('.') => number.to_owned(),
        _ => text,
    }
}

/// An item's chip; `None` for an interrupted session, which Resume all covers, and for a suggestion (its own chip).
fn chip(item: &Item) -> Option<Chip> {
    match item {
        Item::Ticket(t) => ticket_chip(t, item.target()),
        Item::Agent(a) => Some(agent_chip(a, item.target())),
    }
}

fn ticket_chip(item: &NeedsYouItem, target: Target) -> Option<Chip> {
    if item.suggestion.is_some() {
        return None;
    }
    let status = item.interrupted_from.unwrap_or(item.status);
    let asks = (
        IconName::MessageCircleQuestionMark,
        Tone::Attn,
        "a question".to_owned(),
    );
    let (icon, tone, reason) = if item.message.is_some() {
        asks
    } else if item.group == NeedsYouGroup::Review {
        let reason = match item.waiting_in.as_ref().and_then(|w| w.delivers) {
            Some(Delivers::Pr) => "open the PR",
            Some(Delivers::Local) => "merge it",
            Some(Delivers::Either) => "deliver it",
            None => "your check",
        };
        (IconName::UserCheck, Tone::Review, reason.to_owned())
    } else {
        match status {
            Status::NeedsYou | Status::CheckerNeedsYou => {
                (IconName::Hand, Tone::Attn, "allow a command".to_owned())
            }
            Status::AsksYou => match item.question.as_ref().and_then(|q| q.via_lead.as_ref()) {
                Some(_) => (
                    IconName::MessageCircleQuestionMark,
                    Tone::Attn,
                    "via the lead".to_owned(),
                ),
                None => asks,
            },
            Status::Idle => (IconName::Hand, Tone::Attn, "idle".to_owned()),
            Status::CheckFailed | Status::CheckNoResult => {
                (IconName::CircleX, Tone::Error, "decide".to_owned())
            }
            Status::DeliveryStopped => (IconName::CircleX, Tone::Error, "merge stopped".to_owned()),
            Status::StartInterrupted => (IconName::CircleX, Tone::Error, "start failed".to_owned()),
            Status::ResumeFailed => (IconName::CircleX, Tone::Error, "resume failed".to_owned()),
            Status::ParentClosed => (
                IconName::CirclePause,
                Tone::Paused,
                "parent closed".to_owned(),
            ),
            // An interrupted session: Resume all's.
            _ => return None,
        }
    };
    Some(Chip {
        label: short_key(&item.key),
        reason,
        icon,
        tone,
        title: format!("{} {} \u{b7} {}", item.key, item.title, item.reason),
        target,
    })
}

fn agent_chip(a: &AgentNeedsYou, target: Target) -> Chip {
    let reason = match (&a.epic, a.status) {
        (_, Status::NeedsYou) => "allow a command",
        (Some(_), _) => "the lead asks",
        (None, _) => "a question",
    };
    Chip {
        label: match &a.epic {
            Some(epic) => short_key(epic),
            None => "Agent".to_owned(),
        },
        reason: reason.to_owned(),
        icon: IconName::Bot,
        tone: Tone::Investigate,
        title: match &a.epic {
            Some(_) => format!("{} \u{b7} {}", a.name, a.reason),
            None => format!(
                "Project agent \u{201c}{}\u{201d} \u{b7} {}",
                a.name, a.reason
            ),
        },
        target,
    }
}

/// The line a suggestion adds to the close and delete confirmations.
pub fn suggested_line(s: &SuggestionItem) -> String {
    format!(
        "Suggested by {}: \u{201c}{}\u{201d}",
        s.lead,
        s.reason.trim()
    )
}

fn suggestion(child: &TicketKey, s: &SuggestionItem) -> Suggestion {
    let (verb, dialog) = match s.kind {
        SuggestionKind::Close => ("Close", SuggestionDialog::Close),
        SuggestionKind::Delete => ("Delete", SuggestionDialog::Delete),
    };
    Suggestion {
        id: s.id.clone(),
        ticket: child.to_string(),
        label: format!("{verb} {}?", short_key(child)),
        heading: format!("{} suggests", s.lead),
        text: s.reason.trim().to_owned(),
        not_now: "Not now",
        confirm: "Confirm",
        dialog,
        line: suggested_line(s),
    }
}

/// One ticket of the Resume all confirmation. `ResumeTicket` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ResumeTicket")]
pub struct ResumeTicket {
    pub key: String,
    pub title: String,
}

/// The Resume all confirmation (*10 → Resume all · confirm*): what it resumes and what it leaves out. Its confirm
/// resumes; nothing resumes without it (d0045). `ResumeAllDialog` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ResumeAllDialog")]
pub struct ResumeAllDialog {
    /// "Resume 1 interrupted session?"
    pub title: String,
    pub text: &'static str,
    pub included: Vec<ResumeTicket>,
    /// "Not included: SHOP-28 Cart badge count. …" when Start interrupted tickets wait too.
    pub not_included: Option<String>,
    pub cancel: &'static str,
    pub cancel_hint: &'static str,
    /// "Resume 1".
    pub confirm: String,
}

/// Resume all's confirmation for `project`'s items (every project's for `None`, the banner's); `None` when no session
/// is left to resume.
pub fn resume_all_dialog(items: &[Item], project: Option<&str>) -> Option<ResumeAllDialog> {
    let tickets = |status: Status| -> Vec<&NeedsYouItem> {
        items
            .iter()
            .filter(|i| project.is_none_or(|p| i.project() == p))
            .filter_map(|i| match i {
                Item::Ticket(t) if t.status == status => Some(&**t),
                _ => None,
            })
            .collect()
    };
    let included: Vec<ResumeTicket> = tickets(Status::Interrupted)
        .into_iter()
        .map(|t| ResumeTicket {
            key: t.key.to_string(),
            title: t.title.clone(),
        })
        .collect();
    if included.is_empty() {
        return None;
    }
    let n = included.len();
    let plural = if n == 1 { "session" } else { "sessions" };
    let left_out: Vec<String> = tickets(Status::StartInterrupted)
        .into_iter()
        .map(|t| format!("{} {}", t.key, t.title))
        .collect();
    let not_included = (!left_out.is_empty()).then(|| {
        let why = if left_out.len() == 1 {
            "Its start never finished, so there\u{2019}s no session to resume. Use Retry start on its card."
        } else {
            "Their starts never finished, so there\u{2019}s no session to resume. Use Retry start on their cards."
        };
        format!("Not included: {}. {why}", left_out.join(", "))
    });
    Some(ResumeAllDialog {
        title: format!("Resume {n} interrupted {plural}?"),
        text: "Each session continues in its own worktree. This starts agents, so it uses tokens.",
        included,
        not_included,
        cancel: "Cancel",
        cancel_hint: "cancel",
        confirm: format!("Resume {n}"),
    })
}

/// One drawn state of the strip for the Gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "StripGalleryState")]
pub struct GalleryState {
    pub label: &'static str,
    pub view: View,
}

/// The strip's states for the Gallery (`gallery.needs_you_strip`): the example's shop-web (`s24-03`), every group
/// (`s24-12`), "Needs you · 0 here" with Next to billing-api, and nothing anywhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "StripGallery")]
pub struct Gallery {
    pub states: Vec<GalleryState>,
}

pub fn gallery() -> Gallery {
    use samples::*;
    let order = vec!["SHOP".to_owned(), "BILL".to_owned()];
    let keys = Some("\u{2318}J");
    let shop = shop_web();
    let mut every = shop.clone();
    every.extend(every_group());
    let only_bill = vec![bill_7()];
    let draw =
        |items: &[Item], project: &str, name: &str| view(items, project, name, None, &order, keys);
    Gallery {
        states: vec![
            GalleryState {
                label: "shop-web · Needs you · 4 (s24-03)",
                view: draw(&shop, "SHOP", "shop-web"),
            },
            GalleryState {
                label: "Every group (s24-12)",
                view: draw(&every, "SHOP", "shop-web"),
            },
            GalleryState {
                label: "Needs you · 0 here · Next to billing-api (s24-12)",
                view: draw(&only_bill, "SHOP", "shop-web"),
            },
            GalleryState {
                label: "Nothing needs you anywhere",
                view: draw(&[], "SHOP", "shop-web"),
            },
        ],
    }
}

/// The example's items (`tickets/m38-tickets.md` → The example), for the Gallery and the tests.
pub mod samples {
    use otaq_core::board::needs_you::{NeedsYouGroup, NeedsYouItem, SuggestionItem, WaitingIn};
    use otaq_core::board::questions::{HumanQuestion, ViaLead};
    use otaq_core::domain::Status;
    use otaq_core::epic_lead::suggest::SuggestionKind;
    use otaq_core::project_agents::AgentNeedsYou;

    use super::Item;

    pub fn ticket(key: &str, title: &str, status: Status, reason: &str) -> NeedsYouItem {
        let project = key.split('-').next().unwrap_or_default().to_owned();
        let project_name = match project.as_str() {
            "BILL" => "billing-api",
            _ => "shop-web",
        };
        NeedsYouItem {
            group: otaq_core::board::needs_you::group_of(status).unwrap_or(NeedsYouGroup::Family),
            project,
            project_name: project_name.into(),
            id: key.into(),
            key: key.parse().expect("key"),
            title: title.into(),
            status,
            interrupted_from: None,
            question: None,
            reason: reason.into(),
            since: 0,
            age_ms: 0,
            blocked: 0,
            message: None,
            suggestion: None,
            lead_note: None,
            change_agent: None,
            waiting_in: None,
        }
    }

    pub fn item(t: NeedsYouItem) -> Item {
        Item::Ticket(Box::new(t))
    }

    /// shop-web as `s24-03` draws it: SHOP-22 allow a command, SHOP-12 a question, SHOP-32 decide, SHOP-18 your check.
    pub fn shop_web() -> Vec<Item> {
        let mut check = ticket(
            "SHOP-18",
            "Checkout API",
            Status::WaitingForYou,
            "your check · passed Security",
        );
        check.waiting_in = Some(WaitingIn {
            stage: "review".into(),
            name: "Review".into(),
            rank: 10,
            delivers: None,
        });
        vec![
            item(ticket(
                "SHOP-22",
                "Fix flaky login redirect",
                Status::NeedsYou,
                "permission: allow rm -rf .next && pnpm build",
            )),
            item(ticket(
                "SHOP-12",
                "Add dark mode toggle",
                Status::AsksYou,
                "asks: follow the system setting?",
            )),
            item(ticket(
                "SHOP-32",
                "Address autocomplete",
                Status::CheckFailed,
                "Security failed 3 times · unescaped SQL",
            )),
            item(check),
        ]
    }

    /// What `s24-12` adds: 40.4 passed on by the Wishlist lead, a waiting Project agent, the lead's suggestion and
    /// three interrupted sessions (the checks are shop-web's).
    pub fn every_group() -> Vec<Item> {
        let mut passed = ticket(
            "SHOP-40.4",
            "Share link",
            Status::AsksYou,
            "asks · passed on by Wishlist lead: Should the share link expire?",
        );
        passed.question = Some(HumanQuestion {
            id: "q4".into(),
            text: "Should the share link expire?".into(),
            options: Vec::new(),
            recommended: None,
            notes: Vec::new(),
            asked_at: 0,
            via_lead: Some(ViaLead {
                lead: "Wishlist lead".into(),
                reason: "PAY-123 doesn't say.".into(),
            }),
        });
        let mut suggestion = ticket(
            "SHOP-40.6",
            "Wishlist export (CSV)",
            Status::Backlog,
            "Wishlist lead suggests closing SHOP-40.6 Wishlist export (CSV)",
        );
        suggestion.group = NeedsYouGroup::Suggestions;
        suggestion.suggestion = Some(SuggestionItem {
            id: "sg1".into(),
            epic: "SHOP-40".parse().expect("key"),
            lead: "Wishlist lead".into(),
            kind: SuggestionKind::Close,
            reason: "PAY-123 doesn't ask for an export.".into(),
        });
        let interrupted = |key: &str, title: &str| {
            item(ticket(
                key,
                title,
                Status::Interrupted,
                "session ended by restart",
            ))
        };
        vec![
            item(passed),
            Item::Agent(Box::new(agent())),
            item(suggestion),
            interrupted("SHOP-33", "Shipping estimate"),
            interrupted("SHOP-29", "Coupon field"),
            interrupted("SHOP-30", "Order history page"),
        ]
    }

    /// A plain Project agent waiting with a question.
    pub fn agent() -> AgentNeedsYou {
        AgentNeedsYou {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            agent: "pa3".into(),
            name: "Checkout slowness".into(),
            epic: None,
            status: Status::AsksYou,
            reason: "asks: Add an index on orders.created_at?".into(),
            question: None,
            since: 0,
        }
    }

    /// billing-api's one item: BILL-7 asks "keep the old template?".
    pub fn bill_7() -> Item {
        item(ticket(
            "BILL-7",
            "Invoice PDF export",
            Status::AsksYou,
            "asks: keep the old template?",
        ))
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::attention::inbox::{InboxCounts, InboxList};

    use super::samples::*;
    use super::*;

    const ORDER: [&str; 3] = ["SHOP", "BILL", "NOTE"];

    fn order() -> Vec<String> {
        ORDER.iter().map(|p| (*p).to_owned()).collect()
    }

    fn draw(items: &[Item], open: Option<&Open>) -> View {
        view(items, "SHOP", "shop-web", open, &order(), Some("\u{2318}J"))
    }

    fn chips(group: &Group) -> Vec<String> {
        group
            .chips
            .iter()
            .map(|c| format!("{} {}", c.label, c.reason))
            .collect()
    }

    fn open_ticket(key: &str) -> Open {
        Open::Ticket { key: key.into() }
    }

    /// NYS-01 (`s24-03`): the open project's items as chips with a short reason, in the queue's order; the checks
    /// after what needs you.
    #[test]
    fn the_example_strip_has_its_chips_in_order() {
        let mut items = shop_web();
        items.push(bill_7());
        let v = draw(&items, None);
        let ids: Vec<GroupId> = v.groups.iter().map(|g| g.id).collect();
        assert_eq!(ids, [GroupId::NeedsYou, GroupId::Checks]);
        assert_eq!(v.groups[0].title, "Needs you \u{b7} 3");
        assert_eq!(
            chips(&v.groups[0]),
            [
                "SHOP-22 allow a command",
                "SHOP-12 a question",
                "SHOP-32 decide"
            ]
        );
        assert_eq!(
            (v.groups[0].chips[0].icon, v.groups[0].chips[0].tone),
            (IconName::Hand, Tone::Attn)
        );
        assert_eq!(v.groups[0].chips[2].tone, Tone::Error);
        assert_eq!(v.groups[1].title, "Checks \u{b7} 1");
        assert_eq!(chips(&v.groups[1]), ["SHOP-18 your check"]);
        assert_eq!(
            v.groups[1].chips[0].target,
            Target::Ticket {
                project: "SHOP".into(),
                key: "SHOP-18".into(),
                tab: Content::Files
            }
        );
        assert_eq!(
            v.groups[0].chips[0].title,
            "SHOP-22 Fix flaky login redirect \u{b7} permission: allow rm -rf .next && pnpm build"
        );
        // Not billing-api's: the strip is about the board in view (d0114).
        assert!(
            v.groups
                .iter()
                .all(|g| g.chips.iter().all(|c| !c.label.starts_with("BILL")))
        );
        assert_eq!(v.empty, None);
        let next = v.next.unwrap();
        assert_eq!((next.label.as_str(), next.elsewhere), ("Next", false));
        assert_eq!(next.keys.as_deref(), Some("\u{2318}J"));
    }

    /// NYS-01 (`s24-12`): every group in the queue's order: a passed-on question and a waiting Project agent under
    /// Needs you, the lead's suggestion with Not now · Confirm, "Interrupted · 3" with Resume all and no chips, then
    /// "Checks · 1".
    #[test]
    fn every_group_in_the_queues_order() {
        let mut items = shop_web();
        items.extend(every_group());
        let list = list(items);
        let v = draw(&super::items(&list), None);
        let titles: Vec<&str> = v.groups.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "Needs you \u{b7} 5",
                "Suggestion",
                "Interrupted \u{b7} 3",
                "Checks \u{b7} 1"
            ]
        );
        assert_eq!(
            chips(&v.groups[0]),
            [
                "SHOP-22 allow a command",
                "SHOP-12 a question",
                "SHOP-32 decide",
                "40.4 via the lead",
                "Agent a question"
            ]
        );
        let agent = &v.groups[0].chips[4];
        assert_eq!((agent.icon, agent.tone), (IconName::Bot, Tone::Investigate));
        assert_eq!(
            agent.target,
            Target::Agent {
                project: "SHOP".into(),
                agent: "pa3".into()
            }
        );
        let s = &v.groups[1].suggestions[0];
        assert_eq!(s.label, "Close 40.6?");
        assert_eq!(s.heading, "Wishlist lead suggests");
        assert_eq!(s.text, "PAY-123 doesn't ask for an export.");
        assert_eq!((s.not_now, s.confirm), ("Not now", "Confirm"));
        assert_eq!(s.dialog, SuggestionDialog::Close);
        assert_eq!(
            s.line,
            "Suggested by Wishlist lead: \u{201c}PAY-123 doesn't ask for an export.\u{201d}"
        );
        assert!(v.groups[2].chips.is_empty());
        assert_eq!(v.groups[2].resume_all, Some("Resume all"));
        assert_eq!(v.groups[3].title, "Checks \u{b7} 1");
    }

    /// Start failed and Parent closed keep a chip under Interrupted; Resume all only while a session can resume.
    #[test]
    fn interrupted_without_a_session_keeps_its_chip() {
        let items = vec![
            item(ticket(
                "SHOP-28",
                "Cart badge count",
                Status::StartInterrupted,
                "start failed",
            )),
            item(ticket(
                "SHOP-27",
                "Gift wrap",
                Status::ParentClosed,
                "parent SHOP-26 was closed without merging",
            )),
        ];
        let v = draw(&items, None);
        assert_eq!(v.groups[0].title, "Interrupted \u{b7} 2");
        assert_eq!(
            chips(&v.groups[0]),
            ["SHOP-28 start failed", "SHOP-27 parent closed"]
        );
        assert_eq!(v.groups[0].resume_all, None);
    }

    /// T-0490 (#58; d0045, d0063): after a restart a check interrupted mid-run is under Interrupted (Resume starts a
    /// fresh check); a ticket interrupted while it asked keeps its question with the turquoise items.
    #[test]
    fn an_interrupted_check_is_interrupted_not_a_kept_question() {
        use crate::view_models::card_fixtures;
        use otaq_core::board::needs_you::{ProjectTickets, needs_you_items};
        use otaq_core::board::questions::HumanQuestion;
        use otaq_core::flow::default::VALIDATE;
        use otaq_core::flow::default_flow;

        let mut checking = card_fixtures::ticket("SHOP-15", "Coupon field", Status::Interrupted);
        checking.place = otaq_core::domain::Place::stage_of(VALIDATE);
        checking.interrupted_from = Some(Status::Checking);
        let mut asked = card_fixtures::ticket("SHOP-22", "Login", Status::Interrupted);
        asked.interrupted_from = Some(Status::AsksYou);
        asked.question = Some(HumanQuestion {
            id: "q1".into(),
            text: "Which email provider should I use?".into(),
            options: Vec::new(),
            recommended: None,
            notes: Vec::new(),
            asked_at: 0,
            via_lead: None,
        });
        let flow = default_flow();
        let tickets = [checking, asked];
        let found = needs_you_items(
            &[ProjectTickets {
                key: "SHOP",
                name: "shop-web",
                tickets: &tickets,
                flow: &flow,
            }],
            card_fixtures::NOW,
        );
        let reasons: Vec<(String, NeedsYouGroup, &str)> = found
            .iter()
            .map(|i| (i.key.to_string(), i.group, i.reason.as_str()))
            .collect();
        assert_eq!(
            reasons,
            [
                (
                    "SHOP-22".to_owned(),
                    NeedsYouGroup::Family,
                    "asks: Which email provider should I use?"
                ),
                (
                    "SHOP-15".to_owned(),
                    NeedsYouGroup::Stopped,
                    "session interrupted"
                ),
            ]
        );
        let items: Vec<Item> = found.into_iter().map(item).collect();
        let v = draw(&items, None);
        let heads: Vec<(GroupId, &str)> =
            v.groups.iter().map(|g| (g.id, g.title.as_str())).collect();
        assert_eq!(
            heads,
            [
                (GroupId::NeedsYou, "Needs you \u{b7} 1"),
                (GroupId::Interrupted, "Interrupted \u{b7} 1"),
            ]
        );
        assert_eq!(chips(&v.groups[0]), ["SHOP-22 a question"]);
        assert!(v.groups[1].chips.is_empty());
        assert_eq!(v.groups[1].resume_all, Some("Resume all"));
    }

    /// An epic lead's own question opens its epic on the Lead tab; a suggestion's child opens on its description.
    #[test]
    fn a_lead_opens_its_epic_and_a_suggestion_its_child() {
        let mut lead = agent();
        lead.epic = Some("SHOP-40".parse().unwrap());
        lead.name = "Wishlist lead".into();
        let v = draw(&[Item::Agent(Box::new(lead.clone()))], None);
        assert_eq!(chips(&v.groups[0]), ["SHOP-40 the lead asks"]);
        assert_eq!(
            Item::Agent(Box::new(lead)).target(),
            Target::Ticket {
                project: "SHOP".into(),
                key: "SHOP-40".into(),
                tab: Content::Lead
            }
        );
        let suggestion = every_group().remove(2);
        assert!(matches!(
            suggestion.target(),
            Target::Ticket {
                tab: Content::Overview,
                ..
            }
        ));
    }

    /// NYS-01 (`s24-12`): "Needs you · 0 here", "You’re through shop-web’s items." and the accent "Next ·
    /// billing-api, 1".
    #[test]
    fn needs_you_0_here_points_to_the_next_project() {
        let v = draw(&[bill_7()], None);
        assert!(v.groups.is_empty());
        let empty = v.empty.unwrap();
        assert_eq!(empty.title, "Needs you \u{b7} 0 here");
        assert_eq!(
            empty.text.as_deref(),
            Some("You\u{2019}re through shop-web\u{2019}s items.")
        );
        let next = v.next.unwrap();
        assert_eq!(
            (next.label.as_str(), next.elsewhere),
            ("Next \u{b7} billing-api, 1", true)
        );
        // Nothing anywhere: no Next, no "through" line.
        let v = draw(&[], None);
        assert_eq!(v.empty.unwrap().text, None);
        assert_eq!(v.next, None);
    }

    /// NYS-02: Next walks this project in order, then reads "Next · billing-api, 1" on its last item and opens
    /// billing-api's first; it doesn't wrap while another project has items.
    #[test]
    fn next_walks_this_project_then_the_next_one() {
        let mut items = shop_web();
        items.push(bill_7());
        let order = order();
        let key = |step: Option<Step>| match step.map(|s| s.item().target()) {
            Some(Target::Ticket { key, .. }) => key,
            other => panic!("{other:?}"),
        };
        assert_eq!(key(next(&items, "SHOP", None, &order)), "SHOP-22");
        assert_eq!(
            key(next(&items, "SHOP", Some(&open_ticket("SHOP-22")), &order)),
            "SHOP-12"
        );
        // A ticket that isn't an item (a card the user opened): the first one.
        assert_eq!(
            key(next(&items, "SHOP", Some(&open_ticket("SHOP-41")), &order)),
            "SHOP-22"
        );
        let last = open_ticket("SHOP-18");
        let step = next(&items, "SHOP", Some(&last), &order).unwrap();
        assert!(matches!(
            step,
            Step::Elsewhere {
                name: "billing-api",
                count: 1,
                ..
            }
        ));
        assert_eq!(key(Some(step)), "BILL-7");
        assert_eq!(
            draw(&items, Some(&last)).next.unwrap().label,
            "Next \u{b7} billing-api, 1"
        );
        // From billing-api's last, back round to shop-web (the rail's order wraps).
        assert_eq!(
            key(next(&items, "BILL", Some(&open_ticket("BILL-7")), &order)),
            "SHOP-22"
        );
        // Alone with its items, a project wraps to its first.
        let alone = shop_web();
        assert_eq!(key(next(&alone, "SHOP", Some(&last), &order)), "SHOP-22");
        // Nothing anywhere: nothing opens.
        assert_eq!(next(&[], "SHOP", None, &order), None);
    }

    /// NYS-02: a waiting plain Project agent is an item Next walks, after the tickets that need you and before the
    /// suggestions; the panel showing it counts as open.
    #[test]
    fn a_waiting_project_agent_is_an_item() {
        let mut tickets = shop_web();
        tickets.extend(every_group());
        let list = list(tickets);
        let items = super::items(&list);
        let order = order();
        let after_passed = next(&items, "SHOP", Some(&open_ticket("SHOP-40.4")), &order);
        assert_eq!(
            after_passed.unwrap().item().target(),
            Target::Agent {
                project: "SHOP".into(),
                agent: "pa3".into()
            }
        );
        let after_agent = next(
            &items,
            "SHOP",
            Some(&Open::Agent {
                agent: "pa3".into(),
            }),
            &order,
        );
        assert!(matches!(
            after_agent.unwrap().item().target(),
            Target::Ticket { ref key, .. } if key == "SHOP-40.6"
        ));
    }

    /// Resume all asks first, for the project's interrupted sessions, naming those it leaves out.
    #[test]
    fn resume_all_lists_the_projects_sessions() {
        let mut items = every_group();
        items.push(item(ticket(
            "SHOP-28",
            "Cart badge count",
            Status::StartInterrupted,
            "start failed",
        )));
        let mut other = ticket(
            "BILL-9",
            "Refund webhooks",
            Status::Interrupted,
            "session ended by restart",
        );
        other.group = NeedsYouGroup::Stopped;
        items.push(item(other));
        let d = resume_all_dialog(&items, Some("SHOP")).unwrap();
        assert_eq!(d.title, "Resume 3 interrupted sessions?");
        assert_eq!(d.confirm, "Resume 3");
        assert_eq!(
            d.included
                .iter()
                .map(|t| t.key.as_str())
                .collect::<Vec<_>>(),
            ["SHOP-33", "SHOP-29", "SHOP-30"]
        );
        assert_eq!(
            d.not_included.as_deref(),
            Some(
                "Not included: SHOP-28 Cart badge count. Its start never finished, so there\u{2019}s no session to \
                 resume. Use Retry start on its card."
            )
        );
        assert_eq!(resume_all_dialog(&items, None).unwrap().included.len(), 4);
        assert_eq!(resume_all_dialog(&shop_web(), Some("SHOP")), None);
    }

    #[test]
    fn a_childs_key_is_its_number() {
        assert_eq!(short_key(&"SHOP-40.4".parse().unwrap()), "40.4");
        assert_eq!(short_key(&"SHOP-22".parse().unwrap()), "SHOP-22");
    }

    /// The service's list as it arrives: tickets in queue order, the agents apart.
    fn list(items: Vec<Item>) -> InboxList {
        let mut tickets = Vec::new();
        let mut agents = Vec::new();
        for i in items {
            match i {
                Item::Ticket(t) => tickets.push(*t),
                Item::Agent(a) => agents.push(*a),
            }
        }
        tickets.sort_by_key(|t| t.group);
        InboxList {
            counts: InboxCounts::of(&tickets, &order()),
            items: tickets,
            project_agents: agents,
        }
    }
}
