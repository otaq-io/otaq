//! Migrations that change existing rows (SDD §8): a database is written by the migrations up to a version, filled
//! with rows of that time, then opened with [`Store::open`], which runs the rest. Every test uses a temporary
//! folder, never the real data folder.

use std::borrow::Cow;
use std::path::Path;
use std::str::FromStr;

use otaq_core::naming;
use otaq_core::store::Store;
use serde_json::json;
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

static ALL: Migrator = sqlx::migrate!("./migrations");

/// A database at `path` with the migrations up to `version` applied, as an older Otaq left it.
async fn database_at(path: &Path, version: i64) -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite:")
        .unwrap()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .connect_with(options)
        .await
        .unwrap();
    let older = Migrator {
        migrations: Cow::Owned(
            ALL.iter()
                .filter(|m| m.version <= version)
                .cloned()
                .collect(),
        ),
        ..Migrator::DEFAULT
    };
    older.run(&pool).await.unwrap();
    pool
}

async fn exec(pool: &SqlitePool, sql: &str) {
    sqlx::query(sqlx::AssertSqlSafe(sql))
        .execute(pool)
        .await
        .unwrap();
}

/// T-0175 (TKT-04, d0067): migration 0022 appends each ticket's "Done when" items to its description under a
/// "## Done when" heading, in position order, and drops the table; a ticket without items keeps its body byte
/// for byte.
#[tokio::test]
async fn done_when_items_move_into_the_description() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 21).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    for (id, number, kind, body) in [
        ("t18", 18, "ticket", "Show the prices.\n\n"),
        ("t19", 19, "ticket", "  Keep me exactly.\n\n"),
        ("t20", 20, "ticket", ""),
        ("t40", 40, "ticket", "Wishlist."),
    ] {
        sqlx::query(
            "INSERT INTO tickets (id, project_id, number, kind, title, body, board_column, state, created_by,
                                  created_at, updated_at)
             VALUES (?, 'p', ?, ?, 'T', ?, 'backlog', 'backlog', 'me', 0, 0)",
        )
        .bind(id)
        .bind(number)
        .bind(kind)
        .bind(body)
        .execute(&pool)
        .await
        .unwrap();
    }
    // Stored out of order: the position decides.
    for (ticket, position, text) in [
        ("t18", 1, "works on mobile widths"),
        ("t18", 0, "sorted by price"),
        ("t20", 0, "only item"),
        ("t40", 0, "every piece merged"),
    ] {
        sqlx::query(
            "INSERT INTO ticket_done_when (ticket_id, position, text, added_by) VALUES (?, ?, ?, 'me')",
        )
        .bind(ticket)
        .bind(position)
        .bind(text)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let body = |id: &'static str| {
        let pool = store.pool().clone();
        async move {
            sqlx::query("SELECT body FROM tickets WHERE id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .get::<String, _>(0)
        }
    };
    assert_eq!(
        body("t18").await,
        "Show the prices.\n\n## Done when\n\n- sorted by price\n- works on mobile widths"
    );
    assert_eq!(
        body("t19").await,
        "  Keep me exactly.\n\n",
        "no items: untouched"
    );
    assert_eq!(body("t20").await, "## Done when\n\n- only item");
    assert_eq!(
        body("t40").await,
        "Wishlist.\n\n## Done when\n\n- every piece merged"
    );
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'ticket_done_when'",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(tables, 0, "the table is dropped");
}

/// T-0177 (TKT-09, d0065): migration 0023 gives existing tickets priority 3; from migration 0066 it is never NULL.
#[tokio::test]
async fn existing_tickets_get_normal() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 22).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, epic_id, child_number, kind, title, board_column, state,
                              created_by, created_at, updated_at)
         VALUES ('t18', 'p', 18, NULL, 0, 'ticket', 'T', 'backlog', 'backlog', 'me', 0, 0),
                ('t19', 'p', 19, NULL, 0, 'ticket', 'U', 'backlog', 'backlog', 'me', 0, 0)",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let rows: Vec<(String, i64)> = sqlx::query_as("SELECT id, priority FROM tickets ORDER BY id")
        .fetch_all(store.pool())
        .await
        .unwrap();
    assert_eq!(rows, [("t18".to_owned(), 3), ("t19".to_owned(), 3)]);
    let refused = sqlx::query("UPDATE tickets SET priority = 5 WHERE id = 't18'")
        .execute(store.pool())
        .await;
    assert!(refused.is_err(), "1 to 4 only");
}

/// T-0178 (d0066): migration 0024 moves every queued status to the Ready for Dev column and adds ready_since.
#[tokio::test]
async fn queued_tickets_move_to_ready_for_dev() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 23).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    let states = [
        "queued",
        "waiting_for_slot",
        "held",
        "parent_closed",
        "start_interrupted",
        "working",
        "backlog",
    ];
    for (n, state) in states.iter().enumerate() {
        let column = match *state {
            "backlog" => "backlog",
            _ => "dev",
        };
        sqlx::query(
            "INSERT INTO tickets (id, project_id, number, kind, title, board_column, state, created_by,
                                  created_at, updated_at)
             VALUES (?, 'p', ?, 'ticket', 'T', ?, ?, 'me', 0, 0)",
        )
        .bind(*state)
        .bind(n as i64 + 1)
        .bind(column)
        .bind(*state)
        .execute(&pool)
        .await
        .unwrap();
    }
    pool.close().await;

    // Read at 0024: 0046 drops the column (T-0400).
    let pool = database_at(&path, 24).await;
    let rows: Vec<(String, String, Option<i64>)> =
        sqlx::query_as("SELECT state, board_column, ready_since FROM tickets ORDER BY number")
            .fetch_all(&pool)
            .await
            .unwrap();
    for (state, column, ready_since) in rows {
        let expected = match state.as_str() {
            "working" => "dev",
            "backlog" => "backlog",
            _ => "ready_for_dev",
        };
        assert_eq!(column, expected, "{state}");
        assert_eq!(ready_since, None, "the scheduler fills it in");
    }
}

/// T-0182 (d0063, d0064): migration 0025 adds the validations table and the ticket columns; a ticket from before
/// M12 has no Review entry.
#[tokio::test]
async fn validation_columns_start_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 24).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, kind, title, board_column, state, created_by, created_at,
                              updated_at)
         VALUES ('t18', 'p', 18, 'ticket', 'T', 'ready_for_review', 'ready_for_review', 'me', 0, 0)",
    )
    .await;
    pool.close().await;

    // Up to 0025 only: 0051 drops the round counter (each check counts its own, T-0417).
    let store = database_at(&path, 25).await;
    let row: (Option<String>, i64, i64, Option<String>, bool) = sqlx::query_as(
        "SELECT review_entry, validation_round, auto_merge_conflicts, auto_merge_stop, merged_automatically
         FROM tickets WHERE id = 't18'",
    )
    .fetch_one(&store)
    .await
    .unwrap();
    assert_eq!(row, (None, 0, 0, None, false));
    exec(
        &store,
        "INSERT INTO validations (id, ticket_id, round, trigger, started_at) VALUES ('v1', 't18', 1, 'agent', 0)",
    )
    .await;
}

