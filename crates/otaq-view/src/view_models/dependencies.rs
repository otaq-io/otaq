//! Dependencies on the board and in the sheet (DPV-01…DPV-04; d0006, d0049; *24 · Dependencies on the board*
//! `s24-09`, *Dependencies: always on, and in the List* `s24-10`, *Adding one, and dragging a blocked ticket*
//! `s24-11`). A ticket that hasn't started and waits for a parent says so on its card and row, in amber with a link
//! icon ("Blocked · waits for SHOP-18 and SHOP-29 to merge"); a ticket others wait for says what it blocks, muted
//! ("Blocks SHOP-20" on the card, "blocks 1" on the row). Hovering a linked card explains it in a tooltip ("SHOP-20 is
//! blocked" · "by SHOP-18 (Review) and SHOP-29 (Dev)"); Always keeps every link drawn ([`links`]). Starting a blocked
//! ticket opens [`Dialog`], whose only button keeps it waiting (the core refuses the start,
//! `otaq_core::board::moves::blocks_start`). In the sheet's Overview, under the description, [`Section`] lists
//! Blocked by and Blocks with where each ticket is now; Add picks the direction, then searches ([`search`]); a loop is
//! refused with [`loop_refused`]. Who may edit them is the core's, unchanged (DEP-01…DEP-04).

use std::collections::HashSet;

use otaq_core::board::readiness::{ReadyState, WaitingParent, readiness};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, Status, TicketKey};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::theme::Palette;
use crate::view_models::card::{CardExtras, CardViewModel, lowered};
use crate::view_models::chain;
use crate::view_models::flow_columns::Columns;
use crate::view_models::list_view::{DONE, READY};
use crate::view_models::status::{self, PillView};

/// The section's heading (*s24-11*: DEPENDENCIES).
pub const TITLE: &str = "Dependencies";
/// The section's Add, and the picker's directions.
pub const ADD: &str = "Add";
pub const BLOCKED_BY: &str = "Blocked by";
pub const BLOCKS: &str = "Blocks";
/// The picker's field.
pub const SEARCH: &str = "Search by key or title";
/// A row's ✕.
pub const REMOVE: &str = "Remove";
/// DPV-04: a link that would close a loop (the core's `cycle` refusal).
pub fn loop_refused() -> String {
    format!(
        "A ticket can\u{2019}t wait for one that waits for it: {} refuses loops.",
        otaq_core::naming::PRODUCT_NAME
    )
}
/// DPV-03's only button, focused, and what Esc does.
pub const KEEP_IT_WAITING: &str = "Keep it waiting";
/// Under DPV-03's message (*s24-11*).
pub const REMOVE_FIRST: &str = "To start it, remove the dependency in the ticket first.";

/// "SHOP-18", "SHOP-18 and SHOP-29", "SHOP-17, SHOP-18 and SHOP-29".
pub fn and_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The parents `ticket` still waits for, in key order (none while it is ready, or when a parent closed: that
/// card says so in red, d0044).
pub fn waiting_for(ticket: &Ticket, all: &[Ticket]) -> Vec<WaitingParent> {
    match readiness(ticket, all) {
        ReadyState::Waiting { parents } => parents,
        ReadyState::Ready | ReadyState::ParentClosed { .. } => Vec::new(),
    }
}

/// Whether the ticket hasn't started yet: it is in Backlog or waits in Ready for Dev (a parked ticket has started).
fn not_started(ticket: &Ticket) -> bool {
    ticket.status != Status::Parked && matches!(ticket.place, Place::Backlog | Place::Waiting(None))
}

fn parent_name(p: &WaitingParent) -> String {
    p.key.to_string()
}

/// DPV-01's line on a card and a row: "Blocked · waits for SHOP-18 and SHOP-29 to merge", for a ticket that hasn't
/// started while a parent isn't merged. Replaces the old "after SHOP-18" and the Queued "waits for…" text.
pub fn blocked_line(ticket: &Ticket, all: &[Ticket]) -> Option<String> {
    if !not_started(ticket) {
        return None;
    }
    let parents = waiting_for(ticket, all);
    (!parents.is_empty()).then(|| {
        let names: Vec<String> = parents.iter().map(parent_name).collect();
        format!("Blocked \u{b7} waits for {} to merge", and_list(&names))
    })
}

/// The tickets waiting for `ticket` directly: its children not done yet, in key order. None once it is done.
pub fn waiting_children<'a>(ticket: &Ticket, all: &'a [Ticket]) -> Vec<&'a Ticket> {
    if ticket.place.is_done() {
        return Vec::new();
    }
    let mut children: Vec<&Ticket> = all
        .iter()
        .filter(|t| t.depends_on.contains(&ticket.key) && !t.place.is_done())
        .collect();
    children.sort_by(|a, b| a.key.cmp(&b.key));
    children
}

