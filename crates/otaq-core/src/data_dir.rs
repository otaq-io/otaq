//! The data folder, `~/.otaq/` on every platform (d0055): the database, `wt/<repo-name>/<KEY>` worktrees,
//! `warm/<repo-name>` warm builds (d0082), `skills/<id>` imported workflow skills (d0092), `reports/<repo-name>/<KEY>` comment reports (d0109), `logs/` and the service socket. Nothing is ever written into the user's repositories (d0023).
//!
//! `OTAQ_HOME` points the whole folder somewhere else. Tests always set it to a temporary directory, so
//! they never read or write the real `~/.otaq/`.

use std::ffi::OsString;
use std::fmt;
use std::io;
use std::path::{Path, PathBuf};

use crate::domain::TicketKey;
use crate::naming;

/// Where Otaq keeps its data. Only builds paths; folders are created with [`DataDir::ensure_dir`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataDir {
    root: PathBuf,
}

impl DataDir {
    /// A data folder at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `OTAQ_HOME` if it is set and not empty, otherwise `~/.otaq`.
    pub fn from_env() -> Result<Self, DataDirError> {
        Self::resolve(std::env::var_os(Self::env_var()), std::env::home_dir())
    }

    fn resolve(
        override_dir: Option<OsString>,
        user_home: Option<PathBuf>,
    ) -> Result<Self, DataDirError> {
        if let Some(dir) = override_dir.filter(|v| !v.is_empty()) {
            return Ok(Self::new(dir));
        }
        let user_home = user_home.ok_or(DataDirError::NoHomeFolder)?;
        Ok(Self::new(user_home.join(naming::data_dir_name())))
    }

