//! Drag and drop on the board (UI-02, TKT-04; *03 · Board & columns → Drag · over an allowed column, Drag ·
//! over a forbidden column*). The rules are the core's [`check_move_in`] on the project's flow: a column
//! under the dragged card is drawn allowed (accent) or forbidden (red, with the reason). A drop does what the
//! rules allow, and a ticket with a short or empty description starts without a warning (d0067). Backlog → Ready
//! for Dev or Dev queues it (it lands in Ready for Dev until it can start), Ready for Dev → Backlog takes it out of
//! the queue without asking (d0066). A drop on Approved (since d0087 the Review column's Approved section) asks how
//! (Local merge or Open PR) and never approves by itself (T-0095); Dev → Backlog asks to park first (T-0107);
//! Approved → Ready for Review (the Review column outside its Approved section) is PR rework (the PR stays,
//! T-0150). The page's drop targets map onto the core's drop targets (`board_page::DropTargetId`,
//! `flow_columns::Columns::drop_target`). Each rule has a `_in` form that takes the project's flow; the short form
//! is the default flow's.

use otaq_core::board::moves::{
    DropOn, MERGED_LOCKED, MoveAction, MoveCheck, ONLY_AGENT_SUBMITS, check_move_in,
};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::flow::engine::{Target, skipped_by_drag};
use otaq_core::flow::{Flow, Stage};

use crate::icons::IconName;

fn default_flow() -> &'static Flow {
    &otaq_core::flow::default_file().flow
}

/// *03 · Drag · over a forbidden column*: the reason over Ready for Review.
pub const ONLY_AGENTS_TO_REVIEW: &str = "Only agents move tickets to Ready for Review";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropFeedback {
    /// The card's own column: no styling.
    Home,
    Allowed,
    Forbidden {
        title: String,
        detail: Option<&'static str>,
    },
}

/// [`feedback_in`] on the default flow.
pub fn feedback(ticket: &Ticket, tickets: &[Ticket], column: impl Into<DropOn>) -> DropFeedback {
    feedback_in(default_flow(), ticket, tickets, column)
}

/// How a column looks while `ticket` is dragged over it, on `flow`'s board. The project's tickets don't change it.
pub fn feedback_in(
    flow: &Flow,
    ticket: &Ticket,
    _tickets: &[Ticket],
    column: impl Into<DropOn>,
) -> DropFeedback {
    let column = column.into();
    match check_move_in(flow, ticket, &column) {
        MoveCheck::Unchanged => DropFeedback::Home,
        MoveCheck::Refused { reason } => {
            let detail = match reason.as_str() {
                // Only a Backlog card can be queued instead.
                ONLY_AGENT_SUBMITS if ticket.place.is_backlog() => {
                    Some("Drag to Ready for Dev to queue it.")
                }
                MERGED_LOCKED => Some("Fixing merged work means a new ticket."),
                _ => None,
            };
            // The frame's shorter wording for the board (the CLI keeps the core's).
            let title = match reason.as_str() {
                ONLY_AGENT_SUBMITS => ONLY_AGENTS_TO_REVIEW.to_owned(),
                _ => reason,
            };
            DropFeedback::Forbidden { title, detail }
        }
        _ => DropFeedback::Allowed,
    }
}

/// The accent label under the pointer over an allowed column (*Drag · over an allowed column*: "Drop to queue ·
/// starts in priority order"; *PR rework*: "Drop to rework · PR #481 stays"): what the drop will do. `None` over the
/// card's own column and over a forbidden one.
pub fn drop_hint(
    ticket: &Ticket,
    tickets: &[Ticket],
    column: impl Into<DropOn>,
) -> Option<(IconName, String)> {
    drop_hint_in(default_flow(), ticket, tickets, column, None)
}

