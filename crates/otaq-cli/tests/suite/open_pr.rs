//! The Open PR flow (T-0143, REV-10, d0050) against a temporary repository whose `origin` is a local bare
//! repository: the click types the Open PR skill into the ticket's live session (an ordinary `sh`, no `claude`),
//! the skill's steps are fed through the CLI (`pr propose`, `pr opened`), and Otaq applies the confirmed
//! edits itself (reword with unchanged trees, rename, lock) before anything is pushed.

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

    // The click types the Open PR skill into the live session; the ticket stays in Ready for Review.
    let outcome = w.prs.open("SHOP-1", false, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    // The Delivery stage's `skill: my-pr`, typed by its own name; nothing is written into the worktree (T2).
    w.wait_for_output(&s.agent.session, &claude_skill("my-pr"))
        .await;
    assert!(!s.worktree.join(".agents").exists());
    assert_eq!(w.status("SHOP-1").await, Status::WaitingForYou);
    let flow = w.board.pr_flow(&s.ticket.id).await.unwrap().unwrap();
    assert_eq!(flow.state, FlowState::Preparing);

    // The skill proposes through the CLI; a proposal naming the wrong commits is refused.
    let wrong = json!({"branch": "feature/SHOP-1-login", "commits": [{"sha": "abcdef1", "message": "x"}],
                       "title": "SHOP-1 Login redirect"});
    let ran = s
        .agent
        .run_with_stdin(
            &format!("'{CB}' pr propose --stdin --json"),
            &wrong.to_string(),
        )
        .await;
    assert_ne!(ran.status, 0, "{ran:?}");
    let proposal = json!({
        "branch": "feature/SHOP-1-login",
        "commits": [
            {"sha": &before[0].0[..7], "message": "SHOP-1: redirect after login"},
            {"sha": &before[1].0[..7], "message": "SHOP-1: test the redirect"}
        ],
        "title": "SHOP-1 Login redirect",
        "description": "Returns to the page you came from."
    });
    let ran = s
        .agent
        .run_with_stdin(&format!("'{CB}' pr propose --stdin"), &proposal.to_string())
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let view = w.board.pr_flow_view("SHOP-1").await.unwrap();
    let flow = view.flow.clone().unwrap();
    assert_eq!(flow.state, FlowState::Proposed);
    assert_eq!(flow.proposal.as_ref().unwrap().commits[0].sha, before[0].0);
    assert_eq!(view.target, "main");
    assert_eq!(view.commits.len(), 2);

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

    // Unsent text in the session asks before typing (d0048).
    w.sessions
        .input(&s.agent.session, b"half a thought")
        .unwrap();
    let outcome = w.prs.open("SHOP-1", true, false, &me()).await.unwrap();
    assert!(
        matches!(outcome, PrOutcome::NeedsConfirmation { .. }),
        "{outcome:?}"
    );
    assert!(w.board.pr_flow(&s.ticket.id).await.unwrap().is_none());
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
