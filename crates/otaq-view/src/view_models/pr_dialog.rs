//! The Open PR / Push to PR dialog (REV-10, REV-14, UI-07; d0050, d0041, d0059; *07 → Open PR · skill is
//! preparing…, Open PR · confirmation, Open PR · pushing, Open PR · success, Open PR · invalid or taken branch name,
//! Open PR · not logged in, Push to PR · confirmation, Open PR · epic SHOP-40*). Everything the skill proposed is
//! editable; the branch name is checked live (debounced); Push to PR locks the branch and shows only the new
//! commits. Nothing is pushed before the confirm click.

use otaq_core::board::tickets::Ticket;
use otaq_core::pr::flow::{BranchCheck, FlowKind, FlowState, PrFlowView};
use otaq_core::pr::proposal::{CommitMessage, Proposal};
use otaq_core::pr::provider::PrProvider;

/// How long the branch name must stay unchanged before it is checked against the repository and origin.
pub const CHECK_AFTER_MS: u64 = 300;

/// Where the dialog is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    /// The built-in skill's CLI isn't logged in: the exact command, Retry (UI-07).
    NotLoggedIn { cli: String, login: String },
    /// The skill runs; no proposal yet.
    Preparing,
    /// The proposal is shown, editable.
    Proposed,
    /// The user confirmed: live steps until the skill reports the PR.
    Pushing,
    /// The skill reported a problem instead of a PR (a rejected push): the message, Retry.
    Reported { message: String },
}

/// The branch name's live check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameCheck {
    /// Not checked yet (just typed).
    Pending,
    Ok,
    Problem(String),
}

/// The debounce for the live check: each edit gets a number; only the newest one is checked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Debounce {
    generation: u64,
}

impl Debounce {
    /// An edit happened: returns its number.
    pub fn edit(&mut self) -> u64 {
        self.generation += 1;
        self.generation
    }

    /// The newest edit's number.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Whether the check started for edit `generation` is still the newest one.
    pub fn is_current(&self, generation: u64) -> bool {
        generation == self.generation
    }
}

/// One editable commit message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitField {
    pub sha: String,
    pub message: String,
}

/// The dialog's data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrDialog {
    pub kind: FlowKind,
    pub epic: bool,
    pub key: String,
    pub ticket_title: String,
    pub skill: String,
    pub target: String,
    /// "GitHub · acme/shop-web"; `None` when no provider is detected (a custom skill).
    pub provider: Option<String>,
    /// "gh: logged in".
    pub login: Option<String>,
    pub stage: Stage,
    /// The branch before the proposal: `otaq/SHOP-18`.
    pub branch_was: String,
    pub branch: String,
    pub branch_locked: bool,
    pub check: NameCheck,
    pub commits: Vec<CommitField>,
    pub title: String,
    pub description: String,
    pub pr_number: Option<u32>,
    pub debounce: Debounce,
}

impl PrDialog {
    /// The dialog for `ticket` as it opens (the skill is starting), or the not-logged-in state.
    pub fn opening(ticket: &Ticket, skill: &str, provider: Option<&PrProvider>) -> Self {
        let kind = if ticket.flags.pr_number.is_some() {
            FlowKind::Push
        } else {
            FlowKind::Open
        };
        let branch = ticket
            .work
            .as_ref()
            .and_then(|w| w.branch.clone())
            .unwrap_or_default();
        let stage = match provider.and_then(|p| Some((p.provider?, p))) {
            Some((detected, p))
                if p.built_in_open_pr() && p.cli_found && p.logged_in == Some(false) =>
            {
                let cli = detected.tool().command().to_owned();
                let login = if cli == "gh" {
                    "gh auth login"
                } else {
                    "az login"
                };
                Stage::NotLoggedIn {
                    cli,
                    login: login.to_owned(),
                }
            }
            _ => Stage::Preparing,
        };
        Self {
            kind,
            epic: ticket.is_epic(),
            key: ticket.key.to_string(),
            ticket_title: ticket.title.clone(),
            skill: skill.to_owned(),
            target: String::new(),
            provider: provider.and_then(|p| {
                let name = p.provider?.name();
                Some(match &p.remote {
                    Some(remote) => format!("{name} \u{b7} {}", repo_path(remote)),
                    None => name.to_owned(),
                })
            }),
            login: provider.and_then(|p| {
                let cli = p.provider?.tool().command();
                Some(match p.logged_in {
                    Some(true) => format!("{cli}: signed in"),
                    Some(false) => format!("{cli}: not signed in"),
                    None => format!("{cli}: login unknown"),
                })
            }),
            stage,
            branch_was: branch.clone(),
            branch,
            branch_locked: ticket.work.as_ref().is_some_and(|w| w.branch_locked),
            check: NameCheck::Pending,
            commits: Vec::new(),
            title: String::new(),
            description: String::new(),
            pr_number: ticket.flags.pr_number,
            debounce: Debounce::default(),
        }
    }

