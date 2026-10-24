//! Seeding a new worktree at start (WRM-04, SES-01 step 3, SES-02; d0082, d0010, d0023).
//!
//! Only a **new** worktree is seeded (not one a returning ticket keeps, not a folder a re-run start reuses), and only
//! while warm builds are on for the project. How depends on the data folder's disk ([`HandoffMode`], d0085):
//!
//! - **Where it can clone** the seed is, in order:
//!   1. the project's warm build, under a reader slot named by the ticket, so a Clear waits for the copy (WRM-06);
//!   2. with none, the user's own checkout, if it has gitignored files. Its env files are left out. A checkout on
//!      another disk can't be cloned ([`CloneError::Unsupported`]) and is skipped: never a plain copy (WRM-01);
//!   3. else nothing.
//! - **Where it can't** (M19-b) the nearest build in the stock ([`Stock::claim_nearest`]) is claimed and **renamed**
//!   into the worktree: it leaves the stock, and what is left of the entry goes to the trash. A rename that fails
//!   (another disk, a file held open) moves back what it moved, so the entry stays in the stock. The checkout is
//!   never used: it can't be cloned there, and renaming would take the user's own build away (plan §4.3.2).
//!
//! The seed's gitignored paths are cloned (or renamed) into the worktree, skipping every path git already checked out
//! there (a folder both have is gone into) and the warm build's record and times file. Then **the timestamp trick**: every
//! tracked file gets the modified time it had where the build was made (the warm build's times file, recorded at
//! Done; or the checkout's own times), and the files that differ from the seed's commit (`git diff --name-only
//! <seed> HEAD`, plus a checkout's own uncommitted changes to tracked files) get the current time, as does a tracked
//! file with no recorded time. Build tools compare source times with output times, so they redo only what differs,
//! and a file committed after the last build still looks newer than its output (Q71). A seed commit git can't
//! resolve, or a warm build without a times file (kept before they were recorded), skips the trick: the files keep
//! their checkout times and the first build redoes more, which is slower but correct.
//!
//! Seeding never fails a start: a failure removes what was cloned, is logged, and the ticket builds from nothing.
//! Nothing here knows any build tool: every gitignored file is treated the same.

use std::collections::HashSet;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use super::clone::{CloneError, Cloner, remove_tree};
use super::store::{Claim, TrackedTimes, WarmBuilds, record_file_name, repo_name, times_file_name};
use crate::board::Board;
use crate::git::Git;
use crate::store::now_ms;
use crate::store::projects::Project;
use crate::ticket_env::handoff::{HandoffMode, Mover};
use crate::ticket_env::stock::Stock;

/// Seeds new worktrees. Cheap to clone.
#[derive(Clone)]
pub struct Seeder {
    warm: WarmBuilds,
    stock: Stock,
    cloner: Arc<dyn Cloner>,
    mover: Arc<dyn Mover>,
}

impl fmt::Debug for Seeder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Seeder")
            .field("cloner", &self.cloner)
            .field("mover", &self.mover)
            .finish_non_exhaustive()
    }
}

/// How a seed's paths get into the worktree.
#[derive(Clone)]
enum Handover {
    Clone(Arc<dyn Cloner>),
    Move(Arc<dyn Mover>),
}

impl Handover {
    fn run(&self, from: &Path, to: &Path, only: &[PathBuf]) -> Result<(), CloneError> {
        match self {
            Self::Clone(cloner) => cloner.clone_tree(from, to, Some(only)),
            Self::Move(mover) => mover.move_tree(from, to, Some(only)),
        }
    }
}

/// Where a worktree was seeded from, for the first line of its setup log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeededFrom {
    /// The project's warm build: its "built at" commit, the ticket it was kept from and when, when it has a record;
    /// the stock entry it came from, and whether it was renamed out of the stock (`moved`) rather than cloned.
    WarmBuild {
        built_at: Option<String>,
        from: Option<String>,
        kept_at: Option<i64>,
        entry: u64,
        moved: bool,
    },
    /// The user's own checkout at this `HEAD` (`None` before its first commit).
    Checkout { head: Option<String> },
}

/// Where a Merge check's scratch worktree was seeded from ([`Seeder::seed_scratch`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScratchSeed {
    /// The ticket's own worktree build.
    Worktree,
    /// The project's warm build.
    WarmBuild,
}