/// T-0183 (d0063): migration 0026 rebuilds `sessions` so a validator (and a Project agent) can be stored; every
/// session and the rows that point at it (usage, deliveries) survive.
#[tokio::test]
async fn sessions_take_the_new_kinds_and_keep_their_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 25).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, kind, title, board_column, state, created_by, created_at,
                              updated_at)
         VALUES ('t18', 'p', 18, 'ticket', 'T', 'dev', 'working', 'me', 0, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO sessions (id, project_id, ticket_id, kind, claude_session_id, state, started_at, summary_dismissed_at)
         VALUES ('s1', 'p', 't18', 'agent', 'c1', 'running', 5, 7)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO pending_deliveries (id, session_id, kind, text, created_at) VALUES ('d1', 's1', 'agent_message', 'hi', 0)",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let row: (String, String, Option<String>, Option<i64>) = sqlx::query_as(
        "SELECT kind, state, tool_session_id, summary_dismissed_at FROM sessions WHERE id = 's1'",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(
        row,
        ("agent".into(), "running".into(), Some("c1".into()), Some(7))
    );
    let deliveries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM pending_deliveries")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(deliveries, 1, "nothing cascaded");
    for kind in ["validator", "project_agent"] {
        sqlx::query(
            "INSERT INTO sessions (id, project_id, ticket_id, kind, state, started_at) VALUES (?, 'p', 't18', ?, 'running', 0)",
        )
        .bind(format!("s-{kind}"))
        .bind(kind)
        .execute(store.pool())
        .await
        .unwrap();
    }
    assert!(
        sqlx::query(
            "INSERT INTO sessions (id, project_id, kind, state, started_at) VALUES ('x', 'p', 'nope', 'running', 0)",
        )
        .execute(store.pool())
        .await
        .is_err()
    );
    let fk: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(fk, 1, "foreign keys are on again");
}

/// T-0199 (d0068): migration 0028 adds `project_agents` and `sessions.project_agent_id`; existing sessions keep
/// their rows with no Project agent, and deleting a Project agent keeps its sessions.
#[tokio::test]
async fn project_agents_start_empty_and_their_sessions_outlive_them() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 27).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO sessions (id, project_id, kind, state, started_at) VALUES ('s1', 'p', 'project', 'ended', 5)",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let agent: Option<String> =
        sqlx::query_scalar("SELECT project_agent_id FROM sessions WHERE id = 's1'")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(agent, None);
    exec(
        store.pool(),
        "INSERT INTO project_agents (id, project_id, tool, tool_session_id, state, created_at)
         VALUES ('a1', 'p', 'claude', 'c1', 'running', 0)",
    )
    .await;
    exec(
        store.pool(),
        "INSERT INTO sessions (id, project_id, kind, state, started_at, project_agent_id)
         VALUES ('s2', 'p', 'project_agent', 'running', 6, 'a1')",
    )
    .await;
    let row = sqlx::query(
        "SELECT name, name_source, first_request, limit_stopped, closed_at FROM project_agents WHERE id = 'a1'",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<Option<String>, _>(0), None);
    assert_eq!(row.get::<Option<String>, _>(1), None);
    assert_eq!(row.get::<Option<String>, _>(2), None);
    assert_eq!(row.get::<i64, _>(3), 0);
    assert_eq!(row.get::<Option<i64>, _>(4), None);
    let bad = sqlx::query(
        "INSERT INTO project_agents (id, project_id, tool, tool_session_id, state, created_at)
         VALUES ('a2', 'p', 'claude', 'c2', 'asleep', 0)",
    )
    .execute(store.pool())
    .await;
    assert!(bad.is_err(), "states are checked");
    exec(store.pool(), "DELETE FROM project_agents WHERE id = 'a1'").await;
    let agent: Option<String> =
        sqlx::query_scalar("SELECT project_agent_id FROM sessions WHERE id = 's2'")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(agent, None, "the session stays");
}

