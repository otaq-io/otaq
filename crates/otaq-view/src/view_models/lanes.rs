//! Epics on the Board and in the List (EPL-01, EPL-02; d0080 refined, d0083, d0091; `s24-06`).
//!
//! **Lanes: Epic** ([`build`]): the Board splits into one lane per epic across the same columns, "No epic" last. A
//! lane's head has the epic's name, its External ID (or its key), "5 tickets", a progress bar of its merged children,
//! and its lead with what it's doing ("Wishlist lead · re-planning"); an epic the Project agent made says "from the
//! Project agent" (d0091). Lanes fold by their head (`ViewPrefs::folded_lanes`, SHB-05). Each lane's cells hold the
//! cards of its column the Board would show, in the Board's order; a folded column's cell is empty. Dragging a lane
//! head is dragging its epic: the page sends the epic's key through `board.drag` / `board.drop`, so 0080's epic drags
//! (queue, take back, Park all) run unchanged.
//!
//! Which lane a ticket is in: an epic that isn't Done has a lane with its own card and its children; every other
//! ticket is in "No epic", including a Done epic and its children (Q-A). Under a search or filter a lane with no card
//! left is not drawn.
//!
//! **Group by Epic** ([`by_epic`]): the List's epic groups (EPL-02). Each epic is a parent row with its own track
//! (where the epic as a whole is: the earliest stage a child works in, else Ready for Dev while one waits, else
//! Backlog; the epic's own place once it is past Backlog), "1 of 5 merged · Wishlist lead is re-planning" (or "· from
//! the Project agent"), and its children nested under it in key order; parents fold with the same preference as
//! lanes. A click on a parent opens the epic like a ticket. The List (T-0461) draws each child with its own row.
//!
//! Order (d0047, d0080): pinned epics (most recently pinned first) → an epic with a child that needs the user (or
//! whose lead asks you) → the rest, by when the epic was first queued (never queued last), then by key.

use std::cmp::Reverse;

use otaq_core::board::epic_flight::children_of;
use otaq_core::board::epics::progress_of;
use otaq_core::board::settings::ViewPrefs;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, StatusSet, TicketKey};
use otaq_core::flow::Flow;
use otaq_core::project_agents::lead::{Lead, LeadStatus};
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::board::{self, BoardInput, is_card};
use crate::view_models::board_page::{BoardCard, ColumnId, ColumnPage, DoneFold};
use crate::view_models::flow_columns::{ColumnKind, Columns};
use crate::view_models::lead_status::{self, LeadStatusView};
use crate::view_models::view_prefs::Change;

/// The last lane's name and the List's last group.
pub const NO_EPIC: &str = "No epic";
/// The "No epic" lane's id in `ViewPrefs::folded_lanes` (epic lanes go by the epic's key).
pub const NO_EPIC_LANE: &str = "no-epic";
/// What an epic the Project agent made says after its count (d0091).
pub const FROM_PROJECT_AGENT: &str = "from the Project agent";
/// The fold button's tooltips.
pub const FOLD_LANE: &str = "Fold this lane";
pub const UNFOLD_LANE: &str = "Unfold this lane";
/// The List's heading over the parent rows.
pub const EPICS: &str = "Epics";

/// One lane of the Board's Lanes: Epic (EPL-01). `BoardLane` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BoardLane")]
pub struct Lane {
    /// The epic's key, or [`NO_EPIC_LANE`].
    pub id: String,
    /// The epic's key: what dragging the head drags, its right-click menu and `g e`; `null` for "No epic".
    pub epic: Option<String>,
    pub head: LaneHead,
    /// Folded: only the head shows.
    pub folded: bool,
    /// What the head's fold button sends.
    pub fold: Change,
    /// "Fold this lane" or "Unfold this lane".
    pub fold_label: String,
    /// One per board column, in the board's order; a folded column's cell has no cards.
    pub cells: Vec<LaneCell>,
}

/// A lane's head. `BoardLaneHead` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardLaneHead")]
pub struct LaneHead {
    /// "Wishlist", "No epic".
    pub name: String,
    /// "PAY-123 · 5 tickets", "SHOP-34 · 3 tickets · from the Project agent", "5 tickets".
    pub detail: String,
    /// The bar of merged children; `null` for "No epic".
    pub progress: Option<LaneProgress>,
    /// The lead's status in its full form ("Wishlist lead · re-planning"); `null` without a live lead.
    pub lead: Option<LeadStatusView>,
    /// A child needs the user or the lead asks you: the folded lane's dot.
    pub needs_you: bool,
    pub pinned: bool,
}

/// The bar on a lane head. `BoardLaneProgress` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BoardLaneProgress")]
pub struct LaneProgress {
    pub merged: u32,
    pub total: u32,
    /// "1 of 5 merged": the bar's tooltip and accessible name.
    pub label: String,
}

/// A lane's part of one column. `BoardLaneCell` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "BoardLaneCell")]
pub struct LaneCell {
    pub column: ColumnId,
    /// In the column's order (Dev's cards that need the user first, Review's sections one after the other).
    pub cards: Vec<BoardCard>,
    /// Done: the epic's merged children as one line (EPC-16); its cards follow each other in `cards`.
    pub folds: Vec<DoneFold>,
}

