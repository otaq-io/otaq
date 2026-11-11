//! The Merge check (T-0491; d0120, Q475) against temporary repositories: Local merge runs the project's command on
//! the merged result in Otaq's scratch worktree before the target moves, only when merging the target in brought in commits the
//! branch didn't have. The repository's own folder stands for the user's checkout; the commands are plain `sh`
//! scripts, and the ticket's agent is an `sh` session.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::board::settings::EnvVar;
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, AutoMergeStop, MergeCheckState, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::naming;
use otaq_core::process::testing::sh_one_line;
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::testing::TestService;
use otaq_core::skills::testing::claude_skill;

fn me() -> Actor {
    Actor::User("me".into())
}

/// The project's test: once `guard` exists, no `.ts` file may say "emerald" (DEMO-11's guard test, finding 46).
const TEST_SH: &str = "if [ -f guard ] && grep -l emerald *.ts; then echo 'FAIL: emerald is used'; exit 1; fi\n\
                       echo ok\n";

async fn set_check(w: &TestService, command: &str) {
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.merge_check = Some(command.to_owned());
    w.board
        .save_project_settings("SHOP", &settings)
        .await
        .unwrap();
}

/// A ticket in Ready for Review with one commit on `file` in its worktree.
async fn reviewed(w: &TestService, title: &str, file: &str, content: &str) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", title).await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, file, content);
    w.force_status(&t, Status::WaitingForYou).await;
    (w.board.ticket(&t.key.to_string()).await.unwrap(), wt)
}

async fn merge(w: &TestService, key: &str) -> LocalMergeOutcome {
    w.review
        .local_merge(key, &format!("{key}: merged"), true, &me())
        .await
        .unwrap()
}

fn main_of(repo: &Path) -> String {
    git(repo, &["rev-parse", "main"])
}

/// Every file of the user's checkout, ignored and untracked ones too, outside `.git`.
fn checkout_status(repo: &Path) -> String {
    git(
        repo,
        &[
            "status",
            "--porcelain",
            "--ignored",
            "--untracked-files=all",
        ],
    )
}

/// Finding 46: A and B each pass `test.sh` alone; A merges, then B, whose combination with A fails. B isn't merged,
/// `main` stays at A, and B is back at its Agent stage with the comment and the output as its report.
#[tokio::test(flavor = "multi_thread")]
async fn a_combination_that_fails_the_check_is_not_merged_and_goes_back_to_the_agent() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    commit(&repo, "test.sh", TEST_SH);
    set_check(&w, "sh test.sh").await;
    let (a, _) = reviewed(&w, "Accent token", "guard", "no emerald\n").await;
    let (b, _) = reviewed(&w, "Card", "card.ts", "bg-emerald-500\n").await;
    let agent = w.agent_shell(&b).await;

    let merged = merge(&w, "SHOP-1").await;
    assert!(
        matches!(merged, LocalMergeOutcome::Merged { .. }),
        "{merged:?}"
    );
    let main = main_of(&repo);

    let outcome = merge(&w, "SHOP-2").await;
    assert_eq!(
        outcome,
        LocalMergeOutcome::CheckFailed {
            target: "main".into(),
            command: "sh test.sh".into(),
            exit: Some(1),
            output: "card.ts\nFAIL: emerald is used".into(),
        },
        "the dialog has what the check printed"
    );
    assert_eq!(main_of(&repo), main, "main unchanged");
    assert_eq!(w.status("SHOP-1").await, Status::Merged);
    let back = w.board.ticket("SHOP-2").await.unwrap();
    assert_eq!(back.status, Status::MergeConflictFixing);
    assert_eq!(back.flags.merge_check, Some(MergeCheckState::Failed));
    assert_eq!(
        back.flags.merge_check.unwrap().label(),
        "Merge check failed \u{b7} fixing"
    );
    assert_eq!(
        back.flags.conflicts_with, None,
        "not a conflict on the card"
    );
    assert_eq!(back.auto_merge_conflicts, 1);
    // The comment, with the output attached as its report (d0109).
    let comments = w.board.ticket_comments("SHOP-2").await.unwrap().comments;
    let comment = comments.last().expect("a comment");
    assert_eq!(
        comment.body,
        "Merge check `sh test.sh` failed on the result with main merged in (exit 1). Fix it, then submit again."
    );
    let report = w
        .board
        .ticket_report("SHOP-2", comment.n, None)
        .await
        .unwrap();
    let output = std::fs::read_to_string(&report.attachment.path).unwrap();
    assert!(output.contains("FAIL: emerald is used"), "{output}");
    // The agent gets restack, and where to read why (CMT-05: never the comment's text). This shell wasn't started
    // with the skill, so the skill is named by its file (d0118).
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    let typed = &pending.last().unwrap().text;
    assert!(typed.contains(&claude_skill("restack")), "{typed}");
    assert!(
        typed.ends_with(&format!(
            "SHOP-2's merge check failed on the result with `main` merged in; the comment has its output. Run `{} \
             prime`.",
            naming::COMMAND_NAME
        )),
        "{typed}"
    );
    // Waiting as "1 message waiting" it isn't pending yet; typed (Deliver now), the card says it waits for the agent
    // to pick it up (d0118).
    assert_eq!(back.flags.hand_off, None);
    w.deliveries.deliver_now(&agent.session).await.unwrap();
    assert_eq!(
        w.board.ticket("SHOP-2").await.unwrap().flags.hand_off,
        Some(otaq_core::domain::HandOffKind::MergeCheck)
    );
    // `prime` tells it what to resolve first.
    let prime = otaq_core::prime::render(
        &w.board
            .prime_context("SHOP-2", w.adapters.for_new_session())
            .await
            .unwrap(),
    );
    assert!(
        prime.contains(
            "The project's merge check failed on this branch with the latest `main` merged in"
        ),
        "{prime}"
    );
    let _ = a;
    w.stop().await;
}

