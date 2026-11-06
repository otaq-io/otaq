//! `otaq comment`, `otaq comment edit|delete`, `otaq comments` and `otaq report` (T-0442; CMT-06, d0109): from the
//! user's terminal (a key, no ticket variable) and from inside an agent session (an ordinary `sh`, never an agent
//! tool, on `$OTAQ_TICKET`), each with `--json`.

use std::io::Write;
use std::process::{Command, Output, Stdio};

use otaq_core::board::ticket_comments::tell_agent_line;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::Status;
use otaq_core::naming;
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// The user's terminal, in the test's home folder: no ticket variable.
fn cb(w: &TestService, args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(CB)
        .args(args)
        .env(DataDir::env_var(), w.data.root())
        .env_remove(naming::ticket_env_var())
        .current_dir(w.home.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.unwrap_or_default().as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn the_user_posts_lists_edits_deletes_and_reads_reports() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    w.create("SHOP", "Export orders as CSV").await;
    let run = w.home.path().join("test-run.txt");
    std::fs::write(
        &run,
        "running 3 tests\ntest limit ... FAILED\nfailures: 1\n",
    )
    .unwrap();

    let posted = cb(
        &w,
        &["comment", "SHOP-1", "--stdin"],
        Some("Also keep the column order the same as the table on screen.\n"),
    );
    assert_eq!(stdout(&posted).trim(), "Posted comment 1 on SHOP-1.");
    // A relative report path is the terminal's folder's.
    let posted = cb(
        &w,
        &[
            "comment",
            "SHOP-1",
            "--stdin",
            "--report-file",
            "test-run.txt",
        ],
        Some("Fail: the 10,000 row limit isn't enforced."),
    );
    assert_eq!(
        stdout(&posted).trim(),
        "Posted comment 2 on SHOP-1 with test-run.txt (50 bytes)."
    );
    let posted = cb(
        &w,
        &["comment", "SHOP-1", "--stdin", "--json"],
        Some("Third."),
    );
    let value = json(&posted);
    assert_eq!(value["key"], "SHOP-1");
    assert_eq!(value["n"], 3);
    assert_eq!(value["body"], "Third.");
    assert_eq!(value["author"]["kind"], "user");
    let posted = cb(
        &w,
        &["comment", "SHOP-1", "--stdin", "--silent"],
        Some("Fourth."),
    );
    assert_eq!(stdout(&posted).trim(), "4");

    // Refused: too long (the service's text), a missing report (before anything is posted).
    let refused = cb(
        &w,
        &["comment", "SHOP-1", "--stdin", "--json"],
        Some(&"ş".repeat(2_001)),
    );
    assert!(!refused.status.success());
    assert_eq!(
        json(&refused)["error"]["message"],
        "Comment is 2,001 characters; the limit is 2,000. Put the full text in a report with `--report-file` and \
         keep the comment to what the next agent needs."
    );
    let refused = cb(
        &w,
        &["comment", "SHOP-1", "--stdin", "--report-file", "nope.txt"],
        Some("See the log."),
    );
    assert!(!refused.status.success());
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr).trim(),
        format!(
            "error: Can't read the report {}: it doesn't exist",
            // The terminal's folder as the process sees it (macOS: under /private; Windows: as it was given).
            if cfg!(windows) {
                w.home.path().to_path_buf()
            } else {
                w.home.path().canonicalize().unwrap()
            }
            .join("nope.txt")
            .display()
        )
    );
    let refused = cb(&w, &["comment", "SHOP-1"], None);
    assert!(!refused.status.success());
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr).trim(),
        "error: Write the comment first"
    );

    // Edit and delete.
    let edited = cb(
        &w,
        &["comment", "edit", "SHOP-1", "3", "--stdin"],
        Some("Third, edited."),
    );
    assert_eq!(stdout(&edited).trim(), "Edited comment 3 on SHOP-1.");
    let edited = cb(
        &w,
        &["comment", "edit", "SHOP-1", "3", "--stdin", "--json"],
        Some("Third, again."),
    );
    assert_eq!(json(&edited)["body"], "Third, again.");
    assert!(json(&edited)["edited_at"].is_i64());
    let deleted = cb(&w, &["comment", "delete", "SHOP-1", "4"], None);
    assert_eq!(
        stdout(&deleted).trim(),
        "Deleted comment 4 on SHOP-1. Later agents won't read it; History keeps what it said."
    );
    let deleted = cb(&w, &["comment", "delete", "SHOP-1", "1", "--json"], None);
    assert_eq!(json(&deleted)["n"], 1);
    let refused = cb(&w, &["comment", "delete", "SHOP-1", "x"], None);
    assert!(!refused.status.success());

    // The list: oldest first, without the deleted ones.
    let listed = stdout(&cb(&w, &["comments", "SHOP-1"], None));
    assert!(
        listed.starts_with("SHOP-1 \u{b7} 2 comments\n\n2 \u{b7} You \u{b7} "),
        "{listed}"
    );
    assert!(
        listed.contains(
            "\nFail: the 10,000 row limit isn't enforced.\nReport: test-run.txt \u{b7} 50 bytes, 3 lines \u{b7} `otaq report SHOP-1 2`\n\n3 \
             \u{b7} You \u{b7} "
        ),
        "{listed}"
    );
    assert!(
        listed.trim_end().ends_with(" \u{b7} edited\nThird, again."),
        "{listed}"
    );
    let listed = json(&cb(&w, &["comments", "SHOP-1", "--json"], None));
    assert_eq!(listed["key"], "SHOP-1");
    let comments = listed["comments"].as_array().unwrap();
    assert_eq!(comments.len(), 2);
    assert_eq!(comments[0]["n"], 2);
    let attached = &comments[0]["attachments"][0];
    assert_eq!(
        (
            &attached["n"],
            &attached["name"],
            &attached["kind"],
            &attached["bytes"],
            &attached["lines"]
        ),
        (
            &json!(1),
            &json!("test-run.txt"),
            &json!("text"),
            &json!(50),
            &json!(3)
        )
    );

    // The report: printed whole, its path, or its JSON (never its text).
    let printed = cb(&w, &["report", "SHOP-1", "2"], None);
    assert_eq!(
        stdout(&printed),
        "running 3 tests\ntest limit ... FAILED\nfailures: 1\n"
    );
    let path = stdout(&cb(&w, &["report", "SHOP-1", "2", "--path"], None));
    let path = path.trim();
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
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        std::fs::read_to_string(&run).unwrap()
    );
    let report = json(&cb(&w, &["report", "SHOP-1", "2", "--json"], None));
    assert_eq!(report["attachment"]["path"], path);
    assert_eq!(report["attachment"]["lines"], 3);
    let refused = cb(&w, &["report", "SHOP-1", "3"], None);
    assert!(!refused.status.success());
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr).trim(),
        "error: SHOP-1's comment 3 has no attachments"
    );
    w.stop().await;
}

