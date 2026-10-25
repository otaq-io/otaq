//! The ticket-change note (SES-08, d0048): when the **user** saves a change to a ticket whose agent is running,
//! the agent gets a short note with only what changed, for example *"The ticket was updated by the user. The
//! description changed: added 'works on mobile widths'. Run `otaq prime` for the full ticket."*
//!
//! The note goes through the automatic-delivery queue (T-0054): it waits until the agent is idle between turns and
//! its input line is empty. Agent edits send nothing; Queued, Parked and Backlog tickets get nothing.
//!
//! Several saves before a delivery queue **one note each, in order** (implementation choice): each says exactly
//! what that save changed, and they are typed one after the other when the agent is idle.

use super::tickets::Ticket;
use crate::naming;

/// The note for a change from `before` to `after`, or `None` when nothing the agent works from changed (pins,
/// External ID, status). The base can't change once a ticket has started (d0051), so it never appears.
pub fn change_note(before: &Ticket, after: &Ticket) -> Option<String> {
    let parts = change_parts(before, after);
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "The ticket was updated by the user. {} Run `{} prime` for the full ticket.",
        parts.join(" "),
        naming::COMMAND_NAME
    ))
}

/// The note's sentences, one per kind of change ("The description changed: added 'works on mobile widths'."); the
/// editor shows them after a save too.
pub fn change_parts(before: &Ticket, after: &Ticket) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    if before.title != after.title {
        parts.push(format!("The title is now '{}'.", after.title));
    }
    if before.body.trim() != after.body.trim() {
        parts.push(description_part(&before.body, &after.body));
    }
    let added: Vec<String> = after
        .depends_on
        .iter()
        .filter(|k| !before.depends_on.contains(k))
        .map(ToString::to_string)
        .collect();
    let removed: Vec<String> = before
        .depends_on
        .iter()
        .filter(|k| !after.depends_on.contains(k))
        .map(ToString::to_string)
        .collect();
    if !added.is_empty() {
        parts.push(format!("It now depends on {}.", added.join(", ")));
    }
    if !removed.is_empty() {
        parts.push(format!("It no longer depends on {}.", removed.join(", ")));
    }
    parts
}

fn quoted(items: &[&str]) -> String {
    items
        .iter()
        .map(|i| format!("'{i}'"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Up to this many added or removed lines are quoted; a bigger change only says that the description changed
/// (implementation choice: the note stays short, `prime` has the rest).
const QUOTED_LINES: usize = 3;

/// "The description changed: added 'works on mobile widths'." Lines are compared without their list markers, so
/// a criterion added to a "Done when" list in the description reads as itself (d0067).
fn description_part(before: &str, after: &str) -> String {
    if after.trim().is_empty() {
        return "The description was cleared.".to_owned();
    }
    let old = lines(before);
    let new = lines(after);
    let added: Vec<&str> = new.iter().copied().filter(|l| !old.contains(l)).collect();
    let removed: Vec<&str> = old.iter().copied().filter(|l| !new.contains(l)).collect();
    let mut what = Vec::new();
    if !added.is_empty() {
        what.push(format!("added {}", quoted(&added)));
    }
    if !removed.is_empty() {
        what.push(format!("removed {}", quoted(&removed)));
    }
    if what.is_empty() || added.len() + removed.len() > QUOTED_LINES {
        "The description changed.".to_owned()
    } else {
        format!("The description changed: {}.", what.join("; "))
    }
}

/// The description's non-empty lines, trimmed, without a leading list marker ("- ", "* ", "- [ ] ").
fn lines(text: &str) -> Vec<&str> {
    text.lines()
        .map(|line| {
            let line = line.trim();
            let line = line
                .strip_prefix("- ")
                .or_else(|| line.strip_prefix("* "))
                .unwrap_or(line);
            let line = line
                .strip_prefix("[ ] ")
                .or_else(|| line.strip_prefix("[x] "))
                .unwrap_or(line);
            line.trim()
        })
        .filter(|line| !line.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tickets::Base;
    use crate::domain::{Actor, Flags, Status};

    fn ticket(body: &str) -> Ticket {
        Ticket {
            id: "t19".into(),
            key: "SHOP-19".parse().unwrap(),
            title: "Price table".into(),
            body: body.into(),
            priority: Default::default(),
            place: crate::domain::Place::stage_of(crate::flow::default::DEV),
            status: Status::Working,
            base: Base::Branch("main".into()),
            base_locked: true,
            external_id: None,
            depends_on: vec![],
            flags: Flags::default(),
            created_by: Actor::User("me".into()),
            updated_by: None,
            started_at: Some(1),
            created_at: 0,
            updated_at: 0,
            status_since: None,
            queued_at: None,
            ready_since: None,
            validation_round: 0,
            auto_merge_conflicts: 0,
            auto_merge_stop: None,
            work: None,
            start_problem: None,
            interrupted_from: None,
            signal: None,
            question: None,
            pushed_differs: None,
        }
    }

    fn note(before: &Ticket, after: &Ticket) -> String {
        change_note(before, after).unwrap()
    }

    #[test]
    fn the_sdd_sample() {
        let before = ticket("Show the prices.\n\n## Done when\n\n- sorted by price");
        let after = ticket(
            "Show the prices.\n\n## Done when\n\n- sorted by price\n- works on mobile widths",
        );
        assert_eq!(
            note(&before, &after),
            format!(
                "The ticket was updated by the user. The description changed: added 'works on mobile widths'. \
                 Run `{} prime` for the full ticket.",
                naming::COMMAND_NAME
            )
        );
    }

    #[test]
    fn each_kind_of_change() {
        let before = ticket("Show the prices.\n- a\n- b");
        let mut after = ticket("Show the prices.\n- a\n- c");
        after.title = "Price table v2".into();
        after.depends_on = vec!["SHOP-18".parse().unwrap()];
        let text = note(&before, &after);
        assert_eq!(
            text,
            format!(
                "The ticket was updated by the user. The title is now 'Price table v2'. The description changed: \
                 added 'c'; removed 'b'. It now depends on SHOP-18. Run `{} prime` for the full ticket.",
                naming::COMMAND_NAME
            )
        );

        let reordered = note(&ticket("a\nb"), &ticket("b\na"));
        assert!(
            reordered.contains("The description changed. "),
            "{reordered}"
        );
        let big = note(&ticket("a"), &ticket("a\nb\nc\nd\ne"));
        assert!(big.contains("The description changed. "), "{big}");
        let mut unlinked = ticket("x");
        unlinked.depends_on = vec!["SHOP-18".parse().unwrap()];
        assert!(note(&unlinked, &ticket("x")).contains("It no longer depends on SHOP-18."));
        assert!(note(&ticket("x"), &ticket("  ")).contains("The description was cleared."));
    }

    #[test]
    fn nothing_the_agent_works_from_changed() {
        let before = ticket("a");
        let mut after = before.clone();
        after.external_id = Some("PAY-123".into());
        after.flags.pinned_at = Some(5);
        after.body = "a\n".into();
        assert_eq!(change_note(&before, &after), None);
    }
}
