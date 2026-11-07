//! The stock of builds per repository (M19-a, d0085), on top of the warm build store ([`WarmBuilds`]):
//!
//! - **The limit** ([`Stock::limit`]): where builds are renamed ([`HandoffMode::Move`]), a stock keeps as many
//!   builds as the repository's tickets that could start soon: those waiting to begin whose dependencies are done and
//!   that wait only for a slot or the plan limit ([`scheduler::stock_limit`]); at least 1. Where builds are cloned
//!   ([`HandoffMode::Clone`]) it keeps one, forward only, as before.
//! - **The nearest build** ([`Stock::claim_nearest`]): the ticket's own build first; then a build made at an ancestor
//!   of the ticket's commit, with the fewest changed files; then the rest, nearest to the base branch's tip first;
//!   among equals, the one kept last. The first one no other ticket has claimed is claimed.
//! - **Eviction** ([`Stock::enforce_limit`]): over the limit, the builds farthest from the base branch's tip go to
//!   the trash first (a build whose commit git can't find counts as the farthest), then the oldest. A claimed build
//!   is never evicted. At most one pass per repository runs at a time; asked for meanwhile, it goes once more.
//! - **Add** ([`Stock::add`]): where builds are cloned, the store's forward-only replace; where they are renamed,
//!   always added as the next entry, then the limit is applied.
//!
//! Git runs in the project's repository, never under the store's lock.

use std::cmp::Reverse;
use std::collections::HashMap;
use std::path::Path;

use crate::board::{Board, BoardError};
use crate::git::Git;
use crate::scheduler;
use crate::store::projects::Project;
use crate::ticket_env::handoff::HandoffMode;
use crate::warm::seed::split_z;
use crate::warm::store::{Claim, Entry, Incoming, WarmBuilds, WarmError, WarmRecord, repo_name};

/// What [`Stock::add`] (and [`WarmBuilds::replace`]) did with a new build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Added {
    /// It is in the stock now, as this entry.
    Kept { entry: u64 },
    /// Dropped: where builds are cloned, the build there is newer or belongs to another project; or warm builds are
    /// off now.
    Dropped,
    /// Added as this entry, then evicted at once: it was the farthest from the base branch's tip with the stock
    /// over its limit.
    Evicted { entry: u64 },
}

/// One entry, for [`rank`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) n: u64,
    /// The ticket it was kept from; empty without a record.
    pub(crate) from: String,
    /// The commit it was built at; empty without a record.
    pub(crate) built_at: String,
    pub(crate) kept_at: i64,
}

/// How well an entry fits a ticket, best first; within a kind, fewer changed files first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Fit {
    /// The ticket's own build.
    Own,
    /// Built at an ancestor of the ticket's commit, this many files changed since.
    Ancestor(usize),
    /// Built elsewhere, this many files from the base branch's tip (`usize::MAX`: unknown).
    Other(usize),
}

/// The entry numbers of `cands`, best fit first ([`Fit`]'s order), then the one kept last.
pub(crate) fn rank(mut cands: Vec<(Candidate, Fit)>) -> Vec<u64> {
    cands.sort_by_key(|(c, fit)| (*fit, Reverse(c.kept_at), c.n));
    cands.into_iter().map(|(c, _)| c.n).collect()
}

/// The entries to evict to get `entries` (`(entry, distance to the base branch's tip, kept at)`) down to `limit`:
/// the farthest first, an unknown distance being the farthest; among equals, the oldest.
pub(crate) fn evictions(entries: &[(u64, Option<usize>, i64)], limit: usize) -> Vec<u64> {
    let over = entries.len().saturating_sub(limit);
    let mut order: Vec<&(u64, Option<usize>, i64)> = entries.iter().collect();
    order.sort_by_key(|(n, distance, kept_at)| {
        (Reverse(distance.unwrap_or(usize::MAX)), *kept_at, *n)
    });
    order.into_iter().take(over).map(|(n, _, _)| *n).collect()
}

