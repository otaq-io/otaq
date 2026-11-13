//! The warm build store (WRM-02, WRM-05…WRM-07, d0082, d0085): a project's build stock at
//! `<app-data>/warm/<repo-name>/<n>/`, each entry's record, the lock that lets a Clear wait for a running copy,
//! replace (forward only), the trash, and Clear.
//!
//! - **The stock** is `warm/<repo-name>/`, marked by its marker file ([`stock_marker_name`]). Each numbered child
//!   `<n>/` is one **entry**: a worktree's gitignored files as laid out there, plus the record file
//!   ([`record_file_name`]): built at, base, from, kept at, and the project it was kept for; and the times file
//!   ([`times_file_name`], [`TrackedTimes`]): the tracked files' modified times in the kept worktree (Q71). Seeding
//!   skips both. They move with the entry. Entry numbers only grow ([`WarmBuilds::next_entry`]). On a disk that can
//!   clone the stock holds one entry (M19-a: behaviour as before).
//! - **Warm builds on** for a project ([`WarmBuilds::enabled`]): its switch is on. The data folder's disk decides only
//!   how a build is handed over ([`WarmBuilds::mode`]): cloned where it can clone, renamed where it can't (M19-b).
//! - **The lock**, one per stock: a seed holds a **reader** slot, named by its ticket, while it clones the newest
//!   entry ([`WarmBuilds::read`]). Replace and Clear take the **writer** slot and wait for the readers; while a writer
//!   waits, no new reader starts, so a Clear never waits for more than the copies already running.
//!   [`WarmBuilds::status`] says who is copying, for "Clearing… (waiting for SHOP-21's copy)".
//! - **Claims** ([`WarmBuilds::claim`]): a ticket that takes an entry claims it first, so two tickets never share one.
//!   A Clear waits for the claims too. Once its build is renamed into the worktree, the rest of the entry goes to the
//!   trash ([`WarmBuilds::taken`]).
//! - **Replace** ([`WarmBuilds::replace`]): the caller moves the new files into `<repo-name>.incoming/`
//!   ([`WarmBuilds::incoming`], one keep at a time per stock), then, under the writer slot, an older build is dropped
//!   and a newer one is renamed in as the next entry; the entries before it go to the trash.
//! - **Add and evict** ([`WarmBuilds::add_entry`], [`WarmBuilds::evict`]): where builds are renamed, not cloned, a
//!   build is always added as the next entry, and the stock ([`crate::ticket_env::stock`]) moves the entries over its
//!   limit that no ticket has claimed to the trash.
//! - **The trash** is `<repo-name>.trash/`: entries are renamed into it ([`WarmBuilds::to_trash`]) and deleted in the
//!   background, one pass at a time per stock.
//! - **Clear** ([`WarmBuilds::clear`]) renames the whole stock into the trash under the writer slot. It never turns the
//!   switch off.
//! - At service start ([`remove_interrupted`]): warm builds kept before today's stock format (an older marker, no
//!   marker, a `.mig` folder) are thrown away once (056); children of a stock that aren't entries, and `.incoming` and `.old` folders left by a crash, are deleted; the
//!   trash is emptied in the background.
//! - Each change (kept, replaced, cleared, a Clear starting to wait) publishes one [`Event::WarmBuildChanged`].
//!
//! Nothing here knows any build tool: every gitignored file is treated the same.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex as AsyncMutex, Notify, OwnedMutexGuard, oneshot};

use super::clone::{CloneProbe, CloneSupport, remove_tree};
use super::usage::ReadProbe;
use crate::board::{Board, BoardError};
use crate::data_dir::{DataDir, DataDirError};
use crate::naming;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::store::now_ms;
use crate::store::projects::Project;
use crate::ticket_env::handoff::HandoffMode;
use crate::ticket_env::stock::{self, Added};

/// The record file inside a warm build: `.otaq-warm.json`. Its leading dot and Otaq's name keep it clear of a
/// project's own files; seeding skips it.
pub fn record_file_name() -> String {
    format!(".{}-warm.json", naming::COMMAND_NAME)
}

/// What a warm build remembers about where it came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarmRecord {
    /// The worktree's `HEAD` when it was kept (the seed's commit for the timestamp trick).
    pub built_at: String,
    /// `git merge-base HEAD <base branch>` at that moment, for "forward only".
    pub base: String,
    /// The ticket it was kept from: `SHOP-21`.
    pub from: String,
    /// When it was kept, in milliseconds since the epoch.
    pub kept_at: i64,
    /// The id of the project it was kept for (two projects whose folders share a name share `warm/<repo-name>`).
    #[serde(default)]
    pub project: String,
}

impl WarmRecord {
    /// Reads the record of the warm build at `folder`; `None` when there is none or it can't be read.
    pub fn read(folder: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(folder.join(record_file_name())).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Writes the record into `folder`.
    pub fn write(&self, folder: &Path) -> io::Result<()> {
        let json = serde_json::to_string_pretty(self).expect("the record serializes");
        std::fs::write(folder.join(record_file_name()), json)
    }
}

/// The times file inside a warm build: `.otaq-warm-times`, next to the record. Seeding skips it too.
pub fn times_file_name() -> String {
    format!(".{}-warm-times", naming::COMMAND_NAME)
}

/// The first entry of a times file: its format.
const TIMES_FORMAT: &str = "times 1";

/// The modified times of a worktree's tracked files where a build was made (Q71): recorded at Done into the warm
/// build's times file, or read from the user's checkout. A seeded worktree gives its unchanged tracked files these
/// times, so a file committed after the last build still looks newer than the build's output.
///
/// The file is compact for tens of thousands of files: NUL-separated entries, the first [`TIMES_FORMAT`], then one
/// `<seconds> <nanoseconds> <path>` per file (times since the epoch; paths as `git ls-files -z` gives them).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TrackedTimes(HashMap<String, std::time::SystemTime>);

impl TrackedTimes {
    /// The modified times of `files` (relative to `root`) on disk. A symlink's own time, not followed; a file that
    /// isn't there is left out.
    pub fn measure(root: &Path, files: &[String]) -> Self {
        Self(
            files
                .iter()
                .filter_map(|file| {
                    let time = std::fs::symlink_metadata(root.join(file))
                        .and_then(|m| m.modified())
                        .ok()?;
                    Some((file.clone(), time))
                })
                .collect(),
        )
    }

