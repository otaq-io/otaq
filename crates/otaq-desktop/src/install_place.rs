//! **Where the app runs from** (T-0579; UPD-06; d0140 gap review G3): on macOS a release runs only from an
//! Applications folder. Example: Mia opens Otaq straight from the downloaded `.dmg`; the window offers only **Move to
//! Applications**, which copies it to `/Applications`, reopens it from there and quits, and the board loads in the
//! moved copy. Until then nothing runs from a place that disappears: no service, no update check, no `bin` link.
//!
//! - **Installed** ([`place`]): the bundle's real path is `/Applications/Otaq.app` or `~/Applications/Otaq.app`.
//!   Anything else isn't: a mounted volume (`/Volumes/…`, the disk image), a copy macOS translocated
//!   (`/private/var/folders/…/AppTranslocation/…`, an app opened from Downloads), or any other folder.
//! - **The gate** ([`gate`]): only a release (built with `OTAQ_RELEASE=1`, [`release`]) on macOS, run from a bundle,
//!   is held back. A build from source runs wherever `install-local.sh --dest` put it; Windows and Linux install to
//!   fixed folders or run an AppImage from anywhere (the runtime copy makes that safe).
//! - **The move** ([`put_in_place`]): the bundle is copied beside the target under a temporary name and swapped in,
//!   replacing an older `Otaq.app`; `/Applications` first, `~/Applications` when that can't be written
//!   ([`move_into`]). The copy loses macOS's quarantine flag, so it isn't translocated again. Stopping the older
//!   copy's service is the caller's ([`crate::api`]'s `move_app`), before the swap.
//! - **The hand-over** ([`reopen_command`], [`wait_for_previous`]): the moved copy is opened as a new instance with
//!   `OTAQ_MOVED_FROM=<pid>`, and waits for that process to exit before it claims the one-copy lock (d0139,
//!   [`crate::one_copy`]); otherwise it would find the lock held by the copy that is quitting, hand its start over to
//!   it and exit, leaving nothing open.

use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_view::view_models::move_app::{Failure, Source};

/// How long a moved copy waits for the copy that opened it to quit.
const PREVIOUS_WAIT: Duration = Duration::from_secs(15);
const PREVIOUS_POLL: Duration = Duration::from_millis(50);

/// Where a bundle runs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// `/Applications/Otaq.app` or `~/Applications/Otaq.app`.
    Installed,
    /// A mounted volume: the disk image.
    DiskImage,
    /// A copy macOS runs from a hidden temporary folder (an app opened from Downloads).
    Translocated,
    /// Any other folder.
    Elsewhere,
}

impl Place {
    /// How the Move window speaks of it.
    pub fn source(self) -> Source {
        match self {
            Self::DiskImage => Source::DiskImage,
            Self::Installed | Self::Translocated | Self::Elsewhere => Source::Elsewhere,
        }
    }
}

/// Where the bundle at `bundle` (its real path) runs from; `home` is the user's home folder (real path).
pub fn place(bundle: &Path, home: Option<&Path>) -> Place {
    let name = naming::bundle_folder_name();
    let installed = std::iter::once(PathBuf::from("/Applications"))
        .chain(home.map(|h| h.join("Applications")))
        .any(|folder| bundle == folder.join(&name));
    if installed {
        Place::Installed
    } else if bundle.starts_with("/Volumes") {
        Place::DiskImage
    } else if bundle
        .components()
        .any(|c| c.as_os_str() == "AppTranslocation")
    {
        Place::Translocated
    } else {
        Place::Elsewhere
    }
}

/// Whether the app starts as usual or shows only the Move window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Run,
    /// Not installed: only the Move window, for the bundle at `bundle`.
    MoveFirst {
        bundle: PathBuf,
        place: Place,
    },
}

impl Gate {
    /// Whether the service, the update check and the `bin` link may start.
    pub fn runs(&self) -> bool {
        matches!(self, Self::Run)
    }
}