/// A seeded worktree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seeded {
    /// The project's name: `shop-web`.
    pub project: String,
    pub from: SeededFrom,
    /// How long the clone and the timestamp trick took.
    pub took: Duration,
    /// When it was seeded, in milliseconds since the epoch (the warm build's age is measured from it).
    pub at: i64,
}

impl Seeded {
    /// "Started from shop-web's warm build of 3e1f9a2 (SHOP-21, 2 h ago) in 4s.", "Took shop-web's build of 3e1f9a2
    /// (SHOP-21, 2 h ago) in under 1s." (renamed out of the stock) or "Started from your checkout of shop-web
    /// (9c41e7b).".
    pub fn log_line(&self) -> String {
        let project = &self.project;
        match &self.from {
            SeededFrom::WarmBuild {
                built_at,
                from,
                kept_at,
                moved,
                ..
            } => {
                let of = built_at
                    .as_deref()
                    .map(|sha| format!(" of {}", short_sha(sha)))
                    .unwrap_or_default();
                let origin = match (from, kept_at) {
                    (Some(from), Some(kept_at)) => {
                        format!(" ({from}, {})", age(self.at - kept_at))
                    }
                    (Some(from), None) => format!(" ({from})"),
                    (None, Some(kept_at)) => format!(" ({})", age(self.at - kept_at)),
                    (None, None) => String::new(),
                };
                if *moved {
                    format!("Took {project}'s build{of}{origin} in {}.", took(self.took))
                } else {
                    format!(
                        "Started from {project}'s warm build{of}{origin} in {}.",
                        took(self.took)
                    )
                }
            }
            SeededFrom::Checkout { head } => match head {
                Some(head) => format!(
                    "Started from your checkout of {project} ({}).",
                    short_sha(head)
                ),
                None => format!("Started from your checkout of {project}."),
            },
        }
    }
}

impl Seeder {
    /// Seeds with `cloner` where the disk can clone and `mover` where it can't: [`super::clone::FsCloner`] and
    /// [`crate::ticket_env::handoff::FsMover`] in the service; a test stands in for them.
    pub fn new(
        warm: WarmBuilds,
        stock: Stock,
        cloner: Arc<dyn Cloner>,
        mover: Arc<dyn Mover>,
    ) -> Self {
        Self {
            warm,
            stock,
            cloner,
            mover,
        }
    }

    /// Seeds the new `worktree` of `ticket` (`SHOP-18`) in `project`, if warm builds are on and there is a seed.
    /// Where builds are renamed, `own` puts the ticket's own build first (it comes back for it). `None` when nothing
    /// was seeded, for whatever reason; a failure is logged, never returned.
    pub async fn seed(
        &self,
        board: &Board,
        git: &Git,
        project: &Project,
        ticket: &str,
        worktree: &Path,
        own: bool,
    ) -> Option<Seeded> {
        match self.warm.enabled(&project.key).await {
            Ok(true) => {}
            Ok(false) => return None,
            Err(e) => {
                tracing::warn!(ticket, error = %e, "can't tell whether warm builds are on; not seeding");
                return None;
            }
        }
        let env_files = match board.project_settings(&project.key).await {
            Ok(settings) => settings.env_files,
            Err(e) => {
                tracing::warn!(ticket, error = %e, "can't read the project's settings; not seeding");
                return None;
            }
        };
        // Otaq's skill copies are never seeded either ([`plan`] leaves them out of any seed, d0082).
        let leave_out: Vec<PathBuf> = env_files
            .iter()
            .map(|f| f.trim())
            .filter(|f| !f.is_empty())
            .map(PathBuf::from)
            .collect();
        let started = Instant::now();
        let result = match self.warm.mode() {
            HandoffMode::Clone => {
                self.by_clone(git, project, ticket, worktree, leave_out)
                    .await?
            }
            HandoffMode::Move => {
                self.by_move(git, project, ticket, worktree, leave_out, own)
                    .await?
            }
        };
        let seeded = Seeded {
            project: project.name.clone(),
            from: result,
            took: started.elapsed(),
            at: now_ms(),
        };
        tracing::info!(ticket, line = %seeded.log_line(), "worktree seeded");
        Some(seeded)
    }