/// T-0200 (d0068): migration 0029 lets a Project agent ask: messages and human questions keep every row, and a
/// row names exactly one asker (a ticket or a Project agent).
#[tokio::test]
async fn questions_take_a_project_agent_as_the_asker() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 28).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    for (id, n) in [("t18", 18), ("t20", 20)] {
        exec(
            &pool,
            &format!(
                "INSERT INTO tickets (id, project_id, number, kind, title, board_column, state, created_by,
                                      created_at, updated_at)
                 VALUES ('{id}', 'p', {n}, 'ticket', 'T', 'dev', 'working', 'me', 0, 0)"
            ),
        )
        .await;
    }
    exec(
        &pool,
        "INSERT INTO messages (id, from_ticket_id, to_ticket_id, question, answer, delivery, asked_at, answered_at,
                               answer_delivered_at)
         VALUES ('m1', 't20', 't18', 'q?', 'a.', 'live', 1, 2, 3)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO human_questions (id, ticket_id, question, options_json, recommended, asked_at)
         VALUES ('h1', 't18', 'Postmark or SES?', '[\"Postmark\",\"SES\"]', 1, 4)",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let m: (Option<String>, Option<String>, String, Option<i64>) = sqlx::query_as(
        "SELECT from_ticket_id, from_agent_id, answer, answer_delivered_at FROM messages WHERE id = 'm1'",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(m, (Some("t20".into()), None, "a.".into(), Some(3)));
    let h: (Option<String>, String, Option<i64>) = sqlx::query_as(
        "SELECT ticket_id, options_json, recommended FROM human_questions WHERE id = 'h1'",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(
        h,
        (Some("t18".into()), "[\"Postmark\",\"SES\"]".into(), Some(1))
    );

    exec(
        store.pool(),
        "INSERT INTO project_agents (id, project_id, tool, tool_session_id, state, created_at)
         VALUES ('a1', 'p', 'claude', 'c1', 'running', 0)",
    )
    .await;
    exec(
        store.pool(),
        "INSERT INTO messages (id, from_agent_id, to_ticket_id, question, asked_at) VALUES ('m2', 'a1', 't18', 'q', 5)",
    )
    .await;
    exec(
        store.pool(),
        "INSERT INTO human_questions (id, project_agent_id, question, asked_at) VALUES ('h2', 'a1', 'q', 6)",
    )
    .await;
    for bad in [
        "INSERT INTO messages (id, to_ticket_id, question, asked_at) VALUES ('m3', 't18', 'q', 5)",
        "INSERT INTO messages (id, from_ticket_id, from_agent_id, to_ticket_id, question, asked_at)
         VALUES ('m4', 't20', 'a1', 't18', 'q', 5)",
        "INSERT INTO human_questions (id, question, asked_at) VALUES ('h3', 'q', 6)",
    ] {
        assert!(
            sqlx::query(sqlx::AssertSqlSafe(bad))
                .execute(store.pool())
                .await
                .is_err(),
            "{bad}"
        );
    }
    // Deleting the Project agent takes its questions with it.
    exec(store.pool(), "DELETE FROM project_agents WHERE id = 'a1'").await;
    let left: i64 = sqlx::query_scalar(
        "SELECT (SELECT COUNT(*) FROM messages) + (SELECT COUNT(*) FROM human_questions)",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(left, 2);
}

/// T-0276 (DEC-02, d0084): migration 0032 adds the empty `decisions` table to an M17 database (after 0031): every ticket keeps its
/// row, a decision starts open, and the outcome takes only its values.
#[tokio::test]
async fn decisions_start_empty_and_tickets_keep_their_rows() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 31).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 52, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, epic_id, child_number, kind, title, board_column, state,
                              started_at, created_by, created_at, updated_at, epic_queue_mode)
         VALUES ('t41', 'p', 41, NULL, 0, 'ticket', 'Wishlist API', 'dev', 'working', 3, 'me', 1, 1, 'none'),
                ('t23', 'p', 23, NULL, 0, 'ticket', 'Split user full name', 'ready_for_review',
                 'ready_for_review', 4, 'me', 2, 2, 'none')",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    for (id, title, state) in [
        ("t41", "Wishlist API", "working"),
        ("t23", "Split user full name", "ready_for_review"),
    ] {
        let row = store.ticket(id).await.unwrap().expect("the row is kept");
        assert_eq!(
            (row.title.as_str(), row.state.as_str()),
            (title, state),
            "{id}"
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM decisions")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(count, 0);

    exec(
        store.pool(),
        "INSERT INTO decisions (id, ticket_id, ordinal, question, built, other_options, why, recorded_by, recorded_at)
         VALUES ('d1', 't23', 1, 'q', 'b', '[\"o\"]', 'w', 'agent:SHOP-23', 5)",
    )
    .await;
    let row = sqlx::query("SELECT outcome, outcome_at FROM decisions WHERE id = 'd1'")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(row.get::<String, _>("outcome"), "open");
    assert_eq!(row.get::<Option<i64>, _>("outcome_at"), None);
    let refused = sqlx::query("UPDATE decisions SET outcome = 'maybe' WHERE id = 'd1'")
        .execute(store.pool())
        .await;
    assert!(refused.is_err(), "open, kept or changed_sent_back only");
    let duplicate = sqlx::query(
        "INSERT INTO decisions (id, ticket_id, ordinal, question, built, why, recorded_by, recorded_at)
         VALUES ('d2', 't23', 1, 'q', 'b', 'w', 'me', 6)",
    )
    .execute(store.pool())
    .await;
    assert!(duplicate.is_err(), "one number per ticket");
}

/// T-0309 (ADP-04, d0088): migration 0038 makes sessions tool-neutral on an M19 database (after 0037). An agent
/// session and a Project agent ran Claude Code and keep their ids; a Shell session runs no agent
/// tool; nothing passed a model. The index follows the rename, and a Project agent's tool id may now be empty.
#[tokio::test]
async fn sessions_and_project_agents_take_the_tool_and_keep_their_ids() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 37).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, kind, title, board_column, state, created_by, created_at,
                              updated_at)
         VALUES ('t18', 'p', 18, 'ticket', 'T', 'dev', 'working', 'me', 0, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO project_agents (id, project_id, name, name_source, claude_session_id, state, first_request,
                                     limit_stopped, created_at, ended_at, closed_at, epic_id)
         VALUES ('a1', 'p', 'Checkout slowness', 'agent', 'c-agent', 'ended', 'Why is checkout slow?', 1, 3, 4, 5,
                 NULL)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO sessions (id, project_id, ticket_id, kind, claude_session_id, state, started_at,
                               project_agent_id)
         VALUES ('s1', 'p', 't18', 'agent', 'c-ticket', 'ended', 5, NULL),
                ('s2', 'p', 't18', 'shell', NULL, 'ended', 6, NULL),
                ('s3', 'p', NULL, 'project_agent', 'c-agent', 'ended', 7, 'a1')",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    /// `(id, tool, tool_session_id, model, project_agent_id)`.
    type SessionRow = (
        String,
        Option<String>,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let sessions: Vec<SessionRow> = sqlx::query_as(
        "SELECT id, tool, tool_session_id, model, project_agent_id FROM sessions ORDER BY id",
    )
    .fetch_all(store.pool())
    .await
    .unwrap();
    let claude = || Some("claude".to_owned());
    assert_eq!(
        sessions,
        [
            ("s1".into(), claude(), Some("c-ticket".into()), None, None),
            ("s2".into(), None, None, None, None),
            (
                "s3".into(),
                claude(),
                Some("c-agent".into()),
                None,
                Some("a1".into())
            ),
        ]
    );
    let agents: Vec<(String, String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT id, tool, tool_session_id, model FROM project_agents ORDER BY id")
            .fetch_all(store.pool())
            .await
            .unwrap();
    assert_eq!(
        agents,
        [("a1".into(), "claude".into(), Some("c-agent".into()), None),]
    );
    let row = sqlx::query(
        "SELECT name, name_source, state, first_request, limit_stopped, created_at, ended_at, closed_at
         FROM project_agents WHERE id = 'a1'",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>(0), "Checkout slowness");
    assert_eq!(row.get::<String, _>(1), "agent");
    assert_eq!(row.get::<String, _>(2), "ended");
    assert_eq!(row.get::<String, _>(3), "Why is checkout slow?");
    assert_eq!(row.get::<i64, _>(4), 1);
    assert_eq!(
        (
            row.get::<i64, _>(5),
            row.get::<Option<i64>, _>(6),
            row.get::<Option<i64>, _>(7)
        ),
        (3, Some(4), Some(5))
    );
    let indexes: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND name IN
         ('sessions_tool_session', 'sessions_claude_session', 'project_agents_project', 'project_agents_epic')
         ORDER BY name",
    )
    .fetch_all(store.pool())
    .await
    .unwrap();
    assert_eq!(indexes, ["project_agents_project", "sessions_tool_session"]);
    // A tool that reports its id after start; the sessions still point at their agent.
    exec(
        store.pool(),
        "INSERT INTO project_agents (id, project_id, tool, state, created_at) VALUES ('a2', 'p', 'claude', 'running', 8)",
    )
    .await;
    exec(store.pool(), "DELETE FROM project_agents WHERE id = 'a1'").await;
    let agent: Option<String> =
        sqlx::query_scalar("SELECT project_agent_id FROM sessions WHERE id = 's3'")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(agent, None, "the foreign key still holds");
}

/// T-0304 (M19; d0085, d0086, d0087): migration 0041 (written as 0038, renumbered after M20's 0038–0040) gives
/// tickets their environment columns on an M20 database: existing tickets have their build in the worktree and no
/// ticket resource, and unknown states are refused.
#[tokio::test]
async fn ticket_env_columns_default_and_refuse_bad_values() {
    use otaq_core::domain::{BuildState, ResourceState};

    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 40).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 20, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, epic_id, child_number, kind, title, board_column, state,
                              started_at, created_by, created_at, updated_at)
         VALUES ('t18', 'p', 18, NULL, 0, 'ticket', 'Checkout API', 'ready_for_review', 'ready_for_review', 3,
                 'me', 1, 1),
                ('t19', 'p', 19, NULL, 0, 'ticket', 'Price table', 'backlog', 'backlog', NULL, 'me', 2, 2)",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    for id in ["t18", "t19"] {
        let row = store.ticket(id).await.unwrap().expect("the row is kept");
        assert_eq!(
            (row.build_state.as_str(), row.resource_state.as_str()),
            ("present", "none"),
            "{id}"
        );
        assert_eq!(
            (
                row.build_problem,
                row.resource_name,
                row.resource_problem,
                row.schema_overlap
            ),
            (None, None, None, None),
            "{id}"
        );
    }
    for bad in [
        "UPDATE tickets SET build_state = 'gone' WHERE id = 't18'",
        "UPDATE tickets SET resource_state = 'x' WHERE id = 't18'",
        "UPDATE tickets SET build_state = NULL WHERE id = 't18'",
    ] {
        let refused = sqlx::query(bad).execute(store.pool()).await;
        assert!(refused.is_err(), "{bad}");
    }

    store
        .set_build_state("t18", BuildState::Freed, Some("a file is open"))
        .await
        .unwrap();
    store
        .set_resource("t18", Some("shop_18"), ResourceState::Created, None)
        .await
        .unwrap();
    store
        .set_schema_overlap("t18", Some(r#"[{"other":"SHOP-19","path":"db/**"}]"#))
        .await
        .unwrap();
    let row = store.ticket("t18").await.unwrap().unwrap();
    assert_eq!(
        (
            row.build_state.as_str(),
            row.build_problem.as_deref(),
            row.resource_name.as_deref(),
            row.resource_state.as_str(),
            row.resource_problem.as_deref(),
            row.schema_overlap.as_deref(),
        ),
        (
            "freed",
            Some("a file is open"),
            Some("shop_18"),
            "created",
            None,
            Some(r#"[{"other":"SHOP-19","path":"db/**"}]"#)
        )
    );
}

/// T-0347 (M23; AGT-08, SCH-04; d0086, d0088): migration 0044 on an M22 database: the agent rows keep their values
/// and take the move columns, all empty; `agent_holds` starts empty and takes one hold per ticket or project and
/// role, with its reasons checked. No rows are copied (0088).
#[tokio::test]
async fn agent_rows_take_the_move_columns_and_holds_start_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 43).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, kind, title, board_column, state, created_by, created_at,
                              updated_at)
         VALUES ('t31', 'p', 31, 'ticket', 'Checkout flow', 'backlog', 'backlog', 'me', 0, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO named_agents (id, name, tool, model, use_for, created_at, updated_at)
         VALUES ('strong', 'Strong', 'claude', 'opus', '', 1, 1)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO ticket_agent_rows (ticket_id, role, named_agent_id, set_by, set_at)
         VALUES ('t31', 'developer', 'strong', 'user', 2)",
    )
    .await;
    pool.close().await;

    // Up to 0044 only: 0050 drops the rows (FLW-01).
    let store = database_at(&path, 44).await;
    let row = sqlx::query(
        "SELECT named_agent_id, set_by, set_at, moved_from_agent_id, moved_from_name, moved_reason, moved_tool,
                moved_at
         FROM ticket_agent_rows WHERE ticket_id = 't31'",
    )
    .fetch_one(&store)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>(0), "strong");
    assert_eq!(row.get::<String, _>(1), "user");
    assert_eq!(row.get::<i64, _>(2), 2);
    for column in 3..7 {
        assert_eq!(
            row.get::<Option<String>, _>(column),
            None,
            "column {column}"
        );
    }
    assert_eq!(row.get::<Option<i64>, _>(7), None);
    let holds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM agent_holds")
        .fetch_one(&store)
        .await
        .unwrap();
    assert_eq!(holds, 0);

    // The reasons are checked.
    let bad_move =
        sqlx::query("UPDATE ticket_agent_rows SET moved_reason = 'tired' WHERE ticket_id = 't31'")
            .execute(&store)
            .await;
    assert!(bad_move.is_err());
    exec(
        &store,
        "UPDATE ticket_agent_rows SET moved_from_agent_id = 'strong', moved_from_name = 'Strong',
                moved_reason = 'tool_limit', moved_tool = 'claude', moved_at = 3 WHERE ticket_id = 't31'",
    )
    .await;
    let bad_hold = sqlx::query(
        "INSERT INTO agent_holds (id, ticket_id, role, reason, tool, since)
         VALUES ('h0', 't31', 'developer', 'bored', 'claude', 4)",
    )
    .execute(&store)
    .await;
    assert!(bad_hold.is_err());
    exec(
        &store,
        "INSERT INTO agent_holds (id, ticket_id, project_id, role, reason, tool, model, named_agent_id, since)
         VALUES ('h1', 't31', NULL, 'developer', 'tool_limit', 'claude', 'opus', 'strong', 4),
                ('h2', NULL, 'p', 'project_agent', 'tool_unavailable', 'codex', NULL, NULL, 5)",
    )
    .await;
    let missing_scope = sqlx::query(
        "INSERT INTO agent_holds (id, role, reason, tool, since)
         VALUES ('h3', 'lead', 'tool_limit', 'claude', 6)",
    )
    .execute(&store)
    .await;
    assert!(missing_scope.is_err(), "a ticket's or a project's hold");
    let fallback_held: bool =
        sqlx::query_scalar("SELECT fallback_held FROM agent_holds WHERE id = 'h1'")
            .fetch_one(&store)
            .await
            .unwrap();
    assert!(!fallback_held);
}