/// How many tickets wait behind `ticket`, directly or through others, not counting done ones (the List's "blocks
/// N", the core's `blocked_counts` without the finished ones).
pub fn waiting_behind(ticket: &Ticket, all: &[Ticket]) -> usize {
    let mut seen: HashSet<&TicketKey> = HashSet::new();
    let mut stack = vec![ticket];
    while let Some(at) = stack.pop() {
        for child in waiting_children(at, all) {
            if seen.insert(&child.key) {
                stack.push(child);
            }
        }
    }
    seen.remove(&ticket.key);
    seen.len()
}

/// What a ticket others wait for says (DPV-01): "Blocks SHOP-20" on its card (two keys at most, then "+N"), "blocks
/// 1" on its row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardBlocks")]
pub struct Blocks {
    pub line: String,
    pub tag: String,
}

pub fn blocks(ticket: &Ticket, all: &[Ticket]) -> Option<Blocks> {
    let children = waiting_children(ticket, all);
    if children.is_empty() {
        return None;
    }
    let keys: Vec<String> = children.iter().map(|t| t.key.to_string()).collect();
    let line = if keys.len() <= 2 {
        and_list(&keys)
    } else {
        format!("{}, {} +{}", keys[0], keys[1], keys.len() - 2)
    };
    Some(Blocks {
        line: format!("Blocks {line}"),
        tag: format!("blocks {}", waiting_behind(ticket, all)),
    })
}

/// Where a ticket is now, by the board's words: "Backlog", "Ready", the stage's name ("Review", "Validate"),
/// "Done".
pub fn place_name(columns: &Columns, ticket: &Ticket) -> String {
    match &ticket.place {
        Place::Backlog => "Backlog".to_owned(),
        Place::Waiting(None) => READY.to_owned(),
        Place::Done => DONE.to_owned(),
        Place::Waiting(Some(_)) | Place::InStage(_) => columns.stage_of(ticket).map_or_else(
            || columns.column_of(ticket).title.clone(),
            |s| s.name.clone(),
        ),
    }
}

fn find<'a>(all: &'a [Ticket], key: &TicketKey) -> Option<&'a Ticket> {
    all.iter().find(|t| &t.key == key)
}

/// "SHOP-18 (Review) and SHOP-29 (Dev)", or with `in_` "SHOP-18 (in Review) and SHOP-29 (in Dev)".
fn placed(columns: &Columns, keys: &[&Ticket], in_: bool) -> String {
    let named: Vec<String> = keys
        .iter()
        .map(|t| {
            let at = place_name(columns, t);
            if in_ {
                format!("{} (in {at})", t.key)
            } else {
                format!("{} ({at})", t.key)
            }
        })
        .collect();
    and_list(&named)
}

/// DPV-02's hover tooltip on a linked card. `ChainTip` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ChainTip")]
pub struct Tip {
    /// "SHOP-20 is blocked", "SHOP-18 blocks".
    pub title: String,
    /// "by SHOP-18 (Review) and SHOP-29 (Dev)", "SHOP-20 (Ready)".
    pub detail: String,
}

