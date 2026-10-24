//! The review dialogs as the Tauri page draws them (T-0226; REV-04, REV-06, REV-08, GIT-03, GIT-04, GIT-05, UI-02;
//! d0015, d0017, d0034, d0038, d0043, d0050, d0051): [`approve`] and [`send_back`] turned into plain, serialized
//! shapes. *07 → Send back · review & confirm, Local merge · confirm / success / rebasing / conflict / refused,
//! Approve · changed since review, Uncommitted files · refused, Base branch · SHOP-31 on release/2.3*.
//!
//! The page keeps only which step shows, the edited message and note, and the click.

use otaq_core::board::comments::ReviewComment;
use otaq_core::board::review::ApproveCheck;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::TicketKey;
use otaq_core::review::LocalMergeOutcome;
use serde::Serialize;
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
    pub title: String,
    /// "Into (SHOP-18’s base branch)"
    pub into: String,
    pub target: String,
    /// "~/code/shop-web"
    pub repo: String,
    /// The base is the ticket's own (a hotfix on `release/2.3`), locked since it started (d0051).
    pub locked: bool,
    pub message_label: &'static str,
    pub message: String,
    pub note: String,
    /// "Conflicts with release/2.3. Otaq tries a rebase onto …" while the ticket is flagged (GIT-04).
    pub conflict: Option<String>,
    /// The Merge check's one line (d0120).
    pub merge_check: MergeCheckNote,
    /// "Squash & merge", or "Squash & merge into release/2.3" on another base.
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

/// *Local merge · rebasing onto main…*: the steps while it runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LocalMergeMerging")]
pub struct MergingView {
    pub title: String,
    pub steps: Vec<String>,
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
            send_back: "Send back",
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
    pub into_epic: bool,
    pub message: &'a str,
    pub repo: &'a str,
    /// The project's own base branch (a ticket on another one shows it locked).
    pub project_base: &'a str,
    pub check: &'a ApproveCheck,
    pub menu: ApproveMenu,
    /// The project's Merge check command (d0120), `None` when it has none.
    pub merge_check: Option<&'a str>,
}

