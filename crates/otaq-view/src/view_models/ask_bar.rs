//! The Ask bar and the rail's Project agent button (T-0458; SHB-02, SHB-03; d0113, d0068, d0087; *24 → Row · The
//! Project agent* `s24-05`, *Row · The two views* `s24-03`): closed, the Project agent is a bar floating at the
//! bottom centre of the board and the List, "Ask Noyan… (“why is checkout slow?”)" with ⌘L. Typing shows
//! what to ask about ("ASK ABOUT": the selected ticket, Continue "<last conversation>", New conversation); Return
//! sends and opens the panel on the left (`agent_panel.rs`). A conversation that answered while the user wasn't
//! looking shows "New reply" on the bar and a dot on the rail button; a click on either opens it.
//!
//! **No agent starts before the first message** (d0068, d0087): New conversation and the selected ticket start one
//! with the typed words (`agent_panel.send`'s rule), Continue types them into a running conversation, and nothing is sent
//! without words.
//!
//! **A reply waiting** is per conversation: its last turn ended ([`ProjectAgent::turn_ended_at`]) after the user last
//! looked at it in the panel ([`Looked`]), or it waits for the user (a permission prompt or a question, d0077). The
//! conversation the open panel shows never waits: the user is looking at it.

use std::collections::HashMap;

use otaq_core::project_agents::{AgentState, ProjectAgent};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::key_labels::KeyLabels;
use crate::keymap::KeyAction;
use crate::view_models::agent_panel::agent_waits;

/// The closed bar's text (`s24-05` "Idle").
pub const PLACEHOLDER: &str = "Ask Noyan\u{2026} (\u{201c}why is checkout slow?\u{201d})";
/// The bar's text beside "New reply" (`s24-05` "A reply waiting").
pub const SHORT_PLACEHOLDER: &str = "Ask Noyan\u{2026}";

/// What a suggestion asks about. `AskAbout` on the page, and back in `ask_bar.send`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "AskAbout")]
pub enum About {
    /// The selected ticket: a new conversation about it.
    Ticket,
    /// The last conversation: the words are typed into it.
    Continue,
    /// A new conversation.
    New,
}

/// One row under "ASK ABOUT".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AskSuggestion")]
pub struct Suggestion {
    pub about: About,
    pub icon: IconName,
    /// "SHOP-29 · Coupon field (selected)", "Continue “Checkout slowness”", "New conversation".
    pub label: String,
    /// The ticket a [`About::Ticket`] row asks about.
    pub ticket: Option<String>,
    /// The conversation a [`About::Continue`] row types into.
    pub agent: Option<String>,
}

/// "New reply" on the closed bar, naming the conversation (SHB-03).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AskReply")]
pub struct Reply {
    /// The conversation a click opens.
    pub agent: String,
    pub label: &'static str,
    /// "Checkout slowness".
    pub name: String,
}

/// The rail's Project agent button (SHB-02, `s24-05`): the bot icon, tinted while the panel is open, a dot while a
/// reply waits; "Noyan" with "Open or close · ⌘L" on hover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AskAgentButton")]
pub struct AgentButton {
    pub icon: IconName,
    pub label: &'static str,
    /// The hover's line under the name: "Open or close · ⌘L" (without a key: "Open or close").
    pub hint: String,
    pub tinted: bool,
    pub dot: bool,
}

/// Everything `ask_bar.view` answers for a project. `AskBarView` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AskBarView")]
pub struct AskBarView {
    pub icon: IconName,
    pub placeholder: &'static str,
    /// Beside "New reply".
    pub short_placeholder: &'static str,
    /// ⌘L on the closed bar.
    pub keys: Option<String>,
    /// ↵ while typing.
    pub send_keys: &'static str,
    /// The send key's name for screen readers.
    pub send: &'static str,
    /// "ASK ABOUT".
    pub title: &'static str,
    /// In order; the first is chosen until the user moves.
    pub suggestions: Vec<Suggestion>,
    pub reply: Option<Reply>,
    pub button: AgentButton,
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

/// The selected ticket, when one is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selected<'a> {
    pub key: &'a str,
    pub title: &'a str,
}