    /// Where the disk can clone: the project's warm build, else the user's checkout.
    async fn by_clone(
        &self,
        git: &Git,
        project: &Project,
        ticket: &str,
        worktree: &Path,
        leave_out: Vec<PathBuf>,
    ) -> Option<SeededFrom> {
        match self
            .seed_from_warm(git, project, ticket, worktree, leave_out.clone())
            .await
        {
            Some(found) => found,
            None => {
                let repo = PathBuf::from(&project.repo_path);
                let head = self
                    .seed_from_folder(git, &repo, ticket, worktree, leave_out)
                    .await?;
                Some(SeededFrom::Checkout { head })
            }
        }
    }

    /// Clones the project's warm build into `worktree`, under a reader slot named by `ticket`. `None` when there is
    /// no warm build; `Some(None)` when there is one but seeding from it failed (the worktree is as before).
    async fn seed_from_warm(
        &self,
        git: &Git,
        project: &Project,
        ticket: &str,
        worktree: &Path,
        leave_out: Vec<PathBuf>,
    ) -> Option<Option<SeededFrom>> {
        let handover = Handover::Clone(self.cloner.clone());
        let slot = self.warm.read(project, ticket).await?;
        let record = slot.record().cloned();
        let times = read_times(slot.path()).await;
        let top = match list_top(slot.path()) {
            Ok(top) => top,
            Err(e) => {
                tracing::warn!(ticket, error = %e, "can't read the warm build; not seeding");
                return Some(None);
            }
        };
        let plan = hand_over(&handover, slot.path(), worktree, top, leave_out).await;
        let entry = slot.entry();
        // The copy is done: a Clear waiting for it may go on.
        drop(slot);
        let Some(plan) = plan else {
            return Some(None);
        };
        let commit = record.as_ref().map(|r| r.built_at.clone());
        if self
            .trick(git, worktree, commit.as_deref(), times, Vec::new(), ticket)
            .await
            .is_none()
        {
            tracing::warn!(ticket, "the clone is removed");
            undo(&plan, worktree).await;
            return Some(None);
        }
        Some(Some(SeededFrom::WarmBuild {
            built_at: commit,
            from: record.as_ref().map(|r| r.from.clone()),
            kept_at: record.as_ref().map(|r| r.kept_at),
            entry,
            moved: false,
        }))
    }

    /// Clones the gitignored files of `folder` (the user's checkout, or a ticket's worktree) into `worktree`, with
    /// the times `folder`'s tracked files have and its own uncommitted changes marked new. `Some(head)` (`folder`'s
    /// commit, `None` before its first) when something was cloned.
    async fn seed_from_folder(
        &self,
        git: &Git,
        folder: &Path,
        ticket: &str,
        worktree: &Path,
        leave_out: Vec<PathBuf>,
    ) -> Option<Option<String>> {
        let handover = Handover::Clone(self.cloner.clone());
        let repo = folder.to_owned();
        let ignored = match ignored_paths(git, &repo).await {
            Ok(ignored) if ignored.is_empty() => return None,
            Ok(ignored) => ignored,
            Err(e) => {
                tracing::warn!(ticket, error = %e, "can't list the checkout's ignored files; not seeding");
                return None;
            }
        };
        // The checkout's commit and its own changes, read before cloning so they match what is cloned.
        let head = git.resolve(&repo, "HEAD").await.ok().flatten();
        let uncommitted = match &head {
            Some(_) => match git.run(&repo, &["diff", "--name-only", "-z", "HEAD"]).await {
                Ok(out) => split_z(&out),
                Err(e) => {
                    tracing::warn!(ticket, error = %e, "can't list the checkout's changes; not seeding");
                    return None;
                }
            },
            None => Vec::new(),
        };
        // Where the checkout's build was made: its tracked files' own times.
        let times = match git.run(&repo, &["ls-files", "-z"]).await {
            Ok(out) => {
                let (repo, tracked) = (repo.clone(), split_z(&out));
                tokio::task::spawn_blocking(move || TrackedTimes::measure(&repo, &tracked))
                    .await
                    .ok()
            }
            Err(e) => {
                tracing::warn!(ticket, error = %e, "can't list the checkout's files; not seeding");
                return None;
            }
        };
        let plan = hand_over(&handover, &repo, worktree, ignored, leave_out).await?;
        if self
            .trick(git, worktree, head.as_deref(), times, uncommitted, ticket)
            .await
            .is_none()
        {
            tracing::warn!(ticket, "the clone is removed");
            undo(&plan, worktree).await;
            return None;
        }
        Some(head)
    }

