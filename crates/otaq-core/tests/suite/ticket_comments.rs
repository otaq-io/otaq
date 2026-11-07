//! Ticket comments and reports (T-0442; CMT-01…CMT-04, CMT-06, CMT-14; d0109, d0060): the 2,000-character limit, N
//! per ticket never reused, the three History events, reports copied by streaming with their size and lines (20 MB
//! accepted, one byte more refused with nothing left behind), kept on a comment's delete and removed with the ticket;
//! and who may write, through the socket from inside sessions (an ordinary `sh`, never an agent tool). T-0474 (RPT-02,
//! RPT-03): a copied report keeps its file's name; a ticket created with a report has it as its first comment.

use std::time::Duration;

use otaq_core::board::Refusal;
use otaq_core::board::reports::{AttachmentKind, REPORT_LIMIT_BYTES, ReportSource};
use otaq_core::board::ticket_comments::{
    COMMENT_LIMIT_CHARS, CommentAuthor, FirstComment, TicketComment,
};
use otaq_core::board::tickets::{EventKind, HistoryEvent, NewTicketInput};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Actor;
use otaq_core::protocol::{Event, Reply, Request, TicketCommentPostParams};
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use serde_json::{Value, json};

fn me() -> Actor {
    Actor::User("nevzat".into())
}

fn you() -> CommentAuthor {
    CommentAuthor::User {
        name: "nevzat".into(),
    }
}

fn rust_developer() -> CommentAuthor {
    CommentAuthor::Agent {
        id: Some("rust-dev".into()),
        name: Some("Rust developer".into()),
        tool: None,
        stage: Some("dev".into()),
        round: None,
    }
}

async fn post(w: &TestService, key: &str, body: &str) -> TicketComment {
    w.board
        .post_ticket_comment(key, &you(), body, Vec::new(), &me())
        .await
        .unwrap()
}

/// The ticket's comment events, oldest first.
async fn comment_events(w: &TestService, key: &str) -> Vec<HistoryEvent> {
    let mut events: Vec<HistoryEvent> = w
        .board
        .history(key)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::CommentPosted | EventKind::CommentEdited | EventKind::CommentDeleted
            )
        })
        .collect();
    events.reverse();
    events
}

/// Every file under the data folder's `reports/`.
fn report_files(w: &TestService) -> Vec<std::path::PathBuf> {
    fn walk(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&w.data.reports_dir(), &mut out);
    out
}

