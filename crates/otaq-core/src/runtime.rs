//! The runtime copy (UPD-02, d0140; gap review C1, G1, G7, R4): each build's `otaq` and plugin folder, copied into
//! `<data folder>/runtime/<build>/`, so replacing the installed app never changes what a running agent runs. Example:
//! SHOP-12's agent runs `otaq comment` after Restart now and still gets the `otaq` of the build its service runs.
//!
//! - **The folder** is named by the build ID ([`crate::protocol::build_id`], made file-name safe) and a short stamp of
//!   the copied files (their sizes and times): the build ID changes only when the core is rebuilt, so a rebuild of the
//!   command or the plugin alone still gets a folder of its own. Never by version: two local builds of 0.1.0 get two
//!   folders. A folder there already is reused.
//! - **Never half-written:** the copy is made under a temporary name (`.<name>.tmp-<pid>`) and renamed into place. Two
//!   starts racing for the same build both end up with the complete folder: the second rename fails, finds the
//!   folder complete and uses it.
//! - **Where the service runs** ([`service_runs_from_copy`]): on Windows and Linux the copy is made before the service
//!   starts (by `otaq daemon start`, which the app runs too) and the service runs from it, so the installed
//!   `otaq.exe` is never locked and an AppImage's mount doesn't matter. On macOS the service keeps running from
//!   `Otaq.app` (notifications post only from the bundle, d0090) and makes the copy itself at start. Either way agents
//!   and terminals get the copy: PATH's first entry, every tool's plugin folder, Claude Code's status line.
//! - **Cleanup** at service start ([`clean`]), after the service lock is taken: every other runtime folder is deleted,
//!   except a folder whose `service.pid` names a process that still runs (the service using it) and a temporary
//!   folder whose writer still runs. Only services of this data folder use its runtime folders, and the lock lets one
//!   run at a time, so in practice the only other user is a start that lost the race and is exiting. A folder that
//!   can't be deleted (a Windows lock) stays and is tried again at the next start.
//! - **Codex**: its marketplace is registered machine-wide with a folder path, so it gets one fixed folder,
//!   `runtime/codex-plugin/`, copied over ([`replace_dir`]) at every service start by its adapter
//!   ([`crate::agent_tools::refresh_fixed_plugins`]); its config never changes with the build.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::UNIX_EPOCH;

use crate::data_dir::DataDir;
use crate::naming;

/// The file a service writes into the runtime folder it uses, holding its PID.
const IN_USE_FILE: &str = "service.pid";

/// The plugin folder inside a runtime folder: `<bin>/plugin`, the first place
/// [`crate::agent_tools::claude::find_plugin_dir`] looks.
const PLUGIN_FOLDER: &str = "plugin";

/// What a temporary folder's name carries before its writer's PID: `.<name>.tmp-<pid>-<n>`.
const TMP_MARK: &str = ".tmp-";

/// Numbers this process's temporary folders, so two threads copying at once never share one.
static NEXT_TMP: AtomicU32 = AtomicU32::new(0);

/// Whether the service itself runs from the runtime copy on `os` (`std::env::consts::OS`): on Windows and Linux; on
/// macOS it stays in `Otaq.app` (d0090).
pub fn service_runs_from_copy(os: &str) -> bool {
    os != "macos"
}

/// The hand-check and test seam `OTAQ_DEV_SERVICE_FROM_COPY` (debug builds only): `1` starts the service from the
/// runtime copy, `0` from the installed program, whatever the platform.
pub fn service_from_copy_var() -> String {
    naming::env_var("DEV_SERVICE_FROM_COPY")
}

/// Whether `otaq daemon start` starts this platform's service from the runtime copy ([`service_runs_from_copy`],
/// or [`service_from_copy_var`] in a debug build).
pub fn service_runs_from_copy_here() -> bool {
    runs_from_copy(
        std::env::consts::OS,
        std::env::var(service_from_copy_var()).ok().as_deref(),
        cfg!(debug_assertions),
    )
}

fn runs_from_copy(os: &str, seam: Option<&str>, debug: bool) -> bool {
    match seam.filter(|_| debug) {
        Some("1") => true,
        Some("0") => false,
        _ => service_runs_from_copy(os),
    }
}

/// The installed app's folder, passed to a service started from the runtime copy (`OTAQ_INSTALLED_DIR`): a click on
/// a notification starts the app found there.
pub fn installed_dir_var() -> String {
    naming::env_var("INSTALLED_DIR")
}

