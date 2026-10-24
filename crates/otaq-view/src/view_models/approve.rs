//! Approve ▾ and Local merge (GIT-03, REV-08, UI-02; d0015, d0038, d0050, d0058; *07 → Local merge · confirm,
//! success, rebasing, conflict, refused*; *03 → Approve choice on drop*): the choice (Local merge, Open PR from
//! M10 on delivery units only), the confirmation's texts, and one state per Local merge result. Nothing here
//! pushes, and no state offers to stash or discard.

use otaq_core::board::review::{ApproveCheck, changed_message};
use otaq_core::board::tickets::{Base, Ticket};
use otaq_core::domain::{Status, TicketKey};
use otaq_core::pr::provider::PrProvider;
use otaq_core::review::LocalMergeOutcome;

/// One choice in the Approve menu ("1 Local merge", "2 Open PR on GitHub").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApproveOption {
    pub choice: ApproveChoice,
    pub title: String,
    pub detail: String,
    pub enabled: bool,
}

/// `ApproveChoice` on the page: `"local_merge"`, `"open_pr"`, `"push_to_pr"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "snake_case")]
pub enum ApproveChoice {
    LocalMerge,
    OpenPr,
    /// A ticket with an open PR: its new commits go to that PR (REV-14, d0041).
    PushToPr,
}