/// Whether `epic` has a lane: every epic until it is Done.
fn has_lane(epic: &Ticket) -> bool {
    epic.is_epic() && epic.status.set() != StatusSet::Done
}

/// The epic whose lane `ticket` is in; `None` for "No epic".
pub fn lane_of<'t>(ticket: &Ticket, tickets: &'t [Ticket]) -> Option<&'t Ticket> {
    let key = if ticket.is_epic() {
        &ticket.key
    } else {
        ticket.epic.as_ref()?
    };
    tickets
        .iter()
        .find(|t| &t.key == key)
        .filter(|e| has_lane(e))
}

/// The epics with a lane, in lane order.
pub fn epics_in_order<'t>(tickets: &'t [Ticket], leads: &[Lead]) -> Vec<&'t Ticket> {
    let mut epics: Vec<&Ticket> = tickets.iter().filter(|t| has_lane(t)).collect();
    epics.sort_by_key(|epic| order_key(epic, tickets, lead_of(epic, leads)));
    epics
}

fn lead_of<'l>(epic: &Ticket, leads: &'l [Lead]) -> Option<&'l Lead> {
    leads
        .iter()
        .find(|l| l.epic == epic.key && l.status != LeadStatus::None)
}

/// Pinned (most recently pinned first) → a child needs the user or the lead asks → the rest, by when the epic was
/// first queued (its earliest child's queue time); never queued last; ties go to the lower key.
fn order_key(
    epic: &Ticket,
    tickets: &[Ticket],
    lead: Option<&Lead>,
) -> (u8, Reverse<i64>, bool, Option<i64>, TicketKey) {
    let children = children_of(epic, tickets);
    let queued = children
        .iter()
        .filter_map(|c| c.queued_at.or(c.started_at))
        .min();
    let group = if epic.flags.is_pinned() {
        0
    } else if needs_you(epic, tickets, lead) {
        1
    } else {
        2
    };
    (
        group,
        Reverse(epic.flags.pinned_at.unwrap_or(0)),
        queued.is_none(),
        queued,
        epic.key.clone(),
    )
}

fn needs_you(epic: &Ticket, tickets: &[Ticket], lead: Option<&Lead>) -> bool {
    children_of(epic, tickets)
        .iter()
        .any(|c| c.shows_needs_you())
        || lead.is_some_and(|l| l.status == LeadStatus::AsksYou)
}

fn from_project_agent(epic: &Ticket) -> bool {
    matches!(epic.created_by, Actor::ProjectAgent(_))
}

fn tickets_word(n: usize) -> String {
    match n {
        1 => "1 ticket".to_owned(),
        n => format!("{n} tickets"),
    }
}

/// "1 of 5 merged".
fn merged_label(epic: &Ticket, tickets: &[Ticket]) -> LaneProgress {
    let p = progress_of(&epic.key, tickets);
    LaneProgress {
        merged: p.merged,
        total: p.total,
        label: p.label(),
    }
}

/// The head of `epic`'s lane.
pub fn head(epic: &Ticket, tickets: &[Ticket], lead: Option<&Lead>) -> LaneHead {
    let progress = merged_label(epic, tickets);
    let mut detail = vec![
        epic.external_id
            .clone()
            .filter(|e| !e.is_empty())
            .unwrap_or_else(|| epic.key.to_string()),
        tickets_word(progress.total as usize),
    ];
    if from_project_agent(epic) {
        detail.push(FROM_PROJECT_AGENT.to_owned());
    }
    LaneHead {
        name: epic.title.clone(),
        detail: detail.join(" \u{b7} "),
        progress: Some(progress),
        lead: lead.map(|l| LeadStatusView::of(epic, Some(l))),
        needs_you: needs_you(epic, tickets, lead),
        pinned: epic.flags.is_pinned(),
    }
}

fn fold_of(id: &str, folded: bool) -> (Change, String) {
    (
        Change::FoldLane {
            epic: id.to_owned(),
            folded: !folded,
        },
        if folded { UNFOLD_LANE } else { FOLD_LANE }.to_owned(),
    )
}

/// Every card of a column page, in its order.
fn cards_of(column: ColumnPage) -> (Vec<BoardCard>, Vec<DoneFold>) {
    if column.collapsed {
        return (Vec::new(), Vec::new());
    }
    let mut cards = column.cards;
    if let Some(dev) = column.dev {
        cards.extend(dev.top);
        cards.extend(dev.working);
    }
    for section in column.sections {
        cards.extend(section.cards);
    }
    (cards, column.folds)
}

