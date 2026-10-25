//! The sheet's Messages section (MSG-01, MSG-03, MSG-06; d0030; *05 → Messages · agent-to-agent (SHOP-40.2),
//! Message states*): every question between this ticket and its directly linked tickets, oldest first, each with
//! its answer and one status line. Read-only, except answering a question that was sent to the user (Needs you).

use otaq_core::domain::TicketKey;
use otaq_core::messages::{Message, MessageState, Route};

use super::card::age_label;
use super::clock::local_hhmm;
use crate::icons::IconName;

/// How a status line looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Asked: waiting.
    Muted,
    /// Delivered (*05 → Message states*: st-working).
    Delivered,
    /// Answered.
    Done,
    /// Sent to the user (Needs you).
    Attention,
}

/// One message line: "SHOP-42 → SHOP-41 · 10:42", the text, and its status. `None` is the user ("you"): an
/// `ask human` question and its answer are logged here too (*10 → Answer · inline*).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub from: Option<TicketKey>,
    pub to: Option<TicketKey>,
    pub time: String,
    pub text: String,
    pub status: String,
    /// The status line's icon (*05 → Message states*): send, check, check-check, message-circle-reply, inbox.
    pub icon: IconName,
    pub tone: Tone,
}

/// A question and, once given, its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thread {
    pub id: String,
    /// When it was asked, for the order.
    pub asked_at: i64,
    pub question: Line,
    pub answer: Option<Line>,
    /// A question sent to the user (Needs you): answered here (the only thing the section lets the user do).
    pub answerable: bool,
}

/// "Messages · 1 thread · linked tickets only".
pub fn heading(threads: &[Thread]) -> String {
    match threads.len() {
        1 => "1 thread \u{b7} linked tickets only".to_owned(),
        n => format!("{n} threads \u{b7} linked tickets only"),
    }
}

/// The threads of a ticket, oldest first.
pub fn threads(messages: &[Message], now: i64) -> Vec<Thread> {
    messages.iter().map(|m| thread(m, now)).collect()
}

/// The agents' threads and the ticket's answered `ask human` questions ("SHOP-21 → you"), oldest first.
pub fn all_threads(
    messages: &[Message],
    answers: &[otaq_core::board::questions::HumanAnswer],
    key: &TicketKey,
    now: i64,
) -> Vec<Thread> {
    let mut out = threads(messages, now);
    out.extend(answers.iter().enumerate().map(|(n, a)| Thread {
        id: format!("human-{n}"),
        asked_at: a.asked_at,
        question: Line {
            from: Some(key.clone()),
            to: None,
            time: local_hhmm(a.asked_at),
            text: a.question.clone(),
            status: "Asked you".to_owned(),
            icon: IconName::MessageCircleQuestionMark,
            tone: Tone::Attention,
        },
        answer: Some(Line {
            from: None,
            to: Some(key.clone()),
            time: local_hhmm(a.answered_at),
            text: a.answer.clone(),
            status: format!(
                "Answered by you \u{b7} {}",
                age_label((now - a.answered_at).max(0))
            ),
            icon: IconName::CheckCheck,
            tone: Tone::Done,
        }),
        answerable: false,
    }));
    out.sort_by_key(|t| t.asked_at);
    out
}

