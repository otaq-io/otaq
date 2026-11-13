//! Seeding a new worktree at start (T-0298, T-0307; WRM-04, SES-01, SES-02, M19-b, d0082, d0085) against temporary
//! repositories, with a temporary data folder, on a disk that can clone (the build is cloned) and one that can't (it
//! is renamed out of the stock): from the warm build, from the checkout only where the disk can clone (env files left
//! out, its uncommitted changes marked changed), nothing to seed, a returning ticket and a start retried after a failed launch
//! not seeded again and one retried after a failed setup seeded afresh,
//! the timestamp trick, a failed clone or rename, the recorded times (Q71), Clear waiting for a running seed, the
//! nearest build, three starts taking three builds, and a returning ticket whose build was freed at Review getting it
//! back (T-0309). Builds are fake: files the test writes. Where the test disk
//! can't clone, a copying stand-in does the cloning (it lives here only; Otaq itself never copies).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, SystemTime};

use crate::common::{Fixture, me, test_cloner};
use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::board::settings::ProjectSettings;
use otaq_core::board::tickets::Ticket;
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{BuildState, StartStep, Status};
use otaq_core::git::testing::{commit, git};
use otaq_core::ports::PortPool;
use otaq_core::service::EventBus;
use otaq_core::session::Sessions;
use otaq_core::start::{StartOutcome, Starter};
use otaq_core::store::projects::Project;
use otaq_core::ticket_env::TicketEnv;
use otaq_core::ticket_env::handoff::{FsMover, HandoffMode, Mover};
use otaq_core::ticket_env::stock::Stock;
use otaq_core::warm::clone::{CloneError, CloneProbe, CloneSupport, Cloner};
use otaq_core::warm::seed::Seeder;
use otaq_core::warm::store::{
    ClearOutcome, TrackedTimes, WarmBuilds, WarmRecord, record_file_name, times_file_name,
};
use otaq_core::warm::usage::{Ledger, ReadProbe};

const MODES: [HandoffMode; 2] = [HandoffMode::Clone, HandoffMode::Move];

/// The file system's cloning where the test disk can, else the copying stand-in; the file system's renames; and
/// every path list either was given.
#[derive(Debug)]
struct Recording {
    inner: Box<dyn Cloner>,
    calls: Mutex<Vec<Vec<PathBuf>>>,
    /// When set: tells the test the copy (or rename) began, then waits to be let go (for "Clear waits for a copy").
    gate: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    /// A clone fails after cloning the first path (leaving it behind, as a careless cloner would); a rename fails
    /// after moving every path, on one that isn't there (the renames move back what they moved).
    fail: bool,
}

impl Recording {
    fn new(data: &DataDir) -> Self {
        Self {
            inner: test_cloner(data),
            calls: Mutex::new(Vec::new()),
            gate: Mutex::new(None),
            fail: false,
        }
    }

    fn calls(&self) -> Vec<Vec<PathBuf>> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, only: Option<&[PathBuf]>) {
        self.calls
            .lock()
            .unwrap()
            .push(only.map(<[PathBuf]>::to_vec).unwrap_or_default());
        if let Some((began, release)) = self.gate.lock().unwrap().take() {
            began.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(60)).unwrap();
        }
    }
}

impl Cloner for Recording {
    fn clone_tree(
        &self,
        from: &Path,
        to: &Path,
        only: Option<&[PathBuf]>,
    ) -> Result<(), CloneError> {
        self.record(only);
        if self.fail {
            let first = &only.unwrap()[..1];
            self.inner.clone_tree(from, to, Some(first))?;
            return Err(CloneError::Io {
                path: to.to_owned(),
                error: std::io::Error::other("disk full"),
            });
        }
        self.inner.clone_tree(from, to, only)
    }
}

impl Mover for Recording {
    fn move_tree(
        &self,
        from: &Path,
        to: &Path,
        only: Option<&[PathBuf]>,
    ) -> Result<(), CloneError> {
        self.record(only);
        if self.fail {
            let mut paths = only.unwrap().to_vec();
            paths.push(PathBuf::from("not-there"));
            return FsMover.move_tree(from, to, Some(&paths));
        }
        FsMover.move_tree(from, to, only)
    }
}

struct World {
    f: Fixture,
    data: DataDir,
    warm: WarmBuilds,
    stock: Stock,
    starter: Starter,
    /// The same starter with no launch context: its starts go as far as the launch and fail there.
    unlaunched: Starter,
    /// The cloner and the mover.
    hand: Arc<Recording>,
}

/// A world whose disk hands builds over by `mode`: forced to clone (`Clone`) or not (`Move`), and records reads.
async fn world_in(mode: HandoffMode, change: impl FnOnce(&mut Recording)) -> World {
    world_with(mode, true, change).await
}

