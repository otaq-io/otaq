//! The review dialogs as the Tauri page draws them (T-0226; REV-04, REV-06, REV-08, GIT-03, GIT-04, GIT-05, UI-02;
//! d0015, d0017, d0034, d0038, d0043, d0050, d0051): [`approve`] and [`send_back`] turned into plain, serialized
//! shapes. *07 → Send back · review & confirm, Local merge · confirm / success / merging / conflict / refused,
//! Approve · changed since review, Uncommitted files · refused, Base branch · SHOP-31 on release/2.3*.
//!
//! The page keeps only which step shows, the edited message and note, and the click.

use otaq_core::board::comments::ReviewComment;
use otaq_core::board::review::ApproveCheck;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::TicketKey;
use otaq_core::git::merge::HookChanges;
use otaq_core::review::LocalMergeOutcome;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::keymap::Platform;
use crate::view_models::approve::{self as ap, MergeView};
use crate::view_models::board_page::ApproveMenu;
use crate::view_models::card::pr_provider_label;
use crate::view_models::pr_dialog;
use crate::view_models::send_back as sb;
use crate::view_models::ticket_comments::CommentBoxView;

/// "Esc", "⌘↵": the keys the dialogs' footers name.
fn submit_keys() -> &'static str {
    match Platform::current() {
        Platform::Mac => "\u{2318}\u{21b5}",
        _ => "Ctrl+\u{21b5}",
    }
}

/// *Local merge · confirm*: where it merges, the proposed squash message, who keeps waiting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LocalMergeConfirm")]
pub struct ConfirmView {
    /// "Merge SHOP-18 into main".
    pub title: String,
    /// "Squashed into one commit on main. Your folder fast-forwards if main is checked out and clean."
    pub lead: String,
    /// "Prepared by Strong while SHOP-18 waited in Review. Edit it if you like."
    pub prepared: Option<String>,
    /// "Strong is still writing the message. Merge now with this one, …": the message is the plain one meanwhile.
    pub writing: Option<String>,
    /// The writer waits for permission: Answer in terminal opens its terminal, beside the plain message.
    pub asks: Option<WriterAsks>,
    /// "Into (SHOP-18’s base branch)"
    pub into: String,
    pub target: String,
    /// "~/code/shop-web"
    pub repo: String,
    /// The base is the ticket's own (a hotfix on `release/2.3`), locked since it started (d0051).
    pub locked: bool,
    pub message_label: &'static str,
    /// The message the box starts from: the user's kept edit, else the proposal.
    pub message: String,
    /// The proposal (the agent's, else the plain one): Use the proposed message puts it back.
    pub proposed: String,
    /// "Use the proposed message", shown while the box differs from `proposed`.
    pub use_proposed: &'static str,
    pub note: String,
    /// "Conflicts with release/2.3. Otaq merges the latest …" while the ticket is flagged (GIT-04).
    pub conflict: Option<String>,
    /// The Merge check's one line (d0120).
    pub merge_check: MergeCheckNote,
    /// "2 manual checks not ticked" beside it while some are open (d0125); it never disables Squash & merge.
    pub manual_checks: Option<String>,
    /// "Squash & merge", or "Merge into release/2.3" on another base.
    pub merge: String,
    pub cancel: &'static str,
    /// "cancel · ⌘↵ merge"
    pub hint: String,
}

/// The confirmation's line about the Merge check (d0120): "Runs `npm test` first when main has moved.", or without
/// one "No merge check · " and the link "set one in project settings", which opens the project's Commands where the
/// Merge check row is. The page shows the `…` parts as code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LocalMergeCheckNote")]
pub struct MergeCheckNote {
    pub text: String,
    pub link: Option<String>,
    /// The project whose settings the link opens.
    pub project: String,
}

impl MergeCheckNote {
    pub fn of(project: &str, command: Option<&str>, target: &str) -> Self {
        match command.map(str::trim).filter(|c| !c.is_empty()) {
            Some(command) => Self {
                text: ap::merge_check_runs_first(command, target),
                link: None,
                project: project.to_owned(),
            },
            None => Self {
                text: format!("{} \u{b7} ", ap::NO_MERGE_CHECK),
                link: Some(ap::SET_ONE_IN_PROJECT_SETTINGS.to_owned()),
                project: project.to_owned(),
            },
        }
    }
}

/// *Local merge · merging*: the steps while it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LocalMergeMerging")]
pub struct MergingView {
    pub title: String,
    pub steps: Vec<String>,
    /// The step git is really on, an index into `steps`.
    pub current: usize,
    /// While one of the user's hooks runs: "running your pre-commit hook · 40 s", under the current step. The page
    /// asks again every second meanwhile.
    pub hook: Option<String>,
    pub label: &'static str,
    pub note: String,
    /// The dialog can be closed while it runs; the merge finishes on its own (d0120).
    pub close: &'static str,
}

/// A new commit since the review: short sha and subject.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReviewNewCommit")]
pub struct CommitView {
    pub sha: String,
    pub subject: String,
}

/// *Approve · changed since review* (REV-06, d0043): only the new commits, then Approve anyway or Send back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReviewChanged")]
pub struct ChangedView {
    pub title: String,
    pub commits: Vec<CommitView>,
    /// The files the new commits changed; their lines come from `diff.file` with `since: reviewed`.
    pub files: Vec<String>,
    pub reviewed: String,
    pub note: &'static str,
    pub anyway: &'static str,
    pub send_back: &'static str,
}

pub fn changed_view(key: &TicketKey, check: &ApproveCheck) -> Option<ChangedView> {
    match ap::changed_view(key, check) {
        MergeView::Changed {
            title,
            commits,
            files,
            reviewed,
        } if !title.is_empty() => Some(ChangedView {
            title,
            commits: commits
                .into_iter()
                .map(|(sha, subject)| CommitView { sha, subject })
                .collect(),
            files,
            reviewed,
            note: "Only the new commits are shown. Approve anyway merges them too.",
            anyway: "Approve anyway",
            send_back: crate::view_models::sheet_actions::SEND_BACK,
        }),
        _ => None,
    }
}

