//! The database across versions (d0140, UPD-08): the copy made before a version changes the layout
//! (`otaq.db.before-<version>`, newest only), the version that last changed it, and a database from a newer build,
//! which this build refuses to open instead of failing with a raw migration error.

use std::collections::HashSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;

use super::StoreError;
use crate::naming;

/// This build's version, the one a layout change it makes is named after.
pub const RUNNING_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The setting holding the version that last changed the database's layout (`"0.2.0"`), written after migrating.
pub const LAST_MIGRATED_BY: &str = "database.last_migrated_by";

/// The `before-` copy of a database, made by `version` just before it changed the layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatabaseCopy {
    pub path: PathBuf,
    /// The version that was about to change the database: the copy is "from before 0.2.0".
    pub version: String,
    /// When the copy was made (its file's modification time), Unix milliseconds.
    pub made_at_ms: i64,
}

/// The typed reason a service doesn't start on a database a newer build changed (d0140 gap review G5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewerDatabase {
    /// The version that last changed the database (`"0.2.0"`).
    pub last_opened_by: String,
    /// The copy from before that version, when there is one.
    pub copy: Option<DatabaseCopy>,
}

impl NewerDatabase {
    /// The plain message the CLI prints (d0140): what happened and the way back.
    pub fn message(&self) -> String {
        let version = &self.last_opened_by;
        let product = naming::PRODUCT_NAME;
        let first = format!("This board was last opened by {product} {version}.");
        match &self.copy {
            Some(_) => format!(
                "{first} Run `{} restore-database` to go back to the copy from before it, or reinstall {version}.",
                naming::COMMAND_NAME
            ),
            None => format!(
                "{first} There's no copy from before it, so reinstall {version} to open this board."
            ),
        }
    }
}

impl fmt::Display for NewerDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for NewerDatabase {}

/// The refusal's name in an error body's `data.refusal`, with the reason under `data.newer_database`.
pub const REFUSAL: &str = "newer_database";

/// The `data` of the error body that carries the reason from the CLI to the app.
pub fn error_data(newer: &NewerDatabase) -> serde_json::Value {
    serde_json::json!({ "refusal": REFUSAL, "newer_database": newer })
}

/// The reason in an error body's `data`, when it carries one.
pub fn from_error_data(data: Option<&serde_json::Value>) -> Option<NewerDatabase> {
    let data = data?;
    (data.get("refusal")?.as_str()? == REFUSAL)
        .then(|| serde_json::from_value(data.get("newer_database")?.clone()).ok())
        .flatten()
}

/// What the migrations table says about the database, compared with what `migrator` knows.
pub(super) struct Layout {
    /// Versions applied to the database.
    applied: HashSet<i64>,
    /// Versions `migrator` has.
    known: HashSet<i64>,
}

impl Layout {
    pub(super) async fn read(pool: &SqlitePool, migrator: &Migrator) -> Result<Self, StoreError> {
        let table: Option<String> = sqlx::query_scalar(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
        )
        .fetch_optional(pool)
        .await?;
        let applied = match table {
            Some(_) => sqlx::query_scalar("SELECT version FROM _sqlx_migrations")
                .fetch_all(pool)
                .await?
                .into_iter()
                .collect(),
            None => HashSet::new(),
        };
        let known = migrator
            .iter()
            .filter(|m| !m.migration_type.is_down_migration())
            .map(|m| m.version)
            .collect();
        Ok(Self { applied, known })
    }

    /// A newer build applied migrations this one doesn't know.
    pub(super) fn is_newer(&self) -> bool {
        self.applied.iter().any(|v| !self.known.contains(v))
    }

    pub(super) fn has_pending(&self) -> bool {
        self.known.iter().any(|v| !self.applied.contains(v))
    }

    /// A fresh database has nothing applied, so nothing to copy.
    pub(super) fn is_fresh(&self) -> bool {
        self.applied.is_empty()
    }
}

/// The reason for a database a newer build changed: its recorded version (else the copy's), and the copy.
pub(super) async fn newer_reason(pool: &SqlitePool, database: &Path) -> NewerDatabase {
    let recorded: Option<String> =
        sqlx::query_scalar("SELECT value_json FROM settings WHERE key = ?")
            .bind(LAST_MIGRATED_BY)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .and_then(|json: String| serde_json::from_str(&json).ok());
    let copy = newest_copy(database);
    let last_opened_by = recorded
        .or_else(|| copy.as_ref().map(|c| c.version.clone()))
        .unwrap_or_else(|| "a newer version".to_owned());
    NewerDatabase {
        last_opened_by,
        copy,
    }
}

/// Records the version that just changed the layout.
pub(super) async fn record_migrated_by(pool: &SqlitePool, version: &str) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
         ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
    )
    .bind(LAST_MIGRATED_BY)
    .bind(serde_json::to_string(version).expect("a string serializes"))
    .bind(super::now_ms())
    .execute(pool)
    .await?;
    Ok(())
}

