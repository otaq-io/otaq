//! The warm build store (T-0296; WRM-02, WRM-05…WRM-07, d0082) against a running service in a temporary data folder:
//! the switch, replace (first, forward, an older one dropped), Clear waiting for a copy, the switch turned off,
//! `warm.list`, and the temporary folders a crash left. Builds are fake: files the test writes. The probe is forced,
//! so nothing here depends on the test disk being able to clone (the store only renames).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use otaq_core::client::{Client, ClientConfig};
use otaq_core::git::testing::{TempRepos, commit, git};
use otaq_core::protocol::{
    Event, ProjectParams, ProjectSettingsParams, Reply, Request, WarmClearParams,
};
use otaq_core::service::testing::TestService;
use otaq_core::store::projects::Project;
use otaq_core::ticket_env::handoff::HandoffMode;
use otaq_core::ticket_env::stock::Added;
use otaq_core::warm::clone::{CloneProbe, CloneSupport};
use otaq_core::warm::store::{
    ClearOutcome, WarmBuildInfo, WarmEntryInfo, WarmList, WarmRecord, record_file_name,
    stock_marker_name,
};
use tokio::sync::broadcast;

fn support(available: bool) -> CloneSupport {
    CloneSupport {
        available,
        file_system: if available { "apfs" } else { "exfat" }.into(),
        volume: "/Volumes/Test".into(),
    }
}

async fn world(available: bool) -> TestService {
    TestService::start_with(|config| otaq_core::service::ServiceConfig {
        clone_probe: Arc::new(CloneProbe::forced(support(available))),
        ..config
    })
    .await
}

fn head(repo: &Path) -> String {
    git(repo, &["rev-parse", "HEAD"]).trim().to_owned()
}

fn record(base: &str, from: &str) -> WarmRecord {
    WarmRecord {
        built_at: base.to_owned(),
        base: base.to_owned(),
        from: from.to_owned(),
        kept_at: 1_760_000_000_000,
        project: String::new(),
    }
}

/// Keeps a fake build (`out/app.bin` holding `content`) for `project` as `from` with this base.
async fn keep(w: &TestService, project: &Project, base: &str, from: &str, content: &str) -> Added {
    let incoming = w.warm.incoming(project).await.unwrap();
    std::fs::create_dir_all(incoming.path().join("out")).unwrap();
    std::fs::write(incoming.path().join("out").join("app.bin"), content).unwrap();
    w.warm
        .replace(project, incoming, record(base, from))
        .await
        .unwrap()
}

/// The project's single stock entry (on a disk that can clone the stock keeps one); a path that isn't there when
/// it has none.
fn entry(w: &TestService, project: &Project) -> PathBuf {
    let parent = w.warm.path(project).unwrap();
    let mut numbers: Vec<u64> = std::fs::read_dir(&parent)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_string_lossy().parse().ok())
        .collect();
    numbers.sort();
    match numbers.as_slice() {
        [] => parent.join("none"),
        [n] => parent.join(n.to_string()),
        more => panic!("one entry expected, found {more:?}"),
    }
}

/// Whether `warm/<repo>.trash` holds nothing (or isn't there).
fn trash_empty(w: &TestService, repo: &str) -> bool {
    std::fs::read_dir(w.data.warm_trash_path(repo).unwrap())
        .map(|mut d| d.next().is_none())
        .unwrap_or(true)
}

fn built(w: &TestService, project: &Project) -> Option<String> {
    std::fs::read_to_string(entry(w, project).join("out").join("app.bin")).ok()
}

async fn client(w: &TestService) -> Client {
    Client::connect(ClientConfig::new(&w.data)).await.unwrap()
}

async fn list(client: &Client) -> WarmList {
    match client.request(Request::WarmList).await.unwrap() {
        Reply::WarmList(list) => *list,
        other => panic!("{other:?}"),
    }
}

async fn warm_event(events: &mut broadcast::Receiver<Event>) -> Event {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .expect("an event in time")
            .unwrap();
        if matches!(event, Event::WarmBuildChanged { .. }) {
            return event;
        }
    }
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..200 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("{what}");
}

fn changed(project: &str) -> Event {
    Event::WarmBuildChanged {
        repo: "shop-web".into(),
        project: Some(project.into()),
    }
}

