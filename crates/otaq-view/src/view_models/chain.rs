//! What waits for a hovered or selected card: hovering a card that other tickets wait for keeps it and them, directly
//! or through others, at full strength and dims everything else; its card gains one line, "SHOP-30 waits for it"
//! ([`crate::view_models::dependencies::Blocks`]). Arrows join them, parent → child. A card nothing waits for lights
//! nothing. No graph view, and no always-on drawing of every link.

use std::collections::HashSet;

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::TicketKey;
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::dependencies::{self, Tip};
use crate::view_models::flow_columns::Columns;

pub use otaq_core::board::stack::{Chain, chain};

/// One arrow, parent → child.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ChainEdge")]
pub struct Edge {
    pub parent: String,
    pub child: String,
}

/// What waits for a card, as the board draws it while the card is hovered or selected. Sent with the board for every
/// card others wait for up front, so a hover never waits on a round trip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ChainView")]
pub struct View {
    /// The hovered or selected card.
    pub key: String,
    /// The card itself and every ticket waiting for it, directly or through others, in key order.
    pub members: Vec<String>,
    pub edges: Vec<Edge>,
    /// A tooltip under the hovered card; none: the card's own line says what waits for it.
    pub tip: Option<Tip>,
}

/// What waits for each ticket in `tickets` that others wait for (the rest light nothing).
pub fn views(_columns: &Columns, tickets: &[Ticket]) -> Vec<View> {
    tickets
        .iter()
        .filter_map(|t| waiting_on(t, tickets))
        .collect()
}

/// `ticket` and the open tickets waiting for it, transitively, with the links between them; `None` when nothing
/// waits for it.
fn waiting_on(ticket: &Ticket, tickets: &[Ticket]) -> Option<View> {
    let mut members: Vec<&TicketKey> = vec![&ticket.key];
    let mut seen: HashSet<&TicketKey> = HashSet::from([&ticket.key]);
    let mut stack = vec![ticket];
    let mut edges = Vec::new();
    while let Some(at) = stack.pop() {
        for child in dependencies::waiting_children(at, tickets) {
            edges.push(Edge {
                parent: at.key.to_string(),
                child: child.key.to_string(),
            });
            if seen.insert(&child.key) {
                members.push(&child.key);
                stack.push(child);
            }
        }
    }
    if members.len() < 2 {
        return None;
    }
    members.sort();
    edges.sort_by(|a, b| (&a.parent, &a.child).cmp(&(&b.parent, &b.child)));
    edges.dedup();
    Some(View {
        key: ticket.key.to_string(),
        members: members.iter().map(ToString::to_string).collect(),
        edges,
        tip: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::board::tickets::Ticket;
    use otaq_core::domain::Status;
    use otaq_core::domain::TicketKey;

    fn sample() -> Vec<Ticket> {
        let with = |key: &str, deps: &[&str]| {
            let mut t = ticket(key, key, Status::Backlog);
            t.depends_on = deps.iter().map(|d| d.parse().unwrap()).collect();
            t
        };
        vec![
            with("SHOP-17", &[]),
            with("SHOP-18", &["SHOP-17"]),
            with("SHOP-19", &[]),
            with("SHOP-20", &["SHOP-18", "SHOP-19"]),
            with("SHOP-21", &["SHOP-20"]),
            with("SHOP-22", &[]),
            with("SHOP-23", &["SHOP-19"]),
        ]
    }

    fn key(k: &str) -> TicketKey {
        k.parse().unwrap()
    }

    #[test]
    fn hovering_shop_20_lights_its_whole_chain() {
        let tickets = sample();
        let chain = chain(&key("SHOP-20"), &tickets).unwrap();
        let mut members: Vec<String> = chain.members.iter().map(ToString::to_string).collect();
        members.sort();
        assert_eq!(
            members,
            ["SHOP-17", "SHOP-18", "SHOP-19", "SHOP-20", "SHOP-21"]
        );
        assert!(!chain.members.contains(&key("SHOP-22")), "dimmed");
        assert!(
            !chain.members.contains(&key("SHOP-23")),
            "a sibling through SHOP-19 isn't in SHOP-20's chain"
        );
        let edges: Vec<String> = chain
            .edges
            .iter()
            .map(|(p, c)| format!("{p}→{c}"))
            .collect();
        assert_eq!(
            edges,
            [
                "SHOP-17→SHOP-18",
                "SHOP-18→SHOP-20",
                "SHOP-19→SHOP-20",
                "SHOP-20→SHOP-21"
            ]
        );
    }

    #[test]
    fn a_ticket_without_links_highlights_nothing() {
        assert_eq!(chain(&key("SHOP-22"), &sample()), None);
        let from_root = chain(&key("SHOP-17"), &sample()).unwrap();
        assert!(
            from_root.members.contains(&key("SHOP-21")),
            "children down the whole way"
        );
        assert!(
            !from_root.members.contains(&key("SHOP-19")),
            "not a co-parent"
        );
    }

    #[test]
    fn hovering_a_card_lights_only_what_waits_for_it() {
        let views = views(
            &Columns::of(&otaq_core::flow::default_file().flow),
            &sample(),
        );
        let keys: Vec<&str> = views.iter().map(|v| v.key.as_str()).collect();
        assert_eq!(
            keys,
            ["SHOP-17", "SHOP-18", "SHOP-19", "SHOP-20"],
            "only cards others wait for light anything"
        );
        let shop_18 = &views[1];
        assert_eq!(shop_18.members, ["SHOP-18", "SHOP-20", "SHOP-21"]);
        assert_eq!(
            shop_18.edges,
            [
                Edge {
                    parent: "SHOP-18".into(),
                    child: "SHOP-20".into()
                },
                Edge {
                    parent: "SHOP-20".into(),
                    child: "SHOP-21".into()
                }
            ]
        );
        assert_eq!(shop_18.tip, None, "the card's own line says it");
        let shop_19 = &views[2];
        assert_eq!(
            shop_19.members,
            ["SHOP-19", "SHOP-20", "SHOP-21", "SHOP-23"]
        );
    }
}
