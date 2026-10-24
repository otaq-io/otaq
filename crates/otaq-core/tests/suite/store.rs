//! The SQLite store (SDD §8): migrations, re-opening, foreign keys and a round trip through every table.
//! Every test uses a database in a temporary folder, never the real data folder.

use otaq_core::data_dir::DataDir;
use otaq_core::store::projects::NewProject;
use otaq_core::store::tickets::NewTicket;
use otaq_core::store::{Store, new_id, now_ms};
use sqlx::Row;

const TABLES: &[&str] = &[
    "active_limits",
    "agent_holds",
    "agent_skills",
    "agents",
    "check_counters",
    "decisions",
    "events",
    "flow_proposals",
    "flow_waiting",
    "human_questions",
    "lead_notes",
    "lead_stuck",
    "lead_suggestions",
    "limit_windows",
    "manual_checks",
    "messages",
    "pending_deliveries",
    "pr_flows",
    "project_agents",
    "project_flows",
    "projects",
    "review_comments",
    "reviews",
    "service_runs",
    "sessions",
    "settings",
    "stage_runs",
    "ticket_comments",
    "ticket_deps",
    "ticket_key_aliases",
    "ticket_stage_choices",
    "tickets",
    "usage",
    "validations",
];

async fn temp_store() -> (tempfile::TempDir, Store) {
    let tmp = tempfile::tempdir().unwrap();
    let store = Store::open_in(&DataDir::new(tmp.path().join("home")))
        .await
        .unwrap();
    (tmp, store)
}

fn shop_web() -> NewProject {
    NewProject {
        key: "SHOP".into(),
        name: "shop-web".into(),
        repo_path: "/Users/me/code/shop-web".into(),
        base_branch: "main".into(),
        flow_setup: false,
    }
}

fn ticket(project_id: &str, number: i64, kind: &str, title: &str) -> NewTicket {
    NewTicket {
        project_id: project_id.into(),
        number,
        epic_id: None,
        child_number: 0,
        kind: kind.into(),
        title: title.into(),
        body: String::new(),
        stage_id: None,
        state: "normal".into(),
        created_by: "user".into(),
    }
}

async fn names(store: &Store, kind: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = ? AND name NOT LIKE 'sqlite_%' \
         AND name NOT LIKE '_sqlx_%' ORDER BY name",
    )
    .bind(kind)
    .fetch_all(store.pool())
    .await
    .unwrap()
}

async fn count(store: &Store, table: &str) -> i64 {
    // Table names come from the fixed list above.
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
        .fetch_one(store.pool())
        .await
        .unwrap()
}

#[tokio::test]
async fn migrations_create_every_table_on_an_empty_database() {
    let (tmp, store) = temp_store().await;
    assert!(tmp.path().join("home").is_dir());
    assert_eq!(names(&store, "table").await, TABLES);

    let indexes = names(&store, "index").await;
    for expected in [
        "ticket_deps_parent",
        "sessions_ticket",
        "tickets_epic",
        "events_ticket",
    ] {
        assert!(indexes.iter().any(|i| i == expected), "{indexes:?}");
    }
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(foreign_keys, 1);
}

#[tokio::test]
async fn opening_twice_is_a_no_op() {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));

    let first = Store::open_in(&data).await.unwrap();
    let project = first.insert_project(&shop_web()).await.unwrap();
    let migrations = count(&first, "_sqlx_migrations").await;
    assert!(migrations >= 1);
    first.close().await;

    let second = Store::open_in(&data).await.unwrap();
    assert_eq!(count(&second, "_sqlx_migrations").await, migrations);
    assert_eq!(names(&second, "table").await, TABLES);
    assert_eq!(
        second.project(&project.id).await.unwrap(),
        Some(project.clone())
    );
}

