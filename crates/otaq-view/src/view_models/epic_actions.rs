//! Epic actions on the board (EPC-01, EPC-06, EPC-07, TKT-07; d0033, d0039, d0044, d0059; *05 · Ticket side
//! panel → Epic dialogs*, *08 → Move to epic · base changes (SHOP-51)*): Move to epic… for tickets that never
//! went to Dev (the dialog previews the new key, says the old key keeps working and how the base changes), and
//! Close epic… with one confirmation built from the service's close plan.

use otaq_core::board::epics::{ClosePlan, progress_of};
use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::{Kind, Status, TicketKey};

/// Move to epic… is offered only for a plain ticket that never went to Dev (d0044).
pub fn can_move_to_epic(ticket: &Ticket) -> bool {
    ticket.kind == Kind::Ticket && ticket.epic.is_none() && !ticket.has_started()
}

/// Close epic… is offered while the epic isn't finished.
pub fn can_close_epic(ticket: &Ticket) -> bool {
    ticket.kind == Kind::Epic && !matches!(ticket.status, Status::Merged | Status::ClosedNotMerged)
}

/// An epic the ticket can move into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EpicChoice {
    pub key: TicketKey,
    pub title: String,
    /// "1 / 3 merged".
    pub progress: String,
    /// The key the ticket would get: SHOP-40.4.
    pub new_key: TicketKey,
}

/// The epics of the ticket's project that are still open, in key order.
pub fn epic_choices(tickets: &[Ticket]) -> Vec<EpicChoice> {
    let mut epics: Vec<EpicChoice> = tickets
        .iter()
        .filter(|t| t.kind == Kind::Epic && can_close_epic(t))
        .map(|e| {
            let p = progress_of(&e.key, tickets);
            EpicChoice {
                key: e.key.clone(),
                title: e.title.clone(),
                progress: p.label(),
                new_key: TicketKey::child(
                    e.key.project.clone(),
                    e.key.number,
                    e.next_child_number.unwrap_or(1),
                ),
            }
        })
        .collect();
    epics.sort_by(|a, b| a.key.cmp(&b.key));
    epics
}

/// The Move dialog's texts for a choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovePreview {
    pub title: String,
    /// "· the old key SHOP-51 still works".
    pub alias_note: String,
    /// Shown when the ticket had another base: "SHOP-51 will start from the epic's branch (otaq/SHOP-40)
    /// instead of release/2.3, and merge back into it."
    pub base_note: Option<String>,
    pub confirm: String,
}

pub fn move_preview(ticket: &Ticket, choice: &EpicChoice, tickets: &[Ticket]) -> MovePreview {
    let epic_base = tickets
        .iter()
        .find(|t| t.key == choice.key)
        .map(|e| e.base.clone());
    let base_note = match (&ticket.base, epic_base) {
        (Base::Branch(mine), Some(Base::Branch(theirs))) if *mine != theirs => Some(format!(
            "{} will start from the epic’s branch ({}) instead of {mine}, and merge back into it.",
            ticket.key,
            otaq_core::naming::branch(&choice.key.to_string()),
        )),
        _ => None,
    };
    MovePreview {
        title: format!("Move {} {} to an epic", ticket.key, ticket.title),
        alias_note: format!("· the old key {} still works", ticket.key),
        base_note,
        confirm: format!("Move to {}", choice.key),
    }
}

/// The Close epic confirmation's texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseDialog {
    pub title: String,
    /// One line per running agent that closing stops.
    pub agents: Vec<String>,
    /// "Also closes … SHOP-40.1 Wishlist API is merged and stays merged."
    pub also: Option<String>,
    /// The worktree rule of d0033.
    pub note: &'static str,
    pub confirm: String,
}

pub const CLOSE_NOTE: &str = "Branches are kept. Worktrees are removed unless they have uncommitted changes (those are listed in Cleanup). Closed tickets can be reopened later.";

