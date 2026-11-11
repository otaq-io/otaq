//! Approve ▾ and Local merge (GIT-03, REV-08, UI-02; d0015, d0038, d0050, d0058; *07 → Local merge · confirm,
//! success, merging, conflict, refused*): the choice, the confirmation's texts, and one state per Local merge
//! result. Approve offers exactly what the flow's delivery allows, named by what it does ("Approve & merge into
//! main", "Approve & open PR"; both when the flow delivers either), then the other ways to finish the ticket. Nothing
//! here pushes, and no state offers to stash or discard.

use otaq_core::board::merge_into::{can_be_merged_in, can_take};
use otaq_core::board::review::{ApproveCheck, changed_message};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Status, TicketKey};
use otaq_core::flow::Delivers;
use otaq_core::pr::provider::PrProvider;
use otaq_core::review::LocalMergeOutcome;
use otaq_core::review::merge_into::{MergeIntoOutcome, conflict_message};

/// One choice in the Approve menu ("1 Local merge", "2 Open PR on GitHub").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApproveOption {
    pub choice: ApproveChoice,
    pub title: String,
    pub detail: String,
    pub enabled: bool,
}

/// `ApproveChoice` on the page: `"done"`, `"local_merge"`, `"open_pr"`, `"push_to_pr"`, `"merge_into"`,
/// `"done_not_merged"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ApproveChoice {
    /// "Done, nothing to merge": the branch has no commits beyond its base; the ticket finishes at once, counting as
    /// merged, with nothing merged (`review.done`).
    Done,
    LocalMerge,
    OpenPr,
    /// A ticket with an open PR: its new commits go to that PR (REV-14, d0041).
    PushToPr,
    /// Merge into another ticket…: the picker of tickets whose branch can take this one's, so they land together.
    MergeInto,
    /// "Done, don't merge": the branch has commits the user chooses not to land. After its confirmation the ticket
    /// finishes as Done does, counting as merged, with the commits kept on its branch (`review.done_not_merged`).
    DoneNotMerged,
}

/// "Approve SHOP-18 Checkout API", "Push SHOP-26 Add sitemap to PR #481".
pub fn menu_title(ticket: &Ticket) -> String {
    if let Some(n) = ticket.flags.pr_number {
        format!("Push {} {} to PR #{n}", ticket.key, ticket.title)
    } else {
        format!("Approve {} {}", ticket.key, ticket.title)
    }
}

/// The line under the choices.
pub fn menu_note(ticket: &Ticket) -> String {
    if ticket.flags.pr_number.is_some() {
        format!(
            "Pushes the branch to origin. {} doesn\u{2019}t watch the PR; use Mark merged or Mark closed once the PR is merged or closed.",
            otaq_core::naming::PRODUCT_NAME
        )
    } else {
        "Each opens a confirmation step before anything runs.".to_owned()
    }
}

/// The Approve choice that merges locally: "Approve & merge into main".
pub fn approve_and_merge(target: &str) -> String {
    format!("Approve & merge into {target}")
}

/// The Approve choice that opens a PR.
pub const APPROVE_AND_OPEN_PR: &str = "Approve & open PR";

/// The choice that leads the menu when the branch has no commits beyond its base.
pub const DONE_NOTHING_TO_MERGE: &str = "Done, nothing to merge";
/// Why Local merge and Open PR are disabled then.
pub const NO_CHANGES_IN_THE_REPO: &str = "No changes in the repo";
/// The line under the choices then: Done runs at once, with no confirmation.
pub const DONE_NOTE: &str =
    "Done finishes the ticket at once; it counts as merged and its branch is kept.";

/// The last choice when the branch has commits: finish without landing them.
pub const DONE_DONT_MERGE: &str = "Done, don\u{2019}t merge";

/// What won't land, in the confirmation: "SHOP-18’s 3 commits won’t be merged; its branch is kept".
pub fn not_merged_message(key: &TicketKey, commits: u32) -> String {
    let commits = match commits {
        1 => "1 commit".to_owned(),
        n => format!("{n} commits"),
    };
    format!("{key}\u{2019}s {commits} won\u{2019}t be merged; its branch is kept")
}

/// The confirmation "Done, don't merge" opens.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(rename = "DoneNotMergedView")]
pub struct DoneNotMergedView {
    /// "Done, don’t merge SHOP-18 Checkout API".
    pub title: String,
    /// What won't land ([`not_merged_message`]).
    pub message: String,
    /// What happens on confirm.
    pub note: String,
    /// The confirm button.
    pub confirm: String,
}

pub fn done_not_merged_view(ticket: &Ticket, target: &str, commits: u32) -> DoneNotMergedView {
    let key = &ticket.key;
    DoneNotMergedView {
        title: format!("{DONE_DONT_MERGE} {key} {}", ticket.title),
        message: not_merged_message(key, commits),
        note: format!(
            "Nothing lands on {target}. {key} finishes as Done and counts as merged, so tickets waiting on it start. \
             The worktree is removed unless it has uncommitted changes."
        ),
        confirm: DONE_DONT_MERGE.to_owned(),
    }
}

