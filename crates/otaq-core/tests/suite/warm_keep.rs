//! Keeping the build at Done (T-0297, T-0307; WRM-03, REC-05, M19-b, d0082, d0085) against a running service in a
//! temporary data folder and temporary repositories. The "build" is fake: the test writes gitignored files and folders
//! into the worktree. The probe is forced, so builds are cloned (a disk that can clone) or renamed (one that can't)
//! whatever the test disk can do; keeping only renames either way. A seed after a keep clones where the test disk
//! can, else a copying stand-in does; where builds are renamed it renames. The read probe is forced too (reads are
//! recorded, unless a test says otherwise), so a build is trimmed to what the last tickets used (d054).

use crate::common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use otaq_core::board::settings::{AgentLimits, GlobalSettings};
use otaq_core::domain::{Actor, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::testing::TestService;
use otaq_core::store::projects::Project;
use otaq_core::ticket_env::handoff::{FsMover, HandoffMode};
use otaq_core::ticket_env::stock::Stock;
use otaq_core::warm::clone::{CloneProbe, CloneSupport};
use otaq_core::warm::seed::{SeededFrom, Seeder};
use otaq_core::warm::store::{TrackedTimes, WarmRecord, record_file_name, times_file_name};
use otaq_core::warm::usage::{Ledger, RECENT_TICKETS, ReadProbe};

fn me() -> Actor {
    Actor::User("me".into())
}

/// Both disks: one that can clone (`true`), one that can't (`false`).
const BOTH: [bool; 2] = [true, false];

async fn world(available: bool) -> TestService {
    world_reading(available, true).await
}

/// [`world`] on a disk that records reads (`reads`) or doesn't.
async fn world_reading(available: bool, reads: bool) -> TestService {
    let w = TestService::start_with(|config| otaq_core::service::ServiceConfig {
        clone_probe: Arc::new(CloneProbe::forced(CloneSupport {
            available,
            file_system: if available { "apfs" } else { "NTFS" }.into(),
            volume: "/Volumes/Test".into(),
        })),
        read_probe: Arc::new(ReadProbe::forced(reads)),
        ..config
    })
    .await;
    let mode = if available {
        HandoffMode::Clone
    } else {
        HandoffMode::Move
    };
    assert_eq!(w.warm.mode(), mode);
    w
}

/// A seeder as the service makes one: renames where the disk can't clone; clones where it can (with the copying
/// stand-in where the test disk can't).
fn seeder(w: &TestService) -> Seeder {
    Seeder::new(
        w.warm.clone(),
        Stock::new(w.warm.clone(), w.board.clone()),
        Arc::from(common::test_cloner(&w.data)),
        Arc::new(FsMover),
    )
}

/// shop-web with a committed `.gitignore` and its env files in the checkout, listed in its settings.
async fn shop_web(w: &TestService) -> Project {
    let project = w.add_project("shop-web").await;
    let repo = Path::new(&project.repo_path);
    commit(
        repo,
        ".gitignore",
        "out/\n*.log\n.env.local\nconfig/\nlocked/\n",
    );
    std::fs::write(repo.join(".env.local"), "API=1\n").unwrap();
    std::fs::create_dir_all(repo.join("config")).unwrap();
    std::fs::write(repo.join("config").join(".env"), "DB=1\n").unwrap();
    let mut settings = w.board.project_settings("SHOP").await.unwrap();
    settings.env_files = vec![".env.local".into(), "config/.env".into()];
    w.board
        .set_project_settings("SHOP", &settings)
        .await
        .unwrap();
    project
}

/// A started ticket with one commit of its own.
async fn started(w: &TestService, title: &str, file: &str) -> (String, PathBuf) {
    let t = w.create("SHOP", title).await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, file, "code\n");
    w.force_status(&t, Status::WaitingForYou).await;
    (t.key.to_string(), wt)
}

/// What a build leaves: gitignored files and folders, and the env files copied in at start.
fn build(wt: &Path, content: &str) {
    std::fs::create_dir_all(wt.join("out").join("deep")).unwrap();
    std::fs::write(wt.join("out").join("deep").join("app.bin"), content).unwrap();
    std::fs::write(wt.join("build.log"), "built\n").unwrap();
    std::fs::write(wt.join(".env.local"), "API=1\n").unwrap();
    std::fs::create_dir_all(wt.join("config")).unwrap();
    std::fs::write(wt.join("config").join(".env"), "DB=1\n").unwrap();
    std::fs::write(wt.join("config").join("cache.json"), "{}\n").unwrap();
}

