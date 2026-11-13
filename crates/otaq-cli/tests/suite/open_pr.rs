//! The Open PR flow (T-0143, REV-10, d0050) against a temporary repository whose `origin` is a local bare
//! repository: the dialog opens at once on the message the delivery's agent wrote in Review (`pr message`), Otaq
//! applies the confirmed edits itself (reword with unchanged trees, rename, lock), and only then types the Open PR
//! skill into the ticket's live session (an ordinary `sh`, no `claude`); the skill's steps are fed through the CLI
//! (`pr propose` for a Push, `pr opened`).

use otaq_core::service::testing::said;
use otaq_core::skills::testing::claude_skill;
use std::path::{Path, PathBuf};

use otaq_core::board::projects::AddProject;
use otaq_core::board::settings::WorkflowStep;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::pr::flow::{BranchCheck, FlowState};
use otaq_core::pr::proposal::{CommitMessage, Proposal};
use otaq_core::pr::service::PrOutcome;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::skills::testing::skill_folder;
use serde_json::{Value, json};

const CB: &str = env!("CARGO_BIN_EXE_otaq");

struct Setup {
    w: TestService,
    ticket: Ticket,
    agent: AgentShell,
    repo: PathBuf,
    worktree: PathBuf,
    origin: PathBuf,
}

fn me() -> Actor {
    Actor::User("me".into())
}

/// SHOP-1 in Ready for Review with two commits, its agent's session live, and `origin` a local bare repository.
async fn setup() -> Setup {
    let w = TestService::start().await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    let origin = w.repos.root().join("shop-web-origin.git");
    w.board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    // origin is a local folder, not GitHub or Azure DevOps: the project uses its own Open PR skill.
    let folder = skill_folder(w.data.root(), "my-pr");
    w.use_step_skill(WorkflowStep::OpenPr, &folder).await;
    let ticket = w.create("SHOP", "Login redirect").await;
    let worktree = w.start_worktree(&ticket).await;
    commit(&worktree, "login.ts", "redirect\n");
    commit(&worktree, "login.test.ts", "test\n");
    w.force_status(&ticket, Status::Working).await;
    w.board
        .submit_for_review(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Redirects after login.")
                .await
                .as_ref(),
            &me(),
        )
        .await
        .unwrap();
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    let agent = w.agent_shell(&ticket).await;
    Setup {
        w,
        ticket,
        agent,
        repo,
        worktree,
        origin,
    }
}

fn commits(repo: &Path, range: &str) -> Vec<(String, String)> {
    git(repo, &["log", "--reverse", "--format=%H %s", range])
        .lines()
        .map(|l| {
            let (sha, subject) = l.split_once(' ').unwrap();
            (sha.to_owned(), subject.to_owned())
        })
        .collect()
}

