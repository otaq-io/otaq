//! The board card's plain words: a card is at most four lines plus one action. Its key, its title, one status line,
//! who and how long; only when set, one muted note under the line (the ticket's own agent for a stage, or the stages
//! it skips). A coloured edge only while the card needs the user: turquoise asks or decides, magenta is the user's
//! review, red broke and waits. Waiting is the status line itself, with an amber hourglass ("Waits for SHOP-27 to
//! merge"). Tokens, models, rounds, Plan pieces, Run ports, decision counts and conflict details are the sheet's,
//! never the card's.
//!
//! [`words`] words the states the board draws; a state it doesn't word keeps the line the card's parts build
//! ([`super::card::status_line`]), so every state the app has keeps a card.

use otaq_core::agents::facts::OwnAgent;
use otaq_core::board::human_check::Arrival;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{AutoMergeStop, MergeCheckState, ReviewEntry, Status};
use otaq_core::signals::{ToolStop, stopped_on_error};
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::theme::Palette;
use crate::view_models::card::{
    CardAction, CardActionKind, CardExtras, Signal, StatusLine, Tone, VALIDATE, held, lowered,
    start_refusal, waits_for_an_agent,
};
use crate::view_models::dependencies::and_list;

/// A Backlog card that hasn't started.
pub const NOT_STARTED: &str = "Not started";
/// A ticket the user's Pause stopped: it picks up where it was on Resume.
pub const PAUSED_LINE: &str = "Paused \u{b7} resumes where it was";
/// A parked ticket: taken out of its stage, its work kept.
pub const PARKED_LINE: &str = "Parked \u{b7} has work in its copy";
/// The front of a line whose tool stopped on an error ("Stopped · login expired").
pub const STOPPED: &str = "Stopped";
/// A session a restart ended.
pub const STOPPED_BY_RESTART: &str = "Stopped by a restart";
/// Resuming its conversation failed.
pub const COULDNT_RESUME: &str = "Couldn\u{2019}t resume its conversation";
/// A local merge running the project's merge check.
pub const MERGING_LINE: &str = "Merging \u{b7} running the merge check";
/// Held while its tool is at its plan limit.
pub const HELD_PLAN_LIMIT: &str = "Held \u{b7} plan limit";
/// Closed without merging, or finished without a merge.
pub const DONE_NOT_MERGED_LINE: &str = "Done \u{b7} not merged";
/// The front of a line that waits for the user's review.
pub const READY_FOR_YOU: &str = "Ready for you";
/// An agent stage's agent at work.
pub const WORKING: &str = "Working";
/// An agent check's checker at work.
pub const CHECKING: &str = "Checking";
/// A PR delivery's agent reported a push the remote doesn't have.
pub const PUSH_NOT_LANDED_LINE: &str =
    "Needs you \u{b7} the branch isn\u{2019}t on the remote as expected";

/// The line and the edge [`words`] give a card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Words {
    pub line: StatusLine,
    pub edge: Option<Tone>,
}

/// The muted line under the status line, only when set: "Dev: Light instead of Strong", "Skips Product spec".
/// `CardNote` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "CardNote")]
pub struct Note {
    pub icon: IconName,
    pub text: String,
}

/// What a delivery problem's agent is fixing, in the card's words and the sheet badge's: "Merge check failed ·
/// fixing" after a failed merge check, else "Conflicts with main · fixing". `base` names the branch when the
/// conflict didn't record one.
pub fn fixing_line(flags: &otaq_core::domain::Flags, base: Option<&str>) -> String {
    if flags.merge_check == Some(MergeCheckState::Failed) {
        return otaq_core::domain::validation::MERGE_CHECK_FAILED_FIXING.to_owned();
    }
    otaq_core::domain::validation::conflicts_fixing(
        flags
            .conflicts_with
            .as_deref()
            .or(base)
            .unwrap_or("its base"),
    )
}