/// What `review.approve` answers: the choice, the confirmation, the running state, and the change found since the
/// review, if any.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ApproveView")]
pub struct ApproveView {
    pub key: String,
    pub menu: ApproveMenu,
    /// The ticket has a PR: approving pushes to it (T-0227's dialog), never a Local merge (REV-14, d0041).
    pub push_to_pr: bool,
    pub confirm: ConfirmView,
    pub merging: MergingView,
    pub changed: Option<ChangedView>,
}

/// The facts the approve dialogs read besides the ticket.
pub struct ApproveFacts<'a> {
    pub ticket: &'a Ticket,
    pub tickets: &'a [Ticket],
    pub target: &'a str,
    /// The proposed message.
    pub message: &'a str,
    /// The user's kept edit of it, shown instead until the ticket lands.
    pub edited: Option<&'a str>,
    pub repo: &'a str,
    /// The project's own base branch (a ticket on another one shows it locked).
    pub project_base: &'a str,
    pub check: &'a ApproveCheck,
    pub menu: ApproveMenu,
    /// The project's Merge check command (d0120), `None` when it has none.
    pub merge_check: Option<&'a str>,
    /// The ticket's manual checks not ticked (d0125).
    pub manual_unticked: u32,
    /// The delivery's agent that wrote `message` while the ticket waited in Review.
    pub prepared_by: Option<&'a str>,
    /// The delivery's agent still writing it.
    pub writing: Option<&'a str>,
    /// The writer's session, when it waits for permission.
    pub asks_in: Option<&'a str>,
    /// Now, for how long a running hook has run.
    pub now_ms: i64,
}

/// The message's writer waits for permission (its tool asks before a command): the dialog says so and offers its
/// terminal to answer in, beside the plain message. The ticket in Review stays calm meanwhile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "WriterAsks")]
pub struct WriterAsks {
    /// The writer's session: its terminal answers the prompt.
    pub session: String,
    /// "Answer in terminal"
    pub answer: &'static str,
    /// "Developer · SHOP-5's message": the terminal's title.
    pub terminal_title: String,
}

/// Answer in terminal, beside the plain message.
pub const ANSWER_IN_TERMINAL: &str = "Answer in terminal";

impl WriterAsks {
    pub fn new(session: &str, agent: &str, key: &str) -> Self {
        Self {
            session: session.to_owned(),
            answer: ANSWER_IN_TERMINAL,
            terminal_title: format!("{agent} \u{b7} {key}\u{2019}s message"),
        }
    }
}

/// "Strong is still writing the message. Merge now with this one, edit it, or wait: it fills in when ready unless
/// you've typed."
pub fn still_writing(agent: &str) -> String {
    format!(
        "{agent} is still writing the message. Merge now with this one, edit it, or wait: it fills in when ready \
         unless you\u{2019}ve typed."
    )
}

/// "Strong wants to run a command to write the message. Merge now with this one, edit it, or wait: …": the writer
/// waits for permission.
pub fn wants_to_run(agent: &str) -> String {
    format!(
        "{agent} wants to run a command to write the message. Merge now with this one, edit it, or wait: it fills \
         in once you answer, unless you\u{2019}ve typed."
    )
}

pub fn approve_view(f: ApproveFacts) -> ApproveView {
    let ticket = f.ticket;
    let other_base = f.target != f.project_base;
    let merging = merging_view(&f);
    ApproveView {
        key: ticket.key.to_string(),
        menu: f.menu,
        push_to_pr: ticket.flags.pr_number.is_some(),
        confirm: ConfirmView {
            title: format!("Merge {} into {}", ticket.key, f.target),
            lead: format!(
                "Squashed into one commit on {t}. Your folder fast-forwards if {t} is checked out and clean.",
                t = f.target
            ),
            prepared: f.prepared_by.map(|agent| {
                format!(
                    "Prepared by {agent} while {} waited in Review. Edit it if you like.",
                    ticket.key
                )
            }),
            writing: f.writing.map(|agent| match f.asks_in {
                Some(_) => wants_to_run(agent),
                None => still_writing(agent),
            }),
            asks: f
                .writing
                .zip(f.asks_in)
                .map(|(agent, session)| WriterAsks::new(session, agent, &ticket.key.to_string())),
            into: ap::into_label(ticket),
            target: f.target.to_owned(),
            repo: crate::shell::tilde(f.repo),
            locked: other_base,
            message_label: "Commit message",
            message: f.edited.unwrap_or(f.message).to_owned(),
            proposed: f.message.to_owned(),
            use_proposed: ap::USE_THE_PROPOSED_MESSAGE,
            note: ap::confirm_note(ticket, f.target, f.tickets),
            conflict: ticket.flags.conflicts_with.as_ref().map(|_| {
                format!(
                    "Conflicts with {t}. {} merges the latest {t} into the branch first; if it still conflicts, its \
                     agent fixes it.",
                    otaq_core::naming::PRODUCT_NAME,
                    t = f.target
                )
            }),
            merge_check: MergeCheckNote::of(
                &ticket.key.project.to_string(),
                f.merge_check,
                f.target,
            ),
            manual_checks: crate::view_models::manual_checks::not_ticked(f.manual_unticked),
            merge: if other_base {
                format!("Merge into {}", f.target)
            } else {
                "Merge".to_owned()
            },
            cancel: "Cancel",
            hint: format!("cancel \u{b7} {} merge", submit_keys()),
        },
        merging,
        changed: changed_view(&ticket.key, f.check),
    }
}

/// *Local merge · merging*: its steps, the one git is really on and the hook that runs.
fn merging_view(f: &ApproveFacts) -> MergingView {
    let ticket = f.ticket;
    let check = f.merge_check.is_some_and(|c| !c.trim().is_empty());
    let progress = ticket
        .flags
        .git
        .as_ref()
        .filter(|g| g.operation == otaq_core::domain::GitOperation::Merge);
    MergingView {
        title: ap::confirm_title(ticket),
        steps: ap::merging_steps(f.target, check),
        current: ap::merging_step(progress, check),
        hook: progress.and_then(|g| {
            let hook = g.hook.as_ref()?;
            Some(crate::view_models::git_progress::hook_line(
                hook, g.step, f.now_ms,
            ))
        }),
        label: "Merging\u{2026}",
        note: format!(
            "Runs in {}\u{2019}s own scratch worktree \u{2014} your checkout is never touched.",
            otaq_core::naming::PRODUCT_NAME
        ),
        close: "Close",
    }
}