/// Tickets that depend on `ticket` and still wait for another parent after it merges: "SHOP-20 still waits for
/// SHOP-19".
pub fn still_waiting(
    ticket: &Ticket,
    tickets: &[Ticket],
) -> Vec<(TicketKey, String, Vec<TicketKey>)> {
    let mut out: Vec<(TicketKey, String, Vec<TicketKey>)> = tickets
        .iter()
        .filter(|t| t.depends_on.contains(&ticket.key) && t.status.is_queued())
        .filter_map(|t| {
            let others: Vec<TicketKey> = t
                .depends_on
                .iter()
                .filter(|p| *p != &ticket.key)
                .filter(|p| {
                    tickets
                        .iter()
                        .find(|x| &x.key == *p)
                        .is_none_or(|x| x.status != Status::Merged)
                })
                .cloned()
                .collect();
            (!others.is_empty()).then(|| (t.key.clone(), t.title.clone(), others))
        })
        .collect();
    out.sort();
    out
}

fn keys(keys: &[TicketKey]) -> String {
    keys.iter()
        .map(|k| k.to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// The menu: what the flow's delivery allows (`delivers`), named by what it does: "Approve & merge into main" for a
/// local merge, "Approve & open PR" for a PR (d0059), both for `either`; then Merge into another ticket…. With the
/// built-in Open PR skill and no detected provider, the PR choice is disabled with "PR provider not detected"
/// (REV-11). A ticket with an open PR offers only Push to PR #N (REV-14, d0041).
///
/// `commits` are the commits on the branch beyond its base (`None`: unknown). With none, the menu leads with "Done,
/// nothing to merge", and the delivery's choices stay listed, disabled, with "No changes in the repo": the diff
/// decides, no ticket field does. With some, "Done, don't merge" comes last, after every way to deliver them (its
/// confirmation says what won't land).
pub fn options(
    ticket: &Ticket,
    target: &str,
    tickets: &[Ticket],
    pr: Option<&PrProvider>,
    commits: Option<u32>,
    delivers: Delivers,
) -> Vec<ApproveOption> {
    if let Some(n) = ticket.flags.pr_number {
        let provider = pr
            .and_then(|p| p.provider)
            .map(|p| format!(" \u{b7} {}", p.name()))
            .unwrap_or_default();
        let branch = ticket
            .work
            .as_ref()
            .and_then(|w| w.branch.clone())
            .unwrap_or_default();
        return vec![ApproveOption {
            choice: ApproveChoice::PushToPr,
            title: format!("Push to PR #{n}"),
            detail: format!("{branch}{provider} \u{b7} only the new commits"),
            enabled: true,
        }];
    }
    let mut out = Vec::new();
    let key = &ticket.key;
    if delivers != Delivers::Pr {
        let mut detail = format!("Squash-merges {key} into {target}");
        if let Some((key, _, others)) = still_waiting(ticket, tickets).first() {
            detail.push_str(&format!("; {key} still waits for {}", keys(others)));
        }
        detail.push('.');
        out.push(ApproveOption {
            choice: ApproveChoice::LocalMerge,
            title: approve_and_merge(target),
            detail,
            enabled: true,
        });
    }
    if delivers != Delivers::Local {
        let blocked = pr.and_then(PrProvider::open_pr_blocked);
        let on = pr
            .and_then(|p| p.provider)
            .map(|p| format!(" on {}", p.name()))
            .unwrap_or_default();
        let instead = match delivers {
            Delivers::Either => " instead",
            _ => "",
        };
        let detail = match blocked {
            Some(why) => why.to_owned(),
            None => format!("Pushes the branch and opens a pull request{on}{instead}."),
        };
        out.push(ApproveOption {
            choice: ApproveChoice::OpenPr,
            title: APPROVE_AND_OPEN_PR.to_owned(),
            detail,
            enabled: blocked.is_none(),
        });
    }
    out.push(merge_into_option(ticket, target, tickets));
    let done = ApproveOption {
        choice: ApproveChoice::Done,
        title: DONE_NOTHING_TO_MERGE.to_owned(),
        detail: format!("The work needed no change, or it\u{2019}s already in {target}."),
        enabled: commits == Some(0),
    };
    let not_merged = |n: u32| ApproveOption {
        choice: ApproveChoice::DoneNotMerged,
        title: DONE_DONT_MERGE.to_owned(),
        detail: format!(
            "Close the ticket and keep {} out of {target}.",
            match n {
                1 => "its commit".to_owned(),
                n => format!("its {n} commits"),
            }
        ),
        enabled: n > 0,
    };
    // Both ways to finish without merging are always listed; only the one that fits the branch is enabled.
    if commits == Some(0) {
        for option in &mut out {
            option.detail = NO_CHANGES_IN_THE_REPO.to_owned();
            option.enabled = false;
        }
        out.insert(0, done);
        out.push(ApproveOption {
            detail: format!("The branch has no commits to keep out of {target}."),
            ..not_merged(0)
        });
    } else if let Some(n) = commits {
        out.push(ApproveOption {
            detail: format!(
                "{} has commits: Done, don\u{2019}t merge keeps them out of {target}.",
                ticket.key
            ),
            ..done
        });
        out.push(not_merged(n));
    }
    out
}

/// The line under the delivery's choices in the Approve menu: the flow decides how a ticket is delivered, and where
/// to change it. Empty for a ticket with an open PR (its new commits go to that PR).
pub fn delivery_note(ticket: &Ticket, target: &str, delivers: Delivers) -> String {
    if ticket.flags.pr_number.is_some() {
        return String::new();
    }
    let how = match delivers {
        Delivers::Either => "Your flow delivers either way.".to_owned(),
        Delivers::Local => format!("Your flow merges into {target}."),
        Delivers::Pr => "Your flow opens a PR.".to_owned(),
    };
    format!("{how} Change it at the Done end of Edit flow.")
}

/// "Merge into another ticket…": enabled when another ticket can take this one's branch (the picker lists them).
fn merge_into_option(ticket: &Ticket, target: &str, tickets: &[Ticket]) -> ApproveOption {
    let can = can_be_merged_in(ticket) && tickets.iter().any(|host| can_take(ticket, host));
    ApproveOption {
        choice: ApproveChoice::MergeInto,
        title: MERGE_INTO.to_owned(),
        detail: if can {
            format!("Its work joins that ticket instead of {target}.")
        } else {
            NO_HOST.to_owned()
        },
        enabled: can,
    }
}

/// The Approve menu's choice that opens the picker.
pub const MERGE_INTO: &str = "Merge into another ticket\u{2026}";

/// Why nothing can take the ticket's branch: the choice's detail, and the empty picker's line.
pub const NO_HOST: &str =
    "No other ticket of this base waits in review, to be delivered or with a PR open";

/// One ticket the picker offers: "SHOP-18 Checkout API · Ready for Review".
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(rename = "MergeIntoItem")]
pub struct MergeIntoItem {
    pub key: String,
    pub title: String,
    /// Where it waits: "Ready for Review", "To deliver", "PR #481 open".
    pub status: String,
}

/// The picker "Merge into another ticket…" opens.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[ts(rename = "MergeIntoPicker")]
pub struct MergeIntoPicker {
    /// "Merge SHOP-20 Coupon codes into…".
    pub title: String,
    /// What happens on a pick.
    pub note: String,
    pub items: Vec<MergeIntoItem>,
    /// No ticket can take it: why ([`NO_HOST`]).
    pub empty: Option<String>,
}

/// The picker for `ticket`, listing `hosts` (the service's `ticket.merge_into_hosts`).
pub fn merge_into_picker(ticket: &Ticket, hosts: &[Ticket]) -> MergeIntoPicker {
    let key = &ticket.key;
    MergeIntoPicker {
        title: format!("Merge {key} {} into\u{2026}", ticket.title),
        note: format!(
            "Squashes {key}\u{2019}s branch into the ticket you pick as one commit. {key} waits as Merged into it and \
             lands with it; that ticket goes back to review with the combined diff."
        ),
        items: hosts
            .iter()
            .map(|host| MergeIntoItem {
                key: host.key.to_string(),
                title: host.title.clone(),
                status: match (host.status, host.flags.pr_number) {
                    (Status::PrOpen, Some(n)) => format!("PR #{n} open"),
                    (status, _) => status.label().to_owned(),
                },
            })
            .collect(),
        empty: hosts.is_empty().then(|| NO_HOST.to_owned()),
    }
}

/// What the merge's answer shows: a toast once merged, else what stopped it (nothing merged). The dialog closes
/// either way; the toast says which.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "MergeIntoResult")]
pub enum MergeIntoView {
    /// "SHOP-20 merged into SHOP-18" · "SHOP-18 is back in review with the combined diff".
    Merged { title: String, text: String },
    /// A conflict or uncommitted files: nothing merged, the ticket where it was.
    Refused { message: String },
}

pub fn merge_into_view(key: &TicketKey, outcome: &MergeIntoOutcome) -> MergeIntoView {
    match outcome {
        MergeIntoOutcome::Merged { into, .. } => MergeIntoView::Merged {
            title: format!("{key} merged into {}", into.key),
            text: format!("{} is back in review with the combined diff", into.key),
        },
        MergeIntoOutcome::Conflict { into, files } => MergeIntoView::Refused {
            message: conflict_message(key, into, files),
        },
        MergeIntoOutcome::RefusedUncommitted { message, .. } => MergeIntoView::Refused {
            message: format!("{message}: nothing was merged"),
        },
    }
}

/// "Approve & open PR on GitHub", or plain "Approve & open PR" when no provider is detected (a custom skill decides).
pub fn open_pr_title(pr: Option<&PrProvider>) -> String {
    match pr.and_then(|p| p.provider) {
        Some(provider) => format!("Approve & open PR on {}", provider.name()),
        None => "Approve & open PR".to_owned(),
    }
}

/// Puts the agent's proposal back in Approve & merge's message, or in Approve & open PR's title, description and
/// commit messages, in place of the user's kept edit.
pub const USE_THE_PROPOSED_MESSAGE: &str = "Use the proposed message";

/// The edit Approve & merge keeps on the ticket: the message as the user left it when it isn't the proposal (spaces
/// around it don't count), else none, and the dialog opens on the proposal again.
pub fn kept_edit(proposed: &str, message: &str) -> Option<String> {
    (message.trim() != proposed.trim()).then(|| message.to_owned())
}

/// "Approve & merge SHOP-18 Checkout API".
pub fn confirm_title(ticket: &Ticket) -> String {
    format!("Approve & merge {} {}", ticket.key, ticket.title)
}

/// "Into (SHOP-18’s base branch)".
pub fn into_label(ticket: &Ticket) -> String {
    format!("Into ({}\u{2019}s base branch)", ticket.key)
}

/// The note under the message: who keeps waiting, and what happens to the worktree.
pub fn confirm_note(ticket: &Ticket, target: &str, tickets: &[Ticket]) -> String {
    let mut note = String::new();
    for (key, title, others) in still_waiting(ticket, tickets) {
        let both = if others.len() == 1 {
            "both parents are"
        } else {
            "all its parents are"
        };
        note.push_str(&format!(
            "{key} {title} still waits for {} to merge; it starts from {target} once {both} merged. ",
            keys(&others)
        ));
    }
    note.push_str(&format!(
        "The worktree for {} is removed unless it has uncommitted changes.",
        ticket.key
    ));
    note
}

/// The Approve / Local merge dialog's line without a Merge check (d0120): "No merge check · set one in project
/// settings", the second half the link to the row.
pub const NO_MERGE_CHECK: &str = "No merge check";
pub const SET_ONE_IN_PROJECT_SETTINGS: &str = "set one in project settings";

/// The dialog's line with a Merge check (d0120): "Runs `npm test` first when main has moved."
pub fn merge_check_runs_first(command: &str, target: &str) -> String {
    format!("Runs `{command}` first when {target} has moved.")
}

/// What happens after a failed Merge check (d0120, no click): the agent fixes it, as after a conflict.
pub fn check_failed_help(key: &TicketKey, target: &str) -> String {
    format!(
        "Nothing was merged into {target}. {key} went back to its agent, which fixes it and moves on again; the \
         check\u{2019}s whole output is attached to its comment."
    )
}

/// The steps the "Merging…" state lists (*Local merge · merging*).
pub fn merging_steps(target: &str) -> Vec<String> {
    vec![
        format!("Fetch {target} and bring it up to date"),
        format!("Merge {target} into the branch"),
        format!("Squash into {target}"),
        "Start waiting tickets whose dependencies are all merged".to_owned(),
    ]
}

/// What the sheet shows after the click.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MergeView {
    /// Done: the toast's title and text, and the started-tickets toast if any.
    Merged {
        title: String,
        text: String,
        started: Option<String>,
    },
    Conflict {
        title: String,
        text: String,
        files: Vec<String>,
    },
    /// The Merge check failed on the result with the target merged in (d0120): nothing merged, the agent fixes it with no click.
    /// `output` is the end of what it printed (empty when it printed nothing).
    CheckFailed {
        title: String,
        text: String,
        output: String,
    },
    /// The user's Stop of the Merge check (Q475): nothing merged, the ticket where it was.
    CheckStopped { message: String },
    /// "shop-web has uncommitted changes on main. Clean up and retry." (Retry).
    DirtyCheckout { message: String },
    /// The base diverged from its remote (Retry).
    Diverged { message: String },
    /// "SHOP-19 has 2 uncommitted files" · View · Send back.
    Uncommitted { message: String, files: Vec<String> },
    /// git has no name and email for the user: nothing merged; they're asked for, then the merge goes on.
    NeedsIdentity { has: otaq_core::git::Identity },
    /// "SHOP-18 changed since you reviewed it · 1 new commit": the new commits (short sha, subject) and the
    /// files they changed · Approve anyway / Send back (REV-06).
    Changed {
        title: String,
        commits: Vec<(String, String)>,
        files: Vec<String>,
        /// The reviewed head: the new commits' diff starts there.
        reviewed: String,
    },
}

