//! The Delivery (T-0419; FLW-15, FLW-16, FLW-17; d0108, d0038, d0050, d0059, d0064) against temporary repositories:
//! a ticket passes its checks (its validator is an ordinary `sh`; the verdict is reported from that session) and the
//! project's flow decides what happens next. A Delivery with `mode: automatic` and `delivers: local` merges it
//! without a click; with `delivers: pr` its agent (an `sh` standing in) gets the PR skill and Otaq confirms the
//! proposal itself, no dialog. Work moved past a check never delivers by itself; a flow with no check says
//! "Delivered without checks". The repository's own folder stands for the user's checkout; `origin` is a local bare
//! repository, and a script named `gh` stands in for GitHub's CLI. A conflict handed back to the agent brings a
//! build freed at Review back first (T-0309, M19-c).

use crate::common::freed;
use otaq_core::service::testing::said;

use std::path::PathBuf;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::tickets::EventKind;
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::domain::{
    Actor, AutoMergeStop, BuildState, Kind, Place, Status, ValidationAction, ValidationTrigger,
};
use otaq_core::flow::default::testing::{
    automatic_local_delivery, automatic_pr_delivery, with_rounds, without_validate,
};
use otaq_core::flow::default::{DELIVER, DEV};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::review::delivery::Delivered;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::session::deliveries::handoff_sentence;

fn me() -> Actor {
    Actor::User("me".into())
}

async fn world() -> TestService {
    TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "exec sleep 30".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        ..config
    })
    .await
}

/// Puts SHOP on Dev → Validate → Done with an automatic local Delivery.
async fn auto_merge(w: &TestService) {
    w.board
        .edit_flow_for_tests("SHOP", automatic_local_delivery)
        .await;
}

/// A working ticket (or an epic's child) with one commit on `file` in its worktree.
async fn working(w: &TestService, title: &str, epic: Option<&Ticket>, file: &str) -> Ticket {
    let t = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: title.into(),
                epic: epic.map(|e| e.key.to_string()),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let wt = w.start_worktree(&t).await;
    commit(&wt, file, &format!("{title}\n"));
    w.force_status(&t, Status::Working).await;
    w.board.ticket(&t.key.to_string()).await.unwrap()
}

/// Submits and passes validation; returns the ticket after it.
async fn pass(w: &TestService, t: &Ticket) -> Ticket {
    let key = t.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "done").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let validator = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .unwrap()
        .id;
    w.validation
        .verdict(
            &key,
            true,
            said(&w.board, &key, "ok").await.as_ref(),
            &validator,
        )
        .await
        .unwrap();
    w.board.ticket(&key).await.unwrap()
}

async fn epic(w: &TestService) -> Ticket {
    w.board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Wishlist".into(),
                kind: Kind::Epic,
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap()
}

/// A click Delivery (the default flow): Ready for Review. An automatic local Delivery: an epic child merges into its
/// epic branch, a standalone ticket into its base. The merge is local and never pushed.
#[tokio::test(flavor = "multi_thread")]
async fn which_tickets_merge_by_themselves() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let epic = epic(&w).await;

    let off = working(&w, "Off", None, "off.ts").await;
    assert_eq!(pass(&w, &off).await.status, Status::WaitingForYou);

    auto_merge(&w).await;
    let child = working(&w, "Wishlist API", Some(&epic), "wishlist.ts").await;
    let main = git(&repo, &["rev-parse", "main"]);
    let merged = pass(&w, &child).await;
    assert_eq!(merged.status, Status::Merged);
    assert!(merged.flags.merged_automatically);
    assert!(!merged.flags.delivered_without_checks, "Validate passed");
    assert_eq!(
        git(
            &repo,
            &[
                "log",
                "-1",
                "--format=%s",
                &naming::branch(&epic.key.to_string())
            ]
        ),
        format!("{}: Wishlist API", child.key)
    );
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main untouched");

    let all = working(&w, "Header", None, "header.ts").await;
    let merged = pass(&w, &all).await;
    assert_eq!(merged.status, Status::Merged);
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s", "main"]),
        format!("{}: Header", all.key)
    );
    let history = w.board.history(&all.key.to_string()).await.unwrap();
    assert!(
        history
            .iter()
            .any(|e| e.kind == EventKind::MergedAutomatically)
    );
    w.stop().await;
}