/// The tooltip of a hovered card: why it is blocked, else what waits for it (Q-A); `None` when it is neither.
pub fn tip(columns: &Columns, ticket: &Ticket, all: &[Ticket]) -> Option<Tip> {
    let parents: Vec<&Ticket> = waiting_for(ticket, all)
        .iter()
        .filter_map(|p| find(all, &p.key))
        .collect();
    if !parents.is_empty() && !ticket.place.is_done() {
        return Some(Tip {
            title: format!("{} is blocked", ticket.key),
            detail: format!("by {}", placed(columns, &parents, false)),
        });
    }
    let children = waiting_children(ticket, all);
    (!children.is_empty()).then(|| Tip {
        title: format!("{} blocks", ticket.key),
        detail: placed(columns, &children, false),
    })
}

/// Always (DPV-02): every link between tickets not done yet, parent → child, in key order; the page draws those
/// whose two cards are in view, quiet and dashed.
pub fn links(all: &[Ticket]) -> Vec<chain::Edge> {
    let mut edges: Vec<(&TicketKey, &TicketKey)> = all
        .iter()
        .filter(|t| !t.place.is_done())
        .flat_map(|t| {
            t.depends_on
                .iter()
                .filter(|p| find(all, p).is_some_and(|p| !p.place.is_done()))
                .map(move |p| (p, &t.key))
        })
        .collect();
    edges.sort();
    edges
        .into_iter()
        .map(|(parent, child)| chain::Edge {
            parent: parent.to_string(),
            child: child.to_string(),
        })
        .collect()
}

/// DPV-03's dialog: a blocked ticket dragged into a stage past Ready for Dev, or started. It goes to (or stays in)
/// Ready for Dev; Keep it waiting (focused, Esc) only closes it. `BlockedDialog` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "BlockedDialog")]
pub struct Dialog {
    pub key: String,
    /// "SHOP-20 is blocked".
    pub title: String,
    /// "It waits for SHOP-18 (in Review) and SHOP-29 (in Dev) to merge. Started now, its agent works without their
    /// changes and may have to redo work when they land."
    pub body: String,
    /// "To start it, remove the dependency in the ticket first."
    pub hint: String,
    pub keep: String,
}

pub fn dialog(columns: &Columns, ticket: &Ticket, all: &[Ticket]) -> Dialog {
    let parents: Vec<&Ticket> = waiting_for(ticket, all)
        .iter()
        .filter_map(|p| find(all, &p.key))
        .collect();
    let (them, their) = if parents.len() == 1 {
        ("it lands", "its")
    } else {
        ("they land", "their")
    };
    Dialog {
        key: ticket.key.to_string(),
        title: otaq_core::board::moves::blocked_text(&ticket.key),
        body: format!(
            "It waits for {} to merge. Started now, its agent works without {their} changes and may have to redo \
             work when {them}.",
            placed(columns, &parents, true)
        ),
        hint: REMOVE_FIRST.to_owned(),
        keep: KEEP_IT_WAITING.to_owned(),
    }
}

/// Which way a link goes from the open ticket: it waits for the other one (Blocked by) or the other waits for it
/// (Blocks). `DependencyDirection` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "DependencyDirection")]
pub enum Direction {
    BlockedBy,
    Blocks,
}

impl Direction {
    /// The link as the core stores it: (child, parent).
    pub fn link<'a>(self, open: &'a str, other: &'a str) -> (&'a str, &'a str) {
        match self {
            Self::BlockedBy => (open, other),
            Self::Blocks => (other, open),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::BlockedBy => BLOCKED_BY,
            Self::Blocks => BLOCKS,
        }
    }
}

/// One linked ticket in the section: its key, title, where it is now ("Review · your check") and its ✕.
/// `DependencyRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DependencyRow")]
pub struct Row {
    pub key: String,
    pub title: String,
    pub place: String,
    /// The dot before it, in its state's colour.
    pub dot: Palette,
    pub direction: Direction,
}

/// Blocked by or Blocks. `DependencyGroup` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DependencyGroup")]
pub struct Group {
    pub direction: Direction,
    /// "Blocked by" / "Blocks" (the page writes it in capitals).
    pub label: String,
    /// "starts when both merge" while it waits for its parents.
    pub note: Option<String>,
    pub rows: Vec<Row>,
    /// "Nothing waits for SHOP-20." when there are no rows.
    pub empty: String,
}