/// The release test's case: the agent sits idle at its prompt with text the user left in its input line when the
/// merge check fails. The restack request waits behind that text, as every automatic message does, and the line
/// stays as the user left it; once the user empties it the request is typed, alone.
#[tokio::test(flavor = "multi_thread")]
async fn the_restack_request_waits_behind_unsent_text_and_is_typed_once_the_line_is_empty() {
    use otaq_core::agent_tools::Tool;
    use otaq_core::caller::Caller;
    use otaq_core::client::{Client, ClientConfig};
    use otaq_core::protocol::{InputParams, Request, encode_bytes};

    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    commit(&repo, "test.sh", TEST_SH);
    set_check(&w, "sh test.sh").await;
    let (_a, _) = reviewed(&w, "Accent token", "guard", "no emerald\n").await;
    let (b, _) = reviewed(&w, "Card", "card.ts", "bg-emerald-500\n").await;
    let agent = w.agent_shell(&b).await;
    // Its turn ended: an automatic message would be typed at once into an empty line.
    let caller = Caller::Agent {
        ticket: b.key.clone(),
        ticket_id: b.id.clone(),
        session: agent.session.clone(),
    };
    w.signals
        .hook(
            &caller,
            Tool::Claude,
            "Stop",
            &serde_json::json!({"background_tasks": [], "session_crons": []}),
        )
        .await
        .unwrap();
    assert!(w.deliveries.is_ready(&agent.session).unwrap());
    // The user's half-typed line, from the app's terminal.
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let typing = |bytes: &[u8]| {
        Request::SessionInput(InputParams {
            session: agent.session.clone(),
            data: encode_bytes(bytes),
            reply: false,
            if_empty: false,
        })
    };
    client.request(typing(b"half-typed")).await.unwrap();
    let shown = || {
        String::from_utf8_lossy(&w.sessions.attach(&agent.session).unwrap().scrollback).into_owned()
    };
    wait_until("the user's text shows", || shown().contains("half-typed")).await;

    assert!(matches!(
        merge(&w, "SHOP-1").await,
        LocalMergeOutcome::Merged { .. }
    ));
    assert!(matches!(
        merge(&w, "SHOP-2").await,
        LocalMergeOutcome::CheckFailed { .. }
    ));
    assert_eq!(w.status("SHOP-2").await, Status::MergeConflictFixing);

    // It waits: nothing typed over the user's text, which is still there to send or clear.
    let restack = claude_skill("restack");
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(pending.len(), 1, "{pending:?}");
    assert!(pending[0].text.contains(&restack), "{}", pending[0].text);
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert!(!shown().contains(&restack), "typed over the user's text");
    assert!(w.sessions.has_unsent_input(&agent.session).unwrap());
    assert_eq!(
        w.board
            .ticket("SHOP-2")
            .await
            .unwrap()
            .flags
            .messages_waiting,
        1
    );

    // The user deletes their text: the line is empty, and the request goes, alone.
    client
        .request(typing(&[0x7f; "half-typed".len()]))
        .await
        .unwrap();
    wait_until("the restack request is typed", || {
        shown().contains(&restack)
    })
    .await;
    tokio::time::timeout(Duration::from_secs(10), async {
        while !w
            .deliveries
            .pending(&agent.session)
            .await
            .unwrap()
            .is_empty()
        {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("nothing waits any more");
    // On screen the line holds the request alone: the user's text went before it, not under it.
    let mut screen = vt100::Parser::new(50, 400, 0);
    screen.process(shown().as_bytes());
    let line = screen
        .screen()
        .contents()
        .lines()
        .find(|l| l.contains(&restack))
        .unwrap_or_default()
        .to_owned();
    assert!(line.starts_with(&restack), "{line:?}");
    drop(client);
    w.stop().await;
}

/// Re-test 5's case: a line pasted into a Codex session with Ctrl+V. Where the terminal's own paste is Ctrl+Shift+V,
/// Ctrl+V reaches Codex as a key and Codex pastes the clipboard itself, so the whole line arrives in one piece that
/// no keystroke carried; the tester then typed one more character. One Backspace brought the keys' count to
/// nothing, and the restack request was typed after the rest of the text, both sent as one message. The screen
/// decides: while it shows text after the prompt marker the request waits, and once the line shows empty it is typed
/// alone.
#[tokio::test(flavor = "multi_thread")]
async fn the_restack_request_waits_behind_a_pasted_line_after_one_backspace() {
    use otaq_core::agent_tools::Tool;
    use otaq_core::caller::Caller;
    use otaq_core::client::{Client, ClientConfig};
    use otaq_core::protocol::{InputParams, Request, encode_bytes};

    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    commit(&repo, "test.sh", TEST_SH);
    set_check(&w, "sh test.sh").await;
    let (_a, _) = reviewed(&w, "Accent token", "guard", "no emerald\n").await;
    let (b, _) = reviewed(&w, "Card", "card.ts", "bg-emerald-500\n").await;
    let agent = w.agent_shell(&b).await;
    let shown = || {
        String::from_utf8_lossy(&w.sessions.attach(&agent.session).unwrap().scrollback).into_owned()
    };
    // The row the cursor is on, as the screen shows it.
    let input_row = || {
        let mut screen = vt100::Parser::new(50, 400, 0);
        screen.process(shown().as_bytes());
        let (row, _) = screen.screen().cursor_position();
        screen
            .screen()
            .rows(0, 400)
            .nth(usize::from(row))
            .unwrap_or_default()
    };
    // An agent tool's input line starts with its prompt marker.
    w.sessions.write(&agent.session, b"PS1='> '\r").unwrap();
    wait_until("the prompt marker shows", || input_row().trim_end() == ">").await;
    let caller = Caller::Agent {
        ticket: b.key.clone(),
        ticket_id: b.id.clone(),
        session: agent.session.clone(),
    };
    w.signals
        .hook(
            &caller,
            Tool::Claude,
            "Stop",
            &serde_json::json!({"background_tasks": [], "session_crons": []}),
        )
        .await
        .unwrap();
    assert!(w.deliveries.is_ready(&agent.session).unwrap());

    // The pasted line, put in by the program in one piece, then one typed character from the app's terminal.
    let pasted = "unsent draft line left by the tester retest";
    w.sessions.write(&agent.session, pasted.as_bytes()).unwrap();
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let typing = |bytes: &[u8]| {
        Request::SessionInput(InputParams {
            session: agent.session.clone(),
            data: encode_bytes(bytes),
            reply: false,
            if_empty: false,
        })
    };
    client.request(typing(b"5")).await.unwrap();
    wait_until("the user's text shows", || {
        input_row().trim_end() == format!("> {pasted}5")
    })
    .await;

    assert!(matches!(
        merge(&w, "SHOP-1").await,
        LocalMergeOutcome::Merged { .. }
    ));
    assert!(matches!(
        merge(&w, "SHOP-2").await,
        LocalMergeOutcome::CheckFailed { .. }
    ));
    let restack = claude_skill("restack");
    let pending = w.deliveries.pending(&agent.session).await.unwrap();
    assert_eq!(pending.len(), 1, "{pending:?}");

    // One Backspace: the keys counted nothing more, but the screen still shows the pasted text. It waits.
    client.request(typing(&[0x7f])).await.unwrap();
    wait_until("the Backspace shows", || {
        input_row().trim_end() == format!("> {pasted}")
    })
    .await;
    tokio::time::sleep(otaq_core::session::KEY_SETTLE * 3).await;
    assert!(!shown().contains(&restack), "typed onto the pasted text");
    assert!(w.sessions.has_unsent_input(&agent.session).unwrap());
    assert_eq!(w.deliveries.pending(&agent.session).await.unwrap().len(), 1);

    // The user empties the line: the request goes, alone.
    client
        .request(typing(&vec![0x7f; pasted.len()]))
        .await
        .unwrap();
    wait_until("the restack request is typed", || {
        shown().contains(&restack)
    })
    .await;
    let mut screen = vt100::Parser::new(50, 400, 0);
    screen.process(shown().as_bytes());
    let line = screen
        .screen()
        .contents()
        .lines()
        .find(|l| l.contains(&restack))
        .unwrap_or_default()
        .to_owned();
    assert!(
        line.strip_prefix("> ")
            .is_some_and(|rest| rest.starts_with(&restack)),
        "{line:?}"
    );
    drop(client);
    w.stop().await;
}

/// The branch already has the target's commits (nothing moved since it was checked): the command doesn't run.
#[tokio::test(flavor = "multi_thread")]
async fn the_check_does_not_run_when_the_target_did_not_move() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let marks = tempfile::tempdir().unwrap();
    let ran = marks.path().join("ran");
    set_check(&w, &format!("touch '{}'", ran.display())).await;
    reviewed(&w, "Checkout API", "api.ts", "api\n").await;

    let outcome = merge(&w, "SHOP-1").await;
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    assert!(!ran.exists(), "the check didn't run");
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s", "main"]),
        "SHOP-1: merged"
    );
    w.stop().await;
}

/// No Merge check: the combination that fails merges, as Local merge did before (d0120).
#[tokio::test(flavor = "multi_thread")]
async fn no_merge_check_merges_as_today() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    commit(&repo, "test.sh", TEST_SH);
    reviewed(&w, "Accent token", "guard", "no emerald\n").await;
    reviewed(&w, "Card", "card.ts", "bg-emerald-500\n").await;
    for key in ["SHOP-1", "SHOP-2"] {
        let outcome = merge(&w, key).await;
        assert!(
            matches!(outcome, LocalMergeOutcome::Merged { .. }),
            "{key}: {outcome:?}"
        );
    }
    assert!(repo.join("guard").is_file() && repo.join("card.ts").is_file());
    w.stop().await;
}

