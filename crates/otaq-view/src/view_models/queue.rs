//! Queued tickets on the board (DEP-03, UI-03; d0049, d0059; *04 · Ticket card variants → Queued · waiting on
//! deps, Queued · waits for merge, Queued · ready to start, Queued · held: base diverged*; *12 · Stack &
//! dependencies → 2 · The chain runs, 3 · One parent merged, 4 · All parents merged*).
//!
//! Every Queued card says what it waits for, from the core's readiness (the same text as `otaq blocked`,
//! the Stack list and the sheet): "waits for SHOP-20 to merge" in the meta row while its parents are still at
//! work; a callout when a parent only waits for the user's merge ("waits for SHOP-18, SHOP-19 to merge (SHOP-18
//! in Review)"); "Starting…" once every parent is merged; "Waiting for a slot" when the agent limit is full.
//! When a ticket that waited for its dependencies starts, a toast says so.

use otaq_core::board::readiness::{ReadyState, WaitingParent, readiness};
use otaq_core::board::stack::queue_reason;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Status, TicketKey};
use otaq_core::start::sequence::WAITING_TO_START;

/// The end of the start toast's title; the toast host gives it the play icon.
pub const DEPENDENCIES_MERGED: &str = "all dependencies merged";

/// What a Queued card shows about its wait.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueueFacts {
    /// The waiting text ("waits for SHOP-20 to merge").
    pub waits_for: Option<String>,
    /// A parent only waits for the user's merge (in Review, or a PR open): the text becomes a callout.
    pub waits_for_merge: bool,
    /// Every dependency is merged: about to start.
    pub starting: bool,
    /// Parent closed: which parent.
    pub closed_parent: Option<TicketKey>,
}

/// The facts for `ticket`, given its project's tickets. Empty for tickets that aren't Queued (or held).
pub fn queue_facts(ticket: &Ticket, all: &[Ticket]) -> QueueFacts {
    if !matches!(ticket.status, Status::Queued | Status::ParentClosed) {
        return QueueFacts::default();
    }
    match readiness(ticket, all) {
        ReadyState::Ready => QueueFacts {
            starting: ticket.status == Status::Queued,
            ..QueueFacts::default()
        },
        ReadyState::ParentClosed { parents } => QueueFacts {
            closed_parent: parents.into_iter().next(),
            ..QueueFacts::default()
        },
        ReadyState::Waiting { parents } => QueueFacts {
            waits_for: queue_reason(ticket, all),
            waits_for_merge: parents.iter().any(merge_pending),
            ..QueueFacts::default()
        },
    }
}

fn merge_pending(parent: &WaitingParent) -> bool {
    matches!(parent.status, Status::WaitingForYou | Status::PrOpen)
}

/// The sheet's notice for a Queued ticket: the card's text as a sentence ("Waits for SHOP-18, SHOP-19 to merge
/// (SHOP-18 in Review).").
pub fn sheet_notice(ticket: &Ticket, all: &[Ticket]) -> Option<String> {
    let reason = queue_reason(ticket, all)?;
    let mut chars = reason.chars();
    let first = chars.next()?;
    Some(format!("{}{}.", first.to_uppercase(), chars.as_str()))
}

/// Where a started ticket branched from: its base and the base commit ("main", " @ 3b7d0e1").
fn branched_from(ticket: &Ticket) -> (String, String) {
    let base = ticket.base.branch().to_owned();
    let sha = ticket
        .work
        .as_ref()
        .and_then(|w| w.base_sha.as_deref())
        .map(|s| format!(" @ {}", &s[..s.len().min(7)]))
        .unwrap_or_default();
    (base, sha)
}

/// "SHOP-20 started · all dependencies merged" / "Checkout page branched from main @ 3b7d0e1", when a ticket
/// that waited for its dependencies leaves Queued with its agent working.
pub fn start_toast(before: &Ticket, after: &Ticket) -> Option<(String, String)> {
    if !WAITING_TO_START.contains(&before.status)
        || after.status != Status::Working
        || after.depends_on.is_empty()
    {
        return None;
    }
    let (base, sha) = branched_from(after);
    Some((
        format!("{} started · {DEPENDENCIES_MERGED}", after.key),
        format!("{} branched from {base}{sha}", after.title),
    ))
}