/// The Board's lanes (EPL-01) for `input` (the board's own input: the filter's `visible`, the focus, the folds),
/// with the project's `leads` and view preferences.
pub fn build(input: &BoardInput, prefs: &ViewPrefs, leads: &[Lead]) -> Vec<Lane> {
    let tickets = input.tickets;
    let filtered = input.visible.is_some();
    let shown = |t: &Ticket| input.visible.is_none_or(|v| v.contains(&t.key));
    let cells = |keys: Vec<TicketKey>| -> Vec<LaneCell> {
        let lane = BoardInput {
            visible: Some(&keys),
            // A lane shows every card of its column: Dev's fold is the whole board's.
            dev_budget: None,
            ..*input
        };
        let mut columns = board::build(&lane);
        crate::view_models::board_page::with_leads(&mut columns, tickets, leads);
        columns
            .into_iter()
            .map(|c| {
                let page = crate::view_models::board_page::column_page(c, tickets);
                let column = page.id.clone();
                let (cards, folds) = cards_of(page);
                LaneCell {
                    column,
                    cards,
                    folds,
                }
            })
            .collect()
    };
    let folded = |id: &str| prefs.folded_lanes.iter().any(|f| f == id);
    let mut lanes: Vec<Lane> = epics_in_order(tickets, leads)
        .into_iter()
        .map(|epic| {
            let keys: Vec<TicketKey> = tickets
                .iter()
                .filter(|t| lane_of(t, tickets).is_some_and(|e| e.key == epic.key) && shown(t))
                .map(|t| t.key.clone())
                .collect();
            let id = epic.key.to_string();
            let (fold, fold_label) = fold_of(&id, folded(&id));
            Lane {
                folded: folded(&id),
                epic: Some(id.clone()),
                head: head(epic, tickets, lead_of(epic, leads)),
                fold,
                fold_label,
                cells: cells(keys),
                id,
            }
        })
        .collect();
    let rest: Vec<TicketKey> = tickets
        .iter()
        .filter(|t| lane_of(t, tickets).is_none() && shown(t))
        .map(|t| t.key.clone())
        .collect();
    let rest_cells = cells(rest);
    let open = rest_cells.iter().map(|c| c.cards.len()).sum::<usize>();
    let (fold, fold_label) = fold_of(NO_EPIC_LANE, folded(NO_EPIC_LANE));
    lanes.push(Lane {
        id: NO_EPIC_LANE.to_owned(),
        epic: None,
        head: LaneHead {
            name: NO_EPIC.to_owned(),
            detail: tickets_word(open),
            progress: None,
            lead: None,
            needs_you: tickets
                .iter()
                .any(|t| lane_of(t, tickets).is_none() && shown(t) && t.shows_needs_you()),
            pinned: false,
        },
        folded: folded(NO_EPIC_LANE),
        fold,
        fold_label,
        cells: rest_cells,
    });
    if filtered {
        lanes.retain(|l| l.cells.iter().any(|c| !c.cards.is_empty()));
    }
    lanes
}

/// Where a row is in the flow (LST-01): the board's columns, its place filled. `ListRowTrack` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ListRowTrack")]
pub struct RowTrack {
    /// The columns' titles, in board order.
    pub stages: Vec<String>,
    /// The filled one.
    pub at: usize,
    /// Its title ("Dev"), next to the track.
    pub label: String,
}

/// An epic's parent row in the List grouped by Epic (EPL-02). `ListParentRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ListParentRow")]
pub struct ParentRow {
    /// The epic's key: a click opens the epic like a ticket (SHT-07).
    pub key: String,
    pub external_id: Option<String>,
    pub name: String,
    pub track: RowTrack,
    /// "1 of 5 merged · Wishlist lead is re-planning", "0 of 3 merged · from the Project agent".
    pub line: String,
    /// The WHO column: the lead; `null` without a live lead.
    pub lead: Option<LeadStatusView>,
    /// A child needs the user or the lead asks you.
    pub needs_you: bool,
}

/// One epic's group: its parent row, then its children's rows. `ListEpicGroup` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ListEpicGroup")]
pub struct EpicGroup {
    pub parent: ParentRow,
    /// The children's keys in key order; the List draws each with its own row (a child that needs you keeps its edge
    /// and action).
    pub children: Vec<String>,
    /// Folded: only the parent row shows.
    pub folded: bool,
    pub fold: Change,
    pub fold_label: String,
}

/// The List grouped by Epic (EPL-02): "Epics 2", each epic's group, then "No epic 5" with its rows.
/// `ListByEpic` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ListByEpic")]
pub struct ByEpic {
    /// "Epics".
    pub epics_title: String,
    /// "2".
    pub epics_count: String,
    pub groups: Vec<EpicGroup>,
    /// "No epic".
    pub rest_title: String,
    /// "5".
    pub rest_count: String,
    /// The tickets with no lane, in key order (the List orders its rows as it does in every group).
    pub rest: Vec<String>,
}

/// What the lead is doing, for the parent row ("Wishlist lead is re-planning", "Wishlist lead asks you").
pub fn lead_doing(lead: &Lead) -> String {
    let (state, short) = lead_status::words(&lead.status);
    match &lead.status {
        LeadStatus::AsksYou => format!("{} asks you", lead.name),
        LeadStatus::Answering { .. } => format!("{} is {short}", lead.name),
        LeadStatus::Idle
        | LeadStatus::CheckingPlan
        | LeadStatus::Replanning
        | LeadStatus::Paused => format!("{} is {state}", lead.name),
        LeadStatus::None
        | LeadStatus::EndedByRestart
        | LeadStatus::EndedInReview
        | LeadStatus::Ended => format!("{} \u{b7} {short}", lead.name),
    }
}

