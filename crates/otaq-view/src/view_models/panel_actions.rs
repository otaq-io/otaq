//! The panel's action bar for each state (UI-04, TKT-08; brief 05, *05 · Ticket side panel → Action bar · every
//! state*; d0015, d0018, d0033, d0052, d0060).
//!
//! The primary button is always the one thing that unblocks the ticket. Actions whose features arrive in later
//! milestones are listed here with the ticket that enables them and stay **hidden** (not disabled) until then.

use otaq_core::agents::facts::TicketAgents;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::StatusSet;
use otaq_core::domain::{Kind, Status};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::delete;
use crate::view_models::epic_actions::{can_close_epic, can_move_to_epic};
use crate::view_models::menu::MenuItem;
use crate::view_models::{card, card_menu};

/// A button of the action bar. `PanelAction` on the page, which sends it back to do it (T-0222).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "PanelAction")]
pub enum Action {
    /// Backlog → Dev (⌘D).
    Start,
    Edit,
    /// Shows the Terminal tab.
    OpenTerminal,
    /// A hand-off the agent hasn't picked up (d0118): types it again, as the user's click (d0048).
    SendAgain,
    /// Run (M9, T-0134).
    Run,
    /// Needs-you family: the Terminal tab, focused.
    AnswerInTerminal,
    /// Queued and waiting for a parent: show the first parent in the panel.
    ViewParent,
    /// Interrupted (T-0121).
    Resume,
    /// Resume failed: the same resume again (REC-03).
    RetryResume,
    /// Resume failed: a new conversation in the same worktree, after a confirmation (REC-03).
    StartFromScratch,
    /// Start interrupted (T-0063).
    RetryStart,
    /// Ready for Review on the default flow, an epic in its human check, a To deliver ticket: the delivery choices
    /// (M6, T-0095).
    Approve,
    /// Ready for Review on the default flow (M6, T-0094): the Send back dialog.
    SendBack,
    /// Your check (FLW-14): Pass moves it on, or offers the delivery choices when a click Delivery comes next.
    Pass,
    /// Your check: the Fail dialog (FLW-14; was Send back, M6, T-0094).
    Fail,
    /// PR open (FLW-16): the "Send SHOP-26 back with PR comments?" dialog.
    SendBackPr,
    /// An epic's PR open (FLW-16): a Backlog child holding the PR's unresolved comments.
    AddChildFromPr,
    /// PR open (M10, T-0150).
    OpenPrInBrowser,
    MarkMerged,
    MarkClosed,
    /// Done (M6, T-0091).
    ViewDiff,
    /// Closed (M6, T-0106).
    Reopen,
    /// Validating…: the Terminal tab on the validator's side (d0063, T-0237).
    OpenValidator,
    /// Needs you · validator: the turquoise "Answer in terminal · Validator".
    AnswerValidator,
    /// Validation failed N times or ended without a result: Ready for Review without a pass (VAL-07).
    MoveOnAnyway,
    /// Validation failed N times: the findings go to the agent with one more round.
    AnotherRound,
    /// Validation failed N times: the user's note goes with the findings (opens the note box).
    AnswerAgent,
    /// Validation ended without a result: a fresh validator.
    RetryValidation,
    /// Auto-merge stopped: the merge again (AMG-03).
    RetryMerge,
    /// Auto-merge stopped: Ready for Review, not merged again (AMG-04).
    MoveToReview,
    /// An epic whose lead ended (by restart, or its agent tool exited): the same conversation (LEAD-12, d0083).
    /// Added by the epic sheet's view, not by [`all_buttons`].
    ResumeLead,
    /// A model the tool refused (AGT-05: Start failed, or a check's No result on the model): Settings → Agents on
    /// that agent's page (d0108 second gap review rule 7; replaces Change agent, Q280).
    OpenAgent,
    /// "Merging · running the merge check" (d0120, Q475): the check ends and nothing is merged.
    StopMergeCheck,
}

/// How a button looks. `PanelButtonLook` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "PanelButtonLook")]
pub enum Look {
    Primary,
    Secondary,
    /// The turquoise "Answer in terminal".
    Attention,
    /// Mark closed.
    Danger,
}

