//! A ready-made database for tests: [`seed`] copies a **template** `otaq.db` into a new data folder, so the
//! `Store::open_in` that follows finds it up to date and runs no migration. Opening a fresh database runs every
//! migration (about a hundred), which is most of a typical test's time on Windows.
//!
//! ```ignore
//! let data = DataDir::new(home.path().join("home"));
//! store::testing::seed(&data);              // copies a migrated otaq.db into the data folder
//! let store = Store::open_in(&data).await?; // finds it up to date: no migration runs
//! ```
//!
//! The template is made by [`Store::open`] itself on a scratch data folder, so it holds exactly what a real first
//! start leaves: every migration, the version record and the shipped agents. Its pool is closed (which checkpoints
//! the WAL into the file) before it is kept at `<system temp>/otaq-test-store/<key>.db`, where `<key>` hashes every
//! migration's checksum and [`RUNNING_VERSION`]: a changed or added migration, or a new version, makes a new
//! template. `Store::open` stays the only code that migrates; this module only copies its result.
//!
//! nextest runs each test in its own process, so the template is shared between processes: the first test to need
//! it builds it in a file of its own (a unique name beside it) and renames it into place; a test that loses that
//! race uses the winner's file. Every builder makes the same content, so the race is harmless.
//!
//! Tests that are about a first start, the migrations or the shipped agents' first apply don't call `seed`: they keep
//! opening a new database.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sqlx::migrate::Migrator;

use super::versions::RUNNING_VERSION;
use super::{MIGRATOR, Store};
use crate::data_dir::DataDir;

/// The folder under the system's temporary folder holding the templates.
const TEMPLATES: &str = "otaq-test-store";

/// Copies the template database into `data`'s database path (creating the data folder), building the template
/// first when this build's has none yet. `data` must have no database yet. Panics on any failure, like the other
/// test helpers.
pub fn seed(data: &DataDir) {
    let dir = std::env::temp_dir().join(TEMPLATES);
    let template = template(&dir, &key(&MIGRATOR, RUNNING_VERSION));
    DataDir::ensure_dir(data.root()).expect("the data folder");
    let database = data.database_path();
    assert!(
        !database.exists(),
        "seed needs a data folder with no database yet: {}",
        database.display()
    );
    std::fs::copy(&template, &database).unwrap_or_else(|e| {
        panic!(
            "can't copy the template database {} to {}: {e}",
            template.display(),
            database.display()
        )
    });
}

/// The template's name: a hash of every migration (its version, description and checksum) and `version`.
fn key(migrator: &Migrator, version: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"otaq-test-store 1\0");
    hash.update(version.as_bytes());
    for migration in migrator.iter() {
        hash.update(b"\0");
        hash.update(migration.version.to_le_bytes());
        hash.update(migration.description.as_bytes());
        hash.update(b"\0");
        hash.update(&migration.checksum);
    }
    hash.finalize()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The template `<dir>/<key>.db`, built by this build's `Store::open` when it isn't there yet.
fn template(dir: &Path, key: &str) -> PathBuf {
    let path = dir.join(format!("{key}.db"));
    if path.is_file() {
        return path;
    }
    std::fs::create_dir_all(dir).expect("the templates folder");
    let building = dir.join(format!(
        "{key}.{}.{}.building",
        std::process::id(),
        super::new_id()
    ));
    build(&building);
    if path.is_file() {
        // Another test built it meanwhile: use the winner's file.
        discard(&building);
        return path;
    }
    if let Err(e) = std::fs::rename(&building, &path) {
        discard(&building);
        // Windows refuses to replace a file another process has open: then the winner's file is there.
        assert!(
            path.is_file(),
            "can't keep the template database at {}: {e}",
            path.display()
        );
    }
    path
}

/// Opens a new database at `path` with `Store::open`, as a first start does, and closes it. It runs on a thread
/// of its own with its own runtime, so `seed` works the same from a test's runtime or from plain code.
fn build(path: &Path) {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("a runtime for the template database");
                runtime.block_on(async {
                    let store = Store::open(path).await.unwrap_or_else(|e| {
                        panic!("can't build the template database {}: {e}", path.display())
                    });
                    // Closing the last connection checkpoints the WAL into the file and removes it.
                    store.close().await;
                });
            })
            .join()
            .expect("the template database's builder");
    });
}