pub fn short(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// "2 waiting tickets started".
pub fn started_label(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 waiting ticket started".to_owned()),
        n => Some(format!("{n} waiting tickets started")),
    }
}

/// The merged toast's title: "SHOP-1 merged into main".
pub fn merged_title(key: &TicketKey, target: &str) -> String {
    format!("{key} merged into {target}")
}

/// The merged toast's text: "Squashed as 9f3e2c1".
pub fn squashed_as(commit: &str) -> String {
    format!("Squashed as {}", short(commit))
}

pub fn view(key: &TicketKey, outcome: &LocalMergeOutcome) -> MergeView {
    match outcome {
        LocalMergeOutcome::Merged {
            target,
            commit,
            started,
            hook_changed,
            ..
        } => MergeView::Merged {
            title: merged_title(key, target),
            // A hook changed what landed: said after it, "· Your pre-commit hook changed 2 files".
            text: match hook_changed {
                Some(changes) => format!("{} \u{b7} {}", squashed_as(commit), changes.sentence()),
                None => squashed_as(commit),
            },
            started: started_label(started.len()),
        },
        LocalMergeOutcome::Conflict { target, files } => MergeView::Conflict {
            title: format!("Merging {target} in stopped \u{2014} conflict"),
            text: format!(
                "{target} moved since {key} was built. Merging the latest {target} into its branch conflicts in {}. It \
                 was aborted; {target} and your own checkout are unchanged.",
                match files.len() {
                    1 => "1 file".to_owned(),
                    n => format!("{n} files"),
                }
            ),
            files: files.clone(),
        },
        LocalMergeOutcome::RefusedDirtyCheckout { message } => MergeView::DirtyCheckout {
            message: message.clone(),
        },
        LocalMergeOutcome::RefusedDiverged { message } => MergeView::Diverged {
            message: message.clone(),
        },
        LocalMergeOutcome::RefusedUncommitted { files, message } => MergeView::Uncommitted {
            message: message.clone(),
            files: files.clone(),
        },
        LocalMergeOutcome::Changed { check } => changed_view(key, check),
        LocalMergeOutcome::NeedsIdentity { has } => MergeView::NeedsIdentity { has: has.clone() },
        // Nothing merged; the ticket is back with its agent, the comment's words (d0120).
        LocalMergeOutcome::CheckFailed {
            target,
            command,
            exit,
            output,
        } => MergeView::CheckFailed {
            title: format!("Merge check failed on {target}"),
            text: otaq_core::domain::validation::merge_check_failed(command, target, *exit),
            output: output.clone(),
        },
        LocalMergeOutcome::CheckStopped { target } => MergeView::CheckStopped {
            message: format!("The merge check was stopped; nothing was merged into {target}."),
        },
    }
}