    /// The recorded time of `file`.
    pub fn get(&self, file: &str) -> Option<std::time::SystemTime> {
        self.0.get(file).copied()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Reads the times file of the warm build at `folder`; `None` when there is none (a warm build kept before the
    /// times were recorded) or it can't be read.
    pub fn read(folder: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(folder.join(times_file_name())).ok()?;
        let mut entries = text.split('\0');
        if entries.next() != Some(TIMES_FORMAT) {
            return None;
        }
        let mut times = HashMap::new();
        for entry in entries.filter(|e| !e.is_empty()) {
            let mut parts = entry.splitn(3, ' ');
            let (Some(secs), Some(nanos), Some(path)) = (parts.next(), parts.next(), parts.next())
            else {
                return None;
            };
            let since = std::time::Duration::new(secs.parse().ok()?, nanos.parse().ok()?);
            times.insert(path.to_owned(), std::time::SystemTime::UNIX_EPOCH + since);
        }
        Some(Self(times))
    }

    /// Writes the times file into `folder`.
    pub fn write(&self, folder: &Path) -> io::Result<()> {
        use std::io::Write;
        let file = std::fs::File::create(folder.join(times_file_name()))?;
        let mut out = io::BufWriter::new(file);
        out.write_all(TIMES_FORMAT.as_bytes())?;
        for (path, time) in &self.0 {
            // A time before the epoch doesn't happen on a build machine; it reads as the epoch.
            let since = time
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            write!(out, "\0{} {} {path}", since.as_secs(), since.subsec_nanos())?;
        }
        out.flush()
    }
}

/// The folder name a project's worktrees and warm build go under: its repository folder's name, as in `wt/`.
pub fn repo_name(project: &Project) -> String {
    Path::new(&project.repo_path)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| project.name.clone())
}

/// The marker file of a stock: `warm/<repo-name>/.otaq-stock`. A folder in `warm/` without it is a warm build from
/// before the stock, thrown away at start (056).
pub fn stock_marker_name() -> String {
    format!(".{}-stock", naming::COMMAND_NAME)
}

/// What the marker file holds: the stock's format. `stock 2` (056): builds trimmed to what recent tickets used; a
/// stock of any other format is thrown away once at start ([`remove_interrupted`]).
const STOCK_FORMAT: &str = "stock 2";

/// `<repo-name>.incoming`, `<repo-name>.old` or `<repo-name>.usage`, next to the stock.
fn sibling(data: &DataDir, repo: &str, suffix: &str) -> Result<PathBuf, DataDirError> {
    Ok(data
        .warm_path(repo)?
        .with_file_name(format!("{repo}.{suffix}")))
}

/// Whether a folder in `warm/` is one of the store's own temporary folders, or the probe's, never a stock.
fn is_temporary(name: &str) -> bool {
    name.starts_with('.')
        || crate::data_dir::WARM_SIBLING_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

/// The entry number of a child of a stock: `0`, `12`; `None` for anything else (`012`, `+1`, the marker).
fn entry_number(name: &str) -> Option<u64> {
    let n: u64 = name.parse().ok()?;
    (n.to_string() == name).then_some(n)
}

/// One entry of a stock, `warm/<repo-name>/<n>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub n: u64,
    pub path: PathBuf,
    /// `None` for an entry without a record (no seed commit, so no timestamp trick).
    pub record: Option<WarmRecord>,
}

/// The entries of the stock at `parent`, by number. Blocking.
fn entries_in(parent: &Path) -> Vec<Entry> {
    let mut entries: Vec<Entry> = std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = entry_number(&e.file_name().to_string_lossy())?;
            let path = e.path();
            path.is_dir().then(|| Entry {
                n,
                record: WarmRecord::read(&path),
                path,
            })
        })
        .collect();
    entries.sort_by_key(|e| e.n);
    entries
}

/// The entries of `warm/<repo>`, by number. Blocking.
fn entries_blocking(data: &DataDir, repo: &str) -> Vec<Entry> {
    data.warm_path(repo)
        .map(|parent| entries_in(&parent))
        .unwrap_or_default()
}

/// The newest entry of the stock at `parent` (the highest number). Blocking.
fn newest_in(parent: &Path) -> Option<Entry> {
    entries_in(parent).pop()
}

/// Makes `parent` a stock: the folder and its marker.
fn make_stock(parent: &Path) -> io::Result<()> {
    DataDir::ensure_dir(parent)?;
    let marker = parent.join(stock_marker_name());
    if !marker.is_file() {
        std::fs::write(marker, STOCK_FORMAT)?;
    }
    Ok(())
}

/// What [`remove_interrupted`] found at service start.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Interrupted {
    /// Folders (and a half-written ledger) a crash left, deleted.
    pub removed: usize,
    /// Warm builds kept before this format (056): stocks of an older format, builds from before the stock and
    /// `<repo>.mig` folders, renamed into their repo's trash.
    pub thrown_away: usize,
    /// The repos whose trash still holds something: [`WarmBuilds::empty_trash_later`] empties them in the background.
    pub trash: Vec<String>,
}

/// Whether the stock at `parent` is of today's format: its marker holds [`STOCK_FORMAT`].
fn is_current_stock(parent: &Path) -> bool {
    std::fs::read_to_string(parent.join(stock_marker_name()))
        .is_ok_and(|format| format.trim() == STOCK_FORMAT)
}

/// Renames `folder` into `warm/<repo>.trash/` at start (056). False when it couldn't (left as it is, logged).
fn throw_away(data: &DataDir, repo: &str, folder: &Path) -> bool {
    let renamed = data
        .warm_trash_path(repo)
        .map_err(io::Error::other)
        .and_then(|trash| {
            DataDir::ensure_dir(&trash)?;
            std::fs::rename(folder, trash.join(format!("{}-start", now_ms())))
        });
    match renamed {
        Ok(()) => {
            tracing::info!(warm_build = %repo, folder = %folder.display(), "an old warm build thrown away");
            true
        }
        Err(e) => {
            tracing::warn!(warm_build = %repo, error = %e, "can't throw an old warm build away; left as it is");
            false
        }
    }
}

/// Puts the warm build in `warm/` in order before anything uses the store, once at service start:
///
/// 1. Warm builds kept before this format are thrown away, once (056): a stock whose marker isn't [`STOCK_FORMAT`],
///    a warm build from before the stock (`warm/<repo>` without the marker) and a `<repo>.mig` folder an older
///    start left are renamed into `<repo>.trash/`. A stock made after that is of today's format and never thrown
///    away again, so no "done" flag is needed. A rename that fails leaves the folder as it is (logged). Worktrees
///    aren't touched.
/// 2. Children of a stock that are neither entries nor the marker (half-made entries) are deleted.
/// 3. The `<repo>.incoming/` and `<repo>.old/` folders a crash left (and a probe's) are deleted. They are Otaq's own
///    temporary folders, not leftovers to confirm. The usage ledger `<repo>.usage` stays; a half-written
///    `<repo>.usage.tmp` is deleted.
/// 4. The trash is **not** emptied here (it may be large): its repos are returned, to empty in the background.
pub fn remove_interrupted(data: &DataDir) -> Interrupted {
    let mut found = Interrupted::default();
    let warm = data.warm_dir();
    if !warm.is_dir() {
        return found;
    }
    let mut names: Vec<String> = std::fs::read_dir(&warm)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let path = warm.join(&name);
        let own = !name.starts_with('.');
        if own && name.ends_with(".usage.tmp") {
            if path.is_file() && std::fs::remove_file(&path).is_ok() {
                found.removed += 1;
            }
            continue;
        }
        if is_temporary(&name) {
            if own && name.ends_with(".usage") {
                // The usage ledger: a file beside the stock, kept.
            } else if let Some(repo) = name.strip_suffix(".trash").filter(|_| own) {
                if std::fs::read_dir(&path).is_ok_and(|mut d| d.next().is_some()) {
                    found.trash.push(repo.to_owned());
                }
            } else if let Some(repo) = name.strip_suffix(".mig").filter(|_| own) {
                if throw_away(data, repo, &path) {
                    found.thrown_away += 1;
                    found.trash.push(repo.to_owned());
                }
            } else {
                remove_tree(&path);
                found.removed += 1;
            }
            continue;
        }
        if !path.is_dir() {
            continue;
        }
        if !is_current_stock(&path) {
            if throw_away(data, &name, &path) {
                found.thrown_away += 1;
                found.trash.push(name);
            }
            continue;
        }
        let marker = stock_marker_name();
        for child in std::fs::read_dir(&path).into_iter().flatten().flatten() {
            let child_name = child.file_name().to_string_lossy().into_owned();
            let entry = entry_number(&child_name).is_some() && child.path().is_dir();
            if !entry && child_name != marker {
                remove_tree(&child.path());
                found.removed += 1;
            }
        }
    }
    found.trash.sort();
    found.trash.dedup();
    found
}