fn tree(repo: &Path, commit: &str) -> String {
    git(repo, &["rev-parse", &format!("{commit}^{{tree}}")])
        .trim()
        .to_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn open_pr_proposes_confirms_rewords_renames_and_reports() {
    let s = setup().await;
    let w = &s.w;
    let before = commits(&s.repo, &format!("main..{}", naming::branch("SHOP-1")));
    assert_eq!(before.len(), 2);

    // Before Open PR there is no flow: a proposal is refused.
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr propose --stdin"), "{}")
        .await;
    assert_ne!(ran.status, 0);

    // The delivery's agent wrote the message while the ticket waited in Review.
    let message = json!({
        "title": "SHOP-1 Login redirect",
        "description": "Returns to the page you came from.",
        "branch": "feature/SHOP-1-login"
    });
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr message --stdin"), &message.to_string())
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert!(
        ran.stdout.contains("SHOP-1's message is with the user"),
        "{ran:?}"
    );

    // The click opens the dialog at once on it: nothing is typed into the session, the ticket stays in Review.
    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    let view = w.board.pr_flow_view("SHOP-1").await.unwrap();
    let flow = view.flow.clone().unwrap();
    assert_eq!(flow.state, FlowState::Proposed);
    let proposed = flow.proposal.as_ref().unwrap();
    assert_eq!(proposed.branch, "feature/SHOP-1-login");
    assert_eq!(proposed.title, "SHOP-1 Login redirect");
    assert_eq!(proposed.description, "Returns to the page you came from.");
    assert_eq!(
        proposed
            .commits
            .iter()
            .map(|c| (c.sha.clone(), c.message.clone()))
            .collect::<Vec<_>>(),
        before,
        "the commits as they are"
    );
    assert!(view.message.as_ref().is_some_and(|m| m.is_ready()));
    assert_eq!(view.target, "main");
    assert_eq!(view.commits.len(), 2);
    assert!(!s.worktree.join(".agents").exists());

    // Live branch-name validation: valid ref, not taken locally, not taken on origin.
    git(
        &s.repo,
        &["push", "--quiet", "origin", "main:refs/heads/taken"],
    );
    for (name, ok) in [
        ("feature/SHOP-1-login", true),
        ("main", false),
        ("taken", false),
        ("a..b", false),
    ] {
        let check = w.board.check_branch_name("SHOP-1", name).await.unwrap();
        assert_eq!(check == BranchCheck::Ok, ok, "{name}: {check:?}");
    }
    // The hints end with the fix (d0093).
    for (name, says) in [
        ("feature/a b", "Use hyphens instead of spaces"),
        (
            "taken",
            "A branch with this name already exists on origin. Choose another name.",
        ),
    ] {
        let check = w.board.check_branch_name("SHOP-1", name).await.unwrap();
        assert_eq!(
            check,
            BranchCheck::Problem {
                message: says.to_owned()
            }
        );
    }

    // A taken name is refused on confirm, and nothing changes.
    let mut edits: Proposal = flow.proposal.unwrap();
    edits.branch = "taken".into();
    assert!(w.prs.confirm("SHOP-1", &edits, false, &me()).await.is_err());
    assert_eq!(
        commits(&s.repo, &format!("main..{}", naming::branch("SHOP-1"))),
        before
    );

    // Confirm with an edited message: Otaq rewords and renames itself; nothing is pushed.
    edits.branch = "feature/SHOP-1-login".into();
    edits.commits[0].message = "SHOP-1: redirect after login".into();
    edits.commits[1] = CommitMessage {
        sha: before[1].0.clone(),
        message: "SHOP-1: cover deep links".into(),
    };
    edits.title = "SHOP-1 Return after login".into();
    let outcome = w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let after = commits(&s.repo, "main..feature/SHOP-1-login");
    assert_eq!(
        after.iter().map(|c| c.1.as_str()).collect::<Vec<_>>(),
        ["SHOP-1: redirect after login", "SHOP-1: cover deep links"]
    );
    for (old, new) in before.iter().zip(&after) {
        assert_ne!(old.0, new.0, "reworded");
        assert_eq!(tree(&s.repo, &old.0), tree(&s.repo, &new.0), "same files");
    }
    assert!(
        git(&s.repo, &["branch", "--list", &naming::branch("SHOP-1")])
            .trim()
            .is_empty(),
        "renamed"
    );
    assert_eq!(
        git(&s.worktree, &["branch", "--show-current"]).trim(),
        "feature/SHOP-1-login",
        "the worktree follows the rename"
    );
    assert!(
        git(&s.origin, &["branch", "--list", "feature/*"])
            .trim()
            .is_empty(),
        "nothing pushed"
    );
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert!(ticket.work.as_ref().unwrap().branch_locked);
    assert_eq!(
        w.board.approve_check("SHOP-1").await.unwrap(),
        otaq_core::board::review::ApproveCheck::Unchanged,
        "rewording isn't a change to review"
    );
    // The Delivery's `skill: my-pr` starts now, typed by its own name and told the user confirmed (T2).
    w.wait_for_output(&s.agent.session, &claude_skill("my-pr"))
        .await;
    w.wait_for_output(&s.agent.session, "The user confirmed the pull request")
        .await;
    // prime shows what was confirmed.
    let ran = s.agent.run(&format!("'{CB}' prime")).await;
    assert!(ran.stdout.contains("## Pull request"), "{}", ran.stdout);
    assert!(ran.stdout.contains("- Title: SHOP-1 Return after login"));
    assert!(ran.stdout.contains("**The user confirmed.**"));

    // The skill pushes (here: by hand) and reports the PR.
    git(
        &s.repo,
        &["push", "--quiet", "origin", "feature/SHOP-1-login"],
    );
    let ran = s
        .agent
        .run(&format!(
            "'{CB}' pr opened https://github.com/acme/shop-web/pull/7 --json"
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let ticket: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(ticket["status"], "pr_open");
    assert_eq!(ticket["flags"]["pr_number"], 7);
    assert!(w.board.pr_flow(&s.ticket.id).await.unwrap().is_none());
    // Reworded commits carry the reviewed content: no warning (T-0145).
    assert!(ticket.get("pushed_differs").is_none(), "{ticket}");
    assert!(ticket["flags"].get("changed_since_review").is_none());
    s.w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_and_refusals_leave_the_ticket_in_review() {
    let s = setup().await;
    let w = &s.w;

    // With the built-in Open PR skill, a remote that isn't GitHub or Azure DevOps refuses (REV-11).
    w.drop_step_skill(WorkflowStep::OpenPr).await;
    let refused = w.prs.open("SHOP-1", false, false, &me()).await.unwrap_err();
    assert!(
        refused.to_string().contains("PR provider not detected"),
        "{refused}"
    );
    let folder = skill_folder(w.data.root(), "my-pr");
    w.use_step_skill(WorkflowStep::OpenPr, &folder).await;

    // Uncommitted files refuse first (REV-08).
    std::fs::write(s.worktree.join("scratch.txt"), "x").unwrap();
    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    let PrOutcome::RefusedUncommitted { files, message } = outcome else {
        panic!("{outcome:?}")
    };
    assert_eq!(files, ["scratch.txt"]);
    assert_eq!(message, "SHOP-1 has 1 uncommitted file");
    std::fs::remove_file(s.worktree.join("scratch.txt")).unwrap();

    // A new commit since the review asks first (REV-06).
    commit(&s.worktree, "late.ts", "late\n");
    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Changed { .. }), "{outcome:?}");

    // Approve anyway starts it; Cancel discards it.
    let outcome = w.prs.open("SHOP-1", true, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let ticket = w.prs.cancel("SHOP-1", &me()).await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou);
    assert!(w.board.pr_flow(&s.ticket.id).await.unwrap().is_none());
    let history = w.board.history("SHOP-1").await.unwrap();
    assert!(history.iter().any(|e| e.kind.as_str() == "pr_cancelled"));
    assert!(
        git(&s.origin, &["branch", "--list"])
            .lines()
            .all(|b| b.trim() == "main" || b.trim() == "* main"),
        "nothing pushed"
    );

    // Unsent text in the session asks before the confirm types the skill (d0048); nothing was changed.
    w.prs.open("SHOP-1", true, false, &me()).await.unwrap();
    let edits = w
        .board
        .pr_flow(&s.ticket.id)
        .await
        .unwrap()
        .unwrap()
        .proposal
        .unwrap();
    // With no message from the delivery's agent, the dialog opens on the plain one.
    assert_eq!(edits.title, "SHOP-1 Login redirect");
    assert_eq!(edits.description, "Redirects after login.");
    w.sessions
        .input(&s.agent.session, b"half a thought")
        .unwrap();
    let outcome = w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
    assert!(
        matches!(outcome, PrOutcome::NeedsConfirmation { .. }),
        "{outcome:?}"
    );
    assert_eq!(
        w.board.pr_flow(&s.ticket.id).await.unwrap().unwrap().state,
        FlowState::Proposed
    );
    s.w.stop().await;
}

/// T-0144 (REV-14, d0041): PR rework and Push to PR. Only the commits not on the PR's branch are proposed and
/// reworded; the pushed ones keep their IDs; the branch stays locked; the ticket returns to PR open, in Review.
#[tokio::test(flavor = "multi_thread")]
async fn rework_then_push_to_pr_rewords_only_the_new_commits() {
    let s = setup().await;
    let w = &s.w;
    // Open the PR (the skill's steps through the CLI, the push by hand).
    w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    let first = commits(&s.repo, &format!("main..{}", naming::branch("SHOP-1")));
    let proposal = json!({
        "branch": "feature/SHOP-1-login",
        "commits": [{"sha": &first[0].0, "message": "SHOP-1: one"}, {"sha": &first[1].0, "message": "SHOP-1: two"}],
        "title": "SHOP-1 Login redirect", "description": "d"
    });
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr propose --stdin"), &proposal.to_string())
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let edits: Proposal = serde_json::from_value(proposal).unwrap();
    w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
    git(
        &s.repo,
        &["push", "--quiet", "origin", "feature/SHOP-1-login"],
    );
    w.prs
        .opened("SHOP-1", "https://github.com/acme/shop-web/pull/7", &me())
        .await
        .unwrap();
    let pushed = commits(&s.repo, "main..feature/SHOP-1-login");

    // Back to the agent with the PR still open, in Review (the delivery is no stage): it adds two commits and
    // resubmits.
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.place, otaq_core::domain::Place::stage_of("review"));
    assert_eq!(ticket.flags.pr_number, Some(7));
    w.force_stage(&ticket, "dev", Status::Working).await;
    commit(&s.worktree, "deep.ts", "deep links\n");
    commit(&s.worktree, "deep.test.ts", "test\n");
    w.board
        .submit_for_review(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Covers deep links.")
                .await
                .as_ref(),
            &me(),
        )
        .await
        .unwrap();
    assert!(
        !w.board.ticket("SHOP-1").await.unwrap().flags.pr_rework,
        "resubmitted: ready to push"
    );

    // Approve offers Push to PR: the Push skill runs; only the two new commits are editable.
    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    w.wait_for_output(&s.agent.session, &claude_skill("push"))
        .await;
    let view = w.board.pr_flow_view("SHOP-1").await.unwrap();
    assert_eq!(
        view.flow.as_ref().unwrap().kind,
        otaq_core::pr::flow::FlowKind::Push
    );
    assert!(view.branch_locked);
    assert_eq!(view.commits.len(), 4);
    assert_eq!(
        view.commits.iter().map(|c| c.pushed).collect::<Vec<_>>(),
        [true, true, false, false]
    );
    let new: Vec<String> = view.commits[2..].iter().map(|c| c.sha.clone()).collect();

    // A proposal that renames the locked branch, or names a pushed commit, is refused.
    let renamed = json!({"branch": "feature/other", "commits": [{"sha": &new[0], "message": "a"}, {"sha": &new[1], "message": "b"}]});
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr propose --stdin"), &renamed.to_string())
        .await;
    assert_ne!(ran.status, 0);
    let pushed_one = json!({"branch": "feature/SHOP-1-login", "commits": [{"sha": &pushed[0].0, "message": "a"}, {"sha": &new[1], "message": "b"}]});
    let ran = s
        .agent
        .run_with_stdin(
            &format!("'{CB}' pr propose --stdin"),
            &pushed_one.to_string(),
        )
        .await;
    assert_ne!(ran.status, 0);
    let good = json!({"branch": "feature/SHOP-1-login", "commits": [
        {"sha": &new[0], "message": "SHOP-1: deep links"}, {"sha": &new[1], "message": "SHOP-1: test deep links"}]});
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr propose --stdin"), &good.to_string())
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");

    let edits: Proposal = serde_json::from_value(good).unwrap();
    w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
    let after = commits(&s.repo, "main..feature/SHOP-1-login");
    assert_eq!(&after[..2], &pushed[..], "pushed commits keep their IDs");
    assert_eq!(
        after[2..].iter().map(|c| c.1.as_str()).collect::<Vec<_>>(),
        ["SHOP-1: deep links", "SHOP-1: test deep links"]
    );
    for (old, new) in new.iter().zip(&after[2..]) {
        assert_eq!(tree(&s.repo, old), tree(&s.repo, &new.0));
    }
    w.wait_for_output(&s.agent.session, "The user confirmed the push")
        .await;
    git(
        &s.repo,
        &["push", "--quiet", "origin", "feature/SHOP-1-login"],
    );
    let ran = s
        .agent
        .run(&format!(
            "'{CB}' pr opened https://github.com/acme/shop-web/pull/7 --json"
        ))
        .await;
    let ticket: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(ticket["status"], "pr_open");
    assert_eq!(ticket["flags"]["pr_number"], 7);
    s.w.stop().await;
}

/// T-0145 (REV-10, REV-06, d0043): after the push the remote branch's content is compared with the reviewed
/// content; an extra commit pushed with it shows "changed since you reviewed it" with only the difference.
#[tokio::test(flavor = "multi_thread")]
async fn extra_pushed_content_shows_the_changed_since_review_warning() {
    let s = setup().await;
    let w = &s.w;
    w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    let first = commits(&s.repo, &format!("main..{}", naming::branch("SHOP-1")));
    let proposal = json!({
        "branch": "feature/SHOP-1-login",
        "commits": [{"sha": &first[0].0, "message": "SHOP-1: one"}, {"sha": &first[1].0, "message": "SHOP-1: two"}],
        "title": "SHOP-1 Login redirect"
    });
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr propose --stdin"), &proposal.to_string())
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let edits: Proposal = serde_json::from_value(proposal).unwrap();
    w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
    // The skill slipped in one more commit before pushing.
    commit(&s.worktree, "extra.txt", "not reviewed\n");
    git(
        &s.repo,
        &["push", "--quiet", "origin", "feature/SHOP-1-login"],
    );
    let ticket = w
        .prs
        .opened("SHOP-1", "https://github.com/acme/shop-web/pull/8", &me())
        .await
        .unwrap();
    let diff = ticket.pushed_differs.expect("the warning");
    assert_eq!(diff.files, ["extra.txt"]);
    assert!(ticket.flags.changed_since_review);
    assert_eq!(
        diff.message("SHOP-1"),
        "SHOP-1 changed since you reviewed it \u{b7} the pushed content differs in 1 file"
    );
    s.w.stop().await;
}

/// Opens SHOP-1's PR flow and confirms its two commits under `feature/SHOP-1-login`: the delivery's agent would push
/// now.
async fn confirmed(s: &Setup) {
    let w = &s.w;
    w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    let first = commits(&s.repo, &format!("main..{}", naming::branch("SHOP-1")));
    let proposal = json!({
        "branch": "feature/SHOP-1-login",
        "commits": [{"sha": &first[0].0, "message": "SHOP-1: one"}, {"sha": &first[1].0, "message": "SHOP-1: two"}],
        "title": "SHOP-1 Login redirect"
    });
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr propose --stdin"), &proposal.to_string())
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let edits: Proposal = serde_json::from_value(proposal).unwrap();
    w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
}

/// The delivery agent's report isn't taken alone: `pr opened` with nothing pushed leaves the ticket in Review, in Needs
/// you with "The branch isn't on the remote as expected" and what the remote has. The reworded commits stay on the
/// local branch, and Retry push is Push to PR from Review, which only pushes.
#[tokio::test(flavor = "multi_thread")]
async fn a_reported_push_the_remote_does_not_have_needs_you() {
    use otaq_core::domain::AutoMergeStop;
    let s = setup().await;
    let w = &s.w;
    confirmed(&s).await;
    let reworded = commits(&s.repo, "main..feature/SHOP-1-login");
    // The agent says it is done without having pushed.
    let ran = s
        .agent
        .run(&format!(
            "'{CB}' pr opened https://github.com/acme/shop-web/pull/7 --json"
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    // Still in Review, where the user approved it: a push that didn't land isn't the agent's work going wrong.
    assert_eq!(ticket.status, Status::WaitingForYou);
    assert!(ticket.place.is_in(otaq_core::flow::default::REVIEW));
    assert_eq!(
        ticket.stopped_in_place(),
        Some(AutoMergeStop::PushNotLanded)
    );
    assert!(ticket.shows_needs_you());
    assert_eq!(ticket.flags.pr_number, Some(7));
    let said = "The branch isn\u{2019}t on the remote as expected \u{b7} the remote has no branch \
                `feature/SHOP-1-login`";
    assert_eq!(
        ticket.signal.as_ref().and_then(|s| s.text.as_deref()),
        Some(said)
    );
    let items = w
        .board
        .needs_you_items(otaq_core::store::now_ms())
        .await
        .unwrap();
    let item = items
        .iter()
        .find(|i| i.key == ticket.key)
        .expect("in Needs you");
    assert!(
        item.reason
            .contains("the branch isn\u{2019}t on the remote as expected"),
        "{}",
        item.reason
    );
    assert!(w.board.pr_flow(&s.ticket.id).await.unwrap().is_none());
    // Nothing undone: the reworded commits are still the local branch's.
    assert_eq!(commits(&s.repo, "main..feature/SHOP-1-login"), reworded);

    // Retry push: Approve from Review, which with a PR is Push to PR; confirmed, the push goes again and the stop
    // clears; the push and its report land.
    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let flow = w.board.pr_flow(&s.ticket.id).await.unwrap().unwrap();
    assert_eq!(flow.kind, otaq_core::pr::flow::FlowKind::Push);
    let again = commits(&s.repo, "main..feature/SHOP-1-login");
    let proposal = json!({
        "branch": "feature/SHOP-1-login",
        "commits": again.iter().map(|c| json!({"sha": &c.0, "message": &c.1})).collect::<Vec<_>>(),
    });
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr propose --stdin"), &proposal.to_string())
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let edits: Proposal = serde_json::from_value(proposal).unwrap();
    w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
    let pushing = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        pushing.stopped_in_place(),
        None,
        "the stop clears as it pushes"
    );
    assert_eq!(pushing.status, Status::WaitingForYou);
    git(
        &s.repo,
        &["push", "--quiet", "origin", "feature/SHOP-1-login"],
    );
    let ticket = w
        .prs
        .opened("SHOP-1", "https://github.com/acme/shop-web/pull/7", &me())
        .await
        .unwrap();
    assert_eq!(ticket.status, Status::PrOpen);
    assert_eq!(ticket.auto_merge_stop, None);
    s.w.stop().await;
}

/// A push that left the remote behind the local branch (a commit made after it) is a mismatch too, naming both
/// commits.
#[tokio::test(flavor = "multi_thread")]
async fn a_remote_branch_behind_the_local_one_needs_you() {
    let s = setup().await;
    let w = &s.w;
    confirmed(&s).await;
    git(
        &s.repo,
        &["push", "--quiet", "origin", "feature/SHOP-1-login"],
    );
    let pushed = git(&s.repo, &["rev-parse", "feature/SHOP-1-login"]);
    commit(&s.worktree, "late.txt", "after the push\n");
    let local = git(&s.repo, &["rev-parse", "feature/SHOP-1-login"]);
    let ticket = w
        .prs
        .opened("SHOP-1", "https://github.com/acme/shop-web/pull/7", &me())
        .await
        .unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou);
    assert_eq!(
        ticket.stopped_in_place(),
        Some(otaq_core::domain::AutoMergeStop::PushNotLanded)
    );
    assert_eq!(
        ticket.signal.as_ref().and_then(|s| s.text.clone()),
        Some(format!(
            "The branch isn\u{2019}t on the remote as expected \u{b7} the remote has `feature/SHOP-1-login` at {}, \
             not {}",
            &pushed.trim()[..7],
            &local.trim()[..7]
        ))
    );
    s.w.stop().await;
}

/// T-0149: a skill that can't push reports it with `ask human`; during the flow that goes to the dialog (the
/// ticket stays in Ready for Review), and Retry asks the skill again.
#[tokio::test(flavor = "multi_thread")]
async fn a_reported_problem_goes_to_the_dialog_and_retry_asks_again() {
    let s = setup().await;
    let w = &s.w;
    w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    let first = commits(&s.repo, &format!("main..{}", naming::branch("SHOP-1")));
    let proposal = json!({
        "branch": "feature/SHOP-1-login",
        "commits": [{"sha": &first[0].0, "message": "one"}, {"sha": &first[1].0, "message": "two"}],
        "title": "SHOP-1 Login redirect"
    });
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr propose --stdin"), &proposal.to_string())
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let edits: Proposal = serde_json::from_value(proposal).unwrap();
    w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
    w.wait_for_output(&s.agent.session, "The user confirmed the pull request")
        .await;

    let ran = s
        .agent
        .run_with_stdin(
            &format!("'{CB}' ask human --stdin"),
            "The push was rejected: origin has commits you don't have.",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    let flow = w.board.pr_flow(&s.ticket.id).await.unwrap().unwrap();
    assert_eq!(
        flow.reported.as_deref(),
        Some("The push was rejected: origin has commits you don't have.")
    );
    assert_eq!(flow.state, FlowState::Pushing);

    let outcome = w.prs.retry("SHOP-1", false).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let flow = w.board.pr_flow(&s.ticket.id).await.unwrap().unwrap();
    assert_eq!(flow.reported, None);
    // The confirmation is typed again.
    let mut typed = 0;
    for _ in 0..100 {
        let out = w
            .wait_for_output(&s.agent.session, "The user confirmed the pull request")
            .await;
        typed = out.matches("The user confirmed the pull request").count();
        if typed >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(typed >= 2, "typed {typed} times");
    s.w.stop().await;
}

/// A project's `feat:`/`fix:` commit-msg hook checks every confirmed message before Otaq rewords: "update stuff" is
/// refused, nothing is rewritten, renamed or pushed, the ticket shows the hook's output and the message, and the
/// user's edits stay; fixed to `fix: …`, the PR goes on with the message as the hook left it (its `Change-Id`).
#[tokio::test(flavor = "multi_thread")]
async fn a_refused_pr_message_changes_nothing_and_shows_on_the_ticket() {
    use otaq_core::board::tickets::EventKind;
    use otaq_core::domain::AutoMergeStop;
    use otaq_core::git::GitStep;
    use otaq_core::pr::message::MessageEdit;
    let s = setup().await;
    let w = &s.w;
    let hook = s.repo.join(".git/hooks/commit-msg");
    std::fs::write(
        &hook,
        "#!/bin/sh\n\
         grep -qE '^(feat|fix):' \"$1\" || { echo 'commitlint: type must be one of [feat, fix]' >&2; exit 1; }\n\
         grep -q '^Change-Id:' \"$1\" || printf '\\nChange-Id: I0123\\n' >> \"$1\"\n",
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let branch = naming::branch("SHOP-1");
    let before = commits(&s.repo, &format!("main..{branch}"));

    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let mut edits = w
        .board
        .pr_flow(&s.ticket.id)
        .await
        .unwrap()
        .unwrap()
        .proposal
        .unwrap();
    edits.branch = "feature/SHOP-1-login".into();
    edits.commits[0].message = "update stuff".into();
    edits.commits[1].message = "fix: cover deep links".into();
    // The dialog keeps the user's edits as they type.
    let kept = w
        .board
        .keep_edited_message(
            "SHOP-1",
            MessageEdit::Pr {
                title: None,
                description: None,
                commits: edits
                    .commits
                    .iter()
                    .map(|c| (c.sha.clone(), c.message.clone()))
                    .collect(),
            },
        )
        .await
        .unwrap();

    let outcome = w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
    let PrOutcome::HookRefused { hook, message } = outcome else {
        panic!("refused: {outcome:?}");
    };
    assert_eq!(hook, "commit-msg");
    assert_eq!(message, "commit-msg hook refused the PR");
    // Nothing rewritten, renamed or pushed.
    assert_eq!(commits(&s.repo, &format!("main..{branch}")), before);
    assert!(
        git(&s.origin, &["branch", "--list", "feature/*"])
            .trim()
            .is_empty()
    );
    assert!(
        git(&s.worktree, &["status", "--porcelain"])
            .trim()
            .is_empty(),
        "the worktree is clean"
    );
    // On the ticket, still in Review: the hook, its output and the message; History keeps it.
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou);
    assert!(ticket.place.is_in(otaq_core::flow::default::REVIEW));
    assert_eq!(ticket.stopped_in_place(), Some(AutoMergeStop::HookRefused));
    assert!(!ticket.work.as_ref().unwrap().branch_locked);
    let refused = ticket
        .flags
        .hook_refusal
        .clone()
        .expect("kept on the ticket");
    assert_eq!(refused.name, "commit-msg");
    assert_eq!(refused.step, Some(GitStep::Reword));
    assert_eq!(refused.message.as_deref(), Some("update stuff"));
    assert!(
        refused
            .output
            .contains("commitlint: type must be one of [feat, fix]"),
        "{}",
        refused.output
    );
    let history = w.board.history("SHOP-1").await.unwrap();
    assert!(history.iter().any(|e| e.kind == EventKind::HookRefused));
    assert!(w.board.pr_flow(&s.ticket.id).await.unwrap().is_none());
    // The user's edits stay to fix it.
    assert_eq!(w.board.edited_message(&s.ticket.id).await.unwrap(), kept);

    // Edit messages and retry: the dialog opens again from Review; fixed, the PR goes on.
    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    // What the dialog fills its commit fields from: the same commits, the refused one's kept text over it.
    let reopened = w.board.pr_flow_view("SHOP-1").await.unwrap();
    let again = reopened.flow.unwrap().proposal.unwrap();
    assert_eq!(
        again.commits.iter().map(|c| &c.sha).collect::<Vec<_>>(),
        edits.commits.iter().map(|c| &c.sha).collect::<Vec<_>>()
    );
    assert_eq!(
        reopened
            .edited
            .commits
            .get(&edits.commits[0].sha)
            .map(String::as_str),
        Some("update stuff")
    );
    edits.commits[0].message = "fix: redirect after login".into();
    let outcome = w.prs.confirm("SHOP-1", &edits, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let after = commits(&s.repo, "main..feature/SHOP-1-login");
    assert_eq!(
        after.iter().map(|c| c.1.as_str()).collect::<Vec<_>>(),
        ["fix: redirect after login", "fix: cover deep links"]
    );
    assert_eq!(
        git(&s.repo, &["log", "-1", "--format=%B", &after[0].0]).trim(),
        "fix: redirect after login\n\nChange-Id: I0123",
        "as the hook left it"
    );
    let ticket = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(ticket.status, Status::WaitingForYou);
    assert_eq!(ticket.auto_merge_stop, None);
    assert_eq!(ticket.flags.hook_refusal, None);
    w.wait_for_output(&s.agent.session, "The user confirmed the pull request")
        .await;
    s.w.stop().await;
}