impl Action {
    /// The button's tooltip, where the label alone doesn't say where it goes.
    pub fn tooltip(self) -> Option<&'static str> {
        match self {
            Self::Start => Some("Moves to Ready for Dev"),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Edit => "Edit",
            Self::OpenTerminal => "Open terminal",
            Self::SendAgain => "Send again",
            Self::Run => "Run",
            Self::AnswerInTerminal => "Answer in terminal",
            Self::ViewParent => "View parent",
            Self::Resume => "Resume",
            Self::RetryResume => "Retry",
            Self::StartFromScratch => "Start from scratch",
            Self::RetryStart => "Retry start",
            Self::Approve => "Approve · Local merge",
            Self::SendBack => "Send back",
            Self::Pass => crate::view_models::card::PASS,
            Self::Fail => crate::view_models::card::FAIL,
            Self::SendBackPr => crate::view_models::card::SEND_BACK_WITH_PR_COMMENTS,
            Self::AddChildFromPr => "Add a child from PR comments",
            Self::OpenPrInBrowser => "Open PR in browser",
            Self::MarkMerged => "Mark merged",
            Self::MarkClosed => "Mark closed",
            Self::ViewDiff => "View diff",
            Self::Reopen => "Reopen",
            Self::OpenValidator => "Open terminal \u{b7} Validator",
            Self::AnswerValidator => "Answer in terminal \u{b7} Validator",
            Self::MoveOnAnyway => "Move on anyway",
            Self::AnotherRound => "Give it another round",
            Self::AnswerAgent => "Answer the agent\u{2026}",
            Self::RetryValidation => "Retry validation",
            Self::RetryMerge => "Retry merge",
            Self::MoveToReview => "Move to Review",
            Self::ResumeLead => "Resume lead",
            Self::OpenAgent => "Open agent\u{2026}",
            Self::StopMergeCheck => crate::view_models::card::STOP,
        }
    }

    pub fn icon(self) -> Option<IconName> {
        Some(match self {
            Self::Start => IconName::Play,
            Self::Edit => IconName::Pencil,
            Self::OpenTerminal
            | Self::AnswerInTerminal
            | Self::OpenValidator
            | Self::AnswerValidator => IconName::SquareTerminal,
            // *Action bar · every state → Validation failed 3 times, … ended without a result, Auto-merge stopped*.
            Self::MoveOnAnyway | Self::MoveToReview => IconName::Eye,
            Self::AnotherRound | Self::RetryValidation | Self::RetryMerge => IconName::RotateCcw,
            Self::AnswerAgent => IconName::MessageSquare,
            Self::SendAgain => IconName::Send,
            Self::Run => IconName::Play,
            Self::ViewParent => IconName::Link2,
            Self::Resume => IconName::Play,
            // Resume failed: Retry and Start from scratch (*16 → Resume · in progress → resumed → failed*).
            Self::RetryResume => IconName::RefreshCw,
            Self::RetryStart | Self::StartFromScratch => IconName::RotateCcw,
            Self::SendBack | Self::Fail | Self::SendBackPr => IconName::Undo2,
            Self::AddChildFromPr => IconName::Plus,
            Self::OpenPrInBrowser => IconName::ExternalLink,
            Self::MarkMerged => IconName::GitMerge,
            Self::ViewDiff => IconName::FileDiff,
            Self::Reopen | Self::ResumeLead => IconName::RotateCcw,
            Self::StopMergeCheck => IconName::Square,
            // *05 → Start failed on the model*, *No result: the validator's model*: labels only.
            Self::Approve | Self::Pass | Self::MarkClosed | Self::OpenAgent => {
                return None;
            }
        })
    }

    /// `None` when it works in this build; otherwise the ticket that wires it (until then it is hidden).
    pub fn arrives_with(self) -> Option<&'static str> {
        match self {
            Self::Start
            | Self::Edit
            | Self::OpenTerminal
            | Self::SendAgain
            | Self::AnswerInTerminal
            | Self::ViewParent
            | Self::RetryStart
            | Self::ViewDiff
            | Self::SendBack
            | Self::Fail
            | Self::Pass
            | Self::SendBackPr
            | Self::AddChildFromPr
            | Self::Approve => None,
            Self::Run => Some("T-0134"),
            Self::Resume | Self::RetryResume | Self::StartFromScratch => None,
            Self::OpenPrInBrowser | Self::MarkMerged | Self::MarkClosed => None,
            Self::Reopen | Self::ResumeLead => None,
            Self::OpenAgent | Self::StopMergeCheck => None,
            Self::OpenValidator
            | Self::AnswerValidator
            | Self::MoveOnAnyway
            | Self::AnotherRound
            | Self::AnswerAgent
            | Self::RetryValidation
            | Self::RetryMerge
            | Self::MoveToReview => None,
        }
    }
}

/// What Send again types, as the unsent-text question names it: "The hand-off again (Sent back)".
pub fn send_again_sending(kind: otaq_core::domain::HandOffKind) -> String {
    format!("The hand-off again ({})", card::hand_off_label(kind))
}

/// The Approve button's label (DEC-05, d0084): "Approve · keeps 1 decision" / "Approve · keeps 3 decisions" while
/// the review has open recorded decisions; with none it reads as before, "Approve · Local merge".
pub fn approve_label(open_decisions: u32) -> String {
    match open_decisions {
        0 => Action::Approve.label().to_owned(),
        1 => "Approve \u{b7} keeps 1 decision".to_owned(),
        n => format!("Approve \u{b7} keeps {n} decisions"),
    }
}

