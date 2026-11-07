//! Free at Review entry (T-0308; M19-c, d0086) against a running service in a temporary data folder and temporary
//! repositories. The "build" is fake: the test writes gitignored files and folders into the worktree. The probe is
//! forced, so builds are cloned (a disk that can clone) or renamed (one that can't, as on exFAT) whatever the test
//! disk can do; the Free only renames either way.
//!
//! The service frees on the events a request publishes (`ticket.submit_by_user` here). Tests that call the
//! services directly announce the status themselves, as a request would ([`announce`]).

use otaq_core::service::testing::said;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::settings::ProjectSettings;
use otaq_core::board::tickets::Ticket;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::{Actor, BuildState, Status, ValidationAction, ValidationTrigger};
use otaq_core::git::testing::{commit, git};
use otaq_core::protocol::{Event, Reply, Request, SubmitByUserParams, ValidationActionParams};
use otaq_core::recovery::cleanup::CleanupGroup;
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::store::projects::Project;
use otaq_core::ticket_env::free::FreeOutcome;
use otaq_core::ticket_env::handoff::HandoffMode;
use otaq_core::warm::clone::{CloneProbe, CloneSupport};
use otaq_core::warm::store::WarmRecord;

fn me() -> Actor {
    Actor::User("me".into())
}

/// Both disks: one that can clone (`true`), one that can't (`false`).
const BOTH: [bool; 2] = [true, false];

/// A service on a disk that can clone (`available`) or can't (exFAT); its validators and agents are `sh` sleeping.
async fn world(available: bool) -> TestService {
    let w = TestService::start_with(|config| ServiceConfig {
        clone_probe: Arc::new(CloneProbe::forced(CloneSupport {
            available,
            file_system: if available { "apfs" } else { "exfat" }.into(),
            volume: "/Volumes/Test".into(),
        })),
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
    .await;
    let mode = if available {
        HandoffMode::Clone
    } else {
        HandoffMode::Move
    };
    assert_eq!(w.warm.mode(), mode);
    w
}

/// shop-web with a committed `.gitignore` and its env files listed in its settings; `change` adjusts them.
async fn shop_web_with(w: &TestService, change: impl FnOnce(&mut ProjectSettings)) -> Project {
    let project = w.add_project("shop-web").await;
    let repo = Path::new(&project.repo_path);
    commit(
        repo,
        ".gitignore",
        "out/\n*.log\n.env.local\nconfig/\nlocked/\n",
    );
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.env_files = vec![".env.local".into(), "config/.env".into()];
    change(&mut settings);
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    // [`submit`]'s Skip this check: the flow marks Validate skippable.
    w.board
        .edit_flow_for_tests(
            "SHOP",
            otaq_core::flow::default::testing::skippable_validate,
        )
        .await;
    project
}

async fn shop_web(w: &TestService) -> Project {
    shop_web_with(w, |_| {}).await
}

/// A started ticket with one commit of its own and a build, waiting for the user (so Move to next stage works).
async fn started(w: &TestService, title: &str, file: &str) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", title).await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, file, "code\n");
    build(&wt, "v1");
    w.force_status(&t, Status::NeedsYou).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), wt)
}

/// What a build leaves: gitignored files and folders, and the env files copied in at start.
fn build(wt: &Path, content: &str) {
    std::fs::create_dir_all(wt.join("out").join("deep")).unwrap();
    std::fs::write(wt.join("out").join("deep").join("app.bin"), content).unwrap();
    std::fs::write(wt.join("build.log"), "built\n").unwrap();
    std::fs::write(wt.join(".env.local"), "API=1\n").unwrap();
    std::fs::create_dir_all(wt.join("config")).unwrap();
    std::fs::write(wt.join("config").join(".env"), "DB=1\n").unwrap();
    std::fs::write(wt.join("config").join("cache.json"), "{}\n").unwrap();
}

async fn client(w: &TestService) -> Client {
    Client::connect(ClientConfig::new(&w.data)).await.unwrap()
}

