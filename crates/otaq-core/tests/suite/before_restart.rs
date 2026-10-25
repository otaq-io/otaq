//! "Before the restart" (T-0113, REC-02; d0045, d0020): the last messages and actions of an Interrupted ticket's
//! conversation, read read-only from a Claude Code folder. The folder is a fixture in a temporary directory with
//! real-shaped JSONL records; the real `~/.claude` is never read.

use std::path::Path;

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::claude::transcript::{last_entries, project_folder};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::protocol::{BeforeRestartParams, Reply, Request};
use otaq_core::recovery::{BeforeRestart, TranscriptEntry};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;

const SESSION: &str = "0199a1b2-3c4d-7e5f-8a9b-0c1d2e3f4a5b";

/// A conversation as Claude Code 2.1.296 writes it (trimmed to the fields that matter), with the records the
/// reader must skip: bookkeeping types, thinking, tool results, a helper agent's record, a meta record, a broken
/// line.
fn fixture() -> String {
    [
        r#"{"type":"permission-mode","permissionMode":"default","sessionId":"S"}"#,
        r#"{"type":"user","message":{"role":"user","content":"<command-message>otaq:work-ticket</command-message>\n<command-name>/otaq:work-ticket</command-name>\n<command-args></command-args>"},"isSidechain":false,"timestamp":"2026-10-09T12:00:00.000Z","sessionId":"S","uuid":"u1"}"#,
        r#"{"type":"user","message":{"role":"user","content":"<system-reminder>ignore me</system-reminder>"},"isMeta":true,"timestamp":"2026-10-09T12:00:00.500Z"}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"hmm","signature":"x"}]},"timestamp":"2026-10-09T12:00:01.000Z"}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"I'll upgrade Next.js and run the tests."}]},"timestamp":"2026-10-09T12:00:02.000Z"}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"pnpm add next@15","description":"Upgrade"}}]},"timestamp":"2026-10-09T12:00:03.000Z"}"#,
        r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","content":"done"}]},"toolUseResult":{"stdout":"done"},"timestamp":"2026-10-09T12:00:04.000Z"}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"subagent noise"}]},"isSidechain":true,"timestamp":"2026-10-09T12:00:04.500Z"}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t2","name":"Edit","input":{"file_path":"/wt/next.config.js","old_string":"a","new_string":"b"}}]},"timestamp":"2026-10-09T12:00:05.000Z"}"#,
        r#"{"type":"file-history-snapshot","messageId":"m","snapshot":{}}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":"#,
        r#"{"type":"user","message":{"role":"user","content":"Keep the old image loader for now."},"timestamp":"2026-10-09T12:00:06.000Z"}"#,
        r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Running the test suite now."},{"type":"tool_use","id":"t3","name":"Bash","input":{"command":"pnpm test"}}]},"timestamp":"2026-10-09T12:00:07.000Z"}"#,
        r#"{"type":"something-new","message":{"content":[{"type":"text","text":"unknown"}]}}"#,
    ]
    .join("\n")
}

fn expected() -> Vec<TranscriptEntry> {
    let at = |s: i64| Some(1_791_547_200_000 + s * 1000);
    vec![
        TranscriptEntry::User {
            text: "/otaq:work-ticket".into(),
            at: at(0),
        },
        TranscriptEntry::Assistant {
            text: "I'll upgrade Next.js and run the tests.".into(),
            at: at(2),
        },
        TranscriptEntry::Action {
            tool: "Bash".into(),
            summary: "pnpm add next@15".into(),
            at: at(3),
        },
        TranscriptEntry::Action {
            tool: "Edit".into(),
            summary: "/wt/next.config.js".into(),
            at: at(5),
        },
        TranscriptEntry::User {
            text: "Keep the old image loader for now.".into(),
            at: at(6),
        },
        TranscriptEntry::Assistant {
            text: "Running the test suite now.".into(),
            at: at(7),
        },
        TranscriptEntry::Action {
            tool: "Bash".into(),
            summary: "pnpm test".into(),
            at: at(7),
        },
    ]
}

#[test]
fn records_become_messages_and_actions_and_the_rest_is_skipped() {
    assert_eq!(last_entries(&fixture(), 40), expected());
    assert_eq!(last_entries(&fixture(), 2), expected()[5..].to_vec());
    assert!(last_entries("not json\n{}\n[]", 40).is_empty());
}

/// Every file under `dir` with its size and modification time.
fn listing(dir: &Path) -> Vec<(String, u64, std::time::SystemTime)> {
    let mut all = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(at) = stack.pop() {
        for entry in std::fs::read_dir(&at).unwrap().flatten() {
            let meta = entry.metadata().unwrap();
            if meta.is_dir() {
                stack.push(entry.path());
            }
            all.push((
                entry.path().display().to_string(),
                meta.len(),
                meta.modified().unwrap(),
            ));
        }
    }
    all.sort();
    all
}