/// [`world_in`] whose disk records reads (`reads`) or not, by the read probe forced.
async fn world_with(mode: HandoffMode, reads: bool, change: impl FnOnce(&mut Recording)) -> World {
    let f = Fixture::shop().await;
    let data = f.data();
    let events = EventBus::new(64);
    let support = match mode {
        HandoffMode::Clone => CloneSupport {
            available: true,
            file_system: "apfs".into(),
            volume: "/".into(),
        },
        HandoffMode::Move => CloneSupport {
            available: false,
            file_system: "NTFS".into(),
            volume: "C:".into(),
        },
    };
    let warm = WarmBuilds::with_probes(
        data.clone(),
        Arc::new(CloneProbe::forced(support)),
        Arc::new(ReadProbe::forced(reads)),
        f.board.clone(),
        events.clone(),
    );
    assert_eq!(warm.mode(), mode);
    let stock = Stock::new(warm.clone(), f.board.clone());
    let mut hand = Recording::new(&data);
    change(&mut hand);
    let hand = Arc::new(hand);
    // A stand-in for `claude` that exits at once: every start goes past seeding, env files and setup and launches. (A
    // start that failed before its worktree was ready would remove the worktree it made.)
    let sessions = Sessions::new(f.board.store().clone(), events.clone());
    let launch = LaunchContext {
        bin_dir: "/opt/cb/bin".into(),
        plugin_dir: "/opt/cb/plugin".into(),
        path: std::env::var_os("PATH").unwrap(),
        claude: ClaudeLaunch {
            program: vec!["sh".into(), "-c".into(), "exit 0".into(), "sh".into()],
            config_dir: None,
        },
    };
    let seeder = Seeder::new(warm.clone(), stock.clone(), hand.clone(), hand.clone());
    let starter_with = |launch: Option<LaunchContext>| {
        let starter = Starter::new(
            f.board.clone(),
            data.clone(),
            PortPool::new(f.board.store().clone()),
            sessions.clone(),
            otaq_core::agent_tools::Adapters::launching_only(launch, data.clone()),
            events.clone(),
        );
        starter.set_seeder(seeder.clone());
        // A returning ticket whose build was freed gets it back through the ticket environment (M19-c).
        starter.set_ticket_env(TicketEnv::new(
            f.board.clone(),
            data.clone(),
            warm.clone(),
            seeder.clone(),
            sessions.clone(),
            events.clone(),
        ));
        starter
    };
    let starter = starter_with(Some(launch));
    let unlaunched = starter_with(None);
    // The fake build writes `out/`; `.env.local` is the env file. Both are gitignored.
    let repo = f.repo();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    commit(&repo, ".gitignore", "out/\n.env.local\n");
    commit(&repo, "src/a.txt", "a\n");
    commit(&repo, "src/b.txt", "b\n");
    World {
        f,
        data,
        warm,
        stock,
        starter,
        unlaunched,
        hand,
    }
}

async fn world(mode: HandoffMode) -> World {
    world_in(mode, |_| {}).await
}

impl World {
    fn repo(&self) -> PathBuf {
        self.f.repo()
    }

    fn moving(&self) -> bool {
        self.warm.mode() == HandoffMode::Move
    }

    fn calls(&self) -> Vec<Vec<PathBuf>> {
        self.hand.calls()
    }

    async fn project(&self) -> Project {
        self.f.board.project("SHOP").await.unwrap()
    }

    async fn settings(&self, change: impl FnOnce(&mut ProjectSettings)) {
        let mut settings = self.f.board.project_settings("SHOP").await.unwrap();
        change(&mut settings);
        self.f
            .board
            .set_project_settings("SHOP", &settings)
            .await
            .unwrap();
    }

    /// Keeps a fake build (`out/app.bin`, made an hour ago) as SHOP-9's, built at `built_at`, with the checkout's
    /// times of every tracked file recorded (it stands in for the kept worktree).
    async fn keep(&self, built_at: &str) {
        let tracked = self.tracked();
        self.keep_as(built_at, "SHOP-9", Some(&tracked)).await;
    }

