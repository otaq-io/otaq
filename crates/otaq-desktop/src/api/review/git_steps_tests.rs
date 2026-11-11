//! The real step and the running hook while Approve & merge runs: shop-web (SHOP) with a pre-commit hook that runs
//! until the test releases it, SHOP-1 in Review. While the hook runs, the ticket, its card on the board and the
//! Merging dialog's view say the step git is on (the squash, not the first step) and "running your pre-commit hook"
//! with a time that grows, whether the dialog is open or not; once the merge has landed nothing says it any more.
//!
//! The app talks to a core `TestService` (temporary `OTAQ_HOME` and repositories); the hook is a plain `sh` script in
//! the temporary repository's `.git/hooks`. No agent tool runs.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::domain::{GitOperation, Status};
use otaq_core::git::GitStep;
use otaq_core::git::testing::{commit, git};
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

use crate::App;
use crate::api::request;
use crate::service::testing::{app_on, connected};

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

async fn until(what: &str, done: impl Fn() -> bool) {
    tokio::time::timeout(Duration::from_secs(20), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("{what} never happened"));
}

/// A pre-commit hook that touches `started`, then runs until `release` exists.
fn slow_pre_commit(repo: &Path, started: &Path, release: &Path) {
    let hook = repo.join(".git/hooks/pre-commit");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\ntouch '{}'\nwhile [ ! -e '{}' ]; do sleep 0.05; done\n",
            started.display(),
            release.display()
        ),
    )
    .unwrap();
    // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// An answer within 5 s: the shared connection isn't waiting behind the merge.
async fn quick(app: &App, method: &str, params: Value) -> Value {
    tokio::time::timeout(Duration::from_secs(5), request(app, method, params))
        .await
        .unwrap_or_else(|_| panic!("{method} waited for the merge"))
        .unwrap()
}

/// The board card of `key`, wherever it is in the view.
fn card<'a>(view: &'a Value, key: &str) -> Option<&'a Value> {
    match view {
        Value::Object(o) if o.get("key") == Some(&json!(key)) && o.contains_key("status") => {
            Some(view)
        }
        Value::Object(o) => o.values().find_map(|v| card(v, key)),
        Value::Array(a) => a.iter().find_map(|v| card(v, key)),
        _ => None,
    }
}