/// Where `epic` is as a whole: its own place once past Backlog, else the earliest stage a child works in, else
/// Ready for Dev while a child waits there, else Backlog (all of them done: Done).
pub fn epic_track(epic: &Ticket, tickets: &[Ticket], flow: &Flow) -> RowTrack {
    let columns = Columns::of(flow);
    let ids: Vec<&str> = columns.iter().map(|c| c.id.as_str()).collect();
    let index = |t: &Ticket| {
        let spot = columns.spot(t).column;
        ids.iter().position(|id| *id == spot).unwrap_or(0)
    };
    let children = children_of(epic, tickets);
    let at = if !epic.place.is_backlog() {
        index(epic)
    } else if let Some(i) = children
        .iter()
        .filter(|c| c.status.set() != StatusSet::Done && !c.place.is_backlog())
        .map(|c| index(c))
        .filter(|i| {
            columns
                .iter()
                .nth(*i)
                .is_some_and(|c| c.kind != ColumnKind::ReadyForDev)
        })
        .min()
    {
        i
    } else if children.iter().any(|c| c.place.is_waiting()) {
        ids.iter()
            .position(|id| {
                columns
                    .get(id)
                    .is_some_and(|c| c.kind == ColumnKind::ReadyForDev)
            })
            .unwrap_or(0)
    } else if !children.is_empty() && children.iter().all(|c| c.status.set() == StatusSet::Done) {
        ids.len() - 1
    } else {
        0
    };
    let stages: Vec<String> = columns.iter().map(|c| c.title.clone()).collect();
    RowTrack {
        label: stages[at].clone(),
        stages,
        at,
    }
}

/// The List grouped by Epic (EPL-02) for a project's `tickets` on `flow`, with its `leads` and view preferences.
/// `visible` is the search's and filter's tickets (`None`: all); an epic with no child shown and not shown itself is
/// left out.
pub fn by_epic(
    tickets: &[Ticket],
    flow: &Flow,
    leads: &[Lead],
    prefs: &ViewPrefs,
    visible: Option<&[TicketKey]>,
) -> ByEpic {
    let shown = |t: &Ticket| visible.is_none_or(|v| v.contains(&t.key));
    let groups: Vec<EpicGroup> = epics_in_order(tickets, leads)
        .into_iter()
        .filter_map(|epic| {
            let mut children: Vec<&Ticket> = children_of(epic, tickets)
                .into_iter()
                .filter(|c| shown(c))
                .collect();
            if children.is_empty() && !shown(epic) {
                return None;
            }
            children.sort_by(|a, b| a.key.cmp(&b.key));
            let lead = lead_of(epic, leads);
            let progress = merged_label(epic, tickets);
            let line = match lead {
                Some(l) => format!("{} \u{b7} {}", progress.label, lead_doing(l)),
                None if from_project_agent(epic) => {
                    format!("{} \u{b7} {FROM_PROJECT_AGENT}", progress.label)
                }
                None => progress.label,
            };
            let id = epic.key.to_string();
            let folded = prefs.folded_lanes.contains(&id);
            let (fold, fold_label) = fold_of(&id, folded);
            Some(EpicGroup {
                parent: ParentRow {
                    key: id,
                    external_id: epic.external_id.clone().filter(|e| !e.is_empty()),
                    name: epic.title.clone(),
                    track: epic_track(epic, tickets, flow),
                    line,
                    lead: lead.map(|l| LeadStatusView::of(epic, Some(l))),
                    needs_you: needs_you(epic, tickets, lead),
                },
                children: children.iter().map(|c| c.key.to_string()).collect(),
                folded,
                fold,
                fold_label,
            })
        })
        .collect();
    let mut rest: Vec<&Ticket> = tickets
        .iter()
        .filter(|t| lane_of(t, tickets).is_none() && is_card(t, tickets) && shown(t))
        .collect();
    rest.sort_by(|a, b| a.key.cmp(&b.key));
    ByEpic {
        epics_title: EPICS.to_owned(),
        epics_count: groups.len().to_string(),
        groups,
        rest_title: NO_EPIC.to_owned(),
        rest_count: rest.len().to_string(),
        rest: rest.iter().map(|t| t.key.to_string()).collect(),
    }
}

/// The gallery's lane heads (`s24-06`): Wishlist with its lead in each state, an epic from the Project agent, and
/// "No epic". `LaneGalleryHead` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LaneGalleryHead")]
pub struct GalleryHead {
    pub name: String,
    pub head: LaneHead,
    pub folded: bool,
}