fn with_suffix(database: &Path, suffix: &str) -> PathBuf {
    let mut name = database.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// The copy's name: `otaq.db.before-0.2.0`.
pub fn copy_path(database: &Path, version: &str) -> PathBuf {
    with_suffix(database, &format!(".before-{version}"))
}

/// Whether `name` is a `before-` copy of the database named `database_name` (only complete copies: one being
/// written has another name).
pub fn is_copy_name(database_name: &str, name: &str) -> bool {
    name.strip_prefix(database_name)
        .and_then(|rest| rest.strip_prefix(".before-"))
        .is_some_and(|version| !version.is_empty())
}

/// Every complete `before-` copy next to `database`.
pub fn copies(database: &Path) -> Vec<DatabaseCopy> {
    let (Some(folder), Some(db_name)) = (
        database.parent(),
        database.file_name().and_then(|n| n.to_str()),
    ) else {
        return Vec::new();
    };
    let prefix = format!("{db_name}.before-");
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if !is_copy_name(db_name, &name) {
                return None;
            }
            let meta = entry.metadata().ok().filter(|m| m.is_file())?;
            let made_at_ms = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .and_then(|d| i64::try_from(d.as_millis()).ok())
                .unwrap_or(0);
            Some(DatabaseCopy {
                path: entry.path(),
                version: name[prefix.len()..].to_owned(),
                made_at_ms,
            })
        })
        .collect()
}

/// The newest copy (only one is kept, but a leftover from a failed cleanup is never chosen over it).
pub fn newest_copy(database: &Path) -> Option<DatabaseCopy> {
    copies(database).into_iter().max_by_key(|c| c.made_at_ms)
}

/// Copies the database to `otaq.db.before-<version>` with `VACUUM INTO` (a consistent copy, the `-wal` included),
/// under a temporary name first, then renamed; then deletes every older copy. A failure leaves the older copy. A
/// copy this version already made (a migration that failed before) is kept: it's the one from before it.
pub(super) async fn copy_before(
    pool: &SqlitePool,
    database: &Path,
    version: &str,
) -> Result<DatabaseCopy, StoreError> {
    let target = copy_path(database, version);
    if !target.is_file() {
        write_copy(pool, database, version, &target).await?;
    }
    for older in copies(database) {
        if older.path != target
            && let Err(e) = std::fs::remove_file(&older.path)
        {
            tracing::warn!(path = %older.path.display(), error = %e, "can't delete an older database copy");
        }
    }
    copies(database)
        .into_iter()
        .find(|c| c.path == target)
        .ok_or_else(|| {
            StoreError::Folder(std::io::Error::other(format!(
                "the database copy {} is missing",
                target.display()
            )))
        })
}

async fn write_copy(
    pool: &SqlitePool,
    database: &Path,
    version: &str,
    target: &Path,
) -> Result<(), StoreError> {
    let partial = with_suffix(database, &format!(".copying-{version}"));
    let _ = std::fs::remove_file(&partial);
    let partial_text = partial.to_str().ok_or_else(|| {
        StoreError::Folder(std::io::Error::other(format!(
            "the database's path isn't valid UTF-8: {}",
            partial.display()
        )))
    })?;
    sqlx::query("VACUUM INTO ?")
        .bind(partial_text)
        .execute(pool)
        .await?;
    std::fs::rename(&partial, target).map_err(StoreError::Folder)?;
    tracing::info!(path = %target.display(), "database copied before changing its layout");
    Ok(())
}

/// For tests (here and in the other crates) only: makes the database at `database` look as if a newer build
/// `version` had changed it: the copy from before it, then a migration this build doesn't know and the recorded
/// version. Never shipped as a migration.
#[doc(hidden)]
pub async fn testing_change_as_newer(database: &Path, version: &str) -> Result<(), StoreError> {
    let store = super::Store::open(database).await?;
    copy_before(store.pool(), database, version).await?;
    sqlx::query(
        "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time)
         VALUES (?, 'from a newer build (test)', TRUE, X'00', 0)",
    )
    .bind(i64::MAX - 1)
    .execute(store.pool())
    .await?;
    record_migrated_by(store.pool(), version).await?;
    store.close().await;
    Ok(())
}

