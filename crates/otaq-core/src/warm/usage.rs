//! What recent tickets used (decisions 054, 055; OTAQ-5): the only module that knows "used". A warm build carries
//! only the gitignored files the last [`RECENT_TICKETS`] tickets' builds wrote or read.
//!
//! - **Used** ([`measure`]): a file (or symlink, by its own times; folders are never judged) whose modified or access
//!   time is at or after [`since`], the moment the worktree was made (its `.git` file's modified time). No `since`:
//!   every file counts as used.
//! - **Arming** ([`arm`]): every file Otaq hands on gets its access time set to `min(mtime − 1 s, now − 2 h)`, its
//!   modified time untouched, so the first read in the next worktree always moves it (APFS and Linux `relatime`
//!   only record a read when the access time is older than the modified time; NTFS only when it is over an hour old).
//!   Clones and renames keep both times, so arming once when a build enters the stock covers every worktree seeded
//!   from it.
//! - **The read probe** ([`ReadProbe`]): at service start, next to the clone probe, a file in `<app-data>/warm/.probe`
//!   is armed and read; where its access time didn't move (`noatime`, NTFS with last-access updates off), nothing is
//!   measured, recorded or left out: builds are handed on whole, as before.
//! - **The ledger** ([`Ledger`]), `warm/<repo>.usage` beside the stock: the last [`RECENT_TICKETS`] tickets, newest
//!   first, and per path a bit mask of the tickets that used it. Its paths are the keep set.
//! - **Applying it**: [`hand_on`] (a build entering the stock: measure, record, leave out, arm) and [`trim_copy`] (a
//!   worktree's copy of the user's checkout: leave out, arm; the checkout itself is never touched, 055). Left-out files
//!   go to the stock's trash, a folder with no kept file as one rename. Neither ever fails what calls it.
//!
//! Nothing here knows any build tool: every gitignored file is judged the same, by its times.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use super::store::{WarmBuilds, record_file_name, repo_name, times_file_name};
use crate::data_dir::DataDir;
use crate::domain::TicketKey;
use crate::store::projects::Project;

/// How many tickets' use a warm build carries (decision 054's number).
pub const RECENT_TICKETS: usize = 5;

/// The first entry of a ledger: its format.
const USAGE_FORMAT: &str = "usage 1";

/// What a hand-on or a checkout copy did with the build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trimmed {
    /// Trimmed to the keep set: `kept` files stay (armed), `dropped` went to the trash.
    Left { kept: usize, dropped: usize },
    /// Left whole: reads aren't recorded on this disk, nothing is recorded yet (a copy), or something failed (logged).
    Whole,
}

// ---------------------------------------------------------------------------------------------------------------
// The ledger
// ---------------------------------------------------------------------------------------------------------------

/// The paths the last [`RECENT_TICKETS`] tickets used, in `warm/<repo>.usage`. NUL-separated entries: the format
/// ([`USAGE_FORMAT`]), the tickets newest first (space-separated), then one `<mask> <path>` per path: relative to the
/// worktree, `/`-separated, a file or symlink; bit *i* of the mask is the *i*-th ticket. A path whose mask is 0 isn't
/// written.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ledger {
    tickets: Vec<String>,
    paths: BTreeMap<String, u32>,
}

impl Ledger {
    /// The tickets, newest first.
    pub fn tickets(&self) -> &[String] {
        &self.tickets
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Whether any of the recent tickets used `path`: the keep set is every path in the ledger.
    pub fn keeps(&self, path: &str) -> bool {
        self.paths.contains_key(path)
    }

    /// The keep set's size.
    pub fn len(&self) -> usize {
        self.paths.len()
    }

    /// Records that `ticket` used `used`. A ticket already listed moves to the front and its earlier paths stay
    /// with it; a new one goes in front, and the one past [`RECENT_TICKETS`] drops out with its bit (and the paths
    /// only it used).
    pub fn record(&mut self, ticket: &str, used: impl IntoIterator<Item = String>) {
        match self.tickets.iter().position(|t| t == ticket) {
            Some(p) => {
                let t = self.tickets.remove(p);
                self.tickets.insert(0, t);
                let below = (1u32 << p) - 1;
                for mask in self.paths.values_mut() {
                    let own = (*mask >> p) & 1;
                    *mask = (*mask & !(below | 1 << p)) | ((*mask & below) << 1) | own;
                }
            }
            None => {
                self.tickets.insert(0, ticket.to_owned());
                self.tickets.truncate(RECENT_TICKETS);
                let all = (1u32 << RECENT_TICKETS) - 1;
                for mask in self.paths.values_mut() {
                    *mask = (*mask << 1) & all;
                }
            }
        }
        for path in used {
            *self.paths.entry(path).or_insert(0) |= 1;
        }
        self.paths.retain(|_, mask| *mask != 0);
    }

    /// Parses a ledger; `None` for an unknown format or a damaged file.
    fn parse(bytes: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(bytes).ok()?;
        let mut entries = text.split('\0');
        if entries.next() != Some(USAGE_FORMAT) {
            return None;
        }
        let tickets: Vec<String> = entries
            .next()?
            .split(' ')
            .filter(|t| !t.is_empty())
            .map(str::to_owned)
            .collect();
        if tickets.len() > RECENT_TICKETS {
            return None;
        }
        let all = (1u32 << tickets.len()) - 1;
        let mut paths = BTreeMap::new();
        for entry in entries.filter(|e| !e.is_empty()) {
            let (mask, path) = entry.split_once(' ')?;
            let mask: u32 = mask.parse().ok()?;
            if mask == 0 || mask & !all != 0 || path.is_empty() {
                return None;
            }
            paths.insert(path.to_owned(), mask);
        }
        Some(Self { tickets, paths })
    }

    fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(16 + self.paths.len() * 64);
        out.extend_from_slice(USAGE_FORMAT.as_bytes());
        out.push(0);
        out.extend_from_slice(self.tickets.join(" ").as_bytes());
        for (path, mask) in &self.paths {
            out.push(0);
            out.extend_from_slice(format!("{mask} {path}").as_bytes());
        }
        out
    }

