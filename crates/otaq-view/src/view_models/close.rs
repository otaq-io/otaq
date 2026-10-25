//! Close ticket, Reopen and Move to next stage (was Submit for review) in the UI (CLS-01, CLS-03, CLS-05; d0033,
//! d0044, d0052, d0108; *05 → Close ticket · panel "…" menu, Close ticket · confirm, Submit for review · dialog,
//! Closed · not merged · Reopen*): which menu items show, and the dialogs' texts.

use otaq_core::board::review::USER_SUBMITS_FROM;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;

use crate::icons::IconName;
use crate::view_models::menu::MenuItem;

pub const CLOSE_TICKET: &str = "close-ticket";
pub const SUBMIT_FOR_REVIEW: &str = "submit-for-review";

/// Close ticket is offered on every ticket that isn't finished.
pub fn can_close(ticket: &Ticket) -> bool {
    !matches!(ticket.status, Status::Merged | Status::ClosedNotMerged)
}

/// "Close ticket…" in the sheet's "…" menu and the card's right-click menu.
pub fn close_item(ticket: &Ticket) -> Option<MenuItem> {
    can_close(ticket).then(|| {
        MenuItem::new(CLOSE_TICKET, "Close ticket…")
            .icon(IconName::CircleSlash)
            .danger()
            .separated()
    })
}

/// "Move to next stage…" (was Submit for review): on a ticket whose agent is at work or stopped; enabled only when
/// it isn't mid-turn. It counts as the agent's `stage next` (d0108 rule 10): an agent check that follows runs.
pub fn submit_item(ticket: &Ticket) -> Option<MenuItem> {
    let shown = crate::view_models::flow_columns::place_set(ticket).at_work()
        && (USER_SUBMITS_FROM.contains(&ticket.status)
            || matches!(ticket.status, Status::Working | Status::WorkingInBackground));
    if !shown {
        return None;
    }
    let item = MenuItem::new(SUBMIT_FOR_REVIEW, "Move to next stage…").icon(IconName::Send);
    Some(if USER_SUBMITS_FROM.contains(&ticket.status) {
        item
    } else {
        item.disabled(true).hint("agent is working")
    })
}

/// While it validates, Close and Park stop both sessions and discard the round (d0063, *Close / Park · while
/// validating*): "SHOP-29’s agent and validator will be stopped. The validation is discarded (round 1 of 3 not
/// counted)."
pub fn validating_stop_line(ticket: &Ticket) -> Option<String> {
    if !matches!(ticket.status, Status::Checking | Status::CheckerNeedsYou) {
        return None;
    }
    let round = ticket
        .flags
        .validating_round
        .map(|r| format!(" (round {} of {} not counted)", r.round, r.of))
        .unwrap_or_default();
    Some(format!(
        "{}\u{2019}s agent and validator will be stopped. The validation is discarded{round}.",
        ticket.key
    ))
}

/// The close confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseText {
    pub title: String,
    pub lines: Vec<String>,
    pub footer: String,
    pub confirm: String,
}

pub fn close_text(ticket: &Ticket, tickets: &[Ticket], agent_running: bool) -> CloseText {
    let mut lines = Vec::new();
    let validating = validating_stop_line(ticket);
    if let Some(line) = &validating {
        lines.push(line.clone());
    } else if agent_running {
        lines.push("Its agent is running. Closing stops it; the session is not kept.".to_owned());
    }
    let mut dependents: Vec<&Ticket> = tickets
        .iter()
        .filter(|t| t.depends_on.contains(&ticket.key) && !t.place.is_done())
        .collect();
    dependents.sort_by(|a, b| a.key.cmp(&b.key));
    for d in dependents {
        lines.push(format!(
            "{} {} depends on it. It stays Queued with \u{201c}parent {} closed\u{201d}, and Needs you offers \
             Reopen parent or Remove dependency.",
            d.key, d.title, ticket.key
        ));
    }
    CloseText {
        title: format!("Close {} {} without merging?", ticket.key, ticket.title),
        lines,
        footer: "The branch is kept. The worktree is removed unless it has uncommitted changes (then it\u{2019}s \
                 listed in Cleanup). You can reopen the ticket later."
            .to_owned(),
        confirm: if validating.is_some() {
            "Stop both & close".to_owned()
        } else if agent_running {
            "Stop agent & close".to_owned()
        } else {
            "Close ticket".to_owned()
        },
    }
}

/// The submit dialog's title and texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitText {
    pub title: String,
    pub intro: String,
    pub footer: String,
}

