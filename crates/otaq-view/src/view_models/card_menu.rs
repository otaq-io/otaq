//! The card's right-click menu (TKT-06, d0047; *03 · Board & columns → Pinned tickets*): Pin / Unpin for any
//! ticket or epic, then the Priority group (TKT-09, d0065: the four values, the current one checked; a pick is a
//! `ticket.update`); Move to epic… for tickets that never went to Dev and Close epic… on epics (T-0043). The
//! menu grows with later tickets (Delete… T-0162; sheet, terminal, diff, Send back, Close ticket with their
//! milestones).
//!
//! On an epic, its card or its track (EPC-12, EPC-14, EPC-17, EPC-18; d0080; *03 → Note · Interactions (0080) →
//! Right-click*): Open epic · Queue never-started (n) / Unqueue never-started (n) · Add ticket to epic · Focus
//! (g e) · Pin epic / Unpin, then the existing items (the Priority group, Delete…, Close epic…). Queue and Unqueue
//! are the drags they stand for (a drop on Ready for Dev, on Backlog), so they show the queue toast and, when every
//! child started, the Park-all confirmation.

use otaq_core::board::epic_flight::{TakeBackPlan, is_never_started, take_back_plan, to_queue};
use otaq_core::board::tickets::Ticket;
use otaq_core::protocol::{KeyParams, Request};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::view_models::epic_actions::{can_close_epic, can_move_to_epic};
use crate::view_models::menu::MenuItem;
use crate::view_models::priority;

pub const PIN: &str = "pin";
pub const UNPIN: &str = "unpin";
pub const MOVE_TO_EPIC: &str = "move-to-epic";
pub const CLOSE_EPIC: &str = "close-epic";
pub const OPEN_EPIC: &str = "open-epic";
pub const QUEUE_EPIC: &str = "queue-epic";
pub const TAKE_BACK_EPIC: &str = "take-back-epic";
pub const ADD_TO_EPIC: &str = "add-to-epic";
pub const FOCUS_EPIC: &str = "focus-epic";

/// The menu for a card (or a track: its epic). `tickets` is the ticket's project, for an epic's counts; `has_epics`:
/// the project has an open epic to move into.
pub fn items(
    ticket: &Ticket,
    tickets: &[Ticket],
    has_epics: bool,
    keys: &KeyLabels,
) -> Vec<MenuItem> {
    let mut items = if ticket.is_epic() {
        epic_items(ticket, tickets, keys)
    } else {
        Vec::new()
    };
    let pin = if ticket.flags.is_pinned() {
        MenuItem::new(UNPIN, "Unpin").icon(IconName::PinOff)
    } else if ticket.is_epic() {
        // Pinning the epic pins its track and its children's start (d0047, EPC-17).
        MenuItem::new(PIN, "Pin epic").icon(IconName::Pin)
    } else {
        MenuItem::new(PIN, "Pin to top").icon(IconName::Pin)
    };
    items.push(if ticket.is_epic() {
        pin.separated()
    } else {
        pin
    });
    // The menu has no submenus: the group sits inline under its label, next to Pin / Unpin.
    items.push(MenuItem::header(priority::MENU_HEADER).separated());
    items.extend(priority::menu_items(ticket.priority));
    if has_epics && can_move_to_epic(ticket) {
        items.push(
            MenuItem::new(MOVE_TO_EPIC, "Move to epic…")
                .icon(IconName::Layers)
                .separated(),
        );
    }
    items.extend(crate::view_models::close::close_item(ticket));
    items.push(crate::view_models::delete::menu_item(ticket));
    if can_close_epic(ticket) {
        items.push(
            MenuItem::new(CLOSE_EPIC, "Close epic…")
                .icon(IconName::CircleSlash)
                .danger()
                .separated(),
        );
    }
    items
}