/// d0108 rule 8: an automatic Delivery with no check before it is allowed; the ticket says "Delivered without
/// checks" and History records it.
#[tokio::test(flavor = "multi_thread")]
async fn a_flow_with_no_check_delivers_without_checks() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    auto_merge(&w).await;
    w.board.edit_flow_for_tests("SHOP", without_validate).await;
    let t = working(&w, "Footer", None, "footer.ts").await;
    let after = w
        .validation
        .submit(
            &t.key.to_string(),
            said(&w.board, &t.key.to_string(), "done").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    assert_eq!(after.status, Status::Merged);
    assert!(after.flags.merged_automatically);
    assert!(after.flags.delivered_without_checks);
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s", "main"]),
        format!("{}: Footer", t.key)
    );
    let history = w.board.history(&t.key.to_string()).await.unwrap();
    let merged = history
        .iter()
        .find(|e| e.kind == EventKind::MergedAutomatically)
        .unwrap();
    assert_eq!(merged.payload["without_checks"], true);
    w.stop().await;
}

/// Skip this check leads to a click (d0108 "Move on anyway": automatic delivery is only for work that passed its
/// checks): the card waits in its Delivery. The user's Move to next stage runs the check like the agent's `stage next`
/// (d0108 rule 10): its pass may deliver by itself.
#[tokio::test(flavor = "multi_thread")]
async fn the_users_routes_past_a_check_lead_to_a_click() {
    let w = world().await;
    w.add_project("shop-web").await;
    auto_merge(&w).await;

    let submitted = working(&w, "Submitted", None, "a.ts").await;
    w.force_status(&submitted, Status::Idle).await;
    let after = w
        .validation
        .submit_by_user(&submitted.key.to_string(), "", &me())
        .await
        .unwrap();
    assert_eq!(after.status, Status::Checking);

    let skipped = working(&w, "Skipped", None, "b.ts").await;
    w.validation
        .submit(
            &skipped.key.to_string(),
            said(&w.board, &skipped.key.to_string(), "done")
                .await
                .as_ref(),
            &Actor::Agent(skipped.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let after = w
        .validation
        .user_action(&skipped.key.to_string(), ValidationAction::Skip, "", &me())
        .await
        .unwrap();
    assert_eq!(
        (after.status, &after.place),
        (Status::ToDeliver, &Place::stage_of(DELIVER))
    );
    assert!(!after.flags.delivering_automatically);
    let now = w.board.delivery_now(&after).await.unwrap().unwrap();
    assert!(!now.automatic, "a skipped check: click mode");
    w.stop().await;
}

/// d0108 "Move on anyway": a ticket moved past its failed check reaches its automatic Delivery in click mode; one
/// whose check passed delivers by itself.
#[tokio::test(flavor = "multi_thread")]
async fn move_on_anyway_forces_click_mode() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    auto_merge(&w).await;
    w.board.edit_flow_for_tests("SHOP", with_rounds(1)).await;

    let failed = working(&w, "Failed", None, "a.ts").await;
    let key = failed.key.to_string();
    w.validation
        .submit(
            &key,
            said(&w.board, &key, "done").await.as_ref(),
            &Actor::Agent(failed.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    let checker = w
        .sessions
        .running_of(&failed.id, SessionKind::Validator)
        .unwrap()
        .id;
    let after = w
        .validation
        .verdict(
            &key,
            false,
            said(&w.board, &key, "- a.ts:1 wrong").await.as_ref(),
            &checker,
        )
        .await
        .unwrap();
    assert_eq!(after.status, Status::CheckFailed);
    // Move on anyway takes it on to the Delivery (the engine's next stage), which waits for the user's click.
    let after = w
        .validation
        .user_action(&key, ValidationAction::MoveOnAnyway, "", &me())
        .await
        .unwrap();
    assert_eq!(
        (after.status, &after.place),
        (Status::ToDeliver, &Place::stage_of(DELIVER)),
        "waits for the user's click"
    );
    assert!(!after.flags.delivering_automatically);
    let now = w.board.delivery_now(&after).await.unwrap().unwrap();
    assert!(!now.automatic);
    assert!(now.forced.is_some(), "the reason for the card");
    assert_ne!(
        git(&repo, &["log", "-1", "--format=%s", "main"]),
        format!("{}: Failed", failed.key)
    );

    let passed = working(&w, "Passed", None, "b.ts").await;
    assert_eq!(pass(&w, &passed).await.status, Status::Merged);
    w.stop().await;
}

/// Success: Done, the cleanup (worktree removed, the agent's session ended) and the waiting tickets start.
#[tokio::test(flavor = "multi_thread")]
async fn a_merge_cleans_up_and_starts_the_waiting_tickets() {
    let w = world().await;
    w.add_project("shop-web").await;
    auto_merge(&w).await;
    let parent = working(&w, "Checkout API", None, "api.ts").await;
    let agent = w.agent_shell(&parent).await;
    let waiting = w.create("SHOP", "Checkout page").await;
    w.board
        .add_dependency(&waiting.key.to_string(), &parent.key.to_string(), &me())
        .await
        .unwrap();
    w.force_status(&waiting, Status::Queued).await;

    let mut listener = w.events.listen();
    let merged = pass(&w, &parent).await;
    assert_eq!(merged.status, Status::Merged);
    // The toast (T-0196, UI-07); no notification.
    let toast = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(otaq_core::protocol::Event::MergedAutomatically { title, message, .. }) =
                listener.receiver.recv().await
            {
                return (title, message);
            }
        }
    })
    .await
    .expect("the merge's toast");
    assert_eq!(
        toast.0,
        format!("{} merged automatically into main", parent.key)
    );
    assert!(
        toast
            .1
            .starts_with("Validated · round 1 · local, not pushed"),
        "{}",
        toast.1
    );
    let worktree = parent.work.as_ref().unwrap().worktree.clone().unwrap();
    assert!(
        !std::path::Path::new(&worktree).exists(),
        "worktree removed"
    );
    assert!(!w.sessions.info(&agent.session).unwrap().is_running());
    // Its start runs in the background, as after any merge.
    let start = std::time::Instant::now();
    while w.status(&waiting.key.to_string()).await == Status::Queued {
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "the waiting ticket never started"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    w.stop().await;
}

/// FLW-17: a conflict goes to Dev's session with restack, no click ("Merge conflict · fixing"); the third in a row
/// stops there, turquoise; Retry merge runs the automatic Delivery again, and a merge resets the count.
#[tokio::test(flavor = "multi_thread")]
async fn conflicts_go_back_to_the_agent_until_the_third() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    auto_merge(&w).await;
    let t = working(&w, "Pricing", None, "lib.ts").await;
    let agent = w.agent_shell(&t).await;
    commit(&repo, "lib.ts", "main\n");

    let after = pass(&w, &t).await;
    assert_eq!(after.status, Status::MergeConflictFixing);
    assert_eq!(after.place, Place::stage_of(DEV));
    assert_eq!(after.auto_merge_conflicts, 1);
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        pending.last().unwrap().text,
        handoff_sentence(w.adapters.of_session(None).skill_root(), "restack", None)
    );
    assert_eq!(
        after.validation_round, 0,
        "the next submit validates from 0"
    );

    for n in 2..=3 {
        let t = w.board.ticket(&t.key.to_string()).await.unwrap();
        let after = pass(&w, &t).await;
        assert_eq!(after.place, Place::stage_of(DEV), "conflict {n}");
        if n < 3 {
            assert_eq!(after.status, Status::MergeConflictFixing);
        } else {
            assert_eq!(after.status, Status::DeliveryStopped);
            assert_eq!(after.auto_merge_stop, Some(AutoMergeStop::KeepsConflicting));
            assert_eq!(
                after.signal.unwrap().text.as_deref(),
                Some("Keeps conflicting with `main`")
            );
        }
    }

    // The user resolves it in main: Retry merge lands it and resets the count.
    commit(&repo, "lib.ts", "Pricing\n");
    let after = w
        .validation
        .user_action(&t.key.to_string(), ValidationAction::RetryMerge, "", &me())
        .await
        .unwrap();
    assert_eq!(after.status, Status::Merged);
    assert_eq!(after.auto_merge_conflicts, 0);
    w.stop().await;
}

