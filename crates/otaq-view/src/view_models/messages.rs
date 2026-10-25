//! The sheet's Messages section (MSG-01, MSG-03, MSG-06; d0030; *05 → Messages · agent-to-agent (SHOP-40.2),
//! Message states*): every question between this ticket and its directly linked tickets, oldest first, each with
//! its answer and one status line. Read-only, except answering a question that was sent to the user (Needs you).
//!
//! **The epic lead's threads** (LEAD-06, LEAD-07, LEAD-10; d0083; *05 → Row · Messages · the lead (0083)*): every
//! question a child asked its lead, the lead's answer or its pass-on to the user (with the user's answer), and the
//! lead's notes, logged on the epic and on the child ("3 threads · with Wishlist lead"). They come from the lead's
//! log ([`LeadEntry`], filled by T-0284's routing and T-0281/T-0285's notes) and read as [`lead_threads`] words them.

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

/// One message line: "SHOP-40.2 → SHOP-40.1 · 10:42", the text, and its status. `None` is the user ("you"): an
/// `ask human` question and its answer are logged here too (*10 → Answer · inline*), unless that end has a name
/// (`from_name` / `to_name`: the epic's lead, "Wishlist lead").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub from: Option<TicketKey>,
    pub to: Option<TicketKey>,
    /// An end that is neither a ticket nor the user: "Wishlist lead".
    pub from_name: Option<String>,
    pub to_name: Option<String>,
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
            from_name: None,
            to_name: None,
            time: local_hhmm(a.asked_at),
            text: a.question.clone(),
            status: "Asked you".to_owned(),
            icon: IconName::MessageCircleQuestionMark,
            tone: Tone::Attention,
        },
        answer: Some(Line {
            from: None,
            to: Some(key.clone()),
            from_name: None,
            to_name: None,
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

/// The user's answer to a question the lead passed on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserAnswer {
    pub text: String,
    pub at: i64,
    /// Typed into the child's session.
    pub delivered: bool,
}

/// What became of a question a child asked its lead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeadOutcome {
    /// Typed into the lead (or waiting to be): "asked lead · waiting", not in Needs you.
    Waiting,
    /// `otaq answer` by the lead.
    Answered {
        text: String,
        at: i64,
        /// Typed into the child's session.
        delivered: bool,
    },
    /// `otaq ask human --pass-on`: an "Asks you" item with the lead's option recommended and its reason.
    PassedOn {
        recommended: Option<String>,
        reason: String,
        answer: Option<UserAnswer>,
    },
    /// The lead's turn ended without an answer (or the lead ended): a plain "Asks you" item.
    HandedBack { answer: Option<UserAnswer> },
}

/// One entry of the lead's log on an epic and its child.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeadEntry {
    /// A child's `ask human` routed to its lead (LEAD-06).
    Question {
        id: String,
        child: TicketKey,
        text: String,
        asked_at: i64,
        outcome: LeadOutcome,
    },
    /// `otaq note` from the lead to a running child (LEAD-07, LEAD-10). `after` names the stop it answers
    /// ("Validation failed 3 times"); `read`: `otaq notes` printed it or it was typed in.
    Note {
        id: String,
        child: TicketKey,
        text: String,
        at: i64,
        after: Option<String>,
        read: bool,
    },
}

impl LeadEntry {
    fn at(&self) -> i64 {
        match self {
            Self::Question { asked_at, .. } => *asked_at,
            Self::Note { at, .. } => *at,
        }
    }
}

/// "3 threads · with Wishlist lead".
pub fn lead_heading(count: usize, lead: &str) -> String {
    match count {
        1 => format!("1 thread \u{b7} with {lead}"),
        n => format!("{n} threads \u{b7} with {lead}"),
    }
}

fn from_lead(
    lead: &str,
    child: &TicketKey,
    at: i64,
    text: &str,
    status: String,
    icon: IconName,
    tone: Tone,
) -> Line {
    Line {
        from: None,
        to: Some(child.clone()),
        from_name: Some(lead.to_owned()),
        to_name: None,
        time: local_hhmm(at),
        text: text.to_owned(),
        status,
        icon,
        tone,
    }
}

fn your_answer(child: &TicketKey, a: &UserAnswer) -> Line {
    Line {
        from: None,
        to: Some(child.clone()),
        from_name: None,
        to_name: None,
        time: local_hhmm(a.at),
        text: a.text.clone(),
        status: if a.delivered {
            format!("Answered by you \u{b7} typed into {child}\u{2019}s session")
        } else {
            format!("Answered by you \u{b7} waiting to reach {child}")
        },
        icon: IconName::CheckCheck,
        tone: Tone::Done,
    }
}