    fn tracked(&self) -> Vec<String> {
        git(&self.repo(), &["ls-files"])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// [`keep`](Self::keep) with the times of only `recorded` (none: a warm build kept before times were recorded).
    async fn keep_with(&self, built_at: &str, recorded: Option<&[String]>) {
        self.keep_as(built_at, "SHOP-9", recorded).await;
    }

    /// Keeps a fake build kept from `from`: its `out/app.bin` holds "warm build" (SHOP-9) or `from`.
    async fn keep_as(&self, built_at: &str, from: &str, recorded: Option<&[String]>) {
        let project = self.project().await;
        let incoming = self.warm.incoming(&project).await.unwrap();
        std::fs::create_dir_all(incoming.path().join("out")).unwrap();
        let output = incoming.path().join("out/app.bin");
        let content = if from == "SHOP-9" { "warm build" } else { from };
        std::fs::write(&output, content).unwrap();
        set_modified(&output, ago(3_600));
        if let Some(recorded) = recorded {
            TrackedTimes::measure(&self.repo(), recorded)
                .write(incoming.path())
                .unwrap();
        }
        let record = WarmRecord {
            built_at: built_at.to_owned(),
            base: built_at.to_owned(),
            from: from.into(),
            kept_at: otaq_core::store::now_ms() - 2 * 3_600_000,
            project: String::new(),
        };
        self.stock.add(&project, incoming, record).await.unwrap();
    }

    /// The stock's entries, by number.
    fn entries(&self) -> Vec<u64> {
        let parent = self.data.warm_path("shop-web").unwrap();
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

    async fn queued(&self, title: &str) -> Ticket {
        let t = self.f.create(title).await;
        self.f.force_status(&t, Status::Queued).await;
        t
    }

    /// Ready, waiting for a slot: it counts toward the stock limit while it waits and starts.
    async fn waiting(&self, title: &str) -> Ticket {
        let t = self.f.create(title).await;
        self.f.force_status(&t, Status::WaitingForSlot).await;
        t
    }

    async fn start(&self, key: &str) -> StartOutcome {
        self.starter
            .start(key, &me(), otaq_core::session::Start::Auto)
            .await
            .unwrap()
    }

    fn worktree(&self, key: &str) -> PathBuf {
        self.data.worktree_path("shop-web", key).unwrap()
    }

    fn log(&self, key: &str) -> String {
        std::fs::read_to_string(self.data.setup_log_path(key).unwrap())
            .unwrap_or_default()
            .replace("\r\n", "\n")
    }

    /// The first words of the log line of a seed from the warm build.
    fn seeded_from(&self) -> &'static str {
        if self.moving() {
            "Took shop-web's build"
        } else {
            "Started from shop-web's warm build"
        }
    }
}

fn head(repo: &Path) -> String {
    git(repo, &["rev-parse", "HEAD"])
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

fn ago(secs: u64) -> SystemTime {
    SystemTime::now() - Duration::from_secs(secs)
}

/// Where the build was made, the tracked files were last written two hours ago, before the build (an hour ago).
fn sources_written_before_the_build(repo: &Path) -> SystemTime {
    let at = ago(7_200);
    for file in [".gitignore", "src/a.txt", "src/b.txt"] {
        set_modified(&repo.join(file), at);
    }
    at
}

fn is_recent(path: &Path) -> bool {
    modified(path) > SystemTime::now() - Duration::from_secs(3_600)
}

/// The start went past seeding, env files and setup, and launched the stand-in agent.
fn went_on(outcome: &StartOutcome) {
    assert!(matches!(outcome, StartOutcome::Started(_)), "{outcome:?}");
}

#[tokio::test]
async fn a_new_worktree_is_seeded_from_the_warm_build_with_the_timestamp_trick() {
    for mode in MODES {
        let w = world(mode).await;
        let written = sources_written_before_the_build(&w.repo());
        let built_at = head(&w.repo());
        w.keep(&built_at).await;
        // main moves on after the build was kept: b.txt changes.
        commit(&w.repo(), "src/b.txt", "b changed\n");
        w.settings(|s| s.setup_command = "echo setup ran".into())
            .await;
        w.queued("SHOP-1").await;

        went_on(&w.start("SHOP-1").await);

        let wt = w.worktree("SHOP-1");
        assert_eq!(
            std::fs::read_to_string(wt.join("out/app.bin")).unwrap(),
            "warm build"
        );
        assert!(!wt.join(record_file_name()).exists(), "the record stays");
        assert!(!wt.join(times_file_name()).exists(), "so do the times");
        assert_eq!(w.calls(), vec![vec![PathBuf::from("out")]]);
        // Unchanged since the seed's commit: the time it had where the build was made, older than the output.
        // Changed: now, newer than the output.
        let output = modified(&wt.join("out/app.bin"));
        assert_eq!(modified(&wt.join("src/a.txt")), written);
        assert_eq!(modified(&wt.join(".gitignore")), written);
        assert!(written < output);
        assert!(is_recent(&wt.join("src/b.txt")));
        assert!(modified(&wt.join("src/b.txt")) > output);
        let log = w.log("SHOP-1");
        let first = format!(
            "{} of {} (SHOP-9, 2 h ago) in ",
            w.seeded_from(),
            &built_at[..7]
        );
        assert!(log.starts_with(&first), "{mode:?}: {log}");
        assert!(log.contains("s.\n$ echo setup ran\nsetup ran"), "{log}");
        assert!(
            w.warm.status("shop-web").copying.is_empty(),
            "slot or claim released"
        );
        // Cloned, the build stays in the stock; renamed, it left it.
        assert_eq!(w.entries().len(), usize::from(!w.moving()), "{mode:?}");
        // Git sees the build as ignored files only.
        assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    }
}

#[tokio::test]
async fn with_no_warm_build_the_checkout_seeds_without_its_env_files_only_where_the_disk_can_clone()
{
    for mode in MODES {
        let w = world(mode).await;
        let repo = w.repo();
        let written = sources_written_before_the_build(&repo);
        // The user's own build, an env file, and an uncommitted change to a tracked file.
        std::fs::create_dir_all(repo.join("out")).unwrap();
        std::fs::write(repo.join("out/app.bin"), "checkout build").unwrap();
        set_modified(&repo.join("out/app.bin"), ago(3_600));
        std::fs::write(repo.join(".env.local"), "SECRET=1\n").unwrap();
        std::fs::write(repo.join("src/a.txt"), "a, being edited\n").unwrap();
        w.settings(|s| s.env_files = vec![".env.local".into()])
            .await;
        w.queued("SHOP-1").await;

        went_on(&w.start("SHOP-1").await);

        let wt = w.worktree("SHOP-1");
        // The env file came from step 4, not from the seed.
        assert!(wt.join(".env.local").exists());
        if w.moving() {
            // Renaming would take the user's own build away: the checkout is never touched (plan §4.3.2).
            assert!(w.calls().is_empty());
            assert!(!wt.join("out").exists());
            assert_eq!(
                std::fs::read_to_string(repo.join("out/app.bin")).unwrap(),
                "checkout build"
            );
            assert_eq!(w.log("SHOP-1"), "");
            continue;
        }
        assert_eq!(
            std::fs::read_to_string(wt.join("out/app.bin")).unwrap(),
            "checkout build"
        );
        assert_eq!(w.calls(), vec![vec![PathBuf::from("out")]]);
        // The checkout's build reflects its uncommitted a.txt: marked changed. b.txt has the checkout's own time.
        assert!(is_recent(&wt.join("src/a.txt")));
        assert_eq!(modified(&wt.join("src/b.txt")), written);
        assert!(written < modified(&wt.join("out/app.bin")));
        assert_eq!(
            w.log("SHOP-1"),
            format!(
                "Started from your checkout of shop-web ({}).\n",
                &head(&repo)[..7]
            )
        );
    }
}

/// The user's own build in the checkout: `out/used.bin` and `out/lib/used.rlib` (which recent tickets used), and
/// `out/stale.bin` and `out/old/` (which none did), all made an hour ago.
fn checkout_build(repo: &Path) -> Vec<(PathBuf, SystemTime)> {
    let files = [
        "out/used.bin",
        "out/lib/used.rlib",
        "out/stale.bin",
        "out/old/a.o",
        "out/old/b.o",
    ];
    for file in files {
        let path = repo.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, file).unwrap();
        set_modified(&path, ago(3_600));
    }
    crate::common::settle(&files.map(|file| repo.join(file)));
    files_and_times(&repo.join("out"))
}

/// Every file under `folder` with its modified and access times, sorted.
fn files_and_times(folder: &Path) -> Vec<(PathBuf, SystemTime)> {
    let mut out = Vec::new();
    let mut stack = vec![folder.to_owned()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            let meta = std::fs::symlink_metadata(&path).unwrap();
            if meta.is_dir() {
                stack.push(path);
            } else {
                out.push((path.clone(), meta.modified().unwrap()));
                out.push((path, meta.accessed().unwrap()));
            }
        }
    }
    out.sort();
    out
}