pub fn submit_text(ticket: &Ticket, now: i64) -> SubmitText {
    let since = ticket
        .signal
        .as_ref()
        .and_then(|s| s.since)
        .or(ticket.status_since);
    let intro = match (ticket.status, since) {
        (Status::Interrupted, _) => {
            "Its session was interrupted. Send back resumes it later.".to_owned()
        }
        (_, Some(at)) => format!(
            "The agent ended its turn {} ago. Its session stays; Send back continues it.",
            crate::view_models::card::age_label(now - at)
        ),
        (_, None) => {
            "The agent isn’t working right now. Its session stays; Send back continues it."
                .to_owned()
        }
    };
    SubmitText {
        title: format!("Move {} {} to the next stage?", ticket.key, ticket.title),
        intro,
        // *Submit for review · dialog (0052, 0063)*, as the agent's own `stage next` (d0108 rule 10).
        footer: "As if the agent finished: an agent check that comes next runs first, and Auto-merge may follow its \
                 pass. The Review card shows \u{201c}Submitted by you\u{201d} with your note."
            .to_owned(),
    }
}

/// The card's line: "Submitted by you · “Paging works, tests pending”".
pub fn submitted_line(note: &str) -> String {
    let note = note.lines().next().unwrap_or_default().trim();
    if note.is_empty() {
        "Submitted by you".to_owned()
    } else {
        format!("Submitted by you \u{b7} \u{201c}{note}\u{201d}")
    }
}

/// The Park confirmation (*03 → Park · Dev → Backlog*): what stops and what is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParkText {
    pub title: String,
    pub message: String,
    /// What is kept, each with its icon (git-branch, folder, message-square).
    pub kept: Vec<(IconName, String)>,
}

pub fn park_text(ticket: &Ticket) -> ParkText {
    let branch = ticket
        .work
        .as_ref()
        .and_then(|w| w.branch.clone())
        .unwrap_or_else(|| otaq_core::naming::branch(&ticket.key.to_string()));
    ParkText {
        title: format!("Park {}?", ticket.key),
        // While it validates, Park's dialog uses the close confirmation's line (d0063).
        message: validating_stop_line(ticket)
            .unwrap_or_else(|| otaq_core::review::park::park_message(&ticket.key.to_string())),
        kept: vec![
            (IconName::GitBranch, branch),
            (
                IconName::Folder,
                "worktree \u{b7} kept with its files".to_owned(),
            ),
            (
                IconName::MessageSquare,
                "conversation \u{b7} resumes on Dev".to_owned(),
            ),
        ],
    }
}

/// The sheet's choices for a child of a closed parent (CLS-04): one pair per closed parent. Otaq picks
/// neither.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParentChoice {
    pub parent: otaq_core::domain::TicketKey,
    pub reopen: String,
    pub remove: String,
}

pub fn closed_parent_choices(ticket: &Ticket, tickets: &[Ticket]) -> Vec<ParentChoice> {
    use otaq_core::board::readiness::{ReadyState, readiness};
    if ticket.status != Status::ParentClosed {
        return Vec::new();
    }
    let ReadyState::ParentClosed { parents } = readiness(ticket, tickets) else {
        return Vec::new();
    };
    let one = parents.len() == 1;
    parents
        .into_iter()
        .map(|parent| ParentChoice {
            reopen: if one {
                "Reopen parent".to_owned()
            } else {
                format!("Reopen {parent}")
            },
            remove: if one {
                "Remove dependency".to_owned()
            } else {
                format!("Remove dependency on {parent}")
            },
            parent,
        })
        .collect()
}