async fn ask(client: &Client, key: &str, limit: Option<u32>) -> BeforeRestart {
    let reply = client
        .request(Request::SessionBeforeRestart(BeforeRestartParams {
            key: key.into(),
            limit,
        }))
        .await
        .unwrap();
    let Reply::BeforeRestart(view) = reply else {
        panic!("{reply:?}")
    };
    *view
}

#[tokio::test(flavor = "multi_thread")]
async fn the_view_reads_the_stored_conversation_or_says_it_is_gone() {
    let claude = tempfile::TempDir::new().unwrap();
    let claude_dir = claude.path().to_owned();
    let w = TestService::start_with(move |mut config: ServiceConfig| {
        config.tools.claude_config_dir = Some(claude_dir);
        config
    })
    .await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Upgrade to Next.js 15").await;
    let shop2 = w.create("SHOP", "Coupon codes").await;
    let shop3 = w.create("SHOP", "Never started").await;
    let worktree = w.give_worktree(&shop1).await;
    w.record_ended_agent(&shop1, SESSION).await;
    w.give_worktree(&shop2).await;
    w.record_ended_agent(&shop2, "0199ffff-0000-7000-8000-000000000000")
        .await;
    let _ = shop3;

    let folder = claude
        .path()
        .join("projects")
        .join(project_folder(&worktree));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(format!("{SESSION}.jsonl")), fixture()).unwrap();
    let before = listing(claude.path());

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    assert_eq!(
        ask(&client, "SHOP-1", None).await,
        BeforeRestart::Available {
            tool: Tool::Claude,
            tool_session_id: SESSION.into(),
            entries: expected(),
        }
    );
    let BeforeRestart::Available { entries, .. } = ask(&client, "SHOP-1", Some(3)).await else {
        panic!("available")
    };
    assert_eq!(entries, expected()[4..].to_vec());

    // Claude Code removed the conversation (its own cleanup): not available, with the ID for History.
    assert_eq!(
        ask(&client, "SHOP-2", None).await,
        BeforeRestart::NotAvailable {
            tool: Some(Tool::Claude),
            tool_session_id: Some("0199ffff-0000-7000-8000-000000000000".into()),
        }
    );
    // No session at all.
    assert_eq!(
        ask(&client, "SHOP-3", None).await,
        BeforeRestart::NotAvailable {
            tool: None,
            tool_session_id: None,
        }
    );

    // A conversation filed under another folder (long paths get a hash suffix) is still found.
    std::fs::rename(
        &folder,
        claude.path().join("projects").join("-long-path-1a2b3c"),
    )
    .unwrap();
    assert!(matches!(
        ask(&client, "SHOP-1", None).await,
        BeforeRestart::Available { .. }
    ));
    std::fs::rename(
        claude.path().join("projects").join("-long-path-1a2b3c"),
        &folder,
    )
    .unwrap();

    // An unreadable file is "not available", never an error.
    let file = folder.join(format!("{SESSION}.jsonl"));
    std::fs::write(&file, [0xff, 0xfe, b'\n', b'{']).unwrap();
    assert!(matches!(
        ask(&client, "SHOP-1", None).await,
        BeforeRestart::Available { entries, .. } if entries.is_empty()
    ));
    std::fs::write(&file, fixture()).unwrap();

    // Read-only: nothing in Claude Code's folder changed (the test's own edits above are undone).
    let after: Vec<_> = listing(claude.path())
        .into_iter()
        .map(|(p, len, _)| (p, len))
        .collect();
    let before: Vec<_> = before.into_iter().map(|(p, len, _)| (p, len)).collect();
    assert_eq!(after, before);
    w.stop().await;
}

/// A test service whose test names no Claude Code folder reads an empty one in the test's own folder, never the
/// user's (M34's guard, kept when `otaq history` went in T-0437): nothing until a conversation is written there.
#[tokio::test(flavor = "multi_thread")]
async fn a_test_service_reads_an_empty_claude_folder_of_its_own() {
    let w = TestService::start_with(|config| config).await;
    let dir = w.claude_dir();
    assert!(dir.starts_with(w.home.path()), "{}", dir.display());
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Upgrade to Next.js 15").await;
    let worktree = w.give_worktree(&shop1).await;
    w.record_ended_agent(&shop1, SESSION).await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    assert!(matches!(
        ask(&client, "SHOP-1", None).await,
        BeforeRestart::NotAvailable { .. }
    ));

    let folder = dir.join("projects").join(project_folder(&worktree));
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(folder.join(format!("{SESSION}.jsonl")), fixture()).unwrap();
    assert!(matches!(
        ask(&client, "SHOP-1", None).await,
        BeforeRestart::Available { .. }
    ));
    w.stop().await;
}