    /// The variable that overrides the data folder: `OTAQ_HOME`.
    pub fn env_var() -> String {
        naming::env_var("HOME")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The SQLite database file. The file name is an implementation choice (SDD §26).
    pub fn database_path(&self) -> PathBuf {
        self.root.join(format!("{}.db", naming::COMMAND_NAME))
    }

    /// The folder holding every worktree, grouped by repository.
    pub fn worktrees_dir(&self) -> PathBuf {
        self.root.join("wt")
    }

    /// A ticket's worktree: `wt/<repo-name>/<KEY>`, for example `wt/shop-web/SHOP-40.1`.
    pub fn worktree_path(&self, repo_name: &str, key: &str) -> Result<PathBuf, DataDirError> {
        let repo = path_component(repo_name)?;
        let key = path_component(key)?;
        Ok(self.worktrees_dir().join(repo).join(key))
    }

    /// The folder holding every project's warm build (WRM-02), next to `wt/` so they share a disk.
    pub fn warm_dir(&self) -> PathBuf {
        self.root.join("warm")
    }

    /// A project's warm build: `warm/<repo-name>`, for example `warm/shop-web`.
    pub fn warm_path(&self, repo_name: &str) -> Result<PathBuf, DataDirError> {
        let repo = warm_component(repo_name)?;
        Ok(self.warm_dir().join(repo))
    }

    /// One entry of a project's build stock (M19-a, d0085): `warm/<repo-name>/<n>`, for example `warm/shop-web/0`.
    pub fn warm_entry_path(&self, repo_name: &str, n: u64) -> Result<PathBuf, DataDirError> {
        Ok(self.warm_path(repo_name)?.join(n.to_string()))
    }

    /// Where a project's stock entries go to be deleted in the background: `warm/<repo-name>.trash`, next to the
    /// stock so a rename into it never crosses a disk.
    pub fn warm_trash_path(&self, repo_name: &str) -> Result<PathBuf, DataDirError> {
        let repo = warm_component(repo_name)?;
        Ok(self.warm_dir().join(format!("{repo}.trash")))
    }

    /// A ticket's own resource folder (d0087): `res/<repo-name>/<KEY>`, for example `res/shop-web/SHOP-18`, set as
    /// `OTAQ_RESOURCE_DIR` where the project has ticket resources. Otaq-owned, like `wt/`.
    pub fn resource_path(&self, repo_name: &str, key: &TicketKey) -> Result<PathBuf, DataDirError> {
        let repo = path_component(repo_name)?;
        let key = key.to_string();
        let key = path_component(&key)?;
        Ok(self
            .root
            .join(naming::resource_dir_name())
            .join(repo)
            .join(key))
    }

    /// The folder holding every comment's report (CMT-03, d0109), grouped by repository and ticket.
    pub fn reports_dir(&self) -> PathBuf {
        self.root.join("reports")
    }

    /// The report of one ticket comment (CMT-03): a copied file keeps its own name in its comment's folder,
    /// `reports/<repo-name>/<KEY>/<comment id>/<name>` (`reports/shop-web/SHOP-34/0f3a…/test-run-login.md`, RPT-02);
    /// pasted text has none, `reports/<repo-name>/<KEY>/<comment id>.txt`. Otaq's own copy, never in the repository
    /// (d0023).
    pub fn report_path(
        &self,
        repo_name: &str,
        key: &TicketKey,
        comment_id: &str,
        name: Option<&str>,
    ) -> Result<PathBuf, DataDirError> {
        let repo = path_component(repo_name)?;
        let key = key.to_string();
        let key = path_component(&key)?;
        let id = path_component(comment_id)?;
        let folder = self.reports_dir().join(repo).join(key);
        Ok(match name {
            Some(name) => folder.join(id).join(path_component(name)?),
            None => folder.join(format!("{id}.txt")),
        })
    }

    /// Otaq's own scratch worktree for a repository (`scratch/<repo-name>`), where Local merges happen,
    /// never in the user's folder (d0038).
    pub fn scratch_path(&self, repo_name: &str) -> Result<PathBuf, DataDirError> {
        let repo = path_component(repo_name)?;
        Ok(self.root.join("scratch").join(repo))
    }

    /// The folder holding Otaq's copies of imported workflow skills (WSK-03, d0092), one folder per setting.
    pub fn skills_dir(&self) -> PathBuf {
        self.root.join("skills")
    }

    /// Otaq's copy of one imported workflow skill: `skills/<id>`, the id being Otaq's own (never the skill's name).
    pub fn skill_path(&self, id: &str) -> Result<PathBuf, DataDirError> {
        let id = path_component(id)?;
        Ok(self.skills_dir().join(id))
    }

    /// The folder a Project agent writes a stage's skill in (FED-06): `skill-writes/<KEY>/<stage id>`, Otaq's own,
    /// never the repo (d0023). Otaq imports it as its copy under [`Self::skills_dir`] and deletes it.
    pub fn skill_writing_path(&self, project: &str, stage: &str) -> Result<PathBuf, DataDirError> {
        let project = path_component(project)?;
        let stage = path_component(stage)?;
        Ok(self.root.join("skill-writes").join(project).join(stage))
    }

    /// The log folder; files rotate daily (see [`crate::logging`]).
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    /// A ticket's setup-command output (SES-01 step 5): `logs/setup/<KEY>.log`, replaced on every start.
    pub fn setup_log_path(&self, key: &str) -> Result<PathBuf, DataDirError> {
        let key = path_component(key)?;
        Ok(self.logs_dir().join("setup").join(format!("{key}.log")))
    }

    /// A ticket's last Merge check output (d0120): `logs/merge-check/<KEY>.log`, replaced by every check.
    pub fn merge_check_log_path(&self, key: &str) -> Result<PathBuf, DataDirError> {
        let key = path_component(key)?;
        Ok(self
            .logs_dir()
            .join("merge-check")
            .join(format!("{key}.log")))
    }

    /// The output of a project's ticket resource Remove and Template refresh commands (d0087):
    /// `logs/resources-<repo-name>.log`, appended to.
    pub fn resource_log_path(&self, repo_name: &str) -> Result<PathBuf, DataDirError> {
        let repo = path_component(repo_name)?;
        Ok(self.logs_dir().join(format!("resources-{repo}.log")))
    }

    /// The service socket (a Unix domain socket on macOS and Linux).
    pub fn socket_path(&self) -> PathBuf {
        self.root.join(format!("{}.sock", naming::COMMAND_NAME))
    }

    /// The service's lock file: locked while a service runs (so only one can), holding its PID.
    pub fn service_lock_path(&self) -> PathBuf {
        self.root.join("service.lock")
    }

    /// Creates `path` and any missing parents. New folders are readable only by the current user.
    pub fn ensure_dir(path: &Path) -> io::Result<()> {
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(path)
    }
}

/// A repository name or ticket key must be one plain path component, so a worktree path never escapes `wt/`.
fn path_component(part: &str) -> Result<&str, DataDirError> {
    let invalid = part.is_empty()
        || part == "."
        || part == ".."
        || part.contains(['/', '\\'])
        || part.contains('\0');
    if invalid {
        Err(DataDirError::InvalidPathPart(part.to_owned()))
    } else {
        Ok(part)
    }
}

/// The suffixes of the warm store's own folders next to a stock: `warm/<repo-name>.incoming`, `.old`, `.trash`, `.mig`.
pub(crate) const WARM_SIBLING_SUFFIXES: [&str; 4] = [".incoming", ".old", ".trash", ".mig"];

/// A repository name for `warm/`: a [`path_component`] that doesn't end with one of [`WARM_SIBLING_SUFFIXES`] (any
/// case, as Windows and macOS compare names), so a stock never shares a folder with another project's trash or
/// migration (`app.trash` is `app`'s trash). Such a project has no warm builds.
fn warm_component(part: &str) -> Result<&str, DataDirError> {
    let part = path_component(part)?;
    let lower = part.to_ascii_lowercase();
    if WARM_SIBLING_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
        Err(DataDirError::InvalidPathPart(part.to_owned()))
    } else {
        Ok(part)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataDirError {
    /// No home folder could be found and the override variable isn't set.
    NoHomeFolder,
    /// A repository name or key that isn't a single path component.
    InvalidPathPart(String),
}

impl fmt::Display for DataDirError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoHomeFolder => write!(
                f,
                "no home folder found; set {} to choose the data folder",
                DataDir::env_var()
            ),
            Self::InvalidPathPart(part) => write!(f, "{part:?} can't be used as a folder name"),
        }
    }
}

