//! The PR dialogs as the Tauri page draws them (T-0227; REV-10, REV-11, REV-12, REV-13, REV-14, EPC-08, CLS-02, UI-07;
//! d0041, d0050, d0051, d0059): [`pr_dialog`] turned into plain, serialized shapes. *07 → Open PR · skill is
//! preparing…, confirmation, pushing, success, invalid or taken branch name, not logged in, Approve · Push to PR #481,
//! Push to PR · confirmation, Open PR · epic SHOP-40*; *05 → SHOP-26 · Approved, PR open*.
//!
//! The page keeps only the fields as the user types them and the click; the dialog's state (the proposal filled once,
//! the debounced name check) stays in [`PrDialog`].

use otaq_core::board::tickets::Ticket;
use otaq_core::pr::flow::{FlowKind, PrFlowView};
use serde::Serialize;
use ts_rs::TS;

use crate::view_models::card::{age_label, pr_provider_label};
use crate::view_models::pr_dialog::{self as pd, PrDialog, Stage};
use crate::view_models::review_page::ToastText;

/// One editable commit message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PrCommitField")]
pub struct CommitView {
    pub sha: String,
    pub message: String,
}

/// The line under the branch name: "Valid name · not taken on origin", or the problem in red.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PrBranchHint")]
pub struct HintView {
    pub text: String,
    pub error: bool,
}