#[derive(Debug)]
pub enum WarmError {
    /// The project's folder name can't be a folder name.
    Path(DataDirError),
    Io {
        path: PathBuf,
        error: io::Error,
    },
    Board(BoardError),
}

impl fmt::Display for WarmError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Path(e) => write!(f, "{e}"),
            Self::Io { path, error } => write!(f, "{}: {error}", path.display()),
            Self::Board(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WarmError {}

impl From<DataDirError> for WarmError {
    fn from(e: DataDirError) -> Self {
        Self::Path(e)
    }
}

impl From<BoardError> for WarmError {
    fn from(e: BoardError) -> Self {
        Self::Board(e)
    }
}

fn io_at(path: &Path) -> impl FnOnce(io::Error) -> WarmError + '_ {
    move |error| WarmError::Io {
        path: path.to_owned(),
        error,
    }
}

/// Who is copying from a stock, and whether a Clear waits for them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarmStatus {
    /// The tickets whose new worktrees are being seeded from it now, or that have claimed one of its entries:
    /// `["SHOP-21"]`.
    pub copying: Vec<String>,
    /// A Clear (or the switch turned off) waits for those copies, then clears.
    pub clearing: bool,
}

/// One entry of a stock, for Settings and Cleanup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarmEntryInfo {
    /// Its number: `0`.
    pub entry: u64,
    pub path: String,
    pub size_bytes: u64,
    /// The short SHA it was built at; `None` without a record (as are `from` and `kept_at`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_at: Option<i64>,
    /// The ticket that has claimed it, while it takes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taken_by: Option<String>,
}

/// One stock in `warm/`, for Settings and Cleanup (`warm.list`). The size is the sum of its entries; the record
/// fields are its newest entry's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarmBuildInfo {
    /// The folder's name: `shop-web`.
    pub repo: String,
    pub path: String,
    /// The key of the project it belongs to; `None` for a folder no project owns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    /// The sum of its entries' file sizes (apparent size, as Cleanup shows for worktrees).
    pub size_bytes: u64,
    /// The short SHA its newest entry was built at; `None` without a record (as are `from` and `kept_at`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub built_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kept_at: Option<i64>,
    #[serde(flatten)]
    pub status: WarmStatus,
    /// Every entry, by number.
    #[serde(default)]
    pub entries: Vec<WarmEntryInfo>,
    /// How many entries the stock keeps; `None` until it is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

/// The `warm.list` reply: whether warm builds are available at all (the switch's reason when not), how builds are
/// handed over on this disk, and every stock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WarmList {
    pub support: CloneSupport,
    #[serde(default)]
    pub mode: HandoffMode,
    pub builds: Vec<WarmBuildInfo>,
}

/// What `warm.clear` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ClearOutcome {
    /// There was no warm build to clear.
    Nothing,
    /// Cleared (its folder is deleted in the background).
    Cleared,
    /// Waiting for these tickets' copies and claims; it clears when they end, and publishes
    /// [`Event::WarmBuildChanged`].
    Waiting { copying: Vec<String> },
}

/// The lock, the claims and the cached sizes of one stock.
#[derive(Default)]
struct Slots {
    /// The reader slots: an id and the ticket being seeded.
    readers: Vec<(u64, String)>,
    writer: bool,
    writers_waiting: u32,
    clearing: bool,
    /// The claimed entries and the ticket each is claimed by.
    claimed: HashMap<u64, String>,
    /// The entries a replace is moving to the trash now: never claimed.
    trashing: HashSet<u64>,
    /// The next entry number not handed out yet (at least the highest on disk plus one).
    next: Option<u64>,
    /// Bumped on every change of the folder, so cached sizes are known stale.
    generation: u64,
    /// `(generation, [(entry, size)])`.
    size: Option<(u64, Vec<(u64, u64)>)>,
    /// Held from moving the files into `.incoming` until replace is done with it: one keep at a time.
    incoming: Arc<AsyncMutex<()>>,
    /// A pass emptying the trash runs; `trash_again`: something came in meanwhile, so it goes once more.
    trash_busy: bool,
    trash_again: bool,
    /// A pass applying the stock limit runs; `limit_again`: asked for meanwhile, so it goes once more.
    limit_busy: bool,
    limit_again: bool,
}

impl Slots {
    /// Claims entry `n` for `ticket`: false when it is claimed already, is going to the trash, or a Clear waits.
    fn claim(&mut self, n: u64, ticket: &str) -> bool {
        if self.clearing || self.claimed.contains_key(&n) || self.trashing.contains(&n) {
            return false;
        }
        self.claimed.insert(n, ticket.to_owned());
        true
    }
}

struct Inner {
    data: DataDir,
    probe: Arc<CloneProbe>,
    /// Whether the data folder's disk records reads (OTAQ-5, [`super::usage`]).
    reads: Arc<ReadProbe>,
    board: Board,
    events: EventBus,
    slots: Mutex<HashMap<String, Slots>>,
    /// Woken on every release of a slot.
    changed: Notify,
    next_reader: AtomicU64,
    /// Keeps two trash names made in the same millisecond apart.
    next_trash: AtomicU64,
}

/// The warm builds of every project. Cheap to clone.
#[derive(Clone)]
pub struct WarmBuilds {
    inner: Arc<Inner>,
}

impl fmt::Debug for WarmBuilds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WarmBuilds")
            .field("dir", &self.inner.data.warm_dir())
            .finish_non_exhaustive()
    }
}

impl WarmBuilds {
    pub fn new(data: DataDir, probe: Arc<CloneProbe>, board: Board, events: EventBus) -> Self {
        Self::with_probes(data, probe, Arc::new(ReadProbe::new()), board, events)
    }

