//! M36's example end to end (T-0448; CMT-15; d0109, d0056, d0079): shop-web (SHOP) on the Spec first preset with
//! Product spec its only planning stage, SHOP-34 "Export orders as CSV", with a stand-in `claude` that runs the real
//! `otaq`. Product spec reads `prime`, writes Agreed into the description and ends with comment 1; the user posts
//! comment 2 while Dev runs (it isn't typed into Dev's session); Dev's `prime` shows both and it ends with comment 3;
//! Validate saves its run in `$OTAQ_RESOURCE_DIR` and fails with it as a report (comment 4); Dev is told only "Validate
//! failed round 1. Run `otaq prime`.", its `stage next` is refused until it runs `prime` (which names the report and
//! never prints it), it greps the report through `otaq report --path`, ends again; Validate passes; the ticket reaches
//! Code review. Every comment's author, stage and round, and History's lines, are checked; a 2,001-character comment
//! and a 21 MB report are refused.
//!
//! No agent tool runs; every folder is temporary (the stand-in's `HOME` too), and the stand-in's PATH has no other
//! `otaq` (`path_without_build_folders`): it only ever runs this build's, by its full path.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::board::moves::DropTarget;
use otaq_core::board::ticket_comments::{CommentAuthor, TicketComment};
use otaq_core::board::tickets::{EventKind, NewTicketInput};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Actor, Place, Status};
use otaq_core::flow::presets::{SPEC_FIRST_FILE, preset};
use otaq_core::naming;
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, path_without_build_folders};
use otaq_core::session::SessionKind;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

const DESCRIPTION: &str = "Admins need the orders as a file for the accountant.";
const AGREED: &str = "\n\n## Agreed\n\n- CSV only, no Excel.\n- Admins only.\n- Reuse the CSV writer in export.rs (SHOP-12).\n";
const PRODUCT_SPEC_SAYS: &str =
    "Agreed with you: CSV only, admins only. Reuse the CSV writer in export.rs from SHOP-12.";
const YOU_SAY: &str = "Also keep the column order the same as the table on screen.";
const DEV_SAYS: &str =
    "Exported the orders as CSV with the table's column order. Tests in export_tests.rs.";
const VALIDATE_SAYS: &str =
    "Fail: the 10,000 row limit isn't enforced; 3 tests fail in export_tests.rs.";
const DEV_FIXED: &str = "Enforced the 10,000 row limit; export_tests.rs passes.";
const VALIDATE_PASSES: &str = "Pass: the row limit holds and export_tests.rs passes.";

fn me() -> Actor {
    Actor::User("me".into())
}

