//! The terminal's header strip and the typing prompts (SES-11, SES-06, SES-07; d0048, d0040): the session state,
//! where the agent works, its `$PORT`, "1 message waiting · Deliver now", the attach command, and the
//! "SHOP-23 has unsent text in its terminal" prompt with Keep it and cancel / Clear it and send.

use otaq_core::domain::{Status, TicketKey, ValidationOutcome};
use otaq_core::naming;
use otaq_core::protocol::{DeliverNowParams, Request, SessionTarget};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::terminal::Phase;
use crate::view_models::clock;

/// How the strip's state dot and label read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    /// Blue: the agent works.
    Working,
    /// Turquoise: the agent waits for you.
    Attention,
    Muted,
}

/// The strip's first item: "Live", "your turn", "Waiting for you", "Reconnecting…", "Ended". `your_turn`: the
/// terminal has focus and the agent waits, so only a subtle "your turn" (ATT-03, d0042).
pub fn state(phase: &Phase, status: Status, your_turn: bool) -> (&'static str, Tone) {
    match phase {
        Phase::Live if your_turn => ("your turn", Tone::Muted),
        Phase::Live if status.is_needs_you_family() => ("Waiting for you", Tone::Attention),
        Phase::Live => ("Live", Tone::Working),
        Phase::Attaching => ("Connecting…", Tone::Muted),
        Phase::Reconnecting => ("Reconnecting…", Tone::Muted),
        Phase::Ended => ("Ended", Tone::Muted),
        Phase::NoSession => ("No session", Tone::Muted),
    }
}

/// The terminal's focus as the service is told it (ATT-03): the keyboard is in the terminal and the Terminal tab is
/// on screen. Panel close, a tab switch, a click outside and ⌃` all make it false.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerminalFocus {
    pub has_focus: bool,
    pub hidden: bool,
}

impl TerminalFocus {
    pub fn reported(self) -> bool {
        self.has_focus && !self.hidden
    }

    /// Applies a change; returns the focus to report when what the service knows changes.
    pub fn apply(&mut self, change: FocusChange) -> Option<bool> {
        let before = self.reported();
        match change {
            FocusChange::Gained => self.has_focus = true,
            FocusChange::Lost => self.has_focus = false,
            FocusChange::Hidden => self.hidden = true,
            FocusChange::Shown => self.hidden = false,
        }
        (self.reported() != before).then_some(self.reported())
    }
}

/// What happened to the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusChange {
    Gained,
    Lost,
    Hidden,
    Shown,
}

/// The worktree as the strip shows it: `…/wt/shop-web/SHOP-19`.
pub fn short_path(worktree: &str) -> String {
    let parts: Vec<&str> = worktree.split('/').filter(|p| !p.is_empty()).collect();
    match parts.len() {
        0..=3 => worktree.to_owned(),
        n => format!("…/{}", parts[n - 3..].join("/")),
    }
}

/// `agent :4019`.
pub fn agent_port(port: u16) -> String {
    format!("agent :{port}")
}

/// `otaq attach SHOP-19`, as copied by the strip's chip.
pub fn attach_command(key: &TicketKey) -> String {
    format!("{} attach {key}", naming::COMMAND_NAME)
}

/// "1 message waiting" / "3 messages waiting"; `None` when nothing waits.
pub fn waiting_label(count: u32) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 message waiting".to_owned()),
        n => Some(format!("{n} messages waiting")),
    }
}

/// "Instructions: shop-web" (d0040).
pub fn instructions_link(project_name: &str) -> String {
    format!("Instructions: {project_name}")
}

/// The colour of the Terminal tab's state label (T-0224). `TerminalStripTone` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "TerminalStripTone")]
pub enum StripTone {
    /// Blue: the agent works.
    Working,
    /// Turquoise: the agent waits for you.
    Attention,
    /// Calm but present: "your turn", "Waiting on its background work".
    Secondary,
    Muted,
    /// Red: Interrupted.
    Error,
}

/// The Terminal tab's state label with its colour. `TerminalStripState` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TerminalStripState")]
pub struct StripState {
    pub label: &'static str,
    pub tone: StripTone,
}