/// A dirty checkout of the target stops with the Local merge's message; Move to Review leaves it waiting in its
/// Delivery for the user's click (this flow has no Review), not merged again.
#[tokio::test(flavor = "multi_thread")]
async fn a_dirty_checkout_stops_it() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    auto_merge(&w).await;
    let t = working(&w, "Checkout API", None, "api.ts").await;
    std::fs::write(repo.join("file1.txt"), "my edit\n").unwrap();
    let after = pass(&w, &t).await;
    assert_eq!(after.status, Status::DeliveryStopped);
    assert_eq!(after.auto_merge_stop, Some(AutoMergeStop::CheckoutDirty));
    assert_eq!(
        after.signal.unwrap().text.as_deref(),
        Some("shop-web has uncommitted changes on main. Clean up and retry.")
    );
    assert_eq!(
        std::fs::read_to_string(repo.join("file1.txt")).unwrap(),
        "my edit\n"
    );
    let after = w
        .validation
        .user_action(
            &t.key.to_string(),
            ValidationAction::MoveToReview,
            "",
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(
        (after.status, &after.place),
        (Status::ToDeliver, &Place::stage_of(DELIVER))
    );
    assert_eq!(after.auto_merge_stop, None);
    w.stop().await;
}

/// On the default flow a stopped Delivery's Move to Review is back in Review, still "Validated · round 1": the
/// stop of a click Local merge that kept conflicting.
#[tokio::test(flavor = "multi_thread")]
async fn a_click_merge_that_keeps_conflicting_stops_in_dev() {
    let w = world().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let t = working(&w, "Pricing", None, "lib.ts").await;
    let key = t.key.to_string();
    w.agent_shell(&t).await;
    commit(&repo, "lib.ts", "main\n");
    for n in 1..=3 {
        let t = w.board.ticket(&key).await.unwrap();
        assert_eq!(pass(&w, &t).await.status, Status::WaitingForYou, "{n}");
        let merged = w.review.local_merge(&key, "m", false, &me()).await.unwrap();
        assert!(matches!(
            merged,
            otaq_core::review::LocalMergeOutcome::Conflict { .. }
        ));
        let after = w.board.ticket(&key).await.unwrap();
        assert_eq!(after.place, Place::stage_of(DEV), "conflict {n}");
        assert_eq!(
            after.status,
            if n < 3 {
                Status::MergeConflictFixing
            } else {
                Status::DeliveryStopped
            }
        );
    }
    let after = w
        .validation
        .user_action(&key, ValidationAction::MoveToReview, "", &me())
        .await
        .unwrap();
    assert_eq!(after.status, Status::WaitingForYou);
    assert_eq!(
        after.flags.review_entry.unwrap().to_string(),
        "Validated · round 1"
    );
    w.stop().await;
}