    /// Seeds Otaq's scratch worktree before a Merge check (d0120) with a build, copy-on-write only (WRM-01: never a
    /// plain copy, and never a rename, which would take the build away from where it lives): the ticket's own
    /// worktree build (`from`) first, else the project's warm build, else nothing. Only while warm builds are on for
    /// the project; the env files are left out (the caller copies them as a start does). `None` when nothing was
    /// seeded; a failure is logged, never returned.
    pub async fn seed_scratch(
        &self,
        board: &Board,
        git: &Git,
        project: &Project,
        ticket: &str,
        from: Option<&Path>,
        scratch: &Path,
    ) -> Option<ScratchSeed> {
        match self.warm.enabled(&project.key).await {
            Ok(true) => {}
            Ok(false) => return None,
            Err(e) => {
                tracing::warn!(ticket, error = %e, "can't tell whether warm builds are on; not seeding");
                return None;
            }
        }
        if self.warm.mode() != HandoffMode::Clone {
            tracing::info!(
                ticket,
                "the data folder's disk can't clone; the merge check builds from nothing"
            );
            return None;
        }
        let leave_out: Vec<PathBuf> = match board.project_settings(&project.key).await {
            Ok(settings) => settings
                .env_files
                .iter()
                .map(|f| f.trim())
                .filter(|f| !f.is_empty())
                .map(PathBuf::from)
                .collect(),
            Err(e) => {
                tracing::warn!(ticket, error = %e, "can't read the project's settings; not seeding");
                return None;
            }
        };
        if let Some(from) = from.filter(|f| f.is_dir())
            && self
                .seed_from_folder(git, from, ticket, scratch, leave_out.clone())
                .await
                .is_some()
        {
            return Some(ScratchSeed::Worktree);
        }
        match self
            .seed_from_warm(git, project, ticket, scratch, leave_out)
            .await
        {
            Some(Some(_)) => Some(ScratchSeed::WarmBuild),
            _ => None,
        }
    }

    /// Where the disk can't clone (M19-b): the nearest build in the stock for the worktree's commit, claimed and
    /// renamed in. A failed rename leaves the entry in the stock; the checkout is never used.
    async fn by_move(
        &self,
        git: &Git,
        project: &Project,
        ticket: &str,
        worktree: &Path,
        leave_out: Vec<PathBuf>,
        own: bool,
    ) -> Option<SeededFrom> {
        // The base for a new ticket, the rebased commit for a reopened one, the ticket's own when it comes back.
        let target = match git.resolve(worktree, "HEAD").await {
            Ok(Some(target)) => target,
            Ok(None) => return None,
            Err(e) => {
                tracing::warn!(ticket, error = %e, "can't read the worktree's commit; not seeding");
                return None;
            }
        };
        let claim = self
            .stock
            .claim_nearest(project, ticket, &target, own)
            .await?;
        let times = read_times(claim.path()).await;
        let top = match list_top(claim.path()) {
            Ok(top) => top,
            Err(e) => {
                tracing::warn!(ticket, error = %e, "can't read the stock's build; not seeding");
                return None;
            }
        };
        let handover = Handover::Move(self.mover.clone());
        // Failed: the claim goes, and the entry stays in the stock for the next ticket.
        let plan = hand_over(&handover, claim.path(), worktree, top, leave_out).await?;
        let (entry, record) = (claim.entry(), claim.record().cloned());
        let commit = record.as_ref().map(|r| r.built_at.clone());
        // Still claimed: the renamed files are the only copy of the build, so a failed trick moves them back into
        // the entry rather than deleting them.
        let marked = self
            .trick(git, worktree, commit.as_deref(), times, Vec::new(), ticket)
            .await;
        if marked.is_none() {
            let (mover, from, to, back) = (
                self.mover.clone(),
                worktree.to_owned(),
                claim.path().to_owned(),
                plan.clone(),
            );
            let moved_back = tokio::task::spawn_blocking(move || {
                let result = mover.move_tree(&from, &to, Some(&back.paths));
                if result.is_ok() {
                    back.remove_made(&from);
                }
                result
            })
            .await;
            if matches!(moved_back, Ok(Ok(()))) {
                tracing::warn!(ticket, entry, "the build is moved back into the stock");
                return None;
            }
            // It stays in the worktree, where the setup finds it; never deleted.
            tracing::warn!(
                ticket,
                entry,
                "the build can't be moved back into the stock; it stays in the worktree"
            );
            self.taken(claim, project, ticket).await;
            return None;
        }
        self.taken(claim, project, ticket).await;
        Some(SeededFrom::WarmBuild {
            built_at: commit,
            from: record.as_ref().map(|r| r.from.clone()),
            kept_at: record.as_ref().map(|r| r.kept_at),
            entry,
            moved: true,
        })
    }