/// The Pass button's label (FLW-14, DEC-05): "Pass", or "Pass · keeps 1 decision" while the review has open recorded
/// decisions.
pub fn pass_label(open_decisions: u32) -> String {
    match open_decisions {
        0 => Action::Pass.label().to_owned(),
        1 => "Pass \u{b7} keeps 1 decision".to_owned(),
        n => format!("Pass \u{b7} keeps {n} decisions"),
    }
}

/// The Fail button's label: "Fail", or "Fail (2)" with the pending review comments it sends (as Send back's, REV-04).
pub fn fail_label(pending: usize) -> String {
    match pending {
        0 => Action::Fail.label().to_owned(),
        n => format!("{} ({n})", Action::Fail.label()),
    }
}

/// With open decisions Local merge / Open PR move into the ▾ menu, so the button opens that choice instead of the
/// Local merge confirmation.
pub fn approve_opens_menu(open_decisions: u32) -> bool {
    open_decisions > 0
}

/// The bar's buttons for a ticket on the default flow, in order, with how each looks. Includes the ones that arrive
/// later.
pub fn all_buttons(ticket: &Ticket) -> Vec<(Action, Look)> {
    all_buttons_in(ticket, false)
}

/// [`all_buttons`] in a flow that isn't the default's shape (`custom`, `flow_columns::keeps_default_look`): Pass ·
/// Fail on a human check (FLW-14). The default flow keeps today's Approve and Send back (d0108, STG-06). Every flow
/// offers Send back with PR comments on an open PR and an epic's Add a child from PR comments (FLW-16).
pub fn all_buttons_in(ticket: &Ticket, custom: bool) -> Vec<(Action, Look)> {
    use Action as A;
    use Look::*;
    let s = ticket.status;
    // While Local merge runs the Merge check, Stop is all there is to do (d0120): the merge finishes on its own.
    if ticket.flags.merge_check == Some(otaq_core::domain::MergeCheckState::Running) {
        return vec![(A::StopMergeCheck, Secondary)];
    }
    if ticket.kind == Kind::Epic {
        // No Start on an epic: its children start (d0059). Its review flow is T-0109.
        // By the kind of stage it is in (STG-08): its Backlog, its human check, its Delivery, Done.
        return match crate::view_models::flow_columns::place_set(ticket) {
            StatusSet::Backlog => vec![(A::Edit, Secondary)],
            // A delivery unit in its human check: Approve, never Send back (EPC-05).
            StatusSet::HumanCheck => vec![(A::Approve, Primary)],
            // The epic's PR (EPC-08): the same manual outcomes as a ticket's (REV-13), and its comments as a child
            // (FLW-16).
            StatusSet::Delivery if s == Status::PrOpen => vec![
                (A::OpenPrInBrowser, Secondary),
                (A::AddChildFromPr, Secondary),
                (A::MarkMerged, Primary),
                (A::MarkClosed, Danger),
            ],
            StatusSet::Done if s == Status::ClosedNotMerged => vec![(A::Reopen, Secondary)],
            _ => Vec::new(),
        };
    }
    // A hand-off typed in that the agent hasn't picked up (d0118): Send again first.
    if card::pending_hand_off(ticket, s).is_some() {
        let mut bar = vec![(A::SendAgain, Primary), (A::OpenTerminal, Secondary)];
        if s == Status::Working {
            bar.push((A::Run, Secondary));
        }
        return bar;
    }
    match s {
        Status::Backlog => vec![(A::Start, Primary), (A::Edit, Secondary)],
        Status::Parked => vec![(A::Start, Primary), (A::Edit, Secondary)],
        // Held because the base diverged: once pushed or pulled, Retry start (d0059).
        Status::Held
            if ticket
                .start_problem
                .as_ref()
                .is_some_and(|p| p.step == otaq_core::domain::StartStep::SyncBase) =>
        {
            vec![(A::RetryStart, Primary), (A::Edit, Secondary)]
        }
        Status::Queued | Status::WaitingForSlot | Status::Held | Status::ParentClosed => {
            let mut bar = vec![(A::Edit, Secondary)];
            if !ticket.depends_on.is_empty() {
                bar.push((A::ViewParent, Secondary));
            }
            bar
        }
        // Start failed on the model (update Y §3): Open agent…, and Retry start beside it.
        Status::StartInterrupted if card::start_refusal(ticket).is_some() => {
            vec![(A::OpenAgent, Primary), (A::RetryStart, Secondary)]
        }
        Status::StartInterrupted => vec![(A::RetryStart, Primary)],
        Status::Working | Status::WorkingInBackground | Status::WaitingForLimitReset => {
            vec![(A::OpenTerminal, Secondary), (A::Run, Secondary)]
        }
        Status::AsksYou | Status::NeedsYou | Status::Idle => vec![(A::AnswerInTerminal, Attention)],
        Status::Interrupted => vec![(A::Resume, Primary)],
        // *16 → Resume · in progress → resumed → failed*: Start from scratch is the primary, Retry beside it.
        Status::ResumeFailed => vec![(A::RetryResume, Secondary), (A::StartFromScratch, Primary)],
        // Your check (FLW-14): Pass offers today's delivery choices when a click Delivery comes next.
        Status::WaitingForYou if custom => vec![(A::Pass, Primary), (A::Fail, Secondary)],
        Status::WaitingForYou => vec![(A::Approve, Primary), (A::SendBack, Secondary)],
        // Waits for the Delivery click (not on the default flow): Local merge or Open PR.
        Status::ToDeliver => vec![(A::Approve, Primary)],
        Status::PrOpen => {
            // Epic children never offer the PR actions; they merge locally (d0059).
            if ticket.epic.is_some() {
                Vec::new()
            } else {
                vec![
                    (A::OpenPrInBrowser, Secondary),
                    (A::SendBackPr, Secondary),
                    (A::MarkMerged, Primary),
                    (A::MarkClosed, Danger),
                ]
            }
        }
        Status::Merged => vec![(A::ViewDiff, Secondary)],
        Status::ClosedNotMerged => vec![(A::Reopen, Secondary)],
        // The M12 rows (d0063, d0064): Stop this check and Skip this check are in the "…" menu.
        Status::Checking => vec![(A::OpenValidator, Secondary), (A::Run, Secondary)],
        Status::CheckerNeedsYou => vec![(A::AnswerValidator, Attention)],
        Status::CheckFailed => vec![
            (A::MoveOnAnyway, Primary),
            (A::AnotherRound, Secondary),
            (A::AnswerAgent, Secondary),
        ],
        // No result on the validator's model: Open agent… first, then today's two.
        Status::CheckNoResult if card::validator_refusal(ticket).is_some() => vec![
            (A::OpenAgent, Primary),
            (A::RetryValidation, Secondary),
            (A::MoveOnAnyway, Secondary),
        ],
        Status::CheckNoResult => vec![(A::RetryValidation, Primary), (A::MoveOnAnyway, Secondary)],
        Status::DeliveryStopped => vec![(A::RetryMerge, Primary), (A::MoveToReview, Secondary)],
        // The agent fixes the Auto-merge conflict in its own terminal.
        Status::MergeConflictFixing => vec![(A::OpenTerminal, Secondary)],
    }
}