/// The user's **Move to next stage**, then **Skip this check**, through the service as the app sends them: since
/// T-0406 Move to next stage runs the Validate check first (d0108 rule 10), so Skip brings the ticket to Review.
async fn submit(w: &TestService, t: &Ticket) {
    let client = client(w).await;
    let reply = client
        .request(Request::TicketSubmitByUser(SubmitByUserParams {
            key: t.key.to_string(),
            note: String::new(),
        }))
        .await
        .unwrap();
    match reply {
        Reply::Ticket(after) => assert_eq!(after.status, Status::Checking),
        other => panic!("{other:?}"),
    }
    let reply = client
        .request(Request::TicketValidationAction(ValidationActionParams {
            key: t.key.to_string(),
            action: ValidationAction::Skip,
            note: String::new(),
        }))
        .await
        .unwrap();
    match reply {
        Reply::Ticket(after) => assert_eq!(after.status, Status::WaitingForYou),
        other => panic!("{other:?}"),
    }
}

/// Publishes the ticket's status as a request that changed it would.
async fn announce(w: &TestService, key: &str) {
    let t = w.board.ticket(key).await.unwrap();
    w.events.publish(Event::CardStatusChanged {
        project: t.key.project.to_string(),
        id: t.id.clone(),
        key: t.key.clone(),
        status: t.status,
    });
}

async fn build_state(w: &TestService, key: &str) -> (BuildState, Option<String>) {
    let work = w.board.ticket(key).await.unwrap().work.unwrap_or_default();
    (work.build_state, work.build_problem)
}

async fn until_freed(w: &TestService, key: &str) {
    for _ in 0..400 {
        if build_state(w, key).await.0 == BuildState::Freed {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "{key}'s build was never freed: {:?}",
        build_state(w, key).await
    );
}

/// Waits a little for a Free that must not come, then checks it didn't.
async fn stays_present(w: &TestService, key: &str) {
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(build_state(w, key).await.0, BuildState::Present, "{key}");
}

/// The project's stock entries, by number.
fn entries(w: &TestService, project: &Project) -> Vec<u64> {
    let parent = w.warm.path(project).unwrap();
    let mut numbers: Vec<u64> = std::fs::read_dir(&parent)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_string_lossy().parse().ok())
        .collect();
    numbers.sort();
    numbers
}

fn entry(w: &TestService, project: &Project, n: u64) -> PathBuf {
    w.warm.path(project).unwrap().join(n.to_string())
}

fn porcelain(wt: &Path) -> String {
    git(wt, &["status", "--porcelain"])
}

async fn merge(w: &TestService, key: &str) {
    let outcome = w.review.local_merge(key, "m", false, &me()).await.unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
}