/// "Show changes": a hook's changes to what landed.
pub const SHOW_CHANGES: &str = "Show changes";

/// A hook changed the files of the commit Approve & merge made, and it landed as the hook left it: the merged toast's
/// and History's "Your pre-commit hook changed 2 files" with Show changes, and the window Show changes opens, each
/// file's difference between what was reviewed and what landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(rename = "HookChangesView")]
pub struct HookChangesView {
    /// "Your pre-commit hook changed 2 files".
    pub title: String,
    /// "Show changes".
    pub show: String,
    /// Under the title in the window: what the lines are.
    pub note: String,
    /// The commit that was squashed (what was reviewed, the target merged in) and the one that landed: each file's
    /// lines are `files.diff` with `since` and `until`.
    pub reviewed: String,
    pub landed: String,
    pub files: Vec<String>,
    /// "Close".
    pub close: String,
}

/// The view of a hook's changes to what landed.
pub fn hook_changes_view(changes: &HookChanges) -> HookChangesView {
    HookChangesView {
        title: changes.sentence(),
        show: SHOW_CHANGES.to_owned(),
        note: "What the hook changed after your review: the difference between what you reviewed and what landed."
            .to_owned(),
        reviewed: changes.reviewed.clone(),
        landed: changes.landed.clone(),
        files: changes.files.clone(),
        close: "Close".to_owned(),
    }
}

/// The merged toast's Show changes when a hook changed what landed; `None` otherwise.
pub fn merged_toast_link(
    key: &TicketKey,
    outcome: &LocalMergeOutcome,
) -> Option<crate::view_models::toasts::ToastLink> {
    let LocalMergeOutcome::Merged {
        hook_changed: Some(changes),
        ..
    } = outcome
    else {
        return None;
    };
    Some(hook_changes_link(key, changes))
}

/// Show changes after a toast's text, opening the window of a hook's changes.
pub fn hook_changes_link(
    key: &TicketKey,
    changes: &HookChanges,
) -> crate::view_models::toasts::ToastLink {
    crate::view_models::toasts::ToastLink::HookChanges {
        label: SHOW_CHANGES.to_owned(),
        ticket: key.to_string(),
        changes: hook_changes_view(changes),
    }
}

/// A toast the page shows (the merged ones; their wording is the view model's).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "ReviewToast")]
pub struct ToastText {
    pub title: String,
    pub text: Option<String>,
}

/// How a Local merge ended, as the dialog shows it. A merge that went through closes the dialog (the toasts say it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "LocalMergeResult")]
pub enum MergeResult {
    Merged {
        toasts: Vec<ToastText>,
    },
    /// *Merge conflict during Local merge*: the files, what happens now (the agent fixes it with no click, FLW-17),
    /// the worktree.
    Conflict {
        title: String,
        text: String,
        files: Vec<String>,
        /// A failed Merge check's output, its end when long; `None` for a conflict or a check that printed nothing.
        output: Option<String>,
        help: String,
        worktree: Option<String>,
        close: &'static str,
    },
    /// The user's Stop of the Merge check (Q475): nothing merged, the ticket where it was.
    Stopped {
        title: String,
        message: String,
        close: &'static str,
    },
    /// *Local merge · refused, uncommitted changes* (the user's checkout) or a diverged base: Retry.
    Refused {
        title: String,
        message: String,
        retry: &'static str,
        close: &'static str,
    },
    /// *Uncommitted files · refused* (REV-08): View · Send back.
    Uncommitted {
        title: String,
        files: Vec<String>,
        text: &'static str,
        view: &'static str,
        send_back: &'static str,
        cancel: &'static str,
    },
    Changed {
        changed: ChangedView,
    },
    /// git has no name and email for the user: nothing merged. The form asks for them; its button saves them and
    /// merges again.
    Identity {
        form: crate::view_models::git_identity::Form,
        /// "Save and merge".
        merge: &'static str,
        cancel: &'static str,
    },
}

