//! M31's end-to-end path through `stage next` and `check pass|fail` (T-0409; AGM-10, AGM-12, AGM-06; d0108, d0063):
//! start, a failed check, Resume, Send back and Local merge on the default flow, with a stand-in `claude` that runs
//! the real `otaq`. As the ticket's agent it commits and ends its stage with `otaq stage next`; as the check it fails
//! once, then passes, with `otaq check fail|pass`. No agent tool runs; every folder is temporary (its `HOME` too).

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::moves::DropTarget;
use otaq_core::domain::{Actor, Status};
use otaq_core::flow::engine::RunResult;
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

fn me() -> Actor {
    Actor::User("me".into())
}

/// The stand-in `claude`, keeping its state in `state` (and Otaq's data folder in `state/data`, so its `otaq` reaches
/// the test's service, never the user's): it logs its arguments, then by the skill it was started
/// with (its last argument names it, or `--resume` resumes the agent's conversation):
/// - the check (`/otaq-validate`): reads `prime --validate`, fails the first round and passes the others, then ends
///   its turn (the Stop hook);
/// - the agent's first start: reads `prime`, commits, `stage next`; once the check has failed it exits (the ticket is
///   Interrupted);
/// - a resumed agent: the same, then works again on each Send back typed into its terminal (with no line comments
///   the plain sentence, "Review sent SHOP-1 back to you…", d0131 Q514; else `/otaq-address-review`), saving the
///   prime it read each time.
fn stand_in(state: &Path) -> String {
    let state = state.display();
    let data_var = otaq_core::data_dir::DataDir::env_var();
    format!(
        r#"
export HOME='{state}/home'
STATE='{state}'
export {data_var}="$(cat "$STATE/data")"
printf 'ARGS %s\n' "$*"
work() {{
    '{CB}' prime > "$STATE/prime-$1.md"
    echo "$1" >> notes.txt
    git add -A
    git -c user.name=t -c user.email=t@t commit -qm "$OTAQ_TICKET: $1"
    printf 'Wrote %s.\n' "$1" | '{CB}' stage next --stdin
}}
stop() {{
    printf '%s' '{{"hook_event_name":"Stop","background_tasks":[],"session_crons":[]}}' | '{CB}' hook --tool claude Stop
}}
case "$*" in
*otaq-validate*)
    '{CB}' prime --validate > "$STATE/check-prime.md"
    if [ -e "$STATE/failed" ]; then
        printf 'Checked: notes.txt is fine.\n' | '{CB}' check pass --stdin
    else
        printf 'notes.txt:1: too short\n' | '{CB}' check fail --stdin
        touch "$STATE/failed"
    fi
    stop
    exec sleep 60
    ;;
*--resume*)
    work fix
    while IFS= read -r line; do
        case "$line" in *otaq-address-review* | *"back to you"*) work review ;; esac
    done
    exec sleep 60
    ;;
*otaq-work-ticket*)
    work first
    while [ ! -e "$STATE/failed" ]; do sleep 0.1; done
    sleep 1
    exit 3
    ;;
esac
exec sleep 60
"#
    )
}