/// The command runs in the scratch worktree seeded copy-on-write from the ticket's build (a file only in its
/// gitignored `build/` is there), with the project's variables and Otaq's own; the scratch worktree is clean after,
/// and nothing is written to the user's checkout. Where the temporary folder's disk can't clone (ext4 on Linux CI),
/// nothing is seeded (WRM-01: never a plain copy), and the variables still arrive.
#[tokio::test(flavor = "multi_thread")]
async fn the_check_sees_the_variables_and_the_seeded_build_and_leaves_nothing_behind() {
    sees_the_variables_and_leaves_nothing_behind(TestService::start().await).await;
}

/// On a disk that can't clone (exFAT, as Otaq is told), the check runs on no seeded build.
#[tokio::test(flavor = "multi_thread")]
async fn on_a_disk_that_cant_clone_the_check_runs_without_a_seeded_build() {
    use otaq_core::warm::clone::{CloneProbe, CloneSupport};
    let w = TestService::start_with(|config| otaq_core::service::ServiceConfig {
        clone_probe: std::sync::Arc::new(CloneProbe::forced(CloneSupport {
            available: false,
            file_system: "exfat".into(),
            volume: "/Volumes/Test".into(),
        })),
        ..config
    })
    .await;
    sees_the_variables_and_leaves_nothing_behind(w).await;
}