impl std::error::Error for DataDirError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_data_dir() -> (tempfile::TempDir, DataDir) {
        let tmp = tempfile::tempdir().unwrap();
        let data = DataDir::new(tmp.path().join("home"));
        (tmp, data)
    }

    #[test]
    fn paths_live_under_the_root() {
        let (tmp, data) = temp_data_dir();
        let root = tmp.path().join("home");
        assert_eq!(data.root(), root);
        let name = naming::COMMAND_NAME;
        assert_eq!(data.database_path(), root.join(format!("{name}.db")));
        assert_eq!(data.worktrees_dir(), root.join("wt"));
        assert_eq!(data.logs_dir(), root.join("logs"));
        assert_eq!(data.socket_path(), root.join(format!("{name}.sock")));
        assert_eq!(data.service_lock_path(), root.join("service.lock"));
    }

    #[test]
    fn worktree_paths_use_repo_name_and_key() {
        let (tmp, data) = temp_data_dir();
        let wt = tmp.path().join("home").join("wt");
        assert_eq!(
            data.worktree_path("shop-web", "SHOP-18").unwrap(),
            wt.join("shop-web").join("SHOP-18")
        );
        assert_eq!(
            data.worktree_path("shop-web", "SHOP-40.1").unwrap(),
            wt.join("shop-web").join("SHOP-40.1")
        );
    }

    #[test]
    fn ticket_resources_have_a_folder_and_a_log() {
        let (tmp, data) = temp_data_dir();
        let root = tmp.path().join("home");
        let key: TicketKey = "SHOP-40.1".parse().unwrap();
        assert_eq!(
            data.resource_path("shop-web", &key).unwrap(),
            root.join(naming::resource_dir_name())
                .join("shop-web")
                .join("SHOP-40.1")
        );
        assert!(data.resource_path("a/b", &key).is_err());
        assert_eq!(
            data.resource_log_path("shop-web").unwrap(),
            root.join("logs").join("resources-shop-web.log")
        );
    }

    #[test]
    fn warm_builds_live_next_to_the_worktrees() {
        let (tmp, data) = temp_data_dir();
        let root = tmp.path().join("home");
        assert_eq!(data.warm_dir(), root.join("warm"));
        assert_eq!(data.warm_dir().parent(), data.worktrees_dir().parent());
        assert_eq!(
            data.warm_path("shop-web").unwrap(),
            root.join("warm").join("shop-web")
        );
        for bad in ["", ".", "..", "a/b", "a\\b", "x\0y"] {
            assert_eq!(
                data.warm_path(bad),
                Err(DataDirError::InvalidPathPart(bad.to_owned()))
            );
        }
    }

    #[test]
    fn stock_entries_and_trash_live_under_warm() {
        let (tmp, data) = temp_data_dir();
        let warm = tmp.path().join("home").join("warm");
        assert_eq!(
            data.warm_entry_path("shop-web", 0).unwrap(),
            warm.join("shop-web").join("0")
        );
        assert_eq!(
            data.warm_entry_path("shop-web", 12).unwrap(),
            warm.join("shop-web").join("12")
        );
        assert_eq!(
            data.warm_trash_path("shop-web").unwrap(),
            warm.join("shop-web.trash")
        );
        for bad in [
            "",
            ".",
            "..",
            "a/b",
            "a\\b",
            "x\0y",
            "app.trash",
            "App.Trash",
            "app.mig",
            "app.incoming",
            "app.old",
        ] {
            assert_eq!(
                data.warm_entry_path(bad, 0),
                Err(DataDirError::InvalidPathPart(bad.to_owned()))
            );
            assert_eq!(
                data.warm_trash_path(bad),
                Err(DataDirError::InvalidPathPart(bad.to_owned()))
            );
        }
    }

    #[test]
    fn worktree_paths_never_escape_the_folder() {
        let (_tmp, data) = temp_data_dir();
        for bad in ["", ".", "..", "a/b", "a\\b", "x\0y"] {
            assert_eq!(
                data.worktree_path(bad, "SHOP-1"),
                Err(DataDirError::InvalidPathPart(bad.to_owned()))
            );
            assert!(data.worktree_path("shop-web", bad).is_err());
        }
    }

    #[test]
    fn ensure_dir_creates_user_only_folders() {
        let (_tmp, data) = temp_data_dir();
        let wt = data.worktree_path("shop-web", "SHOP-18").unwrap();
        DataDir::ensure_dir(&wt).unwrap();
        assert!(wt.is_dir());
        // Creating it again is fine.
        DataDir::ensure_dir(&wt).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for dir in [data.root().to_path_buf(), data.worktrees_dir(), wt] {
                let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
                assert_eq!(mode, 0o700, "{}", dir.display());
            }
        }
    }

    #[test]
    fn override_variable_name() {
        assert_eq!(DataDir::env_var(), naming::env_var("HOME"));
    }

    #[test]
    fn the_override_wins_over_the_home_folder() {
        let data = DataDir::resolve(Some("/tmp/cb".into()), Some("/Users/me".into())).unwrap();
        assert_eq!(data.root(), Path::new("/tmp/cb"));
    }

    #[test]
    fn default_is_the_hidden_folder_in_home() {
        let expected = Path::new("/Users/me").join(naming::data_dir_name());
        for empty in [None, Some(OsString::new())] {
            let data = DataDir::resolve(empty, Some("/Users/me".into())).unwrap();
            assert_eq!(data.root(), expected);
        }
        assert_eq!(
            DataDir::resolve(None, None),
            Err(DataDirError::NoHomeFolder)
        );
    }
}
