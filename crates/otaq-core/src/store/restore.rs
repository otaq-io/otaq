//! Putting the `before-` copy back (d0140 gap review G5, UPD-08): what `otaq restore-database` and the window's
//! **Restore the copy from before 0.2.0** do. The service is stopped, the current database is moved aside as
//! `otaq.db.replaced-<time>` (never deleted), the copy goes back as `otaq.db`, and the service starts again. Each
//! step is reported as it starts, so the window can show the three steps; a step that fails stops the rest and says
//! what's left.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::versions::{self, DatabaseCopy};
use crate::naming;

/// Stops and starts the background service for a restore. The CLI does it as `daemon stop` / `daemon start`.
pub trait ServiceControl {
    /// Stops the service if one runs, and returns once it has let go of the database.
    fn stop(&mut self) -> Result<(), String>;
    fn start(&mut self) -> Result<(), String>;
}

/// The steps page 32 shows while it restores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum RestoreStep {
    ServiceStopped,
    PuttingBackCopy { made_at_ms: i64 },
    StartingService,
}

impl RestoreStep {
    /// The step's line, as page 32 words it.
    pub fn label(&self) -> String {
        match self {
            Self::ServiceStopped => "Background service stopped".to_owned(),
            Self::PuttingBackCopy { made_at_ms } => format!(
                "Putting back the copy from {}",
                versions::copy_time(*made_at_ms, ", ")
            ),
            Self::StartingService => "Starting the background service".to_owned(),
        }
    }
}

/// A restore that finished: the copy put back and where the database it replaced went.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restored {
    pub copy: DatabaseCopy,
    /// `otaq.db.replaced-<time>`; `None` when there was no database to move aside.
    pub replaced: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RestoreError {
    /// There is no `before-` copy: nothing was changed.
    NoCopy,
    /// A step failed: what went wrong and what's left.
    Failed { problem: String, left: String },
}

impl fmt::Display for RestoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCopy => write!(
                f,
                "There's no copy of the board from before a newer {} here; nothing was changed.",
                naming::PRODUCT_NAME
            ),
            Self::Failed { problem, left } => write!(f, "{problem}. {left}"),
        }
    }
}

impl std::error::Error for RestoreError {}

/// The confirmation (page 32): its title and its two paragraphs.
pub fn question(copy: &DatabaseCopy) -> (String, [String; 2]) {
    let version = &copy.version;
    let product = naming::PRODUCT_NAME;
    (
        format!("Restore the board from before {version}?"),
        [
            format!(
                "The board goes back to how it was before {product} {version} changed it, on {}. Changes made on \
                 {version} since then are lost: tickets, comments and moves from those days.",
                versions::copy_time(copy.made_at_ms, " at ")
            ),
            format!(
                "Worktrees, branches and your repos aren't touched. {product}'s background service stops and \
                 starts again while it restores."
            ),
        ],
    )
}

/// The name the current database is moved aside to: `otaq.db.replaced-20261027-091200` (local time), with `-2`,
/// `-3`… when that name is taken.
fn replaced_path(database: &Path, now_ms: i64) -> PathBuf {
    let stamp = match crate::local_time::local(now_ms) {
        Some(t) => format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        ),
        None => now_ms.to_string(),
    };
    let named = |n: u32| {
        let mut name = database.as_os_str().to_owned();
        name.push(format!(".replaced-{stamp}"));
        if n > 1 {
            name.push(format!("-{n}"));
        }
        PathBuf::from(name)
    };
    (1..)
        .map(named)
        .find(|path| !path.exists() && !wal_of(path).exists())
        .expect("a free name")
}

fn wal_of(database: &Path) -> PathBuf {
    let mut name = database.as_os_str().to_owned();
    name.push("-wal");
    PathBuf::from(name)
}

fn shm_of(database: &Path) -> PathBuf {
    let mut name = database.as_os_str().to_owned();
    name.push("-shm");
    PathBuf::from(name)
}

fn shown(path: &Path) -> String {
    path.display().to_string()
}

