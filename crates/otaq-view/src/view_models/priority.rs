//! Ticket priorities in the app (TKT-09, d0065; *01 · Components → Primitives → Priority*, *01 · Foundations →
//! Status Legend → 5 · Priority*). Not a status: an ink-only scale, never turquoise or red, and never colour alone
//! (an icon and the number always go with it). Icon + number on cards, where Normal (P3) is hidden; number + word
//! in the sheet and forms. Every change is a `ticket.update` with `priority`, wherever it is made.

use otaq_core::board::tickets::TicketUpdate;
use otaq_core::domain::{Priority, TicketKey};
use otaq_core::protocol::{Request, TicketUpdateParams};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::menu::MenuItem;

/// The form's hint (*08 · Ticket editor → Full form · filled → Priority field*).
pub const FORM_HINT: &str = "3 Normal by default · decides start order among ready tickets";

/// The menus' group label next to Pin / Unpin.
pub const MENU_HEADER: &str = "PRIORITY";

const MENU_PREFIX: &str = "priority-";

/// How a chip is inked (*5 · Priority*): P1 inverted, P2 outlined in full ink, P3 and P4 muted. `PriorityInk` on
/// the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "PriorityInk")]
pub enum Ink {
    /// Filled with `text-primary`, icon and number in `bg-card`.
    Inverted,
    /// `border-strong` outline, `text-primary` icon and number.
    Strong,
    /// `border` outline, `text-muted` icon and number.
    Muted,
}

pub fn ink(priority: Priority) -> Ink {
    match priority {
        Priority::Urgent => Ink::Inverted,
        Priority::High => Ink::Strong,
        Priority::Normal | Priority::Low => Ink::Muted,
    }
}

pub fn icon(priority: Priority) -> IconName {
    match priority {
        Priority::Urgent => IconName::ChevronsUp,
        Priority::High => IconName::ChevronUp,
        Priority::Normal => IconName::Equal,
        Priority::Low => IconName::ChevronDown,
    }
}

/// "P2".
pub fn short(priority: Priority) -> String {
    format!("P{}", priority.number())
}

/// The chip a card shows after its key: P1, P2 and P4; nothing for Normal.
pub fn on_card(priority: Priority) -> Option<Priority> {
    (priority != Priority::Normal).then_some(priority)
}

/// A priority as the page draws it: the chip (icon, "P2", ink) and the word next to it ("High"); `label` is
/// "P2 · High" for menus and forms. `PriorityView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct PriorityView {
    pub number: u8,
    pub short: String,
    pub word: String,
    pub label: String,
    pub icon: IconName,
    pub ink: Ink,
}

pub fn view(priority: Priority) -> PriorityView {
    PriorityView {
        number: priority.number(),
        short: short(priority),
        word: priority.label().to_owned(),
        label: priority.to_string(),
        icon: icon(priority),
        ink: ink(priority),
    }
}

/// The id a menu item carries ("priority-2").
pub fn menu_id(priority: Priority) -> String {
    format!("{MENU_PREFIX}{}", priority.number())
}

/// The priority a menu item stands for.
pub fn from_menu_id(id: &str) -> Option<Priority> {
    id.strip_prefix(MENU_PREFIX)?.parse().ok()
}

/// The four values, most urgent first, the current one checked: "P1 · Urgent" with the chip's icon.
pub fn menu_items(current: Priority) -> Vec<MenuItem> {
    Priority::ALL
        .into_iter()
        .map(|p| {
            MenuItem::new(menu_id(p), p.to_string())
                .icon(icon(p))
                .checked(p == current)
        })
        .collect()
}

/// The request that sets `key`'s priority.
pub fn request(key: &TicketKey, priority: Priority) -> Request {
    Request::TicketUpdate(TicketUpdateParams {
        key: key.to_string(),
        update: TicketUpdate {
            priority: Some(priority),
            ..TicketUpdate::default()
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cards_hide_normal_only() {
        let shown: Vec<_> = Priority::ALL.into_iter().filter_map(on_card).collect();
        assert_eq!(shown, [Priority::Urgent, Priority::High, Priority::Low]);
        assert_eq!(on_card(Priority::Normal), None);
    }

    #[test]
    fn ink_icon_and_label_per_priority() {
        let rows: Vec<_> = Priority::ALL
            .into_iter()
            .map(|p| (ink(p), icon(p), short(p)))
            .collect();
        assert_eq!(
            rows,
            [
                (Ink::Inverted, IconName::ChevronsUp, "P1".to_owned()),
                (Ink::Strong, IconName::ChevronUp, "P2".to_owned()),
                (Ink::Muted, IconName::Equal, "P3".to_owned()),
                (Ink::Muted, IconName::ChevronDown, "P4".to_owned()),
            ]
        );
    }

    #[test]
    fn the_page_gets_chip_and_word() {
        assert_eq!(
            view(Priority::Urgent),
            PriorityView {
                number: 1,
                short: "P1".into(),
                word: "Urgent".into(),
                label: "P1 · Urgent".into(),
                icon: IconName::ChevronsUp,
                ink: Ink::Inverted,
            }
        );
        assert_eq!(
            serde_json::to_value(view(Priority::Low)).unwrap()["ink"],
            "muted"
        );
    }

    #[test]
    fn menu_lists_four_values_with_the_current_one_checked() {
        let items = menu_items(Priority::High);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            ["P1 · Urgent", "P2 · High", "P3 · Normal", "P4 · Low"]
        );
        let checked: Vec<_> = items.iter().map(|i| i.checked).collect();
        assert_eq!(checked, [false, true, false, false]);
        for (item, p) in items.iter().zip(Priority::ALL) {
            assert_eq!(from_menu_id(&item.id), Some(p));
            assert_eq!(item.icon, Some(icon(p)));
        }
        assert_eq!(from_menu_id("pin"), None);
        assert_eq!(from_menu_id("priority-9"), None);
    }

    #[test]
    fn a_change_is_a_ticket_update_with_the_priority_only() {
        let key: TicketKey = "SHOP-42".parse().unwrap();
        let Request::TicketUpdate(params) = request(&key, Priority::Low) else {
            panic!("not an update");
        };
        assert_eq!(params.key, "SHOP-42");
        assert_eq!(
            params.update,
            TicketUpdate {
                priority: Some(Priority::Low),
                ..TicketUpdate::default()
            }
        );
    }
}