#[tokio::test]
async fn projects_and_tickets_round_trip() {
    let (_tmp, store) = temp_store().await;
    let project = store.insert_project(&shop_web()).await.unwrap();
    assert_eq!(project.key, "SHOP");
    assert_eq!(project.next_ticket_number, 1);
    assert_eq!(project.settings_json, "{}");
    assert_eq!(
        store.project_by_key("SHOP").await.unwrap(),
        Some(project.clone())
    );
    assert_eq!(store.project_by_key("BILL").await.unwrap(), None);

    let epic = store
        .insert_ticket(&ticket(&project.id, 40, "epic", "Wishlist"))
        .await
        .unwrap();
    let child = store
        .insert_ticket(&NewTicket {
            epic_id: Some(epic.id.clone()),
            child_number: 1,
            ..ticket(&project.id, 40, "ticket", "Wishlist button")
        })
        .await
        .unwrap();

    assert_eq!(store.ticket(&epic.id).await.unwrap(), Some(epic.clone()));
    assert_eq!(
        store.ticket_by_number(&project.id, 40, 1).await.unwrap(),
        Some(child.clone())
    );
    assert_eq!(
        store.ticket_by_number(&project.id, 40, 0).await.unwrap(),
        Some(epic)
    );
    assert!(!child.parked && !child.branch_locked);
    assert_eq!(child.sent_back_round, 0);

    // SHOP-40.1 exists once.
    let duplicate = store
        .insert_ticket(&NewTicket {
            child_number: 1,
            ..ticket(&project.id, 40, "ticket", "again")
        })
        .await;
    assert!(duplicate.is_err());
}

#[tokio::test]
async fn foreign_keys_are_enforced_and_removing_a_project_removes_its_tickets() {
    let (_tmp, store) = temp_store().await;
    let orphan = store
        .insert_ticket(&ticket("no-such-project", 1, "ticket", "Orphan"))
        .await;
    assert!(orphan.is_err());

    let project = store.insert_project(&shop_web()).await.unwrap();
    let epic = store
        .insert_ticket(&ticket(&project.id, 40, "epic", "Wishlist"))
        .await
        .unwrap();
    store
        .insert_ticket(&NewTicket {
            epic_id: Some(epic.id.clone()),
            child_number: 1,
            ..ticket(&project.id, 40, "ticket", "Wishlist button")
        })
        .await
        .unwrap();
    sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(&project.id)
        .execute(store.pool())
        .await
        .unwrap();
    assert_eq!(count(&store, "tickets").await, 0);
}