    /// [`new`](Self::new) with the read probe given too: the service's, or a test's
    /// [`forced`](ReadProbe::forced) one.
    pub fn with_probes(
        data: DataDir,
        probe: Arc<CloneProbe>,
        reads: Arc<ReadProbe>,
        board: Board,
        events: EventBus,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                data,
                probe,
                reads,
                board,
                events,
                slots: Mutex::new(HashMap::new()),
                changed: Notify::new(),
                next_reader: AtomicU64::new(1),
                next_trash: AtomicU64::new(1),
            }),
        }
    }

    /// Whether the data folder's disk can clone (probed once, at service start).
    pub fn support(&self) -> &CloneSupport {
        self.inner.probe.support(&self.inner.data)
    }

    /// Whether the data folder's disk records reads (probed once, at service start): where it doesn't, builds are
    /// handed on whole ([`super::usage`]). Blocking the first time.
    pub fn reads_recorded(&self) -> bool {
        self.inner.reads.recorded(&self.inner.data)
    }

    /// The project's usage ledger, `warm/<repo-name>.usage`, beside the stock ([`super::usage::Ledger`]).
    pub fn usage_path(&self, project: &Project) -> Result<PathBuf, DataDirError> {
        sibling(&self.inner.data, &repo_name(project), "usage")
    }

    /// How builds are handed over on the data folder's disk.
    pub fn mode(&self) -> HandoffMode {
        HandoffMode::of(self.support())
    }

    /// Warm builds on for the project (key): its switch is on, whatever the disk (M19-b). Off means no seeding, no
    /// keeping, and Done deletes as before. How a build is handed over is [`mode`](Self::mode).
    pub async fn enabled(&self, project: &str) -> Result<bool, BoardError> {
        Ok(self
            .inner
            .board
            .project_settings(project)
            .await?
            .warm_builds)
    }

    /// The project's stock folder, `warm/<repo-name>` (it may not exist). Its entries are numbered folders in it.
    pub fn path(&self, project: &Project) -> Result<PathBuf, DataDirError> {
        self.inner.data.warm_path(&repo_name(project))
    }

    /// The record of the project's newest entry, when it has one that is its own.
    pub async fn record(&self, project: &Project) -> Option<WarmRecord> {
        let folder = self.path(project).ok()?;
        let record = newest_in(&folder)?.record?;
        let owner = self.owner(&repo_name(project), Some(&record)).await?;
        (owner.id == project.id).then_some(record)
    }

    /// The project's entries, by number: those whose record names it, or, without a record, when it is the stock's
    /// owner by name.
    pub async fn entries(&self, project: &Project) -> Vec<Entry> {
        let repo = repo_name(project);
        let entries = {
            let (data, repo) = (self.inner.data.clone(), repo.clone());
            tokio::task::spawn_blocking(move || entries_blocking(&data, &repo))
                .await
                .unwrap_or_default()
        };
        let Some(projects) = self.projects().await else {
            return Vec::new();
        };
        entries
            .into_iter()
            .filter(|e| {
                owner_among(&projects, &repo, e.record.as_ref()).is_some_and(|o| o.id == project.id)
            })
            .collect()
    }

    /// Claims entry `n` of `warm/<repo>` for `ticket`, so no other ticket takes it. False when it is claimed
    /// already, or a Clear waits. [`unclaim`](Self::unclaim) releases it.
    pub fn claim(&self, repo: &str, n: u64, ticket: &str) -> bool {
        self.with(repo, |s| s.claim(n, ticket))
    }

    /// Releases the claim on entry `n` of `warm/<repo>`.
    pub fn unclaim(&self, repo: &str, n: u64) {
        self.release(repo, |s| {
            s.claimed.remove(&n);
        });
    }

    /// [`claim`](Self::claim) for `entry`, held until the [`Claim`] is dropped. `None` when it is claimed already,
    /// or is gone (a replace or a Clear moved it to the trash after `entry` was read).
    pub fn claim_entry(&self, repo: &str, entry: Entry, ticket: &str) -> Option<Claim> {
        if !self.claim(repo, entry.n, ticket) {
            return None;
        }
        let claim = Claim {
            repo: repo.to_owned(),
            n: entry.n,
            path: entry.path,
            record: entry.record,
            warm: self.clone(),
        };
        // Checked after claiming: from here on nothing moves it, so it is either still there or already gone.
        claim.path.is_dir().then_some(claim)
    }

    /// The project's entry under `claim` was taken (its build renamed into a worktree, M19-b): what is left of it
    /// (its record, its times, the env files left out) goes to the trash, then the claim is let go.
    pub(crate) fn taken(&self, claim: Claim, project: &Project) {
        let repo = claim.repo.clone();
        if let Err(e) = self.to_trash(&repo, &claim.path) {
            tracing::warn!(warm_build = %repo, entry = claim.n, error = %e, "can't move what is left of a taken build to the trash");
        }
        self.changed_folder(&repo);
        drop(claim);
        self.measure_later(&repo);
        self.publish(&repo, Some(project));
    }

    /// The next entry number of `warm/<repo>`: higher than every one on disk and every one handed out before.
    pub fn next_entry(&self, repo: &str) -> u64 {
        let on_disk = entries_blocking(&self.inner.data, repo)
            .last()
            .map_or(0, |e| e.n + 1);
        self.with(repo, |s| {
            let n = s.next.unwrap_or(0).max(on_disk);
            s.next = Some(n + 1);
            n
        })
    }

    /// Renames `folder` (an entry, or a whole stock) into `warm/<repo>.trash/` and empties the trash in the
    /// background.
    pub fn to_trash(&self, repo: &str, folder: &Path) -> Result<(), WarmError> {
        self.trash_as(repo, folder, "")
    }

    fn trash_as(&self, repo: &str, folder: &Path, label: &str) -> Result<(), WarmError> {
        let trash = self.inner.data.warm_trash_path(repo)?;
        DataDir::ensure_dir(&trash).map_err(io_at(&trash))?;
        let seq = self.inner.next_trash.fetch_add(1, Ordering::Relaxed);
        let to = trash.join(format!("{}-{seq}{label}", now_ms()));
        std::fs::rename(folder, &to).map_err(io_at(folder))?;
        self.empty_trash_later(repo);
        Ok(())
    }

    /// Empties `warm/<repo>.trash/` off the async threads: one pass at a time per stock; something trashed during a
    /// pass makes it go once more. Outside a runtime it empties it here.
    pub fn empty_trash_later(&self, repo: &str) {
        let Ok(trash) = self.inner.data.warm_trash_path(repo) else {
            return;
        };
        let start = self.with(repo, |s| {
            if s.trash_busy {
                s.trash_again = true;
                false
            } else {
                s.trash_busy = true;
                true
            }
        });
        if !start {
            return;
        }
        let (store, repo) = (self.clone(), repo.to_owned());
        let pass = move || {
            loop {
                for child in std::fs::read_dir(&trash).into_iter().flatten().flatten() {
                    remove_tree(&child.path());
                }
                let again = store.with(&repo, |s| {
                    let again = std::mem::take(&mut s.trash_again);
                    s.trash_busy = again;
                    again
                });
                if !again {
                    break;
                }
            }
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn_blocking(pass);
            }
            Err(_) => pass(),
        }
    }

    /// Who is copying from (or has claimed an entry of) the stock in `warm/<repo>`, and whether a Clear waits.
    pub fn status(&self, repo: &str) -> WarmStatus {
        self.with(repo, |s| {
            let mut claimed: Vec<(&u64, &String)> = s.claimed.iter().collect();
            claimed.sort();
            WarmStatus {
                copying: s
                    .readers
                    .iter()
                    .map(|(_, t)| t)
                    .chain(claimed.into_iter().map(|(_, t)| t))
                    .cloned()
                    .collect(),
                clearing: s.clearing,
            }
        })
    }

    /// A reader slot on the project's newest entry, named by the ticket being seeded (T-0298), held while it clones.
    /// Waits while a replace or Clear holds or waits for the writer slot. `None` when, by then, the project has no
    /// warm build of its own; the slot is released then.
    pub async fn read(&self, project: &Project, ticket: &str) -> Option<ReadSlot> {
        let repo = repo_name(project);
        let parent = self.inner.data.warm_path(&repo).ok()?;
        let id = self.inner.next_reader.fetch_add(1, Ordering::Relaxed);
        self.wait_until(&repo, |s| {
            let free = !s.writer && s.writers_waiting == 0;
            if free {
                s.readers.push((id, ticket.to_owned()));
            }
            free
        })
        .await;
        let mut slot = ReadSlot {
            store: self.clone(),
            repo,
            id,
            entry: 0,
            path: parent.clone(),
            record: None,
        };
        let newest = newest_in(&parent)?;
        (slot.entry, slot.path, slot.record) = (newest.n, newest.path, newest.record);
        let owner = self.owner(&slot.repo, slot.record.as_ref()).await?;
        (owner.id == project.id).then_some(slot)
    }

    /// The project's `<repo-name>.incoming/` folder for a keep (T-0297), held until [`replace`](Self::replace) takes
    /// it or it is dropped (then deleted in the background). Waits for another keep of the same warm build. The
    /// folder is empty: anything a crash left there is removed first.
    pub async fn incoming(&self, project: &Project) -> Result<Incoming, WarmError> {
        let repo = repo_name(project);
        let path = sibling(&self.inner.data, &repo, "incoming")?;
        let lock = self.with(&repo, |s| s.incoming.clone());
        let guard = lock.lock_owned().await;
        let ready = {
            let path = path.clone();
            tokio::task::spawn_blocking(move || {
                remove_tree(&path);
                DataDir::ensure_dir(&path)
            })
            .await
            .expect("the incoming folder is made")
        };
        ready.map_err(io_at(&path))?;
        Ok(Incoming {
            path,
            guard: Some(guard),
        })
    }

    /// Makes the files in `incoming` the project's warm build, **forward only**: when a warm build exists and its
    /// base isn't an ancestor of (or equal to) `record.base`, the new one is dropped, since an older build never
    /// replaces a newer one. Also dropped when the stock belongs to another project, or warm builds were turned off
    /// meanwhile. Waits for the copies running from the old one. A kept build is renamed in as the next entry, and
    /// the entries before it that no ticket has claimed go to the trash; a dropped one is deleted in the background.
    pub async fn replace(
        &self,
        project: &Project,
        mut incoming: Incoming,
        record: WarmRecord,
    ) -> Result<Added, WarmError> {
        let repo = repo_name(project);
        let parent = self.inner.data.warm_path(&repo)?;
        let record = WarmRecord {
            project: project.id.clone(),
            ..record
        };
        record
            .write(&incoming.path)
            .map_err(io_at(&incoming.path))?;
        let writer = self.write(&repo, false).await;
        let before = entries_in(&parent);
        let keep = self.enabled(&project.key).await?
            && self.is_newer(project, before.last(), &record).await?;
        if !keep {
            drop(writer);
            tracing::info!(project = %project.key, from = %record.from, "an older warm build isn't kept");
            return Ok(Added::Dropped);
        }
        let n = self.next_entry(&repo);
        let folder = self.inner.data.warm_entry_path(&repo, n)?;
        make_stock(&parent).map_err(io_at(&parent))?;
        // Should this fail, the old entries are still in place.
        std::fs::rename(&incoming.path, &folder).map_err(io_at(&folder))?;
        // Taken: nothing is left to delete.
        incoming.guard.take();
        // The unclaimed ones are marked in the same lock as the check, so no claim slips in before the rename.
        let unclaimed: Vec<Entry> = self.with(&repo, |s| {
            before
                .into_iter()
                .filter(|old| !s.claimed.contains_key(&old.n))
                .inspect(|old| {
                    s.trashing.insert(old.n);
                })
                .collect()
        });
        for old in &unclaimed {
            if let Err(e) = self.to_trash(&repo, &old.path) {
                tracing::warn!(project = %project.key, entry = old.n, error = %e, "can't move an older warm build to the trash");
            }
        }
        self.with(&repo, |s| {
            for old in &unclaimed {
                s.trashing.remove(&old.n);
            }
        });
        self.changed_folder(&repo);
        drop(writer);
        self.measure_later(&repo);
        tracing::info!(project = %project.key, from = %record.from, entry = n, "warm build kept");
        self.publish(&repo, Some(project));
        Ok(Added::Kept { entry: n })
    }

    /// Renames the files in `incoming` in as the next entry of the project's stock, whatever is there already (where
    /// builds are renamed, the stock holds several; [`Stock::add`](crate::ticket_env::stock::Stock::add) applies the
    /// limit after). Returns the entry's number.
    pub(crate) async fn add_entry(
        &self,
        project: &Project,
        mut incoming: Incoming,
        record: WarmRecord,
    ) -> Result<u64, WarmError> {
        let repo = repo_name(project);
        let parent = self.inner.data.warm_path(&repo)?;
        let record = WarmRecord {
            project: project.id.clone(),
            ..record
        };
        record
            .write(&incoming.path)
            .map_err(io_at(&incoming.path))?;
        let writer = self.write(&repo, false).await;
        let n = self.next_entry(&repo);
        let folder = self.inner.data.warm_entry_path(&repo, n)?;
        make_stock(&parent).map_err(io_at(&parent))?;
        std::fs::rename(&incoming.path, &folder).map_err(io_at(&folder))?;
        // Taken: nothing is left to delete.
        incoming.guard.take();
        self.changed_folder(&repo);
        drop(writer);
        self.measure_later(&repo);
        tracing::info!(project = %project.key, from = %record.from, entry = n, "build added to the stock");
        self.publish(&repo, Some(project));
        Ok(n)
    }

    /// Every entry of `warm/<repo>`, whichever project it was kept for, by number.
    pub(crate) async fn all_entries(&self, repo: &str) -> Vec<Entry> {
        let (data, repo) = (self.inner.data.clone(), repo.to_owned());
        tokio::task::spawn_blocking(move || entries_blocking(&data, &repo))
            .await
            .unwrap_or_default()
    }

    /// Moves to the trash the entries of `warm/<repo>` that `pick` chooses among the ones no ticket has claimed (and
    /// none is moving to the trash), under the writer slot. `pick` runs under the slots' lock, so no claim slips in
    /// between choosing and moving. Returns the entries moved; one that can't move is logged and left.
    pub(crate) async fn evict(
        &self,
        repo: &str,
        pick: impl FnOnce(&[Entry]) -> Vec<u64>,
    ) -> Result<Vec<u64>, WarmError> {
        let writer = self.write(repo, false).await;
        let entries = entries_blocking(&self.inner.data, repo);
        let picked: Vec<Entry> = self.with(repo, |s| {
            let free: Vec<Entry> = entries
                .into_iter()
                .filter(|e| !s.claimed.contains_key(&e.n) && !s.trashing.contains(&e.n))
                .collect();
            let chosen = pick(&free);
            free.into_iter()
                .filter(|e| chosen.contains(&e.n))
                .inspect(|e| {
                    s.trashing.insert(e.n);
                })
                .collect()
        });
        let mut evicted = Vec::with_capacity(picked.len());
        for entry in &picked {
            match self.to_trash(repo, &entry.path) {
                Ok(()) => evicted.push(entry.n),
                Err(e) => {
                    tracing::warn!(warm_build = %repo, entry = entry.n, error = %e, "can't move a build over the stock limit to the trash");
                }
            }
        }
        self.with(repo, |s| {
            for entry in &picked {
                s.trashing.remove(&entry.n);
            }
        });
        if !evicted.is_empty() {
            self.changed_folder(repo);
            tracing::info!(warm_build = %repo, entries = ?evicted, "builds over the stock limit moved to the trash");
        }
        drop(writer);
        if !evicted.is_empty() {
            self.measure_later(repo);
            self.publish(repo, None);
        }
        Ok(evicted)
    }

    /// Starts a pass applying the stock limit of `warm/<repo>`: false when one runs already (it then goes once more).
    pub(crate) fn begin_limit_pass(&self, repo: &str) -> bool {
        self.with(repo, |s| {
            if s.limit_busy {
                s.limit_again = true;
                false
            } else {
                s.limit_busy = true;
                true
            }
        })
    }

    /// Ends a pass [`begin_limit_pass`](Self::begin_limit_pass) started: true when it was asked for meanwhile, so
    /// the caller runs once more (still holding the pass).
    pub(crate) fn end_limit_pass(&self, repo: &str) -> bool {
        self.with(repo, |s| {
            let again = std::mem::take(&mut s.limit_again);
            s.limit_busy = again;
            again
        })
    }

    /// Lets go of a pass [`begin_limit_pass`](Self::begin_limit_pass) started that can't end itself (its caller was
    /// cancelled): no pass runs then, and the next one asked for starts.
    pub(crate) fn abort_limit_pass(&self, repo: &str) {
        self.with(repo, |s| {
            s.limit_busy = false;
            s.limit_again = false;
        });
    }

    /// Clears the project's stock (WRM-06): waits for the copies running from it and the claims on it, renames it
    /// into the trash and empties that in the background. Returns once it is cleared. The switch stays as it is.
    pub async fn clear(&self, project: &Project) -> Result<ClearOutcome, WarmError> {
        self.clear_repo(&repo_name(project), Some(project), None)
            .await
    }

    /// [`clear`](Self::clear) for a request: answers at once, either done or "waiting for SHOP-21's copy" (the Clear
    /// then goes on in the background and publishes [`Event::WarmBuildChanged`] when done), so the app's other
    /// requests on the same connection don't wait behind it.
    pub async fn start_clear(&self, project: Option<&Project>, repo: &str) -> ClearOutcome {
        let (tx, rx) = oneshot::channel();
        let store = self.clone();
        let (project, repo) = (project.cloned(), repo.to_owned());
        tokio::spawn(async move {
            let what = project
                .as_ref()
                .map(|p| p.key.clone())
                .unwrap_or(repo.clone());
            if let Err(e) = store.clear_repo(&repo, project.as_ref(), Some(tx)).await {
                tracing::warn!(warm_build = %what, error = %e, "can't clear the warm build");
            }
        });
        rx.await.unwrap_or(ClearOutcome::Nothing)
    }

    async fn clear_repo(
        &self,
        repo: &str,
        project: Option<&Project>,
        mut progress: Option<oneshot::Sender<ClearOutcome>>,
    ) -> Result<ClearOutcome, WarmError> {
        let mut answer = |outcome: &ClearOutcome| {
            if let Some(tx) = progress.take() {
                let _ = tx.send(outcome.clone());
            }
        };
        let folder = self.inner.data.warm_path(repo)?;
        let owned = match project {
            Some(project) => {
                let record = newest_in(&folder).and_then(|e| e.record);
                self.owner(repo, record.as_ref())
                    .await
                    .is_some_and(|owner| owner.id == project.id)
            }
            None => true,
        };
        // A second Clear while one waits joins it.
        let already = self.with(repo, |s| std::mem::replace(&mut s.clearing, true));
        if !folder.is_dir() || !owned || already {
            if !already {
                self.with(repo, |s| s.clearing = false);
            }
            let outcome = if already {
                ClearOutcome::Waiting {
                    copying: self.status(repo).copying,
                }
            } else {
                ClearOutcome::Nothing
            };
            answer(&outcome);
            return Ok(outcome);
        }
        let copying = self.status(repo).copying;
        if !copying.is_empty() {
            answer(&ClearOutcome::Waiting { copying });
            self.publish(repo, project);
        }
        let writer = self.write(repo, true).await;
        let result = if folder.is_dir() {
            self.trash_as(repo, &folder, "-all")
        } else {
            Ok(())
        };
        self.with(repo, |s| s.clearing = false);
        if let Err(e) = result {
            drop(writer);
            self.publish(repo, project);
            return Err(e);
        }
        self.changed_folder(repo);
        drop(writer);
        tracing::info!(warm_build = %repo, "warm build cleared");
        self.publish(repo, project);
        answer(&ClearOutcome::Cleared);
        Ok(ClearOutcome::Cleared)
    }

    /// Every stock in `warm/` (WRM-07), by folder name, with its entries. The store's temporary folders, and a folder
    /// without entries, aren't listed; an entry without a record lists with its size only.
    pub async fn list(&self) -> Result<WarmList, BoardError> {
        let dir = self.inner.data.warm_dir();
        let stocks = {
            let dir = dir.clone();
            tokio::task::spawn_blocking(move || {
                let mut repos: Vec<String> = std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|name| !is_temporary(name))
                    .collect();
                repos.sort();
                repos
                    .into_iter()
                    .map(|repo| {
                        let entries = entries_in(&dir.join(&repo));
                        (repo, entries)
                    })
                    .filter(|(_, entries)| !entries.is_empty())
                    .collect::<Vec<_>>()
            })
            .await
            .unwrap_or_default()
        };
        let mut builds = Vec::with_capacity(stocks.len());
        for (repo, entries) in stocks {
            let path = dir.join(&repo);
            let newest = entries.last().and_then(|e| e.record.clone());
            let project = self.owner(&repo, newest.as_ref()).await.map(|p| p.key);
            let sizes = self.sizes(&repo, &entries).await;
            let claimed = self.with(&repo, |s| s.claimed.clone());
            let limit = stock::limit_of(&self.inner.board, self.mode(), &repo)
                .await
                .ok();
            let entries: Vec<WarmEntryInfo> = entries
                .into_iter()
                .zip(sizes)
                .map(|(e, size_bytes)| WarmEntryInfo {
                    entry: e.n,
                    path: e.path.display().to_string(),
                    size_bytes,
                    built_at: e.record.as_ref().map(|r| short_sha(&r.built_at)),
                    from: e.record.as_ref().map(|r| r.from.clone()),
                    kept_at: e.record.as_ref().map(|r| r.kept_at),
                    taken_by: claimed.get(&e.n).cloned(),
                })
                .collect();
            builds.push(WarmBuildInfo {
                size_bytes: entries.iter().map(|e| e.size_bytes).sum(),
                built_at: newest.as_ref().map(|r| short_sha(&r.built_at)),
                from: newest.as_ref().map(|r| r.from.clone()),
                kept_at: newest.as_ref().map(|r| r.kept_at),
                status: self.status(&repo),
                path: path.display().to_string(),
                project,
                repo,
                entries,
                limit,
            });
        }
        Ok(WarmList {
            support: self.support().clone(),
            mode: self.mode(),
            builds,
        })
    }

    async fn projects(&self) -> Option<Vec<Project>> {
        let mut projects = self.inner.board.projects().await.ok()?;
        projects.sort_by(|a, b| a.key.cmp(&b.key));
        Some(projects)
    }

    /// The project a warm build belongs to: the one its record names, else (no record, or that project is gone)
    /// the first project, by key, whose folder has that name.
    async fn owner(&self, repo: &str, record: Option<&WarmRecord>) -> Option<Project> {
        let projects = self.projects().await?;
        owner_among(&projects, repo, record).cloned()
    }

    /// Whether `record` may replace `current`, the stock's newest entry: there is none, or it is the project's own
    /// and has a base that is an ancestor of (or equal to) the new base. A base git can't resolve (history
    /// rewritten) can't be compared, so the new build wins.
    async fn is_newer(
        &self,
        project: &Project,
        current: Option<&Entry>,
        record: &WarmRecord,
    ) -> Result<bool, WarmError> {
        let Some(current) = current else {
            return Ok(true);
        };
        let old = current.record.as_ref();
        let owner = self.owner(&repo_name(project), old).await;
        if owner.is_some_and(|owner| owner.id != project.id) {
            return Ok(false);
        }
        let Some(old) = old else {
            return Ok(true);
        };
        let git = self.inner.board.project_git(&project.key).await?;
        let (code, _) = git
            .run_with_code(
                Path::new(&project.repo_path),
                &["merge-base", "--is-ancestor", &old.base, &record.base],
            )
            .await
            .map_err(BoardError::Git)?;
        Ok(code != 1)
    }

    /// The apparent size of each of `entries` of `warm/<repo>`, measured off the async threads in one pass and cached
    /// until the stock changes.
    async fn sizes(&self, repo: &str, entries: &[Entry]) -> Vec<u64> {
        let wanted: Vec<u64> = entries.iter().map(|e| e.n).collect();
        let (generation, cached) = self.with(repo, |s| (s.generation, s.size.clone()));
        if let Some((at, sizes)) = cached
            && at == generation
            && sizes.iter().map(|(n, _)| *n).eq(wanted.iter().copied())
        {
            return sizes.into_iter().map(|(_, size)| size).collect();
        }
        let measured: Vec<(u64, u64)> = {
            let paths: Vec<(u64, PathBuf)> =
                entries.iter().map(|e| (e.n, e.path.clone())).collect();
            tokio::task::spawn_blocking(move || {
                paths
                    .into_iter()
                    .map(|(n, path)| (n, apparent_size(&path)))
                    .collect()
            })
            .await
            .unwrap_or_else(|_| wanted.iter().map(|n| (*n, 0)).collect())
        };
        self.with(repo, |s| {
            if s.generation == generation {
                s.size = Some((generation, measured.clone()));
            }
        });
        measured.into_iter().map(|(_, size)| size).collect()
    }

    /// Measures a changed stock in the background, so the next `warm.list` finds it cached.
    fn measure_later(&self, repo: &str) {
        let (store, repo) = (self.clone(), repo.to_owned());
        tokio::spawn(async move {
            let entries = {
                let (data, repo) = (store.inner.data.clone(), repo.clone());
                tokio::task::spawn_blocking(move || entries_blocking(&data, &repo))
                    .await
                    .unwrap_or_default()
            };
            store.sizes(&repo, &entries).await;
        });
    }

    fn changed_folder(&self, repo: &str) {
        self.with(repo, |s| {
            s.generation += 1;
            s.size = None;
        });
    }

    fn publish(&self, repo: &str, project: Option<&Project>) {
        self.inner.events.publish(Event::WarmBuildChanged {
            repo: repo.to_owned(),
            project: project.map(|p| p.key.clone()),
        });
    }

    /// Takes the writer slot, waiting for every reader (and, with `claims`, every claim). While it waits, new
    /// readers wait too.
    async fn write(&self, repo: &str, claims: bool) -> WriteSlot {
        let _waiting = WaitingWriter::new(self, repo);
        self.wait_until(repo, |s| {
            let free = !s.writer && s.readers.is_empty() && (!claims || s.claimed.is_empty());
            if free {
                s.writer = true;
            }
            free
        })
        .await;
        WriteSlot {
            store: self.clone(),
            repo: repo.to_owned(),
        }
    }

    /// Waits until `take` (run under the lock) succeeds.
    async fn wait_until(&self, repo: &str, mut take: impl FnMut(&mut Slots) -> bool) {
        loop {
            let notified = self.inner.changed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.with(repo, &mut take) {
                return;
            }
            notified.await;
        }
    }

    fn with<T>(&self, repo: &str, f: impl FnOnce(&mut Slots) -> T) -> T {
        let mut slots = self.inner.slots.lock().expect("warm build slots");
        f(slots.entry(repo.to_owned()).or_default())
    }

    fn release(&self, repo: &str, f: impl FnOnce(&mut Slots)) {
        self.with(repo, f);
        self.inner.changed.notify_waiters();
    }
}