#[tokio::test(flavor = "multi_thread")]
async fn the_limit_counts_characters_and_numbers_are_never_reused() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Export orders as CSV").await;

    // 2,000 characters of two-byte text are accepted; 2,001 refused with CMT-02's text and the real count.
    let at_limit = "ş".repeat(COMMENT_LIMIT_CHARS);
    assert_eq!(post(&w, "SHOP-1", &at_limit).await.n, 1);
    let refused = w
        .board
        .post_ticket_comment("SHOP-1", &you(), &"🦀".repeat(2_001), Vec::new(), &me())
        .await
        .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::TooLong));
    assert_eq!(
        refused.to_string(),
        "Comment is 2,001 characters; the limit is 2,000. Put the full text in a report with `--report-file` and \
         keep the comment to what the next agent needs."
    );
    for blank in ["", "  \n "] {
        let refused = w
            .board
            .post_ticket_comment("SHOP-1", &you(), blank, Vec::new(), &me())
            .await
            .unwrap_err();
        assert_eq!(refused.refusal(), Some(Refusal::EmptyField));
        assert_eq!(
            refused.to_string(),
            "Write the comment or attach a file first"
        );
    }

    // Post 2, delete 2: the next is 3, and the list is 1 and 3, oldest first.
    let second = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &rust_developer(),
            "Reused the CSV writer.",
            Vec::new(),
            &Actor::Agent("SHOP-1".parse().unwrap()),
        )
        .await
        .unwrap();
    assert_eq!(second.n, 2);
    assert_eq!(second.stage_name.as_deref(), Some("Dev"));
    w.board
        .delete_ticket_comment("SHOP-1", 2, &me())
        .await
        .unwrap();
    assert_eq!(post(&w, "SHOP-1", "Also keep the column order.").await.n, 3);
    let listed = w.board.ticket_comments("SHOP-1").await.unwrap();
    assert_eq!(
        listed.comments.iter().map(|c| c.n).collect::<Vec<_>>(),
        [1, 3]
    );
    // A deleted comment can't be edited or deleted again.
    assert!(
        w.board
            .edit_ticket_comment("SHOP-1", 2, "again", &me())
            .await
            .is_err()
    );

    // Editing keeps the limit and marks the comment edited.
    let refused = w
        .board
        .edit_ticket_comment("SHOP-1", 3, &"x".repeat(48_210), &me())
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Comment is 48,210 characters; the limit is 2,000. Put the full text in a report with `--report-file` and \
         keep the comment to what the next agent needs."
    );
    let edited = w
        .board
        .edit_ticket_comment("SHOP-1", 3, "Keep the column order of the table.", &me())
        .await
        .unwrap();
    assert!(edited.edited_at.is_some());
    assert_eq!(edited.body, "Keep the column order of the table.");

    // The three events, with the actor and their payloads.
    let events = comment_events(&w, "SHOP-1").await;
    let kinds: Vec<EventKind> = events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        [
            EventKind::CommentPosted,
            EventKind::CommentPosted,
            EventKind::CommentDeleted,
            EventKind::CommentPosted,
            EventKind::CommentEdited,
        ]
    );
    assert_eq!(events[1].actor.to_string(), "agent:SHOP-1");
    assert_eq!(events[1].payload["n"], 2);
    assert_eq!(events[1].payload["excerpt"], "Reused the CSV writer.");
    assert_eq!(events[1].payload["report"], Value::Null);
    assert_eq!(events[1].payload["author"]["name"], "Rust developer");
    assert_eq!(
        events[0].payload["excerpt"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        121,
        "a long comment's excerpt is its start"
    );
    assert_eq!(events[2].actor, me());
    assert_eq!(
        (&events[2].payload["n"], &events[2].payload["body"]),
        (&json!(2), &json!("Reused the CSV writer."))
    );
    assert_eq!(
        (
            &events[4].payload["n"],
            &events[4].payload["before"],
            &events[4].payload["after"]
        ),
        (
            &json!(3),
            &json!("Also keep the column order."),
            &json!("Keep the column order of the table.")
        )
    );
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reports_are_copied_sized_and_counted_and_live_as_long_as_the_ticket() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Export orders as CSV").await;
    let resources = tempfile::TempDir::new().unwrap();
    let run = resources.path().join("test-run.txt");
    let text = "running 3 tests\ntest export::limit ... FAILED\nfailures: 1\n".repeat(1_000);
    std::fs::write(&run, &text).unwrap();

    let fail = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &rust_developer(),
            "Fail: the 10,000 row limit isn't enforced.",
            vec![ReportSource::Path(run.clone())],
            &me(),
        )
        .await
        .unwrap();
    let [a] = fail.attachments.as_slice() else {
        panic!("one attachment: {:?}", fail.attachments)
    };
    assert_eq!(
        (a.n, a.name.as_str(), a.kind, a.bytes, a.lines),
        (
            1,
            "test-run.txt",
            AttachmentKind::Text,
            text.len() as u64,
            Some(3_000)
        )
    );
    // Copied, never moved.
    assert!(run.exists());
    let report = w.board.ticket_report("SHOP-1", fail.n, None).await.unwrap();
    let path = report.attachment.path.clone();
    assert_eq!(path, a.path);
    assert!(
        path.starts_with(
            &w.data
                .reports_dir()
                .join("shop-web")
                .join("SHOP-1")
                .display()
                .to_string()
        ),
        "{path}"
    );
    // The file keeps its name, in its numbered folder in its comment's (Q414):
    // reports/shop-web/SHOP-1/<id>/1/test-run.txt.
    let stored = std::path::Path::new(&path);
    assert!(stored.ends_with("1/test-run.txt"), "{path}");
    assert_eq!(
        stored.parent().unwrap().parent().unwrap().parent().unwrap(),
        w.data.reports_dir().join("shop-web").join("SHOP-1")
    );
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
    assert!(!report.deleted);
    let events = comment_events(&w, "SHOP-1").await;
    assert_eq!(
        events[0].payload["attachments"],
        json!([{"n": 1, "name": "test-run.txt", "kind": "text", "bytes": text.len(), "lines": 3_000}])
    );

    // Pasted text from the app goes to the same store.
    let pasted = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "The log from my run.",
            vec![ReportSource::Text("one\ntwo".into())],
            &me(),
        )
        .await
        .unwrap();
    // Pasted text has no name of its own: "Pasted text".
    assert_eq!(
        pasted
            .attachments
            .iter()
            .map(|a| (a.name.as_str(), a.kind, a.bytes, a.lines))
            .collect::<Vec<_>>(),
        [("Pasted text", AttachmentKind::Text, 7, Some(2))]
    );

    // Exactly 20 MB is accepted; one byte more refused, with nothing posted and no file left.
    let at_limit = resources.path().join("at-limit.txt");
    std::fs::File::create(&at_limit)
        .unwrap()
        .set_len(REPORT_LIMIT_BYTES)
        .unwrap();
    let big = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "The whole run.",
            vec![ReportSource::Path(at_limit.clone())],
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(big.attachments[0].bytes, REPORT_LIMIT_BYTES);
    let files_before = report_files(&w);
    assert_eq!(files_before.len(), 3);
    // `write`, not `append`: on Windows an append-only handle can't change the file's length (Access is denied).
    std::fs::OpenOptions::new()
        .write(true)
        .open(&at_limit)
        .unwrap()
        .set_len(REPORT_LIMIT_BYTES + 1)
        .unwrap();
    let refused = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "The whole run again.",
            vec![ReportSource::Path(at_limit.clone())],
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::BadReport));
    assert_eq!(
        refused.to_string(),
        "at-limit.txt is more than 20 MB; the limit is 20 MB. Keep the part that matters, for example the failing \
         tests."
    );
    std::fs::OpenOptions::new()
        .write(true)
        .open(&at_limit)
        .unwrap()
        .set_len(212 * 1024 * 1024)
        .unwrap();
    let refused = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "The runaway log.",
            vec![ReportSource::Path(at_limit.clone())],
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "at-limit.txt is 212 MB; the limit is 20 MB. Keep the part that matters, for example the failing tests."
    );
    // A missing file and a relative path: refused before anything is posted.
    let missing = resources.path().join("nope.txt");
    let refused = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "See the log.",
            vec![ReportSource::Path(missing.clone())],
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        format!(
            "Can't read the report {}: it doesn't exist",
            missing.display()
        )
    );
    assert!(
        w.board
            .post_ticket_comment(
                "SHOP-1",
                &you(),
                "See the log.",
                vec![ReportSource::Path("test-run.txt".into())],
                &me(),
            )
            .await
            .is_err()
    );
    assert_eq!(report_files(&w), files_before, "no partial file is left");
    assert_eq!(
        w.board
            .ticket_comments("SHOP-1")
            .await
            .unwrap()
            .comments
            .len(),
        3,
        "nothing was posted"
    );

    // A deleted comment's report stays (History opens it).
    w.board
        .delete_ticket_comment("SHOP-1", fail.n, &me())
        .await
        .unwrap();
    let kept = w.board.ticket_report("SHOP-1", fail.n, None).await.unwrap();
    assert!(kept.deleted);
    assert_eq!(
        std::fs::read_to_string(&kept.attachment.path).unwrap(),
        text
    );
    assert_eq!(report_files(&w).len(), 3);

    // Deleting the never-started ticket removes its reports and its folder (d0060).
    w.board.delete_ticket("SHOP-1").await.unwrap();
    assert!(report_files(&w).is_empty());
    assert!(
        !w.data
            .reports_dir()
            .join("shop-web")
            .join("SHOP-1")
            .exists()
    );
    w.stop().await;
}

