//! The early conflict flag (T-0096, GIT-04; d0038, d0127): after a merge into a branch, every ticket waiting in Review
//! that merges into it is test-merged read-only and flagged "conflicts with `main`" or cleared. A flagged ticket in a
//! human check goes back to its agent with `restack` at once, no click (T-0497), counting toward the same three in a
//! row; one whose PR is open keeps only the flag, and so does every ticket of a paused project until its Resume.
//! `claude` stands in as an `sh` printing its arguments.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::pause::PauseScope;
use otaq_core::board::comments::{NewComment, Side};
use otaq_core::board::conflicts::ConflictReason;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, AutoMergeStop, Place, Status};
use otaq_core::flow::default::DEV;
use otaq_core::git::Git;
use otaq_core::git::testing::{commit, git, snapshot};
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::session::deliveries::handoff_sentence;

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 30";

fn stand_in(config: ServiceConfig) -> ServiceConfig {
    ServiceConfig {
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
    }
}

async fn world() -> TestService {
    TestService::start_with(stand_in).await
}

fn shop() -> PauseScope {
    PauseScope::Project {
        project: "SHOP".into(),
    }
}

fn me() -> Actor {
    Actor::User("me".into())
}

async fn reviewed(w: &TestService, title: &str, file: &str, content: &str) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", title).await;
    let wt = w.start_worktree(&t).await;
    std::fs::create_dir_all(wt.join("lib")).unwrap();
    commit(&wt, file, content);
    w.force_status(&t, Status::WaitingForYou).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), wt)
}

/// Everything git keeps about the repository's state that a test merge must not change.
fn state(repo: &Path) -> (String, String, Vec<String>) {
    (
        git(repo, &["for-each-ref", "--format=%(refname) %(objectname)"]),
        git(repo, &["status", "--porcelain=v1", "--untracked-files=all"]),
        snapshot(repo)
            .into_iter()
            .filter(|p| !p.starts_with(".git/objects") && !p.starts_with(".git/logs"))
            .collect(),
    )
}

/// SHOP-1 and SHOP-2 edit the same line of `lib/pricing.ts`; SHOP-3 another file. All three wait in Review.
async fn three_in_review(w: &TestService) -> (Ticket, Ticket, Ticket) {
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    std::fs::create_dir_all(repo.join("lib")).unwrap();
    commit(&repo, "lib/pricing.ts", "base\n");
    let (shop1, _) = reviewed(w, "Checkout API", "lib/pricing.ts", "from SHOP-1\n").await;
    let (shop2, _) = reviewed(w, "Price table", "lib/pricing.ts", "from SHOP-2\n").await;
    let (shop3, _) = reviewed(w, "Footer links", "lib/footer.ts", "footer\n").await;
    (shop1, shop2, shop3)
}

async fn merge(w: &TestService, key: &str) {
    let outcome = w
        .review
        .local_merge(key, &format!("{key}: merged"), false, &me())
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
}