/// "Remove dependency" asks once and names the link.
pub fn remove_dependency_text(
    child: &otaq_core::domain::TicketKey,
    parent: &otaq_core::domain::TicketKey,
) -> (String, String) {
    (
        format!("Remove the dependency {child} \u{2192} {parent}?"),
        format!(
            "{child} no longer waits for {parent}. It starts once its other dependencies are merged; {parent} stays \
             closed."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;

    #[test]
    fn close_is_offered_until_the_ticket_is_finished() {
        for status in [
            Status::Backlog,
            Status::Working,
            Status::WaitingForYou,
            Status::PrOpen,
        ] {
            assert!(
                close_item(&ticket("SHOP-19", "t", status)).is_some(),
                "{status}"
            );
        }
        assert!(close_item(&ticket("SHOP-19", "t", Status::Merged)).is_none());
        assert!(close_item(&ticket("SHOP-19", "t", Status::ClosedNotMerged)).is_none());
    }

    #[test]
    fn submit_is_enabled_only_when_the_agent_isnt_mid_turn() {
        for status in [
            Status::Idle,
            Status::AsksYou,
            Status::NeedsYou,
            Status::Interrupted,
        ] {
            let item = submit_item(&ticket("SHOP-19", "t", status)).unwrap();
            assert!(!item.disabled, "{status}");
        }
        let working = submit_item(&ticket("SHOP-19", "t", Status::Working)).unwrap();
        assert!(working.disabled);
        assert_eq!(working.hint.as_deref(), Some("agent is working"));
        assert!(submit_item(&ticket("SHOP-19", "t", Status::Backlog)).is_none());
        assert!(submit_item(&ticket("SHOP-19", "t", Status::WaitingForYou)).is_none());
    }

    #[test]
    fn the_close_confirmation_names_the_agent_and_the_children() {
        let shop19 = ticket("SHOP-19", "Price table", Status::Working);
        let mut shop20 = ticket("SHOP-20", "Checkout page", Status::Queued);
        shop20.depends_on = vec![shop19.key.clone()];
        let text = close_text(&shop19, &[shop19.clone(), shop20], true);
        assert_eq!(text.title, "Close SHOP-19 Price table without merging?");
        assert_eq!(
            text.lines,
            [
                "Its agent is running. Closing stops it; the session is not kept.".to_owned(),
                "SHOP-20 Checkout page depends on it. It stays Queued with \u{201c}parent SHOP-19 closed\u{201d}, \
                 and Needs you offers Reopen parent or Remove dependency."
                    .to_owned(),
            ]
        );
        assert_eq!(text.confirm, "Stop agent & close");
        assert_eq!(close_text(&shop19, &[], false).confirm, "Close ticket");
    }

    /// *Close / Park · while validating (SHOP-29)*.
    #[test]
    fn close_and_park_while_validating_stop_both() {
        let mut t = ticket("SHOP-29", "Coupon field", Status::Checking);
        t.flags.validating_round = Some(otaq_core::domain::ValidationRound { round: 1, of: 3 });
        let line = "SHOP-29\u{2019}s agent and validator will be stopped. The validation is discarded (round 1 of 3 \
                    not counted).";
        let text = close_text(&t, &[], true);
        assert_eq!(text.lines, [line.to_owned()]);
        assert_eq!(text.confirm, "Stop both & close");
        assert_eq!(park_text(&t).message, line);
        t.status = Status::Working;
        assert_eq!(close_text(&t, &[], true).confirm, "Stop agent & close");
    }

    #[test]
    fn a_child_of_closed_parents_offers_both_choices_per_parent() {
        let a = ticket("SHOP-19", "Price table", Status::ClosedNotMerged);
        let b = ticket("SHOP-26", "Add sitemap", Status::ClosedNotMerged);
        let mut child = ticket("SHOP-20", "Checkout page", Status::ParentClosed);
        child.depends_on = vec![a.key.clone()];
        let choices = closed_parent_choices(&child, &[a.clone(), b.clone(), child.clone()]);
        assert_eq!(choices.len(), 1);
        assert_eq!(
            (choices[0].reopen.as_str(), choices[0].remove.as_str()),
            ("Reopen parent", "Remove dependency")
        );
        child.depends_on.push(b.key.clone());
        let choices = closed_parent_choices(&child, &[a.clone(), b, child.clone()]);
        assert_eq!(choices[1].reopen, "Reopen SHOP-26");
        let (title, body) = remove_dependency_text(&child.key, &a.key);
        assert_eq!(title, "Remove the dependency SHOP-20 \u{2192} SHOP-19?");
        assert!(body.starts_with("SHOP-20 no longer waits for SHOP-19."));
        let working = ticket("SHOP-21", "x", Status::Working);
        assert!(closed_parent_choices(&working, &[]).is_empty());
    }

    #[test]
    fn the_park_confirmation_says_what_is_kept() {
        let t = ticket("SHOP-25", "Upgrade to Next.js 15", Status::Working);
        let text = park_text(&t);
        assert_eq!(text.title, "Park SHOP-25?");
        assert_eq!(
            text.message,
            "SHOP-25's agent will be stopped. Its branch, files and conversation are kept."
        );
        assert_eq!(
            text.kept[2],
            (
                IconName::MessageSquare,
                "conversation \u{b7} resumes on Dev".to_owned()
            )
        );
        assert_eq!(text.kept[1].0, IconName::Folder);
    }

    #[test]
    fn submit_texts_and_the_card_line() {
        let mut idle = ticket("SHOP-19", "Price table", Status::Idle);
        idle.status_since = Some(0);
        idle.signal = None;
        let text = submit_text(&idle, 120_000);
        assert_eq!(text.title, "Move SHOP-19 Price table to the next stage?");
        assert_eq!(
            text.intro,
            "The agent ended its turn 2m ago. Its session stays; Send back continues it."
        );
        assert!(
            text.footer
                .starts_with("As if the agent finished: an agent check that comes next runs first")
        );
        assert_eq!(
            submitted_line("Paging works, tests pending"),
            "Submitted by you \u{b7} \u{201c}Paging works, tests pending\u{201d}"
        );
        assert_eq!(submitted_line(""), "Submitted by you");
    }
}