/// A pushing step's mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "PrStepState")]
pub enum StepState {
    Done,
    Running,
    Waiting,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PrStep")]
pub struct StepView {
    pub text: String,
    pub state: StepState,
}

/// Where the dialog is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "PrStage")]
pub enum StageView {
    /// *Open PR · not logged in* (UI-07): the exact command; Otaq never logs in for you.
    NotLoggedIn {
        title: String,
        text: String,
        login: String,
        azure: &'static str,
        retry: &'static str,
    },
    /// *Open PR · skill is preparing…*: the fields wait, greyed.
    Preparing { line: String, detail: String },
    /// *Open PR · confirmation*: every field editable.
    Proposed,
    /// *Open PR · pushing*: live steps.
    Pushing { title: String, steps: Vec<StepView> },
    /// The skill reported a problem instead of a PR: its message, Retry.
    Reported {
        title: String,
        message: String,
        text: &'static str,
        retry: &'static str,
    },
}

/// What `pr.view` answers: the dialog for its stage, with every text worded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PrDialogView")]
pub struct DialogView {
    pub key: String,
    /// Push to PR #N (only the new commits; no title or description).
    pub push: bool,
    pub heading: String,
    pub stage: StageView,
    /// "Proposed by /otaq:open-pr · edit anything"
    pub proposed_by: String,
    /// "GitHub · acme/shop-web"
    pub provider: Option<String>,
    /// "gh: logged in"
    pub login: Option<String>,
    pub branch_label: &'static str,
    pub branch: String,
    pub branch_locked: bool,
    pub branch_hint: HintView,
    /// The name check the page asks for after the debounce names this edit.
    pub check: u64,
    pub check_after_ms: u64,
    pub commits_label: String,
    pub commits: Vec<CommitView>,
    pub title_label: &'static str,
    pub title: String,
    pub description_label: &'static str,
    pub description: String,
    /// "Into main · SHOP-18’s base branch" / "PR #481 · into main · GitHub".
    pub target_line: String,
    pub confirm: String,
    pub can_confirm: bool,
    pub cancel: &'static str,
    /// "cancel" after the Esc key.
    pub hint: &'static str,
}

fn provider_name(d: &PrDialog) -> Option<&str> {
    d.provider
        .as_deref()
        .map(|p| p.split(" \u{b7} ").next().unwrap_or(p))
}

/// The steps with their marks: the ones before the push are done, the push runs, the PR waits for it.
pub fn steps(d: &PrDialog) -> Vec<StepView> {
    let all = d.pushing_steps();
    let count = all.len();
    all.into_iter()
        .enumerate()
        .map(|(i, text)| StepView {
            text,
            state: match (i + 2).cmp(&count) {
                std::cmp::Ordering::Less => StepState::Done,
                std::cmp::Ordering::Equal => StepState::Running,
                std::cmp::Ordering::Greater => StepState::Waiting,
            },
        })
        .collect()
}

pub fn dialog_view(d: &PrDialog) -> DialogView {
    let push = d.kind == FlowKind::Push;
    let stage = match &d.stage {
        Stage::NotLoggedIn { cli, login } => StageView::NotLoggedIn {
            title: format!("Can\u{2019}t open the PR \u{2014} {cli} is not signed in"),
            text: format!(
                "Run this in any terminal, then click Retry. {} never logs in for you.",
                otaq_core::naming::PRODUCT_NAME
            ),
            login: format!("$ {login}"),
            azure: "Azure DevOps: az login && az extension add --name azure-devops",
            retry: "Retry",
        },
        Stage::Preparing => StageView::Preparing {
            line: if push {
                "Push skill is preparing\u{2026}".to_owned()
            } else {
                "Open PR skill is preparing\u{2026}".to_owned()
            },
            detail: format!("{} \u{b7} reading the commits", d.skill),
        },
        Stage::Proposed => StageView::Proposed,
        Stage::Pushing => StageView::Pushing {
            title: format!("Pushing {}\u{2026}", d.key),
            steps: steps(d),
        },
        Stage::Reported { message } => StageView::Reported {
            title: format!("The skill stopped: {} wasn\u{2019}t pushed", d.key),
            message: message.clone(),
            text: "Fix it (for example sign in), then Retry. Cancel discards the PR.",
            retry: "Retry",
        },
    };
    let (hint, error) = d.branch_hint();
    let target_line = match (push, d.pr_number) {
        (true, Some(n)) => format!(
            "PR #{n} \u{b7} into {}{}",
            d.target,
            provider_name(d)
                .map(|p| format!(" \u{b7} {p}"))
                .unwrap_or_default()
        ),
        _ => d.target_line(),
    };
    DialogView {
        key: d.key.clone(),
        push,
        heading: d.heading(),
        stage,
        proposed_by: format!("Proposed by {} \u{b7} edit anything", d.skill),
        provider: d.provider.clone(),
        login: d.login.clone(),
        branch_label: "Branch name",
        branch: d.branch.clone(),
        branch_locked: d.branch_locked,
        branch_hint: HintView { text: hint, error },
        check: d.debounce.generation(),
        check_after_ms: pd::CHECK_AFTER_MS,
        commits_label: if push || d.stage == Stage::Proposed {
            d.commits_label()
        } else {
            "Commit messages".to_owned()
        },
        commits: d
            .commits
            .iter()
            .map(|c| CommitView {
                sha: c.sha.clone(),
                message: c.message.clone(),
            })
            .collect(),
        title_label: "PR title",
        title: d.title.clone(),
        description_label: "Description",
        description: d.description.clone(),
        target_line,
        confirm: d.confirm_label(),
        can_confirm: d.can_confirm(),
        cancel: "Cancel",
        hint: "cancel",
    }
}

/// *Approve · Push to PR #481* (REV-14, d0041): before the Push skill runs, what goes to the PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PrPushIntro")]
pub struct PushIntro {
    pub title: String,
    /// "PR #481 · Add sitemap · GitHub"
    pub pr: String,
    pub branch: String,
    /// "2 new commits since the PR was opened"
    pub commits: String,
    pub note: String,
    pub confirm: String,
    pub cancel: &'static str,
}

pub fn push_intro(ticket: &Ticket, flow: &PrFlowView) -> Option<PushIntro> {
    let n = flow.pr_number.or(ticket.flags.pr_number)?;
    let provider = flow
        .pr_url
        .as_deref()
        .and_then(otaq_core::pr::provider::Provider::detect)
        .map(|p| format!(" \u{b7} {}", p.name()))
        .unwrap_or_default();
    let new = flow.commits.iter().filter(|c| !c.pushed).count();
    Some(PushIntro {
        title: format!("Push {} {} to PR #{n}", ticket.key, ticket.title),
        pr: format!("PR #{n} \u{b7} {}{provider}", ticket.title),
        branch: flow.branch.clone(),
        commits: match new {
            0 => "No new commits since the PR was opened".to_owned(),
            1 => "1 new commit since the PR was opened".to_owned(),
            n => format!("{n} new commits since the PR was opened"),
        },
        note: format!(
            "Pushes the branch to origin. {} doesn\u{2019}t watch the PR; use Mark merged or Mark closed once the PR is merged or closed.",
            otaq_core::naming::PRODUCT_NAME
        ),
        confirm: format!("Push to PR #{n}"),
        cancel: "Cancel",
    })
}

/// The toast after the skill reported the PR (*Open PR · success*): "PR #482 opened on GitHub", "SHOP-18 → main ·
/// github.com/acme/shop-web/pull/482".
pub fn success_toast(ticket: &Ticket, kind: FlowKind, target: &str) -> Option<ToastText> {
    let n = ticket.flags.pr_number?;
    let url = ticket.work.as_ref().and_then(|w| w.pr_url.clone());
    let on = pr_provider_label(ticket)
        .map(|p| format!(" on {p}"))
        .unwrap_or_default();
    let title = match kind {
        FlowKind::Open => format!("PR #{n} opened{on}"),
        FlowKind::Push => format!("Pushed to PR #{n}"),
    };
    let link = url
        .as_deref()
        .map(|u| u.split_once("://").map_or(u, |(_, rest)| rest).to_owned());
    Some(ToastText {
        title,
        text: Some(match link {
            Some(link) => format!("{} \u{2192} {target} \u{b7} {link}", ticket.key),
            None => format!("{} \u{2192} {target}", ticket.key),
        }),
    })
}

/// *05 → SHOP-26 · Approved, PR open*: the PR box under the tabs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "PrOpenBox")]
pub struct OpenBox {
    /// "acme/shop-web #481"
    pub label: String,
    pub url: Option<String>,
    /// "opened 2 h ago"
    pub opened: Option<String>,
    /// "feature/SHOP-26-add-sitemap → main"
    pub branch: String,
    pub note: String,
}

/// The box while the ticket's PR is open (REV-13): Otaq doesn't watch it; Mark merged or Mark closed.
pub fn open_box(ticket: &Ticket, now_ms: i64) -> Option<OpenBox> {
    if ticket.status != otaq_core::domain::Status::PrOpen {
        return None;
    }
    let n = ticket.flags.pr_number?;
    let work = ticket.work.as_ref();
    let url = work.and_then(|w| w.pr_url.clone());
    let target = ticket.base.branch().to_owned();
    let branch = work.and_then(|w| w.branch.clone()).unwrap_or_default();
    Some(OpenBox {
        label: url
            .as_deref()
            .map_or_else(|| format!("PR #{n}"), |u| pd::pr_label(u, n)),
        url,
        opened: ticket
            .status_since
            .map(|since| format!("opened {} ago", age_label(now_ms - since))),
        branch: format!("{branch} \u{2192} {target}"),
        note: pd::pr_open_note(pr_provider_label(ticket).as_deref(), &target),
    })
}

#[cfg(test)]
mod tests {
    use otaq_core::board::tickets::Work;
    use otaq_core::domain::Status;
    use otaq_core::pr::flow::{FlowCommit, FlowState, PrFlow};
    use otaq_core::pr::proposal::{CommitMessage, Proposal};
    use otaq_core::pr::provider::{PrProvider, Provider};