fn plain(text: impl Into<String>, dot: Palette) -> Option<Words> {
    Some(Words {
        line: StatusLine {
            text: text.into(),
            dot,
            tone: Tone::Secondary,
            icon: None,
        },
        edge: None,
    })
}

/// A line in the colour of the edge it comes with.
fn edged(text: impl Into<String>, dot: Palette, edge: Tone) -> Option<Words> {
    Some(Words {
        line: StatusLine {
            text: text.into(),
            dot,
            tone: edge,
            icon: None,
        },
        edge: Some(edge),
    })
}

fn asks(text: impl Into<String>) -> Option<Words> {
    edged(text, Palette::Attn, Tone::Attention)
}

fn broke(text: impl Into<String>) -> Option<Words> {
    edged(text, Palette::Error, Tone::Error)
}

fn yours(text: impl Into<String>) -> Option<Words> {
    edged(text, Palette::Review, Tone::Review)
}

/// Approve & merge or a start running its git steps (the Merge check aside, which has its own line): "Merging ·
/// running your pre-commit hook · 40 s", "Starting · making its worktree".
pub fn git_words(ticket: &Ticket, now_ms: i64) -> Option<Words> {
    if ticket.flags.merge_check == Some(MergeCheckState::Running) {
        return None;
    }
    let git = ticket.flags.git.as_ref()?;
    plain(
        crate::view_models::git_progress::card_line(git, now_ms),
        Palette::Working,
    )
}

/// Waiting is the status line itself: an amber hourglass, calm words.
pub fn waiting(text: impl Into<String>) -> Option<Words> {
    Some(Words {
        line: StatusLine {
            text: text.into(),
            dot: Palette::Paused,
            tone: Tone::Secondary,
            icon: Some(IconName::Hourglass),
        },
        edge: None,
    })
}

