//! Otaq's side of the hook mapping of SDD §11.4 (ATT-01, ATT-04; d0009, d0020, d0042, d0046, d0088): from one of
//! Otaq's events ([`ToolEvent`], given by the tool's adapter) and the turn's facts to a [`Signal`] for the ticket.
//!
//! The adapter says what happened; the rules here are Otaq's own. A turn that ended with background work is
//! Working in background; one that ended after a submit or an ask keeps the status those set; any other is Idle.

use serde::{Deserialize, Serialize};

use super::background::BackgroundWork;
use crate::agent_tools::ToolEvent;

/// Starts the stored prompt of a request raised inside a helper agent (the card shows a bot icon for it).
pub const HELPER_PROMPT_PREFIX: &str = "helper agent asks: ";

/// What happened in the session's current turn, as far as the turn-end rule cares.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnFacts {
    /// `otaq review submit` or `otaq ask human` ran in this turn: its turn end is not Idle.
    pub submitted_or_asked: bool,
}

/// What a hook means for the ticket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "signal", rename_all = "snake_case")]
pub enum Signal {
    /// A permission prompt or another request for the user, also from inside a helper agent.
    NeedsYou { prompt: String },
    /// The turn ended with background tasks or wakeups pending (neutral).
    WorkingInBackground { work: BackgroundWork },
    /// The turn ended with nothing pending and no submit or ask: the agent waits (turquoise).
    Idle {
        /// The last line the agent wrote, if any.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_line: Option<String>,
    },
    /// The turn ended after a submit or an ask: the status those set stays.
    TurnEnded,
    /// A new turn started.
    Working {
        /// The text that started it, when the tool gives it (a Project agent's first request).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
    },
    /// The turn ended on the plan's usage limit (USG-03): the agent shows Waiting for limit reset and the tool's
    /// limit becomes active (d0020, d0046), with the reset the tool's signal carried, if any (USG-04).
    LimitReached {
        hit: crate::usage::limit::LimitHit,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resets_at: Option<i64>,
    },
    /// The turn failed for another reason: logged; the status stays.
    Failed {
        error: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    /// Nothing to show (a hook that gave no event, such as another notification type).
    Ignored,
}

impl Signal {
    /// Whether the agent's turn has ended with this signal: it is idle between turns, so automatic deliveries
    /// may be typed (SES-06).
    pub fn ends_turn(&self) -> bool {
        matches!(
            self,
            Self::WorkingInBackground { .. }
                | Self::Idle { .. }
                | Self::TurnEnded
                | Self::LimitReached { .. }
        )
    }
}

/// Otaq's rule for one event (SDD §11.4). `None` for the events that don't concern the ticket's status (tokens,
/// limit windows, the tool id, the model known or refused, an unrecognised hook); the service handles those on
/// their own.
pub fn signal(event: &ToolEvent, turn: TurnFacts) -> Option<Signal> {
    Some(match event {
        ToolEvent::NeedsYou {
            prompt,
            helper_agent,
        } => Signal::NeedsYou {
            prompt: if *helper_agent {
                format!("{HELPER_PROMPT_PREFIX}{prompt}")
            } else {
                prompt.clone()
            },
        },
        // A question the tool shows is handled like a request for the user, with its prompt.
        ToolEvent::AsksYou { prompt } => Signal::NeedsYou {
            prompt: prompt.clone(),
        },
        ToolEvent::TurnEnded {
            background,
            last_message,
        } => {
            if !background.is_empty() {
                Signal::WorkingInBackground {
                    work: background.clone(),
                }
            } else if turn.submitted_or_asked {
                Signal::TurnEnded
            } else {
                Signal::Idle {
                    last_line: last_message.as_deref().and_then(last_line),
                }
            }
        }
        ToolEvent::NewTurn { prompt } => Signal::Working {
            prompt: prompt.clone(),
        },
        ToolEvent::Failed { error, message } => Signal::Failed {
            error: error.clone(),
            message: message.clone(),
        },
        ToolEvent::LimitReached { hit, resets_at } => Signal::LimitReached {
            hit: hit.clone(),
            resets_at: *resets_at,
        },
        // The model events are the service's: T-0336 writes the model that ran on the session, T-0337 applies a
        // refused model.
        ToolEvent::Tokens { .. }
        | ToolEvent::LimitWindows { .. }
        | ToolEvent::ToolIdKnown { .. }
        | ToolEvent::ModelKnown { .. }
        | ToolEvent::ModelRefused { .. }
        | ToolEvent::Unrecognised { .. } => return None,
    })
}

/// Whether `event` says where the agent's turn is: a prompt submitted, a turn ending, a prompt for the user, a failed
/// turn or a refused model. A hand-off the agent hadn't picked up is picked up then (d0118). The model, tokens and
/// limit windows (what the status line also reports), the tool's own id and an event Otaq doesn't know aren't.
pub fn reports_activity(event: &ToolEvent) -> bool {
    matches!(event, ToolEvent::ModelRefused { .. }) || signal(event, TurnFacts::default()).is_some()
}

/// The last non-empty line of a message, without Markdown list or heading marks.
pub fn last_line(message: &str) -> Option<String> {
    message
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .map(|l| l.trim_start_matches(['#', '-', '*', '>', ' ']).to_owned())
        .filter(|l| !l.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signals::background::{BackgroundTask, Wakeup};
    use crate::usage::limit::{LimitHit, LimitKind};

    fn none() -> TurnFacts {
        TurnFacts::default()
    }

    fn turn_ended(background: BackgroundWork, last_message: Option<&str>) -> ToolEvent {
        ToolEvent::TurnEnded {
            background,
            last_message: last_message.map(str::to_owned),
        }
    }

    #[test]
    fn requests_and_questions_are_needs_you_with_the_helper_marker() {
        let ask = |helper_agent| ToolEvent::NeedsYou {
            prompt: "Run command: ls".into(),
            helper_agent,
        };
        assert_eq!(
            signal(&ask(false), none()),
            Some(Signal::NeedsYou {
                prompt: "Run command: ls".into()
            })
        );
        assert_eq!(
            signal(&ask(true), none()),
            Some(Signal::NeedsYou {
                prompt: "helper agent asks: Run command: ls".into()
            })
        );
        assert_eq!(
            signal(
                &ToolEvent::AsksYou {
                    prompt: "Which database?".into()
                },
                none()
            ),
            Some(Signal::NeedsYou {
                prompt: "Which database?".into()
            })
        );
    }

    /// The Stop rule, in its order: background work, else a submit or ask this turn, else Idle.
    #[test]
    fn the_turn_end_rule() {
        let asked = TurnFacts {
            submitted_or_asked: true,
        };
        let work = BackgroundWork {
            tasks: vec![BackgroundTask {
                kind: "shell".into(),
                description: Some("test run".into()),
                helper_agent: false,
            }],
            wakeups: vec![],
        };
        assert_eq!(
            signal(&turn_ended(work.clone(), Some("Waiting.")), asked),
            Some(Signal::WorkingInBackground { work })
        );
        let wakeup = BackgroundWork {
            tasks: vec![],
            wakeups: vec![Wakeup {
                schedule: "10 16 * * *".into(),
                recurring: false,
            }],
        };
        assert!(matches!(
            signal(&turn_ended(wakeup, None), none()),
            Some(Signal::WorkingInBackground { .. })
        ));
        let message = Some("I split the name.\n\n## Should I also update the tests?\n");
        assert_eq!(
            signal(&turn_ended(BackgroundWork::default(), message), asked),
            Some(Signal::TurnEnded)
        );
        assert_eq!(
            signal(&turn_ended(BackgroundWork::default(), message), none()),
            Some(Signal::Idle {
                last_line: Some("Should I also update the tests?".into())
            })
        );
        assert_eq!(
            signal(&turn_ended(BackgroundWork::default(), None), none()),
            Some(Signal::Idle { last_line: None })
        );
    }

    #[test]
    fn turns_failures_and_limits() {
        assert_eq!(
            signal(
                &ToolEvent::NewTurn {
                    prompt: Some("go on".into())
                },
                none()
            ),
            Some(Signal::Working {
                prompt: Some("go on".into())
            })
        );
        let hit = LimitHit {
            kind: LimitKind::Session,
            text: "You've hit your session limit · resets 3:45pm".into(),
        };
        assert_eq!(
            signal(
                &ToolEvent::LimitReached {
                    hit: hit.clone(),
                    resets_at: Some(1_791_547_200_000)
                },
                none()
            ),
            Some(Signal::LimitReached {
                hit,
                resets_at: Some(1_791_547_200_000)
            })
        );
        assert_eq!(
            signal(
                &ToolEvent::Failed {
                    error: "server_error".into(),
                    message: None
                },
                none()
            ),
            Some(Signal::Failed {
                error: "server_error".into(),
                message: None
            })
        );
        for other in [
            ToolEvent::ToolIdKnown {
                tool_id: "x".into(),
            },
            ToolEvent::LimitWindows { windows: vec![] },
            ToolEvent::Unrecognised {
                event: "SessionStart".into(),
            },
        ] {
            assert_eq!(signal(&other, none()), None, "{other:?}");
        }
    }

    #[test]
    fn which_signals_end_the_turn() {
        assert!(Signal::Idle { last_line: None }.ends_turn());
        assert!(Signal::TurnEnded.ends_turn());
        assert!(!Signal::Working { prompt: None }.ends_turn());
        assert!(
            !Signal::NeedsYou {
                prompt: String::new()
            }
            .ends_turn()
        );
    }
}