/// tuq 001, 002, 005, 006: an agent's comment carries a PNG, a .txt and a .zip, each kept with its name, kind and size
/// (lines for text only); `ticket.report` picks each by its number and refuses a choice-less request; a second file
/// over 20 MB refuses the whole comment by its name, with nothing posted and no copied file left; a ticket with them is
/// deleted with its files.
#[tokio::test(flavor = "multi_thread")]
async fn a_comment_carries_several_files_of_any_type() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Export orders as CSV").await;
    let resources = tempfile::TempDir::new().unwrap();
    let png = resources.path().join("shot.png");
    std::fs::write(&png, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
    let txt = resources.path().join("run.txt");
    std::fs::write(&txt, "ok\nFAILED\n").unwrap();
    let zip = resources.path().join("repro.zip");
    std::fs::write(&zip, b"PK\x03\x04\x14\0\0\0").unwrap();
    let posted = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &rust_developer(),
            "What the page looks like, the run and a repro.",
            vec![
                ReportSource::Path(png.clone()),
                ReportSource::Path(txt.clone()),
                ReportSource::Path(zip.clone()),
            ],
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(
        posted
            .attachments
            .iter()
            .map(|a| (a.n, a.name.as_str(), a.kind, a.bytes, a.lines))
            .collect::<Vec<_>>(),
        [
            (1, "shot.png", AttachmentKind::Image, 16, None),
            (2, "run.txt", AttachmentKind::Text, 10, Some(2)),
            (3, "repro.zip", AttachmentKind::Other, 8, None),
        ]
    );
    for (a, source) in [(1, &png), (2, &txt), (3, &zip)] {
        let picked = w
            .board
            .ticket_report("SHOP-1", posted.n, Some(a))
            .await
            .unwrap();
        assert_eq!(picked.attachment.n, a);
        assert_eq!(
            std::fs::read(&picked.attachment.path).unwrap(),
            std::fs::read(source).unwrap()
        );
    }
    let unpicked = w
        .board
        .ticket_report("SHOP-1", posted.n, None)
        .await
        .unwrap_err();
    assert_eq!(unpicked.refusal(), Some(Refusal::BadReport));
    assert_eq!(
        unpicked.to_string(),
        "SHOP-1's comment 1 has 3 attachments; pick one with --attachment: 1 shot.png, 2 run.txt, 3 repro.zip"
    );
    // Each line names the file; text gives its command, the rest their path, never their bytes (tuq 006).
    let lines = posted.attachment_lines(&"SHOP-1".parse().unwrap());
    assert_eq!(
        lines,
        [
            format!(
                "Image: shot.png \u{b7} 16 bytes \u{b7} open this path to view it: `{}`",
                posted.attachments[0].path
            ),
            "Report: run.txt \u{b7} 10 bytes, 2 lines \u{b7} `otaq report SHOP-1 1 --attachment 2`"
                .to_owned(),
            format!(
                "File: repro.zip \u{b7} 8 bytes \u{b7} open this path to view it: `{}`",
                posted.attachments[2].path
            ),
        ]
    );
    let events = comment_events(&w, "SHOP-1").await;
    assert_eq!(
        events[0].payload["attachments"],
        json!([
            {"n": 1, "name": "shot.png", "kind": "image", "bytes": 16},
            {"n": 2, "name": "run.txt", "kind": "text", "bytes": 10, "lines": 2},
            {"n": 3, "name": "repro.zip", "kind": "other", "bytes": 8},
        ])
    );

    // The second of two is 21 MB: refused by its name, nothing posted, nothing copied (the first neither).
    let files_before = report_files(&w);
    let big = resources.path().join("perf.mov");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(21 * 1024 * 1024)
        .unwrap();
    let refused = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &rust_developer(),
            "The recording.",
            vec![ReportSource::Path(txt.clone()), ReportSource::Path(big)],
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::BadReport));
    assert_eq!(
        refused.to_string(),
        "perf.mov is 21 MB; the limit is 20 MB. Keep the part that matters, for example the failing tests."
    );
    assert_eq!(report_files(&w), files_before);
    assert_eq!(
        w.board
            .ticket_comments("SHOP-1")
            .await
            .unwrap()
            .comments
            .len(),
        1
    );

    // The ticket goes with every file and folder.
    w.board.delete_ticket("SHOP-1").await.unwrap();
    assert!(report_files(&w).is_empty());
    assert!(
        !w.data
            .reports_dir()
            .join("shop-web")
            .join("SHOP-1")
            .exists()
    );
    w.stop().await;
}