/// The working card's callout once a ticket with dependencies has started (*12 · Stack & dependencies → 4 · All
/// parents merged*): "from main @ 3b7d0e1 · all dependencies merged". Shown while its agent works; a child only
/// starts once every parent is merged (d0049), so a working ticket with links always started that way.
pub fn started_line(ticket: &Ticket) -> Option<String> {
    if ticket.status != Status::Working || ticket.depends_on.is_empty() {
        return None;
    }
    let (base, sha) = branched_from(ticket);
    if sha.is_empty() {
        return None;
    }
    Some(format!("from {base}{sha} · {DEPENDENCIES_MERGED}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::board::tickets::Work;

    fn deps(mut t: Ticket, parents: &[&str]) -> Ticket {
        t.depends_on = parents.iter().map(|p| p.parse().unwrap()).collect();
        t
    }

    /// Brief 12's chain at a storyboard step: SHOP-18 and SHOP-19 with the given statuses.
    fn chain(s18: Status, s19: Status) -> Vec<Ticket> {
        vec![
            ticket("SHOP-17", "Add Stripe client", Status::Merged),
            deps(ticket("SHOP-18", "Checkout API", s18), &["SHOP-17"]),
            ticket("SHOP-19", "Price table", s19),
            deps(
                ticket("SHOP-20", "Checkout page", Status::Queued),
                &["SHOP-18", "SHOP-19"],
            ),
            deps(
                ticket("SHOP-21", "Order confirmation email", Status::Queued),
                &["SHOP-20"],
            ),
        ]
    }

    fn facts(key: &str, all: &[Ticket]) -> QueueFacts {
        queue_facts(all.iter().find(|t| t.key.to_string() == key).unwrap(), all)
    }

    #[test]
    fn storyboard_2_the_chain_runs() {
        let all = chain(Status::Working, Status::Working);
        let shop20 = facts("SHOP-20", &all);
        assert_eq!(
            shop20.waits_for.as_deref(),
            Some("waits for SHOP-18, SHOP-19 to merge")
        );
        assert!(
            !shop20.waits_for_merge,
            "both parents still at work: meta row"
        );
        // B · SHOP-18 in Review: the line updates and becomes a callout.
        let all = chain(Status::WaitingForYou, Status::Working);
        let shop20 = facts("SHOP-20", &all);
        assert_eq!(
            shop20.waits_for.as_deref(),
            Some("waits for SHOP-18, SHOP-19 to merge (SHOP-18 in Review)")
        );
        assert!(shop20.waits_for_merge);
        assert_eq!(
            facts("SHOP-21", &all).waits_for.as_deref(),
            Some("waits for SHOP-20 to merge")
        );
    }

    #[test]
    fn storyboard_3_and_4() {
        let all = chain(Status::Merged, Status::WaitingForYou);
        assert_eq!(
            facts("SHOP-20", &all).waits_for.as_deref(),
            Some("waits for SHOP-19 to merge (in Review)")
        );
        let all = chain(Status::Merged, Status::Merged);
        let shop20 = facts("SHOP-20", &all);
        assert!(shop20.starting && shop20.waits_for.is_none());
    }

    #[test]
    fn closed_parent_and_slot() {
        let mut all = chain(Status::Merged, Status::ClosedNotMerged);
        all[3].status = Status::ParentClosed;
        assert_eq!(
            facts("SHOP-20", &all).closed_parent,
            Some("SHOP-19".parse().unwrap())
        );
        all[3].status = Status::WaitingForSlot;
        assert_eq!(
            facts("SHOP-20", &all),
            QueueFacts::default(),
            "the pill says it"
        );
        assert_eq!(
            sheet_notice(&all[3], &all).as_deref(),
            Some("Waiting for a slot.")
        );
    }

    #[test]
    fn panel_notice_reuses_the_card_text() {
        let all = chain(Status::WaitingForYou, Status::Working);
        assert_eq!(
            sheet_notice(&all[3], &all).as_deref(),
            Some("Waits for SHOP-18, SHOP-19 to merge (SHOP-18 in Review).")
        );
        assert_eq!(sheet_notice(&all[1], &all), None, "not queued");
    }

    #[test]
    fn a_toast_when_a_waiting_ticket_starts() {
        let all = chain(Status::Merged, Status::Merged);
        let before = all[3].clone();
        let mut after = before.clone();
        after.status = Status::Working;
        after.work = Some(Work {
            base_sha: Some("3b7d0e1c9a".into()),
            ..Work::default()
        });
        assert_eq!(
            start_toast(&before, &after),
            Some((
                "SHOP-20 started · all dependencies merged".into(),
                "Checkout page branched from main @ 3b7d0e1".into()
            ))
        );
        assert_eq!(
            started_line(&after).as_deref(),
            Some("from main @ 3b7d0e1 · all dependencies merged"),
            "storyboard 4's card"
        );
        // No dependencies, or a start that stopped: no toast.
        let mut lone = after.clone();
        lone.depends_on.clear();
        assert_eq!(start_toast(&before, &lone), None);
        assert_eq!(started_line(&lone), None);
        let mut stopped = after.clone();
        stopped.status = Status::StartInterrupted;
        assert_eq!(start_toast(&before, &stopped), None);
        // Already working: an update isn't a start.
        assert_eq!(start_toast(&after, &after), None);
    }
}