/// "Approve SHOP-18 Checkout API", "Approve epic SHOP-40 Wishlist", "Push SHOP-26 Add sitemap to PR #481".
pub fn menu_title(ticket: &Ticket) -> String {
    if let Some(n) = ticket.flags.pr_number {
        format!("Push {} {} to PR #{n}", ticket.key, ticket.title)
    } else if ticket.is_epic() {
        format!("Approve epic {} {}", ticket.key, ticket.title)
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
    } else if ticket.epic.is_some() {
        "Open PR isn\u{2019}t offered for epic children: the epic opens one PR for all of them."
            .to_owned()
    } else {
        "Both open a confirmation step before anything runs.".to_owned()
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

/// The menu: Local merge always; Open PR only on delivery units (standalone tickets and epics, never epic
/// children, d0059). With the built-in Open PR skill and no detected provider, Open PR is disabled with "PR provider
/// not detected" (REV-11). A ticket with an open PR offers only Push to PR #N (REV-14, d0041).
pub fn options(
    ticket: &Ticket,
    target: &str,
    tickets: &[Ticket],
    pr: Option<&PrProvider>,
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
    let (title, mut detail) = match &ticket.base {
        // An epic child squashes back into its epic's branch (*Epic child · Approve offers only Local merge*).
        Base::Epic(epic) => (
            format!("Local merge into {epic}"),
            format!("Squash into {target} \u{b7} local, never pushed"),
        ),
        Base::Branch(_) if ticket.is_epic() => (
            format!("Local merge into {target}"),
            format!("Squash the whole feature into {target} \u{b7} never pushes"),
        ),
        Base::Branch(_) => ("Local merge".to_owned(), format!("Squash into {target}")),
    };
    if let Some((key, _, others)) = still_waiting(ticket, tickets).first() {
        detail.push_str(&format!(" \u{b7} {key} still waits for {}", keys(others)));
    }
    let mut out = vec![ApproveOption {
        choice: ApproveChoice::LocalMerge,
        title,
        detail,
        enabled: true,
    }];
    if ticket.epic.is_none() {
        let blocked = pr.and_then(PrProvider::open_pr_blocked);
        let detail = match blocked {
            Some(why) => why.to_owned(),
            // The epic is the delivery unit: one PR, one commit per merged child (EPC-08, d0059).
            None if ticket.is_epic() => format!("One commit per child \u{b7} into {target}"),
            None => format!(
                "The Open PR skill proposes; you confirm before anything is pushed \u{b7} into {target}"
            ),
        };
        let mut title = open_pr_title(pr);
        if ticket.is_epic() {
            match &ticket.external_id {
                Some(id) => title.push_str(&format!(" \u{b7} one PR for {id}")),
                None => title.push_str(" \u{b7} one PR for the whole epic"),
            }
        }
        out.push(ApproveOption {
            choice: ApproveChoice::OpenPr,
            title,
            detail,
            enabled: blocked.is_none(),
        });
    }
    out
}

/// "Open PR on GitHub", or plain "Open PR" when no provider is detected (a custom skill decides).
pub fn open_pr_title(pr: Option<&PrProvider>) -> String {
    match pr.and_then(|p| p.provider) {
        Some(provider) => format!("Open PR on {}", provider.name()),
        None => "Open PR".to_owned(),
    }
}

/// "Local merge SHOP-18 Checkout API".
pub fn confirm_title(ticket: &Ticket) -> String {
    format!("Local merge {} {}", ticket.key, ticket.title)
}

/// "Into (SHOP-18’s base branch)", or the epic's branch for a child.
pub fn into_label(ticket: &Ticket) -> String {
    match &ticket.base {
        Base::Epic(epic) => format!("Into (the epic {epic}\u{2019}s branch)"),
        Base::Branch(_) => format!("Into ({}\u{2019}s base branch)", ticket.key),
    }
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

/// The steps the "Merging…" state lists (*Local merge · rebasing onto main…*).
pub fn merging_steps(target: &str, into_epic: bool) -> Vec<String> {
    let mut steps = Vec::new();
    if !into_epic {
        steps.push(format!("Fetch {target} and bring it up to date"));
    }
    steps.push(format!("Rebase onto {target}"));
    steps.push(format!("Squash into {target}"));
    steps.push("Start waiting tickets whose dependencies are all merged".to_owned());
    steps
}

/// What the panel shows after the click.
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
    /// "shop-web has uncommitted changes on main. Clean up and retry." (Retry).
    DirtyCheckout { message: String },
    /// The base diverged from its remote (Retry).
    Diverged { message: String },
    /// "SHOP-19 has 2 uncommitted files" · View · Send back.
    Uncommitted { message: String, files: Vec<String> },
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

pub fn view(key: &TicketKey, outcome: &LocalMergeOutcome) -> MergeView {
    match outcome {
        LocalMergeOutcome::Merged {
            target,
            commit,
            started,
            ..
        } => MergeView::Merged {
            title: format!("Local merge done \u{b7} {key}"),
            text: format!("Squashed into {target} as {}", short(commit)),
            started: started_label(started.len()),
        },
        LocalMergeOutcome::Conflict { target, files } => MergeView::Conflict {
            title: format!("Rebase onto {target} stopped \u{2014} conflict"),
            text: format!(
                "{target} moved since {key} was built. The rebase onto the latest {target} conflicts in {}. It \
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
        // Until the dialog's own states (T-0492): the check's comment, and a stop as a refusal to retry.
        LocalMergeOutcome::CheckFailed {
            target,
            command,
            exit,
        } => MergeView::Conflict {
            title: format!("Merge check failed on {target}"),
            text: otaq_core::domain::validation::merge_check_failed(command, target, *exit),
            files: Vec::new(),
        },
        LocalMergeOutcome::CheckStopped { target } => MergeView::DirtyCheckout {
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

/// The panel header's strip: "Conflicts with main · 2 files (src/lib/stripe/session.ts, package.json)".
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

/// What happens after the conflict (FLW-17, no click): the agent fixes it, or the user does by hand.
pub fn conflict_help(key: &TicketKey, target: &str) -> String {
    format!(
        "{key} went back to its agent, which rebases onto {target}, resolves the conflict and moves on again. Or \
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

    #[test]
    fn a_ticket_with_a_pr_offers_only_push_to_pr() {
        let mut t = ticket("SHOP-26", "Add sitemap", Status::WaitingForYou);
        t.flags.pr_number = Some(481);
        t.work = Some(otaq_core::board::tickets::Work {
            branch: Some("feature/SHOP-26-add-sitemap".into()),
            ..Default::default()
        });
        let only = options(&t, "main", &[], None);
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
        let open = &options(&shop18, "main", &tickets, Some(&pr))[1];
        assert_eq!(open.title, "Open PR");
        assert_eq!(open.detail, "PR provider not detected");
        assert!(!open.enabled);
        pr.provider = Some(Provider::GitHub);
        let open = &options(&shop18, "main", &tickets, Some(&pr))[1];
        assert_eq!(open.title, "Open PR on GitHub");
        assert_ne!(open.detail, "PR provider not detected");
    }

    #[test]
    fn the_menu_offers_local_merge_and_open_pr() {
        let (shop18, tickets) = sample();
        let options = options(&shop18, "main", &tickets, None);
        assert_eq!(menu_title(&shop18), "Approve SHOP-18 Checkout API");
        assert_eq!(options[0].choice, ApproveChoice::LocalMerge);
        assert_eq!(
            options[0].detail,
            "Squash into main \u{b7} SHOP-20 still waits for SHOP-19"
        );
        assert!(options[0].enabled);
        assert_eq!(options[1].choice, ApproveChoice::OpenPr);
        assert!(options[1].enabled, "Open PR works from M10");
        let mut child = ticket("SHOP-40.1", "Wishlist API", Status::WaitingForYou);
        child.epic = Some("SHOP-40".parse().unwrap());
        child.base = Base::Epic("SHOP-40".parse().unwrap());
        let epic_branch = otaq_core::naming::branch("SHOP-40");
        let only = super::options(&child, &epic_branch, &[], None);
        assert_eq!(only.len(), 1, "epic children only merge locally");
        assert_eq!(only[0].title, "Local merge into SHOP-40");
        assert_eq!(
            only[0].detail,
            format!("Squash into {epic_branch} \u{b7} local, never pushed")
        );
        assert!(menu_note(&child).starts_with("Open PR isn"));
        let mut epic = ticket("SHOP-40", "Wishlist", Status::WaitingForYou);
        epic.kind = otaq_core::domain::Kind::Epic;
        assert_eq!(menu_title(&epic), "Approve epic SHOP-40 Wishlist");
        let both = super::options(&epic, "main", &[], None);
        assert_eq!(both[0].title, "Local merge into main");
        assert_eq!(
            both[0].detail,
            "Squash the whole feature into main \u{b7} never pushes"
        );
        assert_eq!(
            both[1].choice,
            ApproveChoice::OpenPr,
            "the epic offers Open PR (M10)"
        );
        assert_eq!(both[1].title, "Open PR \u{b7} one PR for the whole epic");
        assert_eq!(both[1].detail, "One commit per child \u{b7} into main");
        epic.external_id = Some("PAY-123".into());
        assert_eq!(
            super::options(&epic, "main", &[], None)[1].title,
            "Open PR \u{b7} one PR for PAY-123"
        );
        assert_eq!(
            into_label(&child),
            "Into (the epic SHOP-40\u{2019}s branch)"
        );
    }

    #[test]
    fn the_confirmation_names_the_target_and_who_keeps_waiting() {
        let (shop18, tickets) = sample();
        assert_eq!(confirm_title(&shop18), "Local merge SHOP-18 Checkout API");
        assert_eq!(into_label(&shop18), "Into (SHOP-18\u{2019}s base branch)");
        assert_eq!(
            confirm_note(&shop18, "main", &tickets),
            "SHOP-20 Checkout page still waits for SHOP-19 to merge; it starts from main once both parents are \
             merged. The worktree for SHOP-18 is removed unless it has uncommitted changes."
        );
        assert_eq!(merging_steps("main", false).len(), 4);
        assert_eq!(merging_steps("epic-branch", true).len(), 3, "no fetch");
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
        };
        assert_eq!(
            conflict_line(&c),
            "Conflicts with main \u{b7} 2 files (src/lib/stripe/session.ts, src/lib/cart/validate.ts)"
        );
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
                }
            ),
            MergeView::Merged {
                title: "Local merge done \u{b7} SHOP-18".into(),
                text: "Squashed into main as 9f3e2c1".into(),
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
        assert_eq!(title, "Rebase onto main stopped \u{2014} conflict");
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