/// The Terminal tab's state label (*06 · Terminal tab*): [`state`], with "your turn" in the secondary colour and
/// "Waiting on its background work" for Working in background (calm, not needs-you, d0042).
pub fn tab_state(phase: &Phase, status: Status, your_turn: bool) -> StripState {
    let (label, tone) = state(phase, status, your_turn);
    let tone = match tone {
        Tone::Working => StripTone::Working,
        Tone::Attention => StripTone::Attention,
        Tone::Muted => StripTone::Muted,
    };
    match phase {
        Phase::Live if your_turn => StripState {
            label,
            tone: StripTone::Secondary,
        },
        Phase::Live if status == Status::WorkingInBackground => StripState {
            label: "Waiting on its background work",
            tone: StripTone::Secondary,
        },
        _ => StripState { label, tone },
    }
}

/// An Interrupted ticket whose session is gone shows "Before the restart" instead of its grid (REC-02).
pub const INTERRUPTED: StripState = StripState {
    label: "Interrupted",
    tone: StripTone::Error,
};

/// How the hint line under the terminal reads. `TerminalHintTone` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "TerminalHintTone")]
pub enum HintTone {
    Muted,
    /// The focus colour: the keyboard is in the terminal, or the user's typed text holds a note back.
    Focus,
}

/// The line under the terminal (*06 · Terminal tab*): how typing reaches the agent now. `TerminalHint` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TerminalHint")]
pub struct Hint {
    pub icon: IconName,
    pub text: String,
    pub tone: HintTone,
}

/// The hint for the agent's terminal (SES-10, d0042, d0048); `None` while it connects or has no session. A scrolled-up
/// terminal shows how far it is scrolled instead (the page's).
pub fn hint(
    phase: &Phase,
    focused: bool,
    status: Status,
    your_turn: bool,
    messages_waiting: u32,
) -> Option<Hint> {
    let line = |icon, text: &str, tone| {
        Some(Hint {
            icon,
            text: text.to_owned(),
            tone,
        })
    };
    let product = naming::PRODUCT_NAME;
    match phase {
        Phase::Attaching | Phase::NoSession => None,
        Phase::Reconnecting => line(
            IconName::RefreshCw,
            "Input paused while reconnecting · output will catch up",
            HintTone::Muted,
        ),
        Phase::Ended => line(
            IconName::Lock,
            "Read-only · the session ended",
            HintTone::Muted,
        ),
        // The agent is busy: the note waits for its turn to end, whatever the line holds.
        Phase::Live if messages_waiting > 0 && status == Status::Working => line(
            IconName::MousePointerClick,
            &format!(
                "{product}’s notes only arrive when the agent is idle and your input line is empty"
            ),
            HintTone::Muted,
        ),
        Phase::Live if messages_waiting > 0 => line(
            IconName::MousePointerClick,
            &format!("Unsent text in the input line · {product}’s note waits until it’s empty"),
            HintTone::Focus,
        ),
        Phase::Live if focused && your_turn => line(
            IconName::Keyboard,
            "Typing goes to the agent · you’re here, so no alert",
            HintTone::Focus,
        ),
        Phase::Live if focused => line(
            IconName::Keyboard,
            "Typing goes to the agent",
            HintTone::Focus,
        ),
        Phase::Live if status == Status::WorkingInBackground => line(
            IconName::MousePointerClick,
            "Not needs-you: not in Needs you until it actually asks for something",
            HintTone::Muted,
        ),
        Phase::Live => line(
            IconName::MousePointerClick,
            "Click the terminal to type to the agent",
            HintTone::Muted,
        ),
    }
}

/// The Validator side of the Terminal tab (VAL-11, d0063; *06 → Row · validator*): the switch's label, the state
/// label, the hint and the attach command of the ticket's validator. `outcome` is how its round ended (`None` while it
/// runs).
pub fn validator_label(round: Option<u32>) -> String {
    match round {
        Some(round) => format!("Validator · round {round}"),
        None => "Validator".to_owned(),
    }
}

/// `otaq attach SHOP-29 --stage validate` while the check runs (`running` names the check's stage, FLW-20); the
/// agent's command once it ended (only a running check can be attached).
pub fn validator_attach_command(key: &TicketKey, running: Option<&str>) -> String {
    match running {
        Some(stage) => format!("{} --stage {stage}", attach_command(key)),
        None => attach_command(key),
    }
}