async fn merge(w: &TestService, key: &str) {
    let outcome = w.review.local_merge(key, "m", false, &me()).await.unwrap();
    assert!(
        matches!(outcome, LocalMergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
}

/// The project's stock entries, by number.
fn entries(w: &TestService, project: &Project) -> Vec<u64> {
    let parent = w.warm.path(project).unwrap();
    let mut numbers: Vec<u64> = std::fs::read_dir(&parent)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().to_string_lossy().parse().ok())
        .collect();
    numbers.sort();
    numbers
}

/// The project's single stock entry; a path that isn't there when it has none.
fn warm(w: &TestService, project: &Project) -> PathBuf {
    let parent = w.warm.path(project).unwrap();
    match entries(w, project).as_slice() {
        [] => parent.join("none"),
        [n] => parent.join(n.to_string()),
        more => panic!("one entry expected, found {more:?}"),
    }
}

fn built(w: &TestService, project: &Project) -> Option<String> {
    std::fs::read_to_string(warm(w, project).join("out/deep/app.bin")).ok()
}

fn rev(repo: &str, what: &str) -> String {
    git(Path::new(repo), &["rev-parse", what]).trim().to_owned()
}

fn modified(path: &Path) -> SystemTime {
    std::fs::symlink_metadata(path).unwrap().modified().unwrap()
}

fn set_modified(path: &Path, time: SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

/// Sets both of `path`'s times: a file neither written nor read since.
fn set_untouched(path: &Path, time: SystemTime) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_accessed(time)
                .set_modified(time),
        )
        .unwrap();
}

/// The project's newest stock entry.
fn newest(w: &TestService, project: &Project) -> PathBuf {
    let last = *entries(w, project).last().expect("an entry");
    w.warm.path(project).unwrap().join(last.to_string())
}

/// The names in the newest entry's `out/`.
fn kept_out(w: &TestService, project: &Project) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(newest(w, project).join("out"))
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// A build of `tickets` files in `out/`, of which only `out/t<used>.bin` is written by this ticket; the others and
/// `out/stale.bin` were there before it started and nothing touched them.
fn build_using(wt: &Path, tickets: usize, used: usize) {
    std::fs::create_dir_all(wt.join("out")).unwrap();
    let before = ago(3_600);
    for i in 1..=tickets {
        let file = wt.join("out").join(format!("t{i}.bin"));
        std::fs::write(&file, format!("t{i}")).unwrap();
        if i != used {
            set_untouched(&file, before);
        }
    }
    std::fs::write(wt.join("out/stale.bin"), "old").unwrap();
    set_untouched(&wt.join("out/stale.bin"), before);
}

fn ledger(w: &TestService, project: &Project) -> Ledger {
    Ledger::read(&w.warm.usage_path(project).unwrap())
}

fn ago(secs: u64) -> SystemTime {
    SystemTime::now() - Duration::from_secs(secs)
}

fn trash_empty(w: &TestService) -> bool {
    std::fs::read_dir(w.data.warm_trash_path("shop-web").unwrap())
        .map(|mut d| d.next().is_none())
        .unwrap_or(true)
}

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..200 {
        if done() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("{what}");
}

/// Agent limits on with one slot, taken by a Working ticket, and `count` Ready tickets waiting for a slot. The
/// scheduler keeps them so: no slot is free.
async fn waiting_for_a_slot(w: &TestService, count: usize) {
    w.board
        .set_global_settings(&GlobalSettings {
            agent_limits: AgentLimits {
                enabled: true,
                max_concurrency: 1,
                ..AgentLimits::default()
            },
            ..GlobalSettings::default()
        })
        .await
        .unwrap();
    let working = w.create("SHOP", "working").await;
    w.force_status(&working, Status::Working).await;
    for i in 0..count {
        let ticket = w.create("SHOP", &format!("waiting {i}")).await;
        w.force_status(&ticket, Status::WaitingForSlot).await;
    }
}

