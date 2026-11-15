//! The SQLite store (d0003, d0055): one database file in the data folder, migrations from day one.
//!
//! SQL here stays portable to Postgres (see `migrations/0001_initial.sql`). Known SQLite-only details, kept
//! to this module: the connection options (WAL, `foreign_keys`) and `?` placeholders in query text.

pub mod agents;
pub mod merged_into;
pub mod projects;
pub mod recents;
pub mod restore;
pub mod settings;
pub mod testing;
pub mod tickets;
pub mod versions;

use std::fmt;
use std::path::Path;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::migrate::{MigrateError, Migrator};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Sqlite, SqlitePool, Transaction};

use crate::data_dir::DataDir;
pub use versions::NewerDatabase;

pub(crate) static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// The migration that makes a flow's delivery its setting (`0085_delivery_setting.sql`); its stored flows are
/// rewritten right after it ([`Store::move_deliveries_into_flows`]).
const DELIVERY_SETTING: i64 = 85;

/// The migration that lets Otaq apply a flow version (`0124_skill_namespaces.sql`); its stored flows name Otaq's
/// skills `otaq:` and `tuq:` right after it ([`Store::name_own_skills_in_flows`]), whenever no rename time is
/// recorded ([`Store::skills_namespaced_at`]).
#[cfg(test)]
const SKILL_NAMESPACES: i64 = 124;

/// The `settings` key that says the rest of migration 0085 ([`Store::move_deliveries_into_flows`]) is still to run:
/// written before the migrations run on a used database, removed in that rest's own transaction. A start that ends
/// between the two (a `daemon start` that gave up ends the service it launched) leaves it, and the next start runs
/// the rest. Migration 0124's rest needs none: it records [`Store::skills_namespaced_at`] in its transaction.
pub(crate) const DELIVERIES_PENDING: &str = "migration_0085_rest_pending";

/// An open database with every migration applied.
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Opens the database in the data folder, creating the folder and the file on first use.
    pub async fn open_in(data: &DataDir) -> Result<Self, StoreError> {
        DataDir::ensure_dir(data.root()).map_err(StoreError::Folder)?;
        Self::open(&data.database_path()).await
    }

    /// Opens (or creates) the database at `path` and runs any pending migrations. Opening an up-to-date
    /// database changes nothing. Before migrations change a database that already has some applied, it is copied
    /// to `otaq.db.before-<this version>` (UPD-08); a database from a newer build is refused with
    /// [`StoreError::NewerDatabase`], unchanged.
    pub async fn open(path: &Path) -> Result<Self, StoreError> {
        let pool = Self::connect(path, true).await?;
        let deliveries_migrate = !versions::applied(&pool, DELIVERY_SETTING).await;
        let marks: &[&str] = if deliveries_migrate {
            &[DELIVERIES_PENDING]
        } else {
            &[]
        };
        migrate(&pool, path, &MIGRATOR, versions::RUNNING_VERSION, marks).await?;
        let store = Self { pool };
        // The rest of migration 0085, which SQL can't write: the flows' Delivery stages become their setting. Also
        // when a start cut short left it undone.
        if deliveries_migrate || store.has_setting(DELIVERIES_PENDING).await? {
            store.move_deliveries_into_flows().await?;
        }
        // The rest of migration 0124: Otaq's skills are named `otaq:` and `tuq:` in every project's flow, once. Until
        // it has run, its time isn't recorded: a start cut short after the migration's SQL leaves it to the next.
        if store.skills_namespaced_at().await?.is_none() {
            store.name_own_skills_in_flows().await?;
        }
        // The first start: the shipped agents applied, once (AGM-02).
        crate::agents::shipped::apply_on_first_start(&store).await?;
        Ok(store)
    }

    /// Whether the database at `path` was changed by a newer build, without changing anything. `None` for a
    /// database this build can open, or none at all.
    pub async fn newer_database(path: &Path) -> Result<Option<NewerDatabase>, StoreError> {
        if !path.is_file() {
            return Ok(None);
        }
        let pool = Self::connect(path, false).await?;
        let layout = versions::Layout::read(&pool, &MIGRATOR).await?;
        let newer = if layout.is_newer() {
            Some(versions::newer_reason(&pool, path).await)
        } else {
            None
        };
        pool.close().await;
        Ok(newer)
    }

    async fn connect(path: &Path, create: bool) -> Result<SqlitePool, StoreError> {
        let options = SqliteConnectOptions::from_str("sqlite:")?
            .filename(path)
            .create_if_missing(create)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal);
        Ok(SqlitePoolOptions::new().connect_with(options).await?)
    }

    async fn has_setting(&self, key: &str) -> Result<bool, StoreError> {
        let found: Option<i64> = sqlx::query_scalar("SELECT 1 FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(found.is_some())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Begins a transaction, always this way and never with `pool().begin()`. `BEGIN IMMEDIATE` takes the write
    /// lock up front, so the transaction waits out another connection's write (the busy timeout). A plain
    /// (deferred) `BEGIN` that reads first and then writes fails at once with "database is locked" when another
    /// connection holds the write lock at that moment: SQLite doesn't wait there, to avoid a deadlock. Windows hit
    /// it whenever the service's background writes (the scheduler, a start) overlapped a request's transaction.
    pub async fn begin_write(&self) -> Result<Transaction<'static, Sqlite>, sqlx::Error> {
        self.pool.begin_with("BEGIN IMMEDIATE").await
    }

    /// Waits for open connections to finish and closes the database.
    pub async fn close(self) {
        self.pool.close().await;
    }
}