/// "Live", "Waiting for you" (Needs you · validator), "Ended · failed", "Ended by restart"…
pub fn validator_state(
    phase: &Phase,
    status: Status,
    outcome: Option<ValidationOutcome>,
) -> StripState {
    let ended = |label| StripState {
        label,
        tone: StripTone::Muted,
    };
    match phase {
        Phase::Live if status == Status::CheckerNeedsYou => StripState {
            label: "Waiting for you",
            tone: StripTone::Attention,
        },
        Phase::Live => StripState {
            label: "Live",
            tone: StripTone::Working,
        },
        Phase::Ended => match outcome {
            Some(ValidationOutcome::Interrupted) => StripState {
                label: "Ended by restart",
                tone: StripTone::Error,
            },
            Some(ValidationOutcome::Pass) => ended("Ended · passed"),
            Some(ValidationOutcome::Fail) => ended("Ended · failed"),
            Some(ValidationOutcome::Cancelled) => ended("Ended · cancelled"),
            Some(ValidationOutcome::NoResult) => ended("Ended · no result"),
            Some(ValidationOutcome::Discarded) => ended("Ended · discarded"),
            Some(ValidationOutcome::Skipped) => ended("Ended · skipped"),
            Some(ValidationOutcome::Stopped) => ended("Ended · stopped"),
            None => ended("Ended"),
        },
        _ => tab_state(phase, status, false),
    }
}

/// The line under the validator's terminal: typing goes to it while it runs (it is the real CLI); read-only after.
pub fn validator_hint(
    phase: &Phase,
    focused: bool,
    outcome: Option<ValidationOutcome>,
) -> Option<Hint> {
    let line = |icon, text: &str, tone| {
        Some(Hint {
            icon,
            text: text.to_owned(),
            tone,
        })
    };
    match phase {
        Phase::Live if focused => line(
            IconName::Keyboard,
            "Typing goes to the validator",
            HintTone::Focus,
        ),
        Phase::Live => line(
            IconName::MousePointerClick,
            "Click the terminal to type to the validator",
            HintTone::Muted,
        ),
        Phase::Ended if outcome == Some(ValidationOutcome::Interrupted) => line(
            IconName::Lock,
            "Read-only · Resume starts a new validator",
            HintTone::Muted,
        ),
        Phase::Ended => line(
            IconName::Lock,
            "Read-only · this validator has ended",
            HintTone::Muted,
        ),
        _ => hint(phase, focused, Status::Checking, false, 0),
    }
}

/// What the tab says while the ticket has no agent session.
pub fn no_session(key: &TicketKey) -> String {
    format!("{key} has no agent session yet. It starts when the ticket goes to Dev.")
}

/// The marker over an ended agent session: "Session ended · 11:02 · 41 min" and why. `TerminalEndMarker` on the
/// page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "TerminalEndMarker")]
pub struct EndMarker {
    pub title: String,
    pub text: String,
}

pub fn end_marker(started_at: i64, ended_at: i64, reason: Option<&str>) -> EndMarker {
    EndMarker {
        title: format!(
            "Session ended · {} · {}",
            clock::local_hhmm(ended_at),
            clock::duration(ended_at - started_at)
        ),
        text: match reason {
            Some("exit 0") | None => "The agent’s session finished.".to_owned(),
            Some(reason) => format!("The agent’s session ended ({reason})."),
        },
    }
}

/// The question asked when a click would type into a line holding the user's unsent text (SES-07).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsentPrompt {
    pub ticket: TicketKey,
    /// The terminal's input line as it is now, when the app can read it.
    pub typed: Option<String>,
    /// What the click wants to send, as the prompt describes it.
    pub sending: String,
    /// The click that asked; "Clear it and send" repeats it with the line cleared first.
    pub click: Click,
    /// The text is a multi-line draft this tool can't clear ("Codex"): only Keep it and cancel (SES-07, Q-N).
    pub unclearable_by: Option<String>,
}

/// The user's click that stopped at the unsent-text question (SES-07, d0048).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Click {
    DeliverNow,
    /// Send back with this note (REV-04).
    SendBack {
        note: String,
    },
    /// Open PR / Push to PR: the skill is typed (REV-10).
    OpenPr,
    /// The PR dialog's confirm: the push instruction is typed.
    PrConfirm,
    /// The PR dialog's Retry after the skill reported a problem.
    PrRetry,
    /// Send again: the hand-off the agent hasn't picked up (d0118).
    SendAgain,
    /// An answer to the agent's question from Needs you, card or panel (ATT-08, ATT-12).
    Answer {
        choice: otaq_core::board::questions::Choice,
    },
}