    use super::*;
    use crate::view_models::card_fixtures::ticket;

    fn github(logged_in: bool) -> PrProvider {
        PrProvider {
            remote: Some("github.com/acme/shop-web".into()),
            provider: Some(Provider::GitHub),
            logged_in: Some(logged_in),
            cli_found: true,
            open_pr_skill: "open-pr".to_owned(),
        }
    }

    fn shop18() -> Ticket {
        let mut t = ticket("SHOP-18", "Checkout API", Status::WaitingForYou);
        t.work = Some(Work {
            branch: Some(otaq_core::naming::branch("SHOP-18")),
            ..Default::default()
        });
        t
    }

    fn flow(state: FlowState, pr: Option<u32>) -> PrFlowView {
        PrFlowView {
            key: "SHOP-18".parse().unwrap(),
            flow: Some(PrFlow {
                kind: if pr.is_some() {
                    FlowKind::Push
                } else {
                    FlowKind::Open
                },
                state,
                proposal: Some(Proposal {
                    branch: "feature/SHOP-18-checkout-api".into(),
                    commits: vec![
                        CommitMessage {
                            sha: "a1b2c3d4".into(),
                            message: "feat(checkout): add POST /checkout".into(),
                        },
                        CommitMessage {
                            sha: "e5f6a7b8".into(),
                            message: "test(checkout): cover declined cards".into(),
                        },
                    ],
                    title: "Checkout API (SHOP-18)".into(),
                    description: "Adds POST /api/checkout.".into(),
                }),
                confirmed: None,
                reported: None,
                started_at: 0,
                automatic: false,
            }),
            target: "main".into(),
            branch: otaq_core::naming::branch("SHOP-18"),
            branch_locked: pr.is_some(),
            commits: vec![
                FlowCommit {
                    sha: "a1".into(),
                    message: "x".into(),
                    pushed: true,
                },
                FlowCommit {
                    sha: "b2".into(),
                    message: "y".into(),
                    pushed: false,
                },
                FlowCommit {
                    sha: "c3".into(),
                    message: "z".into(),
                    pushed: false,
                },
            ],
            pr_number: pr,
            pr_url: pr.map(|n| format!("https://github.com/acme/shop-web/pull/{n}")),
            external_id: None,
            skill: "open-pr".to_owned(),
        }
    }