/// [`drop_hint`] on `flow`'s board. `left` is the stage a parked ticket left, by name, when known: dropped on another
/// stage it says it returns there (*03 → A parked card dropped anywhere*).
pub fn drop_hint_in(
    flow: &Flow,
    ticket: &Ticket,
    _tickets: &[Ticket],
    column: impl Into<DropOn>,
    left: Option<&str>,
) -> Option<(IconName, String)> {
    let column = column.into();
    let action = check_move_in(flow, ticket, &column).action()?;
    let to = match column.in_flow(flow) {
        Some(Target::Stage(stage)) => Some(stage),
        _ => None,
    };
    let to_name = to.map_or("", |s| s.name.as_str());
    let returns_to = left.filter(|_| {
        ticket.status == Status::Parked && to.is_some_and(|s| Some(s) != flow.first_agent_stage())
    });
    Some(match action {
        // Dropped past Ready for Dev and the first Agent stage, a parked card still goes back to the stage it left.
        MoveAction::Start if returns_to.is_some() => (
            IconName::Play,
            format!(
                "Returns to {}, the stage it left",
                returns_to.unwrap_or_default()
            ),
        ),
        MoveAction::Start if ticket.status == Status::Parked => (
            IconName::Play,
            "Drop to resume · same conversation".to_owned(),
        ),
        MoveAction::Start => (
            IconName::Play,
            "Drop to queue · starts in priority order".to_owned(),
        ),
        MoveAction::Unqueue => (
            IconName::Undo2,
            "Drop to take it out of the queue".to_owned(),
        ),
        MoveAction::Park => (
            IconName::SquareParking,
            "Drop to park · asks first".to_owned(),
        ),
        // *Row · Review column (0086 · V)*: the Approved section as the target.
        MoveAction::Approve => (
            IconName::Check,
            format!(
                "Drop to approve {} \u{2192} choose Local merge or Open PR",
                ticket.key
            ),
        ),
        MoveAction::PrRework => (
            IconName::RotateCcw,
            match ticket.flags.pr_number {
                Some(n) => format!("Drop to rework · PR #{n} stays"),
                None => "Drop to rework".to_owned(),
            },
        ),
        MoveAction::Reopen => (IconName::RotateCcw, "Drop to reopen".to_owned()),
        // FLW-18: back to any earlier stage, forward to any later one.
        MoveAction::SendBack => (
            IconName::Undo2,
            format!("Drop to send back to {to_name} \u{b7} asks first"),
        ),
        MoveAction::RunCheckAgain => (
            IconName::RotateCcw,
            format!("Drop to run {to_name} again \u{b7} asks first"),
        ),
        MoveAction::MoveBack => (IconName::Undo2, format!("Drop to move back to {to_name}")),
        MoveAction::NextStage => (IconName::ArrowRight, format!("Drop to move to {to_name}")),
        MoveAction::SkipTo => (
            IconName::SkipForward,
            format!(
                "Drop to skip {}",
                skipped_names(flow, ticket, to).join(", ")
            ),
        ),
    })
}

/// The stages a forward drag of `ticket` to `to` walks over, by name (the ticket's own skips aren't known here).
fn skipped_names<'f>(flow: &'f Flow, ticket: &Ticket, to: Option<&Stage>) -> Vec<&'f str> {
    let index = |id: &str| flow.position(id);
    let (Some(from), Some(to)) = (
        ticket.place.stage_id().and_then(index),
        to.and_then(|s| index(&s.id)),
    ) else {
        return Vec::new();
    };
    let leaves_check = !flow.stages[from].is_agent_stage();
    skipped_by_drag(flow, &[], from, to, leaves_check)
        .into_iter()
        .map(|s| s.name.as_str())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropAction {
    /// Nothing happens (its own column, or refused: the card snaps back).
    Nothing,
    /// Ask the service to move it.
    Move,
    /// Ready for Review → Approved: the approve choice opens; cancelling leaves the card where it was.
    ChooseApprove,
    /// Dev → Backlog: the Park confirmation; Cancel leaves the card in Dev.
    AskPark,
    /// Back to an earlier Agent stage or agent check (FLW-18): the dialog with the instructions or the note first;
    /// Cancel leaves the card where it was.
    AskDragBack,
    /// Allowed by the rules, carried out by a later ticket.
    NotYet(String),
}

/// [`on_drop_in`] on the default flow.
pub fn on_drop(ticket: &Ticket, tickets: &[Ticket], column: impl Into<DropOn>) -> DropAction {
    on_drop_in(default_flow(), ticket, tickets, column)
}