async fn until(w: &TestService, key: &str, what: &str, done: impl AsyncFn(&TestService) -> bool) {
    let reached = tokio::time::timeout(Duration::from_secs(60), async {
        while !done(w).await {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    if reached.is_err() {
        let t = w.board.ticket(key).await.unwrap();
        let runs = w.board.stage_runs(key).await.unwrap();
        let terminals: Vec<String> = [
            otaq_core::session::SessionKind::Agent,
            otaq_core::session::SessionKind::Validator,
        ]
        .into_iter()
        .filter_map(|kind| w.sessions.running_of(&t.id, kind))
        .map(|s| {
            let attached = w.sessions.attach(&s.id).expect("session in memory");
            String::from_utf8_lossy(&attached.scrollback).into_owned()
        })
        .collect();
        panic!(
            "{key} never {what}: {} {:?}\n{runs:#?}\n{}",
            t.status,
            t.start_problem,
            terminals.join("\n----\n")
        );
    }
}

/// The ticket's runs as (stage, result).
async fn runs(w: &TestService, key: &str) -> Vec<(String, Option<RunResult>)> {
    w.board
        .stage_runs(key)
        .await
        .unwrap()
        .into_iter()
        .map(|r| (r.stage, r.result))
        .collect()
}

async fn done_devs(w: &TestService, key: &str) -> usize {
    runs(w, key)
        .await
        .iter()
        .filter(|(stage, result)| stage == "dev" && *result == Some(RunResult::Done))
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn start_check_fail_resume_send_back_and_local_merge() {
    let state = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(state.path().join("home")).unwrap();
    let script = stand_in(state.path());
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    std::fs::write(
        state.path().join("data"),
        w.data.root().display().to_string(),
    )
    .unwrap();
    let project = w.add_project("shop-web").await;
    let t = w.create("SHOP", "Sort notes by date").await;
    let key = t.key.to_string();

    // Start: Dev runs work-ticket, ends with `stage next`; the check fails round 1 and the agent goes back to Dev;
    // its process ends, so the ticket is Interrupted in Dev.
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until(&w, &key, "was interrupted", async |w| {
        w.status(&key).await == Status::Interrupted
    })
    .await;
    let first = std::fs::read_to_string(state.path().join("prime-first.md")).unwrap();
    assert!(
        first.contains(
            "\n## Your stage\n\nDev, as Developer, with the skill `/otaq-work-ticket`.\n"
        ),
        "{first}"
    );
    assert!(first.contains("`otaq stage next --stdin`"), "{first}");
    let check = std::fs::read_to_string(state.path().join("check-prime.md")).unwrap();
    // One line per stage (CMT-07); what Dev said reaches the check as its comment (T-0443's hand-over).
    assert!(
        check.contains("\n## Earlier stages\n\n- Dev · Developer · finished\n"),
        "the check reads Dev's run: {check}"
    );
    assert!(
        check.contains("\n## Comments\n") && check.contains("\n\nWrote first.\n"),
        "the check reads Dev's comment: {check}"
    );
    assert_eq!(
        runs(&w, &key).await[..2],
        [
            ("dev".to_owned(), Some(RunResult::Done)),
            ("validate".to_owned(), Some(RunResult::Failed)),
        ]
    );

    // Resume: the same conversation goes on; it commits, `stage next`, the check passes → Review.
    let conversation = w
        .sessions
        .history(&otaq_core::session::SessionTicket {
            id: t.id.clone(),
            key: t.key.clone(),
        })
        .await
        .unwrap()
        .iter()
        .filter(|s| s.kind == otaq_core::session::SessionKind::Agent)
        .find_map(|s| s.tool_session_id.clone())
        .unwrap();
    w.review.resume(&key, &me()).await.unwrap();
    until(&w, &key, "reached Review after Resume", async |w| {
        w.status(&key).await == Status::WaitingForYou && done_devs(w, &key).await == 2
    })
    .await;
    let agent = w
        .sessions
        .running_of(&t.id, otaq_core::session::SessionKind::Agent)
        .unwrap();
    let out = w.wait_for_output(&agent.id, "ARGS").await;
    assert!(out.contains(&format!("--resume {conversation}")), "{out}");

    // Send back: the live session gets the plain sentence (only a note), ends its stage again; the check passes →
    // Review.
    w.review
        .fail(&key, "Also the tax", None, false, &me())
        .await
        .unwrap();
    until(&w, &key, "reached Review after Send back", async |w| {
        w.status(&key).await == Status::WaitingForYou && done_devs(w, &key).await == 3
    })
    .await;
    let review = std::fs::read_to_string(state.path().join("prime-review.md")).unwrap();
    assert!(
        review.contains("- Review · you · failed\n"),
        "Dev reads the Send back's run under Earlier stages: {review}"
    );
    assert!(
        review.contains("\n\nAlso the tax\n"),
        "Dev reads the Send back's note as your comment: {review}"
    );
    let results: Vec<(String, Option<RunResult>)> = runs(&w, &key)
        .await
        .into_iter()
        .filter(|(_, r)| *r != Some(RunResult::Cancelled))
        .collect();
    let ran = |stage: &str, result| (stage.to_owned(), Some(result));
    assert_eq!(
        results,
        [
            ran("dev", RunResult::Done),
            ran("validate", RunResult::Failed),
            ran("dev", RunResult::Done),
            ran("validate", RunResult::Passed),
            ran("review", RunResult::Failed),
            ran("dev", RunResult::Done),
            ran("validate", RunResult::Passed),
            ("review".to_owned(), None),
        ]
    );

    // Local merge: one squash commit on main.
    let outcome = w
        .review
        .local_merge(&key, &format!("{key}: Sort notes by date"), false, &me())
        .await
        .unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    assert_eq!(w.status(&key).await, Status::Merged);
    let log = std::process::Command::new("git")
        .args(["log", "-1", "--format=%s", "main"])
        .current_dir(&project.repo_path)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&log.stdout).trim(),
        format!("{key}: Sort notes by date")
    );
    let notes = std::process::Command::new("git")
        .args(["show", "main:notes.txt"])
        .current_dir(&project.repo_path)
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&notes.stdout),
        "first\nfix\nreview\n"
    );
    w.stop().await;
}
