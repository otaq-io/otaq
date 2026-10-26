//! The uninstall scan (T-0554, UNI-02; d0134, d0018, d0133, Q565) against temporary repositories and a temporary data
//! folder: every worktree under `wt/` with its ticket and ✎ (SHOP-12 in Review clean, SHOP-14 in Dev with an edited
//! and an untracked file, SHOP-14's helper, SHOP-2's kept worktree, an unowned worktree, a folder git doesn't know,
//! PAY-1 of a project whose repository was deleted), the merge worktree clean and then ✎, sizes without following
//! symlinks, what is running, the app's subscription, branches and trust; the scan changes no file; agents are
//! refused.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agent_tools::claude::trust::ClaudeConfig;
use otaq_core::agent_tools::{LaunchContext, Tool};
use otaq_core::caller::Caller;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::domain::Status;
use otaq_core::git::testing::{commit, git};
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;
use otaq_core::uninstall::{KeptTrust, Running, UninstallScan, WorktreeOwnerKind};
use serde_json::{Value, json};
use tempfile::TempDir;

/// Every file and folder under `dir` with its size and modification time (symlinks not followed).
fn tree(dir: &Path) -> Vec<(String, u64, Option<std::time::SystemTime>)> {
    let mut all = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let meta = entry.path().symlink_metadata().unwrap();
            if meta.is_dir() {
                stack.push(entry.path());
            }
            all.push((
                entry.path().display().to_string(),
                meta.len(),
                meta.modified().ok(),
            ));
        }
    }
    all.sort();
    all
}

/// The data folder's worktrees, warm builds and merge worktrees, and the repositories, `.git` included. The database
/// and logs are left out: the service's own background work writes them.
fn snapshot(
    w: &TestService,
    repos: &[&Path],
) -> Vec<Vec<(String, u64, Option<std::time::SystemTime>)>> {
    let mut all = vec![
        tree(&w.data.worktrees_dir()),
        tree(&w.data.warm_dir()),
        tree(&w.data.root().join("scratch")),
    ];
    all.extend(repos.iter().map(|r| tree(r)));
    all
}

/// One worktree as the test compares it: (folder under `wt/`, ticket, owner, ✎, known to git).
fn summary(scan: &UninstallScan) -> Vec<(String, Option<String>, WorktreeOwnerKind, bool, bool)> {
    scan.worktrees
        .iter()
        .map(|w| {
            (
                format!(
                    "{}/{}",
                    w.repo,
                    Path::new(&w.path).file_name().unwrap().to_string_lossy()
                ),
                w.ticket.as_ref().map(ToString::to_string),
                w.owner,
                w.dirty,
                w.registered,
            )
        })
        .collect()
}

/// Helpers and the Noyan stand-in are `sh` sleeping; never a real agent tool.
fn sh_launch(claude: ClaudeConfig) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            claude: ClaudeLaunch {
                program: vec![
                    "sh".into(),
                    "-c".into(),
                    "exec sleep 60".into(),
                    "sh".into(),
                ],
                config_dir: None,
            },
        }),
        claude: Some(claude),
        ..config
    }
}