    /// Takes in the service's view of the flow: the proposal fills the fields once (later edits are the user's).
    pub fn update(&mut self, view: &PrFlowView) {
        self.target = view.target.clone();
        self.branch_locked = view.branch_locked;
        self.pr_number = view.pr_number;
        let Some(flow) = &view.flow else {
            return;
        };
        self.kind = flow.kind;
        if let Some(message) = &flow.reported {
            self.stage = Stage::Reported {
                message: message.clone(),
            };
            return;
        }
        match flow.state {
            FlowState::Preparing => {
                if !matches!(self.stage, Stage::NotLoggedIn { .. }) {
                    self.stage = Stage::Preparing;
                }
            }
            FlowState::Proposed if self.stage != Stage::Proposed => {
                let Some(p) = &flow.proposal else {
                    return;
                };
                self.stage = Stage::Proposed;
                self.branch_was = view.branch.clone();
                self.branch = if self.branch_locked {
                    view.branch.clone()
                } else {
                    p.branch.clone()
                };
                self.commits = p
                    .commits
                    .iter()
                    .map(|c| CommitField {
                        sha: c.sha.clone(),
                        message: c.message.clone(),
                    })
                    .collect();
                self.title = p.title.clone();
                self.description = p.description.clone();
                self.check = if self.branch_locked {
                    NameCheck::Ok
                } else {
                    NameCheck::Pending
                };
            }
            FlowState::Proposed => {}
            FlowState::Pushing => self.stage = Stage::Pushing,
        }
    }

    /// The branch name was edited: it must be checked again.
    pub fn edit_branch(&mut self, name: &str) -> u64 {
        self.branch = name.to_owned();
        self.check = NameCheck::Pending;
        self.debounce.edit()
    }

    /// A check answered; ignored when the name changed since.
    pub fn checked(&mut self, generation: u64, answer: &BranchCheck) {
        if !self.debounce.is_current(generation) {
            return;
        }
        self.check = match answer {
            BranchCheck::Ok => NameCheck::Ok,
            BranchCheck::Problem { message } => NameCheck::Problem(message.clone()),
        };
    }

    /// "Open PR for SHOP-18 Checkout API", "Open PR for epic SHOP-40 Wishlist", "Push SHOP-26 Add sitemap to PR
    /// #481".
    pub fn heading(&self) -> String {
        match (self.kind, self.pr_number) {
            (FlowKind::Push, Some(n)) => {
                format!("Push {} {} to PR #{n}", self.key, self.ticket_title)
            }
            _ if self.epic => format!("Open PR for epic {} {}", self.key, self.ticket_title),
            _ => format!("Open PR for {} {}", self.key, self.ticket_title),
        }
    }

    /// "Push & open PR" / "Push to PR #481".
    pub fn confirm_label(&self) -> String {
        match (self.kind, self.pr_number) {
            (FlowKind::Push, Some(n)) => format!("Push to PR #{n}"),
            _ => "Push & open PR".to_owned(),
        }
    }

    /// The commit list's label.
    pub fn commits_label(&self) -> String {
        let n = self.commits.len();
        match self.kind {
            FlowKind::Push => {
                format!("New commits ({n}) \u{b7} already pushed commits are not shown")
            }
            FlowKind::Open if self.epic => "Commit messages (one per merged child)".to_owned(),
            FlowKind::Open if n == 1 => "Commit message (1 commit)".to_owned(),
            FlowKind::Open => format!("Commit messages ({n} commits)"),
        }
    }

    /// The line under the branch name.
    pub fn branch_hint(&self) -> (String, bool) {
        if self.branch_locked {
            return ("Locked: set at the first push (Open PR)".to_owned(), false);
        }
        match &self.check {
            NameCheck::Pending => ("Checking…".to_owned(), false),
            NameCheck::Problem(message) => (message.clone(), true),
            NameCheck::Ok if self.branch == self.branch_was => {
                ("Valid name \u{b7} not taken on origin".to_owned(), false)
            }
            NameCheck::Ok => (
                format!(
                    "Valid name \u{b7} not taken on origin \u{b7} was {}",
                    self.branch_was
                ),
                false,
            ),
        }
    }