/// DPV-04's section in Overview, under the description. `DependenciesView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DependenciesView")]
pub struct Section {
    pub key: String,
    pub title: String,
    pub add: String,
    pub blocked_by: Group,
    pub blocks: Group,
    /// The picker's directions, Blocked by first.
    pub directions: Vec<DirectionChoice>,
    pub search: String,
    pub remove: String,
    /// Under the picker's rows (DPV-04): "A ticket can’t wait for one that waits for it: Otaq refuses loops."
    pub rule: String,
}

/// One of the picker's two directions. `DependencyDirectionChoice` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DependencyDirectionChoice")]
pub struct DirectionChoice {
    pub direction: Direction,
    pub label: String,
}

/// "Review · your check", "Dev · working", "Backlog": where the ticket is now, then the first words of its card's
/// line.
/// With the dot of its state's colour (the card's).
pub fn place_line(columns: &Columns, ticket: &Ticket, now_ms: i64) -> (String, Palette) {
    let place = place_name(columns, ticket);
    let extras = CardExtras {
        custom_flow: !crate::view_models::flow_columns::keeps_default_look(columns.flow()),
        ..CardExtras::default()
    };
    let card = CardViewModel::from(ticket, &extras, now_ms);
    let dot = card.one_line.line.as_ref().map_or_else(
        || PillView::of(status::shown(ticket)).palette,
        |line| line.dot,
    );
    let first = card.one_line.line.map(|line| {
        line.text
            .split(" \u{b7} ")
            .next()
            .unwrap_or_default()
            .to_owned()
    });
    let text = match first {
        Some(words) if !words.is_empty() && !words.eq_ignore_ascii_case(&place) => {
            format!("{place} \u{b7} {}", lowered(&words))
        }
        _ => place,
    };
    (text, dot)
}

/// "starts when it merges" / "starts when both merge" / "starts when all 3 merge", while it still waits.
fn starts_when(waiting: usize) -> Option<String> {
    match waiting {
        0 => None,
        1 => Some("starts when it merges".to_owned()),
        2 => Some("starts when both merge".to_owned()),
        n => Some(format!("starts when all {n} merge")),
    }
}

/// The section of `ticket`. `all` are its project's.
pub fn section(columns: &Columns, ticket: &Ticket, all: &[Ticket], now_ms: i64) -> Option<Section> {
    let row = |t: &Ticket, direction| {
        let (place, dot) = place_line(columns, t, now_ms);
        Row {
            key: t.key.to_string(),
            title: t.title.clone(),
            place,
            dot,
            direction,
        }
    };
    let mut parents: Vec<&Ticket> = ticket
        .depends_on
        .iter()
        .filter_map(|k| find(all, k))
        .collect();
    parents.sort_by(|a, b| a.key.cmp(&b.key));
    let mut children: Vec<&Ticket> = all
        .iter()
        .filter(|t| t.depends_on.contains(&ticket.key))
        .collect();
    children.sort_by(|a, b| a.key.cmp(&b.key));
    Some(Section {
        key: ticket.key.to_string(),
        title: TITLE.to_owned(),
        add: ADD.to_owned(),
        blocked_by: Group {
            direction: Direction::BlockedBy,
            label: BLOCKED_BY.to_owned(),
            note: starts_when(waiting_for(ticket, all).len()),
            rows: parents
                .into_iter()
                .map(|t| row(t, Direction::BlockedBy))
                .collect(),
            empty: format!("{} waits for nothing.", ticket.key),
        },
        blocks: Group {
            direction: Direction::Blocks,
            label: BLOCKS.to_owned(),
            note: None,
            rows: children
                .into_iter()
                .map(|t| row(t, Direction::Blocks))
                .collect(),
            empty: format!("Nothing waits for {}.", ticket.key),
        },
        directions: [Direction::BlockedBy, Direction::Blocks]
            .into_iter()
            .map(|direction| DirectionChoice {
                direction,
                label: direction.label().to_owned(),
            })
            .collect(),
        search: SEARCH.to_owned(),
        remove: REMOVE.to_owned(),
        rule: loop_refused(),
    })
}

