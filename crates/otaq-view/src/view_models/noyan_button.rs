//! The rail's Noyan button (T-0458; SHB-02, SHB-03; d0113, d0077; *24 → Row · The Project agent* `s24-05`): the bot
//! at the rail's foot opens or closes Noyan's panel (⌘L). It is tinted while the panel is open, and has a dot while a
//! conversation has a reply the user hasn't seen, or asks them something. Typing to Noyan starts from the box for new
//! work (`new_work.rs`, ⇧↵); continuing a conversation is the panel's.
//!
//! **A reply waiting** is per conversation: its last turn ended ([`ProjectAgent::turn_ended_at`]) after the user last
//! looked at it in the panel ([`Looked`]), or it waits for the user (a permission prompt or a question, d0077). The
//! conversation the open panel shows never waits: the user is looking at it.

use std::collections::HashMap;

use otaq_core::project_agents::{AgentState, ProjectAgent};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::view_models::agent_panel::agent_waits;

/// The rail's Noyan button (SHB-02, `s24-05`): the bot icon, tinted while the panel is open, a dot while a reply
/// waits; "Noyan" with "Open or close · ⌘L" on hover ("Asks you · Open or close · ⌘L" while a conversation asks you,
/// OTQ-9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct NoyanButton {
    pub icon: IconName,
    pub label: &'static str,
    /// The hover's line under the name: "Open or close · ⌘L" (without a key: "Open or close").
    pub hint: String,
    pub tinted: bool,
    pub dot: bool,
    /// What the dot says, for the hover and screen readers: "Asks you", "New reply"; `None` without a dot.
    pub dot_label: Option<&'static str>,
}

/// When the user last looked at each conversation in the panel, kept by the app for its life. A conversation it
/// hasn't seen counts from `since` (the app's start), so a restart of the app doesn't call every old turn new.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Looked {
    since: i64,
    at: HashMap<String, i64>,
}

impl Looked {
    pub fn new(since: i64) -> Self {
        Self {
            since,
            at: HashMap::new(),
        }
    }

    /// The panel shows `agent` now.
    pub fn look(&mut self, agent: &str, now: i64) {
        self.at.insert(agent.to_owned(), now);
    }

    /// When the user last looked at `agent`.
    pub fn last(&self, agent: &str) -> i64 {
        self.at.get(agent).copied().unwrap_or(self.since)
    }
}

/// What [`view`] reads.
pub struct Facts<'a> {
    /// The project's open Project agents.
    pub agents: &'a [ProjectAgent],
    pub looked: &'a Looked,
    /// The panel is open.
    pub open: bool,
    /// The conversation the panel shows, if any.
    pub shown: Option<&'a str>,
    pub keys: &'a KeyLabels,
}

/// Whether `agent` has a reply the user hasn't seen: its turn ended after they last looked, or it waits for them;
/// never the conversation the open panel shows.
pub fn reply_waits(agent: &ProjectAgent, looked: &Looked, open_on: Option<&str>) -> bool {
    if open_on == Some(agent.id.as_str()) {
        return false;
    }
    agent_waits(agent)
        || agent
            .turn_ended_at
            .is_some_and(|ended| ended > looked.last(&agent.id))
}

/// "New reply", or "Asks you" while the conversation waits for the answer to its question.
pub const NEW_REPLY: &str = "New reply";
pub const ASKS_YOU: &str = "Asks you";

/// What a waiting conversation's reply says: "Asks you" while its question is open, else "New reply".
pub fn reply_label(agent: &ProjectAgent) -> &'static str {
    if agent.state == AgentState::Running && agent.question.is_some() {
        ASKS_YOU
    } else {
        NEW_REPLY
    }
}

/// The button's look; `dot` is what its dot says ("Asks you", "New reply"), `None` without one.
pub fn button(open: bool, dot: Option<&'static str>, keys: &KeyLabels) -> NoyanButton {
    let hint = match keys.label(KeyAction::FocusAgentPanel) {
        Some(keys) => format!("Open or close \u{b7} {keys}"),
        None => "Open or close".to_owned(),
    };
    NoyanButton {
        icon: IconName::Bot,
        label: "Noyan",
        hint: match dot {
            Some(ASKS_YOU) => format!("{ASKS_YOU} \u{b7} {hint}"),
            _ => hint,
        },
        tinted: open,
        dot: dot.is_some(),
        dot_label: dot,
    }
}

/// The button for a project: a question anywhere says "Asks you" before any reply.
pub fn view(f: &Facts) -> NoyanButton {
    let open_on = f.shown.filter(|_| f.open);
    let waiting = f
        .agents
        .iter()
        .filter(|a| reply_waits(a, f.looked, open_on))
        .map(reply_label);
    let dot = waiting.max_by_key(|label| *label == ASKS_YOU);
    button(f.open, dot, f.keys)
}

/// Project agents for the tests.
#[cfg(test)]
pub(crate) mod fixtures {
    use otaq_core::agent_tools::Tool;
    use otaq_core::project_agents::{AgentState, NameSource, ProjectAgent};