/// The lead's threads, oldest first (*05 → Messages on the epic (SHOP-40 · Ticket tab)*, *Messages on SHOP-40.2*).
/// Read-only: a passed-on question is answered from Needs you, on the card or in the child's header.
pub fn lead_threads(entries: &[LeadEntry], lead: &str) -> Vec<Thread> {
    let mut entries: Vec<&LeadEntry> = entries.iter().collect();
    entries.sort_by_key(|e| e.at());
    entries
        .into_iter()
        .map(|e| match e {
            LeadEntry::Question {
                id,
                child,
                text,
                asked_at,
                outcome,
            } => {
                let (status, icon, tone, answer) = match outcome {
                    LeadOutcome::Waiting => (
                        "Routed to the lead \u{b7} asked lead \u{b7} waiting (not in Needs you)".to_owned(),
                        IconName::Send,
                        Tone::Delivered,
                        None,
                    ),
                    LeadOutcome::Answered {
                        text: answer,
                        at,
                        delivered,
                    } => (
                        "Routed to the lead \u{b7} not in Needs you".to_owned(),
                        IconName::Send,
                        Tone::Delivered,
                        Some(from_lead(
                            lead,
                            child,
                            *at,
                            answer,
                            if *delivered {
                                format!("Answered by {lead} \u{b7} typed into {child}\u{2019}s session")
                            } else {
                                format!("Answered by {lead} \u{b7} waiting to reach {child}")
                            },
                            IconName::CheckCheck,
                            Tone::Done,
                        )),
                    ),
                    LeadOutcome::PassedOn {
                        recommended,
                        reason,
                        answer,
                    } => {
                        let status = match (answer, recommended) {
                            (None, _) => {
                                format!("Passed on to you by {lead} \u{b7} waiting for your answer")
                            }
                            (Some(_), Some(option)) => format!(
                                "Passed on to you \u{b7} the lead recommended \u{201c}{option}\u{201d}: {reason}"
                            ),
                            (Some(_), None) => format!("Passed on to you \u{b7} {reason}"),
                        };
                        (
                            status,
                            IconName::Inbox,
                            Tone::Attention,
                            answer.as_ref().map(|a| your_answer(child, a)),
                        )
                    }
                    LeadOutcome::HandedBack { answer } => (
                        format!("Sent to you \u{b7} {lead} didn\u{2019}t answer it"),
                        IconName::Inbox,
                        Tone::Attention,
                        answer.as_ref().map(|a| your_answer(child, a)),
                    ),
                };
                Thread {
                    id: id.clone(),
                    asked_at: *asked_at,
                    question: Line {
                        from: Some(child.clone()),
                        to: None,
                        from_name: None,
                        to_name: Some(lead.to_owned()),
                        time: local_hhmm(*asked_at),
                        text: text.clone(),
                        status,
                        icon,
                        tone,
                    },
                    answer,
                    answerable: false,
                }
            }
            LeadEntry::Note {
                id,
                child,
                text,
                at,
                after,
                read,
            } => {
                let status = match (after, read) {
                    (Some(stop), _) => {
                        format!("Lead\u{2019}s note after {stop} \u{b7} round counter back at 0")
                    }
                    (None, true) => format!("Lead\u{2019}s note \u{b7} read by {child}\u{2019}s agent"),
                    (None, false) => {
                        format!("Lead\u{2019}s note \u{b7} unread until {child}\u{2019}s next commit")
                    }
                };
                Thread {
                    id: id.clone(),
                    asked_at: *at,
                    question: from_lead(lead, child, *at, text, status, IconName::MessageSquareText, Tone::Muted),
                    answer: None,
                    answerable: false,
                }
            }
        })
        .collect()
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
            from_name: None,
            to_name: None,
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
            from_name: None,
            to_name: None,
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

    /// *05 → Messages on the epic (SHOP-40 · Ticket tab)*: answered by the lead, passed on and answered by you, and
    /// the lead's note after a stop; *Messages on SHOP-42*: a passed-on question waiting.
    #[test]
    fn the_leads_threads() {
        let child: TicketKey = "SHOP-42".parse().unwrap();
        let page: TicketKey = "SHOP-43".parse().unwrap();
        let log = vec![
            LeadEntry::Note {
                id: "n1".into(),
                child: page.clone(),
                text:
                    "The API returns items[] (SHOP-41). Fix the test, don\u{2019}t change the API."
                        .into(),
                at: 300,
                after: Some("Validation failed 3 times".into()),
                read: true,
            },
            LeadEntry::Question {
                id: "q7".into(),
                child: child.clone(),
                text: "Should the button show a filled heart once the item is saved?".into(),
                asked_at: 100,
                outcome: LeadOutcome::Answered {
                    text: "Yes: filled heart means saved, outline means not saved (plan, step 2)."
                        .into(),
                    at: 110,
                    delivered: true,
                },
            },
            LeadEntry::Question {
                id: "q8".into(),
                child: child.clone(),
                text: "Can guests save items without logging in?".into(),
                asked_at: 200,
                outcome: LeadOutcome::PassedOn {
                    recommended: Some("Logged-in users only".into()),
                    reason: "PAY-123 only mentions signed-in users.".into(),
                    answer: Some(UserAnswer {
                        text: "Logged-in users only (option 1).".into(),
                        at: 250,
                        delivered: true,
                    }),
                },
            },
        ];
        let threads = lead_threads(&log, "Wishlist lead");
        assert_eq!(
            lead_heading(threads.len(), "Wishlist lead"),
            "3 threads \u{b7} with Wishlist lead"
        );
        assert_eq!(
            lead_heading(1, "Wishlist lead"),
            "1 thread \u{b7} with Wishlist lead"
        );
        let ids: Vec<&str> = threads.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["q7", "q8", "n1"], "oldest first");

        let answered = &threads[0];
        assert_eq!(answered.question.from, Some(child.clone()));
        assert_eq!(answered.question.to_name.as_deref(), Some("Wishlist lead"));
        let answer = answered.answer.as_ref().unwrap();
        assert_eq!(answer.from_name.as_deref(), Some("Wishlist lead"));
        assert_eq!(answer.to, Some(child.clone()));
        assert_eq!(
            answer.status,
            "Answered by Wishlist lead \u{b7} typed into SHOP-42\u{2019}s session"
        );

        let passed = &threads[1];
        assert_eq!(
            passed.question.status,
            "Passed on to you \u{b7} the lead recommended \u{201c}Logged-in users only\u{201d}: PAY-123 only \
             mentions signed-in users."
        );
        assert_eq!(passed.question.tone, Tone::Attention);
        let yours = passed.answer.as_ref().unwrap();
        assert_eq!(
            (yours.from.as_ref(), yours.from_name.as_ref()),
            (None, None),
            "you"
        );
        assert_eq!(
            yours.status,
            "Answered by you \u{b7} typed into SHOP-42\u{2019}s session"
        );

        let note = &threads[2];
        assert_eq!(note.question.from_name.as_deref(), Some("Wishlist lead"));
        assert_eq!(note.question.to, Some(page));
        assert_eq!(
            note.question.status,
            "Lead\u{2019}s note after Validation failed 3 times \u{b7} round counter back at 0"
        );
        assert!(threads.iter().all(|t| !t.answerable));

        // On SHOP-42 while the second question waits for you; and before the lead answered.
        let waiting = lead_threads(
            &[
                LeadEntry::Question {
                    id: "q8".into(),
                    child: child.clone(),
                    text: "Can guests save items without logging in?".into(),
                    asked_at: 200,
                    outcome: LeadOutcome::PassedOn {
                        recommended: Some("Logged-in users only".into()),
                        reason: "PAY-123 only mentions signed-in users.".into(),
                        answer: None,
                    },
                },
                LeadEntry::Question {
                    id: "q9".into(),
                    child: child.clone(),
                    text: "Which icon?".into(),
                    asked_at: 300,
                    outcome: LeadOutcome::Waiting,
                },
                LeadEntry::Question {
                    id: "q10".into(),
                    child,
                    text: "Share link expiry?".into(),
                    asked_at: 400,
                    outcome: LeadOutcome::HandedBack { answer: None },
                },
            ],
            "Wishlist lead",
        );
        assert_eq!(
            waiting[0].question.status,
            "Passed on to you by Wishlist lead \u{b7} waiting for your answer"
        );
        assert!(waiting[0].answer.is_none());
        assert_eq!(
            waiting[1].question.status,
            "Routed to the lead \u{b7} asked lead \u{b7} waiting (not in Needs you)"
        );
        assert_eq!(
            waiting[2].question.status,
            "Sent to you \u{b7} Wishlist lead didn\u{2019}t answer it"
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
