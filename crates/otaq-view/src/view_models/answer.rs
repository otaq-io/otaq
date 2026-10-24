//! Answering the agent from Needs you (ATT-08; d0045, d0048, d0042; *10 → Answer · inline, Idle · third needs-you label,
//! After a reboot · answer resumes*): Asks you and Idle items have **Answer**, which opens a reply box in the row;
//! ↵ sends, Esc cancels. An Interrupted item that kept its question reads **Answer & resume**: the one click
//! resumes the conversation and delivers the answer. Unsent text in the terminal asks *Keep it and cancel / Clear
//! it and send* first (the prompt of T-0061). The numbered answer options (ATT-12) build on this box (T-0165).

use otaq_core::board::needs_you::NeedsYouItem;
use otaq_core::board::questions::Choice;
use otaq_core::domain::{Status, TicketKey};
use otaq_core::protocol::{NeedsYouAnswerParams, Request};
use otaq_core::session::deliveries::Delivery;

use crate::icons::IconName;
use crate::view_models::terminal_strip::{Click, UnsentPrompt};

/// How an item takes an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerKind {
    /// Typed into the live session.
    Send,
    /// The session ended with a restart: the answer resumes it (d0045).
    Resume,
}

impl AnswerKind {
    /// The item's answer kind: Asks you and Idle (live or interrupted with their label). A permission prompt
    /// is answered in the terminal.
    pub fn of(item: &NeedsYouItem) -> Option<Self> {
        let takes = |s: Status| matches!(s, Status::AsksYou | Status::Idle);
        if takes(item.status) {
            Some(Self::Send)
        } else if item.status == Status::Interrupted && item.interrupted_from.is_some_and(takes) {
            Some(Self::Resume)
        } else {
            None
        }
    }

    /// The row's button that opens the box.
    pub fn open_label(self) -> &'static str {
        match self {
            Self::Send => "Answer",
            Self::Resume => "Answer & resume",
        }
    }

    /// The box's send button (frame: "Send answer", "Answer & resume" with a play icon).
    pub fn send_label(self) -> &'static str {
        match self {
            Self::Send => "Send answer",
            Self::Resume => "Answer & resume",
        }
    }

    pub fn send_icon(self) -> IconName {
        match self {
            Self::Send => IconName::CornerDownLeft,
            Self::Resume => IconName::Play,
        }
    }
}

/// The free-text answer, if there is one to send (Send is disabled on an empty box).
pub fn free_text(text: &str) -> Option<Choice> {
    let text = text.trim();
    (!text.is_empty()).then(|| Choice::Other(text.to_owned()))
}

/// The `needs_you.answer` request; `clear` is "Clear it and send".
pub fn request(key: &TicketKey, choice: &Choice, clear: bool) -> Request {
    Request::NeedsYouAnswer(NeedsYouAnswerParams {
        key: key.to_string(),
        choice: choice.clone(),
        clear,
    })
}

/// What the app does with the service's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnswerOutcome {
    /// Delivered: the box closes; the item leaves with the next Needs you update.
    Sent,
    /// Unsent text in the terminal: ask first.
    Ask(UnsentPrompt),
}

/// What the unsent-text question says is being sent: "Your answer: option 2", "Your answer: Mailgun".
pub fn sending(choice: &Choice) -> String {
    match choice {
        Choice::Other(text) => format!("Your answer: {text}"),
        Choice::Option(n) => format!("Your answer: option {n}"),
    }
}

/// The outcome of an answer for `key`; `sending` describes it in the prompt.
pub fn outcome(key: &TicketKey, choice: &Choice, delivery: &Delivery) -> AnswerOutcome {
    match delivery {
        Delivery::Sent { .. } => AnswerOutcome::Sent,
        Delivery::NeedsConfirmation { unclearable_by, .. } => AnswerOutcome::Ask(
            UnsentPrompt::for_click(
                key.clone(),
                None,
                sending(choice),
                Click::Answer {
                    choice: choice.clone(),
                },
            )
            .unclearable_by(unclearable_by.clone()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::board::needs_you::NeedsYouGroup;

    fn item(status: Status, kept: Option<Status>) -> NeedsYouItem {
        NeedsYouItem {
            group: NeedsYouGroup::Family,
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            id: "t".into(),
            key: "SHOP-23".parse().unwrap(),
            title: "Split user full name".into(),
            status,
            interrupted_from: kept,
            question: None,
            reason: String::new(),
            since: 0,
            age_ms: 0,
            blocked: 0,
            message: None,
            suggestion: None,
            lead_note: None,
            change_agent: None,
            waiting_in: None,
        }
    }

    #[test]
    fn asks_you_and_idle_take_answers_and_an_interrupted_question_resumes() {
        assert_eq!(
            AnswerKind::of(&item(Status::AsksYou, None)),
            Some(AnswerKind::Send)
        );
        assert_eq!(
            AnswerKind::of(&item(Status::Idle, None)),
            Some(AnswerKind::Send)
        );
        assert_eq!(AnswerKind::of(&item(Status::NeedsYou, None)), None);
        assert_eq!(AnswerKind::of(&item(Status::Interrupted, None)), None);
        let resume = AnswerKind::of(&item(Status::Interrupted, Some(Status::AsksYou))).unwrap();
        assert_eq!(resume, AnswerKind::Resume);
        assert_eq!(
            (resume.open_label(), resume.send_label(), resume.send_icon()),
            ("Answer & resume", "Answer & resume", IconName::Play)
        );
        assert_eq!(AnswerKind::Send.send_label(), "Send answer");
    }

    #[test]
    fn an_empty_box_sends_nothing() {
        assert_eq!(free_text("  "), None);
        assert_eq!(
            free_text(" Only in the profile form for now "),
            Some(Choice::Other("Only in the profile form for now".into()))
        );
    }

    #[test]
    fn unsent_text_asks_and_clear_resends() {
        let key: TicketKey = "SHOP-23".parse().unwrap();
        let choice = Choice::Other("Postmark".into());
        assert_eq!(
            outcome(&key, &choice, &Delivery::Sent { count: 1 }),
            AnswerOutcome::Sent
        );
        let AnswerOutcome::Ask(prompt) = outcome(
            &key,
            &choice,
            &Delivery::NeedsConfirmation {
                ticket: None,
                unclearable_by: None,
            },
        ) else {
            panic!("asks")
        };
        assert_eq!(prompt.title(), "SHOP-23 has unsent text in its terminal");
        assert_eq!(prompt.sending, "Your answer: Postmark");
        let Click::Answer { choice: again } = prompt.click else {
            panic!("answer click")
        };
        assert_eq!(
            request(&key, &again, true),
            Request::NeedsYouAnswer(NeedsYouAnswerParams {
                key: "SHOP-23".into(),
                choice,
                clear: true,
            })
        );
    }
}