#[tokio::test]
async fn done_keeps_the_gitignored_files_and_removes_the_worktree() {
    for available in BOTH {
        let w = world(available).await;
        let project = shop_web(&w).await;
        let (key, wt) = started(&w, "Checkout API", "api.ts").await;
        build(&wt, "v1");
        let api = modified(&wt.join("api.ts"));

        merge(&w, &key).await;

        assert!(!wt.exists(), "worktree removed");
        let folder = warm(&w, &project);
        assert_eq!(built(&w, &project).as_deref(), Some("v1"), "folders kept");
        assert!(folder.join("build.log").is_file(), "files kept");
        assert!(folder.join("config/cache.json").is_file());
        assert!(!folder.join(".env.local").exists(), "env files aren't kept");
        assert!(
            !folder.join("config/.env").exists(),
            "nor inside a kept folder"
        );
        assert!(!folder.join("api.ts").exists(), "tracked files aren't kept");
        let record = WarmRecord::read(&folder).expect("a record");
        let branch = otaq_core::naming::branch(&key);
        assert_eq!(record.built_at, rev(&project.repo_path, &branch));
        let base = git(
            Path::new(&project.repo_path),
            &["merge-base", &branch, "main"],
        );
        assert_eq!(record.base, base.trim());
        assert_eq!(record.from, key);
        assert_eq!(record.project, project.id);
        assert!(record.kept_at > 0);
        assert!(folder.join(record_file_name()).is_file());
        let times = TrackedTimes::read(&folder).expect("the tracked files' times");
        assert_eq!(times.get("api.ts"), Some(api));
        assert!(times.get(".gitignore").is_some());
        assert_eq!(times.get("build.log"), None, "only tracked files");
        w.stop().await;
    }
}

/// Q71: SHOP-1 builds, then edits `cart.ts` and commits it without building again, and is merged. SHOP-2, seeded
/// from that build, must not take `cart.ts` as built: it gets the time it had in SHOP-1's worktree, newer than the
/// output, so a tool comparing times rebuilds it. Files unchanged since the build keep their older times.
#[tokio::test]
async fn a_file_committed_after_the_last_build_is_newer_than_the_output_in_the_next_seed() {
    for available in BOTH {
        let w = world(available).await;
        let project = shop_web(&w).await;
        let (key, wt) = started(&w, "Checkout API", "api.ts").await;
        let written = ago(7_200);
        set_modified(&wt.join("api.ts"), written);
        build(&wt, "v1");
        let built = ago(3_600);
        set_modified(&wt.join("out/deep/app.bin"), built);
        commit(&wt, "cart.ts", "new cart\n");
        let edited = ago(1_800);
        set_modified(&wt.join("cart.ts"), edited);
        merge(&w, &key).await;
        assert_eq!(
            TrackedTimes::read(&warm(&w, &project))
                .unwrap()
                .get("cart.ts"),
            Some(edited)
        );
        // main moves on after the merge.
        commit(Path::new(&project.repo_path), "hotfix.ts", "fix\n");

        let t = w.create("SHOP", "Receipts").await;
        let next = w.start_worktree(&t).await;
        let git = w.board.project_git("SHOP").await.unwrap();
        seeder(&w)
            .seed(&w.board, &git, &project, &t.key.to_string(), &next, false)
            .await
            .expect("seeded");

        let output = modified(&next.join("out/deep/app.bin"));
        assert_eq!(output, built, "a clone or a rename keeps the output's time");
        assert_eq!(modified(&next.join("cart.ts")), edited);
        assert!(modified(&next.join("cart.ts")) > output, "rebuilt");
        assert_eq!(modified(&next.join("api.ts")), written);
        assert!(written < output, "taken as built");
        assert!(
            modified(&next.join("hotfix.ts")) > ago(600),
            "changed since: now"
        );
        assert!(!next.join(times_file_name()).exists());
        // Cloned, the build stays in the stock; renamed, it left it.
        assert_eq!(entries(&w, &project).len(), usize::from(available));
        w.stop().await;
    }
}