/// The command's file name: `otaq`, `otaq.exe` on Windows.
pub fn program_name() -> String {
    format!("{}{}", naming::COMMAND_NAME, std::env::consts::EXE_SUFFIX)
}

/// One build's runtime folder: its `otaq` and its plugin folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeFolder {
    pub dir: PathBuf,
}

impl RuntimeFolder {
    /// The copied `otaq`.
    pub fn program(&self) -> PathBuf {
        self.dir.join(program_name())
    }

    /// The copied plugin folder.
    pub fn plugin_dir(&self) -> PathBuf {
        self.dir.join(PLUGIN_FOLDER)
    }

    fn complete(&self) -> bool {
        self.program().is_file() && self.plugin_dir().is_dir()
    }
}

/// A runtime folder's name: the build ID made file-name safe, then the stamp, `7fb155e9a26d.1791504000-3fa2c1d0`.
pub fn folder_name(build_id: &str, stamp: u32) -> String {
    let safe: String = build_id
        .chars()
        .enumerate()
        .map(|(i, c)| match c {
            '.' if i == 0 => '_',
            c if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') => c,
            _ => '_',
        })
        .collect();
    format!("{safe}-{stamp:08x}")
}

/// This build's runtime folder, made from the installed `otaq` in `bin_dir` and the plugin folder `plugin_dir`
/// unless it is there already. `bin_dir` itself when it already is a runtime folder (a service started from the
/// copy).
pub fn ensure(
    data: &DataDir,
    build_id: &str,
    bin_dir: &Path,
    plugin_dir: &Path,
) -> io::Result<RuntimeFolder> {
    let runtime = data.runtime_dir();
    if is_runtime_folder(bin_dir, &runtime) {
        return Ok(RuntimeFolder {
            dir: bin_dir.to_path_buf(),
        });
    }
    let program = bin_dir.join(program_name());
    let name = folder_name(build_id, stamp(&program, plugin_dir)?);
    let folder = RuntimeFolder {
        dir: runtime.join(&name),
    };
    if folder.complete() {
        return Ok(folder);
    }
    if folder.dir.exists() {
        // Not made by this code (every copy is renamed into place whole): made again.
        remove_all(&folder.dir)?;
    }
    DataDir::ensure_dir(&runtime)?;
    let tmp = runtime.join(tmp_name(&name));
    remove_all(&tmp)?;
    let copied = fs::create_dir(&tmp)
        .and_then(|()| fs::copy(&program, tmp.join(program_name())).map(drop))
        .and_then(|()| copy_dir(plugin_dir, &tmp.join(PLUGIN_FOLDER)))
        .and_then(|()| fs::rename(&tmp, &folder.dir));
    match copied {
        Ok(()) => {
            tracing::info!(folder = %folder.dir.display(), "runtime copy made");
            Ok(folder)
        }
        Err(e) => {
            let _ = remove_all(&tmp);
            // Another start of the same build renamed its copy into place first.
            if folder.complete() {
                Ok(folder)
            } else {
                Err(e)
            }
        }
    }
}

/// Records that this process's service uses `folder`, so no other service's cleanup deletes it.
pub fn mark_in_use(folder: &RuntimeFolder) -> io::Result<()> {
    fs::write(folder.dir.join(IN_USE_FILE), std::process::id().to_string())
}

/// What [`clean`] did.
#[derive(Debug, Default)]
pub struct Cleanup {
    pub removed: Vec<PathBuf>,
    /// Folders a running process still uses.
    pub in_use: Vec<PathBuf>,
    /// Folders that couldn't be deleted; tried again at the next start.
    pub failed: Vec<(PathBuf, io::Error)>,
}

/// Deletes every runtime folder except `keep`, Codex's folder, a folder a running service uses (its `service.pid`)
/// and a temporary folder whose writer still runs. Called by a service at start, holding the service lock.
pub fn clean(data: &DataDir, keep: &Path) -> Cleanup {
    let mut cleanup = Cleanup::default();
    let codex = data.codex_plugin_dir();
    let Ok(entries) = fs::read_dir(data.runtime_dir()) else {
        return cleanup;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_type().is_ok_and(|t| t.is_dir())
            || path.file_name() == keep.file_name()
            || path == codex
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let writer = match tmp_pid(&name) {
            Some(pid) => Some(pid),
            None => read_pid(&path.join(IN_USE_FILE)),
        };
        if writer.is_some_and(crate::process::alive) {
            cleanup.in_use.push(path);
            continue;
        }
        match remove_all(&path) {
            Ok(()) => cleanup.removed.push(path),
            Err(e) => cleanup.failed.push((path, e)),
        }
    }
    cleanup
}