async fn ticket(w: &TestService, t: &Ticket) -> Ticket {
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

/// Still waiting in Review with the flag, and nothing handed to an agent.
async fn only_flagged(w: &TestService, t: &Ticket) {
    let after = ticket(w, t).await;
    assert_eq!(after.status, Status::WaitingForYou);
    assert_eq!(after.flags.conflicts_with.as_deref(), Some("main"));
    assert_eq!(after.auto_merge_conflicts, 0);
    assert!(
        w.sessions.running_of(&t.id, SessionKind::Agent).is_none(),
        "no agent started"
    );
}

async fn until(what: &str, mut done: impl AsyncFnMut() -> bool) {
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        while !done().await {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    assert!(reached.is_ok(), "never: {what}");
}

/// d0127: merging SHOP-1 hands SHOP-2 back to Dev's session with restack, no click; its unsent line comment stays a
/// draft. SHOP-3 doesn't conflict and stays.
#[tokio::test(flavor = "multi_thread")]
async fn merging_one_ticket_hands_back_the_one_that_would_conflict() {
    let w = world().await;
    let (shop1, shop2, shop3) = three_in_review(&w).await;
    let agent = w.agent_shell(&shop2).await;
    w.board
        .add_comment(&NewComment {
            key: shop2.key.to_string(),
            file: "lib/pricing.ts".into(),
            line: 1,
            side: Side::New,
            body: "Round half-up here".into(),
        })
        .await
        .unwrap();

    merge(&w, "SHOP-1").await;

    let back = ticket(&w, &shop2).await;
    assert_eq!(back.status, Status::MergeConflictFixing);
    assert_eq!(back.place, Place::stage_of(DEV));
    assert_eq!(back.auto_merge_conflicts, 1, "counts toward the three");
    assert_eq!(back.flags.conflicts_with.as_deref(), Some("main"));
    assert!(back.flags.conflict_in_review);
    let conflict = w.board.conflict(&shop2.id).await.unwrap().unwrap();
    assert_eq!(conflict.files, ["lib/pricing.ts"]);
    assert_eq!(conflict.reason, ConflictReason::TestMerge);
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        pending.last().unwrap().text,
        handoff_sentence(w.adapters.of_session(None).skill_root(), "restack", None)
    );
    let comments = w
        .board
        .comments(&shop2.key.to_string(), None)
        .await
        .unwrap();
    assert_eq!(comments.pending, 1, "the line comment stays a draft");

    let clean = ticket(&w, &shop3).await;
    assert_eq!(clean.status, Status::WaitingForYou);
    assert_eq!(clean.flags.conflicts_with, None);
    assert_eq!(shop1.flags.conflicts_with, None);
    w.stop().await;
}

/// d0127: a ticket whose PR is open keeps only the flag (its conflicts are the PR's).
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_whose_pr_is_open_keeps_only_the_flag() {
    let w = world().await;
    let (_, shop2, _) = three_in_review(&w).await;
    sqlx::query("UPDATE tickets SET pr_number = 481 WHERE id = ?")
        .bind(&shop2.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    merge(&w, "SHOP-1").await;
    only_flagged(&w, &shop2).await;
    w.stop().await;
}

/// d0127, d0112: while the project is paused the flag stays and nothing starts; the Resume of the pause hands it back.
#[tokio::test(flavor = "multi_thread")]
async fn while_paused_only_the_flag_and_the_hand_back_on_resume() {
    let w = world().await;
    let (_, shop2, _) = three_in_review(&w).await;
    // Its conversation is stored: the hand-back resumes it.
    w.record_ended_agent(&shop2, "conv-shop-2").await;
    w.pause.pause(&shop()).await.unwrap();
    merge(&w, "SHOP-1").await;
    only_flagged(&w, &shop2).await;
    // A restart keeps the pause and the flag.
    let w = w.restart_with(stand_in).await;
    only_flagged(&w, &shop2).await;

    w.pause.resume(&shop(), &me()).await.unwrap();
    let back = ticket(&w, &shop2).await;
    assert_ne!(back.status, Status::WaitingForYou);
    assert_eq!(back.auto_merge_conflicts, 1);
    until("SHOP-2's agent starts", async || {
        w.sessions
            .running_of(&shop2.id, SessionKind::Agent)
            .is_some()
    })
    .await;
    let session = w
        .sessions
        .running_of(&shop2.id, SessionKind::Agent)
        .unwrap();
    let out = w.wait_for_output(&session.id, "ARGS").await;
    assert!(
        out.contains("--resume conv-shop-2") && out.contains("restack"),
        "{out}"
    );
    // A second Resume finds nothing held.
    w.pause.resume(&shop(), &me()).await.unwrap();
    assert_eq!(ticket(&w, &shop2).await.auto_merge_conflicts, 1);
    w.stop().await;
}

/// d0127, d0064: the third conflict in a row stops it turquoise, as at merge.
#[tokio::test(flavor = "multi_thread")]
async fn the_third_conflict_in_a_row_stops_it() {
    let w = world().await;
    let (_, shop2, _) = three_in_review(&w).await;
    sqlx::query("UPDATE tickets SET auto_merge_conflicts = 2 WHERE id = ?")
        .bind(&shop2.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    merge(&w, "SHOP-1").await;
    let after = ticket(&w, &shop2).await;
    assert_eq!(after.status, Status::DeliveryStopped);
    assert_eq!(after.auto_merge_stop, Some(AutoMergeStop::KeepsConflicting));
    assert_eq!(
        after.signal.unwrap().text.as_deref(),
        Some("Keeps conflicting with `main`")
    );
    assert!(
        w.sessions
            .running_of(&shop2.id, SessionKind::Agent)
            .is_none(),
        "no agent started"
    );
    w.stop().await;
}

/// A ticket kept in Review (here: its PR is open) loses the flag once the next test merge is clean.
#[tokio::test(flavor = "multi_thread")]
async fn the_flag_clears_when_the_next_test_merge_is_clean() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    std::fs::create_dir_all(repo.join("lib")).unwrap();
    commit(&repo, "lib/pricing.ts", "base\n");
    reviewed(&w, "Checkout API", "lib/pricing.ts", "from SHOP-1\n").await;
    let (shop2, wt2) = reviewed(&w, "Price table", "lib/pricing.ts", "from SHOP-2\n").await;
    sqlx::query("UPDATE tickets SET pr_number = 481 WHERE id = ?")
        .bind(&shop2.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    merge(&w, "SHOP-1").await;
    assert!(w.board.conflict(&shop2.id).await.unwrap().is_some());

    // SHOP-2's branch changes (its agent rebased and resolved it); the next merge into main re-checks it.
    git(&wt2, &["rebase", "--quiet", "-X", "theirs", "main"]);
    reviewed(&w, "Other", "lib/other.ts", "x\n").await;
    merge(&w, "SHOP-3").await;
    assert_eq!(w.board.conflict(&shop2.id).await.unwrap(), None);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_test_merge_changes_no_worktree_index_branch_or_ref() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    std::fs::create_dir_all(repo.join("lib")).unwrap();
    commit(&repo, "lib/pricing.ts", "base\n");
    let (_, wt) = reviewed(&w, "Price table", "lib/pricing.ts", "ticket\n").await;
    commit(&repo, "lib/pricing.ts", "main moved\n");
    std::fs::write(repo.join("file1.txt"), "the user's edit\n").unwrap();
    let before = (state(&repo), state(&wt));

    let git_ = Git::find().unwrap();
    let files = git_
        .test_merge(&repo, "main", &otaq_core::naming::branch("SHOP-1"))
        .await
        .unwrap();
    assert_eq!(files, Some(vec!["lib/pricing.ts".to_owned()]));
    let clean = git_.test_merge(&repo, "main", "main").await.unwrap();
    assert_eq!(clean, None);
    assert_eq!((state(&repo), state(&wt)), before);
    w.stop().await;
}
