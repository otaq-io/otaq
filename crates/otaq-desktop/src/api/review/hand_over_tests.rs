//! The user's hand-overs from the window, end to end (T-0448; CMT-15, CMT-05, CMT-13; d0109, d0056, d0079): shop-web
//! (SHOP) on the Spec first preset, SHOP-34 "Export orders as CSV" waiting in Code review after Dev ran. **Send back**
//! with a pasted log (over 2,000 characters, so a report) and a note: the note is the user's comment with the log as
//! its report, posted before Dev's conversation resumes, and Dev is told only that a comment came. Then SHOP-34 is
//! **dragged back** from Code review to Dev with a comment: the user's comment, and Dev told "SHOP-34 was sent back to
//! Dev by you with a comment. Run `otaq prime`.".
//!
//! The app talks to a core `TestService` (temporary `OTAQ_HOME` and repositories); `claude` is stood in for by `sh`
//! printing its arguments and sleeping, on a PATH without any `otaq`. No agent tool runs.

use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::ticket_comments::{CommentAuthor, TicketComment};
use otaq_core::board::tickets::{NewTicketInput, Ticket};
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::presets::{SPEC_FIRST_FILE, preset};
use otaq_core::git::testing::commit;
use otaq_core::naming::COMMAND_NAME as CMD;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, path_without_build_folders};
use otaq_core::session::{SessionInfo, SessionKind};
use serde_json::json;

use crate::App;
use crate::api::request;
use crate::service::testing::{app_on, connected};

/// Keeps running, printing its arguments.
const KEEPS_RUNNING: &str = "printf 'ARGS %s\\n' \"$*\"; exec sleep 300";

const SEND_BACK_NOTE: &str = "The 10,000 row limit fails with this log: look for row_limit.";
const DRAG_NOTE: &str = "Use the orders table's column names in the header row.";

fn me() -> Actor {
    Actor::User("me".into())
}

/// shop-web on Spec first, with the stand-in `claude`; SHOP-34 in Code review, its Dev conversation `SHOP-34-dev`
/// ended.
async fn world() -> (TestService, Ticket) {
    let w = TestService::start_with(|config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: path_without_build_folders().into(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), KEEPS_RUNNING.into(), "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    w.add_project("shop-web").await;
    w.board
        .use_flow_for_tests("SHOP", &preset(SPEC_FIRST_FILE).unwrap().read())
        .await;
    for _ in 1..34 {
        w.create("SHOP", "filler").await;
    }
    let t = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Export orders as CSV".into(),
                body: "Admins need the orders as a CSV file.".into(),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(t.key.to_string(), "SHOP-34");
    let wt = w.start_worktree(&t).await;
    commit(&wt, "export.rs", "pub fn export_csv() {}\n");
    w.force_stage(&t, "dev", Status::Working).await;
    w.record_ended_agent(&t, "SHOP-34-dev").await;
    w.force_stage(&t, "code-review", Status::WaitingForYou).await;
    (w, t)
}

async fn app(w: &TestService) -> App {
    let app = app_on(w.data.clone());
    tokio::time::timeout(Duration::from_secs(10), async {
        while !connected(&app.service.connection()) {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the app connects");
    app
}

/// The ticket's running agent session once it works in Dev again (20 s at most).
async fn working_in_dev(w: &TestService, t: &Ticket) -> SessionInfo {
    let key = t.key.to_string();
    let found = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            let now = w.board.ticket(&key).await.unwrap();
            if now.place == Place::stage_of("dev")
                && now.status == Status::Working
                && let Some(s) = w.sessions.running_of(&t.id, SessionKind::Agent)
            {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    found.unwrap_or_else(|_| panic!("{key} never went back to Dev"))
}

async fn comments(w: &TestService) -> Vec<TicketComment> {
    w.board.ticket_comments("SHOP-34").await.unwrap().comments
}

#[tokio::test(flavor = "multi_thread")]
async fn send_back_with_a_pasted_log_and_a_drag_back_post_the_users_comments() {
    let (w, t) = world().await;
    let app = app(&w).await;

    // Send back: the note and the pasted log (over the limit, so a report).
    let log: String = (1..=120)
        .map(|i| format!("test export::case_{i:04} ... ok\n"))
        .chain(["test export_tests::row_limit_10000 ... FAILED\n".to_owned()])
        .collect();
    assert!(log.chars().count() > 2_000);
    let sent = request(
        &app,
        "review.send_back",
        json!({ "key": "SHOP-34", "note": SEND_BACK_NOTE, "report": log }),
    )
    .await
    .unwrap();
    assert_eq!(sent, json!({ "outcome": "done" }));
    let dev = working_in_dev(&w, &t).await;
    let out = w.wait_for_output(&dev.id, "ARGS").await;
    assert!(out.contains("--resume SHOP-34-dev"), "{out}");
    assert!(
        out.contains(&format!(
            "Code review sent SHOP-34 back to you with a comment. Run `{CMD} prime`."
        )),
        "{out}"
    );
    assert!(!out.contains("row_limit") && !out.contains("10,000"), "{out}");

    let posted = comments(&w).await;
    assert_eq!(posted.len(), 1);
    let note = &posted[0];
    assert!(
        matches!(note.author, CommentAuthor::User { .. }),
        "{:?}",
        note.author
    );
    assert_eq!(note.body, SEND_BACK_NOTE);
    assert!(
        note.posted_at <= dev.started_at,
        "posted before Dev resumed"
    );
    let info = note.report.expect("the pasted log is its report");
    assert_eq!(info.bytes, log.len() as u64);
    assert_eq!(info.lines, 121);
    let report = w.board.ticket_report("SHOP-34", note.n).await.unwrap();
    assert_eq!(std::fs::read_to_string(&report.path).unwrap(), log);

    // Dev's conversation ends; SHOP-34 comes back to Code review and is dragged back to Dev with a comment.
    w.sessions.stop(&dev.id).await.unwrap();
    w.force_stage(&t, "code-review", Status::WaitingForYou).await;
    let sent = request(
        &app,
        "board.drag_back",
        json!({ "key": "SHOP-34", "target": "dev", "note": DRAG_NOTE }),
    )
    .await
    .unwrap();
    assert_eq!(sent, json!({ "outcome": "done" }));
    let dev = working_in_dev(&w, &t).await;
    let out = w
        .wait_for_output(
            &dev.id,
            &format!("SHOP-34 was sent back to Dev by you with a comment. Run `{CMD} prime`."),
        )
        .await;
    assert!(out.contains("--resume SHOP-34-dev"), "{out}");
    assert!(!out.contains("header row"), "{out}");

    let posted = comments(&w).await;
    assert_eq!(posted.len(), 2);
    let drag = &posted[1];
    assert!(matches!(drag.author, CommentAuthor::User { .. }));
    assert_eq!((drag.n, drag.body.as_str()), (2, DRAG_NOTE));
    assert!(drag.report.is_none());
    assert!(drag.posted_at <= dev.started_at, "posted before Dev resumed");
    w.stop().await;
}