/// T-0374 (ATT-17, d0061): migration 0045 adds `option_notes_json`; a question asked before it reads as no notes,
/// for its ticket and for the lead.
#[tokio::test]
async fn questions_asked_before_notes_read_as_no_notes() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 44).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, kind, title, board_column, state, created_by, created_at,
                              updated_at)
         VALUES ('t31', 'p', 31, 'ticket', 'Order confirmation email', 'dev', 'asks_you', 'me', 0, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO human_questions (id, ticket_id, question, options_json, recommended, asked_at)
         VALUES ('h1', 't31', 'Postmark or SES?', '[\"Postmark\",\"SES\"]', 1, 4)",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let stored: Option<String> =
        sqlx::query_scalar("SELECT option_notes_json FROM human_questions WHERE id = 'h1'")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(stored, None);
    let data = otaq_core::data_dir::DataDir::new(path.parent().unwrap().join("home"));
    let board = otaq_core::board::Board::new(store, None, data);
    let question = board.open_question("t31").await.unwrap().expect("open");
    assert_eq!(question.options, ["Postmark", "SES"]);
    assert!(question.notes.is_empty());
}

/// T-0400 (STG-04, STG-05; d0108): migration 0046 puts a ticket in each of today's 26 states (and an Interrupted or
/// limit-held validation) in its place and stage of the default flow, drops `board_column`, makes today's
/// validation rounds runs of `validate` and opens a run for each ticket in a stage; nothing else changes.
#[tokio::test]
async fn every_state_moves_to_its_place_and_stage() {
    use otaq_core::domain::{Place, Status};
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 45).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'pocket-notes', '/tmp/pocket-notes', 'NOTE', 'main', 41, 0)",
    )
    .await;
    // (state, interrupted_from, today's column) → (state, stage).
    type Row<'a> = (&'a str, Option<&'a str>, &'a str, &'a str, Option<&'a str>);
    let rows: &[Row] = &[
        ("backlog", None, "backlog", "backlog", None),
        ("parked", None, "backlog", "parked", None),
        ("epic_in_progress", None, "backlog", "backlog", None),
        ("queued", None, "ready_for_dev", "queued", None),
        (
            "waiting_for_slot",
            None,
            "ready_for_dev",
            "waiting_for_slot",
            None,
        ),
        ("held", None, "ready_for_dev", "held", None),
        (
            "parent_closed",
            None,
            "ready_for_dev",
            "parent_closed",
            None,
        ),
        (
            "start_interrupted",
            None,
            "ready_for_dev",
            "start_interrupted",
            None,
        ),
        ("working", None, "dev", "working", Some("dev")),
        (
            "working_in_background",
            None,
            "dev",
            "working_in_background",
            Some("dev"),
        ),
        ("asks_you", None, "dev", "asks_you", Some("dev")),
        ("needs_you", None, "dev", "needs_you", Some("dev")),
        ("idle", None, "dev", "idle", Some("dev")),
        (
            "waiting_for_limit_reset",
            None,
            "dev",
            "waiting_for_limit_reset",
            Some("dev"),
        ),
        (
            "interrupted",
            Some("idle"),
            "dev",
            "interrupted",
            Some("dev"),
        ),
        ("resume_failed", None, "dev", "resume_failed", Some("dev")),
        ("validating", None, "dev", "validating", Some("validate")),
        (
            "validator_needs_you",
            None,
            "dev",
            "validator_needs_you",
            Some("validate"),
        ),
        (
            "validation_failed",
            None,
            "dev",
            "validation_failed",
            Some("validate"),
        ),
        (
            "validation_no_result",
            None,
            "dev",
            "validation_no_result",
            Some("validate"),
        ),
        (
            "merge_conflict_fixing",
            None,
            "dev",
            "merge_conflict_fixing",
            Some("dev"),
        ),
        (
            "auto_merge_stopped",
            None,
            "dev",
            "auto_merge_stopped",
            Some("dev"),
        ),
        (
            "ready_for_review",
            None,
            "ready_for_review",
            "ready_for_review",
            Some("review"),
        ),
        ("pr_open", None, "approved", "pr_open", Some("deliver")),
        ("merged", None, "done", "merged", None),
        ("closed_not_merged", None, "done", "closed_not_merged", None),
        (
            "interrupted",
            Some("validating"),
            "dev",
            "interrupted",
            Some("validate"),
        ),
        (
            "waiting_for_limit_reset",
            Some("validating"),
            "dev",
            "waiting_for_limit_reset",
            Some("validate"),
        ),
    ];
    assert_eq!(
        rows.iter()
            .take(26)
            .map(|r| r.0)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        26,
        "each of today's 26 states once"
    );
    for (n, (state, from, column, _, _)) in rows.iter().enumerate() {
        sqlx::query(
            "INSERT INTO tickets (id, project_id, number, kind, title, board_column, state, interrupted_from,
                                  created_by, created_at, updated_at, state_since, validation_round)
             VALUES (?, 'p', ?, 'ticket', ?, ?, ?, ?, 'me', 1, 2, 3, 1)",
        )
        .bind(format!("t{}", n + 1))
        .bind(n as i64 + 1)
        .bind(format!("Ticket {state}"))
        .bind(*column)
        .bind(*state)
        .bind(*from)
        .execute(&pool)
        .await
        .unwrap();
    }
    exec(
        &pool,
        "INSERT INTO sessions (id, project_id, ticket_id, kind, tool, model, model_ran, named_agent_name, state,
                               started_at, ended_at)
         VALUES ('dev9', 'p', 't9', 'agent', 'claude', 'sonnet', 'claude-sonnet-5-5', 'Developer', 'running', 5,
                 NULL),
                ('val17', 'p', 't17', 'validator', 'claude', 'opus', NULL, 'Validator', 'running', 6, NULL)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO validations (id, ticket_id, round, trigger, session_id, started_at, ended_at, outcome, report)
         VALUES ('v1', 't17', 1, 'agent', NULL, 4, 5, 'fail', 'Missing test'),
                ('v2', 't17', 2, 'agent', 'val17', 6, NULL, NULL, NULL)",
    )
    .await;
    // Every column but the dropped one, as text, before the migration.
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('tickets')")
        .fetch_all(&pool)
        .await
        .unwrap();
    let kept: Vec<&String> = columns
        .iter()
        // 0051 drops the round counters too (each check counts its own, T-0417); 0066 the epic columns, and gives
        // every ticket a priority.
        .filter(|c| {
            !matches!(
                c.as_str(),
                "board_column"
                    | "state"
                    | "validation_round"
                    | "validation_rounds_extra"
                    | "epic_id"
                    | "child_number"
                    | "next_child_number"
                    | "kind"
                    | "epic_queue_mode"
                    | "epic_queue_mode_at"
                    | "joined_queue_by"
                    | "priority"
            )
        })
        .collect();
    let snapshot = |pool: SqlitePool| {
        let list = kept
            .iter()
            .map(|c| format!("COALESCE(CAST({c} AS TEXT), '∅')"))
            .collect::<Vec<_>>()
            .join(" || '|' || ");
        async move {
            sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(format!(
                "SELECT {list} FROM tickets ORDER BY number"
            )))
            .fetch_all(&pool)
            .await
            .unwrap()
        }
    };
    let before = snapshot(pool.clone()).await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let after: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT state, stage_id FROM tickets ORDER BY number")
            .fetch_all(store.pool())
            .await
            .unwrap();
    for ((state, from, _, to, stage), (got_state, got_stage)) in rows.iter().zip(&after) {
        assert_eq!(
            (got_state.as_str(), got_stage.as_deref()),
            (*to, *stage),
            "{state} (from {from:?})"
        );
    }
    let columns: Vec<String> = sqlx::query_scalar("SELECT name FROM pragma_table_info('tickets')")
        .fetch_all(store.pool())
        .await
        .unwrap();
    assert!(!columns.iter().any(|c| c == "board_column"), "{columns:?}");
    assert_eq!(
        snapshot(store.pool().clone()).await,
        before,
        "nothing else changes"
    );

    // The board reads each place.
    let data = otaq_core::data_dir::DataDir::new(path.parent().unwrap().join("home"));
    let board = otaq_core::board::Board::new(store.clone(), None, data);
    // 0050 stored the default flow with `agent:` on Validate, which d0130 no longer reads (no conversion, T-0525): an
    // error naming why, never a panic. The board reads the places once the flow is today's.
    let refused = board.tickets("NOTE").await.unwrap_err().to_string();
    assert!(
        refused.contains("an agent check lists its agents: `agents: [Validator]`"),
        "{refused}"
    );
    sqlx::query("UPDATE project_flows SET yaml = ?")
        .bind(naming::DEFAULT_FLOW_YAML)
        .execute(store.pool())
        .await
        .unwrap();
    let tickets = board.tickets("NOTE").await.unwrap();
    let place = |state: &str, n: usize| {
        let t = tickets
            .iter()
            .find(|t| t.title == format!("Ticket {state}") && t.key.number as usize == n)
            .unwrap();
        (t.place.clone(), t.status)
    };
    assert_eq!(place("parked", 2), (Place::Backlog, Status::Parked));
    assert_eq!(place("held", 6), (Place::Waiting(None), Status::Held));
    assert_eq!(
        place("working", 9),
        (Place::stage_of("dev"), Status::Working)
    );
    assert_eq!(
        place("validating", 17),
        (Place::stage_of("validate"), Status::Checking)
    );
    assert_eq!(
        place("ready_for_review", 23),
        (Place::stage_of("review"), Status::WaitingForYou)
    );
    assert_eq!(
        place("pr_open", 24),
        (Place::stage_of("deliver"), Status::PrOpen)
    );
    assert_eq!(place("merged", 25), (Place::Done, Status::Merged));
    assert_eq!(
        place("interrupted", 27),
        (Place::stage_of("validate"), Status::Interrupted)
    );

    // The rounds are runs of `validate`; tickets in a stage have an open run, with their live session.
    let runs = board.stage_runs("NOTE-17").await.unwrap();
    let runs: Vec<_> = runs
        .iter()
        .map(|r| {
            (
                r.stage.as_str(),
                r.kind.as_str(),
                r.round,
                r.result.map(|r| r.as_str()),
                r.session.as_deref(),
                r.model.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        runs,
        [
            ("validate", "check", Some(1), Some("failed"), None, None),
            (
                "validate",
                "check",
                Some(2),
                None,
                Some("val17"),
                Some("opus")
            )
        ]
    );
    let dev = board.stage_runs("NOTE-9").await.unwrap();
    assert_eq!(dev.len(), 1);
    assert_eq!(
        (
            dev[0].stage.as_str(),
            dev[0].session.as_deref(),
            dev[0].agent.as_deref(),
            dev[0].model.as_deref(),
            dev[0].result
        ),
        (
            "dev",
            Some("dev9"),
            Some("Developer"),
            Some("claude-sonnet-5-5"),
            None
        )
    );
    assert!(board.stage_runs("NOTE-1").await.unwrap().is_empty());
    assert!(board.stage_runs("NOTE-25").await.unwrap().is_empty());
    assert_eq!(
        board.stage_runs("NOTE-24").await.unwrap()[0].stage,
        "deliver"
    );
}

/// The example's M31 data (`tickets/m32-tickets.md`, "The example"), as M31's store wrote it: named agents Strong
/// (Claude Code · opus) and Light (Claude Code · haiku); the app's role defaults Developer = Light, Validator =
/// Strong, Epic lead = Strong, Project agent = Light (`agents.defaults`); the global Agent instructions, Project agent
/// instructions and imported Validate skill `team-validate` (settings rows); pocket-notes (NOTE, Validation on, 2
/// rounds, its own Agent instructions) and pocket-api (API, Validation off, its own Developer = Strong and its own
/// imported Commit skill); NOTE-1 in Review, NOTE-2 waiting for NOTE-1, NOTE-3 in Backlog with a Developer row set by
/// you (Strong), NOTE-4 in Dev on Light, API-1 in Dev, the epic NOTE-10 with NOTE-10.1 merged and NOTE-10.2 in Dev.
async fn m31_example(pool: &SqlitePool) {
    let notes = json!({
        "validation": true, "validation_rounds": 2, "agent_instructions": "use pnpm, not npm",
    });
    let api = json!({
        "validation": false,
        "agents": {"developer": "strong"},
        "workflow_skills": {"commit": {"id": "api-commit", "name": "api-commit", "original": "/team/api-commit",
                                       "imported_at": 7, "fingerprint": "f1"},
                            "push": "", "open_pr": "", "validate": ""},
    });
    for (id, name, key, settings) in [
        ("pn", "pocket-notes", "NOTE", notes),
        ("pa", "pocket-api", "API", api),
    ] {
        sqlx::query(
            "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at,
                                   settings_json)
             VALUES (?, ?, ?, ?, 'main', 11, 0, ?)",
        )
        .bind(id)
        .bind(name)
        .bind(format!("/tmp/{name}"))
        .bind(key)
        .bind(settings.to_string())
        .execute(pool)
        .await
        .unwrap();
    }
    exec(
        pool,
        "INSERT INTO named_agents (id, name, tool, model, use_for, created_at, updated_at)
         VALUES ('strong', 'Strong', 'claude', 'opus', 'Complex features, tricky bugs', 1, 1),
                ('light', 'Light', 'claude', 'haiku', 'Typos, docs, small config', 2, 2)",
    )
    .await;
    let team_validate = json!({"id": "team-validate", "name": "team-validate", "original": "/team/validate",
                               "imported_at": 5, "fingerprint": "f0", "warnings": []});
    for (key, value) in [
        (
            "agents.defaults",
            json!({"developer": "light", "validator": "strong", "epic_lead": "strong",
                   "project_agent": "light"}),
        ),
        ("agents.auto_fallback", json!(false)),
        (
            "agent_instructions",
            json!("Run `just test` before committing"),
        ),
        ("project_agent_instructions", json!("Keep tickets small")),
        (
            "workflow_skills",
            json!({"commit": "/otaq:commit", "push": "", "open_pr": "", "validate": team_validate}),
        ),
    ] {
        sqlx::query("INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, 0)")
            .bind(key)
            .bind(value.to_string())
            .execute(pool)
            .await
            .unwrap();
    }
    exec(
        pool,
        "INSERT INTO tickets (id, project_id, number, child_number, epic_id, kind, title, state, stage_id, created_by,
                              created_at, updated_at)
         VALUES ('n1', 'pn', 1, 0, NULL, 'ticket', 'Sort notes by date', 'ready_for_review', 'review', 'me', 0, 0),
                ('n2', 'pn', 2, 0, NULL, 'ticket', 'Note colors', 'queued', NULL, 'me', 0, 0),
                ('n3', 'pn', 3, 0, NULL, 'ticket', 'Fix README typo', 'backlog', NULL, 'me', 0, 0),
                ('n4', 'pn', 4, 0, NULL, 'ticket', 'Search', 'queued', NULL, 'me', 0, 0),
                ('a1', 'pa', 1, 0, NULL, 'ticket', 'Rate limits', 'working', 'dev', 'me', 0, 0),
                ('n10', 'pn', 10, 0, NULL, 'epic', 'Sharing', 'backlog', NULL, 'me', 0, 0),
                ('n10a', 'pn', 10, 1, 'n10', 'ticket', 'Share link', 'merged', NULL, 'me', 0, 0),
                ('n10b', 'pn', 10, 2, 'n10', 'ticket', 'Share page', 'working', 'dev', 'me', 0, 0)",
    )
    .await;
    exec(
        pool,
        "INSERT INTO ticket_deps (ticket_id, depends_on_id) VALUES ('n2', 'n1')",
    )
    .await;
    exec(
        pool,
        "INSERT INTO ticket_agent_rows (ticket_id, role, named_agent_id, set_by, set_at)
         VALUES ('n3', 'developer', 'strong', 'user', 3)",
    )
    .await;
    // NOTE-4's Dev session on Light (M22 stored the named agent on it; 0046's trigger copied it onto its run).
    exec(
        pool,
        "INSERT INTO sessions (id, project_id, ticket_id, kind, tool, tool_session_id, model, model_ran,
                               named_agent_id, named_agent_name, state, started_at)
         VALUES ('s4', 'pn', 'n4', 'agent', 'claude', 'c-4', 'haiku', 'claude-haiku-5-5', 'light', 'Light',
                 'running', 5)",
    )
    .await;
    // Its start: the ticket enters Dev, and its run takes the live session (0046's trigger).
    exec(
        pool,
        "UPDATE tickets SET state = 'working', stage_id = 'dev' WHERE id = 'n4'",
    )
    .await;
}