async fn sees_the_variables_and_leaves_nothing_behind(w: TestService) {
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    commit(&repo, ".gitignore", "build/\n");
    let marks = tempfile::tempdir().unwrap();
    let seen = marks.path().join("seen");
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.env_vars = vec![EnvVar::new("MY_FLAG", "on")];
    let command = sh_one_line(
        marks.path(),
        "check",
        &format!(
            "(cat build/out.txt 2>/dev/null || echo 'no build') > '{seen}' && echo \"$MY_FLAG ${ticket}\" >> '{seen}' && echo made > made.txt",
            seen = seen.display(),
            ticket = naming::ticket_env_var(),
        ),
    );
    settings.merge_check = Some(command.clone());
    w.board
        .save_project_settings("SHOP", &settings)
        .await
        .unwrap();
    let (_, wt) = reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    std::fs::create_dir_all(wt.join("build")).unwrap();
    std::fs::write(wt.join("build/out.txt"), "built by SHOP-1\n").unwrap();
    commit(&repo, "other.ts", "main moved\n");
    let before = checkout_status(&repo);
    // What Otaq was told about the data folder's disk, and whether that disk really clones (the CI's ext4 doesn't).
    let clones = w.warm.support().available && otaq_core::warm::clone::probe(&w.data).available;

    let outcome = merge(&w, "SHOP-1").await;
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    let seeded = if clones {
        "built by SHOP-1"
    } else {
        "no build"
    };
    assert_eq!(
        std::fs::read_to_string(&seen).unwrap(),
        format!("{seeded}\non SHOP-1\n"),
        "the disk can clone: {clones}"
    );
    let scratch = w.data.scratch_path("shop-web").unwrap();
    assert_eq!(
        checkout_status(&scratch),
        "",
        "the scratch worktree is clean"
    );
    assert!(!scratch.join("build").exists() && !scratch.join("made.txt").exists());
    assert_eq!(
        checkout_status(&repo),
        before,
        "nothing new in the checkout"
    );
    assert!(!repo.join("build").exists());
    // The output is kept in Otaq's data folder.
    let log = w.data.merge_check_log_path("SHOP-1").unwrap();
    assert!(
        std::fs::read_to_string(log)
            .unwrap()
            .starts_with(&format!("$ {command}\n"))
    );
    w.stop().await;
}

