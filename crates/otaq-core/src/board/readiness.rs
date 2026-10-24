//! Readiness, beads-style (DEP-03, DEP-04; d0049, d0044, d0046, d0059): a ticket may start only when **every**
//! parent is Done · Merged into the ticket's base. For an epic child that base is the epic's branch, so a
//! sibling counts once it is merged into the epic branch (d0059). A dependency on an epic is ready when the epic
//! is Done · Merged (d0046). **Closed never counts** (d0044): a closed parent, or an epic whose children all
//! ended Closed, makes the ticket "parent closed" until the user reopens the parent or removes the link.
//!
//! [`readiness`] is pure: the scheduler (T-0079), `otaq ready` / `blocked`, the cards, the Stack list and
//! the sheet all derive the same state and the same waiting text from it ("waits for SHOP-18, SHOP-19 to merge
//! (SHOP-18 in Review)", "waits for SHOP-18 · PR #482 open", "parent SHOP-26 closed"). Nothing is stored (SDD §8).
//!
//! Linked tickets always share a base (DEP-02, enforced when the link is made), so a Merged parent is merged
//! into the child's base; a parent whose base differs anyway is treated as not merged rather than trusted.

use serde::{Deserialize, Serialize};

use super::epics::{EpicProgress, progress_of};
use super::tickets::Ticket;
use crate::domain::{Kind, Status, StatusSet, TicketKey};

/// A parent a ticket still waits for, and how far it got.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitingParent {
    pub key: TicketKey,
    pub status: Status,
    /// The parent is an epic (DEP-04).
    #[serde(default, skip_serializing_if = "is_ticket")]
    pub kind: Kind,
    /// PR open: the PR's number ("PR #482 open").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_number: Option<u32>,
    /// An epic parent's progress ("1 / 3 merged").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<EpicProgress>,
}

fn is_ticket(kind: &Kind) -> bool {
    *kind == Kind::Ticket
}

impl WaitingParent {
    fn is_epic(&self) -> bool {
        self.kind == Kind::Epic
    }

    /// How far the parent got, when it says more than "not merged yet": "in Review", "PR #482 open",
    /// "1 / 3 merged" (an epic still in Backlog).
    pub fn progress_note(&self) -> Option<String> {
        match self.status {
            Status::WaitingForYou => Some("in Review".into()),
            Status::PrOpen => Some(match self.pr_number {
                Some(n) => format!("PR #{n} open"),
                None => "PR open".into(),
            }),
            _ => self.progress.map(|p| p.label()),
        }
    }
}

/// A ticket's dependency state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ReadyState {
    /// Every parent is merged into the ticket's base (or there are none).
    Ready,
    /// These parents aren't merged yet, in key order.
    Waiting { parents: Vec<WaitingParent> },
    /// These parents ended Closed without merging, in key order; they never count (d0044, CLS-04).
    ParentClosed { parents: Vec<TicketKey> },
}

impl ReadyState {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// The card's waiting line: "waits for SHOP-18, SHOP-19 to merge (SHOP-18 in Review)", "waits for SHOP-18 ·
    /// PR #482 open", "waits for epic SHOP-40 to be merged (1 / 3 merged)", "parent SHOP-26 closed". `None`
    /// when ready.
    pub fn reason(&self) -> Option<String> {
        match self {
            Self::Ready => None,
            Self::ParentClosed { parents } => Some(closed_parents_text(parents)),
            Self::Waiting { parents } => Some(waiting_text(parents)),
        }
    }
}

/// "parent SHOP-26 closed", "parents SHOP-26, SHOP-27 closed".
pub fn closed_parents_text(parents: &[TicketKey]) -> String {
    let keys: Vec<String> = parents.iter().map(|k| k.to_string()).collect();
    match keys.as_slice() {
        [one] => format!("parent {one} closed"),
        _ => format!("parents {} closed", keys.join(", ")),
    }
}