pub fn approve_view(f: ApproveFacts) -> ApproveView {
    let ticket = f.ticket;
    let other_base = !f.into_epic && f.target != f.project_base;
    ApproveView {
        key: ticket.key.to_string(),
        menu: f.menu,
        push_to_pr: ticket.flags.pr_number.is_some(),
        confirm: ConfirmView {
            title: ap::confirm_title(ticket),
            into: ap::into_label(ticket),
            target: f.target.to_owned(),
            repo: crate::shell::tilde(f.repo),
            locked: other_base,
            message_label: "Squash commit message",
            message: f.message.to_owned(),
            note: ap::confirm_note(ticket, f.target, f.tickets),
            conflict: ticket.flags.conflicts_with.as_ref().map(|_| {
                format!(
                    "Conflicts with {t}. {} tries a rebase onto the latest {t} first; if it still conflicts, its \
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
            merge: if other_base {
                format!("Squash & merge into {}", f.target)
            } else {
                "Squash & merge".to_owned()
            },
            cancel: "Cancel",
            hint: format!("cancel \u{b7} {} merge", submit_keys()),
        },
        merging: MergingView {
            title: ap::confirm_title(ticket),
            steps: ap::merging_steps(f.target, f.into_epic),
            label: "Merging\u{2026}",
            note: format!(
                "Runs in {}\u{2019}s own scratch worktree \u{2014} your checkout is never touched.",
                otaq_core::naming::PRODUCT_NAME
            ),
            close: "Close",
        },
        changed: changed_view(&ticket.key, f.check),
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
    } = outcome
    else {
        return Vec::new();
    };
    let mut toasts = vec![ToastText {
        title: format!("Local merge done \u{b7} {key}"),
        text: Some(format!("Squashed into {target} as {}", ap::short(commit))),
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
            help: ap::conflict_help(key, target),
            worktree: ticket
                .work
                .as_ref()
                .and_then(|w| w.worktree.as_deref())
                .map(crate::shell::tilde),
            close: "Close",
        },
        // Shown as a conflict is: what failed, and that the agent fixes it with no click (d0120).
        MergeView::CheckFailed { title, text } => MergeResult::Conflict {
            title,
            text,
            files: Vec::new(),
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
            send_back: "Send back",
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
    pub confirm: &'static str,
    pub cancel: &'static str,
    pub hint: &'static str,
}

/// The dialog of `ticket`, going back to the Agent stage `to` ("Dev", [`sb::back_stage`]).
pub fn send_back_view(
    ticket: &Ticket,
    comments: &[ReviewComment],
    tickets: &[Ticket],
    to: &str,
) -> SendBackView {
    let plan = sb::plan(ticket, comments, tickets);
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
        confirm: "Send back",
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
            into_epic: false,
            message: "SHOP-18: Checkout API",
            repo: "/nowhere/code/shop-web",
            project_base: "main",
            check,
            menu: approve_menu(t, target, all, None),
            merge_check: None,
        }
    }

    #[test]
    fn the_confirmation_reads_like_the_frame() {
        let t = review_ticket();
        let all = vec![t.clone()];
        let view = approve_view(facts(&t, &all, "main", &ApproveCheck::Unchanged));
        let c = &view.confirm;
        assert_eq!(c.title, "Local merge SHOP-18 Checkout API");
        assert_eq!(c.into, "Into (SHOP-18\u{2019}s base branch)");
        assert_eq!((c.merge.as_str(), c.locked), ("Squash & merge", false));
        assert_eq!(c.message, "SHOP-18: Checkout API");
        assert!(
            c.note
                .ends_with("is removed unless it has uncommitted changes.")
        );
        assert_eq!(c.conflict, None);
        assert!(c.hint.starts_with("cancel \u{b7} "));
        assert_eq!(view.merging.label, "Merging\u{2026}");
        assert_eq!(view.merging.steps.len(), 4);
        assert!(view.changed.is_none());
        assert!(!view.push_to_pr);
        assert_eq!(view.menu.items[0].title, "Local merge");
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
        };
        let MergeResult::Conflict {
            title,
            text,
            files,
            help,
            ..
        } = merge_result(&t, "main", &failed, &all)
        else {
            panic!("not the check's dialog");
        };
        assert_eq!(title, "Merge check failed on main");
        assert_eq!(
            text,
            "Merge check `npm test` failed on the result rebased onto main (exit 1). Fix it, then submit again."
        );
        assert!(files.is_empty());
        assert_eq!(
            help,
            "Nothing was merged into main. SHOP-18 went back to its agent, which fixes it and moves on again; the \
             check\u{2019}s output is attached to its comment."
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
        assert_eq!(view.confirm.merge, "Squash & merge into release/2.3");
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
            ("Approve anyway", "Send back")
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
        };
        assert_eq!(merged_toasts(&t.key, &merged, &[]).len(), 1);
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
        };
        let MergeResult::Merged { toasts } = merge_result(&t, "main", &merged, &all) else {
            panic!("not merged");
        };
        assert_eq!(toasts[0].title, "Local merge done \u{b7} SHOP-18");
        assert_eq!(
            toasts[0].text.as_deref(),
            Some("Squashed into main as 9f3e2c1")
        );
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
        assert_eq!(title, "Rebase onto main stopped \u{2014} conflict");
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
        assert_eq!((view, send_back), ("View", "Send back"));
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
        let view = send_back_view(&t, &comments, &[t.clone(), child], "Dev");
        assert_eq!(
            view.comment.label.as_deref(),
            Some("Comment for Dev (optional)")
        );
        assert_eq!(view.no_comments, "No line comments.");
        assert_eq!(view.title, "Send SHOP-18 Checkout API back");
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
        assert_eq!(view.waiting[0].text, "Checkout page \u{b7} Ready for Dev");
        assert_eq!(view.pr, None);
        let alone = send_back_view(&t, &[], std::slice::from_ref(&t), "Dev");
        assert!(alone.comments.is_empty() && alone.waiting_title.is_none());
    }
}