/// Words for the dates page 32 shows: "Oct 24 at 09:12" (the confirmation) and "Oct 24, 09:12" (the step).
pub fn copy_time(made_at_ms: i64, joiner: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    match crate::local_time::local(made_at_ms) {
        Some(t) => format!(
            "{} {}{joiner}{:02}:{:02}",
            MONTHS[(t.month as usize).saturating_sub(1) % 12],
            t.day,
            t.hour,
            t.minute
        ),
        None => "an unknown time".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::str::FromStr;

    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::*;
    use crate::store::{MIGRATOR, Store, migrate};

    /// The shipped migrations up to `version`: an older build.
    fn up_to(version: i64) -> Migrator {
        Migrator {
            migrations: Cow::Owned(
                MIGRATOR
                    .iter()
                    .filter(|m| m.version <= version)
                    .cloned()
                    .collect(),
            ),
            ..Migrator::DEFAULT
        }
    }

    async fn pool(path: &Path) -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite:")
            .unwrap()
            .filename(path)
            .create_if_missing(true);
        SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap()
    }

    /// Opens the database at `path` as a build with the migrations up to `last`, named `version`.
    async fn open_as(path: &Path, last: i64, version: &str) -> Result<(), StoreError> {
        let pool = pool(path).await;
        let migrated = migrate(&pool, path, &up_to(last), version).await;
        pool.close().await;
        migrated
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| !n.ends_with("-wal") && !n.ends_with("-shm"))
            .collect();
        names.sort();
        names
    }

    #[tokio::test]
    async fn a_copy_only_when_migrations_are_pending_on_a_used_database() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("board.db");
        // A fresh database: nothing to copy.
        open_as(&db, 10, "0.1.0").await.unwrap();
        assert_eq!(names(tmp.path()), ["board.db"]);
        // Up to date: nothing to copy.
        open_as(&db, 10, "0.1.0").await.unwrap();
        assert_eq!(names(tmp.path()), ["board.db"]);
        // Pending migrations: copied first, as it was before them.
        open_as(&db, 20, "0.2.0").await.unwrap();
        assert_eq!(names(tmp.path()), ["board.db", "board.db.before-0.2.0"]);
        let copy = newest_copy(&db).unwrap();
        assert_eq!(copy.version, "0.2.0");
        let before = pool(&copy.path).await;
        let applied: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
            .fetch_one(&before)
            .await
            .unwrap();
        assert_eq!(applied, 10);
        before.close().await;
        let now = pool(&db).await;
        let recorded: String = sqlx::query_scalar("SELECT value_json FROM settings WHERE key = ?")
            .bind(LAST_MIGRATED_BY)
            .fetch_one(&now)
            .await
            .unwrap();
        assert_eq!(recorded, "\"0.2.0\"");
        now.close().await;
    }

    #[tokio::test]
    async fn only_the_newest_copy_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("board.db");
        open_as(&db, 10, "0.1.0").await.unwrap();
        open_as(&db, 20, "0.2.0").await.unwrap();
        open_as(&db, 30, "0.3.0").await.unwrap();
        assert_eq!(names(tmp.path()), ["board.db", "board.db.before-0.3.0"]);
    }

    #[tokio::test]
    async fn a_database_from_a_newer_build_is_refused_unchanged() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("board.db");
        open_as(&db, 10, "0.1.0").await.unwrap();
        open_as(&db, 20, "0.2.0").await.unwrap();
        let Err(StoreError::NewerDatabase(newer)) = open_as(&db, 10, "0.1.0").await else {
            panic!("refused as newer");
        };
        assert_eq!(newer.last_opened_by, "0.2.0");
        assert_eq!(newer.copy.unwrap().version, "0.2.0");
        // Nothing was copied or migrated.
        assert_eq!(names(tmp.path()), ["board.db", "board.db.before-0.2.0"]);
        assert!(Store::newer_database(&db).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn the_test_change_makes_the_shipped_build_refuse() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("board.db");
        assert!(Store::newer_database(&db).await.unwrap().is_none());
        testing_change_as_newer(&db, "9.0.0").await.unwrap();
        let newer = Store::newer_database(&db).await.unwrap().unwrap();
        assert_eq!(newer.last_opened_by, "9.0.0");
        assert_eq!(
            newer.copy.as_ref().map(|c| c.path.clone()),
            Some(copy_path(&db, "9.0.0"))
        );
        let Err(StoreError::NewerDatabase(refused)) = Store::open(&db).await else {
            panic!("refused as newer");
        };
        assert_eq!(refused, newer);
        assert!(!refused.to_string().contains("migration"));
    }

    #[test]
    fn copy_names() {
        assert!(is_copy_name("board.db", "board.db.before-0.2.0"));
        assert!(!is_copy_name("board.db", "board.db.before-"));
        assert!(!is_copy_name("board.db", "board.db.copying-0.2.0"));
        assert!(!is_copy_name("board.db", "board.db-wal"));
        assert!(!is_copy_name("board.db", "other.db.before-0.2.0"));
        assert_eq!(
            copy_path(Path::new("/d/board.db"), "0.2.0"),
            Path::new("/d/board.db.before-0.2.0")
        );
    }

    #[test]
    fn the_message_says_the_way_back() {
        let with_copy = NewerDatabase {
            last_opened_by: "0.2.0".into(),
            copy: Some(DatabaseCopy {
                path: "/d/board.db.before-0.2.0".into(),
                version: "0.2.0".into(),
                made_at_ms: 0,
            }),
        };
        assert_eq!(
            with_copy.message(),
            format!(
                "This board was last opened by {p} 0.2.0. Run `{c} restore-database` to go back to the copy \
                 from before it, or reinstall 0.2.0.",
                p = naming::PRODUCT_NAME,
                c = naming::COMMAND_NAME
            )
        );
        let without = NewerDatabase {
            copy: None,
            ..with_copy.clone()
        };
        assert!(
            without
                .message()
                .ends_with("reinstall 0.2.0 to open this board.")
        );
        let data = error_data(&with_copy);
        assert_eq!(from_error_data(Some(&data)), Some(with_copy));
        assert_eq!(
            from_error_data(Some(&serde_json::json!({"refusal": "cycle"}))),
            None
        );
        assert_eq!(from_error_data(None), None);
    }
}
