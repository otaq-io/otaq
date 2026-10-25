//! The card's right-click menu (TKT-06, d0047; *03 · Board & columns → Pinned tickets*): Pin / Unpin for any
//! ticket, then the Priority group (TKT-09, d0065: the four values, the current one checked; a pick is a
//! `ticket.update`). The menu grows with later tickets (Delete… T-0162; sheet, terminal, diff, Send back, Close
//! ticket with their milestones).

use otaq_core::board::tickets::Ticket;
use otaq_core::protocol::{KeyParams, Request};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::menu::MenuItem;
use crate::view_models::priority;

pub const PIN: &str = "pin";
pub const UNPIN: &str = "unpin";

/// The menu for a card.
pub fn items(ticket: &Ticket) -> Vec<MenuItem> {
    let mut items = Vec::new();
    let pin = if ticket.flags.is_pinned() {
        MenuItem::new(UNPIN, "Unpin").icon(IconName::PinOff)
    } else {
        MenuItem::new(PIN, "Pin to top").icon(IconName::Pin)
    };
    items.push(pin);
    // The menu has no submenus: the group sits inline under its label, next to Pin / Unpin.
    items.push(MenuItem::header(priority::MENU_HEADER).separated());
    items.extend(priority::menu_items(ticket.priority));
    items.extend(crate::view_models::close::close_item(ticket));
    items.push(crate::view_models::delete::menu_item(ticket));
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
    /// Not an item of this menu.
    Nothing,
}

/// What choosing `id` does; `Sent` only when it has a request for `ticket`.
pub fn choice(id: &str, ticket: &Ticket) -> Choice {
    match id {
        crate::view_models::delete::DELETE => Choice::Delete,
        crate::view_models::close::CLOSE_TICKET => Choice::CloseTicket,
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
    use crate::view_models::board::{BoardInput, build};
    use crate::view_models::card_fixtures::{NOW, ticket};
    use otaq_core::domain::Status;

    #[test]
    fn the_label_follows_the_pin_and_the_board_resorts() {
        let mut tickets = vec![
            ticket("SHOP-24", "Update footer links", Status::Backlog),
            ticket("SHOP-31", "Add order history page", Status::Backlog),
        ];
        let menu = items(&tickets[1]);
        assert_eq!(menu.len(), 8, "pin, the priority group, close and delete");
        assert_eq!(menu[6].id.as_str(), crate::view_models::close::CLOSE_TICKET);
        assert_eq!(menu[7].id.as_str(), crate::view_models::delete::DELETE);
        assert_eq!(
            (menu[0].id.as_str(), menu[0].label.as_str()),
            (PIN, "Pin to top")
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
                helpers: &[],
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
        let menu = items(&tickets[1]);
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
    }

    /// Every item says what it does on the page; dialogs open by overlay id, the rest send their request.
    #[test]
    fn each_item_has_its_choice() {
        let t = ticket("SHOP-21", "Order confirmation email", Status::Backlog);
        let choices: Vec<Choice> = items(&t)
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
                Choice::CloseTicket,
                Choice::Delete,
            ]
        );
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
        let menu = items(&t);
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
    }
}