async fn scan(w: &TestService) -> UninstallScan {
    w.uninstall.scan().await.unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_lists_every_worktree_with_its_ticket_and_changes_nothing() {
    let claude_dir = TempDir::new().unwrap();
    let claude = ClaudeConfig::locate(Some(claude_dir.path().into()), None).unwrap();
    let w = TestService::start_with(sh_launch(claude)).await;
    let shop = w.add_project("shop-web").await;
    let pay = w.add_project("pay-api").await;
    let repo = PathBuf::from(&shop.repo_path);
    let mut tickets = Vec::new();
    for n in 1..=14 {
        tickets.push(w.create("SHOP", &format!("ticket {n}")).await);
    }
    let (shop_2, shop_12, shop_14) = (&tickets[1], &tickets[11], &tickets[13]);

    // SHOP-12 in Review, all committed.
    let wt_12 = w.start_worktree(shop_12).await;
    commit(&wt_12, "done.rs", "fn done() {}\n");
    w.force_status(shop_12, Status::WaitingForYou).await;
    // SHOP-14 in Dev: an edited and an untracked file, its agent at work, and a helper on piece 1.
    let wt_14 = w.start_worktree(shop_14).await;
    commit(&wt_14, "app.rs", "fn app() {}\n");
    w.force_status(shop_14, Status::Working).await;
    let agent = w.agent_shell(shop_14).await;
    let caller = Caller::Agent {
        ticket: shop_14.key.clone(),
        ticket_id: shop_14.id.clone(),
        session: agent.session.clone(),
    };
    let helper = w
        .helpers
        .start(&caller, "SHOP-14", 1, "Wishlist page", None)
        .await
        .unwrap()
        .helper;
    std::fs::write(wt_14.join("app.rs"), "fn app() { todo!() }\n").unwrap();
    std::fs::write(wt_14.join("new.rs"), "fn new() {}\n").unwrap();
    // SHOP-2 Done with its worktree kept for an uncommitted file (REC-05).
    let wt_2 = w.start_worktree(shop_2).await;
    std::fs::write(wt_2.join("draft.ts"), "wip\n").unwrap();
    w.force_status(shop_2, Status::Merged).await;
    sqlx::query("UPDATE tickets SET worktree_kept = TRUE WHERE id = ?")
        .bind(&shop_2.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    // A worktree no ticket owns, and a folder git doesn't know.
    let orphan = w.data.worktrees_dir().join("shop-web").join("SHOP-99");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "otaq/SHOP-99",
            &orphan.display().to_string(),
        ],
    );
    let stray = w.data.worktrees_dir().join("shop-web").join("notes");
    std::fs::create_dir_all(&stray).unwrap();
    std::fs::write(stray.join("todo.txt"), "x\n").unwrap();
    // PAY-1's worktree, then its repository is deleted.
    let pay_1 = w.create(&pay.key, "Refunds").await;
    w.start_worktree(&pay_1).await;
    w.force_status(&pay_1, Status::Working).await;
    std::fs::remove_dir_all(&pay.repo_path).unwrap();
    // The merge worktree, clean.
    let scratch = w.data.scratch_path("shop-web").unwrap();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            &scratch.display().to_string(),
        ],
    );
    // A conversation with Noyan (a stand-in session).
    let _noyan = w.shell(shop_12, SessionKind::ProjectAgent).await;

    let before = snapshot(&w, &[&repo]);
    let s = scan(&w).await;
    assert_eq!(
        summary(&s),
        [
            (
                "pay-api/PAY-1".into(),
                Some("PAY-1".into()),
                WorktreeOwnerKind::Ticket,
                true,
                false
            ),
            (
                "shop-web/SHOP-12".into(),
                Some("SHOP-12".into()),
                WorktreeOwnerKind::Ticket,
                false,
                true
            ),
            (
                "shop-web/SHOP-14".into(),
                Some("SHOP-14".into()),
                WorktreeOwnerKind::Ticket,
                true,
                true
            ),
            (
                "shop-web/SHOP-14.1".into(),
                Some("SHOP-14".into()),
                WorktreeOwnerKind::Helper,
                false,
                true
            ),
            (
                "shop-web/SHOP-2".into(),
                Some("SHOP-2".into()),
                WorktreeOwnerKind::Kept,
                true,
                true
            ),
            (
                "shop-web/SHOP-99".into(),
                None,
                WorktreeOwnerKind::None,
                false,
                true
            ),
            (
                "shop-web/notes".into(),
                None,
                WorktreeOwnerKind::None,
                true,
                false
            ),
        ]
    );
    assert_eq!(
        PathBuf::from(&s.worktrees[3].path),
        PathBuf::from(&helper.worktree)
    );
    for wt in &s.worktrees[1..] {
        assert_eq!(wt.project.as_deref(), Some("SHOP"), "{wt:?}");
        assert_eq!(wt.repo_path.as_deref(), Some(shop.repo_path.as_str()));
        assert!(wt.size_bytes > 0);
    }
    assert_eq!(s.worktrees[0].project.as_deref(), Some(pay.key.as_str()));
    assert_eq!(s.merge_worktrees.len(), 1);
    let merge = &s.merge_worktrees[0];
    assert_eq!(
        (
            merge.repo.as_str(),
            merge.project.as_deref(),
            merge.registered,
            merge.dirty
        ),
        ("shop-web", Some("SHOP"), true, false)
    );
    assert_eq!(
        s.projects
            .iter()
            .map(|p| (p.key.as_str(), p.repo_exists))
            .collect::<Vec<_>>(),
        [("SHOP", true), (pay.key.as_str(), false)]
    );
    assert_eq!(s.tickets, 15);
    assert_eq!(
        s.running,
        Running {
            agents: 2,
            noyan_conversations: 1
        }
    );
    assert!(!s.app_open);
    // SHOP-2, 12, 14, 14.1 and 99; PAY's repository is gone, so it has no line.
    assert_eq!(
        s.branches
            .iter()
            .map(|b| (b.project.as_str(), b.repo.as_str(), b.count))
            .collect::<Vec<_>>(),
        [("SHOP", "shop-web", 5)]
    );
    assert_eq!(
        s.trust,
        [KeptTrust {
            tool: Tool::Claude,
            projects: vec![pay.key.clone(), "SHOP".into()]
        }]
    );
    assert_eq!(PathBuf::from(&s.data_folder), w.data.root());

    // The scan changed nothing, however often it runs.
    scan(&w).await;
    assert_eq!(snapshot(&w, &[&repo]), before);

    // A crash during a merge leaves the merge worktree ✎.
    std::fs::write(scratch.join("conflict.txt"), "<<<<<<<\n").unwrap();
    assert!(scan(&w).await.merge_worktrees[0].dirty);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn sizes_are_counted_without_following_symlinks() {
    let w = TestService::start().await;
    let outside = TempDir::new().unwrap();
    // 16 GB as its length says (sparse: nothing is written), so following the link would show at once.
    std::fs::File::create(outside.path().join("big.bin"))
        .unwrap()
        .set_len(1 << 34)
        .unwrap();
    let warm = w.data.warm_dir().join("shop-web");
    std::fs::create_dir_all(&warm).unwrap();
    std::fs::write(warm.join("lib.rlib"), vec![0u8; 1000]).unwrap();
    let link = warm.join("outside");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();

    let s = scan(&w).await;
    let link_size = link.symlink_metadata().unwrap().len();
    assert_eq!(s.warm_size_bytes, 1000 + link_size);
    assert!(s.data_size_bytes >= s.warm_size_bytes);
    assert!(s.data_size_bytes < 1 << 34, "{}", s.data_size_bytes);
}