    /// A claimed entry was renamed into a worktree: what is left of it goes, and the limit is applied again.
    async fn taken(&self, claim: Claim, project: &Project, ticket: &str) {
        self.warm.taken(claim, project);
        // One build fewer, and the ticket no longer waits: the limit may be lower now.
        if let Err(e) = self.stock.enforce_limit(&repo_name(project)).await {
            tracing::warn!(ticket, error = %e, "can't apply the stock limit after a take");
        }
    }

    /// The timestamp trick after a clone or a rename, with the `times` the tracked files had where the build was
    /// made. `None` when it failed (the caller undoes the handover); a seed commit git can't resolve, or no recorded
    /// times, skips it.
    async fn trick(
        &self,
        git: &Git,
        worktree: &Path,
        commit: Option<&str>,
        times: Option<TrackedTimes>,
        mut changed: Vec<String>,
        ticket: &str,
    ) -> Option<()> {
        let resolved = match commit {
            Some(commit) => git.resolve(worktree, commit).await.ok().flatten(),
            None => None,
        };
        let Some(commit) = resolved else {
            tracing::info!(
                ticket,
                ?commit,
                "the seed's commit can't be resolved; files keep their checkout times"
            );
            return Some(());
        };
        let Some(times) = times else {
            tracing::info!(
                ticket,
                "the warm build has no recorded times; files keep their checkout times"
            );
            return Some(());
        };
        let lists = async {
            let tracked = split_z(&git.run(worktree, &["ls-files", "-z"]).await?);
            let diff = git
                .run(worktree, &["diff", "--name-only", "-z", &commit, "HEAD"])
                .await?;
            Ok::<_, crate::git::GitError>((tracked, split_z(&diff)))
        }
        .await;
        let result = match lists {
            Ok((tracked, diff)) => {
                changed.extend(diff);
                let root = worktree.to_owned();
                tokio::task::spawn_blocking(move || mark_times(&root, &tracked, &changed, &times))
                    .await
                    .unwrap_or_else(|e| Err(io::Error::other(e)))
                    .map_err(|e| e.to_string())
            }
            Err(e) => Err(e.to_string()),
        };
        match result {
            Ok(()) => Some(()),
            Err(error) => {
                tracing::warn!(ticket, %error, "the timestamp trick failed");
                None
            }
        }
    }
}

/// Removes a clone's paths from `worktree` ([`Plan::undo`]).
async fn undo(plan: &Plan, worktree: &Path) {
    let (plan, root) = (plan.clone(), worktree.to_owned());
    let _ = tokio::task::spawn_blocking(move || plan.undo(&root)).await;
}

/// Reads the times file of the build in `folder`.
async fn read_times(folder: &Path) -> Option<TrackedTimes> {
    let path = folder.to_owned();
    tokio::task::spawn_blocking(move || TrackedTimes::read(&path))
        .await
        .ok()
        .flatten()
}

