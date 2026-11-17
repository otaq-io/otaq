//! A check judges only what changed since the user last approved (tuq decision 116; F-E1-3 of the 0.3 regression
//! test): the user's Pass at any human check records the commit the work stands at, and `prime --validate` gives a
//! later check the diff since then, with the approved changes named as context. With no approval (or one that no
//! longer holds) the check judges the whole branch, as before. On a Spec first-shaped test flow (Product spec, Spec
//! review by you, Dev, Validate, QA by you); `claude` is stood in for by ordinary commands, and the project is paused
//! so no agent starts on its own.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::pause::PauseScope;
use otaq_core::board::moves::DropOn;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::{CheckBy, Delivers, DeliveryMode, FlowFile, Stage, StageKind, yaml};
use otaq_core::git::testing::{commit, git};
use otaq_core::prime::{ValidatePrime, render_validate};
use otaq_core::review::PassOutcome;
use otaq_core::review::drag::DragAnswers;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

async fn world() -> TestService {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), KEEPS_RUNNING.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w
}

fn every_kind() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn human_check(id: &str, name: &str) -> Stage {
    Stage {
        id: id.into(),
        name: name.into(),
        kind: StageKind::Check { by: CheckBy::You },
        who: Default::default(),
        skill: None,
        options: Vec::new(),
        rounds: None,
        fail_to: None,
        description: None,
    }
}

/// Product spec → Spec review (by you) → Dev → Validate → QA (by you) → a click PR delivery.
fn spec_first() -> FlowFile {
    let mut file = every_kind();
    file.flow
        .stages
        .remove(file.flow.position("security").unwrap());
    file.flow
        .stages
        .insert(1, human_check("spec-review", "Spec review"));
    file.flow.delivery.mode = DeliveryMode::Click;
    file.flow.delivery.delivers = Delivers::Pr;
    file
}

/// The Spec first-shaped world, paused, with a started ticket whose spec stage committed a README change.
async fn spec_written(w: &TestService) -> (Ticket, PathBuf) {
    w.board.use_flow_for_tests("SHOP", &spec_first()).await;
    w.pause
        .pause(&PauseScope::Project {
            project: "SHOP".into(),
        })
        .await
        .unwrap();
    let t = w.create("SHOP", "Show the version").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "README.md", "# shop\n\n## Product decisions (tuq)\n");
    (t, wt)
}