/// The changed-since-review state from the check (REV-06, d0043).
pub fn changed_view(key: &TicketKey, check: &ApproveCheck) -> MergeView {
    match check {
        ApproveCheck::Unchanged => MergeView::Changed {
            title: String::new(),
            commits: Vec::new(),
            files: Vec::new(),
            reviewed: String::new(),
        },
        ApproveCheck::Changed {
            new_commits,
            files,
            reviewed,
            ..
        } => MergeView::Changed {
            reviewed: reviewed.clone(),
            title: changed_message(&key.to_string(), new_commits.len()),
            commits: new_commits
                .iter()
                .map(|c| (short(&c.sha), c.subject.clone()))
                .collect(),
            files: files.iter().map(|f| f.path.clone()).collect(),
        },
    }
}

/// The sheet header's strip: "Conflicts with main · 2 files (src/lib/stripe/session.ts, package.json)".
pub fn conflict_line(conflict: &otaq_core::board::conflicts::Conflict) -> String {
    let mut line = format!("Conflicts with {}", conflict.target);
    match conflict.files.len() {
        0 => {}
        1 => line.push_str(&format!(" \u{b7} 1 file ({})", conflict.files[0])),
        n => line.push_str(&format!(
            " \u{b7} {n} files ({})",
            conflict.files.join(", ")
        )),
    }
    line
}