/// "setup command failed" → "Setup command failed".
fn capitalized(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The check the card counts for: its name, the default flow's Validate when it has none.
fn check_name(ticket: &Ticket) -> &str {
    ticket.flags.check.as_deref().unwrap_or(VALIDATE)
}

/// How a ticket waiting for the user's review got there, without its round: "passed Validate", "skipped Security".
pub fn arrived(ticket: &Ticket) -> Option<String> {
    let flags = &ticket.flags;
    if let Some(arrival) = &flags.arrival {
        return Some(match arrival {
            Arrival::Passed { stage, .. } => format!("passed {stage}"),
            Arrival::Skipped { stage, .. } => format!("skipped {stage}"),
            Arrival::FailedMovedOn { stage, .. } => {
                format!("{stage} failed \u{b7} moved on by you")
            }
            Arrival::MovedBack => "moved back by you".to_owned(),
            Arrival::Finished { stage } => format!("{stage} finished"),
        });
    }
    let check = check_name(ticket);
    Some(match flags.review_entry?.entry {
        ReviewEntry::Validated => format!("passed {check}"),
        ReviewEntry::Skipped => format!("skipped {check}"),
        ReviewEntry::FailedMoved => format!("{check} failed \u{b7} moved on by you"),
        ReviewEntry::SubmittedByUser => "moved on by you".to_owned(),
    })
}

/// The card's status line and edge in plain words; `None` keeps the line its parts build. `status` is the one the
/// card shows (a question a pause interrupted shows as that question); `paused` is a ticket the user's Pause stopped.
pub fn words(ticket: &Ticket, status: Status, x: &CardExtras, paused: bool) -> Option<Words> {
    let flags = &ticket.flags;
    if let Some(missing) = &flags.missing_skill {
        return broke(missing.text());
    }
    if flags.merge_check == Some(MergeCheckState::Running) {
        return plain(MERGING_LINE, Palette::Working);
    }
    if paused {
        return plain(PAUSED_LINE, Palette::Paused);
    }
    // A helper waiting for the user keeps the line that names the helper ("40.3 needs you: …").
    if crate::view_models::pieces::wait_line(ticket).is_some() {
        return None;
    }
    if let Some(text) = &x.blocked {
        return waiting(text.clone());
    }
    // A role held before it starts: the hold says why ("Waiting for Codex's limit…").
    if held(ticket, status, x).is_some() && !status.is_needs_you_family() {
        return None;
    }
    let question = || x.question.as_deref().map(lowered);
    match status {
        Status::Backlog => plain(NOT_STARTED, Palette::Queued),
        Status::Parked => plain(PARKED_LINE, Palette::Paused),
        Status::Queued if flags.resuming_conversation => None,
        Status::Queued if x.starting => None,
        Status::Queued => waiting(capitalized(x.waits_for.as_deref()?)),
        Status::WaitingForSlot => waiting(waits_for_an_agent(x.in_line)),
        Status::Held | Status::WaitingForLimitReset if x.held_by_limit => {
            plain(HELD_PLAN_LIMIT, Palette::Paused)
        }
        Status::WaitingForLimitReset => plain(HELD_PLAN_LIMIT, Palette::Paused),
        Status::Held => waiting(match &x.held_reason {
            Some(reason) if !reason.starts_with("Held") => {
                format!("Held \u{b7} {}", lowered(first_sentence(reason)))
            }
            Some(reason) => reason.clone(),
            None => "Held".to_owned(),
        }),
        Status::ParentClosed => plain(
            match &x.closed_parent {
                Some(parent) => format!("Parent {parent} was closed"),
                None => "A parent was closed".to_owned(),
            },
            Palette::Queued,
        ),
        Status::StartInterrupted if start_refusal(ticket).is_some() => {
            broke("Start failed \u{b7} couldn\u{2019}t run the model")
        }
        Status::StartInterrupted => broke(match &x.start_failure {
            Some(step) => format!("Start failed \u{b7} {}", lowered(step)),
            None => "Start failed".to_owned(),
        }),
        Status::Working if flags.hand_off.is_some() => None,
        Status::Working if flags.validation_round.is_some() => plain(
            format!("Fixing \u{b7} sent back by {}", check_name(ticket)),
            Palette::Working,
        ),
        Status::Working if flags.sent_back.is_some() => {
            plain("Fixing \u{b7} sent back by you", Palette::Working)
        }
        // A planning stage says what it writes ("Working · writing the plan").
        Status::Working => plain(
            match &x.working_on {
                Some(what) => format!("{WORKING} \u{b7} {what}"),
                None => WORKING.to_owned(),
            },
            Palette::Working,
        ),
        Status::WorkingInBackground => plain(
            "Working \u{b7} in the background".to_owned(),
            Palette::Working,
        ),
        Status::MergeConflictFixing if flags.hand_off.is_some() => None,
        Status::MergeConflictFixing => plain(
            fixing_line(flags, x.project_base.as_deref()),
            Palette::Working,
        ),
        Status::AsksYou => asks(match question() {
            Some(q) => format!("Asks you \u{b7} {q}"),
            None => "Asks you a question".to_owned(),
        }),
        // A helper's prompt keeps the line that names the helper.
        Status::NeedsYou if x.helper_agent => None,
        Status::NeedsYou | Status::CheckerNeedsYou => asks(match question() {
            Some(q) => format!("Asks you \u{b7} {q}"),
            None => "Asks you \u{b7} allow a command?".to_owned(),
        }),
        // Its tool stopped on an error of its own: it broke and waits, and the line says why.
        Status::Idle if let Some(error) = stopped_on_error(ticket) => broke(stopped_line(error)),
        // A turn that ended: it waits for the user's next word.
        Status::Idle => asks("Idle"),
        Status::Interrupted => broke(STOPPED_BY_RESTART),
        Status::ResumeFailed => broke(COULDNT_RESUME),
        // The delivery agent reported a push the remote doesn't have: the user decides (what the remote has is the
        // sheet's).
        Status::DeliveryStopped if ticket.auto_merge_stop == Some(AutoMergeStop::PushNotLanded) => {
            asks(PUSH_NOT_LANDED_LINE)
        }
        // A project's git hook refused Approve & merge: "Needs you · commit-msg hook refused the merge" (its output
        // and the way on are the sheet's).
        Status::DeliveryStopped if let Some(line) = super::card::hook_refusal_line(ticket) => {
            asks(line)
        }
        Status::DeliveryStopped => broke(match ticket.auto_merge_stop {
            Some(AutoMergeStop::BaseDiverged) => "Held \u{b7} base diverged".to_owned(),
            Some(AutoMergeStop::CheckoutDirty) => {
                "Can\u{2019}t merge \u{b7} checkout has changes".to_owned()
            }
            Some(AutoMergeStop::KeepsFailingMergeCheck) => "Merge check keeps failing".to_owned(),
            Some(AutoMergeStop::DeliveryFailed) => "Couldn\u{2019}t deliver".to_owned(),
            _ => format!(
                "Keeps conflicting with {}",
                flags
                    .conflicts_with
                    .as_deref()
                    .or(x.project_base.as_deref())
                    .unwrap_or("its base")
            ),
        }),
        Status::Checking => plain(CHECKING, Palette::Working),
        Status::CheckFailed => {
            let rounds = ticket.validation_round.max(1);
            let times = if rounds == 1 { "time" } else { "times" };
            asks(format!(
                "{} failed {rounds} {times} \u{b7} decide",
                check_name(ticket)
            ))
        }
        Status::CheckNoResult => asks(format!(
            "{} gave no result \u{b7} decide",
            check_name(ticket)
        )),
        Status::WaitingForYou if flags.pr_rework => yours("Back for rework"),
        // A check of planning says what it reviews ("the decisions and the plan").
        Status::WaitingForYou if let Some(what) = &x.check_line => {
            yours(format!("{READY_FOR_YOU} \u{b7} {what}"))
        }
        Status::WaitingForYou => yours(match arrived(ticket) {
            Some(how) => format!("{READY_FOR_YOU} \u{b7} {how}"),
            None => READY_FOR_YOU.to_owned(),
        }),
        Status::ToDeliver if flags.delivering_automatically => plain(
            match &x.delivery_agent {
                Some(agent) => format!("{agent} is opening the PR"),
                None => "The PR is being opened".to_owned(),
            },
            Palette::Working,
        ),
        Status::ToDeliver => yours(match arrived(ticket) {
            Some(how) => format!("{READY_FOR_YOU} \u{b7} {how}"),
            None => READY_FOR_YOU.to_owned(),
        }),
        Status::PrOpen => plain(format!("PR #{} open", flags.pr_number?), Palette::Review),
        Status::MergedInto => plain(
            format!("Merged into {}", flags.merged_into.as_ref()?),
            Palette::Approved,
        ),
        Status::Merged if flags.delivered_without_checks => None,
        Status::Merged if flags.done_without_merge => plain(
            match &flags.nothing_to_merge {
                Some(_) => super::card::DONE_NOTHING_TO_MERGE.to_owned(),
                None => DONE_NOT_MERGED_LINE.to_owned(),
            },
            Palette::Done,
        ),
        Status::Merged => plain(
            format!("Merged into {}", ticket.base.branch()),
            Palette::Approved,
        ),
        Status::ClosedNotMerged if flags.nothing_to_merge.is_some() => {
            plain(super::card::DONE_NOTHING_TO_MERGE, Palette::Done)
        }
        Status::ClosedNotMerged => plain(DONE_NOT_MERGED_LINE, Palette::Done),
    }
}

/// A card whose tool stopped on an error: "Stopped · login expired", "Stopped · API Error: 500".
pub fn stopped_line(error: &str) -> String {
    format!("{STOPPED} \u{b7} {}", ToolStop::of(error).reason())
}

/// The first sentence of a longer message ("Base diverged. Pull it…" → "Base diverged").
fn first_sentence(text: &str) -> &str {
    text.split_once(". ")
        .map_or(text, |(first, _)| first)
        .trim_end_matches('.')
}

/// The card's one action, in the board's words: Answer a question, Open a permission prompt, Retry a failed start,
/// Decide on a closed parent, Mark merged on an open PR. Any other action keeps its label.
pub fn action(status: Status, ticket: &Ticket, action: Option<CardAction>) -> Option<CardAction> {
    let open = |label: &'static str| CardAction {
        kind: CardActionKind::Open,
        label: label.into(),
        icon: None,
        primary: true,
    };
    match status {
        Status::PrOpen if ticket.flags.pr_number.is_some() => Some(CardAction {
            kind: CardActionKind::MarkMerged,
            label: "Mark merged".into(),
            icon: None,
            primary: false,
        }),
        Status::ParentClosed => Some(open(super::card::DECIDE)),
        // Nothing to answer: the sheet says what to do (sign in, then Resume).
        Status::Idle if stopped_on_error(ticket).is_some() => Some(open("Open")),
        _ => action.map(|a| match (status, a.kind) {
            (Status::NeedsYou | Status::CheckerNeedsYou, CardActionKind::Open) => open("Open"),
            (_, CardActionKind::Open) if a.label == "Reply" => open("Answer"),
            (_, CardActionKind::RetryStart) => CardAction {
                label: "Retry".into(),
                ..a
            },
            _ => a,
        }),
    }
}