/// The stock limit of `warm/<repo>` ([`Stock::limit`]) for a disk handing builds over by `mode`.
pub(crate) async fn limit_of(
    board: &Board,
    mode: HandoffMode,
    repo: &str,
) -> Result<usize, BoardError> {
    if mode == HandoffMode::Clone {
        return Ok(1);
    }
    let mut tickets = Vec::new();
    for project in repo_projects(board, repo).await? {
        tickets.extend(board.tickets(&project.key).await?);
    }
    // Readiness only looks at a ticket's own project, and keys carry the project, so one list does for all.
    Ok(scheduler::stock_limit(&tickets, &tickets))
}

/// The projects whose stock is `warm/<repo>`, by key.
async fn repo_projects(board: &Board, repo: &str) -> Result<Vec<Project>, BoardError> {
    let mut projects: Vec<Project> = board
        .projects()
        .await?
        .into_iter()
        .filter(|p| repo_name(p) == repo)
        .collect();
    projects.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(projects)
}

/// The stock of builds of every repository. Cheap to clone.
#[derive(Clone)]
pub struct Stock {
    warm: WarmBuilds,
    board: Board,
}

impl std::fmt::Debug for Stock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stock")
            .field("warm", &self.warm)
            .finish_non_exhaustive()
    }
}

impl Stock {
    pub fn new(warm: WarmBuilds, board: Board) -> Self {
        Self { warm, board }
    }

    /// How many builds `warm/<repo>` keeps: 1 where builds are cloned; else the tickets that could start soon
    /// ([`scheduler::stock_limit`]) across every project whose stock it is, at least 1.
    pub async fn limit(&self, repo: &str) -> Result<usize, BoardError> {
        limit_of(&self.board, self.warm.mode(), repo).await
    }

    /// The project's entries, best fit for a ticket at commit `target` first ([`Fit`]); `own` is the ticket's key,
    /// when its own build counts.
    pub async fn ranked(&self, project: &Project, target: &str, own: Option<&str>) -> Vec<u64> {
        let entries = self.warm.entries(project).await;
        self.rank_entries(project, &entries, target, own).await
    }

    /// The nearest entry for `ticket` at commit `target` ([`ranked`](Self::ranked); with `own`, its own build first),
    /// claimed for it: the next one when another ticket claimed it first. `None` when the stock has none left for it.
    pub async fn claim_nearest(
        &self,
        project: &Project,
        ticket: &str,
        target: &str,
        own: bool,
    ) -> Option<Claim> {
        let repo = repo_name(project);
        let entries = self.warm.entries(project).await;
        let order = self
            .rank_entries(project, &entries, target, own.then_some(ticket))
            .await;
        order.into_iter().find_map(|n| {
            let entry = entries.iter().find(|e| e.n == n)?.clone();
            self.warm.claim_entry(&repo, entry, ticket)
        })
    }

    /// Adds the build in `incoming` to the project's stock. Where builds are cloned: the store's forward-only
    /// replace (one entry). Where they are renamed: always added as the next entry, then the limit applied
    /// ([`enforce_limit`](Self::enforce_limit)); dropped only when warm builds are off for the project.
    pub async fn add(
        &self,
        project: &Project,
        incoming: Incoming,
        record: WarmRecord,
    ) -> Result<Added, WarmError> {
        if self.warm.mode() == HandoffMode::Clone {
            return self.warm.replace(project, incoming, record).await;
        }
        if !self.warm.enabled(&project.key).await? {
            tracing::info!(project = %project.key, from = %record.from, "warm builds are off: the build isn't kept");
            return Ok(Added::Dropped);
        }
        let entry = self.warm.add_entry(project, incoming, record).await?;
        let repo = repo_name(project);
        match self.enforce_limit(&repo).await {
            Ok(evicted) if evicted.contains(&entry) => Ok(Added::Evicted { entry }),
            Ok(_) => Ok(Added::Kept { entry }),
            Err(e) => {
                tracing::warn!(warm_build = %repo, error = %e, "can't apply the stock limit");
                Ok(Added::Kept { entry })
            }
        }
    }