/// Only the app's subscription counts as the app being open (Q565): a CLI's (`otaq attach`) doesn't.
#[tokio::test(flavor = "multi_thread")]
async fn app_open_is_the_apps_subscription_only() {
    let w = TestService::start().await;
    let cli = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let _events = cli.subscribe().await.unwrap();
    assert!(!scan(&w).await.app_open);

    let app = Client::connect(ClientConfig {
        app: true,
        ..ClientConfig::new(&w.data)
    })
    .await
    .unwrap();
    let _app_events = app.subscribe().await.unwrap();
    // Over the socket, as `otaq uninstall` asks.
    match cli.request(Request::UninstallScan).await.unwrap() {
        Reply::UninstallScan(s) => assert!(s.app_open),
        other => panic!("{other:?}"),
    }

    drop(_app_events);
    drop(app);
    tokio::time::timeout(Duration::from_secs(10), async {
        while scan(&w).await.app_open {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("the app's subscription ends with its connection");
}

/// The scan and the removal are the user's: an agent asking for either is refused, and nothing stops.
#[tokio::test(flavor = "multi_thread")]
async fn an_agent_is_refused() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let t = w.create("SHOP", "Agent at work").await;
    let agent = w.agent_shell(&t).await;
    for (method, params) in [
        ("uninstall.scan", Value::Null),
        ("uninstall.remove", json!({"worktrees": true})),
    ] {
        let mut request = json!({"protocol_version": 1, "id": 1, "request": {"method": method}});
        if !params.is_null() {
            request["request"]["params"] = params;
        }
        let ran = agent
            .send_raw(&w.data.socket_path(), &request.to_string())
            .await;
        let answer: Value = serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}: {ran:?}"));
        assert_eq!(answer["error"]["kind"], "refused", "{answer}");
        assert_eq!(
            answer["error"]["data"]["refusal"], "not_allowed",
            "{answer}"
        );
        assert!(
            answer["error"]["message"]
                .as_str()
                .unwrap()
                .contains("can't run the uninstall"),
            "{answer}"
        );
    }
    // Nothing was stopped.
    assert!(w.sessions.info(&agent.session).unwrap().is_running());
    assert!(!w.sessions.is_closed());
}