/// The ticket's own agent for a stage as one line, on its card and on Overview: "Dev: Light instead of Strong".
pub fn own_agent_line(own: &OwnAgent) -> String {
    format!("{}: {} instead of {}", own.stage, own.agent, own.instead_of)
}

/// The muted note under the line, only when set: the ticket's own agent for a stage, from the moment it is set, Backlog
/// included ("Dev: Light instead of Strong"), else the stages the user chose to skip ("Skips Product spec"). Never on a
/// finished card.
pub fn note(ticket: &Ticket, x: &CardExtras) -> Option<Note> {
    if ticket.place.is_done() {
        return None;
    }
    let a = &x.agents;
    if let Some(own) = &a.own_agent {
        return Some(Note {
            icon: IconName::User,
            text: own_agent_line(own),
        });
    }
    (!a.skips.is_empty()).then(|| Note {
        icon: IconName::SkipForward,
        text: format!("Skips {}", and_list(&a.skips)),
    })
}

/// When a finished card finished: "today", "yesterday", "3 days ago" (by whole days since).
pub fn finished(ticket: &Ticket, now_ms: i64) -> Option<Signal> {
    const DAY: i64 = 24 * 60 * 60 * 1000;
    let since = ticket.status_since?;
    let days = now_ms.div_euclid(DAY) - since.div_euclid(DAY);
    Some(Signal {
        text: match days {
            ..=0 => "today".to_owned(),
            1 => "yesterday".to_owned(),
            n => format!("{n} days ago"),
        },
        stale: false,
    })
}

/// How long the card's agent (or the user's review) has been at it: since its status began ("26m", "1h"), keeping
/// whether the agent has gone quiet. Nothing while it hasn't begun or broke (Backlog, waiting, Start failed, a
/// restart), so those cards say only their line; `signal` is the time the parts gave, kept where there is no start.
pub fn how_long(
    ticket: &Ticket,
    status: Status,
    now_ms: i64,
    signal: Option<Signal>,
) -> Option<Signal> {
    let quiet = matches!(
        status,
        Status::Backlog
            | Status::Parked
            | Status::Queued
            | Status::WaitingForSlot
            | Status::ParentClosed
            | Status::StartInterrupted
            | Status::Interrupted
            | Status::ResumeFailed
    ) || (status == Status::Held && ticket.start_problem.is_some());
    if quiet {
        return None;
    }
    let stale = signal.as_ref().is_some_and(|s| s.stale);
    match ticket.status_since {
        Some(since) => Some(Signal {
            text: super::card::age_label(now_ms - since),
            stale,
        }),
        None => signal,
    }
}