/// The third failure in a row (conflicts and failed checks count together) stops turquoise: "Merge check keeps
/// failing on `main`". A branch that already took in main for a failed check is checked again on its next merge.
#[tokio::test(flavor = "multi_thread")]
async fn three_failures_in_a_row_stop_turquoise() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let marks = tempfile::tempdir().unwrap();
    set_check(
        &w,
        &sh_one_line(marks.path(), "check", "echo 'FAIL: 1 of 284'; exit 3"),
    )
    .await;
    let (t, _) = reviewed(&w, "Card", "card.ts", "card\n").await;
    w.agent_shell(&t).await;
    commit(&repo, "other.ts", "main moved\n");
    let main = main_of(&repo);

    for n in 1..=3 {
        if n > 1 {
            w.force_status(&t, Status::WaitingForYou).await;
        }
        let outcome = merge(&w, "SHOP-1").await;
        assert!(
            matches!(
                outcome,
                LocalMergeOutcome::CheckFailed { exit: Some(3), .. }
            ),
            "{n}: {outcome:?}"
        );
        let after = w.board.ticket("SHOP-1").await.unwrap();
        if n < 3 {
            assert_eq!(after.status, Status::MergeConflictFixing, "{n}");
        } else {
            assert_eq!(after.status, Status::DeliveryStopped);
            assert_eq!(
                after.auto_merge_stop,
                Some(AutoMergeStop::KeepsFailingMergeCheck)
            );
            assert_eq!(
                after.signal.unwrap().text.as_deref(),
                Some("Merge check keeps failing on `main`")
            );
        }
    }
    assert_eq!(main_of(&repo), main, "never merged");
    let comments = w.board.ticket_comments("SHOP-1").await.unwrap().comments;
    assert_eq!(comments.len(), 3, "one comment per failure, the stop's too");
    w.stop().await;
}