/// Runs `migrator` on the database at `path` as `version`: refused when a newer build changed it; copied first
/// when migrations are pending on a database that has some applied, and the `settings` keys `marks` written (after
/// the copy, before the migrations: what is left to do once they ran); the version recorded after.
async fn migrate(
    pool: &SqlitePool,
    path: &Path,
    migrator: &Migrator,
    version: &str,
    marks: &[&str],
) -> Result<(), StoreError> {
    let layout = versions::Layout::read(pool, migrator).await?;
    if layout.is_newer() {
        let newer = versions::newer_reason(pool, path).await;
        pool.close().await;
        return Err(StoreError::NewerDatabase(newer));
    }
    if !layout.has_pending() {
        return Ok(());
    }
    if !layout.is_fresh() {
        versions::copy_before(pool, path, version).await?;
        for key in marks {
            sqlx::query(
                "INSERT INTO settings (key, value_json, updated_at) VALUES (?, 'true', ?)
                 ON CONFLICT (key) DO NOTHING",
            )
            .bind(key)
            .bind(now_ms())
            .execute(pool)
            .await?;
        }
    }
    migrator.run(pool).await?;
    versions::record_migrated_by(pool, version).await
}

/// A new id for any row: a UUIDv7 string, so ids sort roughly by creation time.
pub fn new_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Now, as stored in `*_at` columns: milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(since_epoch.as_millis()).unwrap_or(i64::MAX)
}

#[derive(Debug)]
pub enum StoreError {
    Folder(std::io::Error),
    Database(sqlx::Error),
    Migration(MigrateError),
    /// A newer build changed the database: this one doesn't open it (UPD-08).
    NewerDatabase(NewerDatabase),
}

impl fmt::Display for StoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Folder(e) => write!(f, "can't create the data folder: {e}"),
            Self::Database(e) => write!(f, "database error: {e}"),
            Self::Migration(e) => write!(f, "database migration failed: {e}"),
            Self::NewerDatabase(newer) => write!(f, "{newer}"),
        }
    }
}

impl std::error::Error for StoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Folder(e) => Some(e),
            Self::Database(e) => Some(e),
            Self::Migration(e) => Some(e),
            Self::NewerDatabase(newer) => Some(newer),
        }
    }
}

impl From<sqlx::Error> for StoreError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e)
    }
}

impl From<MigrateError> for StoreError {
    fn from(e: MigrateError) -> Self {
        Self::Migration(e)
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

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

    /// A database an older build (migrations up to `last`) left with one project, `SHOP`, whose only flow is `yaml`.
    async fn older_database(path: &Path, last: i64, yaml: &str) {
        let pool = Store::connect(path, true).await.unwrap();
        migrate(&pool, path, &up_to(last), "0.2.0", &[])
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO projects (id, key, name, repo_path, base_branch, created_at)
             VALUES ('p1', 'SHOP', 'Shop', '/shop', 'main', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO project_flows (project_id, version, yaml, applied_by, applied_at, current)
             VALUES ('p1', 1, ?, 'user', 1, TRUE)",
        )
        .bind(yaml)
        .execute(&pool)
        .await
        .unwrap();
        pool.close().await;
    }

    /// The first start of this build that ends right after the migrations' SQL, before their Rust steps: the
    /// service killed by a `daemon start` that gave up waiting.
    async fn killed_after_migrations(path: &Path) {
        let pool = Store::connect(path, true).await.unwrap();
        migrate(
            &pool,
            path,
            &MIGRATOR,
            versions::RUNNING_VERSION,
            &[DELIVERIES_PENDING],
        )
        .await
        .unwrap();
        pool.close().await;
    }

    async fn current_yaml(store: &Store) -> String {
        sqlx::query_scalar("SELECT yaml FROM project_flows WHERE project_id = 'p1' AND current")
            .fetch_one(store.pool())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn the_skill_rename_runs_on_the_next_start_when_the_first_was_cut_short() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("otaq.db");
        let bare = crate::naming::DEFAULT_FLOW_YAML
            .replace("skill: otaq:", "skill: ")
            .replace("skill: tuq:", "skill: ");
        assert!(bare.contains("skill: work-ticket\n"));
        older_database(&db, SKILL_NAMESPACES - 1, &bare).await;
        killed_after_migrations(&db).await;

        let store = Store::open(&db).await.unwrap();
        let yaml = current_yaml(&store).await;
        assert!(yaml.contains("skill: otaq:work-ticket\n"), "{yaml}");
        assert!(store.skills_namespaced_at().await.unwrap().is_some());
        // Once is once: the next start renames nothing more.
        store.close().await;
        let store = Store::open(&db).await.unwrap();
        let versions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM project_flows")
            .fetch_one(store.pool())
            .await
            .unwrap();
        assert_eq!(versions, 2);
        store.close().await;
    }

    #[tokio::test]
    async fn the_delivery_move_runs_on_the_next_start_when_the_first_was_cut_short() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("otaq.db");
        let old = "flow:\n  - id: build\n    name: Dev\n    kind: agent\n    agent: Developer\n    skill: work-ticket\n  \
                   - name: Done\n    kind: delivery\n    mode: click\n    delivers: local\n";
        assert!(crate::flow::yaml::upgraded(old).is_some());
        older_database(&db, DELIVERY_SETTING - 1, old).await;
        killed_after_migrations(&db).await;

        let store = Store::open(&db).await.unwrap();
        let yaml = current_yaml(&store).await;
        assert!(crate::flow::yaml::upgraded(&yaml).is_none(), "{yaml}");
        assert!(!yaml.contains("kind: delivery"), "{yaml}");
        assert!(!store.has_setting(DELIVERIES_PENDING).await.unwrap());
        store.close().await;
    }
}