/// An epic's own items, above Pin: Open epic, Queue / Unqueue never-started (n), Add ticket to epic, Focus.
fn epic_items(epic: &Ticket, tickets: &[Ticket], keys: &KeyLabels) -> Vec<MenuItem> {
    let mut items = vec![MenuItem::new(OPEN_EPIC, "Open epic").icon(IconName::PanelRight)];
    let open = can_close_epic(epic);
    // What a drop on Ready for Dev moves: never-started children are queued, parked ones (and reopened ones) resume.
    let waiting = to_queue(epic, tickets);
    let fresh = waiting.iter().filter(|c| is_never_started(c)).count();
    let resumed = waiting.len() - fresh;
    let queue = match (fresh, resumed) {
        (0, 0) => None,
        (n, 0) => Some(format!("Queue never-started ({n})")),
        (0, n) => Some(format!("Resume parked ({n})")),
        (n, r) => Some(format!("Queue never-started ({n}) · resume {r}")),
    };
    if let Some(label) = queue.filter(|_| open) {
        items.push(MenuItem::new(QUEUE_EPIC, label).icon(IconName::Play));
    }
    match take_back_plan(epic, tickets) {
        // A waiting child that worked before (resumed, reopened) goes back too (Q58): then it says "queued".
        TakeBackPlan::Unqueue {
            unqueue,
            never_started,
            ..
        } => {
            let what = if never_started.len() == unqueue.len() {
                "never-started"
            } else {
                "queued"
            };
            items.push(
                MenuItem::new(
                    TAKE_BACK_EPIC,
                    format!("Unqueue {what} ({})", unqueue.len()),
                )
                .icon(IconName::Undo2),
            );
        }
        // Nothing waits and every started child runs: the take-back is Park all, after its confirmation.
        TakeBackPlan::ParkAll { park, .. } => items.push(
            MenuItem::new(TAKE_BACK_EPIC, format!("Park running ({})…", park.len()))
                .icon(IconName::SquareParking),
        ),
        TakeBackPlan::Refused { .. } => {}
    }
    if open {
        items.push(MenuItem::new(ADD_TO_EPIC, "Add ticket to epic").icon(IconName::Plus));
    }
    let mut focus = MenuItem::new(FOCUS_EPIC, "Focus").icon(IconName::ScanSearch);
    if let Some(keys) = keys.label(KeyAction::FocusEpic) {
        focus = focus.shortcut(keys);
    }
    items.push(focus);
    items
}

/// What a chosen item does on the page (T-0221): most send their request at once; the rest open a dialog by
/// overlay id, so the page never compares item ids itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "CardMenuChoice")]
pub enum Choice {
    /// The item's request was sent (Pin, Unpin, a priority).
    Sent,
    /// The delete confirmation (`delete_ticket`).
    Delete,
    /// The Close ticket confirmation (`close_ticket`).
    CloseTicket,
    /// Move to epic… (the epic dialogs, T-0223).
    MoveToEpic,
    /// Close epic… (the epic dialogs, T-0223).
    CloseEpic,
    /// Open epic: the epic sheet, as a double click on its track's name.
    OpenEpic,
    /// Queue never-started (n): the epic dropped on Ready for Dev (EPC-12), with its queue toast.
    QueueEpic,
    /// Unqueue never-started (n), or Park running (n)…: the epic's track dropped on Backlog (EPC-14), with the
    /// Park-all confirmation when it parks.
    TakeBackEpic,
    /// Add ticket to epic: the New ticket dialog under the epic.
    AddToEpic,
    /// Focus: the epic's filter (EPC-18), as g e.
    FocusEpic,
    /// Not an item of this menu.
    Nothing,
}

/// What choosing `id` does; `Sent` only when it has a request for `ticket`.
pub fn choice(id: &str, ticket: &Ticket) -> Choice {
    match id {
        crate::view_models::delete::DELETE => Choice::Delete,
        crate::view_models::close::CLOSE_TICKET => Choice::CloseTicket,
        MOVE_TO_EPIC => Choice::MoveToEpic,
        CLOSE_EPIC => Choice::CloseEpic,
        OPEN_EPIC if ticket.is_epic() => Choice::OpenEpic,
        QUEUE_EPIC if ticket.is_epic() => Choice::QueueEpic,
        TAKE_BACK_EPIC if ticket.is_epic() => Choice::TakeBackEpic,
        ADD_TO_EPIC if ticket.is_epic() => Choice::AddToEpic,
        FOCUS_EPIC if ticket.is_epic() => Choice::FocusEpic,
        _ if request(id, ticket).is_some() => Choice::Sent,
        _ => Choice::Nothing,
    }
}