/// The seconds of "running your pre-commit hook · 3 s".
fn seconds(line: &str) -> u64 {
    line.rsplit(" \u{b7} ")
        .next()
        .and_then(|t| t.strip_suffix(" s"))
        .and_then(|n| n.parse().ok())
        .unwrap_or_else(|| panic!("no seconds in {line:?}"))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_hook_shows_on_the_card_and_in_the_merging_dialog() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let t = w.create("SHOP", "Card").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "card.ts", "card\n");
    w.force_status(&t, Status::WaitingForYou).await;
    commit(&repo, "other.ts", "main moved\n");
    let marks = tempfile::tempdir().unwrap();
    let started = marks.path().join("started");
    let release = marks.path().join("release");
    slow_pre_commit(&repo, &started, &release);
    let app = app(&w).await;
    let board = json!({ "project": "SHOP", "width": 1376.0, "height": 856.0 });

    let approve = quick(&app, "review.approve", json!({ "key": "SHOP-1" })).await;
    assert_eq!(
        approve["merging"]["steps"],
        json!([
            "Fetch main and bring it up to date",
            "Merge main into the branch",
            "Squash into main",
            "Move main to the new commit",
            "Start waiting tickets whose dependencies are all merged",
        ])
    );
    assert_eq!(
        (&approve["merging"]["current"], &approve["merging"]["hook"]),
        (&json!(0), &Value::Null)
    );

    let merge = request(
        &app,
        "review.local_merge",
        json!({ "key": "SHOP-1", "message": "SHOP-1: Card" }),
    );
    let watch = async {
        until("the pre-commit hook started", || started.exists()).await;
        // git's events are read a few times a second: the hook shows within a moment.
        let mut shown = w.board.ticket("SHOP-1").await.unwrap();
        for _ in 0..100 {
            if shown.flags.git.as_ref().is_some_and(|g| g.hook.is_some()) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            shown = w.board.ticket("SHOP-1").await.unwrap();
        }
        let progress = shown.flags.git.clone().expect("the merge shows on the ticket");
        assert_eq!(progress.operation, GitOperation::Merge);
        assert_eq!(progress.step, Some(GitStep::Commit));
        assert!(progress.earlier.contains(&GitStep::UpdateBranch), "{progress:?}");
        assert_eq!(progress.hook.as_ref().unwrap().name, "pre-commit");

        // The card, with the dialog closed: the board's own view.
        let view = quick(&app, "board.view", board.clone()).await;
        assert_eq!(view["seconds"], true, "the board asks again every second");
        let line = card(&view, "SHOP-1").unwrap()["status"]["text"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(
            line.starts_with("Merging \u{b7} running your pre-commit hook \u{b7} "),
            "{line}"
        );
        assert_eq!(card(&view, "SHOP-1").unwrap()["action"], Value::Null);

        // The dialog: the squash step, the hook under it, its time growing.
        let open = quick(&app, "review.approve", json!({ "key": "SHOP-1" })).await;
        assert_eq!(open["merging"]["current"], 2, "{}", open["merging"]);
        let first = open["merging"]["hook"].as_str().unwrap().to_owned();
        assert!(
            first.starts_with("running your pre-commit hook \u{b7} "),
            "{first}"
        );
        tokio::time::sleep(Duration::from_millis(2_100)).await;
        let later = quick(&app, "review.approve", json!({ "key": "SHOP-1" })).await;
        let second = later["merging"]["hook"].as_str().unwrap().to_owned();
        assert!(seconds(&second) >= seconds(&first) + 2, "{first} → {second}");
        let view = quick(&app, "board.view", board.clone()).await;
        let grown = card(&view, "SHOP-1").unwrap()["status"]["text"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(seconds(&grown) >= seconds(&line) + 2, "{line} → {grown}");

        std::fs::write(&release, "").unwrap();
    };
    let (merged, ()) = tokio::time::timeout(Duration::from_secs(60), async {
        tokio::join!(merge, watch)
    })
    .await
    .expect("the merge ends once the hook does");
    assert_eq!(merged.unwrap()["kind"], "merged");
    assert_eq!(
        git(&repo, &["log", "-1", "--format=%s", "main"]),
        "SHOP-1: Card"
    );
    let after = w.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(after.flags.git, None, "nothing says it once it landed");
    let view = quick(&app, "board.view", board).await;
    assert_eq!(view["seconds"], false);
}

/// A post-merge hook that, only in the user's own folder (main checked out), touches `started`, runs until `release`
/// exists, then prints a line and ends with `exit`.
fn slow_post_merge(repo: &Path, started: &Path, release: &Path, exit: i32) {
    let hook = repo.join(".git/hooks/post-merge");
    std::fs::write(
        &hook,
        format!(
            "#!/bin/sh\n[ \"$(git rev-parse --abbrev-ref HEAD)\" = main ] || exit 0\ntouch '{}'\nwhile [ ! -e '{}' ]; do sleep 0.05; done\necho 'added 3 packages'\nexit {exit}\n",
            started.display(),
            release.display()
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

/// The user's post-merge hook runs in their own folder as main moves there: the card and the Merging dialog say so,
/// naming the folder, on the last git step; once it ends History keeps what it printed. Passed or failed, the ticket
/// is merged.
#[tokio::test(flavor = "multi_thread")]
async fn the_post_merge_hook_in_the_users_folder_shows_and_its_output_stays_in_history() {
    for exit in [0, 1] {
        let w = TestService::start().await;
        let project = w.add_project("shop-web").await;
        let repo = PathBuf::from(&project.repo_path);
        let t = w.create("SHOP", "Card").await;
        let wt = w.start_worktree(&t).await;
        commit(&wt, "card.ts", "card\n");
        w.force_status(&t, Status::WaitingForYou).await;
        let marks = tempfile::tempdir().unwrap();
        let started = marks.path().join("started");
        let release = marks.path().join("release");
        slow_post_merge(&repo, &started, &release, exit);
        let app = app(&w).await;
        let board = json!({ "project": "SHOP", "width": 1376.0, "height": 856.0 });

        let merge = request(
            &app,
            "review.local_merge",
            json!({ "key": "SHOP-1", "message": "SHOP-1: Card" }),
        );
        let watch = async {
            until("the post-merge hook started", || started.exists()).await;
            let mut shown = w.board.ticket("SHOP-1").await.unwrap();
            for _ in 0..100 {
                if shown.flags.git.as_ref().is_some_and(|g| g.hook.is_some()) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
                shown = w.board.ticket("SHOP-1").await.unwrap();
            }
            let progress = shown.flags.git.clone().expect("the merge shows on the ticket");
            assert_eq!(progress.step, Some(GitStep::MoveTarget));
            let hook = progress.hook.as_ref().unwrap();
            assert_eq!(hook.name, "post-merge");
            assert!(hook.folder.ends_with("shop-web"), "{}", hook.folder);
            // main has moved already: the merge landed before the hook runs.
            assert_eq!(
                git(&repo, &["log", "-1", "--format=%s", "main"]),
                "SHOP-1: Card"
            );

            let view = quick(&app, "board.view", board.clone()).await;
            let line = card(&view, "SHOP-1").unwrap()["status"]["text"]
                .as_str()
                .unwrap()
                .to_owned();
            assert!(
                line.starts_with(&format!(
                    "Merging \u{b7} running your post-merge hook in {} \u{b7} ",
                    hook.folder
                )),
                "{line}"
            );

            let open = quick(&app, "review.approve", json!({ "key": "SHOP-1" })).await;
            let steps = open["merging"]["steps"].as_array().unwrap();
            let current = open["merging"]["current"].as_u64().unwrap() as usize;
            assert_eq!(steps[current], "Move main to the new commit", "{}", open["merging"]);
            let first = open["merging"]["hook"].as_str().unwrap().to_owned();
            assert!(
                first.starts_with(&format!("running your post-merge hook in {} \u{b7} ", hook.folder)),
                "{first}"
            );
            tokio::time::sleep(Duration::from_millis(2_100)).await;
            let later = quick(&app, "review.approve", json!({ "key": "SHOP-1" })).await;
            let second = later["merging"]["hook"].as_str().unwrap().to_owned();
            assert!(seconds(&second) >= seconds(&first) + 2, "{first} → {second}");

            std::fs::write(&release, "").unwrap();
            hook.folder.clone()
        };
        let (merged, folder) = tokio::time::timeout(Duration::from_secs(60), async {
            tokio::join!(merge, watch)
        })
        .await
        .expect("the merge ends once the hook does");
        assert_eq!(merged.unwrap()["kind"], "merged");
        let after = w.board.ticket("SHOP-1").await.unwrap();
        assert_eq!(after.status, Status::Merged, "a failing hook changes nothing");
        assert_eq!(after.flags.git, None);

        let history = quick(&app, "sections.history", json!({ "key": "SHOP-1" })).await;
        let rows = history["rows"].as_array().unwrap();
        let said = match exit {
            0 => format!("Your post-merge hook ran in {folder}"),
            _ => format!("Your post-merge hook failed in {folder} (exit 1)"),
        };
        let row = rows
            .iter()
            .find(|r| r["title"] == json!(said))
            .unwrap_or_else(|| panic!("no {said:?} in {history}"));
        assert_eq!(row["detail"], "added 3 packages");
        assert!(rows.iter().any(|r| r["title"] == "Merged"));
        drop(app);
        w.stop().await;
    }
}