/// Every file under `folder`, relative to it, `/`-separated and sorted.
fn files_in(folder: &Path) -> Vec<String> {
    let mut files: Vec<String> = files_and_times(folder)
        .into_iter()
        .map(|(p, _)| {
            p.strip_prefix(folder)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect();
    files.dedup();
    files
}

/// Records that `ticket` used `used` (paths relative to a worktree) in the project's usage ledger.
async fn record_use(w: &World, ticket: &str, used: &[&str]) {
    let path = w.warm.usage_path(&w.project().await).unwrap();
    let mut ledger = Ledger::read(&path);
    ledger.record(ticket, used.iter().map(|p| p.to_string()));
    ledger.write(&path).unwrap();
}

#[tokio::test]
async fn only_the_worktrees_copy_of_the_checkout_is_trimmed_to_what_recent_tickets_used() {
    let w = world(HandoffMode::Clone).await;
    let repo = w.repo();
    let before = checkout_build(&repo);
    record_use(&w, "SHOP-7", &["out/used.bin"]).await;
    record_use(&w, "SHOP-8", &["out/lib/used.rlib", "src/a.txt"]).await;
    w.queued("SHOP-1").await;

    went_on(&w.start("SHOP-1").await);

    let wt = w.worktree("SHOP-1");
    // The copy holds only what the last tickets used; the folder nothing used is gone as a whole.
    assert_eq!(files_in(&wt.join("out")), vec!["lib/used.rlib", "used.bin"]);
    assert!(!wt.join("out/old").exists());
    // The kept files are armed: read since, they would count as used.
    for file in ["out/used.bin", "out/lib/used.rlib"] {
        let meta = std::fs::metadata(wt.join(file)).unwrap();
        assert!(
            meta.accessed().unwrap() < meta.modified().unwrap(),
            "{file}"
        );
    }
    // The checkout's own build is unchanged: the same files, with the same times, and as many (decision 055).
    assert_eq!(files_and_times(&repo.join("out")), before);
    assert_eq!(files_in(&repo.join("out")).len(), 5);
    // The timestamp trick still ran, and git sees the copy as ignored files only.
    assert!(
        w.log("SHOP-1")
            .starts_with("Started from your checkout of shop-web")
    );
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
}

#[tokio::test]
async fn with_an_empty_ledger_or_reads_not_recorded_the_copy_of_the_checkout_is_whole() {
    for reads in [true, false] {
        let w = world_with(HandoffMode::Clone, reads, |_| {}).await;
        let repo = w.repo();
        let before = checkout_build(&repo);
        if !reads {
            // Where reads aren't recorded, even a ledger trims nothing.
            record_use(&w, "SHOP-7", &["out/used.bin"]).await;
        }
        w.queued("SHOP-1").await;

        went_on(&w.start("SHOP-1").await);

        let wt = w.worktree("SHOP-1");
        assert_eq!(
            files_in(&wt.join("out")),
            files_in(&repo.join("out")),
            "{reads}"
        );
        assert_eq!(files_in(&wt.join("out")).len(), 5, "{reads}");
        assert_eq!(files_and_times(&repo.join("out")), before, "{reads}");
    }
}

#[tokio::test]
async fn nothing_is_seeded_without_a_seed_or_with_the_switch_off() {
    for mode in MODES {
        let w = world(mode).await;
        w.settings(|s| s.setup_command = "echo setup ran".into())
            .await;
        w.queued("SHOP-1").await;
        went_on(&w.start("SHOP-1").await);
        assert!(w.calls().is_empty());
        assert!(w.log("SHOP-1").starts_with("$ echo setup ran\n"));

        // The switch off: a warm build is there, but nothing is seeded.
        w.keep(&head(&w.repo())).await;
        w.settings(|s| s.warm_builds = false).await;
        w.queued("SHOP-2").await;
        went_on(&w.start("SHOP-2").await);
        assert!(w.calls().is_empty());
        assert!(!w.worktree("SHOP-2").join("out").exists());
    }
}

#[tokio::test]
async fn a_start_retried_after_a_failed_setup_is_seeded_afresh() {
    for mode in MODES {
        let w = world(mode).await;
        w.keep(&head(&w.repo())).await;
        w.settings(|s| s.setup_command = "exit 1".into()).await;
        w.queued("SHOP-1").await;
        assert!(matches!(
            w.start("SHOP-1").await,
            StartOutcome::Interrupted {
                step: StartStep::Setup,
                ..
            }
        ));
        assert_eq!(w.calls().len(), 1);
        let wt = w.worktree("SHOP-1");
        assert!(!wt.exists(), "the failed start left no worktree");
        // Another build in the stock for the next seed.
        w.keep(&head(&w.repo())).await;

        // Retry start starts over: a new worktree, seeded again, and its log begins anew.
        w.settings(|s| s.setup_command.clear()).await;
        w.starter.retry("SHOP-1", &me()).await.unwrap();
        went_on(&w.start("SHOP-1").await);
        assert_eq!(w.calls().len(), 2);
        assert!(wt.join("out").exists());
        assert!(
            w.log("SHOP-1").starts_with(w.seeded_from()),
            "{}",
            w.log("SHOP-1")
        );
    }
}

#[tokio::test]
async fn a_start_retried_after_a_failed_launch_and_a_returning_ticket_are_not_seeded_again() {
    for mode in MODES {
        let w = world(mode).await;
        w.keep(&head(&w.repo())).await;
        w.settings(|s| s.setup_command = "echo set up".into()).await;
        let t = w.queued("SHOP-1").await;
        assert!(matches!(
            w.unlaunched
                .start("SHOP-1", &me(), otaq_core::session::Start::Auto)
                .await
                .unwrap(),
            StartOutcome::Interrupted {
                step: StartStep::Launch,
                ..
            }
        ));
        assert_eq!(w.calls().len(), 1);
        let wt = w.worktree("SHOP-1");
        assert!(wt.join("out").exists(), "the ready worktree is kept");
        let log = w.log("SHOP-1");
        assert!(log.contains("set up"), "{log}");
        std::fs::remove_dir_all(wt.join("out")).unwrap();
        // Another build in the stock, so a second seed would find one.
        w.keep(&head(&w.repo())).await;

        // Retry start reuses the ready worktree and only launches: not seeded twice (SES-02), no setup again.
        w.starter.retry("SHOP-1", &me()).await.unwrap();
        went_on(&w.start("SHOP-1").await);
        assert_eq!(w.calls().len(), 1);
        assert!(!wt.join("out").exists());
        assert_eq!(w.log("SHOP-1"), log, "setup didn't run again");

        // A returning (parked) ticket keeps its worktree as it is.
        sqlx::query("UPDATE tickets SET parked = TRUE WHERE id = ?")
            .bind(&t.id)
            .execute(w.f.board.store().pool())
            .await
            .unwrap();
        w.f.force_status(&t, Status::Queued).await;
        went_on(&w.start("SHOP-1").await);
        assert_eq!(w.calls().len(), 1);
        assert!(!wt.join("out").exists());
    }
}

#[tokio::test]
async fn a_seed_commit_git_cant_resolve_skips_the_trick() {
    for mode in MODES {
        let w = world(mode).await;
        w.keep("0123456789abcdef0123456789abcdef01234567").await;
        w.queued("SHOP-1").await;
        went_on(&w.start("SHOP-1").await);
        let wt = w.worktree("SHOP-1");
        assert!(wt.join("out/app.bin").exists(), "still seeded");
        // The files keep their checkout times.
        assert!(is_recent(&wt.join("src/a.txt")));
        assert!(is_recent(&wt.join("src/b.txt")));
    }
}

#[tokio::test]
async fn a_tracked_file_without_a_recorded_time_gets_now() {
    for mode in MODES {
        let w = world(mode).await;
        let written = sources_written_before_the_build(&w.repo());
        commit(&w.repo(), "src/c.txt", "c\n");
        set_modified(&w.repo().join("src/c.txt"), written);
        // c.txt is in the seed's commit, but its time wasn't recorded.
        let recorded = [".gitignore", "src/a.txt", "src/b.txt"].map(String::from);
        w.keep_with(&head(&w.repo()), Some(&recorded)).await;
        w.queued("SHOP-1").await;
        went_on(&w.start("SHOP-1").await);
        let wt = w.worktree("SHOP-1");
        assert_eq!(modified(&wt.join("src/a.txt")), written);
        assert!(is_recent(&wt.join("src/c.txt")));
    }
}

#[tokio::test]
async fn a_warm_build_without_recorded_times_skips_the_trick() {
    for mode in MODES {
        let w = world(mode).await;
        sources_written_before_the_build(&w.repo());
        w.keep_with(&head(&w.repo()), None).await;
        w.queued("SHOP-1").await;
        went_on(&w.start("SHOP-1").await);
        let wt = w.worktree("SHOP-1");
        assert!(wt.join("out/app.bin").exists(), "still seeded");
        // Every file keeps git's checkout time: newer than the output, so everything is rebuilt.
        let output = modified(&wt.join("out/app.bin"));
        for file in [".gitignore", "src/a.txt", "src/b.txt"] {
            assert!(is_recent(&wt.join(file)), "{file}");
            assert!(modified(&wt.join(file)) > output, "{file}");
        }
    }
}

/// Keeps a build of two folders, `build/` and `out/`, so a failing cloner or mover has more than one path.
async fn keep_two_folders(w: &World) {
    let project = w.project().await;
    let incoming = w.warm.incoming(&project).await.unwrap();
    for dir in ["build", "out"] {
        std::fs::create_dir_all(incoming.path().join(dir)).unwrap();
        std::fs::write(incoming.path().join(dir).join("f"), "x").unwrap();
    }
    let record = WarmRecord {
        built_at: head(&w.repo()),
        base: head(&w.repo()),
        from: "SHOP-9".into(),
        kept_at: otaq_core::store::now_ms(),
        project: String::new(),
    };
    w.stock.add(&project, incoming, record).await.unwrap();
}

#[tokio::test]
async fn a_failed_clone_leaves_the_worktree_clean_and_the_start_goes_on() {
    let w = world_in(HandoffMode::Clone, |c| c.fail = true).await;
    keep_two_folders(&w).await;
    w.settings(|s| s.setup_command = "echo setup ran".into())
        .await;
    w.queued("SHOP-1").await;

    went_on(&w.start("SHOP-1").await);

    let wt = w.worktree("SHOP-1");
    assert_eq!(w.calls().len(), 1);
    assert!(!wt.join("build").exists() && !wt.join("out").exists());
    assert_eq!(git(&wt, &["status", "--porcelain", "--ignored"]), "");
    assert!(w.log("SHOP-1").starts_with("$ echo setup ran\n"));
}

#[tokio::test]
async fn a_failed_move_leaves_the_entry_in_stock_and_the_start_goes_on() {
    let w = world_in(HandoffMode::Move, |c| c.fail = true).await;
    keep_two_folders(&w).await;
    let entry = w.data.warm_entry_path("shop-web", 0).unwrap();
    w.settings(|s| s.setup_command = "echo setup ran".into())
        .await;
    w.queued("SHOP-1").await;

    went_on(&w.start("SHOP-1").await);

    let wt = w.worktree("SHOP-1");
    assert_eq!(w.calls().len(), 1);
    assert!(!wt.join("build").exists() && !wt.join("out").exists());
    assert_eq!(git(&wt, &["status", "--porcelain", "--ignored"]), "");
    // Every path moved back: the entry and its files are still in the stock, for the next ticket.
    assert_eq!(w.entries(), [0]);
    assert!(entry.join("build/f").is_file() && entry.join("out/f").is_file());
    assert!(WarmRecord::read(&entry).is_some());
    assert!(w.warm.status("shop-web").copying.is_empty(), "claim let go");
    assert!(w.log("SHOP-1").starts_with("$ echo setup ran\n"));
}

/// Windows: a file of the build held open (Explorer, an editor) stops the rename of its folder. The seed is skipped,
/// the build stays in the stock, and the start goes on.
#[cfg(windows)]
#[tokio::test]
async fn an_open_handle_skips_the_seed() {
    use std::os::windows::fs::OpenOptionsExt;

    let w = world(HandoffMode::Move).await;
    w.keep(&head(&w.repo())).await;
    let entry = w.data.warm_entry_path("shop-web", 0).unwrap();
    // Opened as an editor does: others may read it, not delete or rename it.
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1 /* FILE_SHARE_READ */)
        .open(entry.join("out/app.bin"))
        .unwrap();
    w.queued("SHOP-1").await;

    went_on(&w.start("SHOP-1").await);

    assert_eq!(w.calls().len(), 1, "a rename was tried");
    assert!(!w.worktree("SHOP-1").join("out").exists());
    assert_eq!(w.entries(), [0], "the build stays in the stock");
    drop(held);
    assert_eq!(
        std::fs::read_to_string(entry.join("out/app.bin")).unwrap(),
        "warm build"
    );
}

/// The M19-a acceptance: three builds in the stock and three tickets starting at once (each waiting for a slot, so
/// the limit is three): each takes a build of its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn three_starts_with_three_builds_all_start_warm() {
    let w = world(HandoffMode::Move).await;
    for title in ["one", "two", "three"] {
        w.waiting(title).await;
    }
    let tracked = w.tracked();
    let built_at = head(&w.repo());
    for from in ["SHOP-7", "SHOP-8", "SHOP-9"] {
        w.keep_as(&built_at, from, Some(&tracked)).await;
    }
    assert_eq!(w.entries(), [0, 1, 2]);

    let starts: Vec<_> = ["SHOP-1", "SHOP-2", "SHOP-3"]
        .into_iter()
        .map(|key| {
            let starter = w.starter.clone();
            tokio::spawn(async move {
                starter
                    .start(key, &me(), otaq_core::session::Start::Auto)
                    .await
                    .unwrap()
            })
        })
        .collect();
    for start in starts {
        went_on(&start.await.unwrap());
    }

    let mut taken = HashSet::new();
    for key in ["SHOP-1", "SHOP-2", "SHOP-3"] {
        let log = w.log(key);
        assert!(log.starts_with("Took shop-web's build of "), "{key}: {log}");
        let from = ["SHOP-7", "SHOP-8", "SHOP-9"]
            .into_iter()
            .find(|from| log.contains(&format!("({from}, ")))
            .unwrap_or_else(|| panic!("{key}: {log}"));
        assert!(taken.insert(from), "{from} taken twice");
        assert!(w.worktree(key).join("out/app.bin").is_file());
    }
    assert!(w.entries().is_empty(), "every build was taken");
}