/// The gate for a build (`release`) on `os` whose program sits in `bundle` (real path; `None` outside a bundle).
pub fn gate(release: bool, os: &str, bundle: Option<&Path>, home: Option<&Path>) -> Gate {
    match bundle {
        Some(bundle) if release && os == "macos" => match place(bundle, home) {
            Place::Installed => Gate::Run,
            place => Gate::MoveFirst {
                bundle: bundle.to_path_buf(),
                place,
            },
        },
        _ => Gate::Run,
    }
}

/// What the app knows about where it runs: its gate, and the data folder whose service an older copy in
/// Applications may run (the Move window stops it before replacing that copy).
pub struct InstallState {
    pub gate: Gate,
    pub data: Option<DataDir>,
}

/// Whether this is a release build: built with `OTAQ_RELEASE=1` (the release workflow sets it; a build from source
/// doesn't), which `build.rs` passes on as `RELEASE_BUILD`.
pub fn release() -> bool {
    option_env!("RELEASE_BUILD") == Some("1")
}

/// This process's gate.
pub fn current() -> Gate {
    let bundle = otaq_core::attention::bundle::current().map(|b| b.root);
    let home = std::env::home_dir().map(|h| h.canonicalize().unwrap_or(h));
    gate(
        release(),
        std::env::consts::OS,
        bundle.as_deref(),
        home.as_deref(),
    )
}

/// Where a move tries, in order: `/Applications`, then `~/Applications`.
pub fn roots(home: Option<&Path>) -> Vec<PathBuf> {
    std::iter::once(PathBuf::from("/Applications"))
        .chain(home.map(|h| h.join("Applications")))
        .collect()
}

/// Copies the bundle at `source` into the first of `roots` it can, replacing an `Otaq.app` there (`before_replace`
/// runs first when one is there, with its path: the caller stops its service). The moved bundle, or why it failed
/// everywhere: [`Failure::NoPermission`] when every folder refused for rights.
pub fn move_into(
    source: &Path,
    roots: &[PathBuf],
    before_replace: &mut dyn FnMut(&Path),
) -> Result<PathBuf, Failure> {
    let mut errors = Vec::new();
    for (i, root) in roots.iter().enumerate() {
        // `~/Applications` may not exist yet; `/Applications` always does.
        if i > 0
            && let Err(e) = std::fs::create_dir_all(root)
        {
            errors.push(e);
            continue;
        }
        let target = root.join(naming::bundle_folder_name());
        if target.exists() {
            before_replace(&target);
        }
        match put_in_place(source, root) {
            Ok(moved) => return Ok(moved),
            Err(e) => errors.push(e),
        }
    }
    let refused = |e: &io::Error| {
        matches!(
            e.kind(),
            io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem
        )
    };
    match errors.iter().find(|e| !refused(e)) {
        Some(e) => Err(Failure::Other(e.to_string())),
        None => Err(Failure::NoPermission),
    }
}

/// Copies the bundle at `source` to `<root>/Otaq.app`: written beside it under a temporary name, then swapped with
/// an older one, which is deleted. The copy loses macOS's quarantine flag.
pub fn put_in_place(source: &Path, root: &Path) -> io::Result<PathBuf> {
    let name = naming::bundle_folder_name();
    let target = root.join(&name);
    let pid = std::process::id();
    let fresh = root.join(format!(".{name}.moving-{pid}"));
    let old = root.join(format!(".{name}.old-{pid}"));
    remove_all(&fresh)?;
    let swapped = copy_tree(source, &fresh)
        .and_then(|()| clear_quarantine(&fresh))
        .and_then(|()| match std::fs::rename(&target, &old) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        })
        .and_then(|()| std::fs::rename(&fresh, &target));
    if swapped.is_err() && !target.exists() && old.exists() {
        let _ = std::fs::rename(&old, &target);
    }
    let _ = remove_all(&fresh);
    let _ = remove_all(&old);
    swapped.map(|()| target)
}

