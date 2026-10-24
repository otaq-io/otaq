//! The epic's sheet (SHT-07; d0059, d0080, d0083, d0084, d0091; canvas `s25-11`, "An epic opens like a ticket"): the
//! same sheet as a ticket's, opened from a lane head or a parent row. Its header says "1 of 5 merged" and the lead's
//! state ("Lead asks you", [`head`]); its details line Lead, Model, Tickets "5 · 1 merged · 1 working", Priority and the
//! epic's branch (`sheet::epic_details`); its tabs Overview · Tickets · Lead · Files · Activity.
//!
//! - **Overview** ([`overview`]): what the lead asks you with its options (`panel.view`'s answer, drawn as buttons),
//!   the description, **TICKETS** as a stage grid ([`grid`]: each child where it is now, Ready and every stage of the
//!   flow, and what it is doing) and the epic's own attachments.
//! - **Tickets** ([`tickets_view`]): every child as a List row (LST-01, `list_view::row`), with Add ticket to epic and the
//!   rule for when the epic moves on (the old panel's Epic section).
//! - **Lead** is the lead's session (`lead_tab.rs`), **Files** the epic's branch with its children's tags (FTB-06,
//!   `files_tab::epic_source`), **Activity** its history with the messages between its tickets and its lead.

use otaq_core::board::epics::progress_of;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, Status};
use otaq_core::epic_lead::overlap::is_running;
use otaq_core::project_agents::lead::{Lead, LeadStatus};
use serde::Serialize;
use ts_rs::TS;

use crate::theme::Palette;
use crate::view_models::board::BoardInput;
use crate::view_models::card_page::TicketCardView;
use crate::view_models::epic_review;
use crate::view_models::flow_columns::Columns;
use crate::view_models::lead_status::{self, Tone};
use crate::view_models::list_view::{self, Header, Row};
use crate::view_models::status::{self, PillView};

/// The grid's heading and its corner (`s25-11`).
pub const TICKETS: &str = "TICKETS";
pub const TICKET_NOW: &str = "TICKET \u{b7} NOW";
/// The grid's first column: Ready for Dev, before the flow's stages.
pub const READY: &str = "Ready";
/// The Attachments column's note on an epic.
pub const ATTACHMENTS_NOTE: &str = "The lead\u{2019}s plan and the epic\u{2019}s own reports. Each ticket keeps its own attachments.";
/// The Tickets tab with no child yet.
pub const NO_TICKETS: &str = "No tickets in this epic yet.";
pub const ADD_TICKET: &str = "Add ticket to epic";

/// "40.2" for a child of an epic; other keys whole.
pub fn short_key(ticket: &Ticket) -> String {
    match ticket.key.child {
        Some(child) => format!("{}.{child}", ticket.key.number),
        None => ticket.key.to_string(),
    }
}

/// `epic`'s children among `tickets`, by key.
pub fn children<'t>(epic: &Ticket, tickets: &'t [Ticket]) -> Vec<&'t Ticket> {
    let mut children: Vec<&Ticket> = tickets
        .iter()
        .filter(|t| t.epic.as_ref() == Some(&epic.key))
        .collect();
    children.sort_by(|a, b| a.key.cmp(&b.key));
    children
}

// ---- The header ----

/// The lead's state beside the title: "Lead asks you", "Lead re-planning". `EpicLeadPill` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EpicLeadPill")]
pub struct LeadPill {
    pub label: String,
    pub tone: Tone,
}

/// What an epic's header adds to the sheet's: "1 of 5 merged" and the lead's state (none without a lead: the
/// epic's own status shows). `EpicHead` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EpicHead")]
pub struct Head {
    pub progress: String,
    pub lead: Option<LeadPill>,
}

pub fn head(epic: &Ticket, tickets: &[Ticket], lead: Option<&Lead>) -> Head {
    let p = progress_of(&epic.key, tickets);
    Head {
        progress: p.label(),
        lead: lead
            .filter(|l| l.status != LeadStatus::None)
            .map(|l| LeadPill {
                label: format!("Lead {}", lead_status::words(&l.status).1),
                tone: lead_status::tone(&l.status),
            }),
    }
}