    /// WSK-08, REV-11: an imported Open PR skill is shown by its name, and it handles a provider's login itself.
    #[test]
    fn an_imported_skill_is_shown_by_its_name() {
        let mut provider = github(false);
        provider.open_pr_skill = "my-pr".into();
        let mut d = PrDialog::opening(&shop18(), "my-pr", Some(&provider));
        let v = dialog_view(&d);
        assert_eq!(
            v.stage,
            StageView::Preparing {
                line: "Open PR skill is preparing\u{2026}".into(),
                detail: "my-pr \u{b7} reading the commits".into(),
            }
        );
        d.update(&PrFlowView {
            skill: "my-pr".into(),
            ..flow(FlowState::Proposed, None)
        });
        assert_eq!(
            dialog_view(&d).proposed_by,
            "Proposed by my-pr \u{b7} edit anything"
        );
    }

    #[test]
    fn every_stage_reads_like_its_frame() {
        let skill = "open-pr".to_owned();
        let mut d = PrDialog::opening(&shop18(), &skill, Some(&github(true)));
        let v = dialog_view(&d);
        assert_eq!(v.heading, "Open PR for SHOP-18 Checkout API");
        assert_eq!(
            v.stage,
            StageView::Preparing {
                line: "Open PR skill is preparing\u{2026}".into(),
                detail: format!("{skill} \u{b7} reading the commits"),
            }
        );
        assert_eq!(v.commits_label, "Commit messages");
        assert_eq!(v.confirm, "Push & open PR");
        assert!(!v.can_confirm && !v.push);

        d.update(&flow(FlowState::Proposed, None));
        let v = dialog_view(&d);
        assert_eq!(v.stage, StageView::Proposed);
        assert_eq!(
            v.proposed_by,
            format!("Proposed by {skill} \u{b7} edit anything")
        );
        assert_eq!(v.provider.as_deref(), Some("GitHub \u{b7} acme/shop-web"));
        assert_eq!(v.login.as_deref(), Some("gh: signed in"));
        assert_eq!(v.branch, "feature/SHOP-18-checkout-api");
        assert_eq!(v.commits_label, "Commit messages (2 commits)");
        assert_eq!(
            v.target_line,
            "Into main \u{b7} SHOP-18\u{2019}s base branch"
        );
        assert_eq!(v.branch_hint.text, "Checking\u{2026}");
        let generation = d.edit_branch("feature/checkout");
        assert_eq!(dialog_view(&d).check, generation);

        d.update(&flow(FlowState::Pushing, None));
        let StageView::Pushing { title, steps } = dialog_view(&d).stage else {
            panic!("not pushing");
        };
        assert_eq!(title, "Pushing SHOP-18\u{2026}");
        let marks: Vec<_> = steps.iter().map(|s| s.state).collect();
        assert_eq!(
            marks,
            [
                StepState::Done,
                StepState::Done,
                StepState::Running,
                StepState::Waiting
            ]
        );
        assert_eq!(steps[3].text, "Opening the PR on GitHub");

        let d = PrDialog::opening(&shop18(), &skill, Some(&github(false)));
        let StageView::NotLoggedIn { title, login, .. } = dialog_view(&d).stage else {
            panic!("logged in?");
        };
        assert_eq!(
            title,
            "Can\u{2019}t open the PR \u{2014} gh is not signed in"
        );
        assert_eq!(login, "$ gh auth login");
    }