/// The stand-in `claude`. Its state is in `state` (Otaq's data folder in `state/data`, so its `otaq` reaches the
/// test's service); it logs its arguments, then acts by the skill it was started with:
/// - Product spec (`/otaq-product-spec`): `prime`, the description with Agreed (`otaq update --stdin`), `stage next`
///   with comment 1, its turn ends;
/// - Dev (`/otaq-work-ticket`): waits for `state/go-dev` (the user's comment comes first), `prime`, a commit, `stage
///   next` with comment 3, its turn ends; then records each line typed into it, and on a failed check tries `stage
///   next` first (refused), runs `prime`, greps the report through `otaq report --path`, commits and ends again;
/// - Validate (`/otaq-validate`): `prime --validate`, notes `$OTAQ_RESOURCE_DIR` and its folder; round 1 writes a
///   ~48 KB test run into `$OTAQ_RESOURCE_DIR` and fails with it as the report; round 2 passes.
///
/// Nothing typed into it is ever run: lines are only read and matched.
fn stand_in(state: &Path) -> String {
    let state = state.display();
    let data_var = DataDir::env_var();
    format!(
        r#"
export HOME='{state}/home'
STATE='{state}'
export {data_var}="$(cat "$STATE/data")"
printf 'ARGS %s\n' "$*"
otaq() {{
    '{CB}' "$@" 2>> "$STATE/errors"
}}
stop() {{
    printf '%s' '{{"hook_event_name":"Stop","background_tasks":[],"session_crons":[]}}' | otaq hook --tool claude Stop
}}
commit() {{
    echo "$1" >> export.rs
    git add -A
    git -c user.name=t -c user.email=t@t commit -qm "$OTAQ_TICKET: $1"
}}
case "$*" in
*otaq-product-spec*)
    otaq prime > "$STATE/product-spec-prime.md"
    otaq update --stdin < "$STATE/agreed.md" > /dev/null
    otaq stage next --stdin < "$STATE/say-product-spec"
    stop
    exec sleep 300
    ;;
*otaq-validate*)
    if [ -e "$STATE/failed" ]; then round=2; else round=1; fi
    otaq prime --validate > "$STATE/check-prime-$round.md"
    printf '%s\n' "${{OTAQ_RESOURCE_DIR-unset}}" > "$STATE/resource-dir"
    # Git for Windows' sh prints /c/... for a plain pwd; -W gives the Windows path.
    {{ pwd -W 2>/dev/null || pwd -P; }} > "$STATE/check-folder"
    if [ "$round" = 1 ]; then
        awk 'BEGIN {{
            for (i = 1; i <= 1620; i++) printf "test export::case_%04d ... ok\n", i
            print "test export_tests::row_limit_10000 ... FAILED"
            print "test export_tests::row_limit_10001 ... FAILED"
            print "test export_tests::row_limit_paged ... FAILED"
            print "test result: FAILED. 1620 passed; 3 failed"
        }}' > "$OTAQ_RESOURCE_DIR/test-run.txt"
        otaq check fail --stdin --report-file "$OTAQ_RESOURCE_DIR/test-run.txt" < "$STATE/say-validate-fails"
        touch "$STATE/failed"
    else
        otaq check pass --stdin < "$STATE/say-validate-passes"
    fi
    stop
    exec sleep 300
    ;;
*otaq-work-ticket*)
    while [ ! -e "$STATE/go-dev" ]; do sleep 0.05; done
    otaq prime > "$STATE/dev-prime-1.md"
    commit 'pub fn export_csv() {{}}'
    otaq stage next --stdin < "$STATE/say-dev"
    stop
    while IFS= read -r line; do
        printf '%s\n' "$line" >> "$STATE/dev-told.txt"
        case "$line" in
        *'failed round'*)
            printf 'Fixed.' | '{CB}' stage next --stdin > /dev/null 2> "$STATE/dev-refused.txt"
            otaq prime > "$STATE/dev-prime-2.md"
            report=$(otaq report "$OTAQ_TICKET" 4 --path)
            printf '%s\n' "$report" > "$STATE/dev-report-path.txt"
            grep -n FAILED "$report" > "$STATE/dev-grep.txt"
            commit 'const ROW_LIMIT: usize = 10_000;'
            otaq stage next --stdin < "$STATE/say-dev-fixed"
            stop
            ;;
        esac
    done
    exec sleep 300
    ;;
esac
exec sleep 300
"#
    )
}

/// The user's terminal: no ticket variable, the test's data folder.
fn cb(w: &TestService, args: &[&str], stdin: &str) -> Output {
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
    // The CLI may refuse before it reads stdin (a report over the limit): a closed pipe is fine.
    let _ = child.stdin.take().unwrap().write_all(stdin.as_bytes());
    child.wait_with_output().unwrap()
}