/// Inserts one row into every table of §8 and reads each back.
#[tokio::test]
async fn one_row_round_trips_through_every_table() {
    let (_tmp, store) = temp_store().await;
    let pool = store.pool();
    let at = now_ms();

    let project = store.insert_project(&shop_web()).await.unwrap();
    let t18 = store
        .insert_ticket(&ticket(&project.id, 18, "ticket", "Checkout button"))
        .await
        .unwrap();
    let t19 = store
        .insert_ticket(&ticket(&project.id, 19, "ticket", "Cart badge"))
        .await
        .unwrap();

    sqlx::query("INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)")
        .bind("agent_limit")
        .bind(r#"{"enabled":false,"max":5}"#)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
    let value: String = sqlx::query_scalar("SELECT value_json FROM settings WHERE key = ?")
        .bind("agent_limit")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(value, r#"{"enabled":false,"max":5}"#);

    sqlx::query("INSERT INTO ticket_deps (ticket_id, depends_on_id) VALUES (?, ?)")
        .bind(&t19.id)
        .bind(&t18.id)
        .execute(pool)
        .await
        .unwrap();
    let parent: String =
        sqlx::query_scalar("SELECT depends_on_id FROM ticket_deps WHERE ticket_id = ?")
            .bind(&t19.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(parent, t18.id);
    let self_dep = sqlx::query("INSERT INTO ticket_deps (ticket_id, depends_on_id) VALUES (?, ?)")
        .bind(&t18.id)
        .bind(&t18.id)
        .execute(pool)
        .await;
    assert!(self_dep.is_err());

    sqlx::query("INSERT INTO ticket_key_aliases (old_key, ticket_id) VALUES (?, ?)")
        .bind("SHOP-51")
        .bind(&t19.id)
        .execute(pool)
        .await
        .unwrap();
    let aliased: String =
        sqlx::query_scalar("SELECT ticket_id FROM ticket_key_aliases WHERE old_key = ?")
            .bind("SHOP-51")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(aliased, t19.id);

    // Every tool session id is kept: two sessions for one ticket (d0045).
    let session_id = new_id();
    for (id, claude_id) in [(session_id.clone(), "c-1"), (new_id(), "c-2")] {
        sqlx::query(
            "INSERT INTO sessions (id, project_id, ticket_id, kind, pty_id, tool, tool_session_id, state, \
             started_at, ended_at, end_reason) VALUES (?, ?, ?, 'agent', 'pty-1', 'claude', ?, 'ended', ?, ?, \
             'restart')",
        )
        .bind(id)
        .bind(&project.id)
        .bind(&t18.id)
        .bind(claude_id)
        .bind(at)
        .bind(at + 1)
        .execute(pool)
        .await
        .unwrap();
    }
    let claude_ids: Vec<String> = sqlx::query_scalar(
        "SELECT tool_session_id FROM sessions WHERE ticket_id = ? ORDER BY tool_session_id",
    )
    .bind(&t18.id)
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(claude_ids, ["c-1", "c-2"]);
    // The Project terminal has no ticket.
    sqlx::query(
        "INSERT INTO sessions (id, project_id, ticket_id, kind, state, started_at) \
         VALUES (?, ?, NULL, 'project', 'running', ?)",
    )
    .bind(new_id())
    .bind(&project.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO events (id, ticket_id, kind, actor, payload_json, at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(new_id())
    .bind(&t18.id)
    .bind("moved")
    .bind("user")
    .bind(r#"{"to":"dev"}"#)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let row = sqlx::query("SELECT kind, actor, payload_json FROM events WHERE ticket_id = ?")
        .bind(&t18.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("kind"), "moved");
    assert_eq!(row.get::<String, _>("payload_json"), r#"{"to":"dev"}"#);

    sqlx::query(
        "INSERT INTO review_comments (id, ticket_id, round, file, line, side, body, created_at, sent_at) \
         VALUES (?, ?, 1, 'src/cart.ts', 42, 'new', 'Use the shared formatter', ?, NULL)",
    )
    .bind(new_id())
    .bind(&t18.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let row = sqlx::query("SELECT line, body, sent_at FROM review_comments WHERE ticket_id = ?")
        .bind(&t18.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("line"), 42);
    assert_eq!(row.get::<Option<i64>, _>("sent_at"), None);

    sqlx::query(
        "INSERT INTO ticket_comments (id, ticket_id, n, author_kind, author_id, author_name, author_tool, stage_id, \
         round, body, report_file, report_bytes, report_lines, posted_at) \
         VALUES (?, ?, 4, 'agent', 'validator', 'Validator', 'claude', 'validate', 2, 'Fail: 3 tests fail', \
         'reports/shop-web/SHOP-18/x.txt', 49152, 2310, ?)",
    )
    .bind(new_id())
    .bind(&t18.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let row = sqlx::query(
        "SELECT n, round, report_lines, deleted_at FROM ticket_comments WHERE ticket_id = ?",
    )
    .bind(&t18.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(row.get::<i64, _>("n"), 4);
    assert_eq!(row.get::<i64, _>("round"), 2);
    assert_eq!(row.get::<i64, _>("report_lines"), 2310);
    assert_eq!(row.get::<Option<i64>, _>("deleted_at"), None);

    sqlx::query(
        "INSERT INTO manual_checks (id, ticket_id, n, body, author_kind, author_id, author_name, author_tool, \
         stage_id, round, added_at) \
         VALUES (?, ?, 1, 'In Safari, reload offline', 'agent', 'validator', 'Validator', 'claude', 'validate', 2, ?)",
    )
    .bind(new_id())
    .bind(&t18.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let ticked: Option<i64> =
        sqlx::query_scalar("SELECT ticked_at FROM manual_checks WHERE ticket_id = ?")
            .bind(&t18.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(ticked, None);

    sqlx::query(
        "INSERT INTO reviews (id, ticket_id, round, action, at) VALUES (?, ?, 1, 'send_back', ?)",
    )
    .bind(new_id())
    .bind(&t18.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let action: String = sqlx::query_scalar("SELECT action FROM reviews WHERE ticket_id = ?")
        .bind(&t18.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(action, "send_back");

    sqlx::query(
        "INSERT INTO messages (id, from_ticket_id, to_ticket_id, question, answer, delivery, asked_at) \
         VALUES (?, ?, ?, ?, NULL, 'live', ?)",
    )
    .bind(new_id())
    .bind(&t19.id)
    .bind(&t18.id)
    .bind("Which endpoint returns the cart?")
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let question: String =
        sqlx::query_scalar("SELECT question FROM messages WHERE to_ticket_id = ?")
            .bind(&t18.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(question, "Which endpoint returns the cart?");

    sqlx::query(
        "INSERT INTO pending_deliveries (id, session_id, kind, text, created_at) \
         VALUES (?, ?, 'ticket_change', ?, ?)",
    )
    .bind(new_id())
    .bind(&session_id)
    .bind("The ticket was edited")
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let text: String =
        sqlx::query_scalar("SELECT text FROM pending_deliveries WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(text, "The ticket was edited");

    sqlx::query(
        "INSERT INTO usage (session_id, day, input, cached, output) VALUES (?, '2026-10-09', 1200, 5000, 340)",
    )
    .bind(&session_id)
    .execute(pool)
    .await
    .unwrap();
    let row = sqlx::query("SELECT input, cached, reasoning FROM usage WHERE session_id = ?")
        .bind(&session_id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(row.get::<i64, _>("input"), 1200);
    assert_eq!(row.get::<i64, _>("cached"), 5000);
    assert_eq!(row.get::<Option<i64>, _>("reasoning"), None);

    sqlx::query(
        "INSERT INTO limit_windows (tool, key, length_minutes, used_percent, resets_at, observed_at) \
         VALUES ('claude', 'session', 300, 82.5, ?, ?)",
    )
    .bind(at + 3_600_000)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let row = sqlx::query("SELECT used_percent, length_minutes FROM limit_windows")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(row.get::<f64, _>("used_percent"), 82.5);
    assert_eq!(row.get::<i64, _>("length_minutes"), 300);
    // One row per tool and key.
    assert!(
        sqlx::query(
            "INSERT INTO limit_windows (tool, key, used_percent, observed_at) \
             VALUES ('claude', 'session', 1, 1)"
        )
        .execute(pool)
        .await
        .is_err()
    );

    sqlx::query(
        "INSERT INTO active_limits (tool, kind, text, hit_at, resets_at, reset_source) \
         VALUES ('claude', 'session', 'You hit it', ?, ?, 'status_line')",
    )
    .bind(at)
    .bind(at + 3_600_000)
    .execute(pool)
    .await
    .unwrap();
    let row = sqlx::query("SELECT kind, reset_seen_at FROM active_limits WHERE tool = 'claude'")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("kind"), "session");
    assert_eq!(row.get::<Option<i64>, _>("reset_seen_at"), None);

    sqlx::query(
        "INSERT INTO human_questions (id, ticket_id, question, options_json, recommended, asked_at) \
         VALUES (?, ?, ?, '[\"Postmark\",\"SES\"]', 1, ?)",
    )
    .bind(new_id())
    .bind(&t18.id)
    .bind("Which email provider?")
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let recommended: i64 =
        sqlx::query_scalar("SELECT recommended FROM human_questions WHERE ticket_id = ?")
            .bind(&t18.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(recommended, 1);

    sqlx::query(
        "INSERT INTO service_runs (id, pid, build_id, started_at, stopped_at) VALUES (?, 4242, 'abc.1', ?, ?)",
    )
    .bind(new_id())
    .bind(at)
    .bind(at + 60_000)
    .execute(pool)
    .await
    .unwrap();
    let stopped: Option<i64> = sqlx::query_scalar("SELECT stopped_at FROM service_runs")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(stopped, Some(at + 60_000));

    sqlx::query(
        "INSERT INTO pr_flows (ticket_id, kind, state, proposal_json, started_at, updated_at) \
         VALUES (?, 'open', 'proposed', '{}', ?, ?)",
    )
    .bind(&t18.id)
    .bind(at)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO validations (id, ticket_id, round, trigger, session_id, started_at, ended_at, outcome, \
         reason, notes_json, head_sha) VALUES (?, ?, 1, 'agent', NULL, ?, ?, 'fail', 'cart.ts:12 off by one', '[]', 'abc')",
    )
    .bind(new_id())
    .bind(&t18.id)
    .bind(at)
    .bind(at + 1)
    .execute(pool)
    .await
    .unwrap();
    let report: String = sqlx::query_scalar("SELECT reason FROM validations WHERE ticket_id = ?")
        .bind(&t18.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(report, "cart.ts:12 off by one");

    // A Project agent (T-0199, d0068).
    sqlx::query(
        "INSERT INTO project_agents (id, project_id, name, name_source, tool, tool_session_id, state, \
         first_request, created_at) VALUES (?, ?, 'Checkout slowness', 'agent', 'claude', 'c-9', 'running', \
         'why is checkout slow?', ?)",
    )
    .bind(new_id())
    .bind(&project.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let name: String = sqlx::query_scalar("SELECT name FROM project_agents")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(name, "Checkout slowness");

    // A recorded decision (T-0276, d0084).
    sqlx::query(
        "INSERT INTO decisions (id, ticket_id, ordinal, question, built, other_options, why, recorded_by, \
         recorded_at) VALUES (?, ?, 1, 'Order of saved items?', 'newest first', '[\"oldest first\"]', 'w', \
         'agent:SHOP-18', ?)",
    )
    .bind(new_id())
    .bind(&t18.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let outcome: String = sqlx::query_scalar("SELECT outcome FROM decisions WHERE ticket_id = ?")
        .bind(&t18.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(outcome, "open");

    // An epic lead's suggestion (T-0280, d0083).
    sqlx::query(
        "INSERT INTO lead_suggestions (id, epic_id, child_id, kind, reason, suggested_by, created_at) \
         VALUES (?, ?, ?, 'close', 'PAY-123 doesn''t ask for an export.', 'project-agent:l1', ?)",
    )
    .bind(new_id())
    .bind(&t19.id)
    .bind(&t18.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let state: String = sqlx::query_scalar("SELECT state FROM lead_suggestions WHERE child_id = ?")
        .bind(&t18.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(state, "open");

    // An epic lead's note to a running child (T-0281, d0083): unread until read.
    sqlx::query(
        "INSERT INTO lead_notes (id, epic_id, child_id, lead_id, written_by, text, created_at) \
         VALUES (?, ?, ?, 'l1', 'project-agent:l1', 'Use the same order.', ?)",
    )
    .bind(new_id())
    .bind(&t19.id)
    .bind(&t18.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let read_how: Option<String> =
        sqlx::query_scalar("SELECT read_how FROM lead_notes WHERE child_id = ?")
            .bind(&t18.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(read_how, None);

    // A stuck child's one lead attempt (T-0285, d0083): held until the lead's note or the hand-back.
    sqlx::query(
        "INSERT INTO lead_stuck (child_id, epic_id, lead_id, stop, detail, rounds, state, held_at) \
         VALUES (?, ?, 'l1', 'validation_failed', 'orders.test.ts expects `list`', 3, 'held', ?)",
    )
    .bind(&t18.id)
    .bind(&t19.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let state: String = sqlx::query_scalar("SELECT state FROM lead_stuck WHERE child_id = ?")
        .bind(&t18.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(state, "held");

    // An agent with an imported skill (AGM-01): the shipped ones are there already.
    let light = new_id();
    sqlx::query(
        "INSERT INTO agents (id, name, instructions, tool, model, came_from, created_at, updated_at) \
         VALUES (?, 'Light', 'Typos, docs, small config', 'claude', 'haiku', 'you', ?, ?)",
    )
    .bind(&light)
    .bind(at)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO agent_skills (agent_id, position, imported_id, name, original, imported_at, fingerprint) \
         VALUES (?, 0, 'copy-1', 'my-commit', '/tmp/my-commit', ?, 'f')",
    )
    .bind(&light)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let name: String = sqlx::query_scalar(
        "SELECT name FROM agent_skills WHERE agent_id = ? AND imported_id IS NOT NULL",
    )
    .bind(&light)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(name, "my-commit");

    // A held Developer start on that agent (M23, SCH-04): its tool is at its limit.
    sqlx::query(
        "INSERT INTO agent_holds (id, ticket_id, role, reason, tool, model, agent_id, since) \
         VALUES (?, ?, 'developer', 'tool_limit', 'claude', 'haiku', ?, ?)",
    )
    .bind(new_id())
    .bind(&t18.id)
    .bind(&light)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let reason: String = sqlx::query_scalar("SELECT reason FROM agent_holds WHERE ticket_id = ?")
        .bind(&t18.id)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(reason, "tool_limit");

    // A stage run (STG-05, T-0400): a ticket entering a stage opens one.
    sqlx::query("UPDATE tickets SET stage_id = 'dev', state = 'working' WHERE id = ?")
        .bind(&t18.id)
        .execute(pool)
        .await
        .unwrap();
    let (stage, kind): (String, String) =
        sqlx::query_as("SELECT stage_id, kind FROM stage_runs WHERE ticket_id = ?")
            .bind(&t18.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!((stage.as_str(), kind.as_str()), ("dev", "agent"));

    // A stage choice (AGM-08, T-0407): the ticket's Validate runs on that agent, set by the Project agent.
    sqlx::query(
        "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, agent_id, set_by, by_name, reason, set_at) \
         VALUES (?, 'validate', 'agent', ?, 'project_agent', 'Checkout slowness', 'small change', ?)",
    )
    .bind(&t18.id)
    .bind(&light)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let choice: String =
        sqlx::query_scalar("SELECT choice FROM ticket_stage_choices WHERE ticket_id = ?")
            .bind(&t18.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(choice, "agent");

    // Each agent check's own counter (T-0417).
    sqlx::query(
        "INSERT INTO check_counters (ticket_id, stage_id, fails, extra, moved_past, updated_at) \
         VALUES (?, 'security', 2, 1, 'failed', ?)",
    )
    .bind(&t18.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let fails: i64 = sqlx::query_scalar(
        "SELECT fails FROM check_counters WHERE ticket_id = ? AND stage_id = 'security'",
    )
    .bind(&t18.id)
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(fails, 2);

    // An open flow proposal and a waiting flow change (T-0413).
    sqlx::query(
        "INSERT INTO flow_proposals (project_id, yaml, source_json, proposed_by_json, created_at)
         VALUES (?, 'flow: []', '{\"kind\":\"agents_file\"}', '{\"by\":\"user\"}', ?)",
    )
    .bind(&project.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO flow_waiting (project_id, yaml, applied_by, changes_json, since)
         VALUES (?, 'flow: []', 'user', '[\"QA after Security\"]', ?)",
    )
    .bind(&project.id)
    .bind(at)
    .execute(pool)
    .await
    .unwrap();
    let changes: String =
        sqlx::query_scalar("SELECT changes_json FROM flow_waiting WHERE project_id = ?")
            .bind(&project.id)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(changes, r#"["QA after Security"]"#);

    for table in TABLES {
        assert!(count(&store, table).await >= 1, "{table} is empty");
    }
}

#[tokio::test]
async fn agent_counts_come_from_live_agent_sessions() {
    use otaq_core::store::agents::AgentCounts;
    let (_tmp, store) = temp_store().await;
    assert_eq!(store.agent_counts().await.unwrap(), AgentCounts::default());

    let project = store.insert_project(&shop_web()).await.unwrap();
    let at = now_ms();
    let add = async |number: i64, state: &str, kind: &str, ended: Option<i64>| {
        let t = store
            .insert_ticket(&NewTicket {
                stage_id: Some("dev".into()),
                state: state.into(),
                ..ticket(&project.id, number, "ticket", "t")
            })
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO sessions (id, project_id, ticket_id, kind, state, started_at, ended_at) \
             VALUES (?, ?, ?, ?, 'running', ?, ?)",
        )
        .bind(new_id())
        .bind(&project.id)
        .bind(&t.id)
        .bind(kind)
        .bind(at)
        .bind(ended)
        .execute(store.pool())
        .await
        .unwrap();
    };
    add(18, "working", "agent", None).await;
    add(19, "working", "agent", None).await;
    add(20, "working", "agent", None).await;
    add(21, "needs_you", "agent", None).await;
    add(22, "asks_you", "agent", None).await;
    add(23, "working", "agent", Some(at + 1)).await; // ended: not counted
    add(24, "working", "shell", None).await; // not an agent
    assert_eq!(
        store.agent_counts().await.unwrap(),
        AgentCounts {
            working: 3,
            waiting: 2
        }
    );
}