/// [`WarmBuilds::owner`] among `projects` (sorted by key).
fn owner_among<'a>(
    projects: &'a [Project],
    repo: &str,
    record: Option<&WarmRecord>,
) -> Option<&'a Project> {
    if let Some(named) = record.and_then(|r| projects.iter().find(|p| p.id == r.project)) {
        return Some(named);
    }
    projects.iter().find(|p| repo_name(p) == repo)
}

/// A claim on one stock entry ([`WarmBuilds::claim_entry`]), so no other ticket takes it. Released when dropped.
pub struct Claim {
    repo: String,
    n: u64,
    path: PathBuf,
    record: Option<WarmRecord>,
    warm: WarmBuilds,
}

impl Claim {
    /// The entry's number.
    pub fn entry(&self) -> u64 {
        self.n
    }

    /// The entry's folder.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Its record; `None` for an entry without one.
    pub fn record(&self) -> Option<&WarmRecord> {
        self.record.as_ref()
    }
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.warm.unclaim(&self.repo, self.n);
    }
}

impl fmt::Debug for Claim {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Claim")
            .field("entry", &self.n)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

/// A reader slot on a stock entry (T-0298 clones from [`path`](Self::path) while holding it). Released when dropped.
pub struct ReadSlot {
    store: WarmBuilds,
    repo: String,
    id: u64,
    entry: u64,
    path: PathBuf,
    record: Option<WarmRecord>,
}

impl ReadSlot {
    /// The entry's folder.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The entry's number.
    pub fn entry(&self) -> u64 {
        self.entry
    }

