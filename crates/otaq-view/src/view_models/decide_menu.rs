//! Decide on a card whose check ran out of rounds: the check failed as many times in a row as its rounds (set per
//! check in Edit flow, 3 by default), and the card waits in the check, turquoise, with "<check> failed 3 times ·
//! decide". Decide offers three ways on, under "VALIDATE FAILED 3 TIMES IN A ROW":
//!
//! - **Allow 1 more round**: back to where the check's fails go, with the checker's notes, once more.
//! - **Send it on to <next stage>**: past the check, without a pass; the next stage sees the checker's last report.
//! - **Stop here**: nothing changes; it waits in the check until the user chooses.
//!
//! and says where the rounds are set ("Rounds for Validate are set in Edit flow (3)."). The first two are the
//! ticket's existing actions (`another_round`, `move_on_anyway`), so the rule stays the core's.

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Place, Status};
use otaq_core::flow::Flow;
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::flow_editor::CHECK_ROUNDS;

/// One way on. `DecideItem` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DecideItem")]
pub struct Item {
    /// The ticket's action it runs (`another_round`, `move_on_anyway`), or `stop` (nothing).
    pub action: &'static str,
    pub label: String,
    pub hint: String,
}

/// The menu. `DecideMenu` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "DecideMenu")]
pub struct Menu {
    pub key: String,
    /// "VALIDATE FAILED 3 TIMES IN A ROW".
    pub title: String,
    pub items: Vec<Item>,
    /// "Rounds for Validate are set in Edit flow (3)."
    pub note: String,
}

/// The way on that changes nothing.
pub const STOP: &str = "stop";

/// The menu for `ticket`, when its check ran out of rounds; `None` otherwise (Decide then opens the ticket).
pub fn menu(ticket: &Ticket, flow: &Flow) -> Option<Menu> {
    if ticket.status != Status::CheckFailed {
        return None;
    }
    let Place::InStage(id) = &ticket.place else {
        return None;
    };
    let at = flow.position(id)?;
    let check = &flow.stages[at];
    let rounds = check.rounds.unwrap_or(CHECK_ROUNDS);
    let times = ticket.validation_round.max(1);
    let checker = check.agent.as_deref().unwrap_or("the checker");
    let back = flow
        .fail_to_of(check)
        .map_or("the stage before", |s| s.name.as_str());
    let next = flow.stages.get(at + 1).map(|s| s.name.as_str());
    let unit = if times == 1 { "TIME" } else { "TIMES" };
    Some(Menu {
        key: ticket.key.to_string(),
        title: format!(
            "{} FAILED {times} {unit} IN A ROW",
            check.name.to_uppercase()
        ),
        items: vec![
            Item {
                action: "another_round",
                label: "Allow 1 more round".to_owned(),
                hint: format!("Back to {back} with {checker}\u{2019}s notes, once more."),
            },
            Item {
                action: "move_on_anyway",
                label: match next {
                    Some(next) => format!("Send it on to {next}"),
                    None => "Send it on".to_owned(),
                },
                hint: format!("You\u{2019}ll see {checker}\u{2019}s last report there."),
            },
            Item {
                action: STOP,
                label: "Stop here".to_owned(),
                hint: format!("It waits in {} until you choose.", check.name),
            },
        ],
        note: format!("Rounds for {} are set in Edit flow ({rounds}).", check.name),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::flow::presets::presets;

    #[test]
    fn a_check_out_of_rounds_offers_three_ways_on() {
        let flow = presets()[0].read().flow;
        let mut ticket = ticket("SHOP-26", "Price table rounding", Status::CheckFailed);
        ticket.place = Place::stage_of("validate");
        ticket.validation_round = 3;
        let m = menu(&ticket, &flow).unwrap();
        assert_eq!(m.title, "VALIDATE FAILED 3 TIMES IN A ROW");
        let labels: Vec<&str> = m.items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            ["Allow 1 more round", "Send it on to Review", "Stop here"]
        );
        assert_eq!(
            m.items[0].hint,
            "Back to Dev with Checker\u{2019}s notes, once more."
        );
        assert_eq!(m.note, "Rounds for Validate are set in Edit flow (3).");
        // Any other card's Decide opens the ticket.
        ticket.status = Status::Checking;
        assert!(menu(&ticket, &flow).is_none());
    }
}