/// A base that diverged from its remote stops with GIT-01's message.
#[tokio::test(flavor = "multi_thread")]
async fn a_diverged_base_stops_it() {
    let w = world().await;
    let clone = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: clone.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let seed = w.repos.root().join("shop-web-seed");
    auto_merge(&w).await;
    let t = working(&w, "Checkout API", None, "api.ts").await;
    commit(&seed, "team.ts", "from the team\n");
    git(&seed, &["push", "--quiet", "origin", "main"]);
    commit(&clone, "mine.ts", "local only\n");
    let after = pass(&w, &t).await;
    assert_eq!(after.status, Status::DeliveryStopped);
    assert_eq!(after.auto_merge_stop, Some(AutoMergeStop::BaseDiverged));
    assert_eq!(
        after.signal.unwrap().text.as_deref(),
        Some("`main` and `origin/main` have diverged. Push or pull, then retry")
    );
    w.stop().await;
}

/// An automatic Delivery's conflict goes back to the agent without a click: a build freed at Review comes back
/// before the restack skill is queued.
#[tokio::test(flavor = "multi_thread")]
async fn a_conflict_handed_to_the_agent_gets_the_build_back() {
    let w = freed::world().await;
    let project = freed::shop_web(&w, |_| {}).await;
    w.board
        .edit_flow_for_tests("SHOP", automatic_local_delivery)
        .await;
    let repo = PathBuf::from(&project.repo_path);
    let (t, wt) = freed::freed(&w, "Pricing", "lib.ts").await;
    let key = t.key.to_string();
    let agent = w.agent_shell(&t).await;
    commit(&repo, "lib.ts", "main\n");

    assert_eq!(
        w.review.deliver_locally(&key, false).await.unwrap(),
        Delivered::Conflict
    );

    assert_eq!(w.status(&key).await, Status::MergeConflictFixing);
    assert_eq!(freed::app_bin(&wt).as_deref(), Some("Pricing"));
    assert_eq!(freed::state(&w, &key).await, BuildState::Present);
    assert_eq!(freed::entries(&w, &project), Vec::<u64>::new());
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        pending.last().unwrap().text,
        handoff_sentence(w.adapters.of_session(None).skill_root(), "restack", None)
    );
    w.stop().await;
}

/// The commits of `range` in `repo`, oldest first: their full ids.
fn commits(repo: &std::path::Path, range: &str) -> Vec<String> {
    git(repo, &["log", "--reverse", "--format=%H", range])
        .lines()
        .map(str::to_owned)
        .collect()
}

/// What the stand-in `gh` prints for `gh pr create`: the PR's URL.
fn stand_in_gh(dir: &std::path::Path) -> std::path::PathBuf {
    otaq_core::process::testing::stub_command(
        dir,
        "gh",
        "echo https://github.com/acme/shop-web/pull/481",
    )
}