impl UnsentPrompt {
    /// For any other click: `sending` says what it types.
    pub fn for_click(
        ticket: TicketKey,
        typed: Option<String>,
        sending: String,
        click: Click,
    ) -> Self {
        Self {
            ticket,
            typed,
            sending,
            click,
            unclearable_by: None,
        }
    }

    /// For Deliver now: the waiting messages.
    pub fn deliver_now(ticket: TicketKey, typed: Option<String>, waiting: u32) -> Self {
        let sending = match waiting {
            0 | 1 => "The waiting message".to_owned(),
            n => format!("The {n} waiting messages"),
        };
        Self {
            ticket,
            typed,
            sending,
            click: Click::DeliverNow,
            unclearable_by: None,
        }
    }

    /// The same question for a multi-line draft `tool` can't clear, when it names one.
    pub fn unclearable_by(mut self, tool: Option<String>) -> Self {
        self.unclearable_by = tool;
        self
    }

    /// Whether "Clear it and send" is offered.
    pub fn can_clear(&self) -> bool {
        self.unclearable_by.is_none()
    }

    /// Why only Keep it and cancel is offered: "Codex can't clear a multi-line draft. Clear it in the terminal, then
    /// send again."
    pub fn note(&self) -> Option<String> {
        self.unclearable_by.as_deref().map(unclearable_note)
    }

    pub fn title(&self) -> String {
        format!("{} has unsent text in its terminal", self.ticket)
    }

    /// What the two choices do (also the Project agent's question, which has no ticket).
    pub const EXPLANATION: &'static str = "Clear it and send clears the typed text, then sends. Keep it and cancel \
         leaves the terminal as it is; nothing is sent.";

    pub fn explanation(&self) -> &'static str {
        Self::EXPLANATION
    }
}

/// "Codex can't clear a multi-line draft. Clear it in the terminal, then send again." (SES-07, Q-N): the body of the
/// unsent-text question that offers only Keep it and cancel, also for a lead's or Project agent's answer.
pub fn unclearable_note(tool: &str) -> String {
    format!(
        "{tool} can\u{2019}t clear a multi-line draft. Clear it in the terminal, then send again."
    )
}

/// The user's answer to the prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Keep,
    Clear,
}