    pub fn agent(id: &str, name: &str, created_at: i64) -> ProjectAgent {
        ProjectAgent {
            id: id.into(),
            project: "SHOP".into(),
            name: name.into(),
            name_source: Some(NameSource::Agent),
            state: AgentState::Running,
            tool: Tool::ALL[0],
            tool_session_id: None,
            model: None,
            got_request: true,
            mid_turn: false,
            limit_stopped: false,
            created_at,
            ended_at: None,
            closed: false,
            question: None,
            needs_you: None,
            focused: false,
            tokens: Default::default(),
            session: None,
            turn_ended_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fixtures::agent;
    use super::*;
    use crate::key_labels::fixtures as keys;

    fn facts<'a>(
        agents: &'a [ProjectAgent],
        looked: &'a Looked,
        labels: &'a KeyLabels,
    ) -> Facts<'a> {
        Facts {
            agents,
            looked,
            open: false,
            shown: None,
            keys: labels,
        }
    }

    /// SHB-03: a turn that ended after the user last looked is a reply waiting, the button's dot; looking at it
    /// clears it; the open panel's own conversation never waits.
    #[test]
    fn a_reply_waits_per_conversation_until_the_user_looks() {
        let k = keys::base();
        let mut looked = Looked::new(100);
        let mut a = agent("a1", "Checkout slowness", 1);
        let mut b = agent("a2", "Footer audit", 2);
        // A turn that ended before the app started isn't new.
        a.turn_ended_at = Some(50);
        assert!(!view(&facts(std::slice::from_ref(&a), &looked, &k)).dot);
        a.turn_ended_at = Some(200);
        b.turn_ended_at = Some(150);
        let agents = [a.clone(), b.clone()];
        let v = view(&facts(&agents, &looked, &k));
        assert!(v.dot && !v.tinted);
        assert_eq!(v.dot_label, Some("New reply"));
        // Looked at a1: b's reply still waits.
        looked.look("a1", 300);
        assert!(view(&facts(&agents, &looked, &k)).dot);
        // The panel open on a2: nothing waits; closed again on a2, still nothing until a new turn ends.
        let open = view(&Facts {
            open: true,
            shown: Some("a2"),
            ..facts(&agents, &looked, &k)
        });
        assert!(open.tinted && !open.dot);
        looked.look("a2", 300);
        assert!(!view(&facts(&agents, &looked, &k)).dot);
        // A permission prompt waits too (d0077), unless its terminal has the focus.
        b.needs_you = Some("Allow npm test?".into());
        assert!(view(&facts(&[a, b.clone()], &looked, &k)).dot);
        b.focused = true;
        assert!(!view(&facts(&[b], &looked, &k)).dot);
    }

    /// OTQ-9 (d0119): a conversation that asks you says so on the dot, not "New reply", before a newer reply;
    /// answered, it is a reply like any other.
    #[test]
    fn a_question_says_asks_you_on_the_dot() {
        let k = keys::base();
        let looked = Looked::new(0);
        let mut asks = agent("a1", "Import issues", 1);
        asks.question = Some(otaq_core::board::questions::HumanQuestion {
            id: "q1".into(),
            text: "Import closed issues too?".into(),
            options: Vec::new(),
            recommended: None,
            notes: Vec::new(),
            asked_at: 0,
            send_back: None,
        });
        asks.turn_ended_at = Some(10);
        let mut newer = agent("a2", "Checkout slowness", 2);
        newer.turn_ended_at = Some(20);
        let v = view(&facts(&[asks.clone(), newer], &looked, &k));
        assert_eq!(v.dot_label, Some("Asks you"));
        assert_eq!(v.hint, "Asks you \u{b7} Open or close \u{b7} \u{2318}L");
        // Answered: its turn goes on, and a reply is a "New reply" again.
        asks.question = None;
        let v = view(&facts(std::slice::from_ref(&asks), &looked, &k));
        assert_eq!(v.dot_label, Some("New reply"));
        assert_eq!(v.hint, "Open or close \u{b7} \u{2318}L");
    }

    /// SHB-02: the button is tinted while the panel is open and has a dot while a reply waits; its hover names the
    /// user's key.
    #[test]
    fn the_button_tints_while_open_and_dots_while_a_reply_waits() {
        let k = keys::base();
        for (open, dot) in [(false, false), (true, false), (false, true), (true, true)] {
            let b = button(open, dot.then_some(NEW_REPLY), &k);
            assert_eq!((b.tinted, b.dot), (open, dot));
            assert_eq!(
                (b.icon, b.label, b.hint.as_str()),
                (IconName::Bot, "Noyan", "Open or close \u{b7} \u{2318}L")
            );
        }
        assert_eq!(
            button(
                false,
                None,
                &keys::replaced("focus_agent_panel", None, 0, "⌥L")
            )
            .hint,
            "Open or close \u{b7} ⌥L"
        );
        assert_eq!(
            button(false, None, &keys::removed("focus_agent_panel", None, 0)).hint,
            "Open or close"
        );
    }
}