/// FLW-15, FLW-16: an automatic PR Delivery opens the PR with no click and no dialog: the Delivery's agent (the
/// Developer, Dev's agent, so Dev's session) gets the PR skill told it is automatic; its proposal is confirmed by Otaq
/// as it is and the agent is told to push. Send back with PR comments then resumes Dev's session with
/// address-review; the ticket walks Validate again and the Delivery pushes to the same PR, again with no dialog.
#[tokio::test(flavor = "multi_thread")]
async fn an_automatic_pr_opens_with_no_dialog_and_pr_comments_push_to_the_same_pr() {
    use otaq_core::board::settings::WorkflowStep;
    use otaq_core::pr::flow::{FlowKind, FlowState};
    use otaq_core::pr::service::automatic_instruction;

    let w = world().await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    // origin is a local folder, not GitHub: the project uses its own Open PR skill.
    let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "my-pr");
    w.import_step_skill(WorkflowStep::OpenPr, &folder).await;
    w.board
        .edit_flow_for_tests("SHOP", automatic_pr_delivery)
        .await;
    let t = working(&w, "Login redirect", None, "login.ts").await;
    let key = t.key.to_string();
    let agent = w.agent_shell(&t).await;
    let as_agent = Actor::Agent(t.key.clone());
    let tools = tempfile::tempdir().unwrap();
    let gh = stand_in_gh(tools.path());

    // Validate passes: the Delivery starts by itself, Dev's session gets the skill.
    let delivering = pass(&w, &t).await;
    assert_eq!(
        (delivering.status, &delivering.place),
        (Status::ToDeliver, &Place::stage_of(DELIVER))
    );
    assert!(delivering.flags.delivering_automatically);
    let flow = w.board.pr_flow(&t.id).await.unwrap().unwrap();
    assert!(flow.automatic);
    assert_eq!(flow.state, FlowState::Preparing);
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    let skill = &pending.last().unwrap().text;
    assert!(
        skill.starts_with(&handoff_sentence(
            w.adapters.of_session(None).skill_root(),
            "my-pr",
            Some("automatic:")
        )),
        "{skill}"
    );

    // The skill proposes: Otaq confirms it at once, nothing waits in a dialog, and tells the agent to push.
    let ids = commits(&repo, &format!("main..{}", naming::branch(&key)));
    let proposal = serde_json::json!({
        "branch": "feature/SHOP-1-login",
        "commits": [{"sha": &ids[0][..7], "message": "SHOP-1: redirect after login"}],
        "title": "SHOP-1 Login redirect",
        "description": "Returns to the page you came from."
    });
    w.prs
        .propose(&key, &proposal.to_string(), &as_agent)
        .await
        .unwrap();
    let flow = w.board.pr_flow(&t.id).await.unwrap().unwrap();
    assert_eq!(flow.state, FlowState::Pushing, "no dialog");
    let prime = otaq_core::prime::render(
        &w.board
            .prime_context(&key, w.adapters.for_new_session())
            .await
            .unwrap(),
    );
    assert!(prime.contains("- Mode: automatic (no dialog)"), "{prime}");
    assert!(prime.contains("confirmed it automatically."), "{prime}");
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        pending.last().unwrap().text,
        automatic_instruction(FlowKind::Open)
    );
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s", "feature/SHOP-1-login"]),
        "SHOP-1: redirect after login"
    );

    // The agent pushes and opens the PR with `gh` (the stand-in), then reports it.
    git(
        &repo,
        &["push", "--quiet", "origin", "feature/SHOP-1-login"],
    );
    let created = std::process::Command::new(&gh)
        .args([
            "pr",
            "create",
            "--base",
            "main",
            "--head",
            "feature/SHOP-1-login",
        ])
        .output()
        .unwrap();
    let url = String::from_utf8(created.stdout).unwrap();
    let open = w.prs.opened(&key, url.trim(), &as_agent).await.unwrap();
    assert_eq!(
        (open.status, &open.place),
        (Status::PrOpen, &Place::stage_of(DELIVER))
    );
    assert_eq!(open.flags.pr_number, Some(481));
    assert!(!open.flags.delivering_automatically);

    // Send back with PR comments: the note is the user's comment (CMT-05), posted first; Dev's session is typed
    // address-review and what came back, never the note.
    let sent = w
        .review
        .send_back_pr(&key, "Keep the old route working", None, false, &me())
        .await
        .unwrap();
    assert!(matches!(
        sent,
        otaq_core::review::ReturnOutcome::Done { .. }
    ));
    let back = w.board.ticket(&key).await.unwrap();
    assert_eq!(
        (back.status, &back.place),
        (Status::Working, &Place::stage_of(DEV))
    );
    let out = w
        .wait_for_output(
            &agent.session,
            &handoff_sentence(
                w.adapters.of_session(None).skill_root(),
                "address-review",
                Some(
                    "SHOP-1 was sent back to Dev with PR comments and a comment. Run `otaq prime`, then `otaq \
                     review feedback`.",
                ),
            ),
        )
        .await;
    assert!(!out.contains("Keep the old route working"), "{out}");
    let note = crate::common::last_comment(&w.board, &key).await.unwrap();
    assert_eq!(
        (note.author_label(), note.body.as_str()),
        ("You".to_owned(), "Keep the old route working")
    );
    assert!(
        w.board
            .history(&key)
            .await
            .unwrap()
            .iter()
            .any(|e| e.kind == EventKind::SentBackWithPrComments)
    );

    // The fix walks Validate again; the Delivery pushes to the same PR, with no dialog.
    let wt = PathBuf::from(back.work.as_ref().unwrap().worktree.clone().unwrap());
    commit(&wt, "routes.ts", "old route\n");
    let delivering = pass(&w, &back).await;
    assert_eq!(delivering.status, Status::ToDeliver);
    let flow = w.board.pr_flow(&t.id).await.unwrap().unwrap();
    assert_eq!((flow.kind, flow.automatic), (FlowKind::Push, true));
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    let skill = &pending.last().unwrap().text;
    assert!(
        skill.starts_with(&handoff_sentence(
            w.adapters.of_session(None).skill_root(),
            "push",
            Some("automatic:")
        )),
        "{skill}"
    );
    let new = commits(&repo, "origin/feature/SHOP-1-login..feature/SHOP-1-login");
    assert_eq!(new.len(), 1);
    let proposal = serde_json::json!({
        "branch": "feature/SHOP-1-login",
        "commits": [{"sha": &new[0][..7], "message": "SHOP-1: keep the old route"}],
        "title": ""
    });
    w.prs
        .propose(&key, &proposal.to_string(), &as_agent)
        .await
        .unwrap();
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(
        pending.last().unwrap().text,
        automatic_instruction(FlowKind::Push)
    );
    git(
        &repo,
        &["push", "--quiet", "origin", "feature/SHOP-1-login"],
    );
    let open = w.prs.opened(&key, url.trim(), &as_agent).await.unwrap();
    assert_eq!(open.status, Status::PrOpen);
    assert_eq!(open.flags.pr_number, Some(481), "the same PR");
    w.stop().await;
}