/// Copies a folder as it is: files with their permissions (and on macOS their extended attributes), links as links.
fn copy_tree(from: &Path, to: &Path) -> io::Result<()> {
    std::fs::create_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let (path, dest) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            let link = std::fs::read_link(&path)?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(link, &dest)?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(link, &dest)?;
        } else if kind.is_dir() {
            copy_tree(&path, &dest)?;
        } else {
            std::fs::copy(&path, &dest)?;
        }
    }
    Ok(())
}

/// Removes macOS's quarantine flag from the folder and everything in it (`xattr -dr com.apple.quarantine`), so the
/// moved copy opens from where it is instead of being translocated again. Nothing elsewhere.
fn clear_quarantine(bundle: &Path) -> io::Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    // Its exit status isn't checked: `xattr -d` reports a file without the flag as an error on some macOS versions.
    Command::new("/usr/bin/xattr")
        .args(["-dr", "com.apple.quarantine"])
        .arg(bundle)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(drop)
}

fn remove_all(path: &Path) -> io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// The variable a moved copy is opened with: the PID of the copy that moved it, `OTAQ_MOVED_FROM`.
pub fn moved_from_var() -> String {
    naming::env_var("MOVED_FROM")
}

/// Opens the moved bundle as a new instance (`open -n`: the quitting copy has the same bundle ID, and plain `open`
/// would only bring it forward), telling it this process's PID and the data folder (`OTAQ_HOME`, when set: an app
/// opened through LaunchServices doesn't inherit this one's environment).
pub fn reopen_command(bundle: &Path, data_home: Option<&OsStr>) -> Command {
    let mut command = Command::new("open");
    command.arg("-n").arg(bundle).arg("--env").arg(format!(
        "{}={}",
        moved_from_var(),
        std::process::id()
    ));
    if let Some(home) = data_home.filter(|h| !h.is_empty()) {
        let mut pair = std::ffi::OsString::from(format!("{}=", DataDir::env_var()));
        pair.push(home);
        command.arg("--env").arg(pair);
    }
    command
}

/// In a moved copy, waits (at most 15 s) for the copy that moved it to exit, so this one takes the one-copy lock
/// instead of handing over to it. Nothing when the variable isn't set.
pub fn wait_for_previous(value: Option<&str>) {
    let Some(pid) = value.and_then(|v| v.trim().parse::<u32>().ok()) else {
        return;
    };
    let deadline = Instant::now() + PREVIOUS_WAIT;
    while otaq_core::process::alive(pid) && Instant::now() < deadline {
        std::thread::sleep(PREVIOUS_POLL);
    }
}