/// RPT-03 (T-0474): a ticket created with a report has it as its first comment: the user's, marked "at creation",
/// the report under its file's own name; a report that can't be copied creates nothing; an agent can't send one.
#[tokio::test(flavor = "multi_thread")]
async fn a_ticket_created_with_a_report_has_it_as_its_first_comment() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let resources = tempfile::TempDir::new().unwrap();
    let run = resources.path().join("test-run-login.md");
    std::fs::write(&run, "# Test run\n2 failed\n18 passed\n").unwrap();
    let input = |report: ReportSource, line: &str| NewTicketInput {
        project: "SHOP".into(),
        title: "Fix flaky login redirect".into(),
        first_comment: Some(FirstComment {
            body: line.into(),
            report: Some(report),
            ..Default::default()
        }),
        ..Default::default()
    };
    let t = w
        .board
        .create_ticket(
            &input(
                ReportSource::Path(run.clone()),
                "  Fails 2 in 20 on Safari; the run is attached. ",
            ),
            &me(),
        )
        .await
        .unwrap();
    let key = t.key.to_string();
    let comments = w.board.ticket_comments(&key).await.unwrap().comments;
    assert_eq!(comments.len(), 1);
    let first = &comments[0];
    assert_eq!((first.n, &first.author), (1, &you()));
    assert!(first.at_creation);
    assert_eq!(first.stage_name, None);
    assert!(
        first
            .heading()
            .starts_with("You \u{b7} at creation \u{b7} "),
        "{}",
        first.heading()
    );
    assert_eq!(first.body, "Fails 2 in 20 on Safari; the run is attached.");
    assert_eq!(
        first
            .attachments
            .iter()
            .map(|a| (a.n, a.name.as_str(), a.kind, a.bytes, a.lines))
            .collect::<Vec<_>>(),
        [(1, "test-run-login.md", AttachmentKind::Text, 30, Some(3))]
    );
    let report = w.board.ticket_report(&key, 1, None).await.unwrap();
    assert!(
        std::path::Path::new(&report.attachment.path).ends_with("test-run-login.md"),
        "{}",
        report.attachment.path
    );
    assert!(run.exists(), "copied, never moved");
    let events = comment_events(&w, &key).await;
    assert_eq!(events[0].payload["at_creation"], json!(true));
    // A comment posted later isn't "at creation".
    let later = post(&w, &key, "Also check Firefox.").await;
    assert!(!later.at_creation);

    // Pasted text: "Pasted text".
    let pasted = w
        .board
        .create_ticket(
            &input(ReportSource::Text("one\ntwo".into()), "My log."),
            &me(),
        )
        .await
        .unwrap();
    let first = w
        .board
        .ticket_comment(&pasted.key.to_string(), 1)
        .await
        .unwrap();
    assert!(first.at_creation);
    assert_eq!(first.attachments[0].name, "Pasted text");

    // Several files of any kind and no line (the New ticket form's attachments): one comment holding them all, in
    // order, the older single `report` first.
    let shot = resources.path().join("before.png");
    std::fs::write(&shot, [0x89, b'P', b'N', b'G']).unwrap();
    let several = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Cart badge count".into(),
                first_comment: Some(FirstComment {
                    body: String::new(),
                    report: None,
                    attachments: vec![
                        ReportSource::Path(shot.clone()),
                        ReportSource::Path(run.clone()),
                    ],
                }),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let first = w
        .board
        .ticket_comment(&several.key.to_string(), 1)
        .await
        .unwrap();
    assert!(first.at_creation);
    assert_eq!(first.body, "");
    assert_eq!(
        first
            .attachments
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>(),
        ["before.png", "test-run-login.md"]
    );

    // Refused before anything is created: a missing file, no line and no attachment.
    let files_before = report_files(&w);
    let missing = w
        .board
        .create_ticket(
            &input(
                ReportSource::Path(resources.path().join("nope.md")),
                "The run.",
            ),
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(missing.refusal(), Some(Refusal::BadReport), "{missing}");
    let empty = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Nothing to say".into(),
                first_comment: Some(FirstComment {
                    body: "  ".into(),
                    ..Default::default()
                }),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(empty.refusal(), Some(Refusal::EmptyField), "{empty}");
    assert!(w.board.ticket("SHOP-4").await.is_err(), "nothing created");
    assert_eq!(report_files(&w), files_before, "no file left behind");

    // An agent can't send one (its tickets have no first comment from it).
    let dev = w.agent_shell(&t).await;
    w.run_as_agent(&dev, "rust-dev", "Rust developer").await;
    let answer = send(
        &w,
        &dev,
        "ticket.create",
        json!({"project": "SHOP", "title": "Follow-up", "first_comment": {"body": "See this."}}),
    )
    .await;
    assert_eq!(
        refusal(&answer),
        "SHOP-1's agent can't post a new ticket's first comment; ask the user"
    );
    w.stop().await;
}