/// The service request a chosen item sends.
pub fn request(choice: &str, ticket: &Ticket) -> Option<Request> {
    let key = KeyParams {
        key: ticket.key.to_string(),
    };
    match choice {
        PIN => Some(Request::TicketPin(key)),
        UNPIN => Some(Request::TicketUnpin(key)),
        _ => priority::from_menu_id(choice).map(|p| priority::request(&ticket.key, p)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_labels::fixtures;
    use crate::keymap::Platform;
    use crate::view_models::board::{BoardInput, build};
    use crate::view_models::card_fixtures::{NOW, ticket};
    use otaq_core::domain::Status;

    #[test]
    fn the_label_follows_the_pin_and_the_board_resorts() {
        let mut tickets = vec![
            ticket("SHOP-24", "Update footer links", Status::Backlog),
            ticket("SHOP-31", "Add order history page", Status::Backlog),
        ];
        let menu = items(&tickets[1], &[], false, &fixtures::base());
        assert_eq!(
            menu.len(),
            8,
            "pin, the priority group, close and delete; no epic to move into"
        );
        assert_eq!(menu[6].id.as_str(), crate::view_models::close::CLOSE_TICKET);
        assert_eq!(menu[7].id.as_str(), crate::view_models::delete::DELETE);
        assert_eq!(
            (menu[0].id.as_str(), menu[0].label.as_str()),
            (PIN, "Pin to top")
        );
        assert_eq!(
            items(&tickets[1], &[], true, &fixtures::base())[6]
                .id
                .as_str(),
            MOVE_TO_EPIC
        );
        assert_eq!(
            request(&menu[0].id, &tickets[1]),
            Some(Request::TicketPin(KeyParams {
                key: "SHOP-31".into()
            }))
        );
        let order = |tickets: &[Ticket]| -> Vec<String> {
            build(&BoardInput {
                flow: &otaq_core::flow::default_file().flow,
                tickets,
                visible: None,
                strong: None,
                project_base: None,
                now_ms: NOW,
                dev_budget: None,
                folded: &[],
                fold_empty: false,
                agent_names: &[],
                was_validating: &[],
                agents: None,
                agent_list: None,
                starts_held: None,
                paused: false,
            })[0]
                .cards
                .iter()
                .map(|c| c.key().to_string())
                .collect()
        };
        assert_eq!(order(&tickets), ["SHOP-24", "SHOP-31"]);
        // The service pinned it; the event brings the ticket back with pinned_at set.
        tickets[1].flags.pinned_at = Some(1);
        assert_eq!(order(&tickets), ["SHOP-31", "SHOP-24"]);
        let menu = items(&tickets[1], &[], false, &fixtures::base());
        assert_eq!(
            (menu[0].id.as_str(), menu[0].label.as_str()),
            (UNPIN, "Unpin")
        );
        assert_eq!(
            request(UNPIN, &tickets[1]),
            Some(Request::TicketUnpin(KeyParams {
                key: "SHOP-31".into()
            }))
        );
        assert_eq!(request("nothing", &tickets[1]), None);
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = otaq_core::domain::Kind::Epic;
        let epic_menu = items(&epic, &[], true, &fixtures::base());
        assert_eq!(
            epic_menu.len(),
            11,
            "open, add, focus, pin, the priority group, delete and close; no move for an epic"
        );
        assert!(epic_menu[10].danger && epic_menu[10].id.as_str() == CLOSE_EPIC);
    }

    /// Every item says what it does on the page; dialogs open by overlay id, the rest send their request.
    #[test]
    fn each_item_has_its_choice() {
        let t = ticket("SHOP-21", "Order confirmation email", Status::Backlog);
        let choices: Vec<Choice> = items(&t, &[], true, &fixtures::base())
            .iter()
            .filter(|i| !i.header)
            .map(|i| choice(&i.id, &t))
            .collect();
        assert_eq!(
            choices,
            [
                Choice::Sent,
                Choice::Sent,
                Choice::Sent,
                Choice::Sent,
                Choice::Sent,
                Choice::MoveToEpic,
                Choice::CloseTicket,
                Choice::Delete,
            ]
        );
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = otaq_core::domain::Kind::Epic;
        assert_eq!(choice(CLOSE_EPIC, &epic), Choice::CloseEpic);
        assert_eq!(choice("nothing", &t), Choice::Nothing);
        assert_eq!(
            serde_json::to_value(Choice::CloseTicket).unwrap()["kind"],
            "close_ticket"
        );
    }

    /// The Priority group next to Pin / Unpin: its label, the four values with the current one checked, and a
    /// pick sends `ticket.update` with the priority only.
    #[test]
    fn the_priority_group_sets_the_priority() {
        use otaq_core::board::tickets::TicketUpdate;
        use otaq_core::domain::Priority;
        use otaq_core::protocol::TicketUpdateParams;

        let mut t = ticket("SHOP-35", "Slow checkout query", Status::Queued);
        t.priority = Priority::High;
        let menu = items(&t, &[], false, &fixtures::base());
        assert!(menu[1].header && menu[1].label == priority::MENU_HEADER);
        let group: Vec<_> = menu[2..6]
            .iter()
            .map(|i| (i.label.as_str(), i.checked))
            .collect();
        assert_eq!(
            group,
            [
                ("P1 · Urgent", false),
                ("P2 · High", true),
                ("P3 · Normal", false),
                ("P4 · Low", false)
            ]
        );
        assert_eq!(
            request(&menu[2].id, &t),
            Some(Request::TicketUpdate(TicketUpdateParams {
                key: "SHOP-35".into(),
                update: TicketUpdate {
                    priority: Some(Priority::Urgent),
                    ..TicketUpdate::default()
                },
            }))
        );
        // An epic child following its epic's P2 has P2 checked; picking it gives the child its own.
        let mut child = ticket("SHOP-40.2", "Wishlist button", Status::Backlog);
        child.priority = Priority::High;
        child.priority_inherited = true;
        assert!(items(&child, &[], false, &fixtures::base())[3].checked);
        assert!(matches!(
            request(&priority::menu_id(Priority::High), &child),
            Some(Request::TicketUpdate(p)) if p.update.priority == Some(Priority::High)
        ));
    }

    /// EPC-12, EPC-14, EPC-17, EPC-18 (*Note · Interactions → Right-click*): the epic's items with their counts, on
    /// its Backlog card, on its track, and pinned.
    #[test]
    fn the_epic_menu_counts_what_it_queues_and_takes_back() {
        use otaq_core::domain::Kind;
        let wishlist = |children: [Status; 3]| -> Vec<Ticket> {
            let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
            epic.kind = Kind::Epic;
            let mut all = vec![epic];
            for (i, status) in children.into_iter().enumerate() {
                let mut c = ticket(&format!("SHOP-40.{}", i + 1), "Child", status);
                c.epic = Some("SHOP-40".parse().unwrap());
                all.push(c);
            }
            all
        };
        let labels = |all: &[Ticket]| -> Vec<String> {
            items(&all[0], all, false, &fixtures::base())
                .iter()
                .take_while(|i| !i.header)
                .map(|i| i.label.clone())
                .collect()
        };
        // The card: nothing started.
        let card = wishlist([Status::Backlog; 3]);
        assert_eq!(
            labels(&card),
            [
                "Open epic",
                "Queue never-started (3)",
                "Add ticket to epic",
                "Focus",
                "Pin epic"
            ]
        );
        let focus = &items(&card[0], &card, false, &fixtures::base())[3];
        assert_eq!(focus.shortcut.as_deref(), Some("g e"));
        assert_eq!(
            items(&card[0], &card, false, &KeyLabels::base(Platform::WinLinux))[3]
                .shortcut
                .as_deref(),
            Some("g e")
        );
        // KEY-08: the user's key for Focus, none once removed or with letters off.
        let focus_with = |keys: &KeyLabels| items(&card[0], &card, false, keys)[3].shortcut.clone();
        assert_eq!(
            focus_with(&fixtures::replaced("focus_epic", None, 0, "g f")).as_deref(),
            Some("g f")
        );
        assert_eq!(focus_with(&fixtures::removed("focus_epic", None, 0)), None);
        assert_eq!(focus_with(&fixtures::letters_off()), None);
        // The track: 40.1 runs, 40.2 waits, 40.3 not queued.
        let mut track = wishlist([Status::Working, Status::Queued, Status::Backlog]);
        assert!(track[2].place.is_ready_for_dev());
        assert_eq!(
            labels(&track),
            [
                "Open epic",
                "Queue never-started (1)",
                "Unqueue never-started (1)",
                "Add ticket to epic",
                "Focus",
                "Pin epic"
            ]
        );
        // 40.3 parked (it worked before): it resumes rather than queues.
        track[3].status = Status::Parked;
        assert_eq!(labels(&track)[1], "Resume parked (1)");
        // Pinned, everything started and running: Park running, and Unpin.
        track[0].flags.pinned_at = Some(1);
        for c in &mut track[1..] {
            c.status = Status::Working;
            c.place = otaq_core::domain::Place::stage_of("dev");
        }
        assert_eq!(
            labels(&track),
            [
                "Open epic",
                "Park running (3)…",
                "Add ticket to epic",
                "Focus",
                "Unpin"
            ]
        );
        let menu = items(&track[0], &track, false, &fixtures::base());
        let chosen: Vec<Choice> = menu
            .iter()
            .take(4)
            .map(|i| choice(&i.id, &track[0]))
            .collect();
        assert_eq!(
            chosen,
            [
                Choice::OpenEpic,
                Choice::TakeBackEpic,
                Choice::AddToEpic,
                Choice::FocusEpic
            ]
        );
        assert_eq!(choice(QUEUE_EPIC, &track[0]), Choice::QueueEpic);
        assert_eq!(
            choice(QUEUE_EPIC, &track[1]),
            Choice::Nothing,
            "not on a child"
        );
    }
}