/// What [`view`] reads.
pub struct Facts<'a> {
    /// The project's open Project agents (not its epic leads).
    pub agents: &'a [ProjectAgent],
    pub looked: &'a Looked,
    /// The panel is open.
    pub open: bool,
    /// The conversation the panel shows, if any.
    pub shown: Option<&'a str>,
    pub selected: Option<Selected<'a>>,
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

/// The conversation Continue goes on with: the one the panel shows while it runs, else the newest running one.
fn last_conversation<'a>(
    agents: &'a [ProjectAgent],
    shown: Option<&str>,
) -> Option<&'a ProjectAgent> {
    let running = |a: &&ProjectAgent| a.state == AgentState::Running && a.epic.is_none();
    agents
        .iter()
        .filter(running)
        .find(|a| Some(a.id.as_str()) == shown)
        .or_else(|| {
            agents
                .iter()
                .filter(running)
                .max_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)))
        })
}

/// The rail button's look.
pub fn button(open: bool, dot: bool, keys: &KeyLabels) -> AgentButton {
    AgentButton {
        icon: IconName::Bot,
        label: "Noyan",
        hint: match keys.label(KeyAction::FocusAgentPanel) {
            Some(keys) => format!("Open or close \u{b7} {keys}"),
            None => "Open or close".to_owned(),
        },
        tinted: open,
        dot,
    }
}

pub fn view(f: &Facts) -> AskBarView {
    let mut suggestions = Vec::new();
    if let Some(selected) = f.selected {
        suggestions.push(Suggestion {
            about: About::Ticket,
            icon: IconName::Ticket,
            label: format!("{} \u{b7} {} (selected)", selected.key, selected.title),
            ticket: Some(selected.key.to_owned()),
            agent: None,
        });
    }
    if let Some(last) = last_conversation(f.agents, f.shown) {
        suggestions.push(Suggestion {
            about: About::Continue,
            icon: IconName::MessageCircle,
            label: format!("Continue \u{201c}{}\u{201d}", last.name),
            ticket: None,
            agent: Some(last.id.clone()),
        });
    }
    suggestions.push(Suggestion {
        about: About::New,
        icon: IconName::Plus,
        label: "New conversation".to_owned(),
        ticket: None,
        agent: None,
    });
    let open_on = f.shown.filter(|_| f.open);
    // The newest reply: the one whose turn ended last (a wait without a turn's end counts as now).
    let reply = f
        .agents
        .iter()
        .filter(|a| a.epic.is_none() && reply_waits(a, f.looked, open_on))
        .max_by_key(|a| (a.turn_ended_at.unwrap_or(i64::MAX), a.created_at))
        .map(|a| Reply {
            agent: a.id.clone(),
            label: "New reply",
            name: a.name.clone(),
        });
    AskBarView {
        icon: IconName::Bot,
        placeholder: PLACEHOLDER,
        short_placeholder: SHORT_PLACEHOLDER,
        keys: f.keys.label(KeyAction::FocusAgentPanel),
        send_keys: "\u{21b5}",
        send: "Send",
        title: "ASK ABOUT",
        suggestions,
        button: button(f.open, reply.is_some(), f.keys),
        reply,
    }
}

/// The first message a [`About::Ticket`] row starts its conversation with: the ticket named first, unless the words
/// name it already ("why is SHOP-29 back in Dev?").
pub fn about_ticket(key: &str, text: &str) -> String {
    let text = text.trim();
    if text.contains(key) {
        text.to_owned()
    } else {
        format!("About {key}: {text}")
    }
}

/// One Ask bar state in the gallery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AskBarGalleryState")]
pub struct GalleryState {
    pub name: &'static str,
    /// What the bar's field holds: empty, or the words being typed (suggestions show then).
    pub typed: &'static str,
    pub view: AskBarView,
}

/// The gallery's Ask bar section (`s24-05`): closed, typing with a ticket selected and a conversation running, a
/// reply waiting, and the panel open (the rail button tinted).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AskBarGallery")]
pub struct Gallery {
    pub states: Vec<GalleryState>,
}