/// Puts the ticket in `stage` with `status` in one write, so no other stage gets a run on the way.
async fn place_in(w: &TestService, t: &Ticket, stage: &str, status: Status) {
    sqlx::query("UPDATE tickets SET stage_id = ?, state = ?, state_since = ? WHERE id = ?")
        .bind(stage)
        .bind(status.as_str())
        .bind(otaq_core::store::now_ms())
        .bind(&t.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
}

/// The user's Pass at Spec review: the card moves on to wait before Dev.
async fn approve_at_spec_review(w: &TestService, t: &Ticket) {
    place_in(w, t, "spec-review", Status::WaitingForYou).await;
    match w
        .review
        .pass(&t.key.to_string(), &w.validation, &me())
        .await
        .unwrap()
    {
        PassOutcome::Moved { ticket } => {
            assert_eq!(ticket.place, Place::Waiting(Some("dev".into())))
        }
        other => panic!("expected a move, got {other:?}"),
    }
}

async fn approved_sha(w: &TestService, t: &Ticket) -> Option<String> {
    sqlx::query_scalar("SELECT approved_sha FROM tickets WHERE id = ?")
        .bind(&t.id)
        .fetch_one(w.board.store().pool())
        .await
        .unwrap()
}

fn head(wt: &Path) -> String {
    git(wt, &["rev-parse", "HEAD"])
}

/// Dev's work after the approval, then the ticket in Validate: what its checker reads.
async fn checked(w: &TestService, t: &Ticket) -> ValidatePrime {
    place_in(w, t, "validate", Status::Checking).await;
    w.board.validate_prime(&t.key.to_string()).await.unwrap()
}

/// `git diff <base> HEAD`: the whole branch, today's scope.
async fn whole_branch(w: &TestService, t: &Ticket, wt: &Path) -> String {
    let ticket = w.board.ticket(&t.key.to_string()).await.unwrap();
    let base = w.board.current_base_sha(&ticket).await.unwrap().unwrap();
    // Not trimmed: the prime's diff is git's output as it is.
    let out = std::process::Command::new("git")
        .args(["diff", &base, "HEAD"])
        .current_dir(wt)
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap()
}

/// F-E1-3: the README change approved at Spec review isn't in Validate's scope after Dev; it is named as context,
/// before the diff, which holds Dev's change only.
#[tokio::test(flavor = "multi_thread")]
async fn a_change_approved_at_spec_review_is_context_not_scope() {
    let w = world().await;
    let (t, wt) = spec_written(&w).await;
    let approved = head(&wt);
    approve_at_spec_review(&w, &t).await;
    assert_eq!(
        approved_sha(&w, &t).await.as_deref(),
        Some(approved.as_str())
    );

    commit(&wt, "version.txt", "0.3\n");
    let ctx = checked(&w, &t).await;
    assert!(ctx.diff.contains("version.txt"), "{}", ctx.diff);
    assert!(!ctx.diff.contains("README.md"), "{}", ctx.diff);
    let earlier = ctx.approved.as_ref().expect("the approval");
    assert_eq!(earlier.stage, "Spec review");
    assert_eq!(earlier.sha, approved);
    assert_eq!(earlier.files, vec!["A\tREADME.md".to_owned()]);

    let text = render_validate(&ctx);
    let section = text.find("\n## Approved earlier\n").expect(&text);
    let diff = text.find("\n## The diff\n").unwrap();
    assert!(section < diff, "{text}");
    for part in [
        format!(
            "The user approved this ticket's work up to `{}` at Spec review",
            &approved[..12]
        ),
        "never fail the ticket for them or note them as out of scope. Judge only the diff below, the changes made \
         since.\n\n- A README.md\n"
            .to_owned(),
    ] {
        assert!(text.contains(&part), "{part}\n---\n{text}");
    }
    w.stop().await;
}

/// No approval (Dev straight into Validate): the whole branch, and the prime reads as it did before, with no
/// "Approved earlier".
#[tokio::test(flavor = "multi_thread")]
async fn with_no_approval_the_whole_branch_is_judged() {
    let w = world().await;
    let (t, wt) = spec_written(&w).await;
    commit(&wt, "version.txt", "0.3\n");
    let ctx = checked(&w, &t).await;
    assert_eq!(ctx.approved, None);
    assert_eq!(ctx.diff, whole_branch(&w, &t, &wt).await);
    assert!(ctx.diff.contains("README.md") && ctx.diff.contains("version.txt"));
    assert!(!render_validate(&ctx).contains("Approved earlier"));
    w.stop().await;
}

/// The base merged into the branch after the approval: its commits aren't the ticket's, so they stay out of the
/// scope, as the approved change does.
#[tokio::test(flavor = "multi_thread")]
async fn the_base_merged_in_after_the_approval_stays_out() {
    let w = world().await;
    let (t, wt) = spec_written(&w).await;
    approve_at_spec_review(&w, &t).await;
    let repo = PathBuf::from(w.board.project("SHOP").await.unwrap().repo_path);
    commit(&repo, "main.txt", "on main\n");
    git(&wt, &["merge", "--quiet", "--no-edit", "main"]);
    commit(&wt, "version.txt", "0.3\n");

    let ctx = checked(&w, &t).await;
    assert!(ctx.diff.contains("version.txt"), "{}", ctx.diff);
    assert!(!ctx.diff.contains("main.txt"), "{}", ctx.diff);
    assert!(!ctx.diff.contains("README.md"), "{}", ctx.diff);
    assert_eq!(
        ctx.approved.unwrap().files,
        vec!["A\tREADME.md".to_owned()],
        "the approved files, not the base's"
    );
    w.stop().await;
}

/// A base merge that wouldn't apply cleanly to the approved commit: the scope starts at the approved commit as it is
/// (the base's commits show, as they would with no approval), never the whole branch.
#[tokio::test(flavor = "multi_thread")]
async fn a_base_merge_that_conflicts_with_the_approval_starts_at_the_approved_commit() {
    let w = world().await;
    let (t, wt) = spec_written(&w).await;
    let approved = head(&wt);
    approve_at_spec_review(&w, &t).await;
    // Dev and main both write the same README: the branch merges main cleanly, the approved commit wouldn't.
    commit(&wt, "README.md", "# shop v2\n");
    let repo = PathBuf::from(w.board.project("SHOP").await.unwrap().repo_path);
    std::fs::write(repo.join("main.txt"), "on main\n").unwrap();
    git(&repo, &["add", "main.txt"]);
    commit(&repo, "README.md", "# shop v2\n");
    git(&wt, &["merge", "--quiet", "--no-edit", "main"]);

    let ctx = checked(&w, &t).await;
    let out = std::process::Command::new("git")
        .args(["diff", &approved, "HEAD"])
        .current_dir(&wt)
        .output()
        .unwrap();
    assert_eq!(ctx.diff, String::from_utf8(out.stdout).unwrap());
    assert!(ctx.diff.contains("main.txt"), "{}", ctx.diff);
    assert!(ctx.approved.is_some());
    w.stop().await;
}

/// The approved commit is no longer on the branch (reset by hand): the whole branch again.
#[tokio::test(flavor = "multi_thread")]
async fn an_approved_commit_gone_from_the_branch_judges_the_whole_branch() {
    let w = world().await;
    let (t, wt) = spec_written(&w).await;
    approve_at_spec_review(&w, &t).await;
    git(&wt, &["reset", "--quiet", "--hard", "HEAD~1"]);
    commit(&wt, "README.md", "# shop, written again\n");
    commit(&wt, "version.txt", "0.3\n");

    let ctx = checked(&w, &t).await;
    assert_eq!(ctx.approved, None);
    assert_eq!(ctx.diff, whole_branch(&w, &t, &wt).await);
    w.stop().await;
}

/// The latest approval counts: a second Pass at a human check moves the scope's start up.
#[tokio::test(flavor = "multi_thread")]
async fn the_latest_approval_wins() {
    let w = world().await;
    let (t, wt) = spec_written(&w).await;
    approve_at_spec_review(&w, &t).await;
    commit(&wt, "plan.txt", "the plan\n");
    let second = head(&wt);
    approve_at_spec_review(&w, &t).await;
    assert_eq!(approved_sha(&w, &t).await.as_deref(), Some(second.as_str()));
    commit(&wt, "version.txt", "0.3\n");

    let ctx = checked(&w, &t).await;
    assert!(ctx.diff.contains("version.txt"), "{}", ctx.diff);
    assert!(!ctx.diff.contains("plan.txt") && !ctx.diff.contains("README.md"));
    let files = ctx.approved.unwrap().files;
    assert_eq!(
        files,
        vec!["A\tREADME.md".to_owned(), "A\tplan.txt".to_owned()]
    );
    w.stop().await;
}

/// Not approvals: a Fail at the human check and a drag back from it leave the recorded approval as it was.
#[tokio::test(flavor = "multi_thread")]
async fn a_fail_or_a_drag_back_is_no_approval() {
    let w = world().await;
    let (t, wt) = spec_written(&w).await;
    approve_at_spec_review(&w, &t).await;
    let approved = approved_sha(&w, &t).await;
    assert!(approved.is_some());
    commit(&wt, "version.txt", "0.3\n");
    let key = t.key.to_string();

    place_in(&w, &t, "qa", Status::WaitingForYou).await;
    w.review
        .fail(&key, "Not like this.", Vec::new(), false, &me())
        .await
        .unwrap();
    assert_eq!(approved_sha(&w, &t).await, approved, "a Fail");

    place_in(&w, &t, "qa", Status::WaitingForYou).await;
    assert_ne!(Some(head(&wt)), approved);
    let answers = DragAnswers {
        note: "Once more.",
        attachments: Default::default(),
        confirmed: true,
        clear_first: false,
    };
    w.review
        .drag(&key, &DropOn::stage("dev"), answers, &w.validation, &me())
        .await
        .unwrap();
    assert_eq!(approved_sha(&w, &t).await, approved, "a drag back");
    w.stop().await;
}

/// A Pass straight into an agent check (QA by you, then Security): approved, and the check is told plainly that
/// nothing changed since (decision 116 as written).
#[tokio::test(flavor = "multi_thread")]
async fn a_pass_into_an_agent_check_leaves_it_nothing_new() {
    let w = world().await;
    let mut file = every_kind();
    // Dev → Validate → QA → Security → Done.
    let qa = file.flow.stages.remove(file.flow.position("qa").unwrap());
    let security = file.flow.position("security").unwrap();
    file.flow.stages.insert(security, qa);
    w.board.use_flow_for_tests("SHOP", &file).await;
    w.claude_has_skill("team-security-review").await;
    let t = w.create("SHOP", "Export as CSV").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "export.txt", "csv\n");
    place_in(&w, &t, "qa", Status::WaitingForYou).await;
    let key = t.key.to_string();
    w.review.pass(&key, &w.validation, &me()).await.unwrap();
    assert_eq!(
        w.board.ticket(&key).await.unwrap().place,
        Place::stage_of("security")
    );
    assert_eq!(approved_sha(&w, &t).await, Some(head(&wt)));

    let ctx = w.board.validate_prime(&key).await.unwrap();
    assert_eq!(ctx.diff.trim(), "");
    let text = render_validate(&ctx);
    assert!(
        text.contains(
            "Nothing changed since the user approved it at QA: there is nothing new to judge, so pass with a short \
             note saying so.\n\n- A export.txt\n"
        ),
        "{text}"
    );
    w.stop().await;
}