/// FLW-16, d0108 second gap review rule 5: on an epic whose PR is open, Send back with PR comments becomes Add a
/// child from PR comments: a Backlog child holding the unresolved comments (fetched through the stand-in `gh`).
#[tokio::test(flavor = "multi_thread")]
async fn an_epic_adds_a_child_from_pr_comments() {
    const THREADS: &str = r#"{"data": {"repository": {"pullRequest": {"reviewThreads": {"nodes": [
      {"isResolved": false, "path": "lib/wishlist.ts", "line": 12, "originalLine": 12,
       "comments": {"nodes": [{"author": {"login": "anna"}, "body": "Sort by date added."}]}},
      {"isResolved": true, "path": "lib/cart.ts", "line": 3, "originalLine": 3,
       "comments": {"nodes": [{"author": {"login": "anna"}, "body": "Fixed already."}]}}
    ]}}}}}"#;
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    git(
        std::path::Path::new(&project.repo_path),
        &[
            "remote",
            "add",
            "origin",
            "git@github.com:acme/shop-web.git",
        ],
    );
    let epic = epic(&w).await;
    let key = epic.key.to_string();
    w.board
        .pr_opened(&key, "https://github.com/acme/shop-web/pull/482", &me())
        .await
        .unwrap();
    w.force_stage(&epic, DELIVER, Status::PrOpen).await;
    let tools = tempfile::tempdir().unwrap();
    let fixture = tools.path().join("threads.json");
    std::fs::write(&fixture, THREADS).unwrap();
    otaq_core::process::testing::stub_command(
        tools.path(),
        "gh",
        &format!("/bin/cat '{}'", fixture.display()),
    );
    let path = std::ffi::OsString::from(tools.path());

    let err = w
        .review
        .send_back_pr(&key, "", None, false, &me())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("add a child"), "{err}");

    let child = otaq_core::review::delivery::add_child_from_pr(&w.board, &key, &path, &me())
        .await
        .unwrap();
    assert_eq!(child.epic.as_ref(), Some(&epic.key));
    assert_eq!(child.status, Status::Backlog);
    assert_eq!(child.title, "Address the comments on PR #482");
    assert!(
        child
            .body
            .contains("- `lib/wishlist.ts:12` anna: Sort by date added."),
        "{}",
        child.body
    );
    assert!(
        !child.body.contains("Fixed already"),
        "resolved threads stay out"
    );

    // Only an epic whose PR is open.
    let other = w.create("SHOP", "Footer").await;
    assert!(
        otaq_core::review::delivery::add_child_from_pr(
            &w.board,
            &other.key.to_string(),
            &path,
            &me()
        )
        .await
        .is_err()
    );
    w.stop().await;
}