/// Copies the folder `source` over `dest` (a fixed folder in the runtime folder, which keeps its path for every
/// build): written beside it under a temporary name and swapped in, so `dest` is never half-written.
pub fn replace_dir(source: &Path, dest: &Path) -> io::Result<()> {
    let parent = dest
        .parent()
        .ok_or_else(|| io::Error::other("a folder without a parent"))?;
    DataDir::ensure_dir(parent)?;
    let name = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = parent.join(tmp_name(&name));
    let old = parent.join(tmp_name(&format!("{name}.old")));
    remove_all(&tmp)?;
    remove_all(&old)?;
    let swapped = copy_dir(source, &tmp)
        .and_then(|()| match fs::rename(dest, &old) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        })
        .and_then(|()| fs::rename(&tmp, dest));
    let _ = remove_all(&tmp);
    if swapped.is_err() && !dest.exists() {
        let _ = fs::rename(&old, dest);
    }
    let _ = remove_all(&old);
    swapped
}

/// Everything a service does with its runtime folder at start (macOS makes the copy here; on Windows and Linux the
/// service already runs from it): the folder, marked as in use, and the other folders cleaned.
/// `None` when no copy could be made: agents then use the installed folders, as logged.
pub fn prepare(
    data: &DataDir,
    build_id: &str,
    bin_dir: &Path,
    plugin_dir: &Path,
) -> Option<RuntimeFolder> {
    let folder = match ensure(data, build_id, bin_dir, plugin_dir) {
        Ok(folder) => folder,
        Err(e) => {
            tracing::warn!(error = %e, "can't make the runtime copy; agents use the installed program");
            return None;
        }
    };
    if let Err(e) = mark_in_use(&folder) {
        tracing::warn!(error = %e, folder = %folder.dir.display(), "can't mark the runtime folder in use");
    }
    let cleanup = clean(data, &folder.dir);
    if !cleanup.removed.is_empty() || !cleanup.in_use.is_empty() {
        tracing::info!(
            removed = cleanup.removed.len(),
            in_use = cleanup.in_use.len(),
            "older runtime folders cleaned"
        );
    }
    for (path, e) in &cleanup.failed {
        tracing::warn!(error = %e, folder = %path.display(), "can't delete a runtime folder; tried again next start");
    }
    Some(folder)
}

/// Whether `bin_dir` is a folder right inside `runtime`.
fn is_runtime_folder(bin_dir: &Path, runtime: &Path) -> bool {
    let real = |p: &Path| crate::paths::canonical(p).unwrap_or_else(|_| p.to_path_buf());
    bin_dir
        .parent()
        .is_some_and(|parent| real(parent) == real(runtime))
}

fn tmp_name(name: &str) -> String {
    let n = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
    format!(".{name}{TMP_MARK}{}-{n}", std::process::id())
}

/// The writer's PID in a temporary folder's name.
fn tmp_pid(name: &str) -> Option<u32> {
    let (_, rest) = name.strip_prefix('.')?.rsplit_once(TMP_MARK)?;
    rest.split('-').next()?.parse().ok()
}

fn read_pid(path: &Path) -> Option<u32> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

/// A short stamp of what a copy holds: the program's and every plugin file's path, size and modification time.
fn stamp(program: &Path, plugin_dir: &Path) -> io::Result<u32> {
    let mut hash = Fnv::default();
    hash.file(Path::new(""), &fs::metadata(program)?);
    let mut files = Vec::new();
    list_files(plugin_dir, Path::new(""), &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));
    for (relative, meta) in &files {
        hash.file(relative, meta);
    }
    Ok(hash.finish())
}

fn list_files(
    dir: &Path,
    relative: &Path,
    out: &mut Vec<(PathBuf, fs::Metadata)>,
) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let meta = fs::metadata(entry.path())?;
        let path = relative.join(entry.file_name());
        if meta.is_dir() {
            list_files(&entry.path(), &path, out)?;
        } else {
            out.push((path, meta));
        }
    }
    Ok(())
}