    /// "Into main · SHOP-18’s base branch" / "· the epic’s base branch".
    pub fn target_line(&self) -> String {
        if self.epic {
            format!("Into {} \u{b7} the epic\u{2019}s base branch", self.target)
        } else {
            format!(
                "Into {} \u{b7} {}\u{2019}s base branch",
                self.target, self.key
            )
        }
    }

    /// Push & open PR is enabled only for a complete proposal with a valid, free name.
    pub fn can_confirm(&self) -> bool {
        self.stage == Stage::Proposed
            && (self.branch_locked || self.check == NameCheck::Ok)
            && !self.commits.is_empty()
            && self.commits.iter().all(|c| !c.message.trim().is_empty())
            && (self.kind == FlowKind::Push || !self.title.trim().is_empty())
    }

    /// What the confirm click sends.
    pub fn proposal(&self) -> Proposal {
        Proposal {
            branch: self.branch.trim().to_owned(),
            commits: self
                .commits
                .iter()
                .map(|c| CommitMessage {
                    sha: c.sha.clone(),
                    message: c.message.trim().to_owned(),
                })
                .collect(),
            title: self.title.trim().to_owned(),
            description: self.description.trim().to_owned(),
        }
    }

    /// The pushing steps (*Open PR · pushing*).
    pub fn pushing_steps(&self) -> Vec<String> {
        let n = self.commits.len();
        let mut steps = vec![format!(
            "Applied your edits \u{b7} {n} {} reworded",
            if n == 1 { "commit" } else { "commits" }
        )];
        if !self.branch_locked && self.branch != self.branch_was {
            steps.push(format!(
                "Renamed {} \u{2192} {}",
                self.branch_was, self.branch
            ));
        }
        steps.push("Pushing to origin\u{2026}".to_owned());
        steps.push(match (self.kind, &self.provider) {
            (FlowKind::Push, _) => "Updating the PR".to_owned(),
            (FlowKind::Open, Some(p)) => format!(
                "Opening the PR on {}",
                p.split(" \u{b7} ").next().unwrap_or(p)
            ),
            (FlowKind::Open, None) => "Opening the PR".to_owned(),
        });
        steps
    }
}

/// "acme/shop-web" from "github.com/acme/shop-web".
fn repo_path(remote: &str) -> String {
    remote
        .split_once('/')
        .map_or(remote, |(_, path)| path)
        .to_owned()
}