/// d0108 second gap review rule 6: a PR Delivery whose agent isn't Dev's starts a fresh delivery session in the
/// ticket's worktree; once the PR is reported, that session ends and the worktree stays until Done.
#[tokio::test(flavor = "multi_thread")]
async fn another_agent_delivers_in_a_fresh_session() {
    use otaq_core::board::settings::WorkflowStep;

    let w = world().await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "my-pr");
    w.import_step_skill(WorkflowStep::OpenPr, &folder).await;
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            automatic_pr_delivery(file);
            for stage in file.flow.stages.iter_mut().filter(|s| s.id == DELIVER) {
                stage.agent = Some("Validator".into());
            }
        })
        .await;
    let t = working(&w, "Login redirect", None, "login.ts").await;
    let key = t.key.to_string();
    let agent = w.agent_shell(&t).await;

    assert_eq!(pass(&w, &t).await.status, Status::ToDeliver);
    let delivery = w
        .sessions
        .running_of(&t.id, SessionKind::Delivery)
        .expect("a fresh delivery session");
    assert!(
        w.deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty(),
        "Dev's session gets nothing"
    );
    let worktree = t.work.as_ref().unwrap().worktree.clone().unwrap();

    let open = w
        .prs
        .opened(
            &key,
            "https://github.com/acme/shop-web/pull/7",
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    assert_eq!(open.status, Status::PrOpen);
    assert!(!w.sessions.info(&delivery.id).unwrap().is_running());
    assert!(std::path::Path::new(&worktree).is_dir(), "kept until Done");
    w.stop().await;
}

/// d0108 second gap review rule 6 (T-0419 fix): when the Delivery's agent isn't Dev's, every PR step reaches the
/// Delivery's own session. Retry after that session ended starts it again (never types into Dev's running session),
/// and the user's click Open PR runs there too, with its dialog.
#[tokio::test(flavor = "multi_thread")]
async fn retry_and_the_click_reach_the_deliverys_own_agent() {
    use otaq_core::board::settings::WorkflowStep;
    use otaq_core::pr::service::PrOutcome;

    let w = world().await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "my-pr");
    w.import_step_skill(WorkflowStep::OpenPr, &folder).await;
    let shipper = |file: &mut otaq_core::flow::FlowFile| {
        for stage in file.flow.stages.iter_mut().filter(|s| s.id == DELIVER) {
            stage.agent = Some("Validator".into());
        }
    };
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            automatic_pr_delivery(file);
            shipper(file);
        })
        .await;
    let t = working(&w, "Login redirect", None, "login.ts").await;
    let key = t.key.to_string();
    let dev = w.agent_shell(&t).await;

    assert_eq!(pass(&w, &t).await.status, Status::ToDeliver);
    let first = w
        .sessions
        .running_of(&t.id, SessionKind::Delivery)
        .expect("the Delivery's own session");
    // Its session ends before it reports the PR (it crashed, say): Retry starts the Delivery's agent again.
    w.sessions.stop(&first.id).await.unwrap();
    let outcome = w.prs.retry(&key, false).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    let again = w
        .sessions
        .running_of(&t.id, SessionKind::Delivery)
        .expect("the Delivery's session again");
    assert_ne!(again.id, first.id);
    assert!(
        w.deliveries.pending(&dev.session).await.unwrap().is_empty(),
        "nothing queued for Dev's session"
    );
    let attached = w.sessions.attach(&dev.session).unwrap();
    let dev_saw = String::from_utf8_lossy(&attached.scrollback).into_owned();
    assert!(
        !dev_saw.contains(&naming::copy_name("my-pr")),
        "nothing typed into Dev's session: {dev_saw}"
    );

    // Click mode: the user's Open PR runs on the Delivery's agent too, in its own session.
    w.prs.cancel(&key, &me()).await.unwrap();
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Delivery)
            .is_none()
    );
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            for stage in file.flow.stages.iter_mut().filter(|s| s.id == DELIVER) {
                stage.delivery = Some(otaq_core::flow::Delivery {
                    mode: otaq_core::flow::DeliveryMode::Click,
                    delivers: otaq_core::flow::Delivers::Pr,
                });
            }
        })
        .await;
    let outcome = w.prs.open(&key, true, false, &me()).await.unwrap();
    assert!(matches!(outcome, PrOutcome::Started { .. }), "{outcome:?}");
    assert!(
        w.sessions
            .running_of(&t.id, SessionKind::Delivery)
            .is_some()
    );
    let flow = w.board.pr_flow(&t.id).await.unwrap().unwrap();
    assert!(!flow.automatic, "the click's flow keeps its dialog");
    w.stop().await;
}