pub fn gallery_heads() -> Vec<GalleryHead> {
    use crate::view_models::card_fixtures::ticket;
    use crate::view_models::lead_status::sample;
    use otaq_core::domain::{Kind, Status};

    let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
    epic.kind = Kind::Epic;
    epic.external_id = Some("PAY-123".into());
    let child = |n: u32, status: Status| {
        let mut t = ticket(&format!("SHOP-40.{n}"), "Wishlist", status);
        t.epic = Some(epic.key.clone());
        t
    };
    let tickets = vec![
        epic.clone(),
        child(1, Status::Merged),
        child(2, Status::Working),
        child(3, Status::Backlog),
        child(4, Status::Queued),
        child(5, Status::Queued),
    ];
    let mut slow = ticket("SHOP-34", "Checkout slowness", Status::Backlog);
    slow.kind = Kind::Epic;
    slow.created_by = Actor::ProjectAgent("pa-1".into());
    let slow_children: Vec<Ticket> = (1..=3)
        .map(|n| {
            let mut t = ticket(&format!("SHOP-34.{n}"), "Checkout", Status::Queued);
            t.epic = Some(slow.key.clone());
            t
        })
        .chain([slow.clone()])
        .collect();
    let cell = |name: &str, head: LaneHead, folded: bool| GalleryHead {
        name: name.to_owned(),
        head,
        folded,
    };
    let lead = |status| sample(&epic, status, 1_000);
    let mut out: Vec<GalleryHead> = [
        ("Lead · idle", LeadStatus::Idle),
        ("Lead · re-planning", LeadStatus::Replanning),
        ("Lead · asks you", LeadStatus::AsksYou),
        ("Lead · ended by restart", LeadStatus::EndedByRestart),
    ]
    .into_iter()
    .map(|(name, status)| cell(name, head(&epic, &tickets, Some(&lead(status))), false))
    .collect();
    out.push(cell("No lead", head(&epic, &tickets, None), false));
    out.push(cell(
        "From the Project agent",
        head(&slow, &slow_children, None),
        false,
    ));
    out.push(cell(
        "Folded · a child needs you",
        LaneHead {
            needs_you: true,
            ..head(&epic, &tickets, Some(&lead(LeadStatus::AsksYou)))
        },
        true,
    ));
    out.push(cell(
        "No epic",
        LaneHead {
            name: NO_EPIC.to_owned(),
            detail: tickets_word(5),
            progress: None,
            lead: None,
            needs_you: false,
            pinned: false,
        },
        false,
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_labels::KeyLabels;
    use crate::keymap::Platform;
    use crate::view_models::board_page::{Params, ProjectFacts, page};
    use crate::view_models::card_fixtures::{NOW, ticket};
    use crate::view_models::lead_status::sample;
    use otaq_core::board::settings::Lanes;
    use otaq_core::domain::{Kind, Place, Status};

    /// The example's shop-web flow (*24 · The two views*): Grill, Dev (with Validate), Security, Review, PR open.
    fn shop_web() -> Flow {
        let yaml = "\
flow:
  - {id: grill, name: Grill, kind: agent, agent: Griller, skill: grill}
  - {id: dev, name: Dev, kind: agent, agent: Rust developer, skill: work-ticket}
  - {id: validate, name: Validate, kind: check, by: agent, column: with-previous, agent: Validator, skill: validate, rounds: 3}
  - {id: security, name: Security, kind: check, by: agent, agent: Security reviewer, skill: security-review, rounds: 3}
  - {id: review, name: Review, kind: check, by: you}
  - {id: pr, name: PR open, kind: delivery, agent: Shipper, skill: open-pr, mode: click, delivers: pr}
";
        otaq_core::flow::yaml::read(yaml).unwrap().flow
    }

    fn at(key: &str, title: &str, status: Status, place: Place) -> Ticket {
        let mut t = ticket(key, title, status);
        t.place = place;
        t
    }

    fn of(epic: &str, mut t: Ticket, queued_at: Option<i64>) -> Ticket {
        t.epic = Some(epic.parse().unwrap());
        t.queued_at = queued_at;
        t
    }

    /// The example of `s24-06`: Wishlist (PAY-123, five children, its lead re-planning), Checkout slowness (made by
    /// the Project agent, three tickets) and five tickets with no epic.
    fn example() -> Vec<Ticket> {
        let mut wishlist = ticket("SHOP-40", "Wishlist", Status::Backlog);
        wishlist.kind = Kind::Epic;
        wishlist.external_id = Some("PAY-123".into());
        let mut slow = ticket("SHOP-34", "Checkout slowness", Status::Backlog);
        slow.kind = Kind::Epic;
        slow.created_by = Actor::ProjectAgent("pa-1".into());
        let w = "SHOP-40";
        let c = "SHOP-34";
        vec![
            wishlist,
            of(
                w,
                at("SHOP-40.1", "Wishlist API", Status::Merged, Place::Done),
                Some(1),
            ),
            of(
                w,
                at(
                    "SHOP-40.2",
                    "Wishlist button",
                    Status::Working,
                    Place::stage_of("dev"),
                ),
                Some(2),
            ),
            of(
                w,
                at(
                    "SHOP-40.3",
                    "Wishlist page",
                    Status::Backlog,
                    Place::Backlog,
                ),
                None,
            ),
            of(
                w,
                at(
                    "SHOP-40.4",
                    "Share link",
                    Status::Queued,
                    Place::Waiting(None),
                ),
                Some(3),
            ),
            of(
                w,
                at(
                    "SHOP-40.5",
                    "Empty state",
                    Status::Queued,
                    Place::Waiting(None),
                ),
                Some(4),
            ),
            slow,
            of(
                c,
                at(
                    "SHOP-35",
                    "Slow checkout query",
                    Status::Queued,
                    Place::Waiting(None),
                ),
                Some(10),
            ),
            of(
                c,
                at(
                    "SHOP-29",
                    "Coupon field",
                    Status::Working,
                    Place::stage_of("dev"),
                ),
                Some(11),
            ),
            of(
                c,
                at(
                    "SHOP-18",
                    "Checkout API",
                    Status::WaitingForYou,
                    Place::stage_of("review"),
                ),
                Some(12),
            ),
            at(
                "SHOP-31",
                "Hotfix: checkout crash",
                Status::Backlog,
                Place::Backlog,
            ),
            at(
                "SHOP-12",
                "Add dark mode toggle",
                Status::AsksYou,
                Place::stage_of("grill"),
            ),
            at(
                "SHOP-22",
                "Fix flaky login redirect",
                Status::NeedsYou,
                Place::stage_of("dev"),
            ),
            at(
                "SHOP-19",
                "Price table",
                Status::Checking,
                Place::stage_of("security"),
            ),
            at(
                "SHOP-26",
                "Add sitemap",
                Status::PrOpen,
                Place::stage_of("pr"),
            ),
        ]
    }

    fn board(tickets: &[Ticket], leads: &[Lead], prefs: ViewPrefs) -> Vec<Lane> {
        let flow = shop_web();
        let facts = ProjectFacts {
            flow: &flow,
            name: "shop-web",
            repo_path: "/tmp/shop-web",
            base_branch: "main",
            agent_names: &[],
            was_validating: &[],
            leads,
            agent_filter: None,
            agents: None,
            agent_list: None,
            starts_held: None,
            paused: false,
            view: &prefs,
        };
        let params = Params {
            project: "SHOP".into(),
            width: 1376.,
            height: 836.,
            ..Params::default()
        };
        page(
            tickets,
            &facts,
            &params,
            NOW,
            &KeyLabels::base(Platform::Mac),
        )
        .lanes
        .expect("Lanes: Epic")
    }

    fn epic_lanes() -> ViewPrefs {
        ViewPrefs {
            lanes: Lanes::Epic,
            ..ViewPrefs::default()
        }
    }

    fn keys(lane: &Lane, column: &str) -> Vec<String> {
        lane.cells
            .iter()
            .find(|c| c.column.as_str() == column)
            .unwrap_or_else(|| panic!("no cell {column}"))
            .cards
            .iter()
            .map(|c| c.key().to_owned())
            .collect()
    }

    fn replanning(tickets: &[Ticket]) -> Vec<Lead> {
        vec![sample(&tickets[0], LeadStatus::Replanning, 0)]
    }

    /// EPL-01 on the example (`s24-06`): the two epic lanes, then "No epic"; each head as drawn; each lane's cards
    /// in their columns, in the board's columns' order.
    #[test]
    fn the_example_has_two_epic_lanes_and_no_epic() {
        let tickets = example();
        let lanes = board(&tickets, &replanning(&tickets), epic_lanes());
        let ids: Vec<&str> = lanes.iter().map(|l| l.id.as_str()).collect();
        assert_eq!(ids, ["SHOP-40", "SHOP-34", NO_EPIC_LANE]);

        let wishlist = &lanes[0];
        assert_eq!(wishlist.epic.as_deref(), Some("SHOP-40"));
        assert_eq!(wishlist.head.name, "Wishlist");
        assert_eq!(wishlist.head.detail, "PAY-123 · 5 tickets");
        assert_eq!(
            wishlist.head.progress,
            Some(LaneProgress {
                merged: 1,
                total: 5,
                label: "1 of 5 merged".into(),
            })
        );
        assert_eq!(
            wishlist.head.lead.as_ref().unwrap().full,
            "Wishlist lead · re-planning"
        );
        assert!(!wishlist.folded);
        assert_eq!(wishlist.fold_label, FOLD_LANE);
        assert_eq!(
            wishlist.fold,
            Change::FoldLane {
                epic: "SHOP-40".into(),
                folded: true
            }
        );
        let columns: Vec<&str> = wishlist.cells.iter().map(|c| c.column.as_str()).collect();
        assert_eq!(
            columns,
            [
                "backlog",
                "ready_for_dev",
                "grill",
                "dev",
                "security",
                "review",
                "pr",
                "done"
            ]
        );
        assert_eq!(keys(wishlist, "backlog"), ["SHOP-40.3"]);
        assert_eq!(keys(wishlist, "ready_for_dev"), ["SHOP-40.4", "SHOP-40.5"]);
        assert_eq!(keys(wishlist, "dev"), ["SHOP-40.2"]);
        assert!(keys(wishlist, "done").is_empty(), "Done is folded");

        let slow = &lanes[1];
        assert_eq!(slow.head.name, "Checkout slowness");
        assert_eq!(
            slow.head.detail,
            "SHOP-34 · 3 tickets · from the Project agent"
        );
        assert_eq!(slow.head.progress.as_ref().unwrap().label, "0 of 3 merged");
        assert_eq!(slow.head.lead, None);
        assert_eq!(keys(slow, "ready_for_dev"), ["SHOP-35"]);
        assert_eq!(keys(slow, "dev"), ["SHOP-29"]);
        assert_eq!(keys(slow, "review"), ["SHOP-18"]);

        let rest = &lanes[2];
        assert_eq!(rest.epic, None);
        assert_eq!(rest.head.name, "No epic");
        assert_eq!(rest.head.detail, "5 tickets");
        assert_eq!(rest.head.progress, None);
        assert!(rest.head.needs_you);
        assert_eq!(keys(rest, "backlog"), ["SHOP-31"]);
        assert_eq!(keys(rest, "grill"), ["SHOP-12"]);
        assert_eq!(keys(rest, "dev"), ["SHOP-22"]);
        assert_eq!(keys(rest, "security"), ["SHOP-19"]);
        assert_eq!(keys(rest, "pr"), ["SHOP-26"]);

        // In a lane a child's card still names its epic, as on the board without lanes.
        let BoardCard::Ticket { card, .. } = &wishlist.cells[3].cards[0] else {
            panic!("a ticket card")
        };
        assert_eq!(card.epic.as_deref(), Some("Wishlist"));
    }

    /// EPL-01: a folded lane keeps its head and cells, says so and unfolds with the same change; "No epic" folds by
    /// its own id. Lanes: None has no lanes.
    #[test]
    fn a_folded_lane() {
        let tickets = example();
        let lanes = board(
            &tickets,
            &[],
            ViewPrefs {
                folded_lanes: vec!["SHOP-34".into(), NO_EPIC_LANE.into()],
                ..epic_lanes()
            },
        );
        let folded: Vec<(&str, bool)> = lanes.iter().map(|l| (l.id.as_str(), l.folded)).collect();
        assert_eq!(
            folded,
            [("SHOP-40", false), ("SHOP-34", true), (NO_EPIC_LANE, true)]
        );
        assert_eq!(lanes[1].fold_label, UNFOLD_LANE);
        assert_eq!(
            lanes[1].fold,
            Change::FoldLane {
                epic: "SHOP-34".into(),
                folded: false
            }
        );
        assert_eq!(
            lanes[2].fold,
            Change::FoldLane {
                epic: NO_EPIC_LANE.into(),
                folded: false
            }
        );
        let mut prefs = ViewPrefs {
            folded_lanes: vec!["SHOP-34".into()],
            ..ViewPrefs::default()
        };
        crate::view_models::view_prefs::apply(&mut prefs, &lanes[1].fold);
        assert!(prefs.folded_lanes.is_empty());

        // Lanes: None.
        let flow = shop_web();
        let none = ViewPrefs::default();
        let facts = ProjectFacts {
            flow: &flow,
            name: "shop-web",
            repo_path: "/tmp/shop-web",
            base_branch: "main",
            agent_names: &[],
            was_validating: &[],
            leads: &[],
            agent_filter: None,
            agents: None,
            agent_list: None,
            starts_held: None,
            paused: false,
            view: &none,
        };
        let p = page(
            &tickets,
            &facts,
            &Params::default(),
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        assert_eq!(p.lanes, None);
    }

    /// The lane order (d0047, d0080): pinned first, then an epic with a child that needs you (or whose lead asks),
    /// then by first queued; a not-started epic's lane holds its card with the rows.
    #[test]
    fn lanes_follow_pins_and_needs_you() {
        let mut tickets = example();
        assert_eq!(
            board(&tickets, &[], epic_lanes())
                .iter()
                .map(|l| l.id.clone())
                .collect::<Vec<_>>(),
            ["SHOP-40", "SHOP-34", NO_EPIC_LANE]
        );
        // SHOP-29 asks: Checkout slowness needs you.
        let i = tickets
            .iter()
            .position(|t| t.key.to_string() == "SHOP-29")
            .unwrap();
        tickets[i].status = Status::AsksYou;
        let lanes = board(&tickets, &[], epic_lanes());
        assert_eq!(lanes[0].id, "SHOP-34");
        assert!(lanes[0].head.needs_you);
        // Pinning Wishlist puts it first again.
        tickets[0].flags.pinned_at = Some(5);
        let lanes = board(&tickets, &[], epic_lanes());
        assert_eq!(lanes[0].id, "SHOP-40");
        assert!(lanes[0].head.pinned);

        // An epic not started yet: its lane holds its card in Backlog.
        let mut search = ticket("SHOP-45", "Search", Status::Backlog);
        search.kind = Kind::Epic;
        tickets.push(search);
        tickets.push(of(
            "SHOP-45",
            at("SHOP-45.1", "Index", Status::Backlog, Place::Backlog),
            None,
        ));
        let lanes = board(&tickets, &[], epic_lanes());
        let lane = lanes.iter().find(|l| l.id == "SHOP-45").unwrap();
        assert_eq!(keys(lane, "backlog"), ["SHOP-45"]);
        assert_eq!(lanes.last().unwrap().id, NO_EPIC_LANE, "No epic last");
    }

    /// Under a search, a lane with no card left isn't drawn; a Done epic has no lane (Q-A).
    #[test]
    fn a_search_leaves_out_empty_lanes_and_done_epics_have_none() {
        let mut tickets = example();
        let flow = shop_web();
        let prefs = epic_lanes();
        let facts = ProjectFacts {
            flow: &flow,
            name: "shop-web",
            repo_path: "/tmp/shop-web",
            base_branch: "main",
            agent_names: &[],
            was_validating: &[],
            leads: &[],
            agent_filter: None,
            agents: None,
            agent_list: None,
            starts_held: None,
            paused: false,
            view: &prefs,
        };
        let params = Params {
            filter: crate::view_models::search_page::FilterState {
                tokens: Vec::new(),
                text: "coupon".into(),
            },
            ..Params::default()
        };
        let p = page(
            &tickets,
            &facts,
            &params,
            NOW,
            &KeyLabels::base(Platform::Mac),
        );
        let ids: Vec<String> = p.lanes.unwrap().iter().map(|l| l.id.clone()).collect();
        assert_eq!(ids, ["SHOP-34"]);

        tickets[0].status = Status::Merged;
        tickets[0].place = Place::Done;
        let lanes = board(&tickets, &[], epic_lanes());
        assert!(lanes.iter().all(|l| l.id != "SHOP-40"));
    }

    /// EPL-02: Group by Epic in the List: "Epics 2", a parent row per epic with its own track, "1 of 5 merged ·
    /// Wishlist lead is re-planning" (or "· from the Project agent"), its children nested in key order, parents
    /// fold; then "No epic 5".
    #[test]
    fn the_parent_rows() {
        let tickets = example();
        let flow = shop_web();
        let leads = replanning(&tickets);
        let list = by_epic(&tickets, &flow, &leads, &ViewPrefs::default(), None);
        assert_eq!(
            (list.epics_title.as_str(), list.epics_count.as_str()),
            ("Epics", "2")
        );
        let [wishlist, slow] = list.groups.as_slice() else {
            panic!("two groups")
        };
        assert_eq!(wishlist.parent.key, "SHOP-40");
        assert_eq!(wishlist.parent.external_id.as_deref(), Some("PAY-123"));
        assert_eq!(
            wishlist.parent.line,
            "1 of 5 merged · Wishlist lead is re-planning"
        );
        assert_eq!(wishlist.parent.track.label, "Dev");
        assert_eq!(
            wishlist.parent.track.stages,
            [
                "Backlog",
                "Ready for Dev",
                "Grill",
                "Dev",
                "Security",
                "Review",
                "PR open · PR",
                "Done"
            ]
        );
        assert_eq!(wishlist.parent.track.at, 3);
        assert_eq!(
            wishlist.children,
            [
                "SHOP-40.1",
                "SHOP-40.2",
                "SHOP-40.3",
                "SHOP-40.4",
                "SHOP-40.5"
            ]
        );
        assert!(!wishlist.folded);
        assert_eq!(slow.parent.line, "0 of 3 merged · from the Project agent");
        assert_eq!(slow.parent.track.label, "Dev");
        assert_eq!(slow.parent.lead, None);
        assert_eq!(slow.children, ["SHOP-18", "SHOP-29", "SHOP-35"]);
        assert_eq!(
            (list.rest_title.as_str(), list.rest_count.as_str()),
            ("No epic", "5")
        );
        assert_eq!(
            list.rest,
            ["SHOP-12", "SHOP-19", "SHOP-22", "SHOP-26", "SHOP-31"]
        );

        // Folded with the lanes' preference.
        let prefs = ViewPrefs {
            folded_lanes: vec!["SHOP-40".into()],
            ..ViewPrefs::default()
        };
        let list = by_epic(&tickets, &flow, &leads, &prefs, None);
        assert!(list.groups[0].folded);
        assert_eq!(list.groups[0].fold_label, UNFOLD_LANE);
    }

    /// EPL-02: the parent's track: Ready for Dev while children only wait, Backlog before anything is queued, Done
    /// when every child is done; the lead's words.
    #[test]
    fn the_parent_track_and_the_leads_words() {
        let flow = shop_web();
        let mut tickets = example();
        let wishlist = |tickets: &[Ticket]| epic_track(&tickets[0], tickets, &flow);
        tickets[2].status = Status::Merged;
        tickets[2].place = Place::Done;
        assert_eq!(wishlist(&tickets).label, "Ready for Dev");
        for t in tickets.iter_mut().filter(|t| {
            t.epic.as_ref().is_some_and(|e| e.to_string() == "SHOP-40") && t.place.is_waiting()
        }) {
            t.status = Status::Backlog;
            t.place = Place::Backlog;
        }
        assert_eq!(wishlist(&tickets).label, "Backlog");
        for t in tickets
            .iter_mut()
            .filter(|t| t.epic.as_ref().is_some_and(|e| e.to_string() == "SHOP-40"))
        {
            t.status = Status::Merged;
            t.place = Place::Done;
        }
        assert_eq!(wishlist(&tickets).label, "Done");

        let lead = |status| lead_doing(&sample(&tickets[0], status, 0));
        assert_eq!(lead(LeadStatus::Idle), "Wishlist lead is idle");
        assert_eq!(lead(LeadStatus::AsksYou), "Wishlist lead asks you");
        assert_eq!(
            lead(LeadStatus::Answering {
                ticket: "SHOP-40.2".parse().unwrap()
            }),
            "Wishlist lead is answering 40.2"
        );
        assert_eq!(
            lead(LeadStatus::EndedByRestart),
            "Wishlist lead · ended by restart"
        );
    }

    #[test]
    fn the_gallery_has_every_head() {
        let heads = gallery_heads();
        assert_eq!(heads.len(), 8);
        assert!(heads.iter().any(|h| h.folded));
        assert_eq!(heads.last().unwrap().head.name, NO_EPIC);
    }
}
