//! `--stdin-file <PATH>` (fixes-before-0-3, piece 1): every command with `--stdin` reads the same text from a file,
//! so a Codex agent passes long text with a plain command that Codex's `otaq` rule lets run outside its sandbox. And
//! the `prime` of a Codex session says so, while every other tool's prime stays byte for byte what it was.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use otaq_core::agent_tools::Tool;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::Status;
use otaq_core::naming;
use otaq_core::service::testing::{AgentShell, TestService};
use otaq_core::session::SessionKind;
use serde_json::Value;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// Long text as an agent writes it: quotes, backticks, a `$`, blank lines and a trailing newline.
const TEXT: &str =
    "Rename `fullName` to \"first\" and 'last' ($HOME stays).\n\n```ts\nconst a = `x`;\n```\n";

/// `otaq <args>` as the user, `stdin` on its stdin.
fn cb_in(w: &TestService, args: &[&str], stdin: &str) -> Output {
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
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

fn json(out: &Output) -> Value {
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

fn text_file(dir: &Path, name: &str, text: &str) -> String {
    let path = dir.join(name);
    std::fs::write(&path, text).unwrap();
    path.display().to_string()
}

/// `create`, `update`, `comment` and `comment edit`: the text from `--stdin-file` is what the same text on stdin
/// gives, trimmed the same way; a file that can't be read changes nothing.
#[tokio::test(flavor = "multi_thread")]
async fn the_users_commands_read_the_file_as_they_read_stdin() {
    let w = TestService::start().await;
    w.shop_sample().await;
    let dir = tempfile::tempdir().unwrap();
    let file = text_file(dir.path(), "text.md", TEXT);
    let p = ["-P", "SHOP"];

    let by_stdin = json(&cb_in(
        &w,
        &[&p[..], &["create", "By stdin", "--stdin", "--json"]].concat(),
        TEXT,
    ));
    let by_file = json(&cb_in(
        &w,
        &[
            &p[..],
            &["create", "By file", "--stdin-file", &file, "--json"],
        ]
        .concat(),
        "",
    ));
    assert_eq!(by_file["body"], by_stdin["body"]);
    assert_eq!(by_file["body"], TEXT.trim_end());
    let key = by_file["key"].as_str().unwrap().to_owned();

    let updated = json(&cb_in(
        &w,
        &[&p[..], &["update", &key, "--stdin-file", &file, "--json"]].concat(),
        "",
    ));
    assert_eq!(updated["body"], TEXT.trim_end());

    let commented_by_stdin = json(&cb_in(
        &w,
        &[&p[..], &["comment", &key, "--stdin", "--json"]].concat(),
        TEXT,
    ));
    let commented = json(&cb_in(
        &w,
        &[&p[..], &["comment", &key, "--stdin-file", &file, "--json"]].concat(),
        "",
    ));
    assert_eq!(commented["body"], commented_by_stdin["body"]);
    assert_eq!(commented["body"], TEXT.trim_end());
    let n = commented["n"].to_string();
    let edit = text_file(dir.path(), "edit.md", "Edited.\n");
    let edited = json(&cb_in(
        &w,
        &[
            &p[..],
            &["comment", "edit", &key, &n, "--stdin-file", &edit, "--json"],
        ]
        .concat(),
        "",
    ));
    assert_eq!(edited["body"], "Edited.");

    // A file that isn't there: a usage error naming it, and nothing is created.
    let before = w.board.tickets("SHOP").await.unwrap().len();
    let missing = dir.path().join("missing.md").display().to_string();
    let out = cb_in(
        &w,
        &[
            &p[..],
            &["create", "Nothing", "--stdin-file", &missing, "--json"],
        ]
        .concat(),
        "",
    );
    assert_eq!(out.status.code(), Some(1));
    let error: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(error["error"]["kind"], "usage", "{error}");
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap()
            .starts_with(&format!("can't read {missing}: ")),
        "{error}"
    );
    assert_eq!(w.board.tickets("SHOP").await.unwrap().len(), before);

    // Not both, and not with `--plan-stdin`, which reads stdin too.
    for args in [
        &["create", "Both", "--stdin", "--stdin-file", &file][..],
        &["update", &key, "--plan-stdin", "--stdin-file", &file],
    ] {
        let out = cb_in(&w, &[&p[..], args].concat(), "");
        assert_eq!(out.status.code(), Some(2), "{args:?}: {out:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("cannot be used with"),
            "{args:?}: {out:?}"
        );
    }
    assert_eq!(w.board.tickets("SHOP").await.unwrap().len(), before);
    w.stop().await;
}

/// An agent asks and ends its stage with `--stdin-file`, as with `--stdin`; `--stdin` keeps working beside it.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_asks_and_ends_its_stage_from_a_file() {
    let w = TestService::start().await;
    w.shop_sample().await;
    w.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    let ticket = w.board.ticket("SHOP-19").await.unwrap();
    let worktree = w.give_worktree(&ticket).await;
    let agent = w.agent_shell(&ticket).await;
    let dir = tempfile::tempdir().unwrap();

    let question = text_file(
        dir.path(),
        "question.md",
        "Split `fullName` everywhere?\nOnly the profile form uses it today.\n",
    );
    let ran = agent
        .run(&format!(
            "'{CB}' ask human --option Everywhere --option 'Profile only' --stdin-file '{question}' --json"
        ))
        .await;
    assert_eq!(ran.status, 0, "{ran:?}");
    let asked: Value = serde_json::from_str(&ran.stdout).unwrap();
    assert_eq!(asked["status"], "asks_you");
    assert_eq!(
        asked["signal"]["text"],
        "Split `fullName` everywhere?\nOnly the profile form uses it today."
    );

    let ran = agent.run(&format!("'{CB}' prime")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    std::fs::write(worktree.join("file1.txt"), "changed\n").unwrap();
    commit(&worktree);
    let summary = text_file(dir.path(), "summary.md", TEXT);
    let ran = agent
        .run(&format!("'{CB}' stage next --stdin-file '{summary}'"))
        .await;
    assert_eq!(
        (ran.status, ran.stdout.trim()),
        (0, "Finished Dev. SHOP-19 is Ready for Review."),
        "{ran:?}"
    );
    assert_eq!(w.status("SHOP-19").await, Status::WaitingForYou);
    let comments = w.board.ticket_comments("SHOP-19").await.unwrap().comments;
    assert_eq!(comments.last().unwrap().body, TEXT.trim());
    w.stop().await;
}

fn commit(dir: &Path) {
    let ok = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qam",
            "work",
        ])
        .current_dir(dir)
        .status()
        .unwrap()
        .success();
    assert!(ok);
}