/// Where builds are renamed, the nearest build is taken: of two built at ancestors of the new worktree's commit, the
/// one with fewer changed files since.
#[tokio::test]
async fn the_nearest_build_is_taken() {
    let w = world(HandoffMode::Move).await;
    // Two tickets wait for a slot: the stock keeps both builds.
    w.waiting("SHOP-1").await;
    w.waiting("SHOP-2").await;
    let tracked = w.tracked();
    let older = head(&w.repo());
    commit(&w.repo(), "src/c.txt", "c\n");
    commit(&w.repo(), "src/d.txt", "d\n");
    let newer = head(&w.repo());
    commit(&w.repo(), "src/e.txt", "e\n");
    // Kept in the other order, so the newest kept isn't the nearest.
    w.keep_as(&newer, "SHOP-8", Some(&tracked)).await;
    w.keep_as(&older, "SHOP-7", Some(&tracked)).await;
    assert_eq!(w.entries(), [0, 1]);

    went_on(&w.start("SHOP-1").await);

    let log = w.log("SHOP-1");
    assert!(
        log.starts_with(&format!(
            "Took shop-web's build of {} (SHOP-8, ",
            &newer[..7]
        )),
        "{log}"
    );
    assert_eq!(
        std::fs::read_to_string(w.worktree("SHOP-1").join("out/app.bin")).unwrap(),
        "SHOP-8"
    );
    assert_eq!(w.entries(), [1], "the farther one stays");
}