/// Puts the newest `before-` copy back as the database at `database`, stopping and starting the service with
/// `control` and reporting each step to `on_step` as it starts. The current database is moved aside, its `-wal`
/// with it (so the moved file stays whole), and its `-shm` removed; the copy itself stays where it is.
pub fn restore(
    database: &Path,
    control: &mut dyn ServiceControl,
    on_step: &mut dyn FnMut(&RestoreStep),
) -> Result<Restored, RestoreError> {
    let copy = versions::newest_copy(database).ok_or(RestoreError::NoCopy)?;
    let start_again = format!(
        "Start it again with `{} daemon start`.",
        naming::COMMAND_NAME
    );

    control.stop().map_err(|e| RestoreError::Failed {
        problem: format!("The background service couldn't be stopped: {e}"),
        left: "Nothing was changed.".to_owned(),
    })?;
    on_step(&RestoreStep::ServiceStopped);

    on_step(&RestoreStep::PuttingBackCopy {
        made_at_ms: copy.made_at_ms,
    });
    let replaced = if database.exists() {
        let aside = replaced_path(database, super::now_ms());
        std::fs::rename(database, &aside).map_err(|e| RestoreError::Failed {
            problem: format!("{} couldn't be moved aside: {e}", shown(database)),
            left: format!("Nothing was changed; the background service is stopped. {start_again}"),
        })?;
        let wal = wal_of(database);
        if wal.exists()
            && let Err(e) = std::fs::rename(&wal, wal_of(&aside))
        {
            return Err(RestoreError::Failed {
                problem: format!("{} couldn't be moved aside: {e}", shown(&wal)),
                left: format!(
                    "The database was moved to {}; move it back to {} to undo. The background service is stopped.",
                    shown(&aside),
                    shown(database)
                ),
            });
        }
        Some(aside)
    } else {
        None
    };
    let moved_aside = match &replaced {
        Some(aside) => format!(
            "The database it replaces was moved to {} (move it back to {} to undo).",
            shown(aside),
            shown(database)
        ),
        None => String::new(),
    };
    let shm = shm_of(database);
    if shm.exists()
        && let Err(e) = std::fs::remove_file(&shm)
    {
        return Err(RestoreError::Failed {
            problem: format!("{} couldn't be removed: {e}", shown(&shm)),
            left: format!("{moved_aside} The background service is stopped.")
                .trim_start()
                .to_owned(),
        });
    }
    // Copied under another name first, so a half-written file is never taken for the database.
    let mut partial = database.as_os_str().to_owned();
    partial.push(".restoring");
    let partial = PathBuf::from(partial);
    let put_back =
        std::fs::copy(&copy.path, &partial).and_then(|_| std::fs::rename(&partial, database));
    if let Err(e) = put_back {
        let _ = std::fs::remove_file(&partial);
        return Err(RestoreError::Failed {
            problem: format!("The copy {} couldn't be put back: {e}", shown(&copy.path)),
            left: format!("{moved_aside} The background service is stopped.")
                .trim_start()
                .to_owned(),
        });
    }

    on_step(&RestoreStep::StartingService);
    control.start().map_err(|e| RestoreError::Failed {
        problem: format!("The background service didn't start: {e}"),
        left: format!("The copy is back as {}. {start_again}", shown(database)),
    })?;
    Ok(Restored { copy, replaced })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Control {
        calls: Vec<&'static str>,
        fail_stop: bool,
        fail_start: bool,
    }

    impl ServiceControl for Control {
        fn stop(&mut self) -> Result<(), String> {
            self.calls.push("stop");
            if self.fail_stop {
                Err("it didn't stop in time".into())
            } else {
                Ok(())
            }
        }
        fn start(&mut self) -> Result<(), String> {
            self.calls.push("start");
            if self.fail_start {
                Err("see the logs".into())
            } else {
                Ok(())
            }
        }
    }

    fn folder() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("board.db");
        (tmp, db)
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn without_a_copy_nothing_changes() {
        let (tmp, db) = folder();
        std::fs::write(&db, "current").unwrap();
        let mut control = Control::default();
        let result = restore(&db, &mut control, &mut |_| {});
        assert_eq!(result, Err(RestoreError::NoCopy));
        assert!(control.calls.is_empty());
        assert_eq!(names(tmp.path()), ["board.db"]);
    }

    #[test]
    fn the_copy_goes_back_and_the_current_one_is_moved_aside() {
        let (tmp, db) = folder();
        std::fs::write(&db, "current").unwrap();
        std::fs::write(tmp.path().join("board.db-wal"), "wal").unwrap();
        std::fs::write(tmp.path().join("board.db-shm"), "shm").unwrap();
        std::fs::write(tmp.path().join("board.db.before-0.2.0"), "copy").unwrap();
        let mut control = Control::default();
        let mut steps = Vec::new();
        let restored = restore(&db, &mut control, &mut |s| steps.push(s.clone())).unwrap();

        assert_eq!(control.calls, ["stop", "start"]);
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0], RestoreStep::ServiceStopped);
        assert!(matches!(steps[1], RestoreStep::PuttingBackCopy { .. }));
        assert_eq!(steps[2], RestoreStep::StartingService);
        assert_eq!(std::fs::read_to_string(&db).unwrap(), "copy");
        let aside = restored.replaced.unwrap();
        assert_eq!(std::fs::read_to_string(&aside).unwrap(), "current");
        assert_eq!(std::fs::read_to_string(wal_of(&aside)).unwrap(), "wal");
        assert!(!tmp.path().join("board.db-wal").exists());
        assert!(!tmp.path().join("board.db-shm").exists());
        // The copy stays: the board can be restored from it again.
        assert!(tmp.path().join("board.db.before-0.2.0").exists());
        assert_eq!(restored.copy.version, "0.2.0");
    }

    #[test]
    fn a_failed_stop_changes_nothing() {
        let (tmp, db) = folder();
        std::fs::write(&db, "current").unwrap();
        std::fs::write(tmp.path().join("board.db.before-0.2.0"), "copy").unwrap();
        let mut control = Control {
            fail_stop: true,
            ..Control::default()
        };
        let mut steps = Vec::new();
        let Err(RestoreError::Failed { left, .. }) =
            restore(&db, &mut control, &mut |s| steps.push(s.clone()))
        else {
            panic!("the stop failed");
        };
        assert_eq!(left, "Nothing was changed.");
        assert!(steps.is_empty());
        assert_eq!(std::fs::read_to_string(&db).unwrap(), "current");
    }

    #[test]
    fn a_failed_start_says_the_copy_is_back() {
        let (tmp, db) = folder();
        std::fs::write(tmp.path().join("board.db.before-0.2.0"), "copy").unwrap();
        let mut control = Control {
            fail_start: true,
            ..Control::default()
        };
        let Err(RestoreError::Failed { problem, left }) = restore(&db, &mut control, &mut |_| {})
        else {
            panic!("the start failed");
        };
        assert!(problem.contains("see the logs"), "{problem}");
        assert!(left.starts_with("The copy is back as"), "{left}");
        assert_eq!(std::fs::read_to_string(&db).unwrap(), "copy");
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let (tmp, db) = folder();
        let first = replaced_path(&db, 0);
        std::fs::write(&first, "").unwrap();
        let second = replaced_path(&db, 0);
        assert_ne!(first, second);
        assert!(second.to_string_lossy().ends_with("-2"));
        drop(tmp);
    }

    #[test]
    fn the_question_names_the_version_and_the_time() {
        let copy = DatabaseCopy {
            path: "/d/board.db.before-0.2.0".into(),
            version: "0.2.0".into(),
            made_at_ms: 0,
        };
        let (title, [what, untouched]) = question(&copy);
        assert_eq!(title, "Restore the board from before 0.2.0?");
        assert!(what.starts_with(&format!(
            "The board goes back to how it was before {} 0.2.0 changed it, on ",
            naming::PRODUCT_NAME
        )));
        assert!(what.contains(" at "));
        assert!(untouched.starts_with("Worktrees, branches and your repos aren't touched."));
    }
}