/// What moved on the target, under the strip: "SHOP-6 changed README.md", "SHOP-6 changed a.ts and b.ts · 4fa97d6
/// changed c.ts". `None` when git couldn't say.
pub fn moved_line(conflict: &otaq_core::board::conflicts::Conflict) -> Option<String> {
    if conflict.moved.is_empty() {
        return None;
    }
    Some(
        conflict
            .moved
            .iter()
            .map(|m| {
                format!(
                    "{} changed {}",
                    m.by,
                    crate::view_models::dependencies::and_list(&m.files)
                )
            })
            .collect::<Vec<_>>()
            .join(" \u{b7} "),
    )
}

/// A delivery problem the agent fixes on its own merges nothing, and the ticket comes back to its human check once
/// fixed: "Nothing was merged into main. SHOP-5 returns to Review when it's fixed."
pub fn nothing_merged_line(key: &TicketKey, target: &str) -> String {
    format!("Nothing was merged into {target}. {key} returns to Review when it\u{2019}s fixed.")
}

/// Where the problem stands among the ones in a row that stop the ticket: "1 of 3".
pub fn in_a_row(count: u32) -> String {
    format!(
        "{count} of {}",
        otaq_core::board::merge::DELIVERY_CONFLICT_LIMIT
    )
}

/// What the count means, on hover.
pub const IN_A_ROW_HINT: &str =
    "Conflicts and failed merge checks in a row. After the third, it stops and waits for you.";