/// The user's Stop of a running check (Q475): nothing is merged, the ticket stays where it was, and its next merge
/// runs the check again although main is already merged into the branch.
#[tokio::test(flavor = "multi_thread")]
async fn a_stopped_check_merges_nothing_and_runs_again_next_time() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let marks = tempfile::tempdir().unwrap();
    let started = marks.path().join("started");
    let release = marks.path().join("release");
    // Runs until released; the second run passes at once.
    set_check(
        &w,
        &sh_one_line(
            marks.path(),
            "check",
            &format!(
                "if [ -e '{started}' ]; then exit 0; fi; touch '{started}'; while [ ! -e '{release}' ]; do sleep 0.05; done",
                started = started.display(),
                release = release.display()
            ),
        ),
    )
    .await;
    let (t, _) = reviewed(&w, "Card", "card.ts", "card\n").await;
    commit(&repo, "other.ts", "main moved\n");
    let main = main_of(&repo);
    assert!(
        w.review.stop_merge_check("SHOP-1").await.is_err(),
        "none runs yet"
    );

    let review = w.review.clone();
    let running = tokio::spawn(async move {
        review
            .local_merge("SHOP-1", "SHOP-1: merged", true, &me())
            .await
            .unwrap()
    });
    wait_until("the check started", || started.exists()).await;
    let shown = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(shown.flags.merge_check, Some(MergeCheckState::Running));
    assert_eq!(
        shown.flags.merge_check.unwrap().label(),
        "Merging \u{b7} running the merge check"
    );
    w.review.stop_merge_check("SHOP-1").await.unwrap();
    let outcome = running.await.unwrap();
    assert_eq!(
        outcome,
        LocalMergeOutcome::CheckStopped {
            target: "main".into()
        }
    );
    assert_eq!(main_of(&repo), main, "nothing merged");
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.status, Status::WaitingForYou, "where it was");
    assert_eq!(after.flags.merge_check, None);
    std::fs::write(&release, "").unwrap();

    // The branch has main's commits now, but no check passed on them: it runs again (and passes).
    let outcome = merge(&w, "SHOP-1").await;
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    let _ = t;
    w.stop().await;
}

/// Two merges into one branch at once: their checks run one after the other, the second on the first's result.
#[tokio::test(flavor = "multi_thread")]
async fn two_merges_at_once_check_one_after_the_other() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let marks = tempfile::tempdir().unwrap();
    let dir = marks.path().display().to_string();
    let ticket = naming::ticket_env_var();
    // Logs its start and end, notes an overlap, lists what it sees; SHOP-1's waits until released.
    let scripts = tempfile::tempdir().unwrap();
    set_check(
        &w,
        &sh_one_line(
            scripts.path(),
            "check",
            &format!(
            "echo \"${ticket} start\" >> '{dir}/log'; if [ -e '{dir}/running' ]; then echo overlap >> '{dir}/log'; fi; \
             touch '{dir}/running'; ls > '{dir}/'\"${ticket}\".ls; \
             if [ \"${ticket}\" = SHOP-1 ]; then while [ ! -e '{dir}/release' ]; do sleep 0.05; done; fi; \
             rm '{dir}/running'; echo \"${ticket} end\" >> '{dir}/log'"
            ),
        ),
    )
    .await;
    reviewed(&w, "Checkout API", "api.ts", "api\n").await;
    reviewed(&w, "Price table", "price.ts", "price\n").await;
    commit(&repo, "other.ts", "main moved\n");

    let first = {
        let review = w.review.clone();
        tokio::spawn(async move {
            review
                .local_merge("SHOP-1", "SHOP-1: merged", true, &me())
                .await
                .unwrap()
        })
    };
    let log = marks.path().join("log");
    wait_until("SHOP-1's check started", || log.exists()).await;
    let second = {
        let review = w.review.clone();
        tokio::spawn(async move {
            review
                .local_merge("SHOP-2", "SHOP-2: merged", true, &me())
                .await
                .unwrap()
        })
    };
    // SHOP-2's merge is under way and waits for SHOP-1's: give it the chance to start its check early.
    tokio::time::sleep(Duration::from_millis(300)).await;
    std::fs::write(marks.path().join("release"), "").unwrap();
    for merged in [first.await.unwrap(), second.await.unwrap()] {
        assert!(
            matches!(merged, LocalMergeOutcome::Merged { .. }),
            "{merged:?}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(&log).unwrap(),
        "SHOP-1 start\nSHOP-1 end\nSHOP-2 start\nSHOP-2 end\n"
    );
    let seen = std::fs::read_to_string(marks.path().join("SHOP-2.ls")).unwrap();
    assert!(
        seen.lines().any(|f| f == "api.ts"),
        "on SHOP-1's result: {seen}"
    );
    w.stop().await;
}

async fn wait_until(what: &str, mut check: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !check() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting: {what}"));
}