    /// Reads the ledger at `path`. Missing, damaged or of an unknown format: an empty one (logged when it was there).
    pub fn read(path: &Path) -> Self {
        match fs::read(path) {
            Ok(bytes) => Self::parse(&bytes).unwrap_or_else(|| {
                tracing::warn!(ledger = %path.display(), "the usage ledger can't be read; it starts empty");
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    /// Writes the ledger at `path` atomically: `<path>.tmp`, then renamed over it. The caller holds the stock's
    /// `.incoming` lock, so two writers never lose each other's record.
    pub fn write(&self, path: &Path) -> io::Result<()> {
        let tmp = tmp_path(path);
        if let Some(parent) = path.parent() {
            DataDir::ensure_dir(parent)?;
        }
        fs::write(&tmp, self.to_bytes())?;
        fs::rename(&tmp, path).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
}

/// `<repo>.usage.tmp` for `<repo>.usage`.
fn tmp_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".tmp");
    path.with_file_name(name)
}

// ---------------------------------------------------------------------------------------------------------------
// Measuring and arming
// ---------------------------------------------------------------------------------------------------------------

/// When the worktree's build began: its `.git` file's modified time (written once by `git worktree add`). `None`
/// when it has none: then every file counts as used.
pub fn since(worktree: &Path) -> Option<SystemTime> {
    fs::symlink_metadata(worktree.join(".git"))
        .and_then(|m| m.modified())
        .ok()
}

/// Whether a file with metadata `meta` was used since `since`: written or read at or after it. Always without a
/// `since`.
fn was_used(meta: &fs::Metadata, since: Option<SystemTime>) -> bool {
    let Some(since) = since else {
        return true;
    };
    let at_or_after = |t: io::Result<SystemTime>| t.is_ok_and(|t| t >= since);
    at_or_after(meta.modified()) || at_or_after(meta.accessed())
}

/// The access time [`arm`] gives a file modified at `modified`: `min(modified − 1 s, now − 2 h)`.
fn armed_time(modified: SystemTime, now: SystemTime) -> SystemTime {
    let a = modified
        .checked_sub(Duration::from_secs(1))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    let b = now
        .checked_sub(Duration::from_secs(2 * 60 * 60))
        .unwrap_or(SystemTime::UNIX_EPOCH);
    a.min(b)
}

/// Arms `path` (a file or a symlink itself, never followed): its access time becomes `min(mtime − 1 s, now − 2 h)`,
/// so the next read moves it; its modified time is untouched.
pub fn arm(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    let atime = armed_time(meta.modified()?, SystemTime::now());
    platform::set_accessed(path, &meta, atime)
}

#[cfg(unix)]
mod platform {
    use std::fs;
    use std::io;
    use std::path::Path;
    use std::time::SystemTime;

    /// Sets the access time of `path` itself (a symlink is not followed), the modified time left as it is.
    #[allow(clippy::unnecessary_cast)]
    pub(super) fn set_accessed(
        path: &Path,
        _meta: &fs::Metadata,
        atime: SystemTime,
    ) -> io::Result<()> {
        use std::os::unix::ffi::OsStrExt;
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "a null character in the path")
        })?;
        let since = atime
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        // SAFETY: `timespec` is plain data; every field the call reads is set below.
        let mut times: [libc::timespec; 2] = unsafe { std::mem::zeroed() };
        times[0].tv_sec = since.as_secs() as libc::time_t;
        times[0].tv_nsec = since.subsec_nanos() as _;
        times[1].tv_nsec = libc::UTIME_OMIT as _;
        // SAFETY: `c` is a valid null-terminated path and `times` holds two entries.
        let done = unsafe {
            libc::utimensat(
                libc::AT_FDCWD,
                c.as_ptr(),
                times.as_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        };
        if done == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(windows)]
mod platform {
    use std::fs;
    use std::io;
    use std::os::windows::fs::OpenOptionsExt;
    use std::path::Path;
    use std::time::SystemTime;

    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_WRITE_ATTRIBUTES,
    };

    /// Sets the access time of `path` itself (a link is not followed), the modified time left as it is.
    pub(super) fn set_accessed(
        path: &Path,
        meta: &fs::Metadata,
        atime: SystemTime,
    ) -> io::Result<()> {
        let mut flags = FILE_FLAG_OPEN_REPARSE_POINT;
        if meta.is_dir() {
            flags |= FILE_FLAG_BACKUP_SEMANTICS;
        }
        let file = fs::OpenOptions::new()
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(flags)
            .open(path)?;
        file.set_times(fs::FileTimes::new().set_accessed(atime))
    }
}

#[cfg(not(any(unix, windows)))]
mod platform {
    use std::fs;
    use std::io;
    use std::path::Path;
    use std::time::SystemTime;