/// What happens after the conflict (FLW-17, no click): the agent fixes it, or the user does by hand.
pub fn conflict_help(key: &TicketKey, target: &str) -> String {
    format!(
        "{key} went back to its agent, which merges {target} in, resolves the conflict and moves on again. Or \
         resolve it yourself in the worktree."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;

    fn sample() -> (Ticket, Vec<Ticket>) {
        let shop18 = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        let shop19 = ticket("SHOP-19", "Price table", Status::Working);
        let mut shop20 = ticket("SHOP-20", "Checkout page", Status::Queued);
        shop20.depends_on = vec![shop18.key.clone(), shop19.key.clone()];
        (shop18.clone(), vec![shop18, shop19, shop20])
    }

    /// Only a message that differs from the proposal is kept; the proposal typed back (or with spaces around it)
    /// keeps nothing.
    #[test]
    fn only_a_message_that_isnt_the_proposal_is_kept() {
        let proposed = "SHOP-18 Add subtraction\n\nAdds subtract().";
        assert_eq!(kept_edit(proposed, proposed), None);
        assert_eq!(kept_edit(proposed, &format!("{proposed}\n")), None);
        assert_eq!(
            kept_edit(proposed, "feat: add subtraction (SHOP-18)").as_deref(),
            Some("feat: add subtraction (SHOP-18)")
        );
    }

    #[test]
    fn a_ticket_with_a_pr_offers_only_push_to_pr() {
        let mut t = ticket("SHOP-26", "Add sitemap", Status::WaitingForYou);
        t.flags.pr_number = Some(481);
        t.work = Some(otaq_core::board::tickets::Work {
            branch: Some("feature/SHOP-26-add-sitemap".into()),
            ..Default::default()
        });
        let only = options(
            &t,
            "main",
            &[],
            None,
            Some(2),
            otaq_core::flow::Delivers::Either,
        );
        assert_eq!(only.len(), 1, "Local merge and Open PR are hidden");
        assert_eq!(only[0].choice, ApproveChoice::PushToPr);
        assert_eq!(only[0].title, "Push to PR #481");
        assert!(only[0].enabled);
        assert_eq!(menu_title(&t), "Push SHOP-26 Add sitemap to PR #481");
        assert!(menu_note(&t).contains("use Mark merged or Mark closed"));
    }

    #[test]
    fn open_pr_is_disabled_without_a_provider_for_the_built_in_skill() {
        use otaq_core::pr::provider::Provider;
        let (shop18, tickets) = sample();
        let mut pr = PrProvider {
            remote: Some("git.acme.internal/web/handbook".into()),
            provider: None,
            logged_in: None,
            cli_found: false,
            open_pr_skill: "open-pr".to_owned(),
        };
        let open = &options(
            &shop18,
            "main",
            &tickets,
            Some(&pr),
            Some(2),
            otaq_core::flow::Delivers::Either,
        )[1];
        assert_eq!(open.title, "Approve & open PR");
        assert_eq!(open.detail, "PR provider not detected");
        assert!(!open.enabled);
        pr.provider = Some(Provider::GitHub);
        let open = &options(
            &shop18,
            "main",
            &tickets,
            Some(&pr),
            Some(2),
            otaq_core::flow::Delivers::Either,
        )[1];
        assert_eq!(open.title, "Approve & open PR");
        assert_eq!(
            open.detail,
            "Pushes the branch and opens a pull request on GitHub instead."
        );
        assert_ne!(open.detail, "PR provider not detected");
    }

    /// Merge into another ticket…: enabled when another ticket of the base waits with a branch; the picker lists it.
    #[test]
    fn merge_into_is_offered_when_another_ticket_can_take_the_branch() {
        let with_branch = |key: &str, title: &str, status: Status| {
            let mut t = ticket(key, title, status);
            t.work = Some(otaq_core::board::tickets::Work {
                branch: Some(format!("feature/{key}")),
                ..Default::default()
            });
            t
        };
        let shop20 = with_branch("SHOP-20", "Coupon codes", Status::WaitingForYou);
        let shop18 = with_branch("SHOP-18", "Checkout API", Status::WaitingForYou);
        let shop19 = with_branch("SHOP-19", "Price table", Status::Working);
        let all = [shop18.clone(), shop19, shop20.clone()];
        let menu = options(
            &shop20,
            "main",
            &all,
            None,
            Some(2),
            otaq_core::flow::Delivers::Either,
        );
        let merge = &menu[2];
        assert_eq!(merge.choice, ApproveChoice::MergeInto);
        assert_eq!(merge.title, "Merge into another ticket\u{2026}");
        assert!(merge.enabled);
        let alone = options(
            &shop20,
            "main",
            std::slice::from_ref(&shop20),
            None,
            Some(2),
            otaq_core::flow::Delivers::Either,
        );
        assert!(!alone[2].enabled, "nothing can take it");
        assert_eq!(alone[2].detail, NO_HOST);

        let picker = merge_into_picker(&shop20, std::slice::from_ref(&shop18));
        assert_eq!(picker.title, "Merge SHOP-20 Coupon codes into\u{2026}");
        assert_eq!(
            picker.items,
            [MergeIntoItem {
                key: "SHOP-18".into(),
                title: "Checkout API".into(),
                status: "Ready for you".into(),
            }]
        );
        assert_eq!(picker.empty, None);
        assert_eq!(
            merge_into_picker(&shop20, &[]).empty.as_deref(),
            Some(NO_HOST)
        );
        let conflict = MergeIntoOutcome::Conflict {
            into: shop18.key.clone(),
            files: vec!["lib.ts".into()],
        };
        assert_eq!(
            merge_into_view(&shop20.key, &conflict),
            MergeIntoView::Refused {
                message: "SHOP-20 conflicts with SHOP-18 in lib.ts: nothing was merged. SHOP-20 stays where it was."
                    .into()
            }
        );
    }

    #[test]
    fn the_menu_offers_local_merge_and_open_pr() {
        let (shop18, tickets) = sample();
        let options = options(
            &shop18,
            "main",
            &tickets,
            None,
            Some(2),
            otaq_core::flow::Delivers::Either,
        );
        assert_eq!(menu_title(&shop18), "Approve SHOP-18 Checkout API");
        assert_eq!(options[0].choice, ApproveChoice::LocalMerge);
        assert_eq!(
            options[0].detail,
            "Squash-merges SHOP-18 into main; SHOP-20 still waits for SHOP-19."
        );
        assert!(options[0].enabled);
        assert_eq!(options[1].choice, ApproveChoice::OpenPr);
        assert!(options[1].enabled, "Open PR works from M10");
    }

    /// Approve offers exactly what the flow's delivery allows, named by what it does: both for `either`, one for
    /// `local` or `pr`; the other ways to finish stay in the menu.
    #[test]
    fn approve_offers_what_the_flows_delivery_allows() {
        use otaq_core::flow::Delivers;
        let (shop18, tickets) = sample();
        let menu = |delivers| -> Vec<(ApproveChoice, String)> {
            options(&shop18, "main", &tickets, None, Some(2), delivers)
                .into_iter()
                .map(|o| (o.choice, o.title))
                .collect()
        };
        let merge = (
            ApproveChoice::LocalMerge,
            "Approve & merge into main".to_owned(),
        );
        let pr = (ApproveChoice::OpenPr, "Approve & open PR".to_owned());
        let into = (ApproveChoice::MergeInto, MERGE_INTO.to_owned());
        let done = (ApproveChoice::Done, DONE_NOTHING_TO_MERGE.to_owned());
        let dont = (ApproveChoice::DoneNotMerged, DONE_DONT_MERGE.to_owned());
        assert_eq!(
            menu(Delivers::Either),
            [
                merge.clone(),
                pr.clone(),
                into.clone(),
                done.clone(),
                dont.clone()
            ]
        );
        assert_eq!(
            menu(Delivers::Local),
            [merge, into.clone(), done.clone(), dont.clone()]
        );
        assert_eq!(menu(Delivers::Pr), [pr, into, done, dont]);
    }

    /// A branch with no commits beyond its base: "Done, nothing to merge" leads; Local merge and Open PR stay listed,
    /// disabled, with "No changes in the repo".
    #[test]
    fn nothing_to_merge_leads_with_done_and_disables_the_others() {
        let (shop18, tickets) = sample();
        let menu = options(
            &shop18,
            "main",
            &tickets,
            None,
            Some(0),
            otaq_core::flow::Delivers::Either,
        );
        let choices: Vec<_> = menu.iter().map(|o| (o.choice, o.enabled)).collect();
        assert_eq!(
            choices,
            [
                (ApproveChoice::Done, true),
                (ApproveChoice::LocalMerge, false),
                (ApproveChoice::OpenPr, false),
                (ApproveChoice::MergeInto, false),
                (ApproveChoice::DoneNotMerged, false),
            ]
        );
        assert_eq!(menu[0].title, "Done, nothing to merge");
        assert_eq!(
            menu[0].detail,
            "The work needed no change, or it\u{2019}s already in main."
        );
        for other in &menu[1..4] {
            assert_eq!(other.detail, "No changes in the repo");
        }
        // Commits: Done, nothing to merge stays listed, disabled, saying why.
        let with = options(
            &shop18,
            "main",
            &tickets,
            None,
            Some(1),
            otaq_core::flow::Delivers::Either,
        );
        let done = with
            .iter()
            .find(|o| o.choice == ApproveChoice::Done)
            .unwrap();
        assert!(!done.enabled);
        assert_eq!(
            done.detail,
            "SHOP-18 has commits: Done, don\u{2019}t merge keeps them out of main."
        );
        // A count nobody could read: neither way to finish without merging.
        let unknown = options(
            &shop18,
            "main",
            &tickets,
            None,
            None,
            otaq_core::flow::Delivers::Either,
        );
        assert_eq!(unknown[0].choice, ApproveChoice::LocalMerge);
        assert!(unknown.iter().all(|o| o.choice != ApproveChoice::Done));
    }

    /// A branch with commits: "Done, don't merge" comes last, after every way to deliver them, and its confirmation
    /// says what won't land. Not with an open PR, nor when nobody could count the commits.
    #[test]
    fn done_not_merged_comes_last_and_names_what_wont_land() {
        let (shop18, tickets) = sample();
        let menu = options(
            &shop18,
            "main",
            &tickets,
            None,
            Some(2),
            otaq_core::flow::Delivers::Either,
        );
        let choices: Vec<_> = menu.iter().map(|o| o.choice).collect();
        assert_eq!(
            choices,
            [
                ApproveChoice::LocalMerge,
                ApproveChoice::OpenPr,
                ApproveChoice::MergeInto,
                ApproveChoice::Done,
                ApproveChoice::DoneNotMerged,
            ]
        );
        let last = menu.last().unwrap();
        assert_eq!(last.title, "Done, don\u{2019}t merge");
        assert_eq!(
            last.detail,
            "Close the ticket and keep its 2 commits out of main."
        );
        assert!(last.enabled);
        assert!(
            options(
                &shop18,
                "main",
                &tickets,
                None,
                None,
                otaq_core::flow::Delivers::Either
            )
            .iter()
            .all(|o| o.choice != ApproveChoice::DoneNotMerged)
        );
        let mut pr = shop18.clone();
        pr.flags.pr_number = Some(481);
        assert!(
            options(
                &pr,
                "main",
                &tickets,
                None,
                Some(2),
                otaq_core::flow::Delivers::Either
            )
            .iter()
            .all(|o| o.choice != ApproveChoice::DoneNotMerged),
            "the menu stays Push to PR #N"
        );

        let view = done_not_merged_view(&shop18, "main", 3);
        assert_eq!(view.title, "Done, don\u{2019}t merge SHOP-18 Checkout API");
        assert_eq!(
            view.message,
            "SHOP-18\u{2019}s 3 commits won\u{2019}t be merged; its branch is kept"
        );
        assert_eq!(
            not_merged_message(&shop18.key, 1),
            "SHOP-18\u{2019}s 1 commit won\u{2019}t be merged; its branch is kept"
        );
        assert!(view.note.starts_with("Nothing lands on main."));
        assert_eq!(view.confirm, "Done, don\u{2019}t merge");
    }

    #[test]
    fn the_confirmation_names_the_target_and_who_keeps_waiting() {
        let (shop18, tickets) = sample();
        assert_eq!(
            confirm_title(&shop18),
            "Approve & merge SHOP-18 Checkout API"
        );
        assert_eq!(into_label(&shop18), "Into (SHOP-18\u{2019}s base branch)");
        assert_eq!(
            confirm_note(&shop18, "main", &tickets),
            "SHOP-20 Checkout page still waits for SHOP-19 to merge; it starts from main once both parents are \
             merged. The worktree for SHOP-18 is removed unless it has uncommitted changes."
        );
        assert_eq!(merging_steps("main").len(), 4);
    }

    #[test]
    fn the_conflict_strip_names_the_files() {
        use otaq_core::board::conflicts::{Conflict, ConflictReason};
        let c = Conflict {
            target: "main".into(),
            files: vec![
                "src/lib/stripe/session.ts".into(),
                "src/lib/cart/validate.ts".into(),
            ],
            reason: ConflictReason::Merge,
            moved: Vec::new(),
        };
        assert_eq!(
            conflict_line(&c),
            "Conflicts with main \u{b7} 2 files (src/lib/stripe/session.ts, src/lib/cart/validate.ts)"
        );
        assert_eq!(moved_line(&c), None, "git said nothing: no line");
    }

    #[test]
    fn the_conflict_says_what_moved_and_where_it_stands() {
        use otaq_core::board::conflicts::{Conflict, ConflictReason, Moved};
        let c = Conflict {
            target: "main".into(),
            files: vec!["README.md".into(), "a.ts".into(), "b.ts".into()],
            reason: ConflictReason::TestMerge,
            moved: vec![
                Moved {
                    by: "SHOP-6".into(),
                    files: vec!["README.md".into()],
                },
                Moved {
                    by: "4fa97d6".into(),
                    files: vec!["a.ts".into(), "b.ts".into()],
                },
            ],
        };
        assert_eq!(
            moved_line(&c).as_deref(),
            Some("SHOP-6 changed README.md \u{b7} 4fa97d6 changed a.ts and b.ts")
        );
        let key: TicketKey = "SHOP-5".parse().unwrap();
        assert_eq!(
            nothing_merged_line(&key, "main"),
            "Nothing was merged into main. SHOP-5 returns to Review when it\u{2019}s fixed."
        );
        assert_eq!(in_a_row(1), "1 of 3");
    }

    #[test]
    fn every_result_has_its_state() {
        let key: TicketKey = "SHOP-18".parse().unwrap();
        let t = Box::new(ticket("SHOP-18", "Checkout API", Status::Merged));
        assert_eq!(
            view(
                &key,
                &LocalMergeOutcome::Merged {
                    ticket: t,
                    target: "main".into(),
                    commit: "9f3e2c1aaaa".into(),
                    started: vec!["SHOP-20".parse().unwrap(), "SHOP-21".parse().unwrap()],
                    hook_changed: None,
                }
            ),
            MergeView::Merged {
                title: "SHOP-18 merged into main".into(),
                text: "Squashed as 9f3e2c1".into(),
                started: Some("2 waiting tickets started".into()),
            }
        );
        let MergeView::Conflict { title, text, files } = view(
            &key,
            &LocalMergeOutcome::Conflict {
                target: "main".into(),
                files: vec!["package.json".into(), "src/lib/stripe/session.ts".into()],
            },
        ) else {
            panic!()
        };
        assert_eq!(title, "Merging main in stopped \u{2014} conflict");
        assert!(text.contains("conflicts in 2 files"));
        assert_eq!(files.len(), 2);
        assert!(matches!(
            view(
                &key,
                &LocalMergeOutcome::RefusedDirtyCheckout {
                    message: "m".into()
                }
            ),
            MergeView::DirtyCheckout { .. }
        ));
        assert!(matches!(
            view(
                &key,
                &LocalMergeOutcome::RefusedDiverged {
                    message: "m".into()
                }
            ),
            MergeView::Diverged { .. }
        ));
        assert!(matches!(
            view(
                &key,
                &LocalMergeOutcome::RefusedUncommitted {
                    files: vec![],
                    message: "SHOP-18 has 2 uncommitted files".into()
                }
            ),
            MergeView::Uncommitted { .. }
        ));
        assert_eq!(started_label(0), None);
        assert_eq!(
            started_label(1).as_deref(),
            Some("1 waiting ticket started")
        );
    }
}
