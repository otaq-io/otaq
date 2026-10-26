//! Every preset walked end to end (T-0425; FLW-23; d0108, d0056, d0079): each shipped preset is applied to a fresh
//! project the way the user applies one (a proposal from the preset, then Apply), and a ticket walks it with stand-in
//! sessions: Spec first through Spec writer's question to a click Local merge, Solo autopilot to an automatic merge,
//! Team PR to a PR opened with a stand-in `gh`, and the default flow (Reset to the default flow) to a click Local
//! merge. d0108: "Otaq has no code path only presets use … The shipped files are its test cases". `claude` stands in as
//! an `sh` that prints its arguments and then echoes what is typed; `origin` is a local bare repository.

use otaq_core::service::testing::said;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::FixedModels;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::questions::Choice;
use otaq_core::board::tickets::{Ticket, TicketUpdate};
use otaq_core::caller::Caller;
use otaq_core::domain::{Actor, Place, Status, ValidationTrigger};
use otaq_core::flow::apply::ProposeFrom;
use otaq_core::flow::presets::preset;
use otaq_core::flow::proposal::{ApplyChoices, Source};
use otaq_core::flow::{Delivers, engine::RunResult};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::review::{LocalMergeOutcome, PassOutcome};
use otaq_core::scheduler::Scheduler;
use otaq_core::service::testing::TestService;
use otaq_core::service::{EventBus, ServiceConfig};
use otaq_core::session::{SessionInfo, SessionKind};

/// Prints its arguments (the skill it starts with), then echoes whatever is typed into it.
const STAND_IN: &str = "printf 'ARGS %s\\n' \"$*\"; exec cat";

fn me() -> Actor {
    Actor::User("me".into())
}

fn models() -> FixedModels {
    FixedModels::sample()
}

/// A service whose `claude` is the stand-in, with shop-web (SHOP) cloned from a local bare `origin`; its repository.
async fn world() -> (TestService, PathBuf) {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), STAND_IN.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    let repo = w.repos.clone_with_origin("shop-web", "main", 1);
    w.board
        .add_project(&otaq_core::board::projects::AddProject {
            path: repo.display().to_string(),
            ..Default::default()
        })
        .await
        .unwrap();
    (w, repo)
}