    pub(super) fn set_accessed(
        _path: &Path,
        _meta: &fs::Metadata,
        _atime: SystemTime,
    ) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

// ---------------------------------------------------------------------------------------------------------------
// The read probe
// ---------------------------------------------------------------------------------------------------------------

/// Whether the data folder's disk records reads, kept for the life of the service: probed once (at service start,
/// next to the clone probe) and then reused. Tests build it [`forced`](Self::forced) to an answer.
#[derive(Debug, Default)]
pub struct ReadProbe {
    kept: OnceLock<bool>,
}

impl ReadProbe {
    /// Probes on the first [`recorded`](Self::recorded) call.
    pub fn new() -> Self {
        Self::default()
    }

    /// Answers `recorded` without probing.
    pub fn forced(recorded: bool) -> Self {
        let kept = OnceLock::new();
        let _ = kept.set(recorded);
        Self { kept }
    }

    /// The kept answer, probing `data` the first time (logged once when reads aren't recorded).
    pub fn recorded(&self, data: &DataDir) -> bool {
        *self.kept.get_or_init(|| {
            let recorded = probe(data);
            if !recorded {
                tracing::info!(
                    "the data folder's disk doesn't record reads; warm builds are kept whole"
                );
            }
            recorded
        })
    }
}

/// Learns whether the data folder's disk records reads: writes a file in `<app-data>/warm/.probe`, arms it, reads
/// it and checks its access time moved. Any failure answers no. The probe file is removed (not the folder, which
/// the clone probe owns).
pub fn probe(data: &DataDir) -> bool {
    let dir = data.warm_dir().join(".probe");
    let file = dir.join("reads");
    let recorded = (|| -> io::Result<bool> {
        DataDir::ensure_dir(&dir)?;
        fs::write(&file, b"read probe\n")?;
        arm(&file)?;
        let before = fs::symlink_metadata(&file)?.accessed()?;
        fs::read(&file)?;
        let after = fs::symlink_metadata(&file)?.accessed()?;
        Ok(after > before)
    })()
    .unwrap_or(false);
    let _ = fs::remove_file(&file);
    let _ = fs::remove_dir(&dir);
    recorded
}

// ---------------------------------------------------------------------------------------------------------------
// Applying it
// ---------------------------------------------------------------------------------------------------------------

/// A file or folder of a build, as measured.
#[derive(Debug)]
enum Node {
    /// A file or a symlink (by its own times): whether it was used.
    File {
        name: String,
        used: bool,
    },
    Folder {
        name: String,
        children: Vec<Node>,
    },
}

impl Node {
    fn name(&self) -> &str {
        match self {
            Self::File { name, .. } | Self::Folder { name, .. } => name,
        }
    }
}

/// Measures `path` (named `name`): a file or symlink, or a folder and everything in it. A name that isn't UTF-8 is
/// taken lossily (two such names that read the same only keep more).
fn measure(path: &Path, name: String, since: Option<SystemTime>) -> io::Result<Node> {
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() {
        return Ok(Node::File {
            name,
            used: was_used(&meta, since),
        });
    }
    let mut children = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        children.push(measure(&entry.path(), name, since)?);
    }
    Ok(Node::Folder { name, children })
}

/// `prefix/name`, or `name` at the top.
fn joined(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}/{name}")
    }
}

/// The paths of the used files under `node`.
fn used_paths(node: &Node, prefix: &str, out: &mut Vec<String>) {
    let path = joined(prefix, node.name());
    match node {
        Node::File { used: true, .. } => out.push(path),
        Node::File { .. } => {}
        Node::Folder { children, .. } => {
            for child in children {
                used_paths(child, &path, out);
            }
        }
    }
}