/// Clones or renames the `candidates` of `seed` (relative paths) that git didn't check out into `worktree`, leaving
/// out `leave_out`. `None` when there was nothing to hand over or it failed (logged; what it made in the worktree is
/// removed, and a failed rename has moved back what it moved).
async fn hand_over(
    handover: &Handover,
    seed: &Path,
    worktree: &Path,
    candidates: Vec<PathBuf>,
    leave_out: Vec<PathBuf>,
) -> Option<Plan> {
    let handover = handover.clone();
    let moving = matches!(handover, Handover::Move(_));
    let (seed, worktree) = (seed.to_owned(), worktree.to_owned());
    let shown = seed.clone();
    let result = tokio::task::spawn_blocking(move || {
        let plan =
            plan(&seed, &worktree, candidates, &leave_out).map_err(|error| CloneError::Io {
                path: seed.clone(),
                error,
            })?;
        if plan.paths.is_empty() {
            plan.remove_made(&worktree);
            return Ok(None);
        }
        match handover.run(&seed, &worktree, &plan.paths) {
            Ok(()) => Ok(Some(plan)),
            Err(e) if moving => {
                // A failed rename moved back what it moved: whatever is at a planned path now isn't the handover's
                // (it appeared since the plan, or is a build that couldn't be moved back, its only copy), so only
                // the plan's own empty folders go.
                plan.remove_made(&worktree);
                Err(e)
            }
            Err(e) => {
                // A clone: what is at the planned paths is a copy (the stock or the checkout keeps the original).
                plan.undo(&worktree);
                Err(e)
            }
        }
    })
    .await;
    match result {
        Ok(Ok(plan)) => plan,
        Ok(Err(CloneError::Unsupported { path })) if moving => {
            tracing::info!(
                seed = %shown.display(),
                path = %path.display(),
                "the build can't be renamed into the worktree (another disk); not seeding"
            );
            None
        }
        Ok(Err(CloneError::Unsupported { path })) => {
            tracing::info!(
                seed = %shown.display(),
                path = %path.display(),
                "the seed can't be cloned copy-on-write (another disk); not seeding"
            );
            None
        }
        Ok(Err(e)) if moving => {
            tracing::warn!(seed = %shown.display(), error = %e, "renaming the build failed; it stays in the stock and the worktree builds from nothing");
            None
        }
        Ok(Err(e)) => {
            tracing::warn!(seed = %shown.display(), error = %e, "seeding failed; the worktree builds from nothing");
            None
        }
        Err(e) => {
            tracing::warn!(seed = %shown.display(), error = %e, "seeding stopped");
            None
        }
    }
}

/// What a seed clones (or renames): `paths` (relative), and the folders that had to be made for them because an env file
/// inside was left out (removed with them on a failure).
#[derive(Debug, Clone, Default)]
pub(crate) struct Plan {
    pub(crate) paths: Vec<PathBuf>,
    pub(crate) made: Vec<PathBuf>,
}

impl Plan {
    /// Removes everything this plan cloned into `worktree`. None of it was there before (git didn't check it out).
    /// Never for renamed paths: they are the only copy of the build.
    pub(crate) fn undo(&self, worktree: &Path) {
        for path in self.paths.iter().chain(self.made.iter().rev()) {
            remove_tree(&worktree.join(path));
        }
    }

    /// Removes the folders this plan made in `worktree` (for a left-out env file inside), while they are empty.
    pub(crate) fn remove_made(&self, worktree: &Path) {
        for path in self.made.iter().rev() {
            let _ = fs::remove_dir(worktree.join(path));
        }
    }
}

/// The paths of `seed` to clone into `worktree`: each candidate that isn't there yet, and inside a folder both
/// have, its entries the same way. `leave_out` (env files) is never cloned, nor are Otaq's skill copies in the seed
/// (a checkout holds them while a Project agent runs, AGM-07; each session writes its own); a folder holding one is
/// gone into.
pub(crate) fn plan(
    seed: &Path,
    worktree: &Path,
    candidates: Vec<PathBuf>,
    leave_out: &[PathBuf],
) -> io::Result<Plan> {
    let mut leave_out = leave_out.to_vec();
    leave_out.extend(crate::skills::copy::copies_to_skip(seed));
    let leave_out = leave_out.as_slice();
    let mut plan = Plan::default();
    let mut todo = candidates;
    todo.reverse();
    while let Some(rel) = todo.pop() {
        if leave_out.contains(&rel) {
            continue;
        }
        let Ok(source) = fs::symlink_metadata(seed.join(&rel)) else {
            continue;
        };
        let holds_left_out = source.is_dir() && leave_out.iter().any(|l| l.starts_with(&rel));
        let into = match fs::symlink_metadata(worktree.join(&rel)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if !holds_left_out {
                    plan.paths.push(rel);
                    continue;
                }
                plan.made.push(rel.clone());
                fs::create_dir_all(worktree.join(&rel))?;
                rel
            }
            Err(e) => return Err(e),
            Ok(there) if there.is_dir() && source.is_dir() => rel,
            // Git checked it out: the worktree's own file stays.
            Ok(_) => continue,
        };
        let mut children: Vec<PathBuf> = fs::read_dir(seed.join(&into))?
            .map(|entry| entry.map(|e| into.join(e.file_name())))
            .collect::<io::Result<_>>()?;
        children.sort();
        children.reverse();
        todo.extend(children);
    }
    Ok(plan)
}