fn waiting_text(parents: &[WaitingParent]) -> String {
    if let [parent] = parents {
        let note = parent.progress_note();
        if parent.status == Status::PrOpen {
            return format!("waits for {} · {}", parent.key, note.unwrap_or_default());
        }
        let what = if parent.is_epic() {
            format!("epic {} to be merged", parent.key)
        } else {
            format!("{} to merge", parent.key)
        };
        return match note {
            Some(note) => format!("waits for {what} ({note})"),
            None => format!("waits for {what}"),
        };
    }
    let keys: Vec<String> = parents
        .iter()
        .map(|p| {
            if p.is_epic() {
                format!("epic {}", p.key)
            } else {
                p.key.to_string()
            }
        })
        .collect();
    let notes: Vec<String> = parents
        .iter()
        .filter_map(|p| {
            let note = p.progress_note()?;
            Some(match p.status {
                Status::WaitingForYou => format!("{} {note}", p.key),
                _ => format!("{} · {note}", p.key),
            })
        })
        .collect();
    let mut text = format!("waits for {} to merge", keys.join(", "));
    if !notes.is_empty() {
        text.push_str(&format!(" ({})", notes.join(", ")));
    }
    text
}

/// The dependency state of `ticket`, given the tickets of its project (its parents among them). A parent that
/// isn't among `tickets` is gone (deleted tickets lose their links) and is ignored.
pub fn readiness(ticket: &Ticket, tickets: &[Ticket]) -> ReadyState {
    let mut waiting = Vec::new();
    let mut parents: Vec<&Ticket> = ticket
        .depends_on
        .iter()
        .filter_map(|key| tickets.iter().find(|t| &t.key == key))
        .collect();
    parents.sort_by(|a, b| a.key.cmp(&b.key));
    let closed: Vec<TicketKey> = parents
        .iter()
        .filter(|p| is_closed(p, tickets))
        .map(|p| p.key.clone())
        .collect();
    if !closed.is_empty() {
        return ReadyState::ParentClosed { parents: closed };
    }
    for parent in parents {
        if parent.status == Status::Merged && parent.base == ticket.base {
            continue;
        }
        waiting.push(WaitingParent {
            key: parent.key.clone(),
            status: parent.status,
            kind: parent.kind,
            pr_number: parent.flags.pr_number,
            progress: parent
                .is_epic()
                .then(|| progress_of(&parent.key, tickets))
                .filter(|p| p.total > 0),
        });
    }
    if waiting.is_empty() {
        ReadyState::Ready
    } else {
        ReadyState::Waiting { parents: waiting }
    }
}

/// Closed without merging: a closed ticket, or an epic that ended "Closed · not merged" (also before the epic
/// itself moves there: every child Closed, none merged; d0044, d0046).
fn is_closed(parent: &Ticket, tickets: &[Ticket]) -> bool {
    if parent.status == Status::ClosedNotMerged {
        return true;
    }
    if !parent.is_epic() || parent.status == Status::Merged {
        return false;
    }
    let mut children = tickets
        .iter()
        .filter(|t| t.epic.as_ref() == Some(&parent.key))
        .peekable();
    children.peek().is_some() && children.all(|c| c.status == Status::ClosedNotMerged)
}

/// A Backlog ticket that could start: not an epic, every parent merged (d0066). Only a hint: Backlog never
/// starts by itself (SCH-02).
pub fn ready_to_start(ticket: &Ticket, tickets: &[Ticket]) -> bool {
    matches!(ticket.status, Status::Backlog | Status::Parked)
        && ticket.kind != Kind::Epic
        && readiness(ticket, tickets).is_ready()
}