fn thread(m: &Message, now: i64) -> Thread {
    let state = m.state();
    let (status, icon, tone) = match (state, m.route) {
        (MessageState::NeedsYou, _) => (
            format!(
                "Sent to you \u{b7} {} doesn\u{2019}t answer while it\u{2019}s finished",
                m.to
            ),
            IconName::Inbox,
            Tone::Attention,
        ),
        (MessageState::Asked, _) => (
            format!(
                "Asked \u{b7} {}\u{2019}s agent gets it at its next stop",
                m.to
            ),
            IconName::Send,
            Tone::Muted,
        ),
        (_, Some(Route::Resumed)) => (
            format!("Delivered \u{b7} {} is finished, resumed answer-only", m.to),
            IconName::MessageCircleReply,
            Tone::Delivered,
        ),
        (_, Some(Route::NeedsYou)) => (
            "Answered by you".to_owned(),
            IconName::Inbox,
            Tone::Delivered,
        ),
        _ => ("Delivered".to_owned(), IconName::Check, Tone::Delivered),
    };
    let answer = m.answer.as_ref().map(|text| {
        let at = m.answered_at.unwrap_or(m.asked_at);
        let ago = age_label((now - at).max(0));
        let status = match (m.route, state) {
            (Some(Route::Resumed), _) => {
                format!(
                    "Answered by resuming {}\u{2019}s session \u{b7} {ago}",
                    m.to
                )
            }
            (Some(Route::NeedsYou), _) => format!("Answered by you \u{b7} {ago}"),
            (_, MessageState::Answered) => {
                format!("Answered \u{b7} waiting to reach {}", m.asker())
            }
            _ => format!("Answered \u{b7} {ago}"),
        };
        Line {
            from: Some(m.to.clone()),
            to: m.from.clone(),
            time: local_hhmm(at),
            text: text.clone(),
            status,
            icon: IconName::CheckCheck,
            tone: Tone::Done,
        }
    });
    Thread {
        id: m.id.clone(),
        asked_at: m.asked_at,
        question: Line {
            from: m.from.clone(),
            to: Some(m.to.clone()),
            time: local_hhmm(m.asked_at),
            text: m.question.clone(),
            status,
            icon,
            tone,
        },
        answer,
        answerable: state == MessageState::NeedsYou,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(route: Option<Route>) -> Message {
        Message {
            id: "q1".into(),
            from: Some("SHOP-42".parse().unwrap()),
            from_agent: None,
            to: "SHOP-41".parse().unwrap(),
            question: "Which endpoint returns the wishlist?".into(),
            answer: None,
            route,
            asked_at: 0,
            delivered_at: None,
            answered_at: None,
            answer_delivered_at: None,
        }
    }

    #[test]
    fn every_state_has_its_line() {
        let asked = threads(&[message(Some(Route::Live))], 60_000);
        assert!(asked[0].question.status.starts_with("Asked"));
        assert_eq!(asked[0].question.tone, Tone::Muted);
        assert_eq!(asked[0].question.icon, IconName::Send);
        assert!(asked[0].answer.is_none());
        assert!(!asked[0].answerable);

        let mut live = message(Some(Route::Live));
        live.delivered_at = Some(1);
        assert_eq!(threads(&[live.clone()], 0)[0].question.status, "Delivered");
        assert_eq!(
            threads(&[live.clone()], 0)[0].question.icon,
            IconName::Check
        );

        let mut resumed = message(Some(Route::Resumed));
        resumed.delivered_at = Some(1);
        resumed.answer = Some("GET /api/wishlist".into());
        resumed.answered_at = Some(60_000);
        let t = &threads(&[resumed.clone()], 120_000)[0];
        assert_eq!(
            t.question.status,
            "Delivered \u{b7} SHOP-41 is finished, resumed answer-only"
        );
        assert_eq!(t.question.icon, IconName::MessageCircleReply);
        let answer = t.answer.as_ref().unwrap();
        assert_eq!(answer.icon, IconName::CheckCheck);
        assert_eq!(
            answer.from.as_ref().map(ToString::to_string).as_deref(),
            Some("SHOP-41"),
            "the answer goes the other way"
        );
        assert!(
            answer
                .status
                .starts_with("Answered by resuming SHOP-41\u{2019}s session")
        );

        let needs_you = message(Some(Route::NeedsYou));
        let t = &threads(&[needs_you], 0)[0];
        assert!(t.question.status.starts_with("Sent to you \u{b7}"));
        assert_eq!(t.question.tone, Tone::Attention);
        assert_eq!(t.question.icon, IconName::Inbox);
        assert!(t.answerable);

        // Answered but not delivered yet: the asker still waits (MSG-06).
        live.answer = Some("By price.".into());
        live.answered_at = Some(2);
        assert_eq!(
            threads(&[live], 0)[0].answer.as_ref().unwrap().status,
            "Answered \u{b7} waiting to reach SHOP-42"
        );
        assert_eq!(
            heading(&threads(&[resumed], 0)),
            "1 thread \u{b7} linked tickets only"
        );
    }

    /// `ask human` questions with their answers join the section ("Logged in SHOP-21 Messages").
    #[test]
    fn questions_to_the_user_are_logged_too() {
        let key: TicketKey = "SHOP-21".parse().unwrap();
        let answers = [otaq_core::board::questions::HumanAnswer {
            question: "Which email provider?".into(),
            answer: "Answer to your question: Postmark (option 1).".into(),
            asked_at: 5,
            answered_at: 60_005,
        }];
        let all = all_threads(&[message(Some(Route::Live))], &answers, &key, 120_005);
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].asked_at, 0, "oldest first");
        assert_eq!(all[1].question.to, None, "to you");
        assert_eq!(all[1].answer.as_ref().unwrap().from, None, "from you");
        assert!(
            all[1]
                .answer
                .as_ref()
                .unwrap()
                .status
                .starts_with("Answered by you")
        );
    }
}
