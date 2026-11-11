//! Dependencies on the board and in the sheet (DPV-01…DPV-04; d0006, d0049; *24 · Dependencies on the board*
//! `s24-09`, *Dependencies: always on, and in the List* `s24-10`, *Adding one, and dragging a blocked ticket*
//! `s24-11`). A ticket that hasn't started and waits for a parent says so on its card and row, in amber with a link
//! hourglass ("Waits for SHOP-18 and SHOP-29 to merge"); hovering a ticket others wait for highlights them and adds
//! one line to its card ("SHOP-20 waits for it"; "blocks 1" on the row). Hovering a linked card explains it in a tooltip ("SHOP-20 is
//! blocked" · "by SHOP-18 (Review) and SHOP-29 (Dev)"); Always keeps every link drawn ([`links`]). A blocked ticket
//! can be started: it goes into the first stage, waits at its top saying "Waits for SHOP-18 to merge", and begins
//! once its parents are merged. In the sheet's Overview, under the description, [`Section`] lists
//! Waits for and Waiting for it with where each ticket is now. A link is added only from the waiting ticket: Add
//! searches for what it waits for ([`search`]), and the New ticket form takes the same search; Waiting for it is
//! read-only. A loop is refused with [`loop_refused`]. Who may edit them is the core's, unchanged (DEP-01…DEP-04).

use std::collections::HashSet;

use otaq_core::board::readiness::{ReadyState, WaitingParent, readiness};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, Status, TicketKey};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::theme::Palette;
use crate::view_models::card::{CardExtras, CardViewModel, lowered};
use crate::view_models::chain;
use crate::view_models::flow_columns::Columns;
use crate::view_models::list_view::DONE;
use crate::view_models::status::{self, PillView};

/// The section's heading (*s24-11*: DEPENDENCIES).
pub const TITLE: &str = "Dependencies";
/// The section's Add: it adds a ticket this one waits for (a link is only ever added from the waiting side).
pub const ADD: &str = "Add";
/// What the ticket waits for: the only list a link is added to or removed from.
pub const WAITS_FOR: &str = "Waits for";
/// The tickets that wait for it: read-only, each link belongs to the ticket that waits.
pub const WAITING_FOR_IT: &str = "Waiting for it";
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

/// Whether the ticket hasn't begun yet: it is in Backlog, or started and waiting to begin the first stage (a parked
/// ticket has begun).
fn not_started(ticket: &Ticket) -> bool {
    ticket.status != Status::Parked && matches!(ticket.place, Place::Backlog | Place::Waiting(None))
}

fn parent_name(p: &WaitingParent) -> String {
    p.key.to_string()
}