/// A project's stored settings.
async fn project_json(store: &Store, key: &str) -> serde_json::Value {
    let json: String = sqlx::query_scalar("SELECT settings_json FROM projects WHERE key = ?")
        .bind(key)
        .fetch_one(store.pool())
        .await
        .unwrap();
    serde_json::from_str(&json).unwrap()
}

/// A table's presence.
async fn has_table(pool: &SqlitePool, name: &str) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
    )
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Every agent and skill row, as text: what a second start must leave as it is.
async fn agent_rows(store: &Store) -> Vec<String> {
    let mut rows = Vec::new();
    for sql in [
        "SELECT id || '|' || name || '|' || came_from || '|' || updated_at FROM agents ORDER BY id",
        "SELECT agent_id || '|' || position || '|' || COALESCE(shipped, imported_id) FROM agent_skills
         ORDER BY agent_id, position",
    ] {
        let found: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
            .fetch_all(store.pool())
            .await
            .unwrap();
        rows.extend(found);
    }
    rows
}

/// A column's presence.
async fn has_column(pool: &SqlitePool, table: &str, column: &str) -> bool {
    sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM pragma_table_info(?) WHERE name = ?)")
        .bind(table)
        .bind(column)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// T-0412 (FLW-01; d0108, no migration from M32): an M31 database's data isn't moved over. The first start installs