/// Files in the data folder under `dir`, recursively.
fn files_under(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(files_under(&path));
        } else {
            out.push(path);
        }
    }
    out
}

/// tuq 001, 002, 005, 006: one comment carries a PNG, a .txt and a .zip through `--report-file` repeated; `comments
/// --json` lists them with their kind and size; `report --attachment` picks each (an image's or a zip's bytes are
/// never printed, only its path); `prime` lists all three; a 21 MB file is refused by name and posts nothing; deleting
/// the ticket removes the files.
#[tokio::test(flavor = "multi_thread")]
async fn one_comment_carries_three_files_of_any_type() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Export orders as CSV").await;
    let png: Vec<u8> = [b"\x89PNG\r\n\x1a\n".as_slice(), &[0, 0, 0, 13], b"IHDR"].concat();
    std::fs::write(w.home.path().join("shot.png"), &png).unwrap();
    std::fs::write(
        w.home.path().join("run.txt"),
        "running 3 tests\ntest limit ... FAILED\nfailures: 1\n",
    )
    .unwrap();
    let zip: Vec<u8> = [b"PK\x03\x04".as_slice(), &[20, 0, 0, 0, 8, 0], b"repro"].concat();
    std::fs::write(w.home.path().join("repro.zip"), &zip).unwrap();

    let posted = cb(
        &w,
        &[
            "comment",
            "SHOP-1",
            "--stdin",
            "--report-file",
            "shot.png",
            "--report-file",
            "run.txt",
            "--report-file",
            "repro.zip",
        ],
        Some("Before and after, with the log and a repro."),
    );
    assert_eq!(
        stdout(&posted).trim(),
        "Posted comment 1 on SHOP-1 with 3 attachments (81 bytes)."
    );

    // `comments --json`: each with its number, name, kind, size; lines for text only; its path.
    let listed = json(&cb(&w, &["comments", "SHOP-1", "--json"], None));
    let attached = listed["comments"][0]["attachments"].as_array().unwrap();
    let seen: Vec<(u64, &str, &str, u64, Option<u64>)> = attached
        .iter()
        .map(|a| {
            (
                a["n"].as_u64().unwrap(),
                a["name"].as_str().unwrap(),
                a["kind"].as_str().unwrap(),
                a["bytes"].as_u64().unwrap(),
                a["lines"].as_u64(),
            )
        })
        .collect();
    assert_eq!(
        seen,
        [
            (1, "shot.png", "image", png.len() as u64, None),
            (2, "run.txt", "text", 50, Some(3)),
            (3, "repro.zip", "other", zip.len() as u64, None),
        ]
    );
    let paths: Vec<String> = attached
        .iter()
        .map(|a| a["path"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(std::fs::read(&paths[0]).unwrap(), png);
    assert_eq!(std::fs::read(&paths[2]).unwrap(), zip);

    // `comments` names each, with the command for text and the path for the rest.
    let text = stdout(&cb(&w, &["comments", "SHOP-1"], None));
    for line in [
        format!(
            "Image: shot.png \u{b7} {} bytes \u{b7} open this path to view it: {}",
            png.len(),
            paths[0]
        ),
        "Report: run.txt \u{b7} 50 bytes, 3 lines \u{b7} `otaq report SHOP-1 1 --attachment 2`"
            .to_owned(),
        format!(
            "File: repro.zip \u{b7} {} bytes \u{b7} open this path to view it: {}",
            zip.len(),
            paths[2]
        ),
    ] {
        assert!(text.contains(&line), "missing {line:?} in\n{text}");
    }

    // `report` picks each; without `--attachment` it names them.
    let refused = cb(&w, &["report", "SHOP-1", "1"], None);
    assert!(!refused.status.success());
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr).trim(),
        "error: SHOP-1's comment 1 has 3 attachments; pick one with --attachment: 1 shot.png, 2 run.txt, 3 repro.zip"
    );
    assert_eq!(
        stdout(&cb(
            &w,
            &["report", "SHOP-1", "1", "--attachment", "2"],
            None
        )),
        "running 3 tests\ntest limit ... FAILED\nfailures: 1\n"
    );
    for (a, path) in [("1", &paths[0]), ("3", &paths[2])] {
        // Never the bytes: the path, and why on stderr.
        let shown = cb(&w, &["report", "SHOP-1", "1", "--attachment", a], None);
        assert_eq!(stdout(&shown).trim(), path.as_str());
        assert!(
            String::from_utf8_lossy(&shown.stderr).contains("open this path to view it"),
            "{shown:?}"
        );
        let only = stdout(&cb(
            &w,
            &["report", "SHOP-1", "1", "--attachment", a, "--path"],
            None,
        ));
        assert_eq!(only.trim(), path.as_str());
    }
    let picked = json(&cb(
        &w,
        &["report", "SHOP-1", "1", "--attachment", "3", "--json"],
        None,
    ));
    assert_eq!(picked["attachment"]["kind"], "other");
    assert_eq!(picked["attachment"]["name"], "repro.zip");
    let refused = cb(&w, &["report", "SHOP-1", "1", "--attachment", "4"], None);
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr).trim(),
        "error: SHOP-1's comment 1 has no attachment 4"
    );

    // `prime` lists all three under "Reports from you", never their bytes.
    w.force_status(&t, otaq_core::domain::Status::Working).await;
    let context = w
        .board
        .prime_context("SHOP-1", w.adapters.for_new_session())
        .await
        .unwrap();
    let prime = otaq_core::prime::render(&context);
    let section = prime
        .split("## Reports from you")
        .nth(1)
        .unwrap_or_else(|| panic!("no Reports from you in\n{prime}"));
    for name in ["shot.png", "run.txt", "repro.zip"] {
        assert!(
            section.contains(&format!("- {name} \u{b7} ")),
            "{name} in\n{prime}"
        );
    }
    assert!(
        section.contains(&format!("open this path to view it: {}", paths[0])),
        "{prime}"
    );
    assert!(!prime.contains("FAILED"), "a text file's text in prime");

    // A 21 MB file is refused by its name, with the others: nothing posted, nothing copied.
    let before = files_under(&w.data.reports_dir());
    let big = w.home.path().join("perf.mov");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(21 * 1024 * 1024)
        .unwrap();
    let refused = cb(
        &w,
        &[
            "comment",
            "SHOP-1",
            "--stdin",
            "--report-file",
            "run.txt",
            "--report-file",
            "perf.mov",
        ],
        None,
    );
    assert!(!refused.status.success());
    assert_eq!(
        String::from_utf8_lossy(&refused.stderr).trim(),
        "error: perf.mov is 21 MB; the limit is 20 MB. Keep the part that matters, for example the failing tests."
    );
    let listed = json(&cb(&w, &["comments", "SHOP-1", "--json"], None));
    assert_eq!(listed["comments"].as_array().unwrap().len(), 1);
    assert_eq!(files_under(&w.data.reports_dir()), before);

    // Deleting a never-started ticket removes every attachment and its folders (d0060).
    w.create("SHOP", "Order list paging").await;
    stdout(&cb(
        &w,
        &[
            "comment",
            "SHOP-2",
            "--stdin",
            "--report-file",
            "shot.png",
            "--report-file",
            "run.txt",
            "--report-file",
            "repro.zip",
        ],
        Some("The same three."),
    ));
    let shop2 = w.data.reports_dir().join("shop-web").join("SHOP-2");
    assert_eq!(files_under(&shop2).len(), 3);
    w.board.delete_ticket("SHOP-2").await.unwrap();
    assert!(!shop2.exists());
    assert_eq!(files_under(&w.data.reports_dir()), before);
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_comments_on_its_own_ticket_and_greps_a_report() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Export orders as CSV").await;
    w.create("SHOP", "Order list paging").await;
    let agent = w.agent_shell(&shop1).await;
    w.run_as_agent(&agent, "rust-dev", "Rust developer").await;
    let resources = tempfile::TempDir::new().unwrap();
    std::fs::write(
        resources.path().join("test-run.txt"),
        "ok 1\nFAILED export_tests::limit\nok 3\n",
    )
    .unwrap();

    // `$OTAQ_TICKET`, a report saved by the tool and named relative to the folder it runs in.
    let ran = agent
        .run_with_stdin(
            &format!(
                "cd '{}' && '{CB}' comment --stdin --report-file test-run.txt",
                resources.path().display()
            ),
            "Fail: the 10,000 row limit is not enforced.",
        )
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    assert_eq!(
        ran.stdout.trim(),
        "Posted comment 1 on SHOP-1 with test-run.txt (37 bytes)."
    );
    let ran = agent
        .run(&format!("grep -n FAILED \"$('{CB}' report 1 --path)\""))
        .await;
    assert_eq!(ran.stdout.trim(), "2:FAILED export_tests::limit", "{ran:?}");
    let ran = agent
        .run_with_stdin(
            &format!("'{CB}' comment edit 1 --stdin --json"),
            "Fail: see the report.",
        )
        .await;
    let edited: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(edited["author"]["name"], "Rust developer");
    assert_eq!(edited["body"], "Fail: see the report.");

    // Another ticket, and the user's comment: refused.
    let ran = agent
        .run_with_stdin(&format!("'{CB}' comment SHOP-2 --stdin --json"), "Hello")
        .await;
    assert_ne!(ran.status, 0);
    let error: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(
        error["error"]["message"],
        "SHOP-1's agent can't comment on SHOP-2"
    );
    stdout(&cb(&w, &["comment", "SHOP-1", "--stdin"], Some("Mine.")));
    let ran = agent.run(&format!("'{CB}' comment delete 2")).await;
    assert_ne!(ran.status, 0);
    assert_eq!(
        ran.stderr.trim(),
        "error: SHOP-1's agent can only edit or delete its own comments"
    );
    let ran = agent.run(&format!("'{CB}' comments --json")).await;
    let listed: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(listed["comments"].as_array().unwrap().len(), 2);
    w.stop().await;
}

/// d0123: `--tell-agent` sends the flag, so the ticket's running agent is told "The user added comment N. Run `otaq
/// prime` to read it."; off unless given. `otaq comment --help` says what it does.
#[tokio::test(flavor = "multi_thread")]
async fn tell_agent_tells_the_running_agent() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let shop1 = w.create("SHOP", "Export orders as CSV").await;
    w.force_status(&shop1, Status::Working).await;
    let agent = w.agent_shell(&shop1).await;

    stdout(&cb(
        &w,
        &["comment", "SHOP-1", "--stdin"],
        Some("For later."),
    ));
    assert!(
        w.deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty()
    );
    let posted = cb(
        &w,
        &["comment", "SHOP-1", "--stdin", "--tell-agent"],
        Some("Also keep the column order."),
    );
    assert_eq!(stdout(&posted).trim(), "Posted comment 2 on SHOP-1.");
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].text, tell_agent_line(2));

    let help = stdout(&cb(&w, &["comment", "--help"], None));
    assert!(help.contains("--tell-agent"), "{help}");
    assert!(
        help.contains("Also tell the ticket's running agent (or its check) now"),
        "{help}"
    );
    w.stop().await;
}