/// Sends one raw request from inside a session and returns the answer (no `'` in it: it goes through `sh`).
async fn send(w: &TestService, shell: &AgentShell, method: &str, params: Value) -> Value {
    let request = json!({"protocol_version": otaq_core::protocol::PROTOCOL_VERSION, "id": 1, "request": {"method": method, "params": params}});
    let ran = shell
        .send_raw(&w.data.socket_path(), &request.to_string())
        .await;
    serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
        .unwrap_or_else(|e| panic!("{e}: {ran:?}"))
}

fn refusal(answer: &Value) -> &str {
    answer["error"]["message"]
        .as_str()
        .unwrap_or_else(|| panic!("not refused: {answer}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn agents_write_on_their_own_ticket_and_edit_only_their_own() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Export orders as CSV").await;
    w.create("SHOP", "Order list paging").await;
    post(&w, "SHOP-1", "Also keep the column order.").await;
    let dev = w.agent_shell(&shop1).await;
    w.run_as_agent(&dev, "rust-dev", "Rust developer").await;

    // Its own ticket: posted as the agent its session runs, with its tool.
    let posted = send(
        &w,
        &dev,
        "ticket.comment_post",
        json!({"key": "SHOP-1", "body": "Reused the CSV writer in export.rs."}),
    )
    .await;
    let comment = &posted["reply"]["ticket_comment"];
    assert_eq!(comment["n"], 2, "{posted}");
    assert_eq!(
        comment["author"],
        json!({"kind": "agent", "id": "rust-dev", "name": "Rust developer", "tool": "claude"})
    );
    // Another ticket: refused.
    let other = send(
        &w,
        &dev,
        "ticket.comment_post",
        json!({"key": "SHOP-2", "body": "Hello"}),
    )
    .await;
    assert_eq!(refusal(&other), "SHOP-1's agent can't comment on SHOP-2");
    // Its own comment: edited and deleted; the user's: refused.
    let edited = send(
        &w,
        &dev,
        "ticket.comment_edit",
        json!({"key": "SHOP-1", "n": 2, "body": "Reused the CSV writer from SHOP-12."}),
    )
    .await;
    assert_eq!(
        edited["reply"]["ticket_comment"]["body"],
        "Reused the CSV writer from SHOP-12."
    );
    for (method, params) in [
        (
            "ticket.comment_edit",
            json!({"key": "SHOP-1", "n": 1, "body": "x"}),
        ),
        ("ticket.comment_delete", json!({"key": "SHOP-1", "n": 1})),
    ] {
        let answer = send(&w, &dev, method, params).await;
        assert_eq!(
            refusal(&answer),
            "SHOP-1's agent can only edit or delete its own comments"
        );
    }
    // Another agent on the same ticket (a later stage) can't edit Rust developer's comment.
    // (The same session stands in for the later stage's: one agent session per ticket at a time.)
    let reviewer = &dev;
    w.run_as_agent(reviewer, "reviewer", "Code reviewer").await;
    let answer = send(
        &w,
        reviewer,
        "ticket.comment_edit",
        json!({"key": "SHOP-1", "n": 2, "body": "x"}),
    )
    .await;
    assert_eq!(
        refusal(&answer),
        "SHOP-1's agent can only edit or delete its own comments"
    );
    // Anyone reads them.
    let listed = send(&w, reviewer, "ticket.comments", json!({"key": "SHOP-1"})).await;
    assert_eq!(
        listed["reply"]["ticket_comments"]["comments"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let deleted = send(
        &w,
        reviewer,
        "ticket.comment_delete",
        json!({"key": "SHOP-1", "n": 2}),
    )
    .await;
    assert!(deleted.get("error").is_some());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_checker_posts_only_with_its_verdict() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Export orders as CSV").await;
    let checker = w.shell(&shop1, SessionKind::Validator).await;
    let answer = send(
        &w,
        &checker,
        "ticket.comment_post",
        json!({"key": "SHOP-1", "body": "Fail: the limit is not enforced."}),
    )
    .await;
    assert_eq!(refusal(&answer), "SHOP-1's check can only give its verdict");
    // It reads them.
    let listed = send(&w, &checker, "ticket.comments", json!({"key": "SHOP-1"})).await;
    assert!(listed.get("reply").is_some(), "{listed}");
    w.stop().await;
}

/// Q374: the Validator fails with its comment, then edits and deletes it; another agent's comment stays refused.
#[tokio::test(flavor = "multi_thread")]
async fn a_checker_edits_and_deletes_its_own_verdict() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Export orders as CSV").await;
    w.give_worktree(&shop1).await;
    w.board
        .post_ticket_comment(
            "SHOP-1",
            &rust_developer(),
            "Reused the CSV writer.",
            Vec::new(),
            &me(),
        )
        .await
        .unwrap();
    let checker = w.shell(&shop1, SessionKind::Validator).await;
    w.run_as_agent(&checker, "validator", "Validator").await;
    w.validating_with(&shop1, &checker.session).await;
    send(
        &w,
        &checker,
        "ticket.prime_validate",
        json!({"key": "SHOP-1"}),
    )
    .await;
    let failed = send(
        &w,
        &checker,
        "validation.verdict",
        json!({"key": "SHOP-1", "pass": false, "report": "Fail: the limit is not enforced."}),
    )
    .await;
    assert!(failed.get("reply").is_some(), "{failed}");

    let edited = send(
        &w,
        &checker,
        "ticket.comment_edit",
        json!({"key": "SHOP-1", "n": 2, "body": "Fail: the 10,000 row limit is not enforced."}),
    )
    .await;
    assert_eq!(
        edited["reply"]["ticket_comment"]["body"], "Fail: the 10,000 row limit is not enforced.",
        "{edited}"
    );
    let other = send(
        &w,
        &checker,
        "ticket.comment_edit",
        json!({"key": "SHOP-1", "n": 1, "body": "x"}),
    )
    .await;
    assert_eq!(
        refusal(&other),
        "SHOP-1's Validate check can only edit or delete its own comments"
    );
    let deleted = send(
        &w,
        &checker,
        "ticket.comment_delete",
        json!({"key": "SHOP-1", "n": 2}),
    )
    .await;
    assert!(deleted.get("reply").is_some(), "{deleted}");
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn each_change_tells_clients_the_ticket_changed() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Export orders as CSV").await;
    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut events = w.events.subscribe();
    let requests = [
        Request::TicketCommentPost(TicketCommentPostParams {
            key: "SHOP-1".into(),
            body: "Also keep the column order.".into(),
            attachments: Default::default(),
            tell_agent: false,
        }),
        serde_json::from_value(json!({"method": "ticket.comment_edit",
            "params": {"key": "SHOP-1", "n": 1, "body": "Keep the order."}}))
        .unwrap(),
        serde_json::from_value(
            json!({"method": "ticket.comment_delete", "params": {"key": "SHOP-1", "n": 1}}),
        )
        .unwrap(),
    ];
    for request in requests {
        while events.try_recv().is_ok() {}
        let reply = app.request(request).await.unwrap();
        assert!(matches!(reply, Reply::TicketComment(_)), "{reply:?}");
        let changed = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if let Ok(Event::TicketChanged { key, .. }) = events.recv().await
                    && key.to_string() == "SHOP-1"
                {
                    return;
                }
            }
        })
        .await;
        assert!(changed.is_ok(), "ticket_changed after each change");
    }
    w.stop().await;
}