/// `otaq-agents.yaml` and the default flow's agents and nothing else: M31's named agents, role defaults,
/// instructions, workflow skills and ticket rows are gone, the Validation settings with them, and every project is on
/// the default flow as its version 1. A second start changes nothing.
#[tokio::test]
async fn an_m31_database_starts_clean_with_the_shipped_agents() {
    use otaq_core::agents::{AgentModel, CameFrom};
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 46).await;
    m31_example(&pool).await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let agents = store.agents().await.unwrap();
    let shown: Vec<(&str, &CameFrom, bool)> = agents
        .iter()
        .map(|a| {
            (
                a.name.as_str(),
                &a.came_from,
                a.model == AgentModel::Default,
            )
        })
        .collect();
    assert_eq!(
        shown,
        [
            ("Noyan", &CameFrom::AgentsFile, true),
            ("Developer", &CameFrom::DefaultPreset, true),
            ("Validator", &CameFrom::DefaultPreset, true),
        ]
    );
    assert!(
        agents
            .iter()
            .all(|a| a.instructions.is_empty() && a.imported.is_empty())
    );
    assert_eq!(store.default_model().await.unwrap(), None);
    assert!(!has_table(store.pool(), "named_agents").await);
    assert!(!has_table(store.pool(), "ticket_agent_rows").await);
    for key in ["NOTE", "API"] {
        let json = project_json(&store, key).await;
        for gone in ["validation", "validation_rounds", "auto_merge"] {
            assert!(json.get(gone).is_none(), "{key}: {gone}");
        }
        let flow = store.current_flow(key).await.unwrap().expect("a flow");
        assert_eq!(
            (flow.version, flow.yaml.as_str(), flow.preset.as_deref()),
            (
                1,
                default_as_0050_stored().as_str(),
                Some("otaq-default.yaml")
            ),
            "{key}"
        );
    }

    let before = agent_rows(&store).await;
    store.close().await;
    let store = Store::open(&path).await.unwrap();
    assert_eq!(agent_rows(&store).await, before);
}

/// The default flow as 0050 stored it for the projects already there: before T-0415 named `open-pr` on its Delivery
/// (Q254), which changes nothing (the PR path runs `open-pr` when the Delivery names no skill), and before d0130's
/// `agents:` on an agent check (no conversion: such a stored flow no longer reads, T-0525).
fn default_as_0050_stored() -> String {
    let stored = naming::DEFAULT_FLOW_YAML
        .replace("    skill: open-pr\n    mode: click", "    mode: click")
        .replace("    agents: [Validator]\n", "    agent: Validator\n");
    assert_ne!(stored, naming::DEFAULT_FLOW_YAML);
    stored
}