pub fn gallery() -> Gallery {
    let keys = KeyLabels::default();
    let looked = Looked::new(0);
    let mut checkout = fixtures::agent("a1", "Checkout slowness", 1);
    let idle = view(&Facts {
        agents: &[],
        looked: &looked,
        open: false,
        shown: None,
        selected: None,
        keys: &keys,
    });
    let typing = view(&Facts {
        agents: std::slice::from_ref(&checkout),
        looked: &looked,
        open: false,
        shown: None,
        selected: Some(Selected {
            key: "SHOP-29",
            title: "Coupon field",
        }),
        keys: &keys,
    });
    checkout.turn_ended_at = Some(10);
    let replied = view(&Facts {
        agents: std::slice::from_ref(&checkout),
        looked: &looked,
        open: false,
        shown: None,
        selected: None,
        keys: &keys,
    });
    let open = view(&Facts {
        agents: std::slice::from_ref(&checkout),
        looked: &looked,
        open: true,
        shown: Some("a1"),
        selected: None,
        keys: &keys,
    });
    Gallery {
        states: vec![
            GalleryState {
                name: "Idle",
                typed: "",
                view: idle,
            },
            GalleryState {
                name: "Typing (⌘L focuses it)",
                typed: "why is SHOP-29 back in Dev?",
                view: typing,
            },
            GalleryState {
                name: "A reply waiting",
                typed: "",
                view: replied,
            },
            GalleryState {
                name: "Open: the rail button tinted",
                typed: "",
                view: open,
            },
        ],
    }
}