/// Q375: a deleted comment's report still opens for the user (History, their own terminal), never for an agent
/// session: "Agents that start after this won't read it."
#[tokio::test(flavor = "multi_thread")]
async fn a_deleted_comments_report_is_the_users_only() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Export orders as CSV").await;
    let posted = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "The full log.",
            vec![ReportSource::Text("line 1\nline 2\n".into())],
            &me(),
        )
        .await
        .unwrap();
    w.board
        .delete_ticket_comment("SHOP-1", posted.n, &me())
        .await
        .unwrap();

    let app = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let request: Request = serde_json::from_value(
        json!({"method": "ticket.report", "params": {"key": "SHOP-1", "n": posted.n}}),
    )
    .unwrap();
    match app.request(request).await.unwrap() {
        Reply::TicketReport(report) => {
            assert!(report.deleted);
            assert!(std::path::Path::new(&report.attachment.path).exists());
        }
        other => panic!("{other:?}"),
    }
    let dev = w.agent_shell(&shop1).await;
    let refused = send(
        &w,
        &dev,
        "ticket.report",
        json!({"key": "SHOP-1", "n": posted.n}),
    )
    .await;
    assert_eq!(refusal(&refused), "Comment 1 on SHOP-1 was deleted.");
    w.stop().await;
}

/// tuq 007: a comment needs text, or at least one attachment, or both. One attachment and no text posts (the user's and
/// a stage end's hand-over alike); neither is refused; the text may be edited away only while an attachment stays.
#[tokio::test(flavor = "multi_thread")]
async fn a_comment_of_only_attachments_posts_and_an_empty_one_is_refused() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Export orders as CSV").await;
    let shot = w.home.path().join("shot.png");
    std::fs::write(&shot, [137, 80, 78, 71, 13, 10, 26, 10]).unwrap();

    let posted = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "  \n",
            vec![ReportSource::Path(shot)],
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(posted.body, "");
    assert_eq!(posted.attachments.len(), 1);
    assert_eq!(posted.attachments[0].kind, AttachmentKind::Image);
    let events = comment_events(&w, "SHOP-1").await;
    assert_eq!(events[0].payload["excerpt"], "");
    assert_eq!(events[0].payload["attachments"][0]["name"], "shot.png");

    // Neither text nor an attachment: refused, nothing posted.
    let refused = w
        .board
        .post_ticket_comment("SHOP-1", &you(), " ", Vec::new(), &me())
        .await
        .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::EmptyField));
    assert_eq!(
        refused.to_string(),
        "Write the comment or attach a file first"
    );
    assert_eq!(
        w.board
            .ticket_comments("SHOP-1")
            .await
            .unwrap()
            .comments
            .len(),
        1
    );

    // A stage end (`stage next`, `check pass|fail`) with an empty summary and a file hands the file over; with
    // neither, nothing is posted.
    let hand_over = w
        .board
        .stage_end_hand_over(
            &ticket,
            rust_developer(),
            "",
            vec![ReportSource::Text("FAILED\n".into())],
            &[],
        )
        .await
        .unwrap()
        .expect("the file is handed over");
    assert_eq!(hand_over.body, "");
    hand_over.discard_unposted(&w.board).await;
    assert!(
        w.board
            .stage_end_hand_over(&ticket, rust_developer(), "", Vec::new(), &[])
            .await
            .unwrap()
            .is_none()
    );

    // Editing: the text may go while the comment keeps its attachment, never from a comment of only text.
    let edited = w
        .board
        .edit_ticket_comment("SHOP-1", posted.n, "The badge, before.", &me())
        .await
        .unwrap();
    assert_eq!(edited.body, "The badge, before.");
    let edited = w
        .board
        .edit_ticket_comment("SHOP-1", posted.n, "", &me())
        .await
        .unwrap();
    assert_eq!(edited.body, "");
    let worded = post(&w, "SHOP-1", "Only words.").await;
    let refused = w
        .board
        .edit_ticket_comment("SHOP-1", worded.n, "  ", &me())
        .await
        .unwrap_err();
    assert_eq!(
        refused.to_string(),
        "Write the comment or attach a file first"
    );
    w.stop().await;
}