/// Removes a builder's own file and whatever SQLite left beside it.
fn discard(building: &Path) {
    for suffix in ["", "-wal", "-shm"] {
        let mut name = building.as_os_str().to_owned();
        name.push(suffix);
        let _ = std::fs::remove_file(PathBuf::from(name));
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::str::FromStr;
    use std::sync::Barrier;

    use sqlx::SqlitePool;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    use super::*;
    use crate::store::versions::LAST_MIGRATED_BY;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }

    /// The schema: every table, index and trigger as SQLite stores its SQL.
    async fn schema(pool: &SqlitePool) -> Vec<(String, String, Option<String>)> {
        sqlx::query_as(
            "SELECT type, name, sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    /// The applied migrations with when each was applied: a migration run would add or change a row.
    async fn applied(pool: &SqlitePool) -> Vec<(i64, String)> {
        sqlx::query_as(
            "SELECT version, CAST(installed_on AS TEXT) FROM _sqlx_migrations ORDER BY version",
        )
        .fetch_all(pool)
        .await
        .unwrap()
    }

    /// A plain pool on the file at `path`, which changes nothing in it.
    async fn plain(path: &Path) -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite:")
            .unwrap()
            .filename(path);
        SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .unwrap()
    }

    async fn migrated_by(pool: &SqlitePool) -> String {
        sqlx::query_scalar("SELECT value_json FROM settings WHERE key = ?")
            .bind(LAST_MIGRATED_BY)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn agents(pool: &SqlitePool) -> Vec<String> {
        sqlx::query_scalar("SELECT name FROM agents ORDER BY name")
            .fetch_all(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_seeded_database_opens_with_no_migration_and_matches_a_fresh_one() {
        let tmp = tempfile::tempdir().unwrap();
        let seeded = DataDir::new(tmp.path().join("seeded"));
        seed(&seeded);
        let before = plain(&seeded.database_path()).await;
        let as_copied = applied(&before).await;
        before.close().await;

        let store = Store::open_in(&seeded).await.unwrap();
        // No migration ran: the applied rows are the template's, and no copy was made before migrations.
        assert_eq!(applied(store.pool()).await, as_copied);
        assert_eq!(names(seeded.root())[0], "otaq.db");
        assert!(
            names(seeded.root()).iter().all(|n| !n.contains(".before-")),
            "{:?}",
            names(seeded.root())
        );
        let seeded_schema = schema(store.pool()).await;
        let seeded_by = migrated_by(store.pool()).await;
        let seeded_agents = agents(store.pool()).await;
        store.close().await;

        // The same database as a fresh open: schema, migrations, version record and shipped agents.
        let fresh = Store::open_in(&DataDir::new(tmp.path().join("fresh")))
            .await
            .unwrap();
        assert_eq!(seeded_schema, schema(fresh.pool()).await);
        let versions = |rows: &[(i64, String)]| -> Vec<i64> { rows.iter().map(|r| r.0).collect() };
        assert_eq!(versions(&as_copied), versions(&applied(fresh.pool()).await));
        assert_eq!(
            versions(&as_copied),
            MIGRATOR.iter().map(|m| m.version).collect::<Vec<_>>()
        );
        assert_eq!(seeded_by, migrated_by(fresh.pool()).await);
        assert_eq!(seeded_by, format!("\"{RUNNING_VERSION}\""));
        assert!(!seeded_agents.is_empty(), "the shipped agents are there");
        assert_eq!(seeded_agents, agents(fresh.pool()).await);
        fresh.close().await;
    }

    #[test]
    fn seed_works_outside_a_runtime_and_twice_in_a_row_uses_one_template() {
        let tmp = tempfile::tempdir().unwrap();
        seed(&DataDir::new(tmp.path().join("a")));
        seed(&DataDir::new(tmp.path().join("b")));
        let template = std::env::temp_dir()
            .join(TEMPLATES)
            .join(format!("{}.db", key(&MIGRATOR, RUNNING_VERSION)));
        let bytes = std::fs::read(&template).unwrap();
        assert_eq!(std::fs::read(tmp.path().join("a/otaq.db")).unwrap(), bytes);
        assert_eq!(std::fs::read(tmp.path().join("b/otaq.db")).unwrap(), bytes);
    }

    #[test]
    fn builders_at_once_end_with_one_valid_template() {
        let tmp = tempfile::tempdir().unwrap();
        let key = key(&MIGRATOR, RUNNING_VERSION);
        let builders = 4;
        let barrier = Barrier::new(builders);
        let paths: Vec<PathBuf> = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..builders)
                .map(|_| {
                    scope.spawn(|| {
                        barrier.wait();
                        template(tmp.path(), &key)
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let path = tmp.path().join(format!("{key}.db"));
        assert!(paths.iter().all(|p| *p == path), "{paths:?}");
        // Only the template: no builder's file, WAL or shared-memory file left behind.
        assert_eq!(names(tmp.path()), [format!("{key}.db")]);

        let data = DataDir::new(tmp.path().join("data"));
        DataDir::ensure_dir(data.root()).unwrap();
        std::fs::copy(&path, data.database_path()).unwrap();
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let store = Store::open_in(&data).await.unwrap();
            assert_eq!(applied(store.pool()).await.len(), MIGRATOR.iter().count());
            store.close().await;
        });
    }

    #[test]
    fn another_migration_list_or_version_makes_a_new_template() {
        let running = key(&MIGRATOR, RUNNING_VERSION);
        let other_version = key(&MIGRATOR, "0.0.0-other");
        let fewer = Migrator {
            migrations: Cow::Owned(MIGRATOR.iter().skip(1).cloned().collect()),
            ..Migrator::DEFAULT
        };
        let mut changed_one: Vec<_> = MIGRATOR.iter().cloned().collect();
        changed_one.last_mut().unwrap().checksum = Cow::Owned(vec![0; 48]);
        let changed = Migrator {
            migrations: Cow::Owned(changed_one),
            ..Migrator::DEFAULT
        };
        let keys = [
            running.clone(),
            other_version.clone(),
            key(&fewer, RUNNING_VERSION),
            key(&changed, RUNNING_VERSION),
        ];
        for (i, a) in keys.iter().enumerate() {
            for b in &keys[i + 1..] {
                assert_ne!(a, b);
            }
        }
        // The same inputs give the same key.
        assert_eq!(running, key(&MIGRATOR, RUNNING_VERSION));

        let tmp = tempfile::tempdir().unwrap();
        template(tmp.path(), &running);
        template(tmp.path(), &other_version);
        let mut expected = vec![format!("{running}.db"), format!("{other_version}.db")];
        expected.sort();
        assert_eq!(names(tmp.path()), expected);
    }
}