/// The entries at the top of a warm build, without its record and times file.
pub(crate) fn list_top(folder: &Path) -> io::Result<Vec<PathBuf>> {
    let own = [record_file_name(), times_file_name()];
    let mut top: Vec<PathBuf> = fs::read_dir(folder)?
        .filter_map(|entry| entry.ok().map(|e| e.file_name()))
        .filter(|name| !own.iter().any(|own| name.to_string_lossy() == own.as_str()))
        .map(PathBuf::from)
        .collect();
    top.sort();
    Ok(top)
}

/// The checkout's gitignored paths, a wholly ignored folder as one path.
async fn ignored_paths(git: &Git, repo: &Path) -> Result<Vec<PathBuf>, crate::git::GitError> {
    let out = git
        .run(
            repo,
            &[
                "ls-files",
                "-z",
                "--others",
                "--ignored",
                "--exclude-standard",
                "--directory",
            ],
        )
        .await?;
    Ok(split_z(&out)
        .into_iter()
        .map(|p| PathBuf::from(p.trim_end_matches('/')))
        .filter(|p| !p.as_os_str().is_empty())
        .collect())
}

/// The paths of a `-z` git listing.
pub(crate) fn split_z(out: &str) -> Vec<String> {
    out.split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Every tracked file gets its time in `times`, except the changed ones and those without a recorded time, which get
/// the current time. A file that isn't there is passed over.
pub(crate) fn mark_times(
    root: &Path,
    tracked: &[String],
    changed: &[String],
    times: &TrackedTimes,
) -> io::Result<()> {
    let now = SystemTime::now();
    let changed: HashSet<&str> = changed.iter().map(String::as_str).collect();
    for file in tracked {
        let time = match times.get(file) {
            Some(time) if !changed.contains(file.as_str()) => time,
            _ => now,
        };
        match set_time(&root.join(file), time) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            other => other?,
        }
    }
    Ok(())
}

/// Sets the access and modified times of `path` itself (a symlink is not followed).
#[cfg(unix)]
#[allow(clippy::unnecessary_cast)]
fn set_time(path: &Path, time: SystemTime) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let c = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let since = time
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    // SAFETY: `timespec` is plain data; every field the call reads is set below.
    let mut times: [libc::timespec; 2] = unsafe { std::mem::zeroed() };
    for t in &mut times {
        t.tv_sec = since.as_secs() as libc::time_t;
        t.tv_nsec = since.subsec_nanos() as _;
    }
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

/// Sets the access and modified times of `path`. Symlinks are passed over: std can't set a link's own times
/// here, and following one would touch a file outside the trick.
#[cfg(windows)]
fn set_time(path: &Path, time: SystemTime) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_WRITE_ATTRIBUTES,
    };
    if fs::symlink_metadata(path)?.file_type().is_symlink() {
        return Ok(());
    }
    let file = fs::OpenOptions::new()
        .access_mode(FILE_WRITE_ATTRIBUTES)
        // Lets a folder (a submodule) be opened too.
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    file.set_times(fs::FileTimes::new().set_accessed(time).set_modified(time))
}

fn short_sha(sha: &str) -> String {
    sha.chars().take(7).collect()
}

/// "just now", "12 min ago", "2 h ago", "3 d ago".
fn age(ms: i64) -> String {
    let minutes = ms.max(0) / 60_000;
    match minutes {
        0 => "just now".to_owned(),
        1..60 => format!("{minutes} min ago"),
        60..1440 => format!("{} h ago", minutes / 60),
        _ => format!("{} d ago", minutes / 1440),
    }
}