/// Project agents for the tests and the gallery.
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
            epic: None,
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
            selected: None,
            keys: labels,
        }
    }

    fn labels(v: &AskBarView) -> Vec<&str> {
        v.suggestions.iter().map(|s| s.label.as_str()).collect()
    }

    /// `s24-05` "Idle": the bar with its text and ⌘L; only New conversation to ask about; no reply, no dot.
    #[test]
    fn idle_the_bar_asks_and_offers_a_new_conversation() {
        let k = keys::base();
        let looked = Looked::new(0);
        let v = view(&facts(&[], &looked, &k));
        assert_eq!(
            v.placeholder,
            "Ask Noyan\u{2026} (\u{201c}why is checkout slow?\u{201d})"
        );
        assert_eq!(v.keys.as_deref(), Some("\u{2318}L"));
        assert_eq!((v.title, v.send_keys), ("ASK ABOUT", "\u{21b5}"));
        assert_eq!(labels(&v), ["New conversation"]);
        assert_eq!(v.suggestions[0].icon, IconName::Plus);
        assert_eq!(v.reply, None);
        assert!(!v.button.tinted && !v.button.dot);
    }

    /// `s24-05` "Typing": the selected ticket, Continue the last conversation, New conversation, in that order.
    #[test]
    fn typing_suggests_the_ticket_the_last_conversation_and_a_new_one() {
        let k = keys::base();
        let looked = Looked::new(0);
        let older = agent("a1", "Footer audit", 1);
        let newer = agent("a2", "Checkout slowness", 2);
        let mut ended = agent("a3", "Old question", 3);
        ended.state = AgentState::Ended;
        let mut lead = agent("a4", "Wishlist lead", 4);
        lead.epic = Some("SHOP-40".parse().unwrap());
        let agents = [older, newer, ended, lead];
        let v = view(&Facts {
            selected: Some(Selected {
                key: "SHOP-29",
                title: "Coupon field",
            }),
            ..facts(&agents, &looked, &k)
        });
        assert_eq!(
            labels(&v),
            [
                "SHOP-29 \u{b7} Coupon field (selected)",
                "Continue \u{201c}Checkout slowness\u{201d}",
                "New conversation"
            ]
        );
        assert_eq!(v.suggestions[0].ticket.as_deref(), Some("SHOP-29"));
        assert_eq!(v.suggestions[0].icon, IconName::Ticket);
        assert_eq!(
            (v.suggestions[1].about, v.suggestions[1].agent.as_deref()),
            (About::Continue, Some("a2"))
        );
        assert_eq!(v.suggestions[1].icon, IconName::MessageCircle);
        // The conversation the panel shows is the last one while it runs.
        let shown = view(&Facts {
            shown: Some("a1"),
            ..facts(&agents, &looked, &k)
        });
        assert_eq!(labels(&shown)[0], "Continue \u{201c}Footer audit\u{201d}");
    }

    /// SHB-03: a turn that ended after the user last looked is a reply waiting, named on the bar, with the rail's dot;
    /// looking at it clears it; the open panel's own conversation never waits.
    #[test]
    fn a_reply_waits_per_conversation_until_the_user_looks() {
        let k = keys::base();
        let mut looked = Looked::new(100);
        let mut a = agent("a1", "Checkout slowness", 1);
        let mut b = agent("a2", "Footer audit", 2);
        // A turn that ended before the app started isn't new.
        a.turn_ended_at = Some(50);
        assert_eq!(
            view(&facts(std::slice::from_ref(&a), &looked, &k)).reply,
            None
        );
        a.turn_ended_at = Some(200);
        b.turn_ended_at = Some(150);
        let agents = [a.clone(), b.clone()];
        let v = view(&facts(&agents, &looked, &k));
        assert_eq!(
            v.reply,
            Some(Reply {
                agent: "a1".into(),
                label: "New reply",
                name: "Checkout slowness".into()
            })
        );
        assert_eq!(v.short_placeholder, "Ask Noyan\u{2026}");
        assert!(v.button.dot && !v.button.tinted);
        // Looked at a1: b's reply is the one left.
        looked.look("a1", 300);
        let v = view(&facts(&agents, &looked, &k));
        assert_eq!(v.reply.map(|r| r.agent).as_deref(), Some("a2"));
        // The panel open on a2: nothing waits; closed again on a2, still nothing until a new turn ends.
        let open = view(&Facts {
            open: true,
            shown: Some("a2"),
            ..facts(&agents, &looked, &k)
        });
        assert_eq!(open.reply, None);
        assert!(open.button.tinted && !open.button.dot);
        looked.look("a2", 300);
        assert_eq!(view(&facts(&agents, &looked, &k)).reply, None);
        // A permission prompt waits too (d0077), unless its terminal has the focus.
        b.needs_you = Some("Allow npm test?".into());
        let waiting = [a, b.clone()];
        assert_eq!(
            view(&facts(&waiting, &looked, &k))
                .reply
                .map(|r| r.agent)
                .as_deref(),
            Some("a2")
        );
        b.focused = true;
        assert_eq!(view(&facts(&[b], &looked, &k)).reply, None);
    }

    /// SHB-02: the rail button is tinted while the panel is open and has a dot while a reply waits; its hover names
    /// the user's key.
    #[test]
    fn the_rail_button_tints_while_open_and_dots_while_a_reply_waits() {
        let k = keys::base();
        for (open, dot) in [(false, false), (true, false), (false, true), (true, true)] {
            let b = button(open, dot, &k);
            assert_eq!((b.tinted, b.dot), (open, dot));
            assert_eq!(
                (b.icon, b.label, b.hint.as_str()),
                (IconName::Bot, "Noyan", "Open or close \u{b7} \u{2318}L")
            );
        }
        assert_eq!(
            button(
                false,
                false,
                &keys::replaced("focus_agent_panel", None, 0, "⌥L")
            )
            .hint,
            "Open or close \u{b7} ⌥L"
        );
        assert_eq!(
            button(false, false, &keys::removed("focus_agent_panel", None, 0)).hint,
            "Open or close"
        );
    }

    #[test]
    fn a_ticket_question_names_the_ticket_once() {
        assert_eq!(
            about_ticket("SHOP-29", " why is it back in Dev? "),
            "About SHOP-29: why is it back in Dev?"
        );
        assert_eq!(
            about_ticket("SHOP-29", "why is SHOP-29 back in Dev?"),
            "why is SHOP-29 back in Dev?"
        );
    }

    #[test]
    fn the_gallery_draws_the_four_states() {
        let g = gallery();
        assert_eq!(
            g.states.iter().map(|s| s.name).collect::<Vec<_>>(),
            [
                "Idle",
                "Typing (⌘L focuses it)",
                "A reply waiting",
                "Open: the rail button tinted"
            ]
        );
        assert_eq!(g.states[1].view.suggestions.len(), 3);
        assert!(g.states[2].view.reply.is_some());
        assert!(g.states[3].view.button.tinted);
    }
}