/// The callout above the bar when the tool refused a model (AGT-05; *05 → Start failed on the model (SHOP-48)*,
/// *No result: the validator's model (SHOP-29)*): what couldn't run, and the tool's own message in quotes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// "Claude Code couldn't run claude-fable-5-1", "The validator couldn't run claude-fable-5-1 · not counted as a
    /// round".
    pub heading: String,
    pub message: String,
    /// Start failed: red; No result: the needs-you family's turquoise (d0077).
    pub error: bool,
}

/// `agents`: the ticket's rows and sessions (`ticket.agents`); the refused Developer session names the model.
pub fn refusal(ticket: &Ticket, agents: Option<&TicketAgents>) -> Option<Refusal> {
    let quoted = |m: &str| format!("\u{201c}{}\u{201d}", m.trim());
    if let Some(message) = card::start_refusal(ticket) {
        return Some(Refusal {
            heading: card::couldnt_run(agents.and_then(refused_model)),
            message: quoted(message),
            error: true,
        });
    }
    let (model, message) = card::validator_refusal(ticket)?;
    Some(Refusal {
        heading: format!(
            "The validator couldn\u{2019}t run {} \u{b7} not counted as a round",
            model.as_deref().unwrap_or("its model")
        ),
        message: quoted(&message),
        error: false,
    })
}

/// The model of the ticket's last ended Developer session (the one the tool refused): the exact model the tool
/// reported, else the model as passed.
fn refused_model(agents: &TicketAgents) -> Option<&str> {
    agents
        .sessions
        .iter()
        .rev()
        .find(|s| s.role == otaq_core::agents::SessionRole::Developer && !s.running)
        .and_then(|s| s.model_ran.as_deref().or(s.model.as_deref()))
}

/// The buttons shown now: the ones whose features exist in this build.
pub fn buttons(ticket: &Ticket) -> Vec<(Action, Look)> {
    all_buttons(ticket)
        .into_iter()
        .filter(|(a, _)| a.arrives_with().is_none())
        .collect()
}