    /// Its record; `None` for a folder without one (no seed commit, so no timestamp trick).
    pub fn record(&self) -> Option<&WarmRecord> {
        self.record.as_ref()
    }
}

impl Drop for ReadSlot {
    fn drop(&mut self) {
        let id = self.id;
        self.store
            .release(&self.repo, |s| s.readers.retain(|(r, _)| *r != id));
    }
}

impl fmt::Debug for ReadSlot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadSlot")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

struct WriteSlot {
    store: WarmBuilds,
    repo: String,
}

impl Drop for WriteSlot {
    fn drop(&mut self) {
        self.store.release(&self.repo, |s| s.writer = false);
    }
}

/// Counts a writer waiting for its slot (so new readers wait), also when the wait is given up.
struct WaitingWriter {
    store: WarmBuilds,
    repo: String,
}

impl WaitingWriter {
    fn new(store: &WarmBuilds, repo: &str) -> Self {
        store.with(repo, |s| s.writers_waiting += 1);
        Self {
            store: store.clone(),
            repo: repo.to_owned(),
        }
    }
}

impl Drop for WaitingWriter {
    fn drop(&mut self) {
        self.store.release(&self.repo, |s| s.writers_waiting -= 1);
    }
}

/// `<repo-name>.incoming/`, filled by a keep (T-0297 moves the gitignored files into [`path`](Self::path)) and handed
/// to [`WarmBuilds::replace`]. Dropped without that, it is deleted in the background.
pub struct Incoming {
    path: PathBuf,
    guard: Option<OwnedMutexGuard<()>>,
}

impl Incoming {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Incoming {
    fn drop(&mut self) {
        if let Some(guard) = self.guard.take() {
            delete_in_background(self.path.clone(), guard);
        }
    }
}

impl fmt::Debug for Incoming {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Incoming")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

/// Deletes `path` off the async threads, holding `guard` until it is gone.
fn delete_in_background(path: PathBuf, guard: OwnedMutexGuard<()>) {
    let delete = move || {
        remove_tree(&path);
        drop(guard);
    };
    match tokio::runtime::Handle::try_current() {
        Ok(runtime) => {
            runtime.spawn_blocking(delete);
        }
        Err(_) => delete(),
    }
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// The sum of the sizes of everything in `folder` (symlinks not followed).
fn apparent_size(folder: &Path) -> u64 {
    let mut total = 0;
    let mut stack = vec![folder.to_owned()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.path().symlink_metadata() else {
                continue;
            };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_record_round_trips_and_keeps_clear_of_project_files() {
        let tmp = tempfile::tempdir().unwrap();
        let record = WarmRecord {
            built_at: "3e1f9a2c0d".into(),
            base: "9c41e7b".into(),
            from: "SHOP-21".into(),
            kept_at: 1_760_000_000_000,
            project: "p1".into(),
        };
        assert_eq!(WarmRecord::read(tmp.path()), None);
        record.write(tmp.path()).unwrap();
        assert_eq!(WarmRecord::read(tmp.path()), Some(record));
        let name = record_file_name();
        assert!(name.starts_with('.') && name.ends_with("-warm.json"));
        assert!(name.contains(naming::COMMAND_NAME));
        // An unreadable record is no record.
        std::fs::write(tmp.path().join(&name), "{").unwrap();
        assert_eq!(WarmRecord::read(tmp.path()), None);
    }

    #[test]
    fn the_times_round_trip_to_the_nanosecond() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("wt");
        std::fs::create_dir_all(root.join("src dir")).unwrap();
        std::fs::write(root.join("src dir/a b.txt"), "a").unwrap();
        std::fs::write(root.join("c.txt"), "c").unwrap();
        let files = ["src dir/a b.txt", "c.txt", "gone.txt"].map(String::from);
        let times = TrackedTimes::measure(&root, &files);
        assert_eq!(times.len(), 2, "a file that isn't there is left out");
        assert_eq!(TrackedTimes::read(tmp.path()), None);
        times.write(tmp.path()).unwrap();
        let read = TrackedTimes::read(tmp.path()).unwrap();
        assert_eq!(read, times);
        let a = std::fs::metadata(root.join("src dir/a b.txt"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(read.get("src dir/a b.txt"), Some(a));
        let name = times_file_name();
        assert!(name.starts_with('.') && name.contains(naming::COMMAND_NAME));
        // Another format, or a broken entry, is no times file.
        std::fs::write(tmp.path().join(&name), "times 9\0").unwrap();
        assert_eq!(TrackedTimes::read(tmp.path()), None);
        std::fs::write(tmp.path().join(&name), "times 1\0x 1 a").unwrap();
        assert_eq!(TrackedTimes::read(tmp.path()), None);
    }

    #[test]
    fn temporary_folders_are_told_apart_from_warm_builds() {
        let data = DataDir::new("/data");
        assert_eq!(
            sibling(&data, "shop-web", "incoming").unwrap(),
            Path::new("/data/warm/shop-web.incoming")
        );
        assert!(sibling(&data, "../x", "old").is_err());
        for temporary in [
            "shop-web.incoming",
            "shop-web.old",
            "shop-web.trash",
            "shop-web.mig",
            "shop-web.usage",
            "shop-web.usage.tmp",
            ".probe",
        ] {
            assert!(is_temporary(temporary), "{temporary}");
        }
        for build in ["shop-web", "shop.web", "old-app", "trash"] {
            assert!(!is_temporary(build), "{build}");
        }
    }

    #[test]
    fn entries_are_numbered_folders_and_the_marker_names_the_stock() {
        for (name, n) in [
            ("0", Some(0)),
            ("12", Some(12)),
            ("012", None),
            ("+1", None),
            ("x", None),
        ] {
            assert_eq!(entry_number(name), n, "{name}");
        }
        assert_eq!(entry_number(&stock_marker_name()), None);
        let marker = stock_marker_name();
        assert!(marker.starts_with('.') && marker.ends_with("-stock"));
        assert!(marker.contains(naming::COMMAND_NAME));

        let tmp = tempfile::tempdir().unwrap();
        let parent = tmp.path().join("shop-web");
        assert!(entries_in(&parent).is_empty());
        make_stock(&parent).unwrap();
        assert!(is_current_stock(&parent));
        assert_eq!(
            std::fs::read_to_string(parent.join(&marker)).unwrap(),
            STOCK_FORMAT
        );
        for dir in ["10", "2", "junk"] {
            std::fs::create_dir_all(parent.join(dir)).unwrap();
        }
        std::fs::write(parent.join("7"), "a file, not an entry").unwrap();
        let numbers: Vec<u64> = entries_in(&parent).iter().map(|e| e.n).collect();
        assert_eq!(numbers, [2, 10]);
        assert_eq!(newest_in(&parent).unwrap().n, 10);
    }

    /// The names in `warm/`, sorted.
    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn interrupted_folders_are_removed_and_builds_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path());
        assert_eq!(remove_interrupted(&data), Interrupted::default());
        let warm = data.warm_dir();
        make_stock(&warm.join("shop-web")).unwrap();
        for folder in [
            "shop-web/0/deep",
            "shop-web/1.part",
            "shop-web.incoming/deep",
            "shop-web.old",
            ".probe",
        ] {
            std::fs::create_dir_all(warm.join(folder)).unwrap();
        }
        let found = remove_interrupted(&data);
        assert_eq!((found.removed, found.thrown_away), (4, 0));
        assert!(found.trash.is_empty());
        assert_eq!(names_in(&warm), ["shop-web"]);
        assert!(
            data.warm_entry_path("shop-web", 0)
                .unwrap()
                .join("deep")
                .is_dir()
        );
        // A second start leaves today's stock as it is.
        assert_eq!(remove_interrupted(&data), Interrupted::default());
    }

    #[test]
    fn old_warm_builds_are_thrown_away_once_and_the_ledger_stays() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path());
        let warm = data.warm_dir();
        // A stock of the first format.
        let old = warm.join("old-app");
        std::fs::create_dir_all(old.join("0").join("out")).unwrap();
        std::fs::write(old.join(stock_marker_name()), "stock 1").unwrap();
        // A warm build from before the stock, and a migration an older start left.
        std::fs::create_dir_all(warm.join("legacy").join("out")).unwrap();
        std::fs::create_dir_all(warm.join("half.mig").join("out")).unwrap();
        // Today's stock, its ledger and a half-written ledger.
        make_stock(&warm.join("shop-web")).unwrap();
        std::fs::create_dir_all(warm.join("shop-web").join("3").join("out")).unwrap();
        std::fs::write(warm.join("shop-web.usage"), "usage 1\0").unwrap();
        std::fs::write(warm.join("shop-web.usage.tmp"), "usage 1\0half").unwrap();

        let found = remove_interrupted(&data);
        assert_eq!((found.removed, found.thrown_away), (1, 3));
        assert_eq!(found.trash, ["half", "legacy", "old-app"]);
        assert_eq!(
            names_in(&warm),
            [
                "half.trash",
                "legacy.trash",
                "old-app.trash",
                "shop-web",
                "shop-web.usage"
            ]
        );
        for (repo, inside) in [("half", "out"), ("legacy", "out"), ("old-app", "0/out")] {
            let [thrown] =
                <[String; 1]>::try_from(names_in(&warm.join(format!("{repo}.trash")))).unwrap();
            assert!(
                warm.join(format!("{repo}.trash"))
                    .join(thrown)
                    .join(inside)
                    .is_dir(),
                "{repo}"
            );
        }
        assert!(
            data.warm_entry_path("shop-web", 3)
                .unwrap()
                .join("out")
                .is_dir()
        );
        assert_eq!(
            std::fs::read(warm.join("shop-web.usage")).unwrap(),
            b"usage 1\0"
        );

        // Once: the next start only finds the trash to empty.
        let again = remove_interrupted(&data);
        assert_eq!((again.removed, again.thrown_away), (0, 0));
        assert_eq!(again.trash, ["half", "legacy", "old-app"]);
    }

    #[test]
    fn sizes_add_up_the_files() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("a").join("b")).unwrap();
        std::fs::write(tmp.path().join("a").join("one"), "12").unwrap();
        std::fs::write(tmp.path().join("a").join("b").join("two"), "345").unwrap();
        assert_eq!(apparent_size(tmp.path()), 5);
        assert_eq!(short_sha("3e1f9a2c0d"), "3e1f9a2");
    }

    #[test]
    fn a_claimed_entry_is_not_claimed_twice_and_a_clear_refuses_claims() {
        let mut slots = Slots::default();
        assert!(slots.claim(0, "SHOP-21"));
        assert!(!slots.claim(0, "SHOP-22"), "one winner");
        assert!(slots.claim(1, "SHOP-22"));
        slots.claimed.remove(&0);
        assert!(slots.claim(0, "SHOP-23"));
        slots.clearing = true;
        assert!(!slots.claim(2, "SHOP-24"), "a Clear waits: no new claims");
        slots.clearing = false;
        slots.trashing.insert(3);
        assert!(
            !slots.claim(3, "SHOP-25"),
            "going to the trash: not claimed"
        );
    }
}