/// T-0412 (FLW-01, Q296): 0050 drops what M32's move-over and the old settings kept: the Validation, Validation
/// rounds and Auto-merge settings (global and per project), the move-over's record and the names it bound,
/// `agent_skills.stands_for`; it gives every project the default flow as version 1, and leaves everything else.
#[tokio::test]
async fn migration_0050_drops_the_old_settings_and_gives_each_project_its_flow() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 49).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at, settings_json)
         VALUES ('pn', 'pocket-notes', '/tmp/pocket-notes', 'NOTE', 'main', 1, 0,
                 '{\"validation\": true, \"validation_rounds\": 2, \"auto_merge\": \"all\", \"project_notes\": \"pnpm\"}')",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO settings (key, value_json, updated_at)
         VALUES ('validation', 'false', 0), ('validation_rounds', '4', 0), ('auto_merge', '\"epic_children\"', 0),
                ('agents.moved_over', '{\"at\": 1}', 0), ('agents.flow_names', '{\"Developer\": \"d\"}', 0),
                ('theme', '\"dracula\"', 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO agents (id, name, came_from, created_at, updated_at)
         VALUES ('v', 'Validator', 'default_preset', 0, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO agent_skills (agent_id, position, imported_id, name, original, imported_at, fingerprint,
                                   stands_for)
         VALUES ('v', 0, 'team-validate', 'team-validate', '/team/validate', 1, 'f', 'validate')",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    for key in [
        "validation",
        "validation_rounds",
        "auto_merge",
        "agents.moved_over",
        "agents.flow_names",
    ] {
        assert_eq!(store.setting(key).await.unwrap(), None, "{key}");
    }
    assert_eq!(
        store.setting("theme").await.unwrap().as_deref(),
        Some("\"dracula\"")
    );
    assert_eq!(
        project_json(&store, "NOTE").await,
        json!({"project_notes": "pnpm"})
    );
    assert!(!has_column(store.pool(), "agent_skills", "stands_for").await);
    let validator = store.agent("v").await.unwrap().expect("kept");
    assert_eq!(validator.imported[0].name, "team-validate");
    let versions = store.flow_versions("NOTE").await.unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].yaml, default_as_0050_stored());
    assert_eq!(versions[0].applied_by, otaq_core::flow::AppliedBy::Shipped);
    assert!(versions[0].current);
}

/// T-0405 (AGM-02): a fresh install applies the two files; the default model waits for the first connected tool.
#[tokio::test]
async fn a_fresh_install_applies_the_two_files() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let store = Store::open(&path).await.unwrap();
    let agents = store.agents().await.unwrap();
    let names: Vec<(&str, bool)> = agents
        .iter()
        .map(|a| {
            (
                a.name.as_str(),
                a.model == otaq_core::agents::AgentModel::Default,
            )
        })
        .collect();
    assert_eq!(
        names,
        [("Noyan", true), ("Developer", true), ("Validator", true)]
    );
    assert_eq!(store.default_model().await.unwrap(), None);
    assert!(!has_table(store.pool(), "named_agents").await);
}

/// T-0433 (ASK-03, ASK-04): 0054 keeps the open proposals with no tool, lets a project hold several, goes on
/// numbering from where it was (numbers are never reused), and starts every project outside Flow setup.
#[tokio::test]
async fn migration_0054_keeps_the_proposals_and_their_numbers() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 53).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('pn', 'pocket-notes', '/tmp/pocket-notes', 'NOTE', 'main', 1, 0)",
    )
    .await;
    // Proposals 1 to 3 were opened; 2 is open, 1 and 3 were dropped (one open at a time).
    for id in [1, 3, 2] {
        exec(
            &pool,
            &format!(
                "INSERT INTO flow_proposals (id, project_id, yaml, source_json, proposed_by_json, created_at)
                 VALUES ({id}, 'pn', 'flow: []', '{{\"kind\": \"preset\", \"file\": \"x.yaml\"}}',
                         '{{\"by\": \"user\"}}', {id})"
            ),
        )
        .await;
        if id != 2 {
            exec(
                &pool,
                &format!("DELETE FROM flow_proposals WHERE id = {id}"),
            )
            .await;
        }
    }
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let pool = store.pool();
    let kept: Vec<(i64, Option<String>, String)> =
        sqlx::query_as("SELECT id, tool, proposed_by_json FROM flow_proposals")
            .fetch_all(pool)
            .await
            .unwrap();
    assert_eq!(kept, [(2, None, "{\"by\": \"user\"}".to_owned())]);
    let next: i64 = sqlx::query_scalar(
        "INSERT INTO flow_proposals (project_id, tool, yaml, source_json, proposed_by_json, created_at)
         VALUES ('pn', 'opencode', 'flow: []', '{}', '{}', 4) RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(
        next, 4,
        "a second open proposal of the project, the next number"
    );
    let since: Option<i64> =
        sqlx::query_scalar("SELECT flow_setup_since FROM projects WHERE key = 'NOTE'")
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(since, None);
}

/// T-0455 (PAU-01, PAU-03; d0112): migration 0057 starts every project unpaused and rebuilds `project_agents` so an
/// agent can be `paused`, keeping every row, column and the sessions and questions that point at it.
#[tokio::test]
async fn projects_start_unpaused_and_project_agents_can_be_paused() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 56).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, kind, title, state, created_by, created_at, updated_at)
         VALUES ('t40', 'p', 40, 'ticket', 'Wishlist', 'backlog', 'me', 0, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO project_agents (id, project_id, name, name_source, tool, tool_session_id, model, state,
                                     first_request, limit_stopped, created_at, ended_at, closed_at, epic_id,
                                     agent_id, agent_name, model_ran, effort)
         VALUES ('a1', 'p', 'Import GitHub issues', 'agent', 'claude', 'c-agent', 'opus', 'ended_by_restart',
                 'Import the issues', 1, 3, 4, NULL, NULL, 'ag', 'Noyan', 'opus', 'high')",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO sessions (id, project_id, kind, tool, tool_session_id, state, started_at, project_agent_id)
         VALUES ('s1', 'p', 'project_agent', 'claude', 'c-agent', 'ended', 5, 'a1')",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let paused: Option<i64> =
        sqlx::query_scalar("SELECT paused_since FROM projects WHERE id = 'p'")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(paused, None);
    let rows = sqlx::query("SELECT * FROM project_agents ORDER BY id")
        .fetch_all(store.pool())
        .await
        .unwrap();
    let a1 = &rows[0];
    assert_eq!(
        (
            a1.get::<String, _>("name"),
            a1.get::<String, _>("state"),
            a1.get::<String, _>("agent_name"),
            a1.get::<String, _>("effort"),
            a1.get::<i64, _>("limit_stopped"),
        ),
        (
            "Import GitHub issues".to_owned(),
            "ended_by_restart".to_owned(),
            "Noyan".to_owned(),
            "high".to_owned(),
            1
        )
    );
    exec(
        store.pool(),
        "UPDATE project_agents SET state = 'paused' WHERE id = 'a1'",
    )
    .await;
    let session: Option<String> =
        sqlx::query_scalar("SELECT project_agent_id FROM sessions WHERE id = 's1'")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(session.as_deref(), Some("a1"));
}

/// T-0491 (d0120): migration 0059 adds `tickets.merge_check_due` (off), keeping every row.
#[tokio::test]
async fn merge_check_due_starts_off() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 58).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, epic_id, child_number, kind, title, state, created_by,
                              created_at, updated_at)
         VALUES ('t402', 'p', 42, NULL, 0, 'ticket', 'Wishlist button', 'working', 'me', 1, 1)",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let due: bool = sqlx::query_scalar("SELECT merge_check_due FROM tickets WHERE id = 't402'")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert!(!due);
}