/// d0082: skill copies an older Otaq left (marked, gitignored by their own `.gitignore`) and M27's
/// `otaq-custom-*` are neither kept at Done nor seeded into a later worktree, even from a stock that holds them; the
/// build around them is.
#[tokio::test]
async fn skill_copies_are_neither_kept_nor_seeded() {
    use otaq_core::skills::testing::leftover_copy;
    // With a skill of the project's own committed beside the copies, git lists each copy's files one by one (the
    // copy's own `.gitignore`), not `.claude/` as one folder: they stay all the same.
    for (available, tracked) in BOTH.into_iter().flat_map(|a| [(a, false), (a, true)]) {
        let w = world(available).await;
        let project = shop_web(&w).await;
        if tracked {
            std::fs::create_dir_all(Path::new(&project.repo_path).join(".claude/skills/grill-me"))
                .unwrap();
            commit(
                Path::new(&project.repo_path),
                ".claude/skills/grill-me/SKILL.md",
                "mine\n",
            );
        }
        let copies = |wt: &Path| {
            leftover_copy(wt, "otaq-my-commit");
            let old = wt.join(".claude/skills/otaq-custom-commit");
            std::fs::create_dir_all(&old).unwrap();
            std::fs::write(old.join(".gitignore"), "*\n").unwrap();
            std::fs::write(old.join("SKILL.md"), "old").unwrap();
        };
        let all = [
            ".agents/skills/otaq-my-commit",
            ".claude/skills/otaq-my-commit",
            ".claude/skills/otaq-custom-commit",
        ];
        let (key, wt) = started(&w, "Checkout API", "api.ts").await;
        build(&wt, "v1");
        copies(&wt);
        merge(&w, &key).await;

        let folder = warm(&w, &project);
        assert_eq!(built(&w, &project).as_deref(), Some("v1"), "the build kept");
        for copy in all {
            assert!(!folder.join(copy).exists(), "{copy} kept");
        }

        // A stock holding them anyway (kept before the guard) doesn't seed them.
        copies(&folder);
        let t = w.create("SHOP", "Receipts").await;
        let next = w.start_worktree(&t).await;
        let git = w.board.project_git("SHOP").await.unwrap();
        seeder(&w)
            .seed(&w.board, &git, &project, &t.key.to_string(), &next, false)
            .await
            .expect("seeded");
        assert_eq!(
            std::fs::read_to_string(next.join("out/deep/app.bin")).unwrap(),
            "v1"
        );
        for copy in all {
            assert!(!next.join(copy).exists(), "{copy} seeded");
        }
        w.stop().await;
    }
}

#[tokio::test]
async fn a_dirty_worktree_keeps_everything() {
    for available in BOTH {
        let w = world(available).await;
        let project = shop_web(&w).await;
        let (key, wt) = started(&w, "Price table", "price.ts").await;
        build(&wt, "v1");
        std::fs::write(wt.join("notes.md"), "not committed\n").unwrap();
        let t = w.board.ticket(&key).await.unwrap();

        let cleanup = w.review.finish(&t).await.unwrap();

        assert!(!cleanup.worktree_removed);
        assert!(wt.join("out/deep/app.bin").is_file(), "the build stays");
        assert!(wt.join("build.log").is_file());
        assert!(!warm(&w, &project).exists(), "nothing kept");
        w.stop().await;
    }
}

#[tokio::test]
async fn switch_off_deletes_as_before() {
    for available in BOTH {
        let w = world(available).await;
        let project = shop_web(&w).await;
        let mut settings = w.board.project_settings("SHOP").await.unwrap();
        settings.warm_builds = false;
        w.board
            .set_project_settings("SHOP", &settings)
            .await
            .unwrap();
        let (key, wt) = started(&w, "Checkout API", "api.ts").await;
        build(&wt, "v1");

        merge(&w, &key).await;

        assert!(!wt.exists(), "worktree removed");
        assert!(!warm(&w, &project).exists(), "nothing kept");
        w.stop().await;
    }
}

/// M19-b: where the disk can't clone, the build is kept too (renamed into the stock), not deleted as it was before.
#[tokio::test]
async fn a_disk_that_cant_clone_keeps_the_build_in_stock() {
    let w = world(false).await;
    let project = shop_web(&w).await;
    let (key, wt) = started(&w, "Checkout API", "api.ts").await;
    build(&wt, "v1");

    merge(&w, &key).await;

    assert!(!wt.exists(), "worktree removed");
    assert_eq!(built(&w, &project).as_deref(), Some("v1"), "kept");
    let folder = warm(&w, &project);
    assert!(!folder.join(".env.local").exists(), "env files aren't kept");
    assert_eq!(WarmRecord::read(&folder).unwrap().from, key);
    let listed = w.warm.list().await.unwrap();
    assert_eq!(listed.mode, HandoffMode::Move);
    assert_eq!(listed.builds[0].entries.len(), 1);
    w.stop().await;
}