/// The details line's Tickets: "5 · 1 merged · 1 working" (the parts that are none left out).
pub fn tickets_detail(epic: &Ticket, tickets: &[Ticket]) -> String {
    let children = children(epic, tickets);
    let merged = children
        .iter()
        .filter(|c| c.status == Status::Merged)
        .count();
    let working = children.iter().filter(|c| is_running(c.status)).count();
    [
        Some(children.len().to_string()),
        (merged > 0).then(|| format!("{merged} merged")),
        (working > 0).then(|| format!("{working} working")),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" \u{b7} ")
}

// ---- Overview's stage grid ----

/// One cell of the grid: a passed stage (green), the one the child is in (its state's colour, `current`), or one
/// ahead (`fill` null: an empty ring). `EpicGridCell` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EpicGridCell")]
pub struct Cell {
    pub fill: Option<Palette>,
    pub current: bool,
}

/// A child in the grid: "40.2 Wishlist button", "Rust developer · working", and a cell per column. `EpicGridRow` on
/// the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EpicGridRow")]
pub struct GridRow {
    /// Its key: a click opens it.
    pub key: String,
    /// "40.2".
    pub short: String,
    pub title: String,
    /// What it is doing: "Merged · PR #470", "Rust developer · working", "Backlog · after 40.2".
    pub line: String,
    /// It needs you (or the lead asks you about it): the line in the attention colour.
    pub attention: bool,
    pub cells: Vec<Cell>,
}

/// TICKETS as a stage grid (`s25-11`). `EpicGrid` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EpicGrid")]
pub struct Grid {
    /// "TICKETS · 5".
    pub heading: String,
    /// "TICKET · NOW".
    pub corner: String,
    /// Ready, then each stage of the flow.
    pub columns: Vec<String>,
    /// Furthest along first.
    pub rows: Vec<GridRow>,
}

/// "Working" → "working", after a place or a name; a key ("SHOP-40.2 …") stays as it is.
fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(_), Some(second)) if second.is_uppercase() => text.to_owned(),
        (Some(first), _) => first.to_lowercase().chain(text.chars().skip(1)).collect(),
        (None, _) => String::new(),
    }
}

/// What the child is doing, as the grid says it, and whether it needs you.
fn line(child: &Ticket, card: &TicketCardView, columns: &Columns) -> (String, bool) {
    let attention = card.edge.is_some();
    let said = card
        .status
        .as_ref()
        .map(|s| s.text.clone())
        .filter(|s| !s.is_empty());
    if child.place == Place::Done {
        let text = if child.status == Status::Merged {
            match child.flags.pr_number {
                Some(n) => format!("Merged \u{b7} PR #{n}"),
                None => "Merged".to_owned(),
            }
        } else {
            PillView::of(status::shown(child)).label
        };
        return (text, false);
    }
    if attention {
        let text = said.unwrap_or_else(|| PillView::of(status::shown(child)).label);
        return (text, true);
    }
    if is_running(child.status)
        && let Some(who) = &card.who
    {
        let doing = said
            .as_deref()
            .map_or_else(|| "working".to_owned(), lower_first);
        return (format!("{who} \u{b7} {doing}"), false);
    }
    let place = match &child.place {
        Place::Backlog => list_view::BACKLOG.to_owned(),
        Place::Waiting(None) => READY.to_owned(),
        Place::Waiting(Some(_)) | Place::InStage(_) | Place::Done => columns
            .stage_of(child)
            .map_or_else(|| READY.to_owned(), |s| s.name.clone()),
    };
    let text = match said {
        Some(said) => format!("{place} \u{b7} {}", lower_first(&said)),
        None => place,
    };
    (text, false)
}

/// How far along the child is, for the order: Done, then the later stages, Ready, Backlog.
fn progress_rank(child: &Ticket, columns: &Columns) -> usize {
    let flow = columns.flow();
    match &child.place {
        Place::Done => usize::MAX,
        Place::Backlog => 0,
        Place::Waiting(None) => 1,
        Place::Waiting(Some(_)) | Place::InStage(_) => {
            2 + columns
                .stage_of(child)
                .and_then(|s| flow.stages.iter().position(|f| f.id == s.id))
                .unwrap_or(0)
        }
    }
}