#[tokio::test]
async fn warm_builds_are_on_by_default_on_any_disk() {
    let w = world(true).await;
    let shop = w.add_project("shop-web").await;
    assert!(w.warm.enabled("SHOP").await.unwrap());
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    assert!(settings.warm_builds);
    settings.warm_builds = false;
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    assert!(!w.warm.enabled("SHOP").await.unwrap());
    assert_eq!(
        w.warm.path(&shop).unwrap(),
        w.data.root().join("warm").join("shop-web")
    );
    w.stop().await;

    // A disk that can't clone (M19-b): the switch decides there too; builds are renamed, and `warm.list` says so.
    let w = world(false).await;
    w.add_project("shop-web").await;
    assert!(w.board.project_settings("SHOP").await.unwrap().warm_builds);
    assert!(w.warm.enabled("SHOP").await.unwrap());
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.warm_builds = false;
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    assert!(!w.warm.enabled("SHOP").await.unwrap());
    let c = client(&w).await;
    let listed = list(&c).await;
    assert_eq!(listed.support, support(false));
    assert_eq!(listed.mode, HandoffMode::Move);
    assert!(listed.builds.is_empty());
    w.stop().await;
}

#[tokio::test]
async fn replace_keeps_the_first_and_newer_builds_and_drops_older_ones() {
    let w = world(true).await;
    let shop = w.add_project("shop-web").await;
    let repo = Path::new(&shop.repo_path);
    let first = head(repo);
    commit(repo, "a.txt", "a");
    let second = head(repo);
    commit(repo, "b.txt", "b");
    let third = head(repo);
    let mut events = w.events.subscribe();

    // The first build is kept, with its record.
    assert_eq!(
        keep(&w, &shop, &second, "SHOP-21", "second").await,
        Added::Kept { entry: 0 }
    );
    assert_eq!(warm_event(&mut events).await, changed("SHOP"));
    assert_eq!(built(&w, &shop).as_deref(), Some("second"));
    let kept = w.warm.record(&shop).await.unwrap();
    assert_eq!(
        (kept.base.as_str(), kept.from.as_str()),
        (second.as_str(), "SHOP-21")
    );
    assert_eq!(kept.project, shop.id);

    // An older one never replaces it; the dropped files are deleted.
    assert_eq!(
        keep(&w, &shop, &first, "SHOP-19", "first").await,
        Added::Dropped
    );
    assert_eq!(built(&w, &shop).as_deref(), Some("second"));
    let incoming = w.data.warm_dir().join("shop-web.incoming");
    until("the dropped build deleted", || !incoming.exists()).await;

    // The same base, then a newer one, replace it; the old folder goes in the background.
    assert_eq!(
        keep(&w, &shop, &second, "SHOP-22", "again").await,
        Added::Kept { entry: 1 }
    );
    assert_eq!(
        keep(&w, &shop, &third, "SHOP-23", "third").await,
        Added::Kept { entry: 2 }
    );
    assert_eq!(built(&w, &shop).as_deref(), Some("third"));
    assert_eq!(w.warm.record(&shop).await.unwrap().from, "SHOP-23");
    // One entry, numbered after the ones before it; the old ones went to the trash, emptied in the background.
    assert_eq!(
        entry(&w, &shop),
        w.data.warm_entry_path("shop-web", 2).unwrap()
    );
    until("the old build deleted", || trash_empty(&w, "shop-web")).await;
    assert!(!w.data.warm_dir().join("shop-web.old").exists());

    // Nothing is kept once warm builds are off.
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.warm_builds = false;
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    commit(repo, "c.txt", "c");
    let fourth = head(repo);
    assert_eq!(
        keep(&w, &shop, &fourth, "SHOP-24", "fourth").await,
        Added::Dropped
    );
    assert_eq!(built(&w, &shop).as_deref(), Some("third"));
    w.stop().await;
}