/// What `otaq prime` prints in `shell`, with the test's own folders named by placeholders.
async fn prime(w: &TestService, shell: &AgentShell, worktree: &Path) -> String {
    let ran = shell.run(&format!("'{CB}' prime")).await;
    assert_eq!(ran.status, 0, "{ran:?}");
    ran.stdout
        .replace(&worktree.display().to_string(), "<worktree>")
        .replace(&w.data.root().display().to_string(), "<data>")
}

/// The working and validating primes of a session on each tool, as `<tool>.<kind>.md` in `fixtures/prime-by-tool/`.
fn fixture(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/prime-by-tool")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The prime of a Claude Code, Copilot and OpenCode session is byte for byte what it was before `--stdin-file`
/// (the fixtures were printed by the build before it, a1fc97c4), for a ticket's agent and its checker; a Codex
/// session's is that text with the "Long text in Codex" section after it, naming a file of its own session in the
/// temporary folder.
#[tokio::test(flavor = "multi_thread")]
async fn only_a_codex_sessions_prime_gains_the_file_rule() {
    for tool in [Tool::Claude, Tool::Copilot, Tool::OpenCode, Tool::Codex] {
        // A board of its own per tool, so each prints from the same state.
        let w = TestService::start().await;
        w.shop_sample().await;
        let ticket = w.board.ticket("SHOP-19").await.unwrap();
        let worktree = w.give_worktree(&ticket).await;
        let agent = w.shell_on(&ticket, SessionKind::Agent, Some(tool)).await;
        let working = prime(&w, &agent, &worktree).await;
        let validator = w
            .shell_on(&ticket, SessionKind::Validator, Some(tool))
            .await;
        w.validating_with(&ticket, &validator.session).await;
        let validating = prime(&w, &validator, &worktree).await;

        let was_working = fixture(&format!("{tool}.working.md"));
        let was_validating = fixture(&format!("{tool}.validating.md"));
        if tool == Tool::Codex {
            for (now, was, shell) in [
                (&working, &was_working, &agent),
                (&validating, &was_validating, &validator),
            ] {
                let file = otaq_core::prime::long_text_file(&shell.session);
                let section = otaq_core::prime::render_long_text_file(Some(&file));
                assert_eq!(
                    *now,
                    format!("{}\n{}\n", was.trim_end(), section.trim_end())
                );
                assert!(
                    file.starts_with(std::env::temp_dir())
                        && file.ends_with(format!("otaq-{}.md", shell.session)),
                    "{}",
                    file.display()
                );
                assert!(
                    now.contains(&format!("--stdin-file \"{}\"", file.display())),
                    "{now}"
                );
            }
        } else {
            assert_eq!(working, was_working, "{tool}: the working prime changed");
            assert_eq!(
                validating, was_validating,
                "{tool}: the validating prime changed"
            );
            assert!(!working.contains("--stdin-file") && !validating.contains("--stdin-file"));
        }
        w.stop().await;
    }
}