/// One row of `otaq ready` / `blocked`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Readiness {
    pub key: TicketKey,
    pub title: String,
    pub status: Status,
    /// Every parent is merged (or there are none).
    pub ready: bool,
    /// The parents not merged yet (closed ones included), in key order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waiting_for: Vec<WaitingParent>,
    /// A parent ended Closed: the ticket can't become ready until it's reopened or the link removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_closed: Option<TicketKey>,
    /// The waiting text the card shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The dependency state of every ticket of a project that isn't done yet (Done tickets are left out), in key
/// order.
pub fn project_readiness(tickets: &[Ticket]) -> Vec<Readiness> {
    let mut rows: Vec<Readiness> = tickets
        .iter()
        .filter(|t| t.board_group() != StatusSet::Done)
        .map(|t| {
            let state = readiness(t, tickets);
            let waiting_for = t
                .depends_on
                .iter()
                .filter_map(|key| tickets.iter().find(|p| &p.key == key))
                .filter(|p| p.status != Status::Merged || p.base != t.base)
                .map(|p| WaitingParent {
                    key: p.key.clone(),
                    status: p.status,
                    kind: p.kind,
                    pr_number: p.flags.pr_number,
                    progress: None,
                })
                .collect();
            Readiness {
                key: t.key.clone(),
                title: t.title.clone(),
                status: t.status,
                ready: state.is_ready(),
                waiting_for,
                parent_closed: match &state {
                    ReadyState::ParentClosed { parents } => parents.first().cloned(),
                    _ => None,
                },
                reason: state.reason(),
            }
        })
        .collect();
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::tickets::Base;
    use crate::domain::{Actor, Flags};

    fn ticket(key: &str, status: Status) -> Ticket {
        Ticket {
            id: key.into(),
            key: key.parse().unwrap(),
            kind: Kind::Ticket,
            title: key.into(),
            body: String::new(),
            priority: Default::default(),
            priority_inherited: false,
            epic: None,
            place: crate::domain::Place::of(status, None, None),
            status,
            base: Base::Branch("main".into()),
            base_locked: false,
            external_id: None,
            depends_on: Vec::new(),
            aliases: Vec::new(),
            next_child_number: None,
            flags: Flags::default(),
            created_by: Actor::User("me".into()),
            updated_by: None,
            started_at: None,
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
            epic_queue_mode: Default::default(),
            joined_queue_by: None,
        }
    }

    fn deps(mut t: Ticket, parents: &[&str]) -> Ticket {
        t.depends_on = parents.iter().map(|p| p.parse().unwrap()).collect();
        t
    }

    /// The Payments chain of brief 12: SHOP-17 → SHOP-18 → SHOP-20 ← SHOP-19, SHOP-21 after SHOP-20.
    fn chain(s18: Status, s19: Status) -> Vec<Ticket> {
        vec![
            ticket("SHOP-17", Status::Merged),
            deps(ticket("SHOP-18", s18), &["SHOP-17"]),
            ticket("SHOP-19", s19),
            deps(ticket("SHOP-20", Status::Queued), &["SHOP-18", "SHOP-19"]),
            deps(ticket("SHOP-21", Status::Queued), &["SHOP-20"]),
        ]
    }

    fn state_of(key: &str, tickets: &[Ticket]) -> ReadyState {
        let t = tickets.iter().find(|t| t.key.to_string() == key).unwrap();
        readiness(t, tickets)
    }

    fn reason_of(key: &str, tickets: &[Ticket]) -> Option<String> {
        state_of(key, tickets).reason()
    }

    #[test]
    fn storyboard_2_the_chain_runs() {
        let board = chain(Status::Working, Status::Working);
        assert!(state_of("SHOP-18", &board).is_ready(), "SHOP-17 is merged");
        assert!(state_of("SHOP-19", &board).is_ready(), "no parents");
        assert_eq!(
            reason_of("SHOP-20", &board).unwrap(),
            "waits for SHOP-18, SHOP-19 to merge"
        );
        assert_eq!(
            reason_of("SHOP-21", &board).unwrap(),
            "waits for SHOP-20 to merge"
        );

        // The line updates as each parent moves.
        let board = chain(Status::WaitingForYou, Status::Working);
        assert_eq!(
            reason_of("SHOP-20", &board).unwrap(),
            "waits for SHOP-18, SHOP-19 to merge (SHOP-18 in Review)"
        );
        let ReadyState::Waiting { parents } = state_of("SHOP-20", &board) else {
            panic!("waiting");
        };
        let states: Vec<_> = parents
            .iter()
            .map(|p| (p.key.to_string(), p.status))
            .collect();
        assert_eq!(
            states,
            [
                ("SHOP-18".into(), Status::WaitingForYou),
                ("SHOP-19".into(), Status::Working)
            ]
        );
    }

    #[test]
    fn storyboard_3_one_parent_merged() {
        let board = chain(Status::Merged, Status::Working);
        assert_eq!(
            reason_of("SHOP-20", &board).unwrap(),
            "waits for SHOP-19 to merge"
        );
        let board = chain(Status::Merged, Status::WaitingForYou);
        assert_eq!(
            reason_of("SHOP-20", &board).unwrap(),
            "waits for SHOP-19 to merge (in Review)"
        );
    }

    #[test]
    fn storyboard_4_all_parents_merged() {
        let board = chain(Status::Merged, Status::Merged);
        assert_eq!(state_of("SHOP-20", &board), ReadyState::Ready);
        assert_eq!(reason_of("SHOP-20", &board), None);
        // SHOP-21 still waits: SHOP-20 hasn't even started.
        assert_eq!(
            reason_of("SHOP-21", &board).unwrap(),
            "waits for SHOP-20 to merge"
        );
    }

    #[test]
    fn storyboard_5_pr_variant() {
        let mut board = chain(Status::PrOpen, Status::Merged);
        board[1].flags.pr_number = Some(482);
        assert_eq!(
            reason_of("SHOP-20", &board).unwrap(),
            "waits for SHOP-18 · PR #482 open"
        );
        board[2].status = Status::Working;
        assert_eq!(
            reason_of("SHOP-20", &board).unwrap(),
            "waits for SHOP-18, SHOP-19 to merge (SHOP-18 · PR #482 open)"
        );
        // Mark merged.
        let board = chain(Status::Merged, Status::Merged);
        assert!(state_of("SHOP-20", &board).is_ready());
    }

    #[test]
    fn storyboard_6_closed_parent_never_counts() {
        let board = chain(Status::Merged, Status::ClosedNotMerged);
        assert_eq!(
            state_of("SHOP-20", &board),
            ReadyState::ParentClosed {
                parents: vec!["SHOP-19".parse().unwrap()]
            }
        );
        assert_eq!(
            reason_of("SHOP-20", &board).unwrap(),
            "parent SHOP-19 closed"
        );
        // A closed parent wins over parents still in progress.
        let board = chain(Status::Working, Status::ClosedNotMerged);
        assert_eq!(
            reason_of("SHOP-20", &board).unwrap(),
            "parent SHOP-19 closed"
        );
    }

    fn epic_board(children: &[Status], epic_status: Status) -> Vec<Ticket> {
        let mut epic = ticket("SHOP-40", epic_status);
        epic.kind = Kind::Epic;
        let mut board = vec![epic];
        for (i, status) in children.iter().enumerate() {
            let mut child = ticket(&format!("SHOP-40.{}", i + 1), *status);
            child.epic = Some("SHOP-40".parse().unwrap());
            child.base = Base::Epic("SHOP-40".parse().unwrap());
            board.push(child);
        }
        board.push(deps(ticket("SHOP-50", Status::Queued), &["SHOP-40"]));
        board
    }

    #[test]
    fn a_dependency_on_an_epic_is_ready_when_the_epic_is_merged() {
        let board = epic_board(
            &[Status::Merged, Status::Working, Status::Backlog],
            Status::Backlog,
        );
        assert_eq!(
            reason_of("SHOP-50", &board).unwrap(),
            "waits for epic SHOP-40 to be merged (1 of 3 merged)"
        );
        let board = epic_board(
            &[Status::Merged, Status::ClosedNotMerged],
            Status::WaitingForYou,
        );
        assert_eq!(
            reason_of("SHOP-50", &board).unwrap(),
            "waits for epic SHOP-40 to be merged (in Review)"
        );
        let board = epic_board(&[Status::Merged, Status::Merged], Status::Merged);
        assert!(state_of("SHOP-50", &board).is_ready());
    }

    #[test]
    fn an_epic_with_all_children_closed_never_counts() {
        let board = epic_board(
            &[Status::ClosedNotMerged, Status::ClosedNotMerged],
            Status::ClosedNotMerged,
        );
        assert_eq!(
            reason_of("SHOP-50", &board).unwrap(),
            "parent SHOP-40 closed"
        );
        // Also before the epic itself has moved to "Closed · not merged".
        let board = epic_board(
            &[Status::ClosedNotMerged, Status::ClosedNotMerged],
            Status::Backlog,
        );
        assert_eq!(
            reason_of("SHOP-50", &board).unwrap(),
            "parent SHOP-40 closed"
        );
        // An empty epic isn't closed: it waits.
        let board = epic_board(&[], Status::Backlog);
        assert_eq!(
            reason_of("SHOP-50", &board).unwrap(),
            "waits for epic SHOP-40 to be merged"
        );
    }

    #[test]
    fn epic_children_wait_for_siblings_merged_into_the_epic_branch() {
        let mut board = epic_board(&[Status::Working, Status::Queued], Status::Backlog);
        board[2].depends_on = vec!["SHOP-40.1".parse().unwrap()];
        assert_eq!(
            reason_of("SHOP-40.2", &board).unwrap(),
            "waits for SHOP-40.1 to merge"
        );
        board[1].status = Status::Merged;
        assert!(state_of("SHOP-40.2", &board).is_ready());
    }

    #[test]
    fn a_parent_merged_into_another_base_does_not_count() {
        let mut board = chain(Status::Merged, Status::Merged);
        board[1].base = Base::Branch("release/2.3".into());
        assert_eq!(
            reason_of("SHOP-20", &board).unwrap(),
            "waits for SHOP-18 to merge"
        );
    }

    #[test]
    fn several_parents_mixed_with_an_epic() {
        let mut board = epic_board(&[Status::Merged, Status::Working], Status::Backlog);
        board.push(ticket("SHOP-18", Status::WaitingForYou));
        board.last_mut().unwrap().depends_on.clear();
        let i = board
            .iter()
            .position(|t| t.key.to_string() == "SHOP-50")
            .unwrap();
        board[i].depends_on = vec!["SHOP-18".parse().unwrap(), "SHOP-40".parse().unwrap()];
        assert_eq!(
            reason_of("SHOP-50", &board).unwrap(),
            "waits for SHOP-18, epic SHOP-40 to merge (SHOP-18 in Review, SHOP-40 · 1 of 2 merged)"
        );
    }

    #[test]
    fn project_rows_for_ready_and_blocked() {
        let board = chain(Status::WaitingForYou, Status::ClosedNotMerged);
        let rows = project_readiness(&board);
        let keys: Vec<_> = rows.iter().map(|r| r.key.to_string()).collect();
        assert_eq!(keys, ["SHOP-18", "SHOP-20", "SHOP-21"], "Done left out");
        let shop20 = &rows[1];
        assert!(!shop20.ready);
        assert_eq!(shop20.parent_closed, Some("SHOP-19".parse().unwrap()));
        assert_eq!(shop20.reason.as_deref(), Some("parent SHOP-19 closed"));
        assert_eq!(shop20.waiting_for.len(), 2);
        assert!(rows[0].ready && rows[0].reason.is_none());
    }
}