/// Overview's TICKETS (SHT-07): the children of `epic` in `input`'s tickets, furthest along first (then by key), each
/// with a cell for Ready and every stage of the flow.
pub fn grid(epic: &Ticket, input: &BoardInput) -> Grid {
    let columns = Columns::of(input.flow);
    let mut children = children(epic, input.tickets);
    children.sort_by(|a, b| {
        progress_rank(b, &columns)
            .cmp(&progress_rank(a, &columns))
            .then_with(|| a.key.cmp(&b.key))
    });
    let rows = children
        .iter()
        .map(|child| {
            let card = TicketCardView::from_vm(
                crate::view_models::board::ticket_card(input, child),
                child,
            );
            let (line, attention) = line(child, &card, &columns);
            let here = PillView::of(status::shown(child)).palette;
            let track = list_view::track(&columns, child);
            let at = columns
                .stage_of(child)
                .and_then(|s| columns.flow().stages.iter().position(|f| f.id == s.id));
            let done = child.place == Place::Done;
            let ready = match &child.place {
                Place::Backlog => Cell {
                    fill: None,
                    current: false,
                },
                Place::Waiting(None) => Cell {
                    fill: Some(here),
                    current: true,
                },
                _ => Cell {
                    fill: Some(Palette::Approved),
                    current: false,
                },
            };
            let cells = std::iter::once(ready)
                .chain(track.steps.iter().enumerate().map(|(i, step)| Cell {
                    fill: step.fill,
                    current: !done && Some(i) == at,
                }))
                .collect();
            GridRow {
                key: child.key.to_string(),
                short: short_key(child),
                title: child.title.clone(),
                line,
                attention,
                cells,
            }
        })
        .collect();
    Grid {
        heading: format!("{TICKETS} \u{b7} {}", children.len()),
        corner: TICKET_NOW.to_owned(),
        columns: std::iter::once(READY.to_owned())
            .chain(columns.flow().stages.iter().map(|s| s.name.clone()))
            .collect(),
        rows,
    }
}

/// What Overview adds for an epic (`epic.overview`): the grid and the Attachments column's note. `EpicOverviewView`
/// on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "EpicOverviewView")]
pub struct OverviewView {
    pub key: String,
    pub grid: Grid,
    pub attachments_note: String,
}

pub fn overview(epic: &Ticket, input: &BoardInput) -> OverviewView {
    OverviewView {
        key: epic.key.to_string(),
        grid: grid(epic, input),
        attachments_note: ATTACHMENTS_NOTE.to_owned(),
    }
}

// ---- The Tickets tab ----

/// The Tickets tab (`epic.tickets`): every child as a List row, then Add ticket to epic with the next key and when
/// the epic moves on. `EpicTicketsView` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "EpicTicketsView")]
pub struct TicketsView {
    pub key: String,
    pub header: Header,
    pub rows: Vec<Row>,
    /// "No tickets in this epic yet."
    pub empty: Option<String>,
    pub add: String,
    /// "· new tickets get the next key, SHOP-40.6".
    pub next_key: Option<String>,
    /// When every child is merged the epic moves to Ready for Review (EPC-04).
    pub note: String,
}

pub fn tickets_view(epic: &Ticket, input: &BoardInput) -> TicketsView {
    let columns = Columns::of(input.flow);
    let start = list_view::start_target(&columns);
    let rows: Vec<Row> = children(epic, input.tickets)
        .into_iter()
        .map(|child| list_view::row(input, &columns, start.as_ref(), child))
        .collect();
    let header = epic_review::header(epic, input.tickets);
    TicketsView {
        key: epic.key.to_string(),
        header: Header {
            ticket: list_view::HEADER_TICKET.to_owned(),
            flow: list_view::HEADER_FLOW.to_owned(),
            happening: list_view::HEADER_HAPPENING.to_owned(),
            who: list_view::HEADER_WHO.to_owned(),
        },
        empty: rows.is_empty().then(|| NO_TICKETS.to_owned()),
        rows,
        add: ADD_TICKET.to_owned(),
        next_key: header.next_key,
        note: header.note,
    }
}

/// The Gallery's epic sheet (`gallery.epic_sheet`): the Wishlist epic's header with its lead asking, Overview's grid
/// and the Tickets tab. `EpicSheetGallery` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "EpicSheetGallery")]
pub struct Gallery {
    pub head: Head,
    pub overview: OverviewView,
    pub tickets: TicketsView,
}

pub fn gallery() -> Gallery {
    let flow = samples::shop_web();
    let tickets = samples::wishlist();
    let epic = &tickets[0];
    let input = crate::view_models::list_view::samples::input(&flow, &tickets);
    let lead = crate::view_models::lead_status::sample(epic, LeadStatus::AsksYou, 0);
    Gallery {
        head: head(epic, &tickets, Some(&lead)),
        overview: overview(epic, &input),
        tickets: tickets_view(epic, &input),
    }
}