/// FNV-1a, 32 bits: stable across Rust versions, unlike the standard hasher.
struct Fnv(u32);

impl Default for Fnv {
    fn default() -> Self {
        Self(0x811c_9dc5)
    }
}

impl Fnv {
    fn bytes(&mut self, bytes: &[u8]) {
        for b in bytes {
            self.0 = (self.0 ^ u32::from(*b)).wrapping_mul(0x0100_0193);
        }
    }

    fn file(&mut self, relative: &Path, meta: &fs::Metadata) {
        self.bytes(relative.to_string_lossy().as_bytes());
        self.bytes(&meta.len().to_le_bytes());
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        self.bytes(&modified.to_le_bytes());
    }

    fn finish(&self) -> u32 {
        self.0
    }
}

/// Copies the folder `from` into `to` (created), files with their permissions, links followed.
fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if fs::metadata(entry.path())?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

/// Deletes a folder and everything in it; nothing there is fine.
fn remove_all(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An installed app: `otaq` (printing `version`) and a plugin folder with Codex's plugin.
    struct Installed {
        dir: tempfile::TempDir,
    }

    impl Installed {
        fn new(version: &str) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let installed = Self { dir };
            installed.write_program(version);
            let codex = installed.plugin().join("codex").join(".codex-plugin");
            fs::create_dir_all(&codex).unwrap();
            fs::write(codex.join("plugin.json"), "{}").unwrap();
            let claude = installed.plugin().join("claude").join(".claude-plugin");
            fs::create_dir_all(&claude).unwrap();
            fs::write(claude.join("plugin.json"), "{}").unwrap();
            installed
        }

        fn bin(&self) -> PathBuf {
            self.dir.path().join("bin")
        }

        fn plugin(&self) -> PathBuf {
            self.bin().join(PLUGIN_FOLDER)
        }

        /// Replaces the installed `otaq`, as an update or a rebuild does.
        fn write_program(&self, version: &str) {
            fs::create_dir_all(self.bin()).unwrap();
            let program = self.bin().join(program_name());
            let _ = fs::remove_file(&program);
            fs::write(&program, format!("#!/bin/sh\necho {version}\n")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
            }
        }

        fn ensure(&self, data: &DataDir, build: &str) -> RuntimeFolder {
            ensure(data, build, &self.bin(), &self.plugin()).unwrap()
        }
    }

    fn data() -> (tempfile::TempDir, DataDir) {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("home"));
        (tmp, data)
    }

    fn names(data: &DataDir) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(data.runtime_dir())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// A PID that doesn't run: one past the highest the test sees, after its child ended.
    fn ended_pid() -> u32 {
        let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "true" })
            .args(if cfg!(windows) {
                &["/c", "exit"][..]
            } else {
                &[][..]
            })
            .spawn()
            .unwrap();
        let pid = child.id();
        child.wait().unwrap();
        pid
    }

    #[test]
    fn the_platform_choice() {
        for (os, seam, debug, expected) in [
            ("macos", None, true, false),
            ("linux", None, true, true),
            ("windows", None, true, true),
            ("macos", Some("1"), true, true),
            ("linux", Some("0"), true, false),
            // A release build ignores the seam.
            ("macos", Some("1"), false, false),
            ("windows", Some("0"), false, true),
            ("macos", Some("yes"), true, false),
        ] {
            assert_eq!(
                runs_from_copy(os, seam, debug),
                expected,
                "{os} {seam:?} debug={debug}"
            );
        }
        assert_eq!(
            service_from_copy_var(),
            naming::env_var("DEV_SERVICE_FROM_COPY")
        );
        assert_eq!(installed_dir_var(), naming::env_var("INSTALLED_DIR"));
    }

    #[test]
    fn folder_names_are_the_build_made_safe() {
        assert_eq!(
            folder_name("7fb155e9a26d-dirty.1791504000", 0x3fa2c1d0),
            "7fb155e9a26d-dirty.1791504000-3fa2c1d0"
        );
        assert_eq!(folder_name("a/b\\c:d e", 1), "a_b_c_d_e-00000001");
        assert_eq!(folder_name(".hidden", 2), "_hidden-00000002");
    }

    /// The copy holds `otaq` and the plugin; a second start of the same build reuses it; replacing the installed
    /// `otaq` doesn't change the copy a running agent uses.
    #[test]
    fn copy_reuse_and_a_replaced_install() {
        let (_tmp, data) = data();
        let installed = Installed::new("v1");
        let folder = installed.ensure(&data, "abc.1");
        assert!(folder.dir.starts_with(data.runtime_dir()));
        assert!(
            folder
                .dir
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("abc.1-")
        );
        assert_eq!(
            fs::read_to_string(folder.program()).unwrap(),
            "#!/bin/sh\necho v1\n"
        );
        assert!(
            folder
                .plugin_dir()
                .join("claude/.claude-plugin/plugin.json")
                .is_file()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(folder.program()).unwrap().permissions().mode();
            assert_eq!(mode & 0o111, 0o111, "still runnable");
        }

        // Reused: a file added to the copy stays.
        fs::write(folder.dir.join("marker"), "").unwrap();
        assert_eq!(installed.ensure(&data, "abc.1"), folder);
        assert!(folder.dir.join("marker").is_file());

        // The installed app is replaced while the agent runs: the copy keeps the old program.
        installed.write_program("v2");
        assert_eq!(
            fs::read_to_string(folder.program()).unwrap(),
            "#!/bin/sh\necho v1\n"
        );

        // A service already running from the copy uses it as it is.
        assert_eq!(
            ensure(&data, "other", &folder.dir, &folder.plugin_dir()).unwrap(),
            folder
        );
    }

    /// Two local builds of one version (another build ID), or a rebuild of only the command (the same build ID),
    /// get two folders; the older is deleted at the next start once no service uses it.
    #[test]
    fn two_builds_two_folders_and_the_older_is_cleaned() {
        let (_tmp, data) = data();
        let installed = Installed::new("v1");
        let first = installed.ensure(&data, "abc.1");
        let second = installed.ensure(&data, "abc.2");
        assert_ne!(first, second);
        // The same build ID with a rebuilt command (a longer file): a folder of its own.
        installed.write_program("v1 rebuilt");
        let rebuilt = installed.ensure(&data, "abc.2");
        assert_ne!(rebuilt, second);
        assert_eq!(names(&data).len(), 3);

        let cleanup = clean(&data, &rebuilt.dir);
        assert_eq!(cleanup.removed.len(), 2);
        assert!(cleanup.failed.is_empty());
        assert_eq!(
            names(&data),
            [rebuilt.dir.file_name().unwrap().to_string_lossy()]
        );
    }

    /// A half-written folder (a start that died mid-copy) is never used: the next start makes a complete copy beside
    /// it, and the cleanup deletes it. A folder another start is still writing stays.
    #[test]
    fn a_half_written_folder_is_never_used() {
        let (_tmp, data) = data();
        let installed = Installed::new("v1");
        let name = folder_name(
            "abc.1",
            stamp(&installed.bin().join(program_name()), &installed.plugin()).unwrap(),
        );
        fs::create_dir_all(data.runtime_dir()).unwrap();
        let dead = data
            .runtime_dir()
            .join(format!(".{name}{TMP_MARK}{}-0", ended_pid()));
        fs::create_dir(&dead).unwrap();
        fs::write(dead.join(program_name()), "half").unwrap();
        // This test's own PID stands for another start still copying.
        let writing = data
            .runtime_dir()
            .join(format!(".other{TMP_MARK}{}-7", std::process::id()));
        fs::create_dir(&writing).unwrap();
        // A folder with the final name but no program (never made by a rename) is made again.
        fs::create_dir_all(data.runtime_dir().join(&name)).unwrap();

        let folder = installed.ensure(&data, "abc.1");
        assert_eq!(folder.dir.file_name().unwrap().to_string_lossy(), name);
        assert_eq!(
            fs::read_to_string(folder.program()).unwrap(),
            "#!/bin/sh\necho v1\n"
        );

        let cleanup = clean(&data, &folder.dir);
        assert_eq!(cleanup.removed, [dead]);
        assert_eq!(cleanup.in_use, std::slice::from_ref(&writing));
        assert!(writing.is_dir());
    }

    /// Two starts of the same build racing: both end up with the one complete folder.
    #[test]
    fn two_starts_racing_both_get_the_complete_folder() {
        let (_tmp, data) = data();
        let installed = Installed::new("v1");
        let folders: Vec<RuntimeFolder> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..4)
                .map(|_| s.spawn(|| installed.ensure(&data, "abc.1")))
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        assert!(folders.windows(2).all(|w| w[0] == w[1]));
        assert!(folders[0].complete());
        assert_eq!(names(&data).len(), 1, "{:?}", names(&data));
    }

    /// The cleanup keeps a folder a second, still running service uses (its `service.pid`), deletes one whose
    /// service ended, and leaves Codex's folder alone.
    #[test]
    fn cleanup_keeps_a_running_services_folder() {
        let (_tmp, data) = data();
        let installed = Installed::new("v1");
        let ours = installed.ensure(&data, "new");
        let running = installed.ensure(&data, "running");
        let ended = installed.ensure(&data, "ended");
        mark_in_use(&running).unwrap();
        fs::write(ended.dir.join(IN_USE_FILE), ended_pid().to_string()).unwrap();
        replace_dir(&installed.plugin().join("codex"), &data.codex_plugin_dir()).unwrap();

        let cleanup = clean(&data, &ours.dir);
        assert_eq!(cleanup.in_use, std::slice::from_ref(&running.dir));
        assert_eq!(cleanup.removed, std::slice::from_ref(&ended.dir));
        assert!(ours.complete() && running.complete() && !ended.dir.exists());
        assert!(data.codex_plugin_dir().is_dir());
    }

    /// A folder that can't be deleted stays and is tried again at the next start.
    #[cfg(unix)]
    #[test]
    fn a_folder_that_cant_be_deleted_is_tried_again() {
        use std::os::unix::fs::PermissionsExt;
        if rustix::process::geteuid().is_root() {
            return; // Permissions don't stop root.
        }
        let (_tmp, data) = data();
        let installed = Installed::new("v1");
        let ours = installed.ensure(&data, "new");
        let old = installed.ensure(&data, "old");
        // A folder whose contents can't be removed (as a Windows lock would).
        let locked = old.plugin_dir();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
        let cleanup = clean(&data, &ours.dir);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(cleanup.failed.len(), 1);
        assert!(old.dir.exists());

        let again = clean(&data, &ours.dir);
        assert!(again.failed.is_empty());
        assert!(!old.dir.exists());
    }

    /// A fixed folder (Codex's) is the same for every build and copied over at each start.
    #[test]
    fn a_fixed_folder_is_copied_over() {
        let (_tmp, data) = data();
        let installed = Installed::new("v1");
        let dir = data.codex_plugin_dir();
        replace_dir(&installed.plugin().join("codex"), &dir).unwrap();
        let manifest = dir.join(".codex-plugin").join("plugin.json");
        assert_eq!(fs::read_to_string(&manifest).unwrap(), "{}");

        // A new build's plugin: the same folder, its files replaced, a removed file gone.
        fs::write(dir.join("stale"), "").unwrap();
        let newer = Installed::new("v2");
        fs::write(
            newer
                .plugin()
                .join("codex")
                .join(".codex-plugin")
                .join("plugin.json"),
            "{\"version\":2}",
        )
        .unwrap();
        replace_dir(&newer.plugin().join("codex"), &dir).unwrap();
        assert_eq!(fs::read_to_string(&manifest).unwrap(), "{\"version\":2}");
        assert!(!dir.join("stale").exists());
        assert_eq!(names(&data), ["codex-plugin"], "no temporary folder left");

        // A source that can't be copied leaves the folder as it was.
        let empty = tempfile::tempdir().unwrap();
        assert!(replace_dir(&empty.path().join("gone"), &dir).is_err());
        assert!(manifest.is_file());
        assert_eq!(names(&data), ["codex-plugin"]);
    }

    /// At start: the copy, marked in use, Codex's folder and the cleanup together.
    #[test]
    fn prepare_makes_marks_and_cleans() {
        let (_tmp, data) = data();
        let installed = Installed::new("v1");
        let old = installed.ensure(&data, "old");
        let folder = prepare(&data, "new", &installed.bin(), &installed.plugin()).unwrap();
        assert_eq!(
            read_pid(&folder.dir.join(IN_USE_FILE)),
            Some(std::process::id())
        );
        assert!(!old.dir.exists());
        assert!(folder.complete());
        // No installed program: no copy.
        let empty = tempfile::tempdir().unwrap();
        assert_eq!(prepare(&data, "x", empty.path(), empty.path()), None);
    }
}