/// tuq 008: an edit removes one of a comment's two attachments and adds a third in one change: the comment lists the
/// new set (the removed one gone, the new one numbered after every number it had), the `comment_edited` event names
/// the removed and added ones, and the removed file stays: the user still opens it by its number (History), an agent
/// is refused it and a choice-less request no longer counts it. An edit that would leave no text and no attachment,
/// one naming an attachment the comment hasn't got and one adding a file over 20 MB are refused with nothing changed
/// and nothing copied. The ticket goes with every file, the removed one included.
#[tokio::test(flavor = "multi_thread")]
async fn an_edit_removes_and_adds_attachments_and_history_keeps_the_removed_one() {
    use otaq_core::board::ticket_comments::CommentEdit;
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Export orders as CSV").await;
    let resources = tempfile::TempDir::new().unwrap();
    let shot = resources.path().join("shot.png");
    std::fs::write(&shot, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
    let run = resources.path().join("run.txt");
    std::fs::write(&run, "ok\nFAILED\n").unwrap();
    let fixed = resources.path().join("fixed.png");
    std::fs::write(&fixed, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR+").unwrap();
    let posted = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "The badge, before.",
            vec![
                ReportSource::Path(shot.clone()),
                ReportSource::Path(run.clone()),
            ],
            &me(),
        )
        .await
        .unwrap();
    let shot_path = posted.attachments[0].path.clone();

    let edited = w
        .board
        .edit_ticket_comment_with(
            "SHOP-1",
            posted.n,
            &CommentEdit {
                body: None,
                remove: vec![1],
                add: vec![ReportSource::Path(fixed.clone())],
            },
            &me(),
        )
        .await
        .unwrap();
    let set = |c: &TicketComment| {
        c.attachments
            .iter()
            .map(|a| (a.n, a.name.clone(), a.kind))
            .collect::<Vec<_>>()
    };
    let new_set = vec![
        (2, "run.txt".to_owned(), AttachmentKind::Text),
        (3, "fixed.png".to_owned(), AttachmentKind::Image),
    ];
    assert_eq!(edited.body, "The badge, before.", "no new text: it stays");
    assert!(edited.edited_at.is_some());
    assert_eq!(set(&edited), new_set);
    assert_eq!(
        set(&w.board.ticket_comment("SHOP-1", posted.n).await.unwrap()),
        new_set,
        "the thread lists the new set"
    );
    assert_eq!(
        std::fs::read(&edited.attachments[1].path).unwrap(),
        std::fs::read(&fixed).unwrap()
    );
    let events = comment_events(&w, "SHOP-1").await;
    let edit = events.last().unwrap();
    assert_eq!(edit.kind, EventKind::CommentEdited);
    assert_eq!(edit.payload["before"], "The badge, before.");
    assert_eq!(edit.payload["after"], "The badge, before.");
    assert_eq!(
        edit.payload["removed"],
        json!([{"n": 1, "name": "shot.png", "kind": "image", "bytes": 16}])
    );
    assert_eq!(
        edit.payload["added"],
        json!([{"n": 3, "name": "fixed.png", "kind": "image", "bytes": 17}])
    );

    // The removed file stays and opens by its number (History, the image view); a choice-less request counts only
    // what the comment has now.
    let opened = w
        .board
        .ticket_report("SHOP-1", posted.n, Some(1))
        .await
        .unwrap();
    assert!(opened.removed);
    assert_eq!(opened.attachment.path, shot_path);
    assert_eq!(
        std::fs::read(&shot_path).unwrap(),
        std::fs::read(&shot).unwrap()
    );
    assert_eq!(
        w.board
            .ticket_report("SHOP-1", posted.n, None)
            .await
            .unwrap_err()
            .to_string(),
        "SHOP-1's comment 1 has 2 attachments; pick one with --attachment: 2 run.txt, 3 fixed.png"
    );
    let dev = w.agent_shell(&shop1).await;
    let refused = send(
        &w,
        &dev,
        "ticket.report",
        json!({"key": "SHOP-1", "n": posted.n, "attachment": 1}),
    )
    .await;
    assert_eq!(
        refusal(&refused),
        "Attachment 1 of comment 1 on SHOP-1 was removed."
    );

    // Neither text nor an attachment left: refused, nothing changed.
    let files_before = report_files(&w);
    let empty = w
        .board
        .edit_ticket_comment_with(
            "SHOP-1",
            posted.n,
            &CommentEdit {
                body: Some(" ".into()),
                remove: vec![2, 3],
                add: Vec::new(),
            },
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(empty.refusal(), Some(Refusal::EmptyField));
    assert_eq!(
        empty.to_string(),
        "Write the comment or attach a file first"
    );
    // Its text may go while an attachment stays.
    let no_text = w
        .board
        .edit_ticket_comment_with(
            "SHOP-1",
            posted.n,
            &CommentEdit {
                body: Some(String::new()),
                remove: vec![2],
                add: Vec::new(),
            },
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(no_text.body, "");
    assert_eq!(
        set(&no_text),
        [(3, "fixed.png".to_owned(), AttachmentKind::Image)]
    );
    let missing = w
        .board
        .edit_ticket_comment_with(
            "SHOP-1",
            posted.n,
            &CommentEdit {
                remove: vec![1],
                ..CommentEdit::default()
            },
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        missing.to_string(),
        "SHOP-1's comment 1 has no attachment 1"
    );
    let big = resources.path().join("perf.mov");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(21 * 1024 * 1024)
        .unwrap();
    let too_big = w
        .board
        .edit_ticket_comment_with(
            "SHOP-1",
            posted.n,
            &CommentEdit {
                body: Some("Recorded.".into()),
                remove: vec![3],
                add: vec![ReportSource::Path(run.clone()), ReportSource::Path(big)],
            },
            &me(),
        )
        .await
        .unwrap_err();
    assert_eq!(too_big.refusal(), Some(Refusal::BadReport));
    assert_eq!(report_files(&w), files_before);
    let kept = w.board.ticket_comment("SHOP-1", posted.n).await.unwrap();
    assert_eq!(kept.body, "");
    assert_eq!(
        set(&kept),
        [(3, "fixed.png".to_owned(), AttachmentKind::Image)]
    );

    w.stop().await;
}

/// tuq 008, d0060: a ticket goes with every attachment file, those an edit removed too.
#[tokio::test(flavor = "multi_thread")]
async fn a_deleted_ticket_takes_its_removed_attachments() {
    use otaq_core::board::ticket_comments::CommentEdit;
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Export orders as CSV").await;
    let posted = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "Two logs.",
            vec![
                ReportSource::Text("one\n".into()),
                ReportSource::Text("two\n".into()),
            ],
            &me(),
        )
        .await
        .unwrap();
    w.board
        .edit_ticket_comment_with(
            "SHOP-1",
            posted.n,
            &CommentEdit {
                remove: vec![1],
                ..CommentEdit::default()
            },
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(report_files(&w).len(), 2, "the removed file stays");
    w.board.delete_ticket("SHOP-1").await.unwrap();
    assert!(report_files(&w).is_empty());
    w.stop().await;
}

/// One comment never has two attachments of the same name: two `shot.png` (two screenshots, two folders' files) post
/// as `shot.png` and `shot 2.png`, two pastes as "Pasted text" and "Pasted text 2", and an edit's addition is named
/// apart from the ones the comment keeps (a removed one's name is free again).
#[tokio::test(flavor = "multi_thread")]
async fn attachments_of_one_name_are_told_apart() {
    use otaq_core::board::ticket_comments::CommentEdit;
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Export orders as CSV").await;
    let resources = tempfile::TempDir::new().unwrap();
    let (a, b) = (resources.path().join("a"), resources.path().join("b"));
    std::fs::create_dir_all(&a).unwrap();
    std::fs::create_dir_all(&b).unwrap();
    std::fs::write(a.join("shot.png"), b"\x89PNG\r\n\x1a\n").unwrap();
    std::fs::write(b.join("shot.png"), b"\x89PNG\r\n\x1a\n+").unwrap();
    let posted = w
        .board
        .post_ticket_comment(
            "SHOP-1",
            &you(),
            "",
            vec![
                ReportSource::Path(a.join("shot.png")),
                ReportSource::Path(b.join("shot.png")),
                ReportSource::Text("one\n".into()),
                ReportSource::Text("two\n".into()),
            ],
            &me(),
        )
        .await
        .unwrap();
    let names = |c: &TicketComment| {
        c.attachments
            .iter()
            .map(|a| (a.n, a.name.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(&posted),
        [
            (1, "shot.png".to_owned()),
            (2, "shot 2.png".to_owned()),
            (3, "Pasted text".to_owned()),
            (4, "Pasted text 2".to_owned()),
        ]
    );
    assert!(posted.attachments[1].path.ends_with("shot 2.png"));
    assert_eq!(
        std::fs::read(&posted.attachments[1].path).unwrap(),
        b"\x89PNG\r\n\x1a\n+"
    );

    let edited = w
        .board
        .edit_ticket_comment_with(
            "SHOP-1",
            posted.n,
            &CommentEdit {
                body: None,
                remove: vec![2],
                add: vec![
                    ReportSource::Path(a.join("shot.png")),
                    ReportSource::Path(b.join("shot.png")),
                ],
            },
            &me(),
        )
        .await
        .unwrap();
    assert_eq!(
        names(&edited),
        [
            (1, "shot.png".to_owned()),
            (3, "Pasted text".to_owned()),
            (4, "Pasted text 2".to_owned()),
            (5, "shot 2.png".to_owned()),
            (6, "shot 3.png".to_owned()),
        ]
    );
    w.stop().await;
}