    /// Gets `warm/<repo>` down to its [`limit`](Self::limit) where builds are renamed: the entries no ticket has
    /// claimed that are farthest from the base branch's tip go to the trash ([`evictions`]). Where builds are cloned,
    /// nothing (the stock holds one). Returns the entries evicted; while another pass runs, it only asks that one to
    /// go once more and returns none.
    pub async fn enforce_limit(&self, repo: &str) -> Result<Vec<u64>, WarmError> {
        if self.warm.mode() == HandoffMode::Clone || !self.warm.begin_limit_pass(repo) {
            return Ok(Vec::new());
        }
        // Dropped mid-pass (the caller's future cancelled at an `.await`), it lets the pass go, so later ones run.
        let held = LimitPass {
            warm: &self.warm,
            repo,
        };
        let mut evicted = Vec::new();
        loop {
            let pass = self.limit_pass(repo).await;
            let again = self.warm.end_limit_pass(repo);
            match pass {
                Ok(more) => evicted.extend(more),
                // Asked for meanwhile: that pass may see what this one couldn't, so it still runs.
                Err(e) if again => {
                    tracing::warn!(warm_build = %repo, error = %e, "a stock limit pass failed; going once more");
                }
                Err(e) => {
                    std::mem::forget(held);
                    return Err(e);
                }
            }
            if !again {
                std::mem::forget(held);
                return Ok(evicted);
            }
        }
    }

    async fn limit_pass(&self, repo: &str) -> Result<Vec<u64>, WarmError> {
        let limit = self.limit(repo).await?;
        let entries = self.warm.all_entries(repo).await;
        if entries.len() <= limit {
            return Ok(Vec::new());
        }
        let projects = repo_projects(&self.board, repo).await?;
        let mut tips: HashMap<String, (Git, Option<String>)> = HashMap::new();
        let mut distances: HashMap<u64, (Option<usize>, i64)> = HashMap::new();
        for entry in &entries {
            let Some(record) = &entry.record else {
                distances.insert(entry.n, (None, 0));
                continue;
            };
            let project = projects
                .iter()
                .find(|p| p.id == record.project)
                .or(projects.first());
            let distance = match project {
                Some(project) => {
                    if !tips.contains_key(&project.id) {
                        let tip = self.base_tip(project).await?;
                        tips.insert(project.id.clone(), tip);
                    }
                    let (git, tip) = &tips[&project.id];
                    match tip {
                        Some(tip) => changed_files(git, project, &record.built_at, tip).await,
                        None => None,
                    }
                }
                None => None,
            };
            distances.insert(entry.n, (distance, record.kept_at));
        }
        // An entry added since the listing isn't chosen here: the pass its add asks for sees it.
        self.warm
            .evict(repo, |free| {
                let rows: Vec<(u64, Option<usize>, i64)> = free
                    .iter()
                    .filter_map(|e| distances.get(&e.n).map(|(d, k)| (e.n, *d, *k)))
                    .collect();
                evictions(&rows, limit)
            })
            .await
    }

    /// The project's git and the tip of its base branch (`None` when git can't find it).
    async fn base_tip(&self, project: &Project) -> Result<(Git, Option<String>), BoardError> {
        let git = self.board.project_git(&project.key).await?;
        let tip = git
            .resolve(
                Path::new(&project.repo_path),
                &format!("refs/heads/{}", project.base_branch),
            )
            .await
            .unwrap_or(None);
        Ok((git, tip))
    }

    async fn rank_entries(
        &self,
        project: &Project,
        entries: &[Entry],
        target: &str,
        own: Option<&str>,
    ) -> Vec<u64> {
        if entries.is_empty() {
            return Vec::new();
        }
        let (git, tip) = match self.base_tip(project).await {
            Ok(found) => found,
            Err(e) => {
                tracing::warn!(project = %project.key, error = %e, "can't rank the stock's builds");
                return entries.iter().rev().map(|e| e.n).collect();
            }
        };
        let mut cands = Vec::with_capacity(entries.len());
        for entry in entries {
            let Some(record) = &entry.record else {
                cands.push((
                    Candidate {
                        n: entry.n,
                        from: String::new(),
                        built_at: String::new(),
                        kept_at: 0,
                    },
                    Fit::Other(usize::MAX),
                ));
                continue;
            };
            let fit = if own == Some(record.from.as_str()) {
                Fit::Own
            } else if is_ancestor(&git, project, &record.built_at, target).await {
                Fit::Ancestor(
                    changed_files(&git, project, &record.built_at, target)
                        .await
                        .unwrap_or(usize::MAX),
                )
            } else {
                let distance = match &tip {
                    Some(tip) => changed_files(&git, project, &record.built_at, tip).await,
                    None => None,
                };
                Fit::Other(distance.unwrap_or(usize::MAX))
            };
            let cand = Candidate {
                n: entry.n,
                from: record.from.clone(),
                built_at: record.built_at.clone(),
                kept_at: record.kept_at,
            };
            tracing::debug!(project = %project.key, entry = cand.n, from = %cand.from, built_at = %cand.built_at, ?fit, "stock entry ranked");
            cands.push((cand, fit));
        }
        rank(cands)
    }
}