/// STG-09 (T-0419 fix): only the session that runs the card's stage can interrupt it. With the Delivery on its own
/// agent, Dev's leftover session exiting (a crash, a signal) changes nothing while the card is To deliver or PR open,
/// and the Delivery's `pr opened` leaves it "PR open" in its Delivery.
#[tokio::test(flavor = "multi_thread")]
async fn a_leftover_dev_session_ending_leaves_the_delivery_alone() {
    use otaq_core::board::settings::WorkflowStep;

    let w = world().await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "my-pr");
    w.import_step_skill(WorkflowStep::OpenPr, &folder).await;
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            automatic_pr_delivery(file);
            for stage in file.flow.stages.iter_mut().filter(|s| s.id == DELIVER) {
                stage.agent = Some("Validator".into());
            }
        })
        .await;
    let ended = |w: &TestService, session: String| {
        let sessions = w.sessions.clone();
        async move {
            sessions.write(&session, b"exit 3\n").unwrap();
            for _ in 0..500 {
                let info = sessions.info(&session).unwrap();
                if !info.is_running() {
                    return info;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            panic!("the session never ended");
        }
    };

    let t = working(&w, "Login redirect", None, "login.ts").await;
    let key = t.key.to_string();
    let dev = w.agent_shell(&t).await;
    assert_eq!(pass(&w, &t).await.status, Status::ToDeliver);
    let info = ended(&w, dev.session.clone()).await;
    assert_eq!(w.recovery.agent_session_ended(&info).await.unwrap(), None);
    let after = w.board.ticket(&key).await.unwrap();
    assert_eq!(
        (after.status, &after.place),
        (Status::ToDeliver, &Place::stage_of(DELIVER))
    );
    assert!(after.flags.delivering_automatically);
    assert!(
        !w.board
            .history(&key)
            .await
            .unwrap()
            .iter()
            .any(|e| e.kind == EventKind::Interrupted)
    );

    // The Delivery's own session reports the PR: PR open in the Delivery.
    let open = w
        .prs
        .opened(
            &key,
            "https://github.com/acme/shop-web/pull/12",
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    assert_eq!(
        (open.status, &open.place, open.flags.pr_number),
        (Status::PrOpen, &Place::stage_of(DELIVER), Some(12))
    );

    // PR open: a second leftover Dev session ending changes nothing either.
    let dev = w.agent_shell(&t).await;
    let info = ended(&w, dev.session.clone()).await;
    assert_eq!(w.recovery.agent_session_ended(&info).await.unwrap(), None);
    assert_eq!(w.status(&key).await, Status::PrOpen);
    w.stop().await;
}

/// `pr opened` while a PR flow is under way puts the card in its Delivery as "PR open", even when something left it
/// elsewhere meanwhile (T-0419 fix).
#[tokio::test(flavor = "multi_thread")]
async fn pr_opened_puts_the_card_in_its_delivery() {
    use otaq_core::board::settings::WorkflowStep;

    let w = world().await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    let folder = otaq_core::skills::testing::skill_folder(w.data.root(), "my-pr");
    w.import_step_skill(WorkflowStep::OpenPr, &folder).await;
    w.board
        .edit_flow_for_tests("SHOP", automatic_pr_delivery)
        .await;
    let t = working(&w, "Login redirect", None, "login.ts").await;
    let key = t.key.to_string();
    w.agent_shell(&t).await;
    assert_eq!(pass(&w, &t).await.status, Status::ToDeliver);
    w.force_stage(&t, DEV, Status::Interrupted).await;
    let open = w
        .prs
        .opened(
            &key,
            "https://github.com/acme/shop-web/pull/12",
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    assert_eq!(
        (open.status, &open.place),
        (Status::PrOpen, &Place::stage_of(DELIVER))
    );
    assert_eq!(open.interrupted_from, None);
    w.stop().await;
}