/// The request for a Deliver now click and the prompt's answer: Keep sends nothing (and clears nothing); Clear
/// asks the service to clear the line, then deliver.
pub fn deliver_now_request(ticket: &TicketKey, choice: Option<Choice>) -> Option<Request> {
    let target = SessionTarget {
        session: None,
        key: Some(ticket.to_string()),
        ..Default::default()
    };
    match choice {
        Some(Choice::Keep) => None,
        Some(Choice::Clear) => Some(Request::SessionDeliverNow(DeliverNowParams {
            target,
            clear_first: true,
        })),
        None => Some(Request::SessionDeliverNow(DeliverNowParams {
            target,
            clear_first: false,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_follows_the_keyboard_and_the_tab() {
        let mut f = TerminalFocus::default();
        assert_eq!(f.apply(FocusChange::Gained), Some(true));
        assert_eq!(f.apply(FocusChange::Hidden), Some(false), "another tab");
        assert_eq!(f.apply(FocusChange::Shown), Some(true));
        assert_eq!(f.apply(FocusChange::Gained), None, "no repeat");
        assert_eq!(f.apply(FocusChange::Lost), Some(false), "click outside, ⌃`");
        assert_eq!(f.apply(FocusChange::Hidden), None);
        assert_eq!(f.apply(FocusChange::Shown), None);
    }

    fn key() -> TicketKey {
        "SHOP-23".parse().unwrap()
    }

    #[test]
    fn the_validator_side_reads_its_round_and_how_it_ended() {
        assert_eq!(validator_label(Some(2)), "Validator · round 2");
        assert_eq!(
            validator_attach_command(&key(), Some("validate")),
            format!("{} attach SHOP-23 --stage validate", naming::COMMAND_NAME)
        );
        assert_eq!(
            validator_attach_command(&key(), None),
            attach_command(&key())
        );
        let live = validator_state(&Phase::Live, Status::Checking, None);
        assert_eq!((live.label, live.tone), ("Live", StripTone::Working));
        let asks = validator_state(&Phase::Live, Status::CheckerNeedsYou, None);
        assert_eq!(
            (asks.label, asks.tone),
            ("Waiting for you", StripTone::Attention)
        );
        let failed = validator_state(
            &Phase::Ended,
            Status::CheckFailed,
            Some(ValidationOutcome::Fail),
        );
        assert_eq!(
            (failed.label, failed.tone),
            ("Ended · failed", StripTone::Muted)
        );
        let restart = validator_state(
            &Phase::Ended,
            Status::Interrupted,
            Some(ValidationOutcome::Interrupted),
        );
        assert_eq!(
            (restart.label, restart.tone),
            ("Ended by restart", StripTone::Error)
        );
        assert_eq!(
            validator_hint(&Phase::Live, true, None).unwrap().text,
            "Typing goes to the validator"
        );
        assert_eq!(
            validator_hint(&Phase::Live, false, None).unwrap().text,
            "Click the terminal to type to the validator"
        );
        assert_eq!(
            validator_hint(&Phase::Ended, false, Some(ValidationOutcome::Pass))
                .unwrap()
                .text,
            "Read-only · this validator has ended"
        );
        assert_eq!(
            validator_hint(&Phase::Ended, false, Some(ValidationOutcome::Interrupted))
                .unwrap()
                .text,
            "Read-only · Resume starts a new validator"
        );
        assert_eq!(validator_hint(&Phase::Attaching, false, None), None);
    }

    #[test]
    fn states() {
        assert_eq!(
            state(&Phase::Live, Status::Working, false),
            ("Live", Tone::Working)
        );
        assert_eq!(
            state(&Phase::Live, Status::NeedsYou, false),
            ("Waiting for you", Tone::Attention)
        );
        assert_eq!(
            state(&Phase::Reconnecting, Status::Working, false).0,
            "Reconnecting…"
        );
        assert_eq!(
            state(&Phase::Ended, Status::WaitingForYou, false).0,
            "Ended"
        );
        // The terminal has focus and the agent waits: only "your turn" (ATT-03).
        assert_eq!(
            state(&Phase::Live, Status::Idle, true),
            ("your turn", Tone::Muted)
        );
    }

    #[test]
    fn strip_texts() {
        assert_eq!(
            short_path("/Users/me/.data/wt/shop-web/SHOP-19"),
            "…/wt/shop-web/SHOP-19"
        );
        assert_eq!(short_path("wt/a/B-1"), "wt/a/B-1");
        assert_eq!(agent_port(4019), "agent :4019");
        assert_eq!(
            attach_command(&"SHOP-19".parse().unwrap()),
            format!("{} attach SHOP-19", naming::COMMAND_NAME)
        );
        assert_eq!(instructions_link("shop-web"), "Instructions: shop-web");
    }

    #[test]
    fn tab_states() {
        let live = Phase::Live;
        assert_eq!(
            tab_state(&live, Status::Working, false),
            StripState {
                label: "Live",
                tone: StripTone::Working
            }
        );
        assert_eq!(
            tab_state(&live, Status::Idle, true),
            StripState {
                label: "your turn",
                tone: StripTone::Secondary
            }
        );
        assert_eq!(
            tab_state(&live, Status::WorkingInBackground, false),
            StripState {
                label: "Waiting on its background work",
                tone: StripTone::Secondary
            }
        );
        assert_eq!(
            tab_state(&live, Status::AsksYou, false).tone,
            StripTone::Attention
        );
        assert_eq!(
            tab_state(&Phase::Ended, Status::WorkingInBackground, false).label,
            "Ended"
        );
        assert_eq!(INTERRUPTED.label, "Interrupted");
    }

    #[test]
    fn hints_follow_the_phase_the_focus_and_the_waiting_notes() {
        let text = |phase: Phase, focused, status, your_turn, waiting| {
            hint(&phase, focused, status, your_turn, waiting).map(|h| (h.text, h.tone))
        };
        let s = |text: &str| text.replace("{product}", naming::PRODUCT_NAME);
        use HintTone::{Focus, Muted};
        assert_eq!(
            text(Phase::Live, false, Status::Working, false, 0),
            Some((s("Click the terminal to type to the agent"), Muted))
        );
        assert_eq!(
            text(Phase::Live, true, Status::Working, false, 0),
            Some((s("Typing goes to the agent"), Focus))
        );
        assert_eq!(
            text(Phase::Live, true, Status::Working, true, 0),
            Some((
                s("Typing goes to the agent · you’re here, so no alert"),
                Focus
            ))
        );
        assert_eq!(
            text(Phase::Live, true, Status::Idle, true, 1),
            Some((
                s("Unsent text in the input line · {product}’s note waits until it’s empty"),
                Focus
            ))
        );
        assert_eq!(
            text(Phase::Live, false, Status::Working, false, 1),
            Some((
                s(
                    "{product}’s notes only arrive when the agent is idle and your input line is empty"
                ),
                Muted
            ))
        );
        assert_eq!(
            text(Phase::Live, false, Status::WorkingInBackground, false, 0),
            Some((
                s("Not needs-you: not in Needs you until it actually asks for something"),
                Muted
            ))
        );
        assert_eq!(
            text(Phase::Reconnecting, true, Status::Working, false, 0),
            Some((
                s("Input paused while reconnecting · output will catch up"),
                Muted
            ))
        );
        assert_eq!(
            text(Phase::Ended, false, Status::WaitingForYou, false, 0),
            Some((s("Read-only · the session ended"), Muted))
        );
        assert_eq!(
            text(Phase::Attaching, false, Status::Working, false, 0),
            None
        );
        assert_eq!(
            text(Phase::NoSession, false, Status::Backlog, false, 0),
            None
        );
    }

    #[test]
    fn end_markers_and_no_session() {
        let marker = end_marker(0, 41 * 60_000, Some("exit 0"));
        assert!(marker.title.starts_with("Session ended · "));
        assert!(marker.title.ends_with(" · 41 min"));
        assert_eq!(marker.text, "The agent’s session finished.");
        assert_eq!(
            end_marker(0, 0, Some("killed")).text,
            "The agent’s session ended (killed)."
        );
        assert_eq!(
            no_session(&key()),
            "SHOP-23 has no agent session yet. It starts when the ticket goes to Dev."
        );
    }

    #[test]
    fn chip_counts() {
        assert_eq!(waiting_label(0), None);
        assert_eq!(waiting_label(1).as_deref(), Some("1 message waiting"));
        assert_eq!(waiting_label(3).as_deref(), Some("3 messages waiting"));
    }

    /// Q-N: a multi-line draft the tool can't clear offers only Keep it and cancel, and says why.
    #[test]
    fn a_draft_the_tool_cant_clear() {
        let prompt = UnsentPrompt::deliver_now(key(), None, 1);
        assert!(prompt.can_clear());
        assert_eq!(prompt.note(), None);
        let prompt = prompt.unclearable_by(Some("Codex".into()));
        assert!(!prompt.can_clear());
        assert_eq!(
            prompt.note().as_deref(),
            Some(
                "Codex can\u{2019}t clear a multi-line draft. Clear it in the terminal, then send again."
            )
        );
    }

    #[test]
    fn prompt_texts_and_outcomes() {
        let prompt = UnsentPrompt::deliver_now(key(), Some("> Only the profile form".into()), 1);
        assert_eq!(prompt.title(), "SHOP-23 has unsent text in its terminal");
        assert_eq!(prompt.sending, "The waiting message");
        assert_eq!(
            UnsentPrompt::deliver_now(key(), None, 2).sending,
            "The 2 waiting messages"
        );
        // Keep it and cancel: nothing is sent and nothing cleared.
        assert_eq!(deliver_now_request(&key(), Some(Choice::Keep)), None);
        // Clear it and send: the service clears the line, then delivers.
        assert!(matches!(
            deliver_now_request(&key(), Some(Choice::Clear)),
            Some(Request::SessionDeliverNow(DeliverNowParams {
                clear_first: true,
                ..
            }))
        ));
        // The first click never clears.
        assert!(matches!(
            deliver_now_request(&key(), None),
            Some(Request::SessionDeliverNow(DeliverNowParams {
                clear_first: false,
                ..
            }))
        ));
    }
}