/// The toast after the skill reported the PR: "PR opened #481" / "Pushed to PR #481".
pub fn success_toast(kind: FlowKind, number: u32) -> String {
    match kind {
        FlowKind::Open => format!("PR opened #{number}"),
        FlowKind::Push => format!("Pushed to PR #{number}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_models::card_fixtures::ticket;
    use otaq_core::domain::{Kind, Status};
    use otaq_core::pr::flow::{FlowCommit, PrFlow};
    use otaq_core::pr::provider::Provider;

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
        t.work = Some(otaq_core::board::tickets::Work {
            branch: Some(otaq_core::naming::branch("SHOP-18")),
            ..Default::default()
        });
        t
    }

    fn view(state: FlowState, branch_locked: bool) -> PrFlowView {
        PrFlowView {
            key: "SHOP-18".parse().unwrap(),
            flow: Some(PrFlow {
                kind: if branch_locked {
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
            branch_locked,
            commits: vec![FlowCommit {
                sha: "a1b2c3d4".into(),
                message: "x".into(),
                pushed: false,
            }],
            pr_number: branch_locked.then_some(481),
            pr_url: None,
            external_id: None,
            skill: "open-pr".to_owned(),
        }
    }

    #[test]
    fn preparing_then_proposed_with_a_live_name_check() {
        let mut d = PrDialog::opening(&shop18(), "open-pr", Some(&github(true)));
        assert_eq!(d.stage, Stage::Preparing);
        assert_eq!(d.heading(), "Open PR for SHOP-18 Checkout API");
        assert_eq!(d.provider.as_deref(), Some("GitHub \u{b7} acme/shop-web"));
        assert_eq!(d.login.as_deref(), Some("gh: signed in"));
        assert!(!d.can_confirm());
        d.update(&view(FlowState::Proposed, false));
        assert_eq!(d.stage, Stage::Proposed);
        assert_eq!(d.branch, "feature/SHOP-18-checkout-api");
        assert_eq!(d.commits_label(), "Commit messages (2 commits)");
        assert_eq!(
            d.target_line(),
            "Into main \u{b7} SHOP-18\u{2019}s base branch"
        );
        assert!(!d.can_confirm(), "the name isn't checked yet");

        // Debounce: only the newest edit's answer counts.
        let first = d.edit_branch("feature/SHOP-18 checkout");
        let second = d.edit_branch("feature/checkout");
        d.checked(
            first,
            &BranchCheck::Problem {
                message: "no spaces".into(),
            },
        );
        assert_eq!(d.check, NameCheck::Pending, "a stale answer is ignored");
        d.checked(
            second,
            &BranchCheck::Problem {
                message: "A branch with this name already exists on origin. Choose another name."
                    .into(),
            },
        );
        assert_eq!(
            d.branch_hint(),
            (
                "A branch with this name already exists on origin. Choose another name.".to_owned(),
                true
            )
        );
        assert!(!d.can_confirm());
        let third = d.edit_branch("feature/SHOP-18-checkout-api");
        d.checked(third, &BranchCheck::Ok);
        assert!(d.can_confirm());
        assert_eq!(
            d.branch_hint().0,
            format!(
                "Valid name \u{b7} not taken on origin \u{b7} was {}",
                otaq_core::naming::branch("SHOP-18")
            )
        );
        // A later refresh doesn't overwrite the user's edits.
        d.commits[0].message = "edited".into();
        d.update(&view(FlowState::Proposed, false));
        assert_eq!(d.commits[0].message, "edited");
        // Empty messages block the confirm.
        d.commits[1].message = " ".into();
        assert!(!d.can_confirm());
        d.commits[1].message = "ok".into();
        assert_eq!(d.proposal().commits[0].message, "edited");
        d.update(&view(FlowState::Pushing, false));
        assert_eq!(d.stage, Stage::Pushing);
        assert_eq!(
            d.pushing_steps(),
            [
                "Applied your edits \u{b7} 2 commits reworded".to_owned(),
                format!(
                    "Renamed {} \u{2192} feature/SHOP-18-checkout-api",
                    otaq_core::naming::branch("SHOP-18")
                ),
                "Pushing to origin\u{2026}".to_owned(),
                "Opening the PR on GitHub".to_owned(),
            ]
        );
    }

    #[test]
    fn push_to_pr_locks_the_branch() {
        let mut t = shop18();
        t.flags.pr_number = Some(481);
        t.title = "Add sitemap".into();
        let mut d = PrDialog::opening(&t, "push", Some(&github(true)));
        d.update(&view(FlowState::Proposed, true));
        assert_eq!(d.heading(), "Push SHOP-18 Add sitemap to PR #481");
        assert_eq!(d.confirm_label(), "Push to PR #481");
        assert_eq!(
            d.commits_label(),
            "New commits (2) \u{b7} already pushed commits are not shown"
        );
        assert_eq!(d.branch, otaq_core::naming::branch("SHOP-18"));
        assert_eq!(
            d.branch_hint(),
            ("Locked: set at the first push (Open PR)".to_owned(), false)
        );
        d.title.clear();
        assert!(d.can_confirm(), "no title needed for a push");
    }

    #[test]
    fn epic_and_not_logged_in() {
        let mut epic = ticket("SHOP-40", "Wishlist", Status::WaitingForYou);
        epic.kind = Kind::Epic;
        let mut d = PrDialog::opening(&epic, "open-pr", Some(&github(true)));
        d.target = "main".into();
        assert_eq!(d.heading(), "Open PR for epic SHOP-40 Wishlist");
        d.commits = vec![CommitField {
            sha: "a".into(),
            message: "m".into(),
        }];
        assert_eq!(d.commits_label(), "Commit messages (one per merged child)");
        assert_eq!(
            d.target_line(),
            "Into main \u{b7} the epic\u{2019}s base branch"
        );

        let d = PrDialog::opening(&shop18(), "open-pr", Some(&github(false)));
        assert_eq!(
            d.stage,
            Stage::NotLoggedIn {
                cli: "gh".into(),
                login: "gh auth login".into()
            }
        );
        // An imported skill (shown by its name) handles logins itself (REV-11, WSK-08).
        let mut custom = github(false);
        custom.open_pr_skill = "my-pr".into();
        let d = PrDialog::opening(&shop18(), "my-pr", Some(&custom));
        assert_eq!(d.stage, Stage::Preparing);
        assert_eq!(success_toast(FlowKind::Open, 481), "PR opened #481");
    }
}

/// The Mark merged toast: "SHOP-26 merged · 2 waiting tickets started" (REV-13, SCH-05).
pub fn merged_toast(key: &str, started: usize) -> String {
    match started {
        0 => format!("{key} merged"),
        1 => format!("{key} merged \u{b7} 1 waiting ticket started"),
        n => format!("{key} merged \u{b7} {n} waiting tickets started"),
    }
}

/// Mark closed asks first: "Mark PR #481 as closed?" and what it does.
pub fn mark_closed_confirm(number: u32) -> (String, &'static str) {
    (
        format!("Mark PR #{number} as closed?"),
        "The ticket moves to Done as \u{201c}Closed \u{b7} not merged\u{201d}. The branch stays; nothing is pushed or \
         deleted. You can reopen it later.",
    )
}

/// The toast after Mark closed.
pub fn marked_closed_toast(key: &str) -> (String, &'static str) {
    (
        format!("{key} closed \u{b7} not merged"),
        "The branch stays; you can reopen it later.",
    )
}

/// The sheet's note under an open PR (*05 → SHOP-26 · Approved, PR open*): Otaq doesn't watch PRs.
pub fn pr_open_note(provider: Option<&str>, target: &str) -> String {
    let on = provider.map(|p| format!(" on {p}")).unwrap_or_default();
    format!(
        "{} doesn\u{2019}t watch this PR. When it is merged{on}, click Mark merged: the ticket moves to Done, {} \
         fetches {target}, and tickets that were waiting for it start. If it was closed without merging, Mark closed \
         moves the ticket to Done as \u{201c}Closed \u{b7} not merged\u{201d}.",
        otaq_core::naming::PRODUCT_NAME,
        otaq_core::naming::PRODUCT_NAME
    )
}

/// "acme/shop-web #481" from the PR's URL.
pub fn pr_label(url: &str, number: u32) -> String {
    let path = url
        .split_once("://")
        .map_or(url, |(_, rest)| rest)
        .split('/')
        .skip(1)
        .take(2)
        .collect::<Vec<_>>()
        .join("/");
    if path.is_empty() {
        format!("PR #{number}")
    } else {
        format!("{path} #{number}")
    }
}

/// The Send back dialog's note on a ticket with a PR (*07 → Send back · ticket with an Azure DevOps PR*): its
/// unresolved PR comments reach the agent through `review feedback`, fetched when it reads them (d0017).
pub fn send_back_pr_note(number: u32, provider: Option<&str>) -> (String, String) {
    let on = provider.map(|p| format!(" on {p}")).unwrap_or_default();
    (
        format!("PR #{number}{on}"),
        format!(
            "Its unresolved comments are fetched when the agent reads its feedback (`{} review feedback`) and added \
             below your line comments. The PR stays open; after the fix, Approve offers Push to PR #{number}.",
            otaq_core::naming::COMMAND_NAME
        ),
    )
}

#[cfg(test)]
mod pr_open_tests {
    use super::*;

    #[test]
    fn texts() {
        assert_eq!(
            merged_toast("SHOP-26", 2),
            "SHOP-26 merged \u{b7} 2 waiting tickets started"
        );
        assert_eq!(merged_toast("SHOP-26", 0), "SHOP-26 merged");
        assert_eq!(mark_closed_confirm(481).0, "Mark PR #481 as closed?");
        assert!(
            mark_closed_confirm(481)
                .1
                .contains("nothing is pushed or deleted")
        );
        assert_eq!(
            marked_closed_toast("SHOP-26").0,
            "SHOP-26 closed \u{b7} not merged"
        );
        assert_eq!(
            pr_label("https://github.com/acme/shop-web/pull/481", 481),
            "acme/shop-web #481"
        );
        assert!(
            pr_open_note(Some("GitHub"), "main")
                .contains("When it is merged on GitHub, click Mark merged")
        );
        let (title, text) = send_back_pr_note(212, Some("Azure DevOps"));
        assert_eq!(title, "PR #212 on Azure DevOps");
        assert!(text.contains("review feedback"));
    }
}