fn refusal(out: &Output) -> String {
    assert!(
        !out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    err.lines()
        .next()
        .unwrap_or_default()
        .trim_start_matches("error: ")
        .to_owned()
}

fn read(state: &Path, file: &str) -> String {
    std::fs::read_to_string(state.join(file)).unwrap_or_else(|e| panic!("{file}: {e}"))
}

/// Waits (two minutes at most) until `done` holds; else fails with the ticket, its runs, its sessions' screens and
/// what the stand-in's `otaq` printed to stderr.
async fn until(
    w: &TestService,
    state: &Path,
    key: &str,
    what: &str,
    done: impl AsyncFn(&TestService) -> bool,
) {
    let reached = tokio::time::timeout(Duration::from_secs(120), async {
        while !done(w).await {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await;
    if reached.is_err() {
        let t = w.board.ticket(key).await.unwrap();
        let runs = w.board.stage_runs(key).await.unwrap();
        let screens: Vec<String> = [SessionKind::Agent, SessionKind::Validator]
            .into_iter()
            .filter_map(|kind| w.sessions.running_of(&t.id, kind))
            .map(|s| {
                let attached = w.sessions.attach(&s.id).expect("session in memory");
                String::from_utf8_lossy(&attached.scrollback).into_owned()
            })
            .collect();
        panic!(
            "{key} never {what}: {} in {:?} {:?}\n{runs:#?}\n{}\n---- stderr\n{}",
            t.status,
            t.place,
            t.start_problem,
            screens.join("\n----\n"),
            std::fs::read_to_string(state.join("errors")).unwrap_or_default()
        );
    }
}

async fn comments(w: &TestService, key: &str) -> Vec<TicketComment> {
    w.board.ticket_comments(key).await.unwrap().comments
}

/// An agent's comment: (name, stage, round).
fn agent(c: &TicketComment) -> (Option<&str>, Option<&str>, Option<u32>) {
    match &c.author {
        CommentAuthor::Agent {
            name, stage, round, ..
        } => (name.as_deref(), stage.as_deref(), *round),
        other => panic!("comment {} isn't an agent's: {other:?}", c.n),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_example_hands_over_through_comments_and_a_report() {
    let state_dir = tempfile::TempDir::new().unwrap();
    let state = state_dir.path();
    std::fs::create_dir(state.join("home")).unwrap();
    let script = stand_in(state);
    let w = TestService::start_with(move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: path_without_build_folders().into(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    })
    .await;
    std::fs::write(state.join("data"), w.data.root().display().to_string()).unwrap();
    std::fs::write(state.join("agreed.md"), format!("{DESCRIPTION}{AGREED}")).unwrap();
    for (file, text) in [
        ("say-product-spec", PRODUCT_SPEC_SAYS),
        ("say-dev", DEV_SAYS),
        ("say-validate-fails", VALIDATE_SAYS),
        ("say-dev-fixed", DEV_FIXED),
        ("say-validate-passes", VALIDATE_PASSES),
    ] {
        std::fs::write(state.join(file), text).unwrap();
    }
    w.add_project("shop-web").await;
    let mut flow = preset(SPEC_FIRST_FILE).unwrap().read();
    flow.flow
        .stages
        .retain(|s| !["tech-spec", "milestone-plan", "spec-review"].contains(&s.id.as_str()));
    w.board.use_flow_for_tests("SHOP", &flow).await;
    for _ in 1..34 {
        w.create("SHOP", "filler").await;
    }
    let t = w
        .board
        .create_ticket(
            &NewTicketInput {
                project: "SHOP".into(),
                title: "Export orders as CSV".into(),
                body: DESCRIPTION.into(),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let key = t.key.to_string();
    assert_eq!(key, "SHOP-34");

    // Product spec: comment 1 and the Agreed section; the ticket goes on to Dev, whose agent waits for the go.
    w.board
        .move_ticket(&key, DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    w.starter
        .start(&key, &me(), otaq_core::session::Start::Auto)
        .await
        .unwrap();
    until(&w, state, &key, "started Dev", async |w| {
        let t = w.board.ticket(&key).await.unwrap();
        t.place == Place::stage_of("dev")
            && t.status == Status::Working
            && w.sessions.running_of(&t.id, SessionKind::Agent).is_some()
    })
    .await;
    let product_spec_prime = read(state, "product-spec-prime.md");
    assert!(
        product_spec_prime.contains("\n## Your stage\n\nProduct spec, as Spec writer"),
        "{product_spec_prime}"
    );
    assert_eq!(
        w.board.ticket(&key).await.unwrap().body,
        format!("{DESCRIPTION}{AGREED}").trim_end()
    );
    let dev = w.sessions.running_of(&t.id, SessionKind::Agent).unwrap();
    w.wait_for_output(&dev.id, &format!("/{}", naming::copy_name("work-ticket")))
        .await;

    // The user's comment while Dev runs: posted, never typed into Dev's session.
    let posted = cb(&w, &["comment", &key, "--stdin"], YOU_SAY);
    assert!(posted.status.success(), "{posted:?}");
    std::fs::write(state.join("go-dev"), "").unwrap();

    // Dev → Validate fails round 1 → Dev fixes → Validate passes → Code review.
    until(&w, state, &key, "reached Code review", async |w| {
        let t = w.board.ticket(&key).await.unwrap();
        t.place == Place::stage_of("code-review") && t.status == Status::WaitingForYou
    })
    .await;

    // Dev's first prime: comments 1 and 2, in order, with their headings.
    let prime = read(state, "dev-prime-1.md");
    let section = prime
        .split("\n## Comments\n")
        .nth(1)
        .unwrap_or_else(|| panic!("no comments in Dev's prime: {prime}"));
    let first = section.find(PRODUCT_SPEC_SAYS).expect(PRODUCT_SPEC_SAYS);
    let second = section.find(YOU_SAY).expect(YOU_SAY);
    assert!(first < second, "{section}");
    assert!(
        section.contains("\n### Spec writer \u{b7} Product spec \u{b7} "),
        "{section}"
    );
    assert!(section.contains("\n### You \u{b7} "), "{section}");
    assert!(
        prime.contains("## Agreed"),
        "the description is the plan: {prime}"
    );

    // Every comment's author, stage and round; only the fail has a report.
    let all = comments(&w, &key).await;
    let bodies: Vec<(u32, &str)> = all.iter().map(|c| (c.n, c.body.as_str())).collect();
    assert_eq!(
        bodies,
        [
            (1, PRODUCT_SPEC_SAYS),
            (2, YOU_SAY),
            (3, DEV_SAYS),
            (4, VALIDATE_SAYS),
            (5, DEV_FIXED),
            (6, VALIDATE_PASSES),
        ]
    );
    assert_eq!(
        agent(&all[0]),
        (Some("Spec writer"), Some("product-spec"), None)
    );
    assert!(
        matches!(all[1].author, CommentAuthor::User { .. }),
        "{:?}",
        all[1].author
    );
    assert_eq!(agent(&all[2]), (Some("Developer"), Some("dev"), None));
    assert_eq!(
        agent(&all[3]),
        (Some("Validator"), Some("validate"), Some(1))
    );
    assert_eq!(agent(&all[4]), (Some("Developer"), Some("dev"), None));
    assert_eq!(
        agent(&all[5]),
        (Some("Validator"), Some("validate"), Some(2))
    );
    for c in [&all[0], &all[2], &all[3], &all[4], &all[5]] {
        assert!(
            matches!(
                c.author,
                CommentAuthor::Agent {
                    tool: Some(Tool::Claude),
                    ..
                }
            ),
            "{:?}",
            c.author
        );
    }
    assert!(
        all[3]
            .heading()
            .starts_with("Validator \u{b7} Validate \u{b7} round 1 \u{b7} "),
        "{}",
        all[3].heading()
    );
    let with_reports: Vec<u32> = all
        .iter()
        .filter(|c| c.report.is_some())
        .map(|c| c.n)
        .collect();
    assert_eq!(with_reports, [4]);

    // Validate's run: saved in $OTAQ_RESOURCE_DIR, outside the worktree, copied whole as comment 4's report.
    let worktree = std::fs::canonicalize(
        w.board
            .ticket(&key)
            .await
            .unwrap()
            .work
            .unwrap()
            .worktree
            .unwrap(),
    )
    .unwrap();
    let resource_dir = read(state, "resource-dir").trim().to_owned();
    assert_ne!(
        resource_dir, "unset",
        "Validate's session has OTAQ_RESOURCE_DIR"
    );
    let resource_dir = std::fs::canonicalize(&resource_dir).unwrap();
    assert_eq!(
        std::fs::canonicalize(read(state, "check-folder").trim()).unwrap(),
        worktree,
        "the check runs in the worktree"
    );
    assert!(
        !resource_dir.starts_with(&worktree) && !worktree.starts_with(&resource_dir),
        "{resource_dir:?} is outside {worktree:?}"
    );
    let run = std::fs::read(resource_dir.join("test-run.txt")).unwrap();
    assert!(
        (45 * 1024..52 * 1024).contains(&run.len()),
        "a ~48 KB run: {}",
        run.len()
    );
    let report = w.board.ticket_report(&key, 4).await.unwrap();
    assert_eq!(std::fs::read(&report.path).unwrap(), run);
    assert_eq!(report.info.bytes, run.len() as u64);
    assert!(Path::new(&report.path).starts_with(w.data.root()));

    // Dev was told the round and `prime`, never the findings, and never the user's comment.
    let told = read(state, "dev-told.txt");
    assert!(
        told.contains("Validate failed round 1. Run `otaq prime`."),
        "{told}"
    );
    for findings in ["10,000", "export_tests.rs", YOU_SAY] {
        assert!(
            !told.contains(findings),
            "{findings} typed into Dev: {told}"
        );
    }
    let screen =
        String::from_utf8_lossy(&w.sessions.attach(&dev.id).unwrap().scrollback).into_owned();
    assert!(!screen.contains("column order"), "{screen}");

    // Its `stage next` before `prime` was refused, counting comment 4 but not its own comment 3 (Q388); the next prime
    // names the report and never prints it.
    assert_eq!(
        read(state, "dev-refused.txt").trim(),
        "error: Run `otaq prime` first: SHOP-34 has 1 comment you haven't read."
    );
    let prime = read(state, "dev-prime-2.md");
    assert!(prime.contains(VALIDATE_SAYS), "{prime}");
    let line = all[3].report_line(&t.key).unwrap();
    assert!(
        line.starts_with("Report: 48 KB, 1,624 lines \u{b7} `otaq report SHOP-34 4`"),
        "{line}"
    );
    assert!(prime.contains(&line), "{prime}");
    for text in ["FAILED", "case_0001", "test result"] {
        assert!(!prime.contains(text), "the report's text ({text}) in prime");
    }
    assert_eq!(read(state, "dev-report-path.txt").trim(), report.path);
    let grep = read(state, "dev-grep.txt");
    assert!(
        grep.starts_with("1621:test export_tests::row_limit_10000 ... FAILED\n"),
        "{grep}"
    );
    assert_eq!(grep.lines().count(), 4, "{grep}");
    let check = read(state, "check-prime-2.md");
    assert!(
        check.contains(DEV_FIXED) && check.contains(&line),
        "{check}"
    );

    // History (newest first): Spec writer's description edit and every comment.
    let mut history = w.board.history(&key).await.unwrap();
    history.reverse();
    let edit = history
        .iter()
        .find(|e| e.kind == EventKind::Updated)
        .expect("the description edit");
    assert_eq!(edit.payload["author"]["kind"], "agent", "{edit:?}");
    assert_eq!(edit.payload["author"]["name"], "Spec writer", "{edit:?}");
    assert_eq!(edit.payload["author"]["stage"], "product-spec", "{edit:?}");
    let posted: Vec<u64> = history
        .iter()
        .filter(|e| e.kind == EventKind::CommentPosted)
        .map(|e| e.payload["n"].as_u64().unwrap())
        .collect();
    assert_eq!(posted, [1, 2, 3, 4, 5, 6]);

    // Over the limits: refused with their texts, nothing posted.
    let long = "x".repeat(2_001);
    let out = cb(&w, &["comment", &key, "--stdin"], &long);
    assert_eq!(
        refusal(&out),
        "Comment is 2,001 characters; the limit is 2,000. Put the full text in a report with `--report-file` and \
         keep the comment to what the next agent needs."
    );
    let big = state.join("big-run.txt");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(21 * 1024 * 1024)
        .unwrap();
    let out = cb(
        &w,
        &[
            "comment",
            &key,
            "--stdin",
            "--report-file",
            &big.display().to_string(),
        ],
        "The whole run.",
    );
    assert_eq!(
        refusal(&out),
        "The report is 21 MB; the limit is 20 MB. Keep the part that matters, for example the failing tests."
    );
    assert_eq!(comments(&w, &key).await.len(), 6);
    w.stop().await;
}