/// Reopen clears the approval: a reopened ticket's checks start over on the whole branch.
#[tokio::test(flavor = "multi_thread")]
async fn reopen_clears_the_approval() {
    let w = world().await;
    let (t, _wt) = spec_written(&w).await;
    approve_at_spec_review(&w, &t).await;
    assert!(approved_sha(&w, &t).await.is_some());
    w.force_status(&t, Status::ClosedNotMerged).await;
    w.board.reopen(&t.key.to_string(), &me()).await.unwrap();
    let row: (Option<String>, Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT approved_sha, approved_stage_id, approved_at FROM tickets WHERE id = ?",
    )
    .bind(&t.id)
    .fetch_one(w.board.store().pool())
    .await
    .unwrap();
    assert_eq!(row, (None, None, None));
    w.stop().await;
}

/// Otaq rewording the branch's commits one for one (the PR flow) moves the approval with them, so the scope holds.
#[tokio::test(flavor = "multi_thread")]
async fn a_reword_by_otaq_moves_the_approval() {
    let w = world().await;
    let (t, wt) = spec_written(&w).await;
    approve_at_spec_review(&w, &t).await;
    commit(&wt, "version.txt", "0.3\n");
    let old = head(&wt);
    let ticket = w.board.ticket(&t.key.to_string()).await.unwrap();
    let base = w.board.current_base_sha(&ticket).await.unwrap().unwrap();
    git(
        &wt,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "rebase",
            "--quiet",
            "--exec",
            "git commit --quiet --amend -m reworded",
            &base,
        ],
    );
    let new = head(&wt);
    assert_ne!(old, new);
    w.board.record_otaq_update(&t.id, &old, &new).await.unwrap();
    assert_eq!(
        approved_sha(&w, &t).await,
        Some(git(&wt, &["rev-parse", "HEAD~1"]))
    );

    let ctx = checked(&w, &t).await;
    assert!(ctx.approved.is_some());
    assert!(ctx.diff.contains("version.txt"), "{}", ctx.diff);
    assert!(!ctx.diff.contains("README.md"), "{}", ctx.diff);
    w.stop().await;
}