/// Where builds are cloned the stock keeps one, forward only.
#[tokio::test]
async fn a_newer_base_replaces_and_an_older_one_doesnt() {
    let w = world(true).await;
    let project = shop_web(&w).await;
    // SHOP-1 starts on an older main; SHOP-2 after a commit lands there.
    let (older, older_wt) = started(&w, "Old idea", "old.ts").await;
    commit(Path::new(&project.repo_path), "hotfix.ts", "fix\n");
    let (first, first_wt) = started(&w, "Checkout API", "api.ts").await;
    build(&first_wt, "first");
    merge(&w, &first).await;
    assert_eq!(built(&w, &project).as_deref(), Some("first"));

    // Closed without merging, built on the older base: dropped.
    build(&older_wt, "older");
    w.review.close(&older, true, &me()).await.unwrap();
    assert!(!older_wt.exists(), "worktree removed");
    assert_eq!(
        built(&w, &project).as_deref(),
        Some("first"),
        "not replaced"
    );
    let record = WarmRecord::read(&warm(&w, &project)).unwrap();
    assert_eq!(record.from, first);
    let times = TrackedTimes::read(&warm(&w, &project)).unwrap();
    assert!(times.get("api.ts").is_some() && times.get("old.ts").is_none());

    // Started after the merge: a newer base replaces it.
    let (newer, newer_wt) = started(&w, "Receipts", "receipt.ts").await;
    build(&newer_wt, "newer");
    merge(&w, &newer).await;
    assert_eq!(built(&w, &project).as_deref(), Some("newer"), "replaced");
    assert_eq!(WarmRecord::read(&warm(&w, &project)).unwrap().from, newer);
    // The times file moved in with the build.
    let times = TrackedTimes::read(&warm(&w, &project)).unwrap();
    assert!(times.get("receipt.ts").is_some(), "the newer build's times");
    w.stop().await;
}

/// Where builds are renamed, every kept build is added; the stock limit (tickets waiting for a slot) decides how many
/// stay, the one farthest from main's tip going first.
#[tokio::test]
async fn move_mode_keeps_both_then_applies_the_limit() {
    let w = world(false).await;
    let project = shop_web(&w).await;
    waiting_for_a_slot(&w, 2).await;
    let (first, first_wt) = started(&w, "Checkout API", "api.ts").await;
    build(&first_wt, "first");
    merge(&w, &first).await;
    // main moves on: the first build is farther from its tip than the next.
    for file in ["a.ts", "b.ts", "c.ts"] {
        commit(Path::new(&project.repo_path), file, "x\n");
    }
    let (second, second_wt) = started(&w, "Receipts", "receipt.ts").await;
    build(&second_wt, "second");
    merge(&w, &second).await;
    assert_eq!(entries(&w, &project), [0, 1], "both kept: the limit is 2");

    // One waiting ticket starts: the limit is 1, and the farther build goes.
    let tickets = w.board.tickets("SHOP").await.unwrap();
    let waiting = tickets
        .iter()
        .find(|t| t.status == Status::WaitingForSlot)
        .unwrap();
    w.force_status(waiting, Status::Working).await;
    let stock = Stock::new(w.warm.clone(), w.board.clone());
    assert_eq!(stock.enforce_limit("shop-web").await.unwrap(), [0]);
    assert_eq!(entries(&w, &project), [1]);
    assert_eq!(WarmRecord::read(&warm(&w, &project)).unwrap().from, second);
    until("the evicted build deleted", || trash_empty(&w)).await;
    w.stop().await;
}

/// The M19-b acceptance: on a disk that can't clone (NTFS), Done keeps the build in the stock, and a new ticket's
/// worktree takes it (renamed), leaving the stock empty.
#[tokio::test]
async fn done_on_ntfs_then_a_new_ticket_takes_the_build() {
    let w = world(false).await;
    let project = shop_web(&w).await;
    let (key, wt) = started(&w, "Checkout API", "api.ts").await;
    build(&wt, "v1");
    merge(&w, &key).await;
    assert_eq!(entries(&w, &project).len(), 1, "one build in the stock");

    let t = w.create("SHOP", "Receipts").await;
    let next = w.start_worktree(&t).await;
    let git = w.board.project_git("SHOP").await.unwrap();
    let seeded = seeder(&w)
        .seed(&w.board, &git, &project, &t.key.to_string(), &next, false)
        .await
        .expect("seeded");

    assert_eq!(
        std::fs::read_to_string(next.join("out/deep/app.bin")).unwrap(),
        "v1"
    );
    assert!(next.join("config/cache.json").is_file());
    assert!(!next.join(record_file_name()).exists());
    assert!(
        matches!(
            seeded.from,
            SeededFrom::WarmBuild {
                moved: true,
                entry: 0,
                ..
            }
        ),
        "{:?}",
        seeded.from
    );
    assert!(
        seeded.log_line().starts_with("Took shop-web's build of "),
        "{}",
        seeded.log_line()
    );
    assert!(entries(&w, &project).is_empty(), "the stock is empty");
    until("what was left of the entry deleted", || trash_empty(&w)).await;
    w.stop().await;
}