pub fn close_dialog(epic: &Ticket, plan: &ClosePlan, tickets: &[Ticket]) -> CloseDialog {
    let title_of = |k: &TicketKey| {
        tickets
            .iter()
            .find(|t| &t.key == k)
            .map_or_else(String::new, |t| t.title.clone())
    };
    let agents: Vec<String> = plan
        .close
        .iter()
        .filter(|c| c.agent_running)
        .map(|c| {
            format!(
                "{} {} — its agent is running. Closing stops it; the session is not kept.",
                c.key, c.title
            )
        })
        .collect();
    let quiet: Vec<String> = plan
        .close
        .iter()
        .filter(|c| !c.agent_running)
        .map(|c| {
            format!(
                "{} {} ({})",
                c.key,
                c.title,
                crate::view_models::project_agent_page::column_name(c.status)
            )
        })
        .collect();
    let merged: Vec<String> = plan
        .stay_merged
        .iter()
        .map(|k| format!("{k} {}", title_of(k)))
        .collect();
    let mut also = Vec::new();
    if !quiet.is_empty() {
        also.push(format!("Also closes {}.", quiet.join(", ")));
    }
    if !merged.is_empty() {
        let verb = if merged.len() == 1 { "is" } else { "are" };
        also.push(format!(
            "{} {verb} merged and stays merged.",
            merged.join(", ")
        ));
    }
    let n = plan.close.len();
    let tickets_label = format!("{n} ticket{}", if n == 1 { "" } else { "s" });
    let confirm = match agents.len() {
        0 if n == 0 => "Close epic".to_owned(),
        0 => format!("Close epic & {tickets_label}"),
        a => format!(
            "Stop {a} agent{} & close {tickets_label}",
            if a == 1 { "" } else { "s" }
        ),
    };
    CloseDialog {
        title: format!("Close epic {} {}?", epic.key, epic.title),
        agents,
        also: (!also.is_empty()).then(|| also.join(" ")),
        note: CLOSE_NOTE,
        confirm,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::board::epics::ClosingChild;

    fn key(k: &str) -> TicketKey {
        k.parse().unwrap()
    }

    fn wishlist() -> Vec<Ticket> {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        epic.next_child_number = Some(4);
        let child = |k: &str, title: &str, status| {
            let mut t = ticket(k, title, status);
            t.epic = Some(key("SHOP-40"));
            t
        };
        let mut moving = ticket("SHOP-51", "Wishlist share link", Status::Backlog);
        moving.base = Base::Branch("release/2.3".into());
        vec![
            epic,
            child("SHOP-40.1", "Wishlist API", Status::Merged),
            child("SHOP-40.2", "Wishlist button", Status::Working),
            child("SHOP-40.3", "Wishlist page", Status::Backlog),
            moving,
        ]
    }

    #[test]
    fn menu_visibility() {
        let tickets = wishlist();
        assert!(can_move_to_epic(&tickets[4]));
        assert!(
            !can_move_to_epic(&tickets[3]),
            "a child can't move to another epic"
        );
        assert!(
            !can_move_to_epic(&tickets[0]),
            "an epic can't move into an epic"
        );
        let mut started = tickets[4].clone();
        started.started_at = Some(1);
        assert!(!can_move_to_epic(&started), "only never-started tickets");
        assert!(can_close_epic(&tickets[0]));
        assert!(!can_close_epic(&tickets[4]));
    }

    #[test]
    fn move_dialog_previews_key_alias_and_base() {
        let tickets = wishlist();
        let choices = epic_choices(&tickets);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].progress, "1 of 3 merged");
        assert_eq!(choices[0].new_key, key("SHOP-40.4"));
        let p = move_preview(&tickets[4], &choices[0], &tickets);
        assert_eq!(p.title, "Move SHOP-51 Wishlist share link to an epic");
        assert_eq!(p.alias_note, "· the old key SHOP-51 still works");
        assert_eq!(
            p.base_note.as_deref(),
            Some(
                format!(
                    "SHOP-51 will start from the epic’s branch ({}) instead of release/2.3, and merge back into it.",
                    otaq_core::naming::branch("SHOP-40")
                )
                .as_str()
            )
        );
        assert_eq!(p.confirm, "Move to SHOP-40");
        let mut same_base = tickets[4].clone();
        same_base.base = Base::Branch("main".into());
        assert_eq!(
            move_preview(&same_base, &choices[0], &tickets).base_note,
            None
        );
    }

    #[test]
    fn close_confirmation_names_agents_children_and_merged() {
        let tickets = wishlist();
        let plan = ClosePlan {
            epic: key("SHOP-40"),
            close: vec![
                ClosingChild {
                    key: key("SHOP-40.2"),
                    title: "Wishlist button".into(),
                    status: Status::Working,
                    agent_running: true,
                },
                ClosingChild {
                    key: key("SHOP-40.3"),
                    title: "Wishlist page".into(),
                    status: Status::Backlog,
                    agent_running: false,
                },
            ],
            stay_merged: vec![key("SHOP-40.1")],
        };
        let d = close_dialog(&tickets[0], &plan, &tickets);
        assert_eq!(d.title, "Close epic SHOP-40 Wishlist?");
        assert_eq!(
            d.agents,
            [
                "SHOP-40.2 Wishlist button — its agent is running. Closing stops it; the session is not kept."
            ]
        );
        assert_eq!(
            d.also.as_deref(),
            Some(
                "Also closes SHOP-40.3 Wishlist page (Backlog). SHOP-40.1 Wishlist API is merged and stays merged."
            )
        );
        assert_eq!(d.confirm, "Stop 1 agent & close 2 tickets");
        assert!(
            d.note
                .contains("removed unless they have uncommitted changes")
        );
        let quiet = ClosePlan {
            close: plan.close[1..].to_vec(),
            stay_merged: Vec::new(),
            ..plan
        };
        assert_eq!(
            close_dialog(&tickets[0], &quiet, &tickets).confirm,
            "Close epic & 1 ticket"
        );
    }
}