/// What a drop of `ticket` on `column` of `flow`'s board does. The project's tickets don't change it.
pub fn on_drop_in(
    flow: &Flow,
    ticket: &Ticket,
    _tickets: &[Ticket],
    column: impl Into<DropOn>,
) -> DropAction {
    let column = column.into();
    match check_move_in(flow, ticket, &column) {
        MoveCheck::Unchanged | MoveCheck::Refused { .. } => DropAction::Nothing,
        MoveCheck::Allowed {
            action:
                MoveAction::Start
                | MoveAction::Unqueue
                | MoveAction::Reopen
                | MoveAction::PrRework
                | MoveAction::MoveBack
                | MoveAction::NextStage
                | MoveAction::SkipTo,
        } => DropAction::Move,
        MoveCheck::Confirm {
            action: MoveAction::SendBack | MoveAction::RunCheckAgain,
        } => DropAction::AskDragBack,
        MoveCheck::Choose {
            action: MoveAction::Approve,
        } => DropAction::ChooseApprove,
        MoveCheck::Confirm {
            action: MoveAction::Park,
        } => DropAction::AskPark,
        _ => DropAction::NotYet("This move isn’t available yet".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::board::moves::DropTarget;
    use otaq_core::domain::Status;

    #[test]
    fn allowed_and_forbidden_columns() {
        let backlog = ticket("SHOP-31", "Add order history page", Status::Backlog);
        assert_eq!(
            feedback(&backlog, &[], DropTarget::Backlog),
            DropFeedback::Home
        );
        assert_eq!(
            feedback(&backlog, &[], DropTarget::Dev),
            DropFeedback::Allowed
        );
        assert_eq!(
            feedback(&backlog, &[], DropTarget::ReadyForDev),
            DropFeedback::Allowed
        );
        assert_eq!(
            feedback(&backlog, &[], DropTarget::ReadyForReview),
            DropFeedback::Forbidden {
                title: ONLY_AGENTS_TO_REVIEW.into(),
                detail: Some("Drag to Ready for Dev to queue it."),
            }
        );
        let queued = ticket("SHOP-20", "Checkout page", Status::Queued);
        assert_eq!(
            feedback(&queued, &[], DropTarget::Backlog),
            DropFeedback::Allowed
        );
        assert!(matches!(
            feedback(&queued, &[], DropTarget::Dev),
            DropFeedback::Forbidden { .. }
        ));
        let merged = ticket("SHOP-17", "Add Stripe client", Status::Merged);
        assert!(matches!(
            feedback(&merged, &[], DropTarget::Backlog),
            DropFeedback::Forbidden { .. }
        ));
        let working = ticket("SHOP-19", "Price table", Status::Working);
        assert_eq!(
            feedback(&working, &[], DropTarget::Backlog),
            DropFeedback::Allowed,
            "Park"
        );
        assert_eq!(feedback(&working, &[], DropTarget::Dev), DropFeedback::Home);
    }

    #[test]
    fn drop_hints_say_what_the_drop_does() {
        let backlog = ticket("SHOP-24", "Update footer links", Status::Backlog);
        assert_eq!(
            drop_hint(&backlog, &[], DropTarget::ReadyForDev),
            Some((
                IconName::Play,
                "Drop to queue · starts in priority order".to_owned()
            )),
            "the frame's wording"
        );
        assert_eq!(drop_hint(&backlog, &[], DropTarget::Backlog), None, "home");
        assert_eq!(drop_hint(&backlog, &[], DropTarget::ReadyForReview), None);
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        assert_eq!(
            drop_hint(&pr, &[], DropTarget::ReadyForReview).unwrap().1,
            "Drop to rework · PR #481 stays"
        );
        let working = ticket("SHOP-25", "Upgrade to Next.js 15", Status::Working);
        assert_eq!(
            drop_hint(&working, &[], DropTarget::Backlog).unwrap().1,
            "Drop to park · asks first"
        );
        let mut parked = ticket("SHOP-25", "Upgrade to Next.js 15", Status::Parked);
        parked.place = otaq_core::domain::Place::Backlog;
        assert!(
            drop_hint(&parked, &[], DropTarget::Dev)
                .unwrap()
                .1
                .contains("resume")
        );
    }

    #[test]
    fn drops() {
        // An empty description is a plain Start: no warning (d0067).
        let mut t24 = ticket("SHOP-24", "Update footer links", Status::Backlog);
        assert!(t24.body.is_empty());
        assert_eq!(on_drop(&t24, &[], DropTarget::Dev), DropAction::Move);
        t24.body = "Fix the links.".into();
        assert_eq!(on_drop(&t24, &[], DropTarget::Dev), DropAction::Move);
        assert_eq!(
            on_drop(&t24, &[], DropTarget::Approved),
            DropAction::Nothing
        );
        assert_eq!(
            on_drop(&t24, &[], DropTarget::ReadyForDev),
            DropAction::Move
        );
        // Ready for Dev → Backlog: out of the queue, no confirmation (d0066).
        let queued = ticket("SHOP-20", "Checkout page", Status::Queued);
        assert_eq!(on_drop(&queued, &[], DropTarget::Backlog), DropAction::Move);
        let working = ticket("SHOP-19", "Price table", Status::Working);
        assert_eq!(
            on_drop(&working, &[], DropTarget::Backlog),
            DropAction::AskPark
        );
        let review = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        assert_eq!(
            on_drop(&review, &[], DropTarget::Approved),
            DropAction::ChooseApprove
        );
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        assert_eq!(
            feedback(&pr, &[], DropTarget::ReadyForReview),
            DropFeedback::Allowed
        );
        assert_eq!(
            on_drop(&pr, &[], DropTarget::ReadyForReview),
            DropAction::Move,
            "PR rework: the PR stays"
        );
        // Back to Dev from the open PR: the instructions dialog first (FLW-18).
        assert_eq!(feedback(&pr, &[], DropTarget::Dev), DropFeedback::Allowed);
        assert_eq!(on_drop(&pr, &[], DropTarget::Dev), DropAction::AskDragBack);
        assert!(matches!(
            feedback(&pr, &[], DropTarget::Backlog),
            DropFeedback::Forbidden { .. }
        ));
        let closed = ticket("SHOP-26", "Add sitemap", Status::ClosedNotMerged);
        assert_eq!(
            on_drop(&closed, &[], DropTarget::Backlog),
            DropAction::Move,
            "the service reopens or says why not"
        );
    }
}