/// A folder that can't be moved (an empty read-only folder: renaming a folder to another parent needs write
/// permission on it) stops the keep halfway: what was moved goes back, and Done goes on.
#[cfg(unix)]
#[tokio::test]
async fn a_failure_halfway_still_removes_the_worktree() {
    use std::os::unix::fs::PermissionsExt;

    for available in BOTH {
        let w = world(available).await;
        let project = shop_web(&w).await;
        let (key, wt) = started(&w, "Checkout API", "api.ts").await;
        build(&wt, "v1");
        let locked = wt.join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();

        merge(&w, &key).await;

        assert!(!wt.exists(), "worktree removed");
        assert!(!warm(&w, &project).exists(), "nothing kept");
        let incoming = warm(&w, &project).with_extension("incoming");
        until("the half-filled .incoming is deleted", || {
            !incoming.exists()
        })
        .await;
        w.stop().await;
    }
}

/// d054: each Done records what its ticket's build used, and the build goes on with only what the last 5 tickets
/// used. Ticket i writes `out/t<i>.bin`; the other files it carries (the earlier and later tickets' files, and
/// `stale.bin`) are older than its worktree and never read. After the sixth, the kept build holds what tickets 2–6
/// used: not ticket 1's file, nor the file nobody used.
#[tokio::test]
async fn a_kept_build_holds_only_what_the_last_tickets_used() {
    for available in BOTH {
        let w = world(available).await;
        let project = shop_web(&w).await;
        let tickets = RECENT_TICKETS + 1;
        let mut keys = Vec::new();
        for i in 1..=tickets {
            let (key, wt) = started(&w, &format!("Ticket {i}"), &format!("f{i}.ts")).await;
            build_using(&wt, tickets, i);
            merge(&w, &key).await;
            assert!(!wt.exists(), "worktree removed");
            keys.push(key);
            if i == 1 {
                assert_eq!(
                    kept_out(&w, &project),
                    ["t1.bin"],
                    "only what ticket 1 used"
                );
            }
        }
        let expected: Vec<String> = (2..=tickets).map(|i| format!("t{i}.bin")).collect();
        assert_eq!(kept_out(&w, &project), expected, "{available}");
        let recorded = ledger(&w, &project);
        let newest_first: Vec<String> = keys.iter().rev().take(RECENT_TICKETS).cloned().collect();
        assert_eq!(recorded.tickets(), newest_first.as_slice());
        assert!(!recorded.keeps("out/t1.bin"), "ticket 1 dropped out");
        assert!(!recorded.keeps("out/stale.bin"));
        assert!(recorded.keeps("out/t2.bin"));
        let entry = newest(&w, &project);
        assert_eq!(WarmRecord::read(&entry).unwrap().from, keys[tickets - 1]);
        assert!(TrackedTimes::read(&entry).is_some(), "the times file stays");
        // What was left out went to the trash, which empties in the background.
        until("the trash empties", || trash_empty(&w)).await;
        w.stop().await;
    }
}

/// A Free at Review hands the build on the same way: it is recorded and trimmed.
#[tokio::test]
async fn a_free_records_and_trims_too() {
    for available in BOTH {
        let w = world(available).await;
        let project = shop_web(&w).await;
        let (key, wt) = started(&w, "Checkout API", "api.ts").await;
        build_using(&wt, 2, 1);
        let ticket = w.board.ticket(&key).await.unwrap();

        w.ticket_env.free(&ticket.key).await;

        assert!(!wt.join("out").exists(), "freed");
        assert_eq!(ledger(&w, &project).tickets(), std::slice::from_ref(&key));
        assert_eq!(kept_out(&w, &project), ["t1.bin"]);
        assert_eq!(WarmRecord::read(&newest(&w, &project)).unwrap().from, key);
        w.stop().await;
    }
}

/// On a disk that doesn't record reads, a build is handed on whole, as before, and no ledger is written.
#[tokio::test]
async fn without_recorded_reads_the_build_is_kept_whole() {
    for available in BOTH {
        let w = world_reading(available, false).await;
        let project = shop_web(&w).await;
        let (key, wt) = started(&w, "Checkout API", "api.ts").await;
        build_using(&wt, 2, 1);

        merge(&w, &key).await;

        assert_eq!(kept_out(&w, &project), ["stale.bin", "t1.bin", "t2.bin"]);
        assert!(!w.warm.usage_path(&project).unwrap().exists(), "no ledger");
        w.stop().await;
    }
}