    #[test]
    fn push_to_pr_has_its_intro_and_a_locked_branch() {
        let mut t = shop18();
        t.key = "SHOP-26".parse().unwrap();
        t.title = "Add sitemap".into();
        t.flags.pr_number = Some(481);
        let f = flow(FlowState::Proposed, Some(481));
        let intro = push_intro(&t, &f).unwrap();
        assert_eq!(intro.title, "Push SHOP-26 Add sitemap to PR #481");
        assert_eq!(intro.pr, "PR #481 \u{b7} Add sitemap \u{b7} GitHub");
        assert_eq!(intro.commits, "2 new commits since the PR was opened");
        assert_eq!(intro.confirm, "Push to PR #481");
        assert!(push_intro(&shop18(), &flow(FlowState::Proposed, None)).is_none());

        let mut d = PrDialog::opening(&t, "push", Some(&github(true)));
        d.update(&f);
        let v = dialog_view(&d);
        assert!(v.push && v.branch_locked);
        assert_eq!(
            v.branch_hint.text,
            "Locked: set at the first push (Open PR)"
        );
        assert_eq!(v.target_line, "PR #481 \u{b7} into main \u{b7} GitHub");
        assert_eq!(v.confirm, "Push to PR #481");
    }

    #[test]
    fn the_pr_open_box_and_the_success_toast() {
        let mut t = ticket("SHOP-26", "Add sitemap", Status::PrOpen);
        t.flags.pr_number = Some(481);
        t.status_since = Some(0);
        t.work = Some(Work {
            branch: Some("feature/SHOP-26-add-sitemap".into()),
            pr_url: Some("https://github.com/acme/shop-web/pull/481".into()),
            ..Default::default()
        });
        let b = open_box(&t, 2 * 3600 * 1000).unwrap();
        assert_eq!(b.label, "acme/shop-web #481");
        assert_eq!(b.branch, "feature/SHOP-26-add-sitemap \u{2192} main");
        assert!(b.opened.unwrap().starts_with("opened "));
        assert!(
            b.note
                .contains("When it is merged on GitHub, click Mark merged")
        );
        let toast = success_toast(&t, FlowKind::Open, "main").unwrap();
        assert_eq!(toast.title, "PR #481 opened on GitHub");
        assert_eq!(
            toast.text.as_deref(),
            Some("SHOP-26 \u{2192} main \u{b7} github.com/acme/shop-web/pull/481")
        );
        assert_eq!(
            success_toast(&t, FlowKind::Push, "main").unwrap().title,
            "Pushed to PR #481"
        );
        t.status = Status::WaitingForYou;
        assert!(open_box(&t, 0).is_none(), "only while the PR is open");
    }
}