/// One ticket the picker offers: key, title and where it is ("Ready", "Dev"). `DependencySearchRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DependencySearchRow")]
pub struct SearchRow {
    pub key: String,
    pub title: String,
    pub place: String,
}

/// The picker's rows for `query` (key, number or title, case-insensitive) in `direction`: this project's tickets
/// (d0046), not the open ticket, not one already
/// linked, not a done one; at most `limit`. A loop isn't left out: the core refuses it with DPV-04's words.
pub fn search(
    columns: &Columns,
    ticket: &Ticket,
    all: &[Ticket],
    direction: Direction,
    query: &str,
    limit: usize,
) -> Vec<SearchRow> {
    let q = query.trim().to_lowercase();
    let linked = |t: &Ticket| match direction {
        Direction::BlockedBy => ticket.depends_on.contains(&t.key),
        Direction::Blocks => t.depends_on.contains(&ticket.key),
    };
    all.iter()
        .filter(|t| t.key != ticket.key && !linked(t) && !t.place.is_done())
        .filter(|t| {
            q.is_empty()
                || t.key.to_string().to_lowercase().contains(&q)
                || t.title.to_lowercase().contains(&q)
                || t.key.number.to_string() == q.trim_start_matches('#')
        })
        .take(limit)
        .map(|t| SearchRow {
            key: t.key.to_string(),
            title: t.title.clone(),
            place: place_name(columns, t),
        })
        .collect()
}

/// How the core's refusal of a link reads under the picker: DPV-04's words for a loop, else the core's own.
pub fn refusal_text(code: Option<&str>, message: &str) -> String {
    match code {
        Some("cycle") => loop_refused(),
        _ => message.to_owned(),
    }
}

/// The icon of DPV-01's line and of the List's blocked row.
pub const ICON: IconName = IconName::Link;

/// The gallery's dependencies (`s24-09`…`s24-11`): SHOP-20 blocked by SHOP-18 and SHOP-29, SHOP-29's "Blocks", the
/// sheet's section with the picker's rows for "SHOP-3", and DPV-03's dialog. `DependenciesGallery` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "DependenciesGallery")]
pub struct Gallery {
    pub blocked: crate::view_models::card_page::TicketCardView,
    pub blocker: crate::view_models::card_page::TicketCardView,
    pub section: Section,
    pub search: Vec<SearchRow>,
    pub dialog: Dialog,
}

/// The example's shop-web with SHOP-20 waiting for SHOP-18 and SHOP-29.
pub fn example() -> Vec<Ticket> {
    let mut all = crate::view_models::list_view::samples::tickets();
    if let Some(t20) = all.iter_mut().find(|t| t.key.to_string() == "SHOP-20") {
        t20.depends_on = ["SHOP-18", "SHOP-29"]
            .iter()
            .filter_map(|k| k.parse().ok())
            .collect();
    }
    all
}