pub const EDIT_TICKET: &str = "edit-ticket";
pub const COPY_BRANCH: &str = "copy-branch";
pub const COPY_ATTACH: &str = "copy-attach";
pub const STOP_VALIDATION: &str = "stop-validation";
pub const SKIP_TO_REVIEW: &str = "skip-to-review";

/// Validating… and Needs you · validator: the validator runs (VAL-06).
pub fn validating(ticket: &Ticket) -> bool {
    matches!(ticket.status, Status::Checking | Status::CheckerNeedsYou)
}

/// The "…" menu: Stop this check and Skip this check while it validates (d0063), Edit ticket… (the fields the
/// sheet's details line doesn't cover, SHT-01), Pin / Unpin, Copy
/// branch name, Copy attach command (with a session), Move to next stage (hidden while it validates), Move to epic…, Close epic…, Close ticket…, and Delete… (enabled only for
/// never-started tickets, d0060).
pub fn menu(
    ticket: &Ticket,
    has_epics: bool,
    has_branch: bool,
    has_session: bool,
) -> Vec<MenuItem> {
    let mut items = Vec::new();
    if validating(ticket) {
        items.push(
            MenuItem::new(STOP_VALIDATION, "Stop this check")
                .icon(IconName::Square)
                .hint("round not counted"),
        );
        items.push(
            MenuItem::new(SKIP_TO_REVIEW, "Skip this check")
                .icon(IconName::SkipForward)
                .hint("never auto-merged"),
        );
    }
    let edit = MenuItem::new(EDIT_TICKET, "Edit ticket\u{2026}").icon(IconName::Pencil);
    items.push(if items.is_empty() {
        edit
    } else {
        edit.separated()
    });
    let pin = if ticket.flags.is_pinned() {
        MenuItem::new(card_menu::UNPIN, "Unpin").icon(IconName::PinOff)
    } else {
        MenuItem::new(card_menu::PIN, "Pin to top").icon(IconName::Pin)
    };
    items.push(pin);
    if has_branch {
        items.push(MenuItem::new(COPY_BRANCH, "Copy branch name").icon(IconName::GitBranch));
    }
    if has_session {
        items.push(MenuItem::new(COPY_ATTACH, "Copy attach command").icon(IconName::Copy));
    }
    if let Some(submit) = crate::view_models::close::submit_item(ticket) {
        items.push(submit);
    }
    if has_epics && can_move_to_epic(ticket) {
        items.push(MenuItem::new(card_menu::MOVE_TO_EPIC, "Move to epic…").icon(IconName::Layers));
    }
    if can_close_epic(ticket) {
        items.push(
            MenuItem::new(card_menu::CLOSE_EPIC, "Close epic…")
                .icon(IconName::CircleSlash)
                .danger()
                .separated(),
        );
    }
    items.extend(crate::view_models::close::close_item(ticket));
    items.push(delete::menu_item(ticket));
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use Action as A;

    fn shown(status: Status) -> Vec<Action> {
        buttons(&ticket("SHOP-1", "t", status))
            .into_iter()
            .map(|(a, _)| a)
            .collect()
    }

    /// d0120: while Local merge runs the Merge check, the sheet's only button is Stop (Approve can't run twice).
    #[test]
    fn a_running_merge_check_offers_only_stop() {
        for status in [Status::WaitingForYou, Status::ToDeliver] {
            let mut t = ticket("SHOP-1", "t", status);
            t.flags.merge_check = Some(otaq_core::domain::MergeCheckState::Running);
            assert_eq!(buttons(&t), [(A::StopMergeCheck, Look::Secondary)]);
            assert_eq!(
                all_buttons_in(&t, true),
                [(A::StopMergeCheck, Look::Secondary)]
            );
        }
        assert_eq!(A::StopMergeCheck.label(), "Stop");
        let mut t = ticket("SHOP-1", "t", Status::MergeConflictFixing);
        t.flags.merge_check = Some(otaq_core::domain::MergeCheckState::Failed);
        assert_eq!(
            buttons(&t),
            [(A::OpenTerminal, Look::Secondary)],
            "the agent fixes it"
        );
    }

    fn all(status: Status) -> Vec<(Action, Look)> {
        all_buttons(&ticket("SHOP-1", "t", status))
    }

    /// DEC-05: the label for 0, 1 and 3 open decisions; with any, Local merge and Open PR are the ▾ menu's choices.
    #[test]
    fn approve_says_what_it_keeps() {
        use crate::view_models::approve::{ApproveChoice, options};
        assert_eq!(approve_label(0), "Approve \u{b7} Local merge");
        assert_eq!(approve_label(1), "Approve \u{b7} keeps 1 decision");
        assert_eq!(approve_label(3), "Approve \u{b7} keeps 3 decisions");
        assert!(
            !approve_opens_menu(0),
            "as before: Local merge's confirmation"
        );
        assert!(approve_opens_menu(1) && approve_opens_menu(3));
        let shop23 = ticket("SHOP-23", "Split user full name", Status::WaitingForYou);
        let menu: Vec<_> = options(&shop23, "main", &[], None)
            .into_iter()
            .map(|o| o.choice)
            .collect();
        assert_eq!(menu, [ApproveChoice::LocalMerge, ApproveChoice::OpenPr]);
    }

    /// Every row of *Action bar · every state*, as designed (later features included).
    #[test]
    fn every_row_of_the_frame() {
        use Look::*;
        assert_eq!(
            all(Status::Backlog),
            [(A::Start, Primary), (A::Edit, Secondary)]
        );
        assert_eq!(
            all(Status::Working),
            [(A::OpenTerminal, Secondary), (A::Run, Secondary)]
        );
        for s in [Status::NeedsYou, Status::AsksYou, Status::Idle] {
            assert_eq!(all(s), [(A::AnswerInTerminal, Attention)]);
        }
        let mut waiting = ticket("SHOP-20", "t", Status::Queued);
        waiting.depends_on = vec!["SHOP-18".parse().unwrap()];
        assert_eq!(
            all_buttons(&waiting),
            [(A::Edit, Secondary), (A::ViewParent, Secondary)]
        );
        assert_eq!(all(Status::Interrupted), [(A::Resume, Primary)]);
        assert_eq!(all(Status::StartInterrupted), [(A::RetryStart, Primary)]);
        assert_eq!(
            all(Status::WaitingForYou),
            [(A::Approve, Primary), (A::SendBack, Secondary)]
        );
        assert_eq!(
            all(Status::PrOpen),
            [
                (A::OpenPrInBrowser, Secondary),
                (A::SendBackPr, Secondary),
                (A::MarkMerged, Primary),
                (A::MarkClosed, Danger)
            ]
        );
        assert_eq!(all(Status::Merged), [(A::ViewDiff, Secondary)]);
        assert_eq!(all(Status::ClosedNotMerged), [(A::Reopen, Secondary)]);
    }

    /// FLW-14, FLW-16 (`s03-46`): in another flow a human check offers Pass · Fail; an open PR Send back with PR
    /// comments and an epic's open PR Add a child from PR comments on every flow.
    #[test]
    fn other_flows_pass_fail_and_send_back_pr_comments() {
        use Look::*;
        let custom = |t: &Ticket| all_buttons_in(t, true);
        let review = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        assert_eq!(custom(&review), [(A::Pass, Primary), (A::Fail, Secondary)]);
        let mut pr = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        pr.flags.pr_number = Some(481);
        assert_eq!(
            custom(&pr),
            [
                (A::OpenPrInBrowser, Secondary),
                (A::SendBackPr, Secondary),
                (A::MarkMerged, Primary),
                (A::MarkClosed, Danger)
            ]
        );
        let mut epic = ticket("SHOP-40", "Wishlist", Status::PrOpen);
        epic.kind = Kind::Epic;
        epic.place = otaq_core::domain::Place::stage_of("deliver");
        assert_eq!(custom(&epic)[1], (A::AddChildFromPr, Secondary));
        assert!(all_buttons(&epic).contains(&(A::AddChildFromPr, Secondary)));
        assert_eq!(A::Pass.label(), "Pass");
        assert_eq!(A::SendBackPr.label(), "Send back with PR comments");
    }

    #[test]
    fn later_features_are_hidden_not_disabled() {
        assert_eq!(shown(Status::Backlog), [A::Start, A::Edit]);
        assert_eq!(
            shown(Status::Working),
            [A::OpenTerminal],
            "Run comes with M9"
        );
        assert_eq!(shown(Status::WaitingForYou), [A::Approve, A::SendBack]);
        assert_eq!(
            shown(Status::PrOpen),
            [
                A::OpenPrInBrowser,
                A::SendBackPr,
                A::MarkMerged,
                A::MarkClosed
            ],
            "M10, FLW-16"
        );
        assert_eq!(shown(Status::Interrupted), [A::Resume]);
        assert_eq!(
            shown(Status::ResumeFailed),
            [A::RetryResume, A::StartFromScratch]
        );
        assert_eq!(shown(Status::NeedsYou), [A::AnswerInTerminal]);
        assert_eq!(shown(Status::StartInterrupted), [A::RetryStart]);
        // *16 → Resume · in progress → resumed → failed*; Retry start as on the 05 action bar.
        assert_eq!(A::RetryResume.icon(), Some(IconName::RefreshCw));
        assert_eq!(
            buttons(&ticket(
                "BILL-4",
                "Invoice PDF export",
                Status::ResumeFailed
            )),
            [
                (A::RetryResume, Look::Secondary),
                (A::StartFromScratch, Look::Primary)
            ],
            "Start from scratch is the primary"
        );
        assert_eq!(A::StartFromScratch.icon(), Some(IconName::RotateCcw));
        assert_eq!(A::RetryStart.icon(), Some(IconName::RotateCcw));
    }

    #[test]
    fn a_diverged_hold_offers_retry_start() {
        let mut held = ticket("BILL-6", "Invoices", Status::Held);
        assert_eq!(buttons(&held), [(A::Edit, Look::Secondary)]);
        held.start_problem = Some(otaq_core::board::tickets::StartProblem {
            step: otaq_core::domain::StartStep::SyncBase,
            message: "`develop` and `origin/develop` have diverged. Push or pull, then retry"
                .into(),
        });
        assert_eq!(buttons(&held)[0], (A::RetryStart, Look::Primary));
    }

    #[test]
    fn epics_and_children_follow_their_rules() {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::Backlog);
        epic.kind = Kind::Epic;
        assert_eq!(
            all_buttons(&epic),
            [(A::Edit, Look::Secondary)],
            "no Start on an epic"
        );
        let mut child = ticket("SHOP-40.2", "Wishlist page", Status::PrOpen);
        child.epic = Some("SHOP-40".parse().unwrap());
        assert!(
            all_buttons(&child).is_empty(),
            "children never offer PR actions"
        );
        let child = ticket("SHOP-40.1", "Wishlist API", Status::Backlog);
        assert_eq!(buttons(&child)[0].0, A::Start);
    }

    #[test]
    fn menu_delete_only_for_never_started() {
        let fresh = ticket("SHOP-21", "Order confirmation email", Status::Backlog);
        let items = menu(&fresh, false, false, false);
        let delete = items.last().unwrap();
        assert_eq!(delete.label, "Delete…");
        assert!(!delete.disabled);
        assert_eq!(
            delete.hint.as_deref(),
            Some("Never started · nothing on disk")
        );

        let mut started = ticket("SHOP-19", "Price table", Status::Working);
        started.started_at = Some(1);
        let items = menu(&started, false, true, true);
        let delete = items.last().unwrap();
        assert!(delete.disabled);
        assert_eq!(
            delete.hint.as_deref(),
            Some("Started tickets can only be closed.")
        );
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Edit ticket\u{2026}",
                "Pin to top",
                "Copy branch name",
                "Copy attach command",
                "Move to next stage…",
                "Close ticket…",
                "Delete…"
            ]
        );
        assert!(items[4].disabled, "mid-turn");
    }

    /// The M12 rows of *Action bar · every state* (d0063, d0064).
    #[test]
    fn the_validation_rows() {
        use Look::*;
        assert_eq!(
            all(Status::Checking),
            [(A::OpenValidator, Secondary), (A::Run, Secondary)]
        );
        assert_eq!(shown(Status::Checking), [A::OpenValidator]);
        assert_eq!(
            all(Status::CheckerNeedsYou),
            [(A::AnswerValidator, Attention)]
        );
        assert_eq!(
            all(Status::CheckFailed),
            [
                (A::MoveOnAnyway, Primary),
                (A::AnotherRound, Secondary),
                (A::AnswerAgent, Secondary)
            ]
        );
        assert_eq!(
            all(Status::CheckNoResult),
            [(A::RetryValidation, Primary), (A::MoveOnAnyway, Secondary)]
        );
        assert_eq!(
            all(Status::DeliveryStopped),
            [(A::RetryMerge, Primary), (A::MoveToReview, Secondary)]
        );
        assert_eq!(A::OpenValidator.label(), "Open terminal \u{b7} Validator");
        assert_eq!(A::AnswerAgent.label(), "Answer the agent\u{2026}");
        assert_eq!(A::MoveOnAnyway.icon(), Some(IconName::Eye));
    }

    /// While it validates: Stop this check and Skip this check first; both submit items hidden.
    #[test]
    fn the_menu_while_validating() {
        let mut t = ticket("SHOP-29", "Coupon field", Status::Checking);
        t.started_at = Some(1);
        let items = menu(&t, false, true, false);
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Stop this check",
                "Skip this check",
                "Edit ticket\u{2026}",
                "Pin to top",
                "Copy branch name",
                "Close ticket…",
                "Delete…"
            ]
        );
        assert_eq!(items[0].hint.as_deref(), Some("round not counted"));
        assert_eq!(items[1].hint.as_deref(), Some("never auto-merged"));
        assert!(items[2].separated);
        let mut idle = ticket("SHOP-19", "Price table", Status::Idle);
        idle.started_at = Some(1);
        let items = menu(&idle, false, false, false);
        assert_eq!(items[0].label, "Edit ticket\u{2026}");
        assert!(!items[0].separated);
        assert!(
            items
                .iter()
                .any(|i| i.label == "Move to next stage…" && !i.disabled)
        );
        t.status = Status::CheckerNeedsYou;
        assert_eq!(menu(&t, false, true, false)[0].id, STOP_VALIDATION);
    }

    /// SHOP-48's facts after the refusal: one ended Developer session on Fable test, `fable` → `claude-fable-5-1`.
    fn refused_session() -> TicketAgents {
        use otaq_core::agents::store::SessionAgentRun;
        TicketAgents {
            key: Some("SHOP-48".parse().unwrap()),
            kind: otaq_core::domain::Kind::Ticket,
            status: Some(Status::StartInterrupted),
            agents: Vec::new(),
            stages: Vec::new(),
            lead: None,
            sessions: vec![SessionAgentRun {
                id: "s1".into(),
                role: otaq_core::agents::SessionRole::Developer,
                running: false,
                started_at: 1,
                ended_at: Some(2),
                tool: None,
                agent_id: Some("fable".into()),
                agent_name: Some("Fable test".into()),
                model: Some("fable".into()),
                model_ran: Some("claude-fable-5-1".into()),
                effort: None,
                tokens: None,
                round_outcome: None,
                moved_on: None,
            }],
            tokens: Vec::new(),
            holds: Vec::new(),
            place: None,
            path: Vec::new(),
            runs: Vec::new(),
            choices: Vec::new(),
        }
    }

    /// AGT-05, update Y §3, d0108 second gap review rule 7: Start failed on the model offers Open agent… and Retry
    /// start; No result on the validator's model Open agent…, Retry validation and Move on anyway; each with its
    /// callout.
    #[test]
    fn a_refused_model_opens_the_agent() {
        use crate::view_models::card_fixtures::{MODEL_ISSUE_MESSAGE, agent_variants};
        let fixture = |name: &str| {
            agent_variants()
                .into_iter()
                .find(|v| v.name == name)
                .unwrap()
                .ticket
        };
        let start = fixture("Start failed on the model");
        assert_eq!(
            buttons(&start),
            [
                (A::OpenAgent, Look::Primary),
                (A::RetryStart, Look::Secondary)
            ]
        );
        assert_eq!(
            refusal(&start, None).unwrap().heading,
            "Claude Code couldn\u{2019}t run the model",
            "no session to name it"
        );
        let r = refusal(&start, Some(&refused_session())).unwrap();
        assert_eq!(
            r.heading,
            "Claude Code couldn\u{2019}t run claude-fable-5-1"
        );
        assert_eq!(r.message, format!("\u{201c}{MODEL_ISSUE_MESSAGE}\u{201d}"));
        assert!(r.error);
        let no_result = fixture("No result \u{b7} the validator\u{2019}s model");
        assert_eq!(
            buttons(&no_result),
            [
                (A::OpenAgent, Look::Primary),
                (A::RetryValidation, Look::Secondary),
                (A::MoveOnAnyway, Look::Secondary),
            ]
        );
        let r = refusal(&no_result, None).unwrap();
        assert_eq!(
            r.heading,
            "The validator couldn\u{2019}t run claude-fable-5-1 \u{b7} not counted as a round"
        );
        assert!(!r.error);
        assert_eq!(A::OpenAgent.label(), "Open agent\u{2026}");
        // Other start problems and No results: as before, no callout.
        assert_eq!(shown(Status::StartInterrupted), [A::RetryStart]);
        assert_eq!(
            shown(Status::CheckNoResult),
            [A::RetryValidation, A::MoveOnAnyway]
        );
        assert_eq!(
            refusal(&ticket("SHOP-28", "t", Status::StartInterrupted), None),
            None
        );
    }

    /// Send again leads the bar while a hand-off waits for the agent (d0118), for each kind; gone once picked up.
    #[test]
    fn send_again_while_a_hand_off_waits_for_the_agent() {
        use otaq_core::domain::HandOffKind;
        for kind in HandOffKind::ALL {
            let status = match kind {
                HandOffKind::MergeConflict | HandOffKind::MergeCheck => Status::MergeConflictFixing,
                _ => Status::Working,
            };
            let mut t = ticket("SHOP-18", "t", status);
            t.flags.hand_off = Some(kind);
            let bar = buttons(&t);
            assert_eq!(bar[0], (A::SendAgain, Look::Primary), "{kind:?}");
            assert!(bar.iter().any(|(a, _)| *a == A::OpenTerminal), "{kind:?}");
            t.flags.hand_off = None;
            assert!(
                !buttons(&t).iter().any(|(a, _)| *a == A::SendAgain),
                "{kind:?}"
            );
        }
        assert_eq!(A::SendAgain.label(), "Send again");
        assert_eq!(
            send_again_sending(HandOffKind::SendBack),
            "The hand-off again (Sent back)"
        );
    }
}
