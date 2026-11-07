//! A project's board with no tickets yet: its columns, as on any day. A new user's first project also shows "Your
//! first ticket" waiting in Backlog: a tiny task that works on any repository and is safe to merge, prefilled and
//! editable in place, with the time and whose plan it uses said plainly. Create & start creates and starts it (and asks
//! for notifications, once); Not now leaves an empty Backlog with its plain Add ticket row. Either answer is for good:
//! the card is the one-time [`Hint::FirstTicket`]. A project added later, or after Not now, is just its columns.

use otaq_core::agent_tools::Tool;
use otaq_core::agents::{AgentList, AgentModel};
use otaq_core::flow::Flow;
use otaq_core::store::settings::Hint;
use serde::Serialize;
use ts_rs::TS;

use super::tool_problems::tool_word;

/// The first ticket's title, as prefilled.
pub const TITLE: &str = "Tidy the README";

/// The first ticket's description, as prefilled.
pub const BODY: &str = "Fix typos and broken links. Change nothing else.";

/// The line under the fields.
pub const TIP: &str = "Small, clear tasks work best.";

/// "Your first ticket" in Backlog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "FirstTicketView")]
pub struct FirstTicket {
    /// "Your first ticket", the card's label.
    pub label: String,
    /// "Not started".
    pub status: String,
    /// The title as prefilled; the user edits it in place.
    pub title: String,
    pub title_label: String,
    /// The description as prefilled.
    pub body: String,
    pub body_label: String,
    /// "Small, clear tasks work best."
    pub tip: String,
    /// "Takes about 5 minutes and uses your Claude Code plan."
    pub cost: String,
    /// Create & start, the primary button.
    pub start: String,
    pub starting: String,
    pub not_now: String,
    /// A title is needed: what Create & start says without one.
    pub no_title: String,
}

/// Whether the first ticket shows: the project has no tickets and the user never answered the card.
pub fn shows(tickets: usize, seen: &[Hint]) -> bool {
    tickets == 0 && !seen.contains(&Hint::FirstTicket)
}

/// The tool of the flow's first Agent stage: its agent's own, else the default model's. `None` when neither is known.
pub fn first_stage_tool(flow: &Flow, agents: Option<&AgentList>) -> Option<Tool> {
    let agents = agents?;
    let name = flow.first_agent_stage()?.named_agents().next()?;
    let agent = agents
        .agents
        .iter()
        .find(|a| a.name.eq_ignore_ascii_case(name))?;
    match &agent.model {
        AgentModel::Tool { tool, .. } => Some(*tool),
        AgentModel::Default => agents.default_model.as_ref().map(|m| m.tool),
    }
}

/// The card, saying whose plan it uses: the first stage's `tool`.
pub fn first_ticket(tool: Option<Tool>) -> FirstTicket {
    let plan = tool.map_or_else(
        || "your agent tool\u{2019}s plan".to_owned(),
        |t| format!("your {} plan", tool_word(t)),
    );
    FirstTicket {
        label: "Your first ticket".to_owned(),
        status: "Not started".to_owned(),
        title: TITLE.to_owned(),
        title_label: "Title".to_owned(),
        body: BODY.to_owned(),
        body_label: "Description".to_owned(),
        tip: TIP.to_owned(),
        cost: format!("Takes about 5 minutes and uses {plan}."),
        start: "Create & start".to_owned(),
        starting: "Starting\u{2026}".to_owned(),
        not_now: "Not now".to_owned(),
        no_title: "Give it a title first.".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_ticket_shows_on_an_empty_board_until_answered() {
        assert!(shows(0, &[]));
        assert!(!shows(1, &[]), "a project with tickets");
        assert!(!shows(0, &[Hint::FirstTicket]), "answered: never again");
    }

    #[test]
    fn the_card_is_prefilled_and_says_whose_plan_it_uses() {
        let card = first_ticket(Some(Tool::Claude));
        assert_eq!(card.title, "Tidy the README");
        assert_eq!(
            card.body,
            "Fix typos and broken links. Change nothing else."
        );
        assert_eq!(card.tip, "Small, clear tasks work best.");
        assert_eq!(
            card.cost,
            "Takes about 5 minutes and uses your Claude Code plan."
        );
        assert_eq!(
            (card.start.as_str(), card.not_now.as_str()),
            ("Create & start", "Not now")
        );
        assert_eq!(
            first_ticket(Some(Tool::Copilot)).cost,
            "Takes about 5 minutes and uses your Copilot plan."
        );
        assert_eq!(
            first_ticket(None).cost,
            "Takes about 5 minutes and uses your agent tool\u{2019}s plan."
        );
    }
}