/// "under 1s", "4s", "2 min".
fn took(d: Duration) -> String {
    match d.as_secs() {
        0 => "under 1s".to_owned(),
        s @ 1..60 => format!("{s}s"),
        s => format!("{} min", s / 60),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded(from: SeededFrom, took: Duration) -> Seeded {
        Seeded {
            project: "shop-web".into(),
            from,
            took,
            at: 10 * 3_600_000,
        }
    }

    #[test]
    fn the_log_line_says_where_the_seed_came_from() {
        let warm = SeededFrom::WarmBuild {
            built_at: Some("3e1f9a2c0ffee".into()),
            from: Some("SHOP-21".into()),
            kept_at: Some(8 * 3_600_000),
            entry: 3,
            moved: false,
        };
        assert_eq!(
            seeded(warm, Duration::from_millis(4_300)).log_line(),
            "Started from shop-web's warm build of 3e1f9a2 (SHOP-21, 2 h ago) in 4s."
        );
        // Renamed out of the stock (a disk that can't clone).
        let taken = SeededFrom::WarmBuild {
            built_at: Some("3e1f9a2c0ffee".into()),
            from: Some("SHOP-21".into()),
            kept_at: Some(8 * 3_600_000),
            entry: 3,
            moved: true,
        };
        assert_eq!(
            seeded(taken, Duration::from_millis(100)).log_line(),
            "Took shop-web's build of 3e1f9a2 (SHOP-21, 2 h ago) in under 1s."
        );
        let bare = SeededFrom::WarmBuild {
            built_at: None,
            from: None,
            kept_at: None,
            entry: 0,
            moved: false,
        };
        assert_eq!(
            seeded(bare, Duration::from_millis(200)).log_line(),
            "Started from shop-web's warm build in under 1s."
        );
        let checkout = SeededFrom::Checkout {
            head: Some("9c41e7bdeadbeef".into()),
        };
        assert_eq!(
            seeded(checkout, Duration::from_secs(3)).log_line(),
            "Started from your checkout of shop-web (9c41e7b)."
        );
    }

    #[test]
    fn ages_and_durations_read_short() {
        assert_eq!(age(10_000), "just now");
        assert_eq!(age(12 * 60_000), "12 min ago");
        assert_eq!(age(3 * 86_400_000), "3 d ago");
        assert_eq!(took(Duration::from_secs(150)), "2 min");
    }

    /// A rename that fails because a planned path appeared in the worktree meanwhile (a session or the user made
    /// it): what is there now is never deleted, and the plan's own empty folder goes.
    #[derive(Debug)]
    struct AppearsThenFails;

    impl Mover for AppearsThenFails {
        fn move_tree(
            &self,
            _from: &Path,
            to: &Path,
            only: Option<&[PathBuf]>,
        ) -> Result<(), CloneError> {
            let first = to.join(&only.unwrap()[0]);
            fs::create_dir_all(first.parent().unwrap()).unwrap();
            fs::write(&first, "someone else's").unwrap();
            Err(CloneError::Io {
                path: first,
                error: io::Error::from(io::ErrorKind::AlreadyExists),
            })
        }
    }

    #[tokio::test]
    async fn a_failed_handover_never_deletes_what_appeared_meanwhile() {
        let dir = tempfile::tempdir().unwrap();
        let (seed, wt) = (dir.path().join("seed"), dir.path().join("wt"));
        for f in ["out/app.bin", "config/cache", "config/.env.local"] {
            let path = seed.join(f);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "x").unwrap();
        }
        fs::create_dir_all(&wt).unwrap();
        let handover = Handover::Move(Arc::new(AppearsThenFails));
        let candidates = ["config", "out"].map(PathBuf::from).to_vec();
        let leave_out = vec![PathBuf::from("config/.env.local")];

        let plan = hand_over(&handover, &seed, &wt, candidates, leave_out).await;

        assert!(plan.is_none());
        assert_eq!(
            fs::read_to_string(wt.join("config/cache")).unwrap(),
            "someone else's"
        );
        assert!(!wt.join("out").exists());
        assert!(
            seed.join("out/app.bin").exists(),
            "the build stays in the stock"
        );
    }

    #[test]
    fn the_plan_skips_what_git_checked_out_and_the_env_files() {
        let dir = tempfile::tempdir().unwrap();
        let (seed, wt) = (dir.path().join("seed"), dir.path().join("wt"));
        for f in [
            "out/app.bin",
            "src/gen.o",
            "src/main.c",
            "config/.env.local",
            "config/cache",
        ] {
            let path = seed.join(f);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "x").unwrap();
        }
        fs::create_dir_all(wt.join("src")).unwrap();
        fs::write(wt.join("src/main.c"), "tracked").unwrap();
        let candidates = ["config", "out", "src"].map(PathBuf::from).to_vec();
        let leave_out = [PathBuf::from("config/.env.local")];
        let plan = plan(&seed, &wt, candidates, &leave_out).unwrap();
        assert_eq!(
            plan.paths,
            ["config/cache", "out", "src/gen.o"]
                .map(PathBuf::from)
                .to_vec()
        );
        assert_eq!(plan.made, vec![PathBuf::from("config")]);
        plan.undo(&wt);
        assert!(!wt.join("config").exists());
        assert!(wt.join("src/main.c").exists());
    }
}
