//! How a stage ends (T-0406; AGM-10, STG-10; d0108, d0063, d0031): History records "Finished <stage>" for an Agent
//! stage that ends with `stage next` into anything but a human check, and "Submitted for review" before a human check
//! (Q263), on the test flow (`tests/flows/every-kind.yaml`) and the default flow. Who may end a stage: a check
//! session never `stage next` / `stage done`, an Agent stage's session never a verdict, and the refusals name the
//! default flow's stages too (Q269). No agent tool runs; every data folder is temporary.

use otaq_core::service::testing::said;
use std::path::Path;

use otaq_core::board::tickets::{EventKind, HistoryEvent, Ticket};
use otaq_core::domain::{Actor, Place, Status, ValidationTrigger};
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::service::testing::{AgentShell, TestService};
use serde_json::{Value, json};

fn every_kind_file() -> FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

async fn into_stage(w: &TestService, ticket: &Ticket, stage: &str) {
    sqlx::query("UPDATE tickets SET stage_id = ? WHERE id = ?")
        .bind(stage)
        .bind(&ticket.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
}

/// SHOP on the default flow (`on`), or on it without Validate.
async fn validation(w: &TestService, on: bool) {
    w.board
        .edit_flow_for_tests("SHOP", |file| {
            if on {
                *file = otaq_core::flow::default_file().clone();
            } else {
                otaq_core::flow::default::testing::without_validate(file);
            }
        })
        .await;
}

/// The ticket's stage ends, newest first: (kind, the stage it names, the summary).
async fn stage_ends(w: &TestService, key: &str) -> Vec<(EventKind, Option<String>, String)> {
    let text = |e: &HistoryEvent, field: &str| e.payload[field].as_str().map(str::to_owned);
    w.board
        .history(key)
        .await
        .unwrap()
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::ReviewSubmitted | EventKind::StageFinished
            )
        })
        .map(|e| {
            (
                e.kind,
                text(e, "stage"),
                text(e, "summary").unwrap_or_default(),
            )
        })
        .collect()
}

async fn submit(w: &TestService, ticket: &Ticket, summary: &str) -> Ticket {
    w.validation
        .submit(
            &ticket.key.to_string(),
            said(&w.board, &ticket.key.to_string(), summary)
                .await
                .as_ref(),
            &Actor::Agent(ticket.key.clone()),
            ValidationTrigger::Agent,
        )
        .await
        .unwrap()
}

/// The test flow: finishing Product spec waits before Dev and records "Finished Product spec"; finishing Dev starts the
/// Validate check and records "Finished Dev"; neither is "Submitted for review". `prime` still finds the latest
/// summary.
#[tokio::test(flavor = "multi_thread")]
async fn finished_stages_on_the_test_flow() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.board.use_flow_for_tests("SHOP", &every_kind_file()).await;
    let t = w.create("SHOP", "Export as CSV").await;
    w.start_worktree(&t).await;
    w.force_status(&t, Status::Working).await;
    into_stage(&w, &t, "product-spec").await;

    let after = submit(&w, &t, "Agreed: CSV with a header row.").await;
    assert_eq!(after.place, Place::Waiting(Some("dev".into())));
    assert_eq!(
        stage_ends(&w, "SHOP-1").await,
        [(
            EventKind::StageFinished,
            Some("Product spec".into()),
            "Agreed: CSV with a header row.".into()
        )]
    );

    w.force_status(&t, Status::Working).await; // in Dev
    let after = submit(&w, &t, "CSV export with a header row.").await;
    assert_eq!(after.place, Place::stage_of("validate"));
    assert_eq!(
        stage_ends(&w, "SHOP-1").await[0],
        (
            EventKind::StageFinished,
            Some("Dev".into()),
            "CSV export with a header row.".into()
        )
    );
    assert_eq!(
        w.board.review_summary(&t.id).await.unwrap().as_deref(),
        Some("CSV export with a header row.")
    );
    w.stop().await;
}

/// The default flow: before Review (Validation off) "Submitted for review"; into Validate (Validation on) "Finished
/// Dev".
#[tokio::test(flavor = "multi_thread")]
async fn the_default_flow_submits_for_review_and_finishes_dev() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let off = w.create("SHOP", "Sort notes by date").await;
    let on = w.create("SHOP", "Search notes").await;
    for t in [&off, &on] {
        w.start_worktree(t).await;
        w.force_status(t, Status::Working).await;
    }

    validation(&w, false).await;
    let after = submit(&w, &off, "Sorted by date.").await;
    assert_eq!(after.status, Status::WaitingForYou);
    assert_eq!(
        stage_ends(&w, "SHOP-1").await,
        [(EventKind::ReviewSubmitted, None, "Sorted by date.".into())]
    );

    validation(&w, true).await;
    let after = submit(&w, &on, "Search across titles.").await;
    assert_eq!(after.place, Place::stage_of("validate"));
    assert_eq!(
        stage_ends(&w, "SHOP-2").await,
        [(
            EventKind::StageFinished,
            Some("Dev".into()),
            "Search across titles.".into()
        )]
    );
    w.stop().await;
}

async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request =
        json!({"protocol_version": 1, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn refused(answer: &Value) -> &str {
    assert_eq!(answer["error"]["kind"], "refused", "{answer}");
    answer["error"]["message"].as_str().unwrap()
}

/// STG-10 with AGM-10's names, on the default flow: the Validate check's session may not `stage next` or `stage
/// done`; the Dev session may not `check pass`. The refusals name the stage (Q269).
#[tokio::test(flavor = "multi_thread")]
async fn ending_a_stage_by_stage_kind_on_the_default_flow() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let dev = w.create("SHOP", "Sort notes by date").await;
    let checked = w.create("SHOP", "Search notes").await;

    let dev_session = w.agent_shell(&dev).await;
    w.force_status(&dev, Status::Working).await;
    let answer = send(
        &w,
        &dev_session,
        "validation.verdict",
        json!({"key": "SHOP-1", "pass": true}),
    )
    .await;
    assert_eq!(
        refused(&answer),
        "SHOP-1's Dev session can't give a check verdict"
    );

    let checker = w.agent_shell(&checked).await;
    w.force_status(&checked, Status::Checking).await;
    sqlx::query("UPDATE stage_runs SET session_id = ? WHERE ticket_id = ? AND ended_at IS NULL")
        .bind(&checker.session)
        .bind(&checked.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    for method in ["ticket.stage_next", "ticket.stage_done"] {
        let answer = send(
            &w,
            &checker,
            method,
            json!({"key": "SHOP-2", "summary": "Done"}),
        )
        .await;
        assert_eq!(
            refused(&answer),
            "SHOP-2's Validate check can only give its verdict",
            "{method}"
        );
    }
    assert_eq!(w.status("SHOP-2").await, Status::Checking);
    w.stop().await;
}