/// A limit pass [`Stock::enforce_limit`] holds. Dropped before the pass ends itself, it lets the pass go.
struct LimitPass<'a> {
    warm: &'a WarmBuilds,
    repo: &'a str,
}

impl Drop for LimitPass<'_> {
    fn drop(&mut self) {
        self.warm.abort_limit_pass(self.repo);
    }
}

/// Whether `ancestor` is an ancestor of (or equal to) `of`; a commit git can't find is not.
async fn is_ancestor(git: &Git, project: &Project, ancestor: &str, of: &str) -> bool {
    matches!(
        git.run_with_code(
            Path::new(&project.repo_path),
            &["merge-base", "--is-ancestor", ancestor, of],
        )
        .await,
        Ok((0, _))
    )
}

/// How many files differ between commits `from` and `to`; `None` when git can't tell (a commit it can't find).
async fn changed_files(git: &Git, project: &Project, from: &str, to: &str) -> Option<usize> {
    git.run(
        Path::new(&project.repo_path),
        &["diff", "--name-only", "-z", from, to],
    )
    .await
    .ok()
    .map(|out| split_z(&out).len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(n: u64, kept_at: i64) -> Candidate {
        Candidate {
            n,
            from: format!("SHOP-{n}"),
            built_at: format!("c{n}"),
            kept_at,
        }
    }

    #[test]
    fn own_build_ranks_first() {
        let order = rank(vec![
            (cand(0, 10), Fit::Ancestor(0)),
            (cand(1, 5), Fit::Own),
            (cand(2, 20), Fit::Other(0)),
        ]);
        assert_eq!(order, [1, 0, 2]);
    }

    #[test]
    fn ancestors_rank_by_fewest_changed_files() {
        let order = rank(vec![
            (cand(0, 30), Fit::Ancestor(7)),
            (cand(1, 10), Fit::Ancestor(2)),
            (cand(2, 20), Fit::Ancestor(2)),
            (cand(3, 40), Fit::Other(1)),
        ]);
        // Equal changes: the one kept last first; any ancestor before a build made elsewhere.
        assert_eq!(order, [2, 1, 0, 3]);
    }

    #[test]
    fn without_an_ancestor_the_nearest_to_base_tip_wins() {
        let order = rank(vec![
            (cand(0, 30), Fit::Other(9)),
            (cand(1, 10), Fit::Other(usize::MAX)),
            (cand(2, 20), Fit::Other(1)),
            (cand(3, 40), Fit::Other(5)),
        ]);
        assert_eq!(order, [2, 3, 0, 1]);
        assert!(rank(Vec::new()).is_empty());
    }

    #[test]
    fn evictions_take_the_farthest_then_the_oldest() {
        let entries = [
            (0, Some(1), 10),
            (1, Some(9), 20),
            (2, Some(5), 30),
            (3, Some(5), 5),
        ];
        assert_eq!(evictions(&entries, 4), Vec::<u64>::new());
        assert_eq!(evictions(&entries, 9), Vec::<u64>::new());
        assert_eq!(evictions(&entries, 3), [1]);
        // Equal distances: the oldest goes first.
        assert_eq!(evictions(&entries, 2), [1, 3]);
        assert_eq!(evictions(&entries, 1), [1, 3, 2]);
        assert_eq!(evictions(&entries, 0), [1, 3, 2, 0]);
    }

    #[test]
    fn unknown_distance_is_evicted_first() {
        let entries = [(0, Some(9), 10), (1, None, 50), (2, Some(1), 20)];
        assert_eq!(evictions(&entries, 2), [1]);
        assert_eq!(evictions(&entries, 1), [1, 0]);
    }
}