pub fn gallery() -> Gallery {
    use crate::view_models::card_fixtures::NOW;
    use crate::view_models::card_page::TicketCardView;
    use crate::view_models::list_view::samples::{input, shop_web};
    let flow = shop_web();
    let columns = Columns::of(&flow);
    let all = example();
    let get = |key: &str| {
        all.iter()
            .find(|t| t.key.to_string() == key)
            .expect("the example has it")
    };
    let card = |key: &str| {
        let t = get(key);
        TicketCardView::from_vm(
            crate::view_models::board::ticket_card(&input(&flow, &all), t),
            t,
        )
    };
    let t20 = get("SHOP-20");
    Gallery {
        blocked: card("SHOP-20"),
        blocker: card("SHOP-29"),
        section: section(&columns, t20, &all, NOW).expect("a ticket"),
        search: search(&columns, t20, &all, Direction::BlockedBy, "SHOP-3", 3),
        dialog: dialog(&columns, t20, &all),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::list_view::samples::shop_web;

    #[test]
    fn the_gallery_draws_the_example() {
        let g = gallery();
        assert_eq!(
            g.blocked.status.as_ref().map(|s| s.text.as_str()),
            Some("Blocked \u{b7} waits for SHOP-18 and SHOP-29 to merge")
        );
        assert_eq!(
            g.blocker.blocks.as_ref().map(|b| b.line.as_str()),
            Some("Blocks SHOP-20")
        );
        assert_eq!(g.search.len(), 3);
        assert_eq!(g.dialog.title, "SHOP-20 is blocked");
    }

    fn get<'a>(all: &'a [Ticket], key: &str) -> &'a Ticket {
        all.iter().find(|t| t.key.to_string() == key).unwrap()
    }

    #[test]
    fn the_blocked_line_names_every_parent_still_to_merge() {
        let all = example();
        assert_eq!(
            blocked_line(get(&all, "SHOP-20"), &all).as_deref(),
            Some("Blocked \u{b7} waits for SHOP-18 and SHOP-29 to merge")
        );
        // A working ticket isn't blocked, whatever it links to; a merged parent no longer counts.
        assert_eq!(blocked_line(get(&all, "SHOP-29"), &all), None);
        let mut merged = all.clone();
        for t in merged.iter_mut().filter(|t| t.key.to_string() == "SHOP-18") {
            t.status = Status::Merged;
            t.place = Place::Done;
        }
        assert_eq!(
            blocked_line(get(&merged, "SHOP-20"), &merged).as_deref(),
            Some("Blocked \u{b7} waits for SHOP-29 to merge")
        );
        assert_eq!(
            and_list(&["A".into(), "B".into(), "C".into()]),
            "A, B and C"
        );
    }

    #[test]
    fn blockers_say_what_they_block() {
        let all = example();
        let b = blocks(get(&all, "SHOP-29"), &all).unwrap();
        assert_eq!(
            (b.line.as_str(), b.tag.as_str()),
            ("Blocks SHOP-20", "blocks 1")
        );
        assert_eq!(blocks(get(&all, "SHOP-20"), &all), None);
    }

    #[test]
    fn the_tooltip_explains_a_blocked_card_and_a_blocker() {
        let flow = shop_web();
        let columns = Columns::of(&flow);
        let all = example();
        assert_eq!(
            tip(&columns, get(&all, "SHOP-20"), &all),
            Some(Tip {
                title: "SHOP-20 is blocked".into(),
                detail: "by SHOP-18 (Review) and SHOP-29 (Dev)".into(),
            })
        );
        assert_eq!(
            tip(&columns, get(&all, "SHOP-18"), &all),
            Some(Tip {
                title: "SHOP-18 blocks".into(),
                detail: "SHOP-20 (Ready)".into(),
            })
        );
        assert_eq!(tip(&columns, get(&all, "SHOP-33"), &all), None);
    }

    #[test]
    fn always_draws_every_open_link() {
        let mut all = example();
        // A merged parent's link is history: not drawn.
        let mut old = all
            .iter()
            .find(|t| t.key.to_string() == "SHOP-1")
            .unwrap()
            .clone();
        old.key = "SHOP-6".parse().unwrap();
        all.push(old);
        all.iter_mut()
            .find(|t| t.key.to_string() == "SHOP-33")
            .unwrap()
            .depends_on = vec!["SHOP-6".parse().unwrap()];
        let pairs: Vec<(String, String)> = links(&all)
            .into_iter()
            .map(|e| (e.parent, e.child))
            .collect();
        assert_eq!(
            pairs,
            [
                ("SHOP-18".to_owned(), "SHOP-20".to_owned()),
                ("SHOP-29".to_owned(), "SHOP-20".to_owned()),
            ]
        );
    }

    #[test]
    fn the_dialog_says_what_it_waits_for_with_one_button() {
        let flow = shop_web();
        let columns = Columns::of(&flow);
        let all = example();
        let d = dialog(&columns, get(&all, "SHOP-20"), &all);
        assert_eq!(d.title, "SHOP-20 is blocked");
        assert_eq!(
            d.body,
            "It waits for SHOP-18 (in Review) and SHOP-29 (in Dev) to merge. Started now, its agent works without \
             their changes and may have to redo work when they land."
        );
        assert_eq!(
            d.hint,
            "To start it, remove the dependency in the ticket first."
        );
        assert_eq!(d.keep, "Keep it waiting");
    }

    #[test]
    fn the_sheet_lists_both_directions_with_places() {
        let flow = shop_web();
        let columns = Columns::of(&flow);
        let all = example();
        let s = section(
            &columns,
            get(&all, "SHOP-20"),
            &all,
            crate::view_models::card_fixtures::NOW,
        )
        .unwrap();
        assert_eq!(s.title, "Dependencies");
        assert_eq!(s.blocked_by.label, "Blocked by");
        assert_eq!(s.blocked_by.note.as_deref(), Some("starts when both merge"));
        let rows: Vec<(&str, &str, &str)> = s
            .blocked_by
            .rows
            .iter()
            .map(|r| (r.key.as_str(), r.title.as_str(), r.place.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("SHOP-18", "Checkout API", "Review \u{b7} your check"),
                ("SHOP-29", "Coupon field", "Dev \u{b7} working"),
            ]
        );
        assert_eq!(
            (s.blocked_by.rows[0].dot, s.blocked_by.rows[1].dot),
            (Palette::Review, Palette::Working)
        );
        assert_eq!(s.rule, loop_refused());
        assert!(s.blocks.rows.is_empty());
        assert_eq!(s.blocks.empty, "Nothing waits for SHOP-20.");
        let s18 = section(&columns, get(&all, "SHOP-18"), &all, 0).unwrap();
        assert_eq!(s18.blocks.rows[0].key, "SHOP-20");
        assert_eq!(s18.blocked_by.note, None);
        assert_eq!(
            s.directions
                .iter()
                .map(|d| d.label.as_str())
                .collect::<Vec<_>>(),
            ["Blocked by", "Blocks"]
        );
        // Removing SHOP-29 from SHOP-20 is the link (child SHOP-20, parent SHOP-29); from SHOP-29's Blocks, the same.
        assert_eq!(
            Direction::BlockedBy.link("SHOP-20", "SHOP-29"),
            ("SHOP-20", "SHOP-29")
        );
        assert_eq!(
            Direction::Blocks.link("SHOP-29", "SHOP-20"),
            ("SHOP-20", "SHOP-29")
        );
    }

    #[test]
    fn search_leaves_out_itself_the_linked_and_the_done_and_refuses_loops_in_words() {
        let flow = shop_web();
        let columns = Columns::of(&flow);
        let all = example();
        let t20 = get(&all, "SHOP-20");
        let keys = |rows: Vec<SearchRow>| rows.into_iter().map(|r| r.key).collect::<Vec<_>>();
        assert_eq!(
            keys(search(
                &columns,
                t20,
                &all,
                Direction::BlockedBy,
                "SHOP-3",
                10
            )),
            ["SHOP-31", "SHOP-35", "SHOP-33", "SHOP-30", "SHOP-32"]
        );
        let found = search(&columns, t20, &all, Direction::BlockedBy, "slow", 10);
        assert_eq!(
            (found[0].key.as_str(), found[0].place.as_str()),
            ("SHOP-35", "Ready")
        );
        assert!(
            keys(search(&columns, t20, &all, Direction::BlockedBy, "", 99))
                .iter()
                .all(|k| { !["SHOP-20", "SHOP-18", "SHOP-29", "SHOP-1"].contains(&k.as_str()) })
        );
        assert_eq!(
            refusal_text(Some("cycle"), "SHOP-20 → SHOP-29 → SHOP-20"),
            loop_refused()
        );
        assert_eq!(
            refusal_text(Some("cross_base"), "Different bases"),
            "Different bases"
        );
    }
}