/// What to do with one path of a build.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    /// Kept: arm it.
    Arm(String),
    /// Left out: to the trash, with the number of files it holds.
    Drop(String, usize),
}

/// Decides `node` against the keep set: a kept file is armed, any other file dropped, a folder with no kept file
/// dropped as one. Returns (kept files, files).
fn decide(
    node: &Node,
    prefix: &str,
    keeps: &dyn Fn(&str) -> bool,
    out: &mut Vec<Action>,
) -> (usize, usize) {
    let path = joined(prefix, node.name());
    match node {
        Node::File { .. } if keeps(&path) => {
            out.push(Action::Arm(path));
            (1, 1)
        }
        Node::File { .. } => {
            out.push(Action::Drop(path, 1));
            (0, 1)
        }
        Node::Folder { children, .. } => {
            let mut inner = Vec::new();
            let (mut kept, mut files) = (0, 0);
            for child in children {
                let (k, f) = decide(child, &path, keeps, &mut inner);
                kept += k;
                files += f;
            }
            if kept == 0 {
                out.push(Action::Drop(path, files));
            } else {
                out.extend(inner);
            }
            (kept, files)
        }
    }
}

/// Carries out `actions` under `root`: dropped paths renamed into the stock's trash, kept ones armed. A path that
/// can't be moved stays (and is armed); one that can't be armed is logged. Returns (kept, dropped) files.
fn apply(warm: &WarmBuilds, repo: &str, root: &Path, actions: &[Action]) -> (usize, usize) {
    let (mut kept, mut dropped, mut unarmed) = (0, 0, 0);
    let arm_logged = |path: &Path, kept: &mut usize, unarmed: &mut usize| {
        *kept += 1;
        if let Err(e) = arm(path) {
            *unarmed += 1;
            if *unarmed == 1 {
                tracing::warn!(path = %path.display(), error = %e, "can't arm a kept file");
            }
        }
    };
    for action in actions {
        match action {
            Action::Arm(rel) => arm_logged(&root.join(rel), &mut kept, &mut unarmed),
            Action::Drop(rel, files) => {
                let path = root.join(rel);
                match warm.to_trash(repo, &path) {
                    Ok(()) => dropped += files,
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "can't move a file nothing recent used to the trash; it stays");
                        kept += files;
                    }
                }
            }
        }
    }
    if unarmed > 1 {
        tracing::warn!(unarmed, "kept files couldn't be armed");
    }
    (kept, dropped)
}

/// The record and times files at the top of a build: never measured, left out or armed.
fn own_files() -> HashSet<String> {
    [record_file_name(), times_file_name()]
        .into_iter()
        .collect()
}

/// Measures what `ticket` used in the build at `incoming` (its files written or read at or after `since`), records
/// it in the project's ledger, moves the files no recent ticket used to the stock's trash and arms the rest. Called
/// with the stock's `.incoming` lock held (the [`Incoming`](super::store::Incoming) for `incoming`), before the
/// build's record is written and the stock takes it. [`Trimmed::Whole`] when reads aren't recorded on this disk or
/// anything failed before trimming (logged): never an error, so a Free or a Done never fails because of it.
pub async fn hand_on(
    warm: &WarmBuilds,
    project: &Project,
    ticket: &TicketKey,
    incoming: &Path,
    since: Option<SystemTime>,
) -> Trimmed {
    let (warm, ticket, incoming) = (warm.clone(), ticket.to_string(), incoming.to_owned());
    let (repo, ledger_path) = (repo_name(project), warm.usage_path(project));
    tokio::task::spawn_blocking(move || {
        if !warm.reads_recorded() {
            return Trimmed::Whole;
        }
        let ledger_path = match ledger_path {
            Ok(path) => path,
            Err(e) => {
                tracing::warn!(ticket, error = %e, "no usage ledger for this project; the build is kept whole");
                return Trimmed::Whole;
            }
        };
        hand_on_blocking(&warm, &repo, &ticket, &incoming, since, &ledger_path)
    })
    .await
    .unwrap_or(Trimmed::Whole)
}

fn hand_on_blocking(
    warm: &WarmBuilds,
    repo: &str,
    ticket: &str,
    incoming: &Path,
    since: Option<SystemTime>,
    ledger_path: &Path,
) -> Trimmed {
    let own = own_files();
    let measured = (|| -> io::Result<Vec<Node>> {
        let mut nodes = Vec::new();
        for entry in fs::read_dir(incoming)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if !own.contains(&name) {
                nodes.push(measure(&entry.path(), name, since)?);
            }
        }
        Ok(nodes)
    })();
    let nodes = match measured {
        Ok(nodes) => nodes,
        Err(e) => {
            tracing::warn!(ticket, error = %e, "can't measure the build; it is kept whole");
            return Trimmed::Whole;
        }
    };
    let mut used = Vec::new();
    for node in &nodes {
        used_paths(node, "", &mut used);
    }
    let mut ledger = Ledger::read(ledger_path);
    ledger.record(ticket, used);
    if let Err(e) = ledger.write(ledger_path) {
        tracing::warn!(ticket, ledger = %ledger_path.display(), error = %e, "can't write the usage ledger; the build is kept whole");
        return Trimmed::Whole;
    }
    let mut actions = Vec::new();
    for node in &nodes {
        decide(node, "", &|p| ledger.keeps(p), &mut actions);
    }
    let (kept, dropped) = apply(warm, repo, incoming, &actions);
    tracing::debug!(
        ticket,
        kept,
        dropped,
        "kept {kept} files, left out {dropped} nothing used in the last {RECENT_TICKETS} tickets"
    );
    Trimmed::Left { kept, dropped }
}