/// The example's Wishlist epic (`s25-11`), for the Gallery and the tests.
pub mod samples {
    use otaq_core::board::tickets::Ticket;
    use otaq_core::domain::{Kind, Place, Status};
    use otaq_core::flow::Flow;

    use crate::view_models::card_fixtures::ticket;

    /// The example's shop-web flow for `s25-11`: Grill, Dev, Validate (in Dev's column), Security, Review, PR.
    pub fn shop_web() -> Flow {
        crate::view_models::list_view::samples::shop_web()
    }

    fn child(key: &str, title: &str, status: Status, place: Place) -> Ticket {
        let mut t = ticket(key, title, status);
        t.place = place;
        t.epic = Some("SHOP-40".parse().unwrap());
        t.base = otaq_core::board::tickets::Base::Epic("SHOP-40".parse().unwrap());
        t
    }

    /// The Wishlist epic (PAY-123) and its five children (`s25-11`): 40.1 merged (PR #470), 40.2 in Dev, 40.3 in
    /// Backlog after 40.2, 40.4 and 40.5 waiting in Ready.
    pub fn wishlist() -> Vec<Ticket> {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Working);
        epic.kind = Kind::Epic;
        epic.external_id = Some("PAY-123".into());
        epic.place = Place::Backlog;
        epic.next_child_number = Some(6);
        let mut merged = child("SHOP-40.1", "Wishlist API", Status::Merged, Place::Done);
        merged.flags.pr_number = Some(470);
        let mut page = child(
            "SHOP-40.3",
            "Wishlist page",
            Status::Backlog,
            Place::Backlog,
        );
        page.depends_on = vec!["SHOP-40.2".parse().unwrap()];
        let mut share = child(
            "SHOP-40.4",
            "Share link",
            Status::Queued,
            Place::Waiting(None),
        );
        share.queued_at = Some(3);
        share.depends_on = vec!["SHOP-40.2".parse().unwrap()];
        let mut empty = child(
            "SHOP-40.5",
            "Empty state",
            Status::Queued,
            Place::Waiting(None),
        );
        empty.queued_at = Some(4);
        vec![
            epic,
            merged,
            child(
                "SHOP-40.2",
                "Wishlist button",
                Status::Working,
                Place::stage_of("dev"),
            ),
            page,
            share,
            empty,
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::samples::{shop_web, wishlist};
    use super::*;
    use crate::view_models::lead_status::sample;
    use crate::view_models::list_view::samples::input;

    fn shown(cells: &[Cell]) -> String {
        cells
            .iter()
            .map(|c| match (c.fill, c.current) {
                (None, _) => '\u{b7}',
                (Some(Palette::Approved), false) => 'v',
                (Some(_), true) => '@',
                (Some(_), false) => '?',
            })
            .collect()
    }

    /// The Wishlist epic's Overview (`s25-11`): TICKETS · 5 over Ready and the flow's stages, furthest along first,
    /// each child where it is now and what it is doing.
    #[test]
    fn the_wishlist_epics_stage_grid() {
        let flow = shop_web();
        let tickets = wishlist();
        let epic = &tickets[0];
        let view = overview(epic, &input(&flow, &tickets));
        assert_eq!(view.key, "SHOP-40");
        assert_eq!(view.attachments_note, ATTACHMENTS_NOTE);
        let grid = view.grid;
        assert_eq!(grid.heading, "TICKETS \u{b7} 5");
        assert_eq!(grid.corner, "TICKET \u{b7} NOW");
        assert_eq!(
            grid.columns,
            [
                "Ready", "Grill", "Dev", "Validate", "Security", "Review", "PR open"
            ]
        );
        let rows: Vec<(&str, &str, String)> = grid
            .rows
            .iter()
            .map(|r| (r.short.as_str(), r.title.as_str(), shown(&r.cells)))
            .collect();
        assert_eq!(
            rows,
            [
                ("40.1", "Wishlist API", "vvvvvvv".to_owned()),
                (
                    "40.2",
                    "Wishlist button",
                    "vv@\u{b7}\u{b7}\u{b7}\u{b7}".to_owned()
                ),
                (
                    "40.4",
                    "Share link",
                    "@\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}".to_owned()
                ),
                (
                    "40.5",
                    "Empty state",
                    "@\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}".to_owned()
                ),
                (
                    "40.3",
                    "Wishlist page",
                    "\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}\u{b7}".to_owned()
                ),
            ]
        );
        assert_eq!(grid.rows[0].line, "Merged \u{b7} PR #470");
        assert_eq!(grid.rows[0].key, "SHOP-40.1");
        assert_eq!(grid.rows[1].cells[2].fill, Some(Palette::Working));
        assert_eq!(grid.rows[3].cells[0].fill, Some(Palette::Queued));
        let lines: Vec<&str> = grid.rows.iter().map(|r| r.line.as_str()).collect();
        assert_eq!(
            lines,
            [
                "Merged \u{b7} PR #470",
                "Dev \u{b7} working",
                // DPV-01's blocked line, as the children's cards say it (T-0463).
                "Ready \u{b7} blocked \u{b7} waits for SHOP-40.2 to merge",
                "Ready \u{b7} starting\u{2026}",
                "Backlog \u{b7} blocked \u{b7} waits for SHOP-40.2 to merge",
            ]
        );
        assert!(grid.rows.iter().all(|r| !r.attention));
    }

    /// A working child names its agent once several agents are shown: "Rust developer · working".
    #[test]
    fn a_working_child_says_who_works_on_it() {
        let flow = shop_web();
        let tickets = wishlist();
        let columns = Columns::of(&flow);
        let child = &tickets[2];
        let mut card = TicketCardView::from_vm(
            crate::view_models::board::ticket_card(&input(&flow, &tickets), child),
            child,
        );
        card.who = Some("Rust developer".into());
        assert_eq!(
            line(child, &card, &columns),
            ("Rust developer \u{b7} working".to_owned(), false)
        );
        card.who = None;
        assert!(line(child, &card, &columns).0.starts_with("Dev"));
    }

    /// The header: "1 of 5 merged" and "Lead asks you"; the details' Tickets "5 · 1 merged · 1 working".
    #[test]
    fn the_header_and_the_tickets_detail() {
        let tickets = wishlist();
        let epic = &tickets[0];
        let lead = sample(epic, LeadStatus::AsksYou, 0);
        let h = head(epic, &tickets, Some(&lead));
        assert_eq!(h.progress, "1 of 5 merged");
        assert_eq!(
            h.lead,
            Some(LeadPill {
                label: "Lead asks you".into(),
                tone: Tone::AsksYou
            })
        );
        let replanning = sample(epic, LeadStatus::Replanning, 0);
        assert_eq!(
            head(epic, &tickets, Some(&replanning)).lead.unwrap().label,
            "Lead re-planning"
        );
        assert_eq!(head(epic, &tickets, None).lead, None);
        assert_eq!(
            tickets_detail(epic, &tickets),
            "5 \u{b7} 1 merged \u{b7} 1 working"
        );
        assert_eq!(tickets_detail(epic, &tickets[..1]), "0");
    }

    /// The Tickets tab: every child as a List row in key order, Start on the Backlog one, and Add ticket to epic.
    #[test]
    fn the_tickets_tab_lists_every_child() {
        let flow = shop_web();
        let tickets = wishlist();
        let view = tickets_view(&tickets[0], &input(&flow, &tickets));
        let keys: Vec<&str> = view.rows.iter().map(|r| r.card.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "SHOP-40.1",
                "SHOP-40.2",
                "SHOP-40.3",
                "SHOP-40.4",
                "SHOP-40.5"
            ]
        );
        assert!(view.rows[2].start.is_some(), "40.3 is in Backlog");
        assert!(view.rows[1].start.is_none());
        assert_eq!(view.rows[1].track.label, "Dev");
        assert_eq!(view.header.flow, list_view::HEADER_FLOW);
        assert_eq!(view.add, ADD_TICKET);
        assert_eq!(
            view.next_key.as_deref(),
            Some("\u{b7} new tickets get the next key, SHOP-40.6")
        );
        assert_eq!(view.empty, None);
        let alone = tickets_view(&tickets[0], &input(&flow, &tickets[..1]));
        assert_eq!(alone.empty.as_deref(), Some(NO_TICKETS));
    }
}

#[cfg(test)]
mod gallery_tests {
    /// The Gallery's epic sheet: Wishlist's lead asks, its grid and its five tickets.
    #[test]
    fn the_gallery_draws_the_wishlist_epic() {
        let g = super::gallery();
        assert_eq!(g.head.lead.unwrap().label, "Lead asks you");
        assert_eq!(g.overview.grid.rows.len(), 5);
        assert_eq!(g.tickets.rows.len(), 5);
    }
}