/// The toasts after a Local merge: "Local merge done · SHOP-18", "Couldn't update main from origin · merged into your
/// local main" with git's reason when the fetch failed (d0121), then who started ("SHOP-20 started · all dependencies
/// merged").
pub fn merged_toasts(
    key: &TicketKey,
    outcome: &LocalMergeOutcome,
    tickets: &[Ticket],
) -> Vec<ToastText> {
    let LocalMergeOutcome::Merged {
        ticket,
        target,
        commit,
        started,
        hook_changed,
    } = outcome
    else {
        return Vec::new();
    };
    // A hook changed what landed: said in the merged toast itself ("· Your pre-commit hook changed 2 files"), its
    // Show changes from `merged_toast_link`. Nothing extra otherwise.
    let text = match hook_changed {
        Some(changes) => format!("{} \u{b7} {}", ap::squashed_as(commit), changes.sentence()),
        None => ap::squashed_as(commit),
    };
    let mut toasts = vec![ToastText {
        title: ap::merged_title(key, target),
        text: Some(text),
    }];
    if let Some(warning) = ticket
        .work
        .as_ref()
        .and_then(|w| w.remote_warning.as_ref())
        .filter(|w| w.went_on == otaq_core::domain::WentOn::Merged)
    {
        toasts.push(ToastText {
            title: crate::view_models::card::remote_warning_text(warning),
            text: Some(warning.reason.clone()),
        });
    }
    match started.as_slice() {
        [] => {}
        [one] => toasts.push(ToastText {
            title: format!("{one} started \u{b7} all dependencies merged"),
            text: tickets.iter().find(|t| &t.key == one).map(|t| {
                format!(
                    "{} branched from {target} at {}",
                    t.title,
                    ap::short(commit)
                )
            }),
        }),
        many => toasts.push(ToastText {
            title: ap::started_label(many.len()).unwrap_or_default(),
            text: Some(
                many.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        }),
    }
    toasts
}

/// The toasts after "Done, nothing to merge": "Done · SHOP-18" and who started.
pub fn done_toasts(key: &TicketKey, started: &[TicketKey]) -> Vec<ToastText> {
    finished_toasts(
        key,
        "Nothing to merge; it counts as merged and its branch is kept",
        started,
    )
}

/// The toasts after "Done, don't merge": "Done · SHOP-18" · "Not merged; …" and who started.
pub fn done_not_merged_toasts(key: &TicketKey, started: &[TicketKey]) -> Vec<ToastText> {
    finished_toasts(
        key,
        "Not merged; it counts as merged and its branch keeps the commits",
        started,
    )
}

fn finished_toasts(key: &TicketKey, text: &str, started: &[TicketKey]) -> Vec<ToastText> {
    let mut toasts = vec![ToastText {
        title: format!("Done \u{b7} {key}"),
        text: Some(text.to_owned()),
    }];
    if let Some(title) = ap::started_label(started.len()) {
        toasts.push(ToastText {
            title,
            text: Some(
                started
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
        });
    }
    toasts
}

/// The text of the toast for a Local merge that failed with an error (not an outcome the dialog shows): git's own
/// failure ("git commit --quiet --allow-empty -F /var/folders/…/T/.tmpW7HFC2 failed (exit 128): Author identity
/// unknown … fatal: unable to auto-detect email address …") becomes its step and git's last word, "git commit
/// failed: unable to auto-detect email address …", without the temporary paths and git's own advice. The whole
/// message stays in the service's log. Anything else is shown as it is.
pub fn merge_failed_text(message: &str) -> String {
    let Some(rest) = message.strip_prefix("git ") else {
        return message.to_owned();
    };
    let Some((command, reason)) = rest.split_once(" failed") else {
        return message.to_owned();
    };
    let step = command.split_whitespace().next().unwrap_or("");
    let stderr = reason.split_once("): ").map_or("", |(_, s)| s);
    let last = stderr
        .lines()
        .map(str::trim)
        .rev()
        .find(|l| !l.is_empty())
        .map(|l| {
            l.trim_start_matches("fatal: ")
                .trim_start_matches("error: ")
                .to_owned()
        });
    match last {
        Some(last) => format!("git {step} failed: {last}"),
        None => format!("git {step} failed"),
    }
}

pub fn merge_result(
    ticket: &Ticket,
    target: &str,
    outcome: &LocalMergeOutcome,
    tickets: &[Ticket],
) -> MergeResult {
    let key = &ticket.key;
    match ap::view(key, outcome) {
        MergeView::Merged { .. } => MergeResult::Merged {
            toasts: merged_toasts(key, outcome, tickets),
        },
        MergeView::Conflict { title, text, files } => MergeResult::Conflict {
            title,
            text,
            files,
            output: None,
            help: ap::conflict_help(key, target),
            worktree: ticket
                .work
                .as_ref()
                .and_then(|w| w.worktree.as_deref())
                .map(crate::shell::tilde),
            close: "Close",
        },
        // Shown as a conflict is: what failed, and that the agent fixes it with no click (d0120).
        MergeView::CheckFailed {
            title,
            text,
            output,
        } => MergeResult::Conflict {
            title,
            text,
            files: Vec::new(),
            output: Some(output).filter(|o| !o.is_empty()),
            help: ap::check_failed_help(key, target),
            worktree: ticket
                .work
                .as_ref()
                .and_then(|w| w.worktree.as_deref())
                .map(crate::shell::tilde),
            close: "Close",
        },
        MergeView::CheckStopped { message } => MergeResult::Stopped {
            title: "Merge check stopped".to_owned(),
            message,
            close: "Close",
        },
        MergeView::DirtyCheckout { message } | MergeView::Diverged { message } => {
            MergeResult::Refused {
                title: format!("Can\u{2019}t merge into {target} yet"),
                message,
                retry: "Retry",
                close: "Close",
            }
        }
        MergeView::Uncommitted { message, files } => MergeResult::Uncommitted {
            title: message,
            files,
            text: "Send it back so the agent commits them, or look at them first.",
            view: "View",
            send_back: crate::view_models::sheet_actions::SEND_BACK,
            cancel: "Cancel",
        },
        MergeView::NeedsIdentity { has } => MergeResult::Identity {
            form: crate::view_models::git_identity::form(&has),
            merge: "Save and merge",
            cancel: "Cancel",
        },
        MergeView::Changed { .. } => match outcome {
            LocalMergeOutcome::Changed { check } => match changed_view(key, check) {
                Some(changed) => MergeResult::Changed { changed },
                None => MergeResult::Merged { toasts: Vec::new() },
            },
            _ => MergeResult::Merged { toasts: Vec::new() },
        },
    }
}

/// One pending comment as Send back lists it: "api/checkout/route.ts · L9" and the text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SendBackComment")]
pub struct SendBackComment {
    pub at: String,
    pub body: String,
}

/// A ticket that keeps waiting: "SHOP-20", "Checkout page · Queued".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SendBackWaiting")]
pub struct WaitingRow {
    pub key: String,
    pub text: String,
}

/// The PR note on a ticket with a PR (*Send back · ticket with an Azure DevOps PR*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SendBackPrNote")]
pub struct PrNote {
    pub title: String,
    pub text: String,
}

/// *Send back · review & confirm* (REV-04).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "SendBackView")]
pub struct SendBackView {
    pub key: String,
    pub title: String,
    pub intro: String,
    pub pr: Option<PrNote>,
    pub receive_label: &'static str,
    pub comments: Vec<SendBackComment>,
    /// "No line comments."
    pub no_comments: &'static str,
    /// "Comment for Dev (optional)" and its line (CMT-13).
    pub comment: CommentBoxView,
    pub waiting_title: Option<String>,
    pub waiting: Vec<WaitingRow>,
    /// "Then Security runs again before it comes back to you." ([`sb::runs_again`]).
    pub footnote: Option<String>,
    /// "Send back to Product spec", as the title (d0093).
    pub confirm: String,
    pub cancel: &'static str,
    pub hint: &'static str,
}

/// The dialog of `ticket`, going back to the Agent stage `to` ("Product spec", [`sb::back_stage`]); `footnote` names
/// the agent checks it walks again ([`sb::runs_again`]).
pub fn send_back_view(
    ticket: &Ticket,
    comments: &[ReviewComment],
    tickets: &[Ticket],
    to: &str,
    footnote: Option<String>,
) -> SendBackView {
    let plan = sb::plan(ticket, comments, tickets, to);
    SendBackView {
        key: ticket.key.to_string(),
        title: plan.title,
        intro: plan.intro,
        pr: ticket.flags.pr_number.map(|n| {
            let (title, text) =
                pr_dialog::send_back_pr_note(n, pr_provider_label(ticket).as_deref());
            PrNote { title, text }
        }),
        receive_label: "THE AGENT WILL RECEIVE",
        comments: plan
            .comments
            .into_iter()
            .map(|(at, body)| SendBackComment { at, body })
            .collect(),
        no_comments: "No line comments.",
        comment: sb::comment_box(to),
        waiting_title: (!plan.waiting.is_empty()).then(|| sb::waiting_title(&ticket.key)),
        waiting: plan
            .waiting
            .into_iter()
            .map(|(key, text)| WaitingRow {
                key: key.to_string(),
                text,
            })
            .collect(),
        footnote,
        confirm: sb::title(to),
        cancel: "Cancel",
        hint: "cancel",
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::board::comments::Side;
    use otaq_core::board::review::NewCommit;
    use otaq_core::domain::Status;
    use otaq_core::git::diff::{ChangedFile, FileStatus};

    use super::*;
    use crate::view_models::board_page::approve_menu;
    use crate::view_models::card_fixtures::ticket;

    fn review_ticket() -> Ticket {
        ticket("SHOP-18", "Checkout API", Status::WaitingForYou)
    }

    #[test]
    fn a_failed_git_step_reads_as_its_step_and_gits_last_word() {
        let raw = "git commit --quiet --allow-empty -F /var/folders/63/h8cy/T/.tmpW7HFC2 failed (exit 128): Author \
                   identity unknown\n\n*** Please tell me who you are.\n\nRun\n\n  git config --global user.email \
                   \"you@example.com\"\n\nto set your account's default identity.\n\nfatal: unable to auto-detect \
                   email address (got 'nt@Studio.(none)')";
        assert_eq!(
            merge_failed_text(raw),
            "git commit failed: unable to auto-detect email address (got 'nt@Studio.(none)')"
        );
        assert_eq!(
            merge_failed_text("git rev-parse refs/heads/main failed"),
            "git rev-parse failed"
        );
        assert_eq!(
            merge_failed_text("SHOP-18 is Done; Local merge works on a ticket in Ready for Review"),
            "SHOP-18 is Done; Local merge works on a ticket in Ready for Review"
        );
    }

    #[test]
    fn a_merge_without_a_git_identity_asks_for_it_in_the_dialog() {
        let t = review_ticket();
        let outcome = LocalMergeOutcome::NeedsIdentity {
            has: otaq_core::git::Identity {
                name: Some("Ada".into()),
                email: None,
            },
        };
        let MergeResult::Identity {
            form,
            merge,
            cancel,
        } = merge_result(&t, "main", &outcome, &[])
        else {
            panic!("expected the identity form");
        };
        assert_eq!((merge, cancel), ("Save and merge", "Cancel"));
        assert_eq!(
            form.title,
            "git doesn\u{2019}t know your name and email yet"
        );
        assert_eq!((form.name.as_str(), form.email.as_str()), ("Ada", ""));
    }

    fn facts<'a>(
        t: &'a Ticket,
        all: &'a [Ticket],
        target: &'a str,
        check: &'a ApproveCheck,
    ) -> ApproveFacts<'a> {
        ApproveFacts {
            ticket: t,
            tickets: all,
            target,
            message: "SHOP-18: Checkout API",
            edited: None,
            repo: "/nowhere/code/shop-web",
            project_base: "main",
            check,
            menu: approve_menu(
                t,
                target,
                all,
                None,
                Some(1),
                otaq_core::flow::Delivers::Either,
            ),
            merge_check: None,
            manual_unticked: 0,
            prepared_by: None,
            writing: None,
            asks_in: None,
            now_ms: 0,
        }
    }

    #[test]
    fn the_confirmation_reads_like_the_frame() {
        let t = review_ticket();
        let all = vec![t.clone()];
        let view = approve_view(facts(&t, &all, "main", &ApproveCheck::Unchanged));
        let c = &view.confirm;
        assert_eq!(c.title, "Merge SHOP-18 into main");
        assert_eq!(
            c.lead,
            "Squashed into one commit on main. Your folder fast-forwards if main is checked out and clean."
        );
        assert_eq!(c.message_label, "Commit message");
        assert_eq!(c.into, "Into (SHOP-18\u{2019}s base branch)");
        assert_eq!((c.merge.as_str(), c.locked), ("Merge", false));
        assert_eq!((c.prepared.as_ref(), c.writing.as_ref()), (None, None));
        assert_eq!(c.message, "SHOP-18: Checkout API");
        assert!(
            c.note
                .ends_with("is removed unless it has uncommitted changes.")
        );
        assert_eq!(c.conflict, None);
        assert!(c.hint.starts_with("cancel \u{b7} "));
        assert_eq!(view.merging.label, "Merging\u{2026}");
        assert_eq!(view.merging.steps.len(), 5);
        assert_eq!(
            (view.merging.current, view.merging.hook.as_ref()),
            (0, None)
        );
        assert!(view.changed.is_none());
        assert!(!view.push_to_pr);
        assert_eq!(view.menu.items[0].title, "Approve & merge into main");
    }

    /// d0120: without a Merge check the dialog says so once and links to the row; with one it names the command and
    /// the branch it merges into.
    #[test]
    fn the_confirmation_says_whether_a_merge_check_runs() {
        let t = review_ticket();
        let all = vec![t.clone()];
        let none = approve_view(facts(&t, &all, "main", &ApproveCheck::Unchanged));
        assert_eq!(
            none.confirm.merge_check,
            MergeCheckNote {
                text: "No merge check \u{b7} ".into(),
                link: Some("set one in project settings".into()),
                project: "SHOP".into(),
            }
        );
        let blank = approve_view(ApproveFacts {
            merge_check: Some("  "),
            ..facts(&t, &all, "main", &ApproveCheck::Unchanged)
        });
        assert_eq!(blank.confirm.merge_check, none.confirm.merge_check);
        let set = approve_view(ApproveFacts {
            merge_check: Some("npm test"),
            ..facts(&t, &all, "main", &ApproveCheck::Unchanged)
        });
        assert_eq!(
            set.confirm.merge_check,
            MergeCheckNote {
                text: "Runs `npm test` first when main has moved.".into(),
                link: None,
                project: "SHOP".into(),
            }
        );
        let hotfix = approve_view(ApproveFacts {
            merge_check: Some("make check"),
            ..facts(&t, &all, "release/2.3", &ApproveCheck::Unchanged)
        });
        assert_eq!(
            hotfix.confirm.merge_check.text,
            "Runs `make check` first when release/2.3 has moved."
        );
        assert_eq!(
            none.merging.close, "Close",
            "it can be closed while it runs"
        );
    }

    /// d0125: the confirmation counts the manual checks not ticked, and says nothing once all are.
    #[test]
    fn the_confirmation_counts_unticked_manual_checks() {
        let t = review_ticket();
        let all = vec![t.clone()];
        let ticked = approve_view(facts(&t, &all, "main", &ApproveCheck::Unchanged));
        assert_eq!(ticked.confirm.manual_checks, None);
        let open = approve_view(ApproveFacts {
            manual_unticked: 2,
            ..facts(&t, &all, "main", &ApproveCheck::Unchanged)
        });
        assert_eq!(
            open.confirm.manual_checks.as_deref(),
            Some("2 manual checks not ticked")
        );
        assert_eq!(open.confirm.merge, "Merge", "never blocked");
    }

    /// The confirmation opens at once: on the delivery's agent's message once it wrote it, else on the plain one with
    /// who is still writing.
    #[test]
    fn the_confirmation_says_who_prepared_the_message_or_still_writes_it() {
        let t = review_ticket();
        let all = vec![t.clone()];
        let ready = approve_view(ApproveFacts {
            prepared_by: Some("Strong"),
            ..facts(&t, &all, "main", &ApproveCheck::Unchanged)
        });
        assert_eq!(
            ready.confirm.prepared.as_deref(),
            Some("Prepared by Strong while SHOP-18 waited in Review. Edit it if you like.")
        );
        let writing = approve_view(ApproveFacts {
            writing: Some("Strong"),
            ..facts(&t, &all, "main", &ApproveCheck::Unchanged)
        });
        assert_eq!(
            writing.confirm.writing.as_deref(),
            Some(
                "Strong is still writing the message. Merge now with this one, edit it, or wait: it fills in when \
                 ready unless you\u{2019}ve typed."
            )
        );
        assert_eq!(writing.confirm.prepared, None);
        assert_eq!(writing.confirm.asks, None);
    }

    /// The user's kept edit opens the box instead of the proposal, which stays at hand for Use the proposed message.
    #[test]
    fn a_kept_edit_opens_the_box_with_the_proposal_at_hand() {
        let t = review_ticket();
        let all = vec![t.clone()];
        let plain = approve_view(facts(&t, &all, "main", &ApproveCheck::Unchanged));
        assert_eq!(plain.confirm.message, plain.confirm.proposed);
        let kept = approve_view(ApproveFacts {
            edited: Some("feat: checkout API (SHOP-18)"),
            prepared_by: Some("Strong"),
            ..facts(&t, &all, "main", &ApproveCheck::Unchanged)
        });
        let c = &kept.confirm;
        assert_eq!(c.message, "feat: checkout API (SHOP-18)");
        assert_eq!(c.proposed, "SHOP-18: Checkout API");
        assert_eq!(c.use_proposed, "Use the proposed message");
    }

    /// The writer waits for permission: the line says so in plain words, and Answer in terminal opens its session's
    /// terminal beside the plain message, which can still be merged now.
    #[test]
    fn a_writer_waiting_for_permission_offers_answer_in_terminal() {
        let t = review_ticket();
        let all = vec![t.clone()];
        let asks = approve_view(ApproveFacts {
            writing: Some("Developer"),
            asks_in: Some("s-writer"),
            ..facts(&t, &all, "main", &ApproveCheck::Unchanged)
        });
        let c = &asks.confirm;
        assert_eq!(
            c.writing.as_deref(),
            Some(
                "Developer wants to run a command to write the message. Merge now with this one, edit it, or wait: \
                 it fills in once you answer, unless you\u{2019}ve typed."
            )
        );
        let a = c.asks.as_ref().expect("Answer in terminal");
        assert_eq!(a.answer, "Answer in terminal");
        assert_eq!(a.session, "s-writer");
        assert_eq!(
            a.terminal_title,
            "Developer \u{b7} SHOP-18\u{2019}s message"
        );
        assert_eq!(
            c.message, "SHOP-18: Checkout API",
            "the plain message stays"
        );
    }

    /// d0120, Q475: a failed check reads as a conflict does (the agent fixes it, no click); a stopped one says nothing
    /// was merged.
    #[test]
    fn a_failed_or_stopped_merge_check_has_its_dialog() {
        let t = review_ticket();
        let all = vec![t.clone()];
        let failed = LocalMergeOutcome::CheckFailed {
            target: "main".into(),
            command: "npm test".into(),
            exit: Some(1),
            output: "FAIL src/cart.test.ts\n  expected 2, got 3".into(),
        };
        let MergeResult::Conflict {
            title,
            text,
            files,
            output,
            help,
            ..
        } = merge_result(&t, "main", &failed, &all)
        else {
            panic!("not the check's dialog");
        };
        assert_eq!(title, "Merge check failed on main");
        assert_eq!(
            text,
            "Merge check `npm test` failed on the result with main merged in (exit 1). Fix it, then submit again."
        );
        assert!(files.is_empty());
        assert_eq!(
            output.as_deref(),
            Some("FAIL src/cart.test.ts\n  expected 2, got 3"),
            "the box shows what failed"
        );
        assert_eq!(
            help,
            "Nothing was merged into main. SHOP-18 went back to its agent, which fixes it and moves on again; the \
             check\u{2019}s whole output is attached to its comment."
        );
        let stopped = LocalMergeOutcome::CheckStopped {
            target: "main".into(),
        };
        assert_eq!(
            merge_result(&t, "main", &stopped, &all),
            MergeResult::Stopped {
                title: "Merge check stopped".into(),
                message: "The merge check was stopped; nothing was merged into main.".into(),
                close: "Close",
            }
        );
    }

    #[test]
    fn a_hotfix_on_its_own_base_names_it_and_its_conflict() {
        let mut t = ticket("SHOP-31", "Hotfix: checkout crash", Status::WaitingForYou);
        t.flags.conflicts_with = Some("release/2.3".into());
        let all = vec![t.clone()];
        let view = approve_view(facts(&t, &all, "release/2.3", &ApproveCheck::Unchanged));
        assert_eq!(view.confirm.merge, "Merge into release/2.3");
        assert!(view.confirm.locked);
        let conflict = view.confirm.conflict.unwrap();
        assert!(conflict.starts_with("Conflicts with release/2.3. "));
        assert!(conflict.ends_with("if it still conflicts, its agent fixes it."));
    }

    #[test]
    fn new_commits_since_the_review_show_first() {
        let t = review_ticket();
        let check = ApproveCheck::Changed {
            new_commits: vec![NewCommit {
                sha: "4d9e2b1aaaa".into(),
                subject: "fix(checkout): handle missing payment_intent".into(),
            }],
            files: vec![ChangedFile {
                status: FileStatus::Modified,
                path: "src/app/api/checkout/route.ts".into(),
                old_path: None,
                added: 4,
                removed: 1,
                binary: false,
                size: None,
                old_size: None,
                similarity: None,
            }],
            reviewed: "abc".into(),
            head: "def".into(),
        };
        let all = vec![t.clone()];
        let changed = approve_view(facts(&t, &all, "main", &check))
            .changed
            .unwrap();
        assert_eq!(
            changed.title,
            "SHOP-18 changed since you reviewed it \u{b7} 1 new commit"
        );
        assert_eq!(changed.commits[0].sha, "4d9e2b1");
        assert_eq!(changed.files, ["src/app/api/checkout/route.ts"]);
        assert_eq!(changed.reviewed, "abc");
        assert_eq!(
            (changed.anyway, changed.send_back),
            ("Approve anyway", "Send back\u{2026}")
        );
    }

    /// d0121: merged into the local base with the remote unreachable: a warning toast with git's reason, after
    /// "Local merge done".
    #[test]
    fn a_merge_without_the_remote_warns() {
        use otaq_core::domain::{RemoteWarning, WentOn};
        let mut t = review_ticket();
        t.work = Some(otaq_core::board::tickets::Work {
            remote_warning: Some(RemoteWarning {
                went_on: WentOn::Merged,
                branch: "main".into(),
                remote: "origin".into(),
                reason: "git fetch --quiet origin failed (exit 128): Host key verification failed."
                    .into(),
            }),
            ..Default::default()
        });
        let merged = LocalMergeOutcome::Merged {
            ticket: Box::new(t.clone()),
            target: "main".into(),
            commit: "9f3e2c1abc".into(),
            started: Vec::new(),
            hook_changed: None,
        };
        let toasts = merged_toasts(&t.key, &merged, &[]);
        assert_eq!(toasts.len(), 2);
        assert_eq!(
            toasts[1].title,
            "Couldn\u{2019}t update main from origin \u{b7} merged into your local main"
        );
        assert_eq!(
            toasts[1].text.as_deref(),
            Some("git fetch --quiet origin failed (exit 128): Host key verification failed.")
        );
        // A warning left from the start isn't the merge's.
        let mut started = t.clone();
        started
            .work
            .as_mut()
            .unwrap()
            .remote_warning
            .as_mut()
            .unwrap()
            .went_on = WentOn::Started;
        let merged = LocalMergeOutcome::Merged {
            ticket: Box::new(started),
            target: "main".into(),
            commit: "9f3e2c1abc".into(),
            started: Vec::new(),
            hook_changed: None,
        };
        assert_eq!(merged_toasts(&t.key, &merged, &[]).len(), 1);
    }

    /// A hook changed what landed: the merged toast says so, with Show changes; without one, nothing extra.
    #[test]
    fn a_merge_a_hook_changed_says_so_in_its_toast() {
        let t = review_ticket();
        let changes = HookChanges {
            hook: Some("pre-commit".into()),
            files: vec!["calc.js".into(), "util.js".into()],
            reviewed: "aaa".into(),
            landed: "9f3e2c1abc".into(),
        };
        let merged = LocalMergeOutcome::Merged {
            ticket: Box::new(t.clone()),
            target: "main".into(),
            commit: "9f3e2c1abc".into(),
            started: Vec::new(),
            hook_changed: Some(changes.clone()),
        };
        let toasts = merged_toasts(&t.key, &merged, &[]);
        assert_eq!(toasts.len(), 1);
        assert_eq!(
            toasts[0].text.as_deref(),
            Some("Squashed as 9f3e2c1 \u{b7} Your pre-commit hook changed 2 files")
        );
        let Some(crate::view_models::toasts::ToastLink::HookChanges {
            label,
            ticket,
            changes: view,
        }) = merged_toast_link(&t.key, &merged)
        else {
            panic!("Show changes");
        };
        assert_eq!(label, "Show changes");
        assert_eq!(ticket, "SHOP-18");
        assert_eq!(view, hook_changes_view(&changes));

        let plain = LocalMergeOutcome::Merged {
            ticket: Box::new(t.clone()),
            target: "main".into(),
            commit: "9f3e2c1abc".into(),
            started: Vec::new(),
            hook_changed: None,
        };
        assert_eq!(
            merged_toasts(&t.key, &plain, &[])[0].text.as_deref(),
            Some("Squashed as 9f3e2c1")
        );
        assert_eq!(merged_toast_link(&t.key, &plain), None);
    }

    #[test]
    fn each_merge_result_has_its_dialog() {
        let mut t = review_ticket();
        let mut child = ticket("SHOP-20", "Checkout page", Status::Queued);
        child.depends_on = vec![t.key.clone()];
        let all = vec![t.clone(), child];
        let merged = LocalMergeOutcome::Merged {
            ticket: Box::new(t.clone()),
            target: "main".into(),
            commit: "9f3e2c1abc".into(),
            started: vec!["SHOP-20".parse().unwrap()],
            hook_changed: None,
        };
        let MergeResult::Merged { toasts } = merge_result(&t, "main", &merged, &all) else {
            panic!("not merged");
        };
        // The words the first approve's toast uses: "SHOP-18 merged into main · Squashed as 9f3e2c1".
        assert_eq!(toasts[0].title, "SHOP-18 merged into main");
        assert_eq!(toasts[0].text.as_deref(), Some("Squashed as 9f3e2c1"));
        assert_eq!(
            toasts[1].title,
            "SHOP-20 started \u{b7} all dependencies merged"
        );
        assert_eq!(
            toasts[1].text.as_deref(),
            Some("Checkout page branched from main at 9f3e2c1")
        );

        let conflict = LocalMergeOutcome::Conflict {
            target: "main".into(),
            files: vec!["src/lib/stripe/session.ts".into(), "package.json".into()],
        };
        let MergeResult::Conflict {
            title, text, help, ..
        } = merge_result(&t, "main", &conflict, &all)
        else {
            panic!("not a conflict");
        };
        assert_eq!(title, "Merging main in stopped \u{2014} conflict");
        assert!(text.contains("conflicts in 2 files"));
        assert!(help.starts_with("SHOP-18 went back to its agent"), "{help}");

        let dirty = LocalMergeOutcome::RefusedDirtyCheckout {
            message: "shop-web has uncommitted changes on main. Clean up and retry.".into(),
        };
        let MergeResult::Refused {
            title,
            message,
            retry,
            ..
        } = merge_result(&t, "main", &dirty, &all)
        else {
            panic!("not refused");
        };
        assert_eq!(title, "Can\u{2019}t merge into main yet");
        assert!(message.starts_with("shop-web has uncommitted changes on main."));
        assert_eq!(retry, "Retry");

        t.key = "SHOP-19".parse().unwrap();
        let uncommitted = LocalMergeOutcome::RefusedUncommitted {
            files: vec!["src/components/pricing/PriceTable.tsx".into()],
            message: "SHOP-19 has 2 uncommitted files".into(),
        };
        let MergeResult::Uncommitted {
            title,
            view,
            send_back,
            text,
            ..
        } = merge_result(&t, "main", &uncommitted, &all)
        else {
            panic!("not uncommitted");
        };
        assert_eq!(title, "SHOP-19 has 2 uncommitted files");
        assert_eq!((view, send_back), ("View", "Send back\u{2026}"));
        assert!(text.starts_with("Send it back so the agent commits them"));
    }

    #[test]
    fn send_back_lists_comments_note_and_waiting_tickets() {
        let t = review_ticket();
        let mut child = ticket("SHOP-20", "Checkout page", Status::Queued);
        child.depends_on = vec![t.key.clone()];
        let comments = vec![ReviewComment {
            id: "1".into(),
            round: 1,
            file: "api/checkout/route.ts".into(),
            line: 9,
            side: Side::New,
            body: "Return { code, message }".into(),
            created_at: 0,
            sent_at: None,
        }];
        let view = send_back_view(&t, &comments, &[t.clone(), child], "Dev", None);
        assert_eq!(
            view.comment.label.as_deref(),
            Some("Comment for Dev (optional)")
        );
        assert_eq!(view.no_comments, "No line comments.");
        assert_eq!(view.title, "Send back to Dev");
        assert_eq!(view.confirm, "Send back to Dev");
        assert!(
            view.intro
                .starts_with("SHOP-18 goes straight back to its agent in Dev")
        );
        assert_eq!(view.comments[0].at, "api/checkout/route.ts \u{b7} L9");
        assert_eq!(
            view.waiting_title.as_deref(),
            Some("These keep waiting until SHOP-18 is merged")
        );
        assert_eq!(view.waiting[0].key, "SHOP-20");
        assert_eq!(
            view.waiting[0].text,
            "Checkout page \u{b7} Waiting to start"
        );
        assert_eq!(view.pr, None);
        let alone = send_back_view(&t, &[], std::slice::from_ref(&t), "Dev", None);
        assert!(alone.comments.is_empty() && alone.waiting_title.is_none());
        assert_eq!(alone.footnote, None);
    }

    /// d0093, d0131 (`s30-02`): Spec review's Send back goes to Product spec, which isn't the nearest Agent stage; the
    /// title and the confirm name it, the intro says its agent continues there, and the checks it walks again are
    /// named.
    #[test]
    fn the_dialog_names_the_stage_it_goes_back_to() {
        let mut t = ticket("SHOP-40", "Order history page", Status::WaitingForYou);
        t.place = otaq_core::domain::Place::of(Status::WaitingForYou, Some("spec-review"), None);
        let footnote = Some("Then Validate runs again before it comes back to you.".to_owned());
        let view = send_back_view(
            &t,
            &[],
            std::slice::from_ref(&t),
            "Product spec",
            footnote.clone(),
        );
        assert_eq!(view.title, "Send back to Product spec");
        assert_eq!(view.confirm, "Send back to Product spec");
        assert_eq!(
            view.intro,
            "SHOP-40 goes straight back to its agent in Product spec (Working) and continues in the same session."
        );
        assert_eq!(
            view.comment.label.as_deref(),
            Some("Comment for Product spec (optional)")
        );
        assert_eq!(view.footnote, footnote);
    }
}