/// The waiting line on a card and a row, for a ticket that hasn't begun while a parent isn't merged: "Waits for
/// SHOP-18 and SHOP-29 to merge", in Backlog as once started (it then waits at the top of the first stage). Waiting
/// is the status line itself.
pub fn blocked_line(ticket: &Ticket, all: &[Ticket]) -> Option<String> {
    if !not_started(ticket) {
        return None;
    }
    let parents = waiting_for(ticket, all);
    (!parents.is_empty()).then(|| {
        let names: Vec<String> = parents.iter().map(parent_name).collect();
        let parents = and_list(&names);
        format!("Waits for {parents} to merge")
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

/// What a ticket others wait for says: "SHOP-20 waits for it" on its hovered card (two keys at most, then "+N"),
/// "blocks 1" on its row.
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
    let (line, verb) = match keys.len() {
        1 => (keys[0].clone(), "waits"),
        2 => (and_list(&keys), "wait"),
        n => (format!("{}, {} +{}", keys[0], keys[1], n - 2), "wait"),
    };
    Some(Blocks {
        line: format!("{line} {verb} for it"),
        tag: format!("blocks {}", waiting_behind(ticket, all)),
    })
}

/// Where a ticket is now, by the board's words: "Backlog", the stage's name ("Dev" for a started ticket waiting to
/// begin it, "Review", "Validate"), "Done".
pub fn place_name(columns: &Columns, ticket: &Ticket) -> String {
    match &ticket.place {
        Place::Backlog => "Backlog".to_owned(),
        Place::Done => DONE.to_owned(),
        Place::Waiting(_) | Place::InStage(_) => columns.stage_of(ticket).map_or_else(
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
    /// "by SHOP-18 (Review) and SHOP-29 (Dev)", "SHOP-20 (Dev)".
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
}

/// Waits for or Waiting for it. `DependencyGroup` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DependencyGroup")]
pub struct Group {
    /// "Waits for" / "Waiting for it" (the page writes it in capitals).
    pub label: String,
    /// Whether its rows have a ✕: only what the open ticket waits for (Waiting for it is read-only).
    pub removable: bool,
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
    pub waits_for: Group,
    pub waiting_for_it: Group,
    pub search: String,
    pub remove: String,
    /// Under the picker's rows (DPV-04): "A ticket can’t wait for one that waits for it: Otaq refuses loops."
    pub rule: String,
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
    let row = |t: &Ticket| {
        let (place, dot) = place_line(columns, t, now_ms);
        Row {
            key: t.key.to_string(),
            title: t.title.clone(),
            place,
            dot,
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
        waits_for: Group {
            label: WAITS_FOR.to_owned(),
            removable: true,
            note: starts_when(waiting_for(ticket, all).len()),
            rows: parents.into_iter().map(row).collect(),
            empty: format!("{} waits for nothing.", ticket.key),
        },
        waiting_for_it: Group {
            label: WAITING_FOR_IT.to_owned(),
            removable: false,
            note: None,
            rows: children.into_iter().map(row).collect(),
            empty: format!("Nothing waits for {}.", ticket.key),
        },
        search: SEARCH.to_owned(),
        remove: REMOVE.to_owned(),
        rule: loop_refused(),
    })
}

/// One ticket the picker offers: key, title and where it is ("Backlog", "Dev"). `DependencySearchRow` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DependencySearchRow")]
pub struct SearchRow {
    pub key: String,
    pub title: String,
    pub place: String,
}

/// What the waiting ticket's search leaves out: itself and what it already waits for.
pub fn taken(ticket: &Ticket) -> Vec<TicketKey> {
    std::iter::once(ticket.key.clone())
        .chain(ticket.depends_on.iter().cloned())
        .collect()
}

/// The picker's rows for `query` (key, number or title, case-insensitive): the tickets of the waiting ticket's
/// project (d0046) it could wait for, leaving out the `taken` ones ([`taken`], or what a new ticket's form already
/// picked) and the done ones; at most `limit`. A loop isn't left out: the core refuses it with DPV-04's words.
pub fn search(
    columns: &Columns,
    all: &[Ticket],
    taken: &[TicketKey],
    query: &str,
    limit: usize,
) -> Vec<SearchRow> {
    let q = query.trim().to_lowercase();
    all.iter()
        .filter(|t| !taken.contains(&t.key) && !t.place.is_done())
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

/// The New ticket form's "Waits for": the tickets the new one will wait for, picked with the section's search
/// before it exists, and linked once it is created. `DependencyFormField` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DependencyFormField")]
pub struct FormField {
    pub label: String,
    /// "optional · it starts once they merge".
    pub hint: String,
    pub add: String,
    pub search: String,
    pub remove: String,
}

pub fn form_field() -> FormField {
    FormField {
        label: WAITS_FOR.to_owned(),
        hint: "optional \u{b7} it starts once they merge".to_owned(),
        add: ADD.to_owned(),
        search: SEARCH.to_owned(),
        remove: REMOVE.to_owned(),
    }
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

/// The gallery's dependencies (`s24-09`…`s24-11`): SHOP-20 blocked by SHOP-18 and SHOP-29, SHOP-29's "Blocks", and the
/// sheet's section with the picker's rows for "SHOP-3". `DependenciesGallery` on the page.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "DependenciesGallery")]
pub struct Gallery {
    pub blocked: crate::view_models::card_page::TicketCardView,
    pub blocker: crate::view_models::card_page::TicketCardView,
    pub section: Section,
    pub search: Vec<SearchRow>,
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
        search: search(&columns, &all, &taken(t20), "SHOP-3", 3),
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
            Some("Waits for SHOP-18 and SHOP-29 to merge")
        );
        assert_eq!(
            g.blocker.blocks.as_ref().map(|b| b.line.as_str()),
            Some("SHOP-20 waits for it")
        );
        assert_eq!(g.search.len(), 3);
    }

    fn get<'a>(all: &'a [Ticket], key: &str) -> &'a Ticket {
        all.iter().find(|t| t.key.to_string() == key).unwrap()
    }

    /// Waiting is the line itself: "Waits for…", started (waiting to begin) or still in Backlog.
    #[test]
    fn the_blocked_line_names_every_parent_still_to_merge() {
        let all = example();
        assert_eq!(
            blocked_line(get(&all, "SHOP-20"), &all).as_deref(),
            Some("Waits for SHOP-18 and SHOP-29 to merge")
        );
        let mut in_backlog = all.clone();
        for t in in_backlog
            .iter_mut()
            .filter(|t| t.key.to_string() == "SHOP-20")
        {
            t.status = Status::Backlog;
            t.place = Place::Backlog;
        }
        assert_eq!(
            blocked_line(get(&in_backlog, "SHOP-20"), &in_backlog).as_deref(),
            Some("Waits for SHOP-18 and SHOP-29 to merge")
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
            Some("Waits for SHOP-29 to merge")
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
            ("SHOP-20 waits for it", "blocks 1")
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
                detail: "SHOP-20 (Product spec)".into(),
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

    /// A held ticket that skips Spec first's planning is named at Dev, the stage it starts at, in the section of a
    /// ticket waiting for it and in the hover tooltip; queued, at Product spec, the top of the first stage.
    #[test]
    fn a_held_blocker_is_named_at_the_stage_it_starts_at() {
        use crate::view_models::flow_columns::held_fixture::{spec_first, starts_at_dev, waiting};
        let file = spec_first();
        let agents = starts_at_dev();
        let columns = Columns::of(&file.flow).knowing(Some(&agents));
        let held = waiting(Status::Held);
        let mut blocked =
            crate::view_models::card_fixtures::ticket("NOTE-20", "Import CSV", Status::Backlog);
        blocked.depends_on = vec![held.key.clone()];
        let all = vec![held.clone(), blocked.clone()];
        assert_eq!(place_name(&columns, &held), "Dev");
        let s = section(
            &columns,
            &blocked,
            &all,
            crate::view_models::card_fixtures::NOW,
        )
        .unwrap();
        let place = &s.waits_for.rows[0].place;
        assert!(
            place == "Dev" || place.starts_with("Dev \u{b7} "),
            "{place}"
        );
        assert_eq!(
            tip(&columns, &blocked, &all).unwrap().detail,
            "by NOTE-12 (Dev)"
        );
        assert_eq!(
            place_name(&columns, &waiting(Status::Queued)),
            "Product spec"
        );
    }

    #[test]
    fn the_sheet_lists_what_it_waits_for_and_what_waits_for_it() {
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
        assert_eq!(s.waits_for.label, "Waits for");
        assert!(s.waits_for.removable);
        assert_eq!(s.waits_for.note.as_deref(), Some("starts when both merge"));
        let rows: Vec<(&str, &str, &str)> = s
            .waits_for
            .rows
            .iter()
            .map(|r| (r.key.as_str(), r.title.as_str(), r.place.as_str()))
            .collect();
        assert_eq!(
            rows,
            [
                ("SHOP-18", "Checkout API", "Review \u{b7} ready for you"),
                ("SHOP-29", "Coupon field", "Dev \u{b7} working"),
            ]
        );
        assert_eq!(
            (s.waits_for.rows[0].dot, s.waits_for.rows[1].dot),
            (Palette::Review, Palette::Working)
        );
        assert_eq!(s.rule, loop_refused());
        assert!(s.waiting_for_it.rows.is_empty());
        assert_eq!(s.waiting_for_it.empty, "Nothing waits for SHOP-20.");
        // What waits for a ticket is listed on it, read-only: the link is the waiting ticket's.
        let s18 = section(&columns, get(&all, "SHOP-18"), &all, 0).unwrap();
        assert_eq!(s18.waiting_for_it.label, "Waiting for it");
        assert_eq!(s18.waiting_for_it.rows[0].key, "SHOP-20");
        assert!(!s18.waiting_for_it.removable);
        assert_eq!(s18.waits_for.note, None);
    }

    #[test]
    fn search_leaves_out_itself_the_linked_and_the_done_and_refuses_loops_in_words() {
        let flow = shop_web();
        let columns = Columns::of(&flow);
        let all = example();
        let t20 = taken(get(&all, "SHOP-20"));
        let keys = |rows: Vec<SearchRow>| rows.into_iter().map(|r| r.key).collect::<Vec<_>>();
        assert_eq!(
            keys(search(&columns, &all, &t20, "SHOP-3", 10)),
            ["SHOP-31", "SHOP-35", "SHOP-33", "SHOP-30", "SHOP-32"]
        );
        // A new ticket leaves out only what its form already picked.
        let picked: Vec<TicketKey> = vec!["SHOP-31".parse().unwrap()];
        assert_eq!(
            keys(search(&columns, &all, &picked, "SHOP-3", 10)),
            ["SHOP-35", "SHOP-33", "SHOP-30", "SHOP-32"]
        );
        let found = search(&columns, &all, &t20, "slow", 10);
        assert_eq!(
            (found[0].key.as_str(), found[0].place.as_str()),
            ("SHOP-35", "Product spec")
        );
        assert!(
            keys(search(&columns, &all, &t20, "", 99))
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
