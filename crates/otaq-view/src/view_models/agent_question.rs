//! A Project agent's open question (OTQ-9; d0119, d0068, d0061, d0048): what Noyan asked with `otaq ask human`, shown
//! in its chip's popover in the Needs you strip and above its terminal in the panel, with the same answer UI as a
//! ticket's (its 2–4 options with their notes and the recommended one picked, or a free-text box for an open
//! question). Sending calls `project_agent.answer {id, choice, clear}`; unsent text in its terminal asks *Keep it and
//! cancel / Clear it and send* first.

use otaq_core::board::questions::HumanQuestion;
use otaq_core::project_agents::{AgentState, ProjectAgent};
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::agent_panel::LABEL;
use crate::view_models::answer_options::{self, Answer};

/// The question and how to answer it. `AgentQuestion` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentQuestion")]
pub struct AgentQuestion {
    pub project: String,
    /// The Project agent's id: `agent_panel.answer` sends to it.
    pub agent: String,
    /// "Noyan · Checkout slowness asks".
    pub title: String,
    /// The question, its options (none for an open question) and the buttons' words.
    pub answer: Answer,
}

/// "Noyan · Checkout slowness asks".
pub fn title(name: &str) -> String {
    format!("{LABEL} \u{b7} {name} asks")
}

/// The question box of `agent`'s open question, by its id, name and question.
pub fn of(project: &str, agent: &str, name: &str, question: &HumanQuestion) -> AgentQuestion {
    AgentQuestion {
        project: project.to_owned(),
        agent: agent.to_owned(),
        title: title(name),
        answer: answer_options::agent_question(question),
    }
}

/// The shown agent's question, while it runs and asks.
pub fn of_agent(agent: &ProjectAgent) -> Option<AgentQuestion> {
    if agent.state != AgentState::Running {
        return None;
    }
    let question = agent.question.as_ref()?;
    Some(of(&agent.project, &agent.id, &agent.name, question))
}

/// The unsent-text question's title: "Noyan · Checkout slowness has unsent text in its terminal".
pub fn unsent_title(name: &str) -> String {
    format!("{LABEL} \u{b7} {name} has unsent text in its terminal")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn question(options: &[&str], recommended: Option<u32>) -> HumanQuestion {
        HumanQuestion {
            id: "q1".into(),
            text: "Import closed issues too?".into(),
            options: options.iter().map(|o| (*o).to_owned()).collect(),
            recommended,
            notes: options
                .iter()
                .map(|o| Some(format!("{o}: the note.")))
                .collect(),
            asked_at: 0,
        }
    }

    #[test]
    fn options_come_with_notes_the_recommended_one_and_other() {
        let q = of(
            "SHOP",
            "a1",
            "Import issues",
            &question(&["Yes", "No"], Some(2)),
        );
        assert_eq!(q.title, "Noyan \u{b7} Import issues asks");
        assert_eq!(q.agent, "a1");
        assert_eq!(q.answer.question, "q1");
        assert_eq!(q.answer.text, "Import closed issues too?");
        let rows: Vec<_> = q
            .answer
            .rows
            .iter()
            .map(|r| (r.number, r.label.as_str(), r.recommended, r.picked, r.other))
            .collect();
        assert_eq!(
            rows,
            [
                (1, "Yes", false, false, false),
                (2, "No", true, true, false),
                (3, "Other\u{2026} type your own answer", false, false, true),
            ]
        );
        assert_eq!(q.answer.rows[0].note.as_deref(), Some("Yes: the note."));
        assert_eq!(q.answer.send, "Send answer");
    }

    #[test]
    fn an_open_question_has_a_free_text_box() {
        let q = of("SHOP", "a1", "Import issues", &question(&[], None));
        assert!(q.answer.rows.is_empty());
        assert_eq!(q.answer.other_placeholder, "Type your answer");
        assert_eq!(q.answer.hint, "\u{21b5} sends");
    }

    #[test]
    fn the_unsent_text_question_names_the_conversation() {
        assert_eq!(
            unsent_title("Checkout slowness"),
            "Noyan \u{b7} Checkout slowness has unsent text in its terminal"
        );
    }
}
