//! The SQLite store (d0003, d0055): one database file in the data folder, migrations from day one.
//!
//! SQL here stays portable to Postgres (see `migrations/0001_initial.sql`). Known SQLite-only details, kept
//! to this module: the connection options (WAL, `foreign_keys`) and `?` placeholders in query text.

pub mod agents;
pub mod projects;
pub mod recents;
pub mod restore;
pub mod settings;
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
        migrate(&pool, path, &MIGRATOR, versions::RUNNING_VERSION).await?;
        let store = Self { pool };
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
/// when migrations are pending on a database that has some applied; the version recorded after.
async fn migrate(
    pool: &SqlitePool,
    path: &Path,
    migrator: &Migrator,
    version: &str,
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