/// Applies the preset `file` to SHOP through a proposal from it and Apply, as the Flow tab's preset list does: no
/// problems, nothing waits (the board is quiet), and the stored version runs the preset's stages, marked as from it.
async fn apply_preset(w: &TestService, file: &str) {
    let proposal = w
        .board
        .propose_from(
            "SHOP",
            &ProposeFrom::Preset { file: file.into() },
            &models(),
        )
        .await
        .unwrap();
    assert_eq!(
        proposal.source,
        Source::Preset {
            file: file.to_owned()
        }
    );
    assert!(proposal.problems.is_empty(), "{:?}", proposal.problems);
    assert!(!proposal.diff.same, "{file} changes the flow");
    let applied = w
        .board
        .apply_proposal("SHOP", proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert!(applied.waiting.is_none(), "a quiet board applies at once");
    let version = applied.version.expect("a new flow version");
    assert_eq!(version.preset.as_deref(), Some(file));
    assert!(version.current);
    assert_eq!(
        w.board.effective_flow("SHOP").await.unwrap(),
        preset(file).unwrap().read().flow,
        "{file}"
    );
}

/// Polls `ticket` until `done` holds (10 s at most).
async fn until(w: &TestService, key: &str, what: &str, done: impl Fn(&Ticket) -> bool) -> Ticket {
    let start = std::time::Instant::now();
    loop {
        let t = w.board.ticket(key).await.unwrap();
        if done(&t) {
            return t;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{key} never {what}: {:?} in {:?}",
            t.status,
            t.place
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Working in `stage`.
async fn working_in(w: &TestService, key: &str, stage: &str) -> Ticket {
    until(w, key, &format!("worked in {stage}"), |t| {
        t.status == Status::Working && t.place == Place::stage_of(stage)
    })
    .await
}

/// The ticket's running session of `kind` (10 s at most).
async fn session(w: &TestService, t: &Ticket, kind: SessionKind) -> SessionInfo {
    for _ in 0..500 {
        if let Some(s) = w.sessions.running_of(&t.id, kind) {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{} has no {kind:?} session", t.key);
}

/// A scheduler pass, as the service's runs after the request a CLI command sends (these calls go to the core
/// directly, so no request wakes it).
async fn schedule(w: &TestService) {
    Scheduler::new(
        w.board.clone(),
        Arc::new(w.starter.clone()),
        EventBus::new(16),
        Duration::from_secs(3600),
    )
    .pass()
    .await
    .unwrap();
}

/// A new ticket dragged to Ready for Dev and started, as the user's drag starts it.
async fn started(w: &TestService, title: &str) -> Ticket {
    let t = w.create("SHOP", title).await;
    let key = t.key.to_string();
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    w.board.ticket(&key).await.unwrap()
}

fn worktree(t: &Ticket) -> PathBuf {
    PathBuf::from(t.work.as_ref().unwrap().worktree.clone().unwrap())
}

/// The agent's `otaq stage next`.
async fn stage_next(w: &TestService, t: &Ticket, summary: &str) -> Ticket {
    w.validation
        .submit(
            &t.key.to_string(),
            said(&w.board, &t.key.to_string(), summary).await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap()
}

/// Dev's agent (the stand-in, started with `work-ticket`) commits and runs `stage next`; Validate's checker (started
/// with `validate`) passes it. The ticket afterwards.
async fn dev_and_validate(w: &TestService, t: &Ticket, file: &str) -> Ticket {
    let key = t.key.to_string();
    let t = working_in(w, &key, "dev").await;
    let dev = session(w, &t, SessionKind::Agent).await;
    w.wait_for_output(&dev.id, &format!("/{}", naming::copy_name("work-ticket")))
        .await;
    commit(&worktree(&t), file, &format!("{}\n", t.title));
    let checking = stage_next(w, &t, "Done.").await;
    assert_eq!(
        (checking.status, &checking.place),
        (Status::Checking, &Place::stage_of("validate"))
    );
    let checker = session(w, &t, SessionKind::Validator).await;
    w.wait_for_output(&checker.id, &format!("/{}", naming::copy_name("validate")))
        .await;
    w.validation
        .verdict(
            &key,
            true,
            said(&w.board, &key, "ok").await.as_ref(),
            &checker.id,
        )
        .await
        .unwrap()
}

/// The human check `review`'s Pass, right before the click Delivery, offers both deliveries; the click Local merge
/// lands the ticket on `main` and it is Done.
async fn pass_review_and_merge(w: &TestService, repo: &Path, t: &Ticket, review: &str) {
    let key = t.key.to_string();
    let at = w.board.ticket(&key).await.unwrap();
    assert_eq!(
        (at.status, &at.place),
        (Status::WaitingForYou, &Place::stage_of(review))
    );
    let passed = w.review.pass(&key, &w.validation, &me()).await.unwrap();
    assert!(
        matches!(
            passed,
            PassOutcome::Choose {
                delivers: Delivers::Either
            }
        ),
        "{passed:?}"
    );
    let message = format!("{}: {}", t.key, t.title);
    let merged = w
        .review
        .local_merge(&key, &message, false, &me())
        .await
        .unwrap();
    assert!(
        matches!(merged, LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );
    let done = w.board.ticket(&key).await.unwrap();
    assert_eq!((done.status, &done.place), (Status::Merged, &Place::Done));
    assert!(!done.flags.merged_automatically, "the user's click");
    assert_eq!(git(repo, &["log", "-1", "--format=%s", "main"]), message);
}

/// The ticket's runs, oldest first: (stage, result).
async fn runs(w: &TestService, t: &Ticket) -> Vec<(String, Option<RunResult>)> {
    w.board
        .stage_runs(&t.key.to_string())
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.stage, r.result))
        .collect()
}

fn run(stage: &str, result: RunResult) -> (String, Option<RunResult>) {
    (stage.to_owned(), Some(result))
}

/// Spec first (d0129): Spec writer (started with `product-spec`) asks one question, the user's answer is typed into its
/// session, it writes what was agreed into the ticket and runs `stage next`; Architect (`tech-spec`) and Milestone
/// planner (`milestone-plan`) each run in the same worktree, in a new session; Spec review's Pass sends the ticket on
/// to Dev; Validate passes, Code review's Pass offers both, and the click Local merge lands it.
#[tokio::test(flavor = "multi_thread")]
async fn spec_first_from_the_question_to_a_click_local_merge() {
    let (w, repo) = world().await;
    apply_preset(&w, "spec-first.yaml").await;
    let t = started(&w, "Export as CSV").await;
    let key = t.key.to_string();
    let t = working_in(&w, &key, "product-spec").await;
    let spec_writer = session(&w, &t, SessionKind::Agent).await;
    w.wait_for_output(
        &spec_writer.id,
        &format!("/{}", naming::copy_name("product-spec")),
    )
    .await;

    // `otaq ask human`: the question waits for the user; the answer is typed into Spec writer's session.
    let question = "Which columns go in the CSV?";
    w.board
        .store_question(&t.id, question, &[], &[], None)
        .await
        .unwrap();
    let caller = Caller::Agent {
        ticket: t.key.clone(),
        ticket_id: t.id.clone(),
        session: spec_writer.id.clone(),
    };
    w.signals.ask_human(&caller, &t.id, question).await.unwrap();
    assert_eq!(w.status(&key).await, Status::AsksYou);
    w.needs_you
        .answer(&key, &Choice::Other("Title and date".into()), false, &me())
        .await
        .unwrap();
    w.wait_for_output(&spec_writer.id, "Title and date").await;
    assert_eq!(w.status(&key).await, Status::Working);

    // `otaq update` with what was agreed, then `otaq stage next`: the ticket waits before Tech spec.
    let agreed = "Export the notes as CSV: title and date columns, a header row.";
    w.board
        .update_ticket(
            &key,
            &TicketUpdate {
                body: Some(agreed.into()),
                ..Default::default()
            },
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    let waiting = stage_next(&w, &t, "Agreed: title and date.").await;
    assert_eq!(waiting.place, Place::Waiting(Some("tech-spec".into())));
    assert_eq!(waiting.body, agreed);
    schedule(&w).await;

    // Tech spec, then Milestone plan: each starts in the same worktree, in a new conversation, with its own skill;
    // the one before ended. Milestone plan's `stage next` brings the ticket to Spec review.
    let mut before = spec_writer.id.clone();
    for (stage, next) in [
        ("tech-spec", Place::Waiting(Some("milestone-plan".into()))),
        ("milestone-plan", Place::stage_of("spec-review")),
    ] {
        let at = working_in(&w, &key, stage).await;
        assert_eq!(worktree(&at), worktree(&t));
        let planning = session(&w, &at, SessionKind::Agent).await;
        assert_ne!(planning.id, before);
        assert!(!w.sessions.info(&before).unwrap().is_running());
        w.wait_for_output(&planning.id, &format!("/{}", naming::copy_name(stage)))
            .await;
        let after = stage_next(&w, &at, "Written.").await;
        assert_eq!(after.place, next, "{stage}");
        before = planning.id;
        schedule(&w).await;
    }
    let spec_review = w.board.ticket(&key).await.unwrap();
    assert_eq!(spec_review.status, Status::WaitingForYou);
    let passed = w.review.pass(&key, &w.validation, &me()).await.unwrap();
    assert!(matches!(passed, PassOutcome::Moved { .. }), "{passed:?}");
    schedule(&w).await;

    // Dev starts in the same worktree, in a new conversation; Milestone plan's session ended.
    let in_dev = working_in(&w, &key, "dev").await;
    assert_eq!(worktree(&in_dev), worktree(&t));
    let dev = session(&w, &in_dev, SessionKind::Agent).await;
    assert_ne!(dev.id, before);
    assert!(!w.sessions.info(&before).unwrap().is_running());

    dev_and_validate(&w, &in_dev, "export.ts").await;
    pass_review_and_merge(&w, &repo, &in_dev, "code-review").await;
    let walked = runs(&w, &t).await;
    assert_eq!(
        &walked[..4],
        [
            run("product-spec", RunResult::Done),
            run("tech-spec", RunResult::Done),
            run("milestone-plan", RunResult::Done),
            run("spec-review", RunResult::Passed),
        ],
        "{walked:?}"
    );
    assert_eq!(
        &walked[4..6],
        [
            run("dev", RunResult::Done),
            run("validate", RunResult::Passed)
        ],
        "{walked:?}"
    );
    w.stop().await;
}

/// Solo autopilot: Dev, then Validate's pass delivers by itself: a Local merge into `main`, not pushed.
#[tokio::test(flavor = "multi_thread")]
async fn solo_autopilot_to_an_automatic_merge() {
    let (w, repo) = world().await;
    apply_preset(&w, "solo-autopilot.yaml").await;
    let origin_main = git(&repo, &["rev-parse", "origin/main"]);
    let t = started(&w, "Search notes").await;
    let merged = dev_and_validate(&w, &t, "search.ts").await;
    assert_eq!(
        (merged.status, &merged.place),
        (Status::Merged, &Place::Done)
    );
    assert!(merged.flags.merged_automatically);
    assert!(!merged.flags.delivered_without_checks, "Validate passed");
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s", "main"]),
        format!("{}: Search notes", t.key)
    );
    git(&repo, &["fetch", "--quiet", "origin"]);
    assert_eq!(
        git(&repo, &["rev-parse", "origin/main"]),
        origin_main,
        "local, not pushed"
    );
    w.stop().await;
}

/// Makes `repo`'s `origin` read as GitHub (`git@github.com:acme/shop-web.git`, so the built-in Open PR skill is
/// offered) while git still reaches the local bare repository it was cloned from: the repository's `ssh` is a stand-in
/// (written into `tools`) that runs the asked-for git program on that repository.
fn github_origin(repo: &Path, tools: &Path) {
    let bare = git(repo, &["remote", "get-url", "origin"]);
    let ssh = tools.join("ssh.sh");
    // Its last argument is the command, `git-upload-pack 'acme/shop-web.git'`: the program, on the bare repository.
    std::fs::write(
        &ssh,
        format!("for last; do :; done\nexec ${{last%% *}} '{bare}'\n"),
    )
    .unwrap();
    let command = format!("sh '{}'", ssh.display().to_string().replace('\\', "/"));
    git(repo, &["config", "core.sshCommand", &command]);
    git(
        repo,
        &[
            "remote",
            "set-url",
            "origin",
            "git@github.com:acme/shop-web.git",
        ],
    );
}

/// Team PR: Validate's pass starts the Delivery by itself; Shipper isn't Dev's agent, so a fresh delivery session
/// runs `open-pr`, told it is automatic; Otaq confirms the proposal with no dialog, the agent pushes to `origin` and
/// opens the PR with the stand-in `gh`, and the card waits with its PR open.
#[tokio::test(flavor = "multi_thread")]
async fn team_pr_to_a_pr_opened_with_gh() {
    use otaq_core::pr::flow::FlowState;

    let (w, repo) = world().await;
    let tools = tempfile::tempdir().unwrap();
    github_origin(&repo, tools.path());
    apply_preset(&w, "team-pr.yaml").await;
    let t = started(&w, "Login redirect").await;
    let key = t.key.to_string();
    let as_agent = Actor::Agent(t.key.clone());

    let delivering = dev_and_validate(&w, &t, "login.ts").await;
    assert_eq!(
        (delivering.status, &delivering.place),
        (Status::ToDeliver, &Place::stage_of("deliver"))
    );
    assert!(delivering.flags.delivering_automatically);
    let shipper = session(&w, &t, SessionKind::Delivery).await;
    w.wait_for_output(
        &shipper.id,
        &format!("/{} automatic:", naming::copy_name("open-pr")),
    )
    .await;
    let flow = w.board.pr_flow(&t.id).await.unwrap().unwrap();
    assert!(flow.automatic);

    // The proposal is confirmed by Otaq at once: no dialog.
    let branch = naming::branch(&key);
    let head = git(&repo, &["rev-parse", "--short=7", &branch]);
    let proposal = serde_json::json!({
        "branch": "feature/SHOP-1-login",
        "commits": [{"sha": head, "message": "SHOP-1: redirect after login"}],
        "title": "SHOP-1 Login redirect",
        "description": "Returns to the page you came from."
    });
    w.prs
        .propose(&key, &proposal.to_string(), &as_agent)
        .await
        .unwrap();
    let flow = w.board.pr_flow(&t.id).await.unwrap().unwrap();
    assert_eq!(flow.state, FlowState::Pushing, "no dialog");

    // The agent pushes and opens the PR with `gh` (the stand-in), then reports it.
    git(
        &repo,
        &["push", "--quiet", "origin", "feature/SHOP-1-login"],
    );
    let gh = otaq_core::process::testing::stub_command(
        tools.path(),
        "gh",
        "echo https://github.com/acme/shop-web/pull/12",
    );
    let created = std::process::Command::new(&gh)
        .args(["pr", "create", "--head", "feature/SHOP-1-login"])
        .output()
        .unwrap();
    let url = String::from_utf8(created.stdout).unwrap();
    let open = w.prs.opened(&key, url.trim(), &as_agent).await.unwrap();
    assert_eq!(
        (open.status, &open.place),
        (Status::PrOpen, &Place::stage_of("deliver"))
    );
    assert_eq!(open.flags.pr_number, Some(12));
    assert!(!open.flags.delivered_without_checks);
    assert_eq!(
        git(
            &repo.with_file_name("shop-web-origin.git"),
            &["log", "-1", "--format=%s", "feature/SHOP-1-login"]
        ),
        "SHOP-1: redirect after login"
    );
    assert!(
        !w.sessions.info(&shipper.id).unwrap().is_running(),
        "the delivery session ends once the PR is reported"
    );
    w.stop().await;
}

/// The default flow, applied back as Reset to the default flow after another preset: Dev, Validate, Review's Pass
/// and the click Local merge, as today's board.
#[tokio::test(flavor = "multi_thread")]
async fn the_default_flow_to_a_click_local_merge() {
    let (w, repo) = world().await;
    apply_preset(&w, "solo-autopilot.yaml").await;
    apply_preset(&w, naming::DEFAULT_FLOW_FILE).await;
    let t = started(&w, "Checkout page").await;
    dev_and_validate(&w, &t, "checkout.ts").await;
    pass_review_and_merge(&w, &repo, &t, "review").await;
    w.stop().await;
}