/// Whether replacing `target` must stop this data folder's service first: when it runs from inside `target` (its
/// program is known), or when its program can't be told.
pub fn stops_service(service_program: Option<&Path>, target: &Path) -> bool {
    service_program.is_none_or(|program| program.starts_with(target))
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::naming::PRODUCT_NAME;

    const HOME: &str = "/Users/mia";

    fn app(folder: &str) -> PathBuf {
        Path::new(folder).join(naming::bundle_folder_name())
    }

    /// The path rule (UPD-06), as a table.
    #[test]
    fn the_place_of_a_bundle() {
        let home = Some(Path::new(HOME));
        for (bundle, expected) in [
            (app("/Applications"), Place::Installed),
            (app("/Users/mia/Applications"), Place::Installed),
            (
                app(&format!("/Volumes/{PRODUCT_NAME} 0.2.0")),
                Place::DiskImage,
            ),
            (
                app("/private/var/folders/x7/abc/T/AppTranslocation/1F2E/d"),
                Place::Translocated,
            ),
            (app("/Users/mia/Downloads"), Place::Elsewhere),
            (app("/Applications/Utilities"), Place::Elsewhere),
            (app("/Users/sam/Applications"), Place::Elsewhere),
            (
                Path::new("/Applications").join(format!("{PRODUCT_NAME} 2.app")),
                Place::Elsewhere,
            ),
        ] {
            assert_eq!(place(&bundle, home), expected, "{}", bundle.display());
        }
        assert_eq!(
            place(&app("/Users/mia/Applications"), None),
            Place::Elsewhere
        );
    }

    /// The gate: only a macOS release from a bundle outside Applications shows the Move window; a build from
    /// source, Windows and Linux, and a program outside a bundle always run.
    #[test]
    fn the_gate() {
        let home = Some(Path::new(HOME));
        let dmg = app(&format!("/Volumes/{PRODUCT_NAME}"));
        let from_source = app("/Users/mia/check");
        for (release, os, bundle, runs) in [
            (true, "macos", Some(app("/Applications")), true),
            (true, "macos", Some(dmg.clone()), false),
            (
                true,
                "macos",
                Some(app("/private/var/folders/x7/T/AppTranslocation/1F/d")),
                false,
            ),
            (true, "macos", Some(app("/Users/mia/Downloads")), false),
            (false, "macos", Some(dmg.clone()), true),
            (false, "macos", Some(from_source.clone()), true),
            (true, "macos", None, true),
            (true, "linux", Some(from_source.clone()), true),
            (true, "windows", Some(dmg.clone()), true),
        ] {
            let gate = gate(release, os, bundle.as_deref(), home);
            assert_eq!(gate.runs(), runs, "{release} {os} {bundle:?}");
        }
        assert_eq!(
            gate(true, "macos", Some(&dmg), home),
            Gate::MoveFirst {
                bundle: dmg,
                place: Place::DiskImage
            }
        );
        assert_eq!(Place::DiskImage.source(), Source::DiskImage);
        assert_eq!(Place::Translocated.source(), Source::Elsewhere);
    }

    /// A bundle as the disk image holds it: a program, a link, a nested folder.
    fn bundle(root: &Path, version: &str) -> PathBuf {
        let app = root.join(naming::bundle_folder_name());
        let macos = app.join("Contents").join("MacOS");
        std::fs::create_dir_all(&macos).unwrap();
        std::fs::write(macos.join("program"), version).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                macos.join("program"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
            std::os::unix::fs::symlink("MacOS/program", app.join("Contents").join("current"))
                .unwrap();
        }
        app
    }

    fn version_of(app: &Path) -> String {
        std::fs::read_to_string(app.join("Contents/MacOS/program")).unwrap()
    }

    /// The move copies the bundle into the first folder, replacing an older one after `before_replace`, and leaves
    /// no temporary folder.
    #[test]
    fn the_move_replaces_an_older_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let source = bundle(&tmp.path().join("Volumes").join(PRODUCT_NAME), "0.2.0");
        let system = tmp.path().join("Applications");
        bundle(&system, "0.1.0");
        let user = tmp.path().join("home/Applications");
        let mut replaced = Vec::new();
        let moved = move_into(&source, &[system.clone(), user.clone()], &mut |t| {
            replaced.push(t.to_path_buf())
        })
        .unwrap();
        assert_eq!(moved, system.join(naming::bundle_folder_name()));
        assert_eq!(replaced, std::slice::from_ref(&moved));
        assert_eq!(version_of(&moved), "0.2.0");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let link = std::fs::read_link(moved.join("Contents/current")).unwrap();
            assert_eq!(link, Path::new("MacOS/program"));
            let mode = std::fs::metadata(moved.join("Contents/MacOS/program"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
        }
        let left: Vec<_> = std::fs::read_dir(&system)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, [naming::bundle_folder_name()]);
        assert!(!user.exists(), "the second folder isn't touched");
        assert_eq!(version_of(&source), "0.2.0", "the source stays");
    }

    /// `/Applications` can't be written: `~/Applications`, made when missing. Neither: the permission failure.
    #[cfg(unix)]
    #[test]
    fn without_rights_the_next_folder_then_the_failure() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let source = bundle(&tmp.path().join("Volumes").join(PRODUCT_NAME), "0.2.0");
        let system = tmp.path().join("Applications");
        std::fs::create_dir(&system).unwrap();
        std::fs::set_permissions(&system, std::fs::Permissions::from_mode(0o555)).unwrap();
        if std::fs::create_dir(system.join("probe")).is_ok() {
            return; // Permissions don't stop root.
        }
        let home = tmp.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let user = home.join("Applications");
        let moved = move_into(&source, &[system.clone(), user.clone()], &mut |_| {}).unwrap();
        assert_eq!(moved, user.join(naming::bundle_folder_name()));

        std::fs::remove_dir_all(&user).unwrap();
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o555)).unwrap();
        let failed = move_into(&source, &[system.clone(), user], &mut |_| {});
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(&system, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(failed, Err(Failure::NoPermission));
    }

    /// Any other failure says the system's words.
    #[test]
    fn a_missing_source_is_another_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let system = tmp.path().join("Applications");
        std::fs::create_dir(&system).unwrap();
        let failed = move_into(&tmp.path().join("gone.app"), &[system], &mut |_| {});
        assert!(matches!(failed, Err(Failure::Other(_))), "{failed:?}");
    }

    /// The moved copy loses the quarantine flag, so macOS doesn't translocate it again.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_moved_copy_has_no_quarantine_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let source = bundle(&tmp.path().join("Downloads"), "0.2.0");
        let program = source.join("Contents/MacOS/program");
        let set = Command::new("/usr/bin/xattr")
            .args(["-w", "com.apple.quarantine", "0081;00000000;Safari;"])
            .arg(&program)
            .status()
            .unwrap();
        assert!(set.success());
        let root = tmp.path().join("Applications");
        std::fs::create_dir(&root).unwrap();
        let moved = put_in_place(&source, &root).unwrap();
        let read = Command::new("/usr/bin/xattr")
            .arg("-l")
            .arg(moved.join("Contents/MacOS/program"))
            .output()
            .unwrap();
        assert!(
            !String::from_utf8_lossy(&read.stdout).contains("com.apple.quarantine"),
            "{read:?}"
        );
    }

    #[test]
    fn the_reopen_command() {
        let command = reopen_command(&app("/Applications"), Some(OsStr::new("/tmp/h")));
        assert_eq!(command.get_program(), "open");
        let args: Vec<_> = command.get_args().map(|a| a.to_string_lossy()).collect();
        assert_eq!(
            args,
            [
                "-n".to_owned(),
                app("/Applications").display().to_string(),
                "--env".to_owned(),
                format!("{}={}", moved_from_var(), std::process::id()),
                "--env".to_owned(),
                format!("{}=/tmp/h", DataDir::env_var()),
            ]
        );
        assert_eq!(moved_from_var(), naming::env_var("MOVED_FROM"));
    }

    /// The moved copy waits for the one that moved it; a PID that already ended, or none, doesn't hold it up.
    #[test]
    fn the_moved_copy_waits_for_the_previous_one() {
        let mut child = Command::new("sleep").arg("0.3").spawn().unwrap();
        let pid = child.id().to_string();
        let started = Instant::now();
        let reaper = std::thread::spawn(move || child.wait());
        wait_for_previous(Some(&pid));
        assert!(started.elapsed() >= Duration::from_millis(250));
        reaper.join().unwrap().unwrap();
        let started = Instant::now();
        wait_for_previous(Some(&pid));
        wait_for_previous(None);
        wait_for_previous(Some("not a pid"));
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn which_service_is_stopped() {
        let target = app("/Applications");
        assert!(stops_service(
            Some(&target.join("Contents/MacOS/cli")),
            &target
        ));
        assert!(stops_service(None, &target));
        assert!(!stops_service(
            Some(&app("/Users/mia/check").join("Contents/MacOS/cli")),
            &target
        ));
    }
}