/// Trims a worktree's copy of the user's checkout (decision 055): of the `cloned` paths (relative to `worktree`),
/// the files no recent ticket used are moved to the stock's trash and the rest armed. The checkout itself is never
/// touched. With an empty ledger the copy stays whole (and is armed); with reads not recorded it is left as it is.
/// Never an error.
pub async fn trim_copy(
    warm: &WarmBuilds,
    project: &Project,
    worktree: &Path,
    cloned: &[PathBuf],
) -> Trimmed {
    let (warm, worktree, cloned) = (warm.clone(), worktree.to_owned(), cloned.to_vec());
    let (repo, ledger_path) = (repo_name(project), warm.usage_path(project));
    tokio::task::spawn_blocking(move || {
        if !warm.reads_recorded() {
            return Trimmed::Whole;
        }
        let ledger = ledger_path.map(|p| Ledger::read(&p)).unwrap_or_default();
        trim_copy_blocking(&warm, &repo, &worktree, &cloned, &ledger)
    })
    .await
    .unwrap_or(Trimmed::Whole)
}

fn trim_copy_blocking(
    warm: &WarmBuilds,
    repo: &str,
    worktree: &Path,
    cloned: &[PathBuf],
    ledger: &Ledger,
) -> Trimmed {
    let mut actions = Vec::new();
    let whole = ledger.is_empty();
    let keeps = |p: &str| whole || ledger.keeps(p);
    for rel in cloned {
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let Some((name, parents)) = parts.split_last() else {
            continue;
        };
        let node = match measure(&worktree.join(rel), name.clone(), None) {
            Ok(node) => node,
            Err(e) => {
                tracing::warn!(path = %rel.display(), error = %e, "can't read a copied path; it is kept");
                continue;
            }
        };
        decide(&node, &parents.join("/"), &keeps, &mut actions);
    }
    let (kept, dropped) = apply(warm, repo, worktree, &actions);
    if whole {
        tracing::debug!(
            kept,
            "no recent ticket recorded yet; the checkout's copy is whole"
        );
        return Trimmed::Whole;
    }
    tracing::debug!(
        kept,
        dropped,
        "kept {kept} files, left out {dropped} nothing used in the last {RECENT_TICKETS} tickets"
    );
    Trimmed::Left { kept, dropped }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn keep_set(ledger: &Ledger) -> Vec<&str> {
        ledger.paths.keys().map(String::as_str).collect()
    }

    #[test]
    fn the_ledger_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("shop.usage");
        assert_eq!(
            Ledger::read(&path),
            Ledger::default(),
            "missing reads as empty"
        );
        let mut ledger = Ledger::default();
        ledger.record("SHOP-1", paths(&["target/a", "dir with space/b c"]));
        ledger.record("SHOP-2", paths(&["target/a", "target/d"]));
        ledger.write(&path).unwrap();
        assert!(!tmp_path(&path).exists(), "the temporary file is renamed");
        assert_eq!(tmp_path(&path), tmp.path().join("shop.usage.tmp"));
        let read = Ledger::read(&path);
        assert_eq!(read, ledger);
        assert_eq!(read.tickets(), ["SHOP-2", "SHOP-1"]);
        assert_eq!(read.paths["target/a"], 0b11);
        assert_eq!(read.paths["target/d"], 0b01);
        assert_eq!(read.paths["dir with space/b c"], 0b10);
        let bytes = fs::read(&path).unwrap();
        assert!(bytes.starts_with(b"usage 1\0SHOP-2 SHOP-1\0"));
    }

    #[test]
    fn a_sixth_ticket_drops_the_oldest_and_the_paths_only_it_used() {
        let mut ledger = Ledger::default();
        ledger.record("T-1", paths(&["only-1", "shared"]));
        for n in 2..=5 {
            ledger.record(&format!("T-{n}"), paths(&["shared", &format!("only-{n}")]));
        }
        assert_eq!(ledger.tickets().len(), 5);
        assert!(ledger.keeps("only-1"));
        ledger.record("T-6", paths(&["only-6"]));
        assert_eq!(ledger.tickets(), ["T-6", "T-5", "T-4", "T-3", "T-2"]);
        assert!(!ledger.keeps("only-1"), "only T-1 used it");
        assert!(ledger.keeps("shared") && ledger.keeps("only-2") && ledger.keeps("only-6"));
        assert_eq!(ledger.paths["shared"], 0b11110);
        assert_eq!(ledger.paths["only-2"], 0b10000);
    }

    #[test]
    fn a_returning_ticket_moves_to_the_front_and_keeps_its_paths() {
        let mut ledger = Ledger::default();
        for n in 1..=5 {
            ledger.record(&format!("T-{n}"), paths(&[&format!("only-{n}")]));
        }
        // T-2 is sent back and handed on again: it is the newest now, with what it used both times.
        ledger.record("T-2", paths(&["again-2"]));
        assert_eq!(ledger.tickets(), ["T-2", "T-5", "T-4", "T-3", "T-1"]);
        assert_eq!(ledger.paths["only-2"], 0b00001);
        assert_eq!(ledger.paths["again-2"], 0b00001);
        assert_eq!(ledger.paths["only-5"], 0b00010);
        assert_eq!(ledger.paths["only-3"], 0b01000);
        assert_eq!(ledger.paths["only-1"], 0b10000);
        // A new ticket now drops T-1, the oldest, not T-2.
        ledger.record("T-6", Vec::new());
        assert!(!ledger.keeps("only-1"));
        assert!(ledger.keeps("only-2") && ledger.keeps("again-2"));
    }

    #[test]
    fn fewer_than_five_tickets_all_count() {
        let mut ledger = Ledger::default();
        ledger.record("T-1", paths(&["a"]));
        ledger.record("T-2", paths(&["b"]));
        assert_eq!(keep_set(&ledger), ["a", "b"]);
        // A ticket that used nothing still takes a place.
        ledger.record("T-3", Vec::new());
        assert_eq!(ledger.tickets().len(), 3);
        assert_eq!(keep_set(&ledger), ["a", "b"]);
    }

    #[test]
    fn a_damaged_or_unknown_ledger_reads_as_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("shop.usage");
        for bad in [
            &b"usage 2\0T-1\x001 a"[..],
            b"usage 1\0T-1\0 a",
            b"usage 1\0T-1\x002 a",
            b"usage 1\0T-1\x000 a",
            b"usage 1\0T-1\x001a",
            b"usage 1\0T-1 T-2 T-3 T-4 T-5 T-6\x001 a",
            b"usage 1",
            b"\xff\xfe",
            b"",
        ] {
            fs::write(&path, bad).unwrap();
            assert_eq!(Ledger::read(&path), Ledger::default(), "{bad:?}");
        }
    }

    fn set_times(path: &Path, accessed: SystemTime, modified: SystemTime) {
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(
                fs::FileTimes::new()
                    .set_accessed(accessed)
                    .set_modified(modified),
            )
            .unwrap();
    }

    #[test]
    fn measuring_sees_writes_and_reads_since_the_start() {
        let tmp = tempfile::tempdir().unwrap();
        let since = SystemTime::now() - Duration::from_secs(3600);
        let (before, after) = (
            since - Duration::from_secs(60),
            since + Duration::from_secs(60),
        );
        for (name, accessed, modified) in [
            ("written", before, after),
            ("read", after, before),
            ("neither", before, before),
            ("at-since", before, since),
        ] {
            let path = tmp.path().join(name);
            fs::write(&path, name).unwrap();
            set_times(&path, accessed, modified);
        }
        let mut used = Vec::new();
        used_paths(
            &measure(tmp.path(), String::new(), Some(since)).unwrap(),
            "",
            &mut used,
        );
        used.sort();
        let used: Vec<&str> = used.iter().map(|p| p.trim_start_matches('/')).collect();
        assert_eq!(used, ["at-since", "read", "written"]);
        // No `since`: everything counts.
        let mut all = Vec::new();
        used_paths(
            &measure(tmp.path(), String::new(), None).unwrap(),
            "",
            &mut all,
        );
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn since_is_the_git_files_time() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(since(tmp.path()), None);
        fs::write(tmp.path().join(".git"), "gitdir: /x").unwrap();
        let time = SystemTime::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        set_times(&tmp.path().join(".git"), time, time);
        assert_eq!(since(tmp.path()), Some(time));
    }

    #[test]
    fn arming_puts_the_access_time_below_both_and_keeps_the_modified_time() {
        let tmp = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        for (name, modified) in [
            ("fresh", now - Duration::from_secs(5)),
            ("old", now - Duration::from_secs(10 * 3600)),
        ] {
            let path = tmp.path().join(name);
            fs::write(&path, name).unwrap();
            set_times(&path, now, modified);
            arm(&path).unwrap();
            let meta = fs::metadata(&path).unwrap();
            let (atime, mtime) = (meta.accessed().unwrap(), meta.modified().unwrap());
            assert_eq!(mtime, modified, "{name}: the modified time stays");
            assert!(atime < mtime, "{name}: below the modified time");
            let after = SystemTime::now();
            assert!(
                atime <= after - Duration::from_secs(2 * 3600),
                "{name}: below now - 2 h"
            );
            // To the second: the larger allowed time, not just any older one.
            let expected = armed_time(modified, now);
            let gap = atime
                .max(expected)
                .duration_since(atime.min(expected))
                .unwrap();
            assert!(gap < Duration::from_secs(5), "{name}: {gap:?}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn arming_a_symlink_never_touches_its_target() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        fs::write(&target, "t").unwrap();
        let now = SystemTime::now();
        set_times(&target, now, now);
        let link = tmp.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        arm(&link).unwrap();
        let target_meta = fs::metadata(&target).unwrap();
        assert!(target_meta.accessed().unwrap() >= now - Duration::from_secs(1));
        let link_meta = fs::symlink_metadata(&link).unwrap();
        assert!(link_meta.accessed().unwrap() < link_meta.modified().unwrap());
    }

    #[test]
    fn a_folder_with_no_kept_file_is_dropped_as_one() {
        let tree = Node::Folder {
            name: "target".into(),
            children: vec![
                Node::Folder {
                    name: "deps".into(),
                    children: vec![
                        Node::File {
                            name: "new.rlib".into(),
                            used: true,
                        },
                        Node::File {
                            name: "old.rlib".into(),
                            used: false,
                        },
                    ],
                },
                Node::Folder {
                    name: "incremental".into(),
                    children: vec![
                        Node::File {
                            name: "a".into(),
                            used: false,
                        },
                        Node::Folder {
                            name: "s".into(),
                            children: vec![Node::File {
                                name: "b".into(),
                                used: false,
                            }],
                        },
                    ],
                },
                Node::Folder {
                    name: "empty".into(),
                    children: vec![],
                },
            ],
        };
        let mut actions = Vec::new();
        let keeps = |p: &str| p == "target/deps/new.rlib";
        assert_eq!(decide(&tree, "", &keeps, &mut actions), (1, 4));
        assert_eq!(
            actions,
            [
                Action::Arm("target/deps/new.rlib".into()),
                Action::Drop("target/deps/old.rlib".into(), 1),
                Action::Drop("target/incremental".into(), 2),
                Action::Drop("target/empty".into(), 0),
            ]
        );
    }

    #[test]
    fn the_probe_answers_on_this_disk_and_leaves_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().to_owned());
        let recorded = probe(&data);
        // macOS (APFS) and Linux (relatime) record a read of an armed file; Windows depends on the volume.
        #[cfg(any(target_os = "macos", target_os = "linux"))]
        assert!(recorded, "an armed file's read is recorded here");
        let _ = recorded;
        assert!(!data.warm_dir().join(".probe").exists());
        assert!(!ReadProbe::forced(false).recorded(&data));
        assert!(ReadProbe::forced(true).recorded(&data));
    }

    // -- hand_on and trim_copy on temp folders -----------------------------------------------------------------

    struct World {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        data: DataDir,
        project: Project,
    }

    async fn world(reads: bool) -> (World, WarmBuilds) {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_owned();
        let data = DataDir::new(root.join("data"));
        let store = crate::store::Store::open(&root.join("otaq.db"))
            .await
            .unwrap();
        let board = crate::board::Board::new(store, None, data.clone());
        let warm = WarmBuilds::with_probes(
            data.clone(),
            std::sync::Arc::new(super::super::clone::CloneProbe::forced(
                super::super::clone::CloneSupport {
                    available: true,
                    file_system: "apfs".into(),
                    volume: "/".into(),
                },
            )),
            std::sync::Arc::new(ReadProbe::forced(reads)),
            board,
            crate::service::EventBus::new(16),
        );
        let project = Project {
            id: "p1".into(),
            key: "SHOP".into(),
            name: "Shop".into(),
            repo_path: root.join("shop").to_string_lossy().into_owned(),
            base_branch: "main".into(),
            next_ticket_number: 1,
            settings_json: "{}".into(),
            created_at: 0,
            paused_since: None,
            paused_by: None,
        };
        (
            World {
                _tmp: tmp,
                root,
                data,
                project,
            },
            warm,
        )
    }

    /// Writes `files` under `dir` with times older than `since` (unused) unless listed in `used`.
    fn build(dir: &Path, files: &[&str], used: &[&str], since: SystemTime) {
        let old = since - Duration::from_secs(3600);
        let new = since + Duration::from_secs(10);
        for f in files {
            let path = dir.join(f);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, f).unwrap();
            let t = if used.contains(f) { new } else { old };
            set_times(&path, old, t);
        }
    }

    fn files_under(dir: &Path) -> Vec<String> {
        let mut out = Vec::new();
        if let Ok(node) = measure(dir, String::new(), None) {
            used_paths(&node, "", &mut out);
        }
        let mut out: Vec<String> = out
            .into_iter()
            .map(|p| p.trim_start_matches('/').to_owned())
            .collect();
        out.sort();
        out
    }

    fn key(n: u32) -> TicketKey {
        format!("SHOP-{n}").parse().unwrap()
    }

    #[tokio::test]
    async fn hand_on_keeps_what_recent_tickets_used_and_trashes_the_rest() {
        let (w, warm) = world(true).await;
        let since = SystemTime::now() - Duration::from_secs(600);
        let incoming = w.root.join("incoming");
        let all = [
            "target/deps/a.rlib",
            "target/deps/b.rlib",
            "target/incremental/x/s",
            "out.log",
        ];
        // SHOP-1 used a.rlib; SHOP-2 used b.rlib.
        build(&incoming, &all, &["target/deps/a.rlib"], since);
        fs::write(incoming.join(record_file_name()), "{}").unwrap();
        fs::write(incoming.join(times_file_name()), "times 1").unwrap();
        let first = hand_on(&warm, &w.project, &key(1), &incoming, Some(since)).await;
        assert_eq!(
            first,
            Trimmed::Left {
                kept: 1,
                dropped: 3
            }
        );
        let mut expected = vec![
            record_file_name(),
            times_file_name(),
            "target/deps/a.rlib".to_owned(),
        ];
        expected.sort();
        assert_eq!(files_under(&incoming), expected, "Otaq's own files stay");
        let kept = fs::metadata(incoming.join("target/deps/a.rlib")).unwrap();
        assert!(
            kept.accessed().unwrap() < kept.modified().unwrap(),
            "kept files are armed"
        );
        assert!(
            !incoming.join("target/incremental").exists(),
            "a folder with nothing kept goes whole"
        );

        // The next ticket's build: a.rlib unused this time, b.rlib written. a.rlib stays (SHOP-1 is recent).
        fs::remove_dir_all(&incoming).unwrap();
        build(&incoming, &all, &["target/deps/b.rlib"], since);
        let second = hand_on(&warm, &w.project, &key(2), &incoming, Some(since)).await;
        assert_eq!(
            second,
            Trimmed::Left {
                kept: 2,
                dropped: 2
            }
        );
        assert_eq!(
            files_under(&incoming),
            ["target/deps/a.rlib", "target/deps/b.rlib"]
        );
        let ledger = Ledger::read(&warm.usage_path(&w.project).unwrap());
        assert_eq!(ledger.tickets(), ["SHOP-2", "SHOP-1"]);
        assert!(
            w.data.warm_trash_path("shop").unwrap().is_dir(),
            "dropped files went to the trash"
        );
    }

    #[tokio::test]
    async fn hand_on_keeps_the_build_whole_when_reads_arent_recorded() {
        let (w, warm) = world(false).await;
        let since = SystemTime::now() - Duration::from_secs(600);
        let incoming = w.root.join("incoming");
        build(&incoming, &["a", "b/c"], &[], since);
        assert_eq!(
            hand_on(&warm, &w.project, &key(1), &incoming, Some(since)).await,
            Trimmed::Whole
        );
        assert_eq!(files_under(&incoming), ["a", "b/c"]);
        assert!(
            !warm.usage_path(&w.project).unwrap().exists(),
            "no ledger is written"
        );
    }

    #[tokio::test]
    async fn trim_copy_trims_only_the_copy_and_is_whole_with_an_empty_ledger() {
        let (w, warm) = world(true).await;
        let since = SystemTime::now() - Duration::from_secs(600);
        let worktree = w.root.join("wt");
        build(
            &worktree,
            &["target/a", "target/b", "node_modules/m/x.js", "src/main.rs"],
            &[],
            since,
        );
        let cloned = [PathBuf::from("target"), PathBuf::from("node_modules")];
        // Empty ledger: whole, but armed.
        assert_eq!(
            trim_copy(&warm, &w.project, &worktree, &cloned).await,
            Trimmed::Whole
        );
        assert_eq!(files_under(&worktree).len(), 4);
        let a = fs::metadata(worktree.join("target/a")).unwrap();
        assert!(a.accessed().unwrap() < a.modified().unwrap());
        // A ledger that keeps target/a only.
        let mut ledger = Ledger::default();
        ledger.record("SHOP-1", paths(&["target/a"]));
        fs::create_dir_all(w.data.warm_dir()).unwrap();
        ledger.write(&warm.usage_path(&w.project).unwrap()).unwrap();
        let trimmed = trim_copy(&warm, &w.project, &worktree, &cloned).await;
        assert_eq!(
            trimmed,
            Trimmed::Left {
                kept: 1,
                dropped: 2
            }
        );
        assert_eq!(
            files_under(&worktree),
            ["src/main.rs", "target/a"],
            "tracked files are never touched"
        );
    }
}