/// The M19-c acceptance: Submit (validation off) frees the build: `git status` is clean, the env files are in
/// place, `out/` is in the stock; the worktree, branch, ports and the agent's session are untouched.
#[tokio::test(flavor = "multi_thread")]
async fn review_entry_frees_the_build_and_the_worktree_stays_clean() {
    for available in BOTH {
        let w = world(available).await;
        let project = shop_web(&w).await;
        let (t, wt) = started(&w, "Checkout API", "api.ts").await;
        let key = t.key.to_string();
        let agent = w.agent_shell(&t).await;
        let before = t.work.clone().unwrap();

        submit(&w, &t).await;
        until_freed(&w, &key).await;

        assert_eq!(porcelain(&wt), "", "the worktree stays clean");
        assert_eq!(
            std::fs::read_to_string(wt.join(".env.local")).unwrap(),
            "API=1\n"
        );
        assert_eq!(
            std::fs::read_to_string(wt.join("config").join(".env")).unwrap(),
            "DB=1\n",
            "an env file inside a freed folder stays"
        );
        assert!(!wt.join("out").exists(), "out/ is gone");
        assert!(!wt.join("build.log").exists());
        assert!(!wt.join("config").join("cache.json").exists());
        assert!(wt.join("api.ts").is_file(), "tracked files stay");

        assert_eq!(entries(&w, &project), [0], "one entry in the stock");
        let freed = entry(&w, &project, 0);
        assert_eq!(
            std::fs::read_to_string(freed.join("out/deep/app.bin")).unwrap(),
            "v1"
        );
        assert!(freed.join("config/cache.json").is_file());
        assert!(!freed.join("config/.env").exists(), "env files aren't kept");
        assert!(!freed.join(".env.local").exists());
        assert_eq!(WarmRecord::read(&freed).unwrap().from, key);

        let after = w.board.ticket(&key).await.unwrap();
        assert_eq!(after.status, Status::WaitingForYou);
        let work = after.work.unwrap();
        assert_eq!(
            (
                &work.worktree,
                &work.branch,
                work.agent_port,
                work.app_port("app")
            ),
            (
                &before.worktree,
                &before.branch,
                before.agent_port,
                before.app_port("app")
            )
        );
        assert_eq!(work.build_problem, None);
        let session = w
            .sessions
            .running_of(&t.id, SessionKind::Agent)
            .expect("the agent's session still runs");
        assert_eq!(session.id, agent.session);
        let branch = otaq_core::naming::branch(&key);
        assert_eq!(
            git(&wt, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
            branch,
            "the worktree is still on its branch"
        );
        // Freed is freed: a second Free does nothing.
        assert_eq!(w.ticket_env.free(&t.key).await, FreeOutcome::NotNeeded);
        w.stop().await;
    }
}

/// A ticket that entered Review without an event (a path that doesn't publish) is freed by the sweep; Dev isn't.
#[tokio::test]
async fn the_sweep_frees_what_no_event_announced() {
    let w = world(false).await;
    let project = shop_web(&w).await;
    let (review, _) = started(&w, "In review", "a.ts").await;
    let (working, working_wt) = started(&w, "Working", "b.ts").await;
    w.force_status(&review, Status::WaitingForYou).await;
    w.force_status(&working, Status::Working).await;

    w.ticket_env.sweep().await;

    assert_eq!(
        build_state(&w, &review.key.to_string()).await.0,
        BuildState::Freed
    );
    assert_eq!(
        build_state(&w, &working.key.to_string()).await.0,
        BuildState::Present
    );
    assert!(working_wt.join("out/deep/app.bin").is_file());
    assert_eq!(entries(&w, &project), [0]);
    w.stop().await;
}

/// With validation on, the build stays while the ticket validates (the validator works in it) and is freed after
/// the pass.
#[tokio::test(flavor = "multi_thread")]
async fn with_validation_on_the_build_is_freed_only_after_the_pass() {
    let w = world(false).await;
    let project = shop_web(&w).await;
    let (t, wt) = started(&w, "Cart totals", "cart.ts").await;
    let key = t.key.to_string();
    w.force_status(&t, Status::Working).await;

    let validating = w
        .validation
        .submit(
            &key,
            said(&w.board, &key, "Cart totals add up.").await.as_ref(),
            &Actor::Agent(t.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap();
    assert_eq!(validating.status, Status::Checking);
    announce(&w, &key).await;
    w.ticket_env.sweep().await;
    stays_present(&w, &key).await;
    assert!(wt.join("out/deep/app.bin").is_file());

    let validator = w
        .sessions
        .running_of(&t.id, SessionKind::Validator)
        .expect("a validator runs")
        .id;
    let passed = w
        .validation
        .verdict(
            &key,
            true,
            said(&w.board, &key, "ok").await.as_ref(),
            &validator,
        )
        .await
        .unwrap();
    assert_eq!(passed.status, Status::WaitingForYou);
    announce(&w, &key).await;
    until_freed(&w, &key).await;
    assert!(!wt.join("out").exists());
    assert_eq!(entries(&w, &project), [0]);
    w.stop().await;
}

/// An open Run holds the Free (Otaq never stops the user's process); when it ends the build is freed.
#[tokio::test(flavor = "multi_thread")]
async fn an_open_run_holds_the_free_until_it_closes() {
    let w = world(false).await;
    let project = shop_web(&w).await;
    let (t, wt) = started(&w, "Checkout page", "page.ts").await;
    let key = t.key.to_string();
    let run = w.shell(&t, SessionKind::Run).await;

    submit(&w, &t).await;
    stays_present(&w, &key).await;
    assert_eq!(w.ticket_env.free(&t.key).await, FreeOutcome::Waiting);
    assert!(
        wt.join("out/deep/app.bin").is_file(),
        "the Run keeps its build"
    );
    assert_eq!(build_state(&w, &key).await, (BuildState::Present, None));

    w.sessions.stop(&run.session).await.unwrap();
    until_freed(&w, &key).await;
    assert!(!wt.join("out").exists());
    assert_eq!(entries(&w, &project), [0]);
    w.stop().await;
}

/// A Free that fails (a file of the build held open on Windows; a worktree that can't be renamed out of elsewhere)
/// keeps the build in place, records why, shows in Cleanup's "could not free" group and never changes the status.
/// The user's retry from Cleanup frees it.
#[tokio::test(flavor = "multi_thread")]
async fn a_free_that_fails_keeps_the_build_and_shows_in_cleanup() {
    let w = world(false).await;
    let project = shop_web(&w).await;
    let (t, wt) = started(&w, "Locked build", "lock.ts").await;
    let key = t.key.to_string();
    w.force_status(&t, Status::WaitingForYou).await;
    let held = hold(&wt);

    let outcome = w.ticket_env.free(&t.key).await;
    assert!(matches!(outcome, FreeOutcome::Failed(_)), "{outcome:?}");
    let (state, problem) = build_state(&w, &key).await;
    assert_eq!(state, BuildState::Present);
    assert!(problem.is_some());
    assert_eq!(
        w.status(&key).await,
        Status::WaitingForYou,
        "Review isn't broken"
    );
    // Everything is back where it was.
    assert!(wt.join("out/deep/app.bin").is_file());
    assert!(wt.join("build.log").is_file());
    assert!(wt.join("config/cache.json").is_file());
    assert!(entries(&w, &project).is_empty());
    // The sweep leaves a failed one to the user.
    w.ticket_env.sweep().await;
    assert_eq!(build_state(&w, &key).await.0, BuildState::Present);

    let items = w.cleanup.scan().await.unwrap();
    let item = items
        .iter()
        .find(|i| i.group == CleanupGroup::CouldNotFree)
        .expect("a could-not-free item");
    assert_eq!(item.id, t.id);
    assert_eq!(item.ticket.as_ref(), Some(&t.key));
    assert_eq!(Some(&item.why), problem.as_ref());
    assert_eq!(
        item.path.as_deref(),
        Some(wt.display().to_string().as_str())
    );

    release(held);
    let done = w.cleanup.delete(&[item.target()], true).await.unwrap();
    assert_eq!(done.deleted, [item.target()], "{:?}", done.failed);
    assert_eq!(build_state(&w, &key).await, (BuildState::Freed, None));
    assert!(!wt.join("out").exists());
    assert_eq!(entries(&w, &project), [0]);
    assert!(
        !w.cleanup
            .scan()
            .await
            .unwrap()
            .iter()
            .any(|i| i.group == CleanupGroup::CouldNotFree)
    );
    w.stop().await;
}

/// Windows: a file of the build opened as an editor does (others may read it, not rename it).
#[cfg(windows)]
fn hold(wt: &Path) -> std::fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1 /* FILE_SHARE_READ */)
        .open(wt.join("out/deep/app.bin"))
        .unwrap()
}

#[cfg(windows)]
fn release(held: std::fs::File) {
    drop(held);
}

/// Elsewhere: the worktree folder made read-only, so nothing can be renamed out of it.
#[cfg(unix)]
fn hold(wt: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(wt, std::fs::Permissions::from_mode(0o555)).unwrap();
    wt.to_owned()
}

#[cfg(unix)]
fn release(wt: PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&wt, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// Approved (a pull request open) stays freed: nothing comes back.
#[tokio::test(flavor = "multi_thread")]
async fn approved_tickets_stay_freed() {
    let w = world(false).await;
    let project = shop_web(&w).await;
    let (t, wt) = started(&w, "Add sitemap", "sitemap.ts").await;
    let key = t.key.to_string();
    submit(&w, &t).await;
    until_freed(&w, &key).await;

    w.force_status(&t, Status::PrOpen).await;
    announce(&w, &key).await;
    assert_eq!(w.ticket_env.free(&t.key).await, FreeOutcome::NotNeeded);
    stays_freed(&w, &key).await;
    assert!(!wt.join("out").exists());
    assert_eq!(entries(&w, &project), [0]);
    w.stop().await;
}

async fn stays_freed(w: &TestService, key: &str) {
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(build_state(w, key).await.0, BuildState::Freed, "{key}");
}

/// Done after a Free keeps nothing more (the build is in the stock already), removes the worktree and resets the
/// build state, so a reopened ticket is never taken for freed.
#[tokio::test(flavor = "multi_thread")]
async fn done_after_a_free_moves_nothing_and_resets_the_state() {
    for available in BOTH {
        let w = world(available).await;
        let project = shop_web(&w).await;
        let (t, wt) = started(&w, "Checkout API", "api.ts").await;
        let key = t.key.to_string();
        submit(&w, &t).await;
        until_freed(&w, &key).await;
        let kept_at = WarmRecord::read(&entry(&w, &project, 0)).unwrap().kept_at;

        merge(&w, &key).await;

        assert!(!wt.exists(), "worktree removed");
        assert_eq!(entries(&w, &project), [0], "nothing more is kept");
        assert_eq!(
            WarmRecord::read(&entry(&w, &project, 0)).unwrap().kept_at,
            kept_at,
            "the freed build is the one in stock"
        );
        let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
        assert_eq!(row.build_state, "present");
        assert_eq!(row.build_problem, None);
        assert_eq!(row.worktree_path, None);
        w.stop().await;
    }
}

/// A pass with Auto-merge merges at once, so the Free and the Release race: whichever comes second sees what the
/// first did, and the build is moved once.
#[tokio::test(flavor = "multi_thread")]
async fn free_and_auto_merge_never_both_move_the_build() {
    let w = world(false).await;
    let project = shop_web_with(&w, |_| {}).await;
    w.board
        .edit_flow_for_tests(
            "SHOP",
            otaq_core::flow::default::testing::automatic_local_delivery,
        )
        .await;
    let (t, wt) = started(&w, "Footer links", "footer.ts").await;
    let key = t.key.to_string();
    w.force_status(&t, Status::Working).await;
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
    // Frees keep trying while the verdict merges.
    let freeing = {
        let (env, key) = (w.ticket_env.clone(), t.key.clone());
        tokio::spawn(async move {
            let mut outcomes = Vec::new();
            for _ in 0..40 {
                outcomes.push(env.free(&key).await);
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            outcomes
        })
    };
    let merged = w
        .validation
        .verdict(
            &key,
            true,
            said(&w.board, &key, "ok").await.as_ref(),
            &validator,
        )
        .await
        .unwrap();
    assert_eq!(merged.status, Status::Merged);
    let outcomes = freeing.await.unwrap();
    assert!(
        outcomes
            .iter()
            .filter(|o| matches!(o, FreeOutcome::Freed { .. }))
            .count()
            <= 1,
        "{outcomes:?}"
    );
    assert!(
        outcomes
            .iter()
            .all(|o| matches!(o, FreeOutcome::Freed { .. } | FreeOutcome::NotNeeded)),
        "{outcomes:?}"
    );

    assert!(!wt.exists(), "worktree removed");
    assert_eq!(entries(&w, &project), [0], "the build is kept once");
    assert_eq!(
        std::fs::read_to_string(entry(&w, &project, 0).join("out/deep/app.bin")).unwrap(),
        "v1"
    );
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.build_state, "present");
    w.stop().await;
}

/// Warm builds off: the Free deletes the build (d0086) and still records it freed; the env files stay.
#[tokio::test(flavor = "multi_thread")]
async fn switch_off_frees_by_deleting() {
    for available in BOTH {
        let w = world(available).await;
        let project = shop_web_with(&w, |s| s.warm_builds = false).await;
        let (t, wt) = started(&w, "Checkout API", "api.ts").await;
        let key = t.key.to_string();

        submit(&w, &t).await;
        until_freed(&w, &key).await;

        assert!(!wt.join("out").exists());
        assert!(!wt.join("build.log").exists());
        assert!(wt.join(".env.local").is_file());
        assert!(wt.join("config/.env").is_file());
        assert_eq!(porcelain(&wt), "");
        assert!(entries(&w, &project).is_empty(), "nothing in the stock");
        w.stop().await;
    }
}