#[tokio::test(flavor = "multi_thread")]
async fn clear_waits_for_a_running_seed() {
    for mode in MODES {
        let (began_tx, began) = mpsc::channel();
        let (release, release_rx) = mpsc::channel();
        let w = world_in(mode, |c| {
            *c.gate.get_mut().unwrap() = Some((began_tx, release_rx))
        })
        .await;
        w.keep(&head(&w.repo())).await;
        w.f.skip_to(21).await;
        assert_eq!(w.queued("SHOP-21").await.key.to_string(), "SHOP-21");
        let starter = w.starter.clone();
        let start = tokio::spawn(async move {
            starter
                .start("SHOP-21", &me(), otaq_core::session::Start::Auto)
                .await
                .unwrap()
        });
        tokio::task::spawn_blocking(move || {
            began
                .recv_timeout(Duration::from_secs(60))
                .expect("the copy began")
        })
        .await
        .unwrap();
        assert_eq!(w.warm.status("shop-web").copying, vec!["SHOP-21"]);

        let warm = w.warm.clone();
        let project = w.project().await;
        let clear = tokio::spawn(async move { warm.clear(&project).await.unwrap() });
        for _ in 0..200 {
            if w.warm.status("shop-web").clearing {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(w.warm.status("shop-web").clearing, "Clear waits");
        assert!(!clear.is_finished());

        release.send(()).unwrap();
        assert_eq!(clear.await.unwrap(), ClearOutcome::Cleared);
        went_on(&start.await.unwrap());
        assert!(w.worktree("SHOP-21").join("out/app.bin").exists());
        assert!(!w.warm.path(&w.project().await).unwrap().exists());
    }
}

/// A returning ticket (sent back, its session gone) whose build was freed at Review gets a build back from the stock
/// before its agent starts, and its build is present again; one whose build is present is left as it is.
#[tokio::test]
async fn a_freed_returning_ticket_is_fetched_and_a_present_one_is_not() {
    for mode in MODES {
        let w = world(mode).await;
        w.keep(&head(&w.repo())).await;
        let t = w.queued("SHOP-1").await;
        went_on(&w.start("SHOP-1").await);
        assert_eq!(w.calls().len(), 1);
        let wt = w.worktree("SHOP-1");

        // Freed at Review (its build left the worktree), then sent back with its session gone.
        std::fs::remove_dir_all(wt.join("out")).unwrap();
        if w.moving() {
            // The first start took the only build: the Free put one back.
            w.keep(&head(&w.repo())).await;
        }
        let store = w.f.board.store();
        store
            .set_build_state(&t.id, BuildState::Freed, None)
            .await
            .unwrap();
        sqlx::query("UPDATE tickets SET resume_skill = 'address-review' WHERE id = ?")
            .bind(&t.id)
            .execute(store.pool())
            .await
            .unwrap();
        w.f.force_status(&t, Status::Queued).await;
        went_on(&w.start("SHOP-1").await);
        assert_eq!(w.calls().len(), 2, "{mode:?}: the build was brought back");
        assert!(wt.join("out/app.bin").is_file());
        assert!(
            w.log("SHOP-1").starts_with(w.seeded_from()),
            "{}",
            w.log("SHOP-1")
        );
        let row = store.ticket(&t.id).await.unwrap().unwrap();
        assert_eq!(row.build_state, "present");

        // Present: the worktree is kept as it is, nothing is brought back.
        std::fs::remove_dir_all(wt.join("out")).unwrap();
        w.f.force_status(&t, Status::Queued).await;
        went_on(&w.start("SHOP-1").await);
        assert_eq!(w.calls().len(), 2, "{mode:?}");
        assert!(!wt.join("out").exists());
    }
}