#[tokio::test]
async fn clear_waits_for_a_running_copy_then_clears() {
    let w = world(true).await;
    let shop = w.add_project("shop-web").await;
    let base = head(Path::new(&shop.repo_path));
    keep(&w, &shop, &base, "SHOP-20", "built").await;
    let c = client(&w).await;
    let mut events = w.events.subscribe();

    // SHOP-21's new worktree is being seeded from it.
    let seed = w
        .warm
        .read(&shop, "SHOP-21")
        .await
        .expect("a warm build to seed from");
    assert_eq!(seed.record().unwrap().from, "SHOP-20");
    assert_eq!(seed.path(), w.data.warm_entry_path("shop-web", 0).unwrap());
    assert_eq!(seed.entry(), 0);

    let clear = Request::WarmClear(WarmClearParams {
        project: Some("SHOP".into()),
        repo: None,
    });
    let Reply::WarmClear(outcome) = c.request(clear.clone()).await.unwrap() else {
        panic!()
    };
    assert_eq!(
        outcome,
        ClearOutcome::Waiting {
            copying: vec!["SHOP-21".into()]
        }
    );
    assert_eq!(warm_event(&mut events).await, changed("SHOP"));
    // "Clearing… (waiting for SHOP-21's copy)": the copy goes on, nothing is cleared yet.
    let listed = list(&c).await;
    let [build]: [WarmBuildInfo; 1] = listed.builds.try_into().unwrap();
    assert_eq!(build.project.as_deref(), Some("SHOP"));
    assert_eq!(build.status.copying, ["SHOP-21"]);
    assert!(build.status.clearing);
    assert!(built(&w, &shop).is_some());
    // A second Clear joins the waiting one.
    let Reply::WarmClear(again) = c.request(clear).await.unwrap() else {
        panic!()
    };
    assert!(matches!(again, ClearOutcome::Waiting { .. }));

    // A new seed waits behind the Clear, then finds nothing to seed from.
    let next = {
        let (warm, shop) = (w.warm.clone(), shop.clone());
        tokio::spawn(async move { warm.read(&shop, "SHOP-22").await.is_some() })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!next.is_finished());

    // The copy ends: the Clear goes on.
    drop(seed);
    assert_eq!(warm_event(&mut events).await, changed("SHOP"));
    assert!(built(&w, &shop).is_none());
    assert!(!next.await.unwrap());
    assert!(!w.warm.path(&shop).unwrap().exists());
    until("the cleared build deleted", || trash_empty(&w, "shop-web")).await;
    assert!(list(&c).await.builds.is_empty());
    // Clearing never turns the switch off.
    assert!(w.board.project_settings("SHOP").await.unwrap().warm_builds);
    // Nothing left to clear.
    let Reply::WarmClear(outcome) = c
        .request(Request::WarmClear(WarmClearParams {
            project: Some("SHOP".into()),
            repo: None,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(outcome, ClearOutcome::Nothing);
    w.stop().await;
}

#[tokio::test]
async fn turning_the_switch_off_clears_the_warm_build() {
    let w = world(true).await;
    let shop = w.add_project("shop-web").await;
    let base = head(Path::new(&shop.repo_path));
    keep(&w, &shop, &base, "SHOP-20", "built").await;
    let c = client(&w).await;
    let mut events = w.events.subscribe();
    let Reply::ProjectSettings(reply) = c
        .request(Request::ProjectSettingsGet(ProjectParams {
            project: "SHOP".into(),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    let mut settings = reply.settings;
    settings.warm_builds = false;
    c.request(Request::ProjectSettingsSet(ProjectSettingsParams {
        project: "SHOP".into(),
        settings: Box::new(settings),
    }))
    .await
    .unwrap();
    assert_eq!(warm_event(&mut events).await, changed("SHOP"));
    assert!(built(&w, &shop).is_none());
    assert!(!w.board.project_settings("SHOP").await.unwrap().warm_builds);
    w.stop().await;
}

#[tokio::test]
async fn warm_list_shows_each_build_with_or_without_a_record() {
    let w = world(true).await;
    let shop = w.add_project("shop-web").await;
    let base = head(Path::new(&shop.repo_path));
    keep(&w, &shop, &base, "SHOP-21", "12345").await;
    // A folder no project owns, without a record, and the store's own folders, which never list.
    let warm = w.data.warm_dir();
    let old_entry = w.data.warm_entry_path("old-app", 0).unwrap();
    std::fs::create_dir_all(old_entry.join("deps")).unwrap();
    std::fs::write(old_entry.join("deps").join("lib"), "abc").unwrap();
    std::fs::create_dir_all(warm.join("shop-web.old")).unwrap();
    std::fs::create_dir_all(warm.join("shop-web.trash").join("1-1")).unwrap();
    // A folder without entries isn't a warm build.
    std::fs::create_dir_all(warm.join("empty").join("deps")).unwrap();
    std::fs::create_dir_all(warm.join(".probe")).unwrap();

    let c = client(&w).await;
    let listed = list(&c).await;
    assert_eq!(listed.support, support(true));
    let [old_app, shop_web]: [WarmBuildInfo; 2] = listed.builds.try_into().unwrap();
    assert_eq!(old_app.repo, "old-app");
    assert_eq!(old_app.project, None);
    assert_eq!(old_app.size_bytes, 3);
    assert_eq!(
        (old_app.built_at, old_app.from, old_app.kept_at),
        (None, None, None)
    );
    assert_eq!(old_app.entries.len(), 1);
    assert_eq!(old_app.entries[0].entry, 0);
    assert_eq!(old_app.entries[0].size_bytes, 3);
    assert_eq!(old_app.entries[0].path, old_entry.display().to_string());
    assert_eq!(listed.mode, HandoffMode::Clone);

    assert_eq!(shop_web.repo, "shop-web");
    assert_eq!(shop_web.project.as_deref(), Some("SHOP"));
    assert_eq!(shop_web.built_at.as_deref(), Some(&base[..7]));
    assert_eq!(shop_web.from.as_deref(), Some("SHOP-21"));
    assert_eq!(shop_web.kept_at, Some(1_760_000_000_000));
    let record_size = std::fs::metadata(entry(&w, &shop).join(record_file_name()))
        .unwrap()
        .len();
    assert_eq!(shop_web.size_bytes, 5 + record_size);
    assert!(shop_web.status.copying.is_empty() && !shop_web.status.clearing);
    let [only]: [WarmEntryInfo; 1] = shop_web.entries.clone().try_into().unwrap();
    assert_eq!(only.size_bytes, shop_web.size_bytes);
    assert_eq!(only.built_at, shop_web.built_at);
    assert_eq!(
        (only.from.as_deref(), only.taken_by),
        (Some("SHOP-21"), None)
    );

    // A folder no project owns is cleared by its name.
    let Reply::WarmClear(outcome) = c
        .request(Request::WarmClear(WarmClearParams {
            project: None,
            repo: Some("old-app".into()),
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(outcome, ClearOutcome::Cleared);
    assert!(!warm.join("old-app").exists());
    w.stop().await;
}

#[tokio::test]
async fn folders_a_crash_left_are_removed_at_start() {
    let home = tempfile::TempDir::new().unwrap();
    let warm = home.path().join("h").join("warm");
    for folder in ["shop-web/0", "shop-web.incoming", "shop-web.old"] {
        std::fs::create_dir_all(warm.join(folder).join("out")).unwrap();
        std::fs::write(warm.join(folder).join("out").join("app.bin"), "x").unwrap();
    }
    std::fs::write(warm.join("shop-web").join(stock_marker_name()), "stock 2").unwrap();
    let w = TestService::start_in(home, TempRepos::new(), |config| {
        otaq_core::service::ServiceConfig {
            clone_probe: Arc::new(CloneProbe::forced(support(true))),
            ..config
        }
    })
    .await;
    assert!(
        warm.join("shop-web")
            .join("0")
            .join("out")
            .join("app.bin")
            .is_file()
    );
    assert!(!warm.join("shop-web.incoming").exists());
    assert!(!warm.join("shop-web.old").exists());
    w.stop().await;
}

/// Starts a service on a data folder whose `warm/` the test filled first.
async fn world_in(home: tempfile::TempDir) -> TestService {
    TestService::start_in(home, TempRepos::new(), |config| {
        otaq_core::service::ServiceConfig {
            clone_probe: Arc::new(CloneProbe::forced(support(true))),
            ..config
        }
    })
    .await
}

#[tokio::test]
async fn old_warm_builds_are_thrown_away_at_start() {
    let home = tempfile::TempDir::new().unwrap();
    let warm = home.path().join("h").join("warm");
    // A build from before the stock, and a stock of the first format.
    let legacy = warm.join("shop-web");
    std::fs::create_dir_all(legacy.join("out")).unwrap();
    std::fs::write(legacy.join("out").join("app.bin"), "legacy").unwrap();
    record("3e1f9a2c0d", "SHOP-9").write(&legacy).unwrap();
    let old = warm.join("old-app");
    std::fs::create_dir_all(old.join("0").join("out")).unwrap();
    std::fs::write(old.join(stock_marker_name()), "stock 1").unwrap();

    let w = world_in(home).await;
    until("the old builds thrown away and their trash emptied", || {
        !legacy.exists()
            && !old.exists()
            && trash_empty(&w, "shop-web")
            && trash_empty(&w, "old-app")
    })
    .await;
    let shop = w.add_project("shop-web").await;
    assert!(
        w.warm.read(&shop, "SHOP-30").await.is_none(),
        "no build left to seed from"
    );
    // The next build kept makes a stock of today's format, which the next start leaves.
    let base = head(Path::new(&shop.repo_path));
    assert_eq!(
        keep(&w, &shop, &base, "SHOP-31", "new").await,
        Added::Kept { entry: 0 }
    );
    assert_eq!(
        std::fs::read_to_string(legacy.join(stock_marker_name())).unwrap(),
        "stock 2"
    );
    let found = otaq_core::warm::store::remove_interrupted(&w.data);
    assert_eq!(found.thrown_away, 0);
    assert_eq!(built(&w, &shop).as_deref(), Some("new"));
    w.stop().await;
}

#[cfg(unix)]
#[test]
fn a_legacy_build_that_cant_be_thrown_away_is_left_alone() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let data = otaq_core::data_dir::DataDir::new(tmp.path());
    let warm = data.warm_dir();
    let legacy = warm.join("shop-web");
    std::fs::create_dir_all(legacy.join("out")).unwrap();
    std::fs::write(legacy.join("out").join("app.bin"), "legacy").unwrap();
    // A read-only `warm/`: nothing in it can be renamed.
    std::fs::set_permissions(&warm, std::fs::Permissions::from_mode(0o555)).unwrap();
    if std::fs::write(warm.join("probe"), "").is_ok() {
        std::fs::set_permissions(&warm, std::fs::Permissions::from_mode(0o755)).unwrap();
        eprintln!("skipped: permissions don't hold here, running as root");
        return;
    }
    let found = otaq_core::warm::store::remove_interrupted(&data);
    std::fs::set_permissions(&warm, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(found.thrown_away, 0);
    assert_eq!(
        std::fs::read_to_string(legacy.join("out").join("app.bin")).unwrap(),
        "legacy"
    );
    assert!(!legacy.join(stock_marker_name()).exists());
}

#[tokio::test]
async fn trash_is_emptied_in_the_background_and_at_start() {
    let home = tempfile::TempDir::new().unwrap();
    let warm = home.path().join("h").join("warm");
    let left = warm.join("shop-web.trash").join("1760000000000-1");
    std::fs::create_dir_all(left.join("out")).unwrap();
    std::fs::write(left.join("out").join("app.bin"), "trashed").unwrap();
    let w = world_in(home).await;
    until("the trash a crash left emptied", || {
        trash_empty(&w, "shop-web")
    })
    .await;

    // At work: every older build replaced goes the same way.
    let shop = w.add_project("shop-web").await;
    let repo = Path::new(&shop.repo_path);
    keep(&w, &shop, &head(repo), "SHOP-21", "first").await;
    commit(repo, "a.txt", "a");
    keep(&w, &shop, &head(repo), "SHOP-22", "second").await;
    until("the replaced build deleted", || trash_empty(&w, "shop-web")).await;
    assert_eq!(built(&w, &shop).as_deref(), Some("second"));
    w.stop().await;
}

#[tokio::test]
async fn half_made_entries_are_removed_at_start() {
    let home = tempfile::TempDir::new().unwrap();
    let stock = home.path().join("h").join("warm").join("shop-web");
    std::fs::create_dir_all(stock.join("0").join("out")).unwrap();
    std::fs::write(stock.join("0").join("out").join("app.bin"), "kept").unwrap();
    std::fs::write(stock.join(stock_marker_name()), "stock 2").unwrap();
    std::fs::create_dir_all(stock.join("1.part").join("out")).unwrap();
    std::fs::write(stock.join("loose.bin"), "x").unwrap();

    let w = world_in(home).await;
    let mut left: Vec<String> = std::fs::read_dir(&stock)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    let mut expected = vec![stock_marker_name(), "0".to_owned()];
    expected.sort();
    assert_eq!(left, expected);
    assert!(stock.join("0").join("out").join("app.bin").is_file());
    w.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_claims_on_one_entry_get_one_winner() {
    let w = world(true).await;
    let shop = w.add_project("shop-web").await;
    let base = head(Path::new(&shop.repo_path));
    keep(&w, &shop, &base, "SHOP-20", "built").await;
    let [entry]: [_; 1] = w.warm.entries(&shop).await.try_into().unwrap();
    assert_eq!(entry.n, 0);

    // Many tickets at once: one gets it.
    let tasks: Vec<_> = (0..8)
        .map(|i| {
            let warm = w.warm.clone();
            tokio::spawn(async move { warm.claim("shop-web", 0, &format!("SHOP-{}", 30 + i)) })
        })
        .collect();
    let mut winners = 0;
    for task in tasks {
        winners += usize::from(task.await.unwrap());
    }
    assert_eq!(winners, 1);
    assert_eq!(w.warm.status("shop-web").copying.len(), 1);
    w.warm.unclaim("shop-web", 0);
    assert!(w.warm.status("shop-web").copying.is_empty());

    // A held claim: the second is refused until the first is dropped.
    let first = w
        .warm
        .claim_entry("shop-web", entry.clone(), "SHOP-21")
        .expect("free");
    assert_eq!((first.entry(), first.path()), (0, entry.path.as_path()));
    assert_eq!(first.record().unwrap().from, "SHOP-20");
    assert!(
        w.warm
            .claim_entry("shop-web", entry.clone(), "SHOP-22")
            .is_none()
    );
    drop(first);
    assert!(
        w.warm
            .claim_entry("shop-web", entry.clone(), "SHOP-22")
            .is_some()
    );

    // An entry read before a replace moved it to the trash can't be claimed after.
    let repo = Path::new(&shop.repo_path);
    commit(repo, "a.txt", "a");
    keep(&w, &shop, &head(repo), "SHOP-23", "newer").await;
    assert!(!entry.path.exists());
    assert!(w.warm.claim_entry("shop-web", entry, "SHOP-24").is_none());
    assert!(w.warm.status("shop-web").copying.is_empty());
    w.stop().await;
}

#[tokio::test]
async fn clear_waits_for_a_claimed_entry() {
    let w = world(true).await;
    let shop = w.add_project("shop-web").await;
    let base = head(Path::new(&shop.repo_path));
    keep(&w, &shop, &base, "SHOP-20", "built").await;
    let c = client(&w).await;
    let mut events = w.events.subscribe();
    let [entry]: [_; 1] = w.warm.entries(&shop).await.try_into().unwrap();
    let claim = w.warm.claim_entry("shop-web", entry, "SHOP-21").unwrap();
    let listed = list(&c).await;
    assert_eq!(
        listed.builds[0].entries[0].taken_by.as_deref(),
        Some("SHOP-21")
    );

    let Reply::WarmClear(outcome) = c
        .request(Request::WarmClear(WarmClearParams {
            project: Some("SHOP".into()),
            repo: None,
        }))
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        outcome,
        ClearOutcome::Waiting {
            copying: vec!["SHOP-21".into()]
        }
    );
    assert_eq!(warm_event(&mut events).await, changed("SHOP"));
    assert!(built(&w, &shop).is_some(), "nothing cleared while claimed");
    // No new claim while the Clear waits.
    assert!(!w.warm.claim("shop-web", 0, "SHOP-22"));

    drop(claim);
    assert_eq!(warm_event(&mut events).await, changed("SHOP"));
    assert!(!w.warm.path(&shop).unwrap().exists());
    until("the cleared build deleted", || trash_empty(&w, "shop-web")).await;
    w.stop().await;
}