/// T-0517 (d0128, d0108; Q511): migration 0063 stores the Project agent role as `noyan`. An agent holding
/// `project-agent` holds `noyan` and its shipped skill reads `noyan`; its skills and the stage choices that name it
/// stay (the rebuild cascades nothing); one agent still holds the role, and `project-agent` is refused.
#[tokio::test]
async fn the_project_agent_role_is_stored_as_noyan() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 62).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, kind, title, state, created_by, created_at, updated_at)
         VALUES ('t40', 'p', 40, 'ticket', 'Wishlist', 'backlog', 'me', 0, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO agents (id, name, instructions, tool, model, role, came_from, created_at, updated_at)
         VALUES ('ag', 'Noyan', 'Split small.', 'claude', 'opus', 'project-agent', 'agents_file', 1, 2),
                ('el', 'Epic lead', '', NULL, NULL, 'epic-lead', 'agents_file', 1, 1),
                ('dev', 'Developer', '', NULL, NULL, NULL, 'default_preset', 1, 1)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO agent_skills (agent_id, position, shipped)
         VALUES ('ag', 0, 'project-agent'), ('ag', 1, 'flow'), ('dev', 0, 'work-ticket')",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO ticket_stage_choices (ticket_id, stage_id, choice, agent_id, set_by, set_at)
         VALUES ('t40', 'dev', 'agent', 'ag', 'user', 3)",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let agents: Vec<(String, String, Option<String>, String)> =
        sqlx::query_as("SELECT id, name, role, instructions FROM agents ORDER BY id")
            .fetch_all(store.pool())
            .await
            .unwrap();
    assert_eq!(
        agents,
        [
            (
                "ag".into(),
                "Noyan".into(),
                Some("noyan".into()),
                "Split small.".into()
            ),
            ("dev".into(), "Developer".into(), None, String::new()),
            // Migration 0066: the `epic-lead` role is gone (d0133); the agent stays.
            ("el".into(), "Epic lead".into(), None, String::new()),
        ]
    );
    let skills: Vec<(String, String)> =
        sqlx::query_as("SELECT agent_id, shipped FROM agent_skills ORDER BY agent_id, position")
            .fetch_all(store.pool())
            .await
            .unwrap();
    assert_eq!(
        skills,
        [
            ("ag".into(), "noyan".into()),
            ("ag".into(), "flow".into()),
            ("dev".into(), "work-ticket".into()),
        ]
    );
    let choice: String =
        sqlx::query_scalar("SELECT agent_id FROM ticket_stage_choices WHERE ticket_id = 't40'")
            .fetch_one(store.pool())
            .await
            .unwrap();
    assert_eq!(choice, "ag");
    let second = sqlx::query("UPDATE agents SET role = 'noyan' WHERE id = 'dev'")
        .execute(store.pool())
        .await;
    assert!(second.is_err(), "one agent holds the role");
    let old = sqlx::query("UPDATE agents SET role = 'project-agent' WHERE id = 'dev'")
        .execute(store.pool())
        .await;
    assert!(old.is_err(), "the old key is refused");
}

/// T-0533 (d0133, d0059, d0108): migration 0066 removes the epic type and the lead. Epics, their children and their
/// leads go (no data is carried over); a ticket keeps its row, its External ID and its priority (never NULL again,
/// Normal for one that had none) and gets an empty Plan; the epic columns, the key aliases and the lead's tables are
/// gone; the `epic-lead` role is refused and its holder keeps its row without it.
#[tokio::test]
async fn one_card_drops_the_epic_type_and_the_lead() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("otaq.db");
    let pool = database_at(&path, 65).await;
    exec(
        &pool,
        "INSERT INTO projects (id, name, repo_path, key, base_branch, next_ticket_number, created_at)
         VALUES ('p', 'shop-web', '/tmp/shop-web', 'SHOP', 'main', 41, 0)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO tickets (id, project_id, number, epic_id, child_number, kind, title, state, created_by,
                              created_at, updated_at, priority, external_id)
         VALUES ('t18', 'p', 18, NULL, 0, 'ticket', 'Checkout', 'backlog', 'me', 0, 0, 2, 'PAY-7'),
                ('t19', 'p', 19, NULL, 0, 'ticket', 'Cart', 'backlog', 'me', 0, 0, NULL, NULL),
                ('t40', 'p', 40, NULL, 0, 'epic', 'Wishlist', 'backlog', 'me', 0, 0, 3, 'PAY-123'),
                ('t401', 'p', 40, 't40', 1, 'ticket', 'Wishlist API', 'backlog', 'me', 0, 0, NULL, NULL)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO ticket_deps (ticket_id, depends_on_id) VALUES ('t19', 't18'), ('t18', 't401')",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO project_agents (id, project_id, tool, state, created_at, epic_id)
         VALUES ('l40', 'p', 'claude', 'ended', 1, 't40'), ('pa', 'p', 'claude', 'ended', 2, NULL)",
    )
    .await;
    exec(
        &pool,
        "INSERT INTO agents (id, name, instructions, role, came_from, created_at, updated_at)
         VALUES ('el', 'Epic lead', '', 'epic-lead', 'agents_file', 1, 1)",
    )
    .await;
    pool.close().await;

    let store = Store::open(&path).await.unwrap();
    let rows: Vec<(String, i64, Option<String>, String)> =
        sqlx::query_as("SELECT id, priority, external_id, plan FROM tickets ORDER BY id")
            .fetch_all(store.pool())
            .await
            .unwrap();
    assert_eq!(
        rows,
        [
            ("t18".into(), 2, Some("PAY-7".into()), String::new()),
            ("t19".into(), 3, None, String::new()),
        ]
    );
    let deps: Vec<(String, String)> =
        sqlx::query_as("SELECT ticket_id, depends_on_id FROM ticket_deps")
            .fetch_all(store.pool())
            .await
            .unwrap();
    assert_eq!(deps, [("t19".to_owned(), "t18".to_owned())]);
    let agents: Vec<String> = sqlx::query_scalar("SELECT id FROM project_agents")
        .fetch_all(store.pool())
        .await
        .unwrap();
    assert_eq!(agents, ["pa"]);
    for column in [
        "epic_id",
        "child_number",
        "next_child_number",
        "kind",
        "epic_queue_mode",
        "epic_queue_mode_at",
        "joined_queue_by",
    ] {
        assert!(
            !has_column(store.pool(), "tickets", column).await,
            "{column}"
        );
    }
    for column in ["plan", "plan_by", "plan_at", "external_id", "priority"] {
        assert!(
            has_column(store.pool(), "tickets", column).await,
            "{column}"
        );
    }
    assert!(!has_column(store.pool(), "project_agents", "epic_id").await);
    for column in [
        "lead_id",
        "lead_epic_id",
        "lead_state",
        "lead_reason",
        "lead_at",
    ] {
        assert!(
            !has_column(store.pool(), "human_questions", column).await,
            "{column}"
        );
    }
    for table in [
        "ticket_key_aliases",
        "lead_notes",
        "lead_stuck",
        "lead_suggestions",
    ] {
        assert!(!has_table(store.pool(), table).await, "{table}");
    }
    let role: Option<String> = sqlx::query_scalar("SELECT role FROM agents WHERE id = 'el'")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(role, None);
    let refused = sqlx::query("UPDATE agents SET role = 'epic-lead' WHERE id = 'el'")
        .execute(store.pool())
        .await;
    assert!(refused.is_err(), "only noyan");
    let null = sqlx::query("UPDATE tickets SET priority = NULL WHERE id = 't18'")
        .execute(store.pool())
        .await;
    assert!(null.is_err(), "priority is never NULL");
    exec(
        store.pool(),
        "INSERT INTO sessions (id, project_id, kind, state, started_at) VALUES ('s', 'p', 'project', 'running', 0)",
    )
    .await;
    let lead_wake = sqlx::query(
        "INSERT INTO pending_deliveries (id, session_id, kind, text, created_at)
         VALUES ('d', 's', 'lead_wake', 'x', 0)",
    )
    .execute(store.pool())
    .await;
    assert!(lead_wake.is_err(), "the lead's deliveries are gone");
}
