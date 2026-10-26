//! Which stage committed which file on a ticket's branch (d0129: "Anything they change shows in the diff at Spec
//! review"; canvas `s30-02`, "The decisions on the ticket's branch").
//!
//! Each Agent stage that ends with `stage next` records the head it ended on (History's Finished and Submitted events,
//! `head` and `stage_id`): the commits up to that head since the previous one are that run's, whatever second they
//! were made in (an agent commits and the next stage starts within the same second). Commits after the last recorded
//! head (a stage still working, or one that ended another way) go to the run whose time holds them: the latest run
//! that started before the commit, when it is an Agent stage's.
//!
//! Read-only, from git objects of the ticket's worktree or branch (as the Files tab reads them, [`super::files`]).

use std::path::Path;

use serde::{Deserialize, Serialize};

use super::stage_runs::StageRun;
use super::tickets::{EventKind, HistoryEvent};
use super::{Board, BoardError};
use crate::git::GitError;
use crate::git::diff::FileStatus;

/// A file one stage run's commits touched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageFile {
    pub path: String,
    /// How the run's first commit that touched it changed it: "added by Product spec".
    pub status: FileStatus,
}

/// What one run of an Agent stage committed, its files in the order they were first committed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunFiles {
    /// The stage's id.
    pub stage: String,
    /// The run's start, which names it among the ticket's runs ([`StageRun::started_at`]).
    pub started_at: i64,
    pub files: Vec<StageFile>,
}

/// `files.stages`: the runs that committed, oldest first; empty before the ticket started.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StageFiles {
    pub runs: Vec<RunFiles>,
}

/// One commit on the ticket's branch: its id, when it was made (Unix milliseconds, whole seconds) and what it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub at: i64,
    pub files: Vec<StageFile>,
}

/// Where an Agent stage run ended: the stage's id, the head it ended on, and when (History's event).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEnd {
    pub stage: String,
    pub head: String,
    pub at: i64,
}

/// The heads the ticket's Agent stages ended on, oldest first, from its History (any order).
pub fn run_ends(events: &[HistoryEvent]) -> Vec<RunEnd> {
    let mut ends: Vec<RunEnd> = events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::StageFinished | EventKind::ReviewSubmitted
            )
        })
        .filter_map(|e| {
            Some(RunEnd {
                stage: e.payload.get("stage_id")?.as_str()?.to_owned(),
                head: e.payload.get("head")?.as_str()?.to_owned(),
                at: e.at,
            })
        })
        .filter(|e| !e.stage.is_empty() && !e.head.is_empty())
        .collect();
    ends.sort_by_key(|e| e.at);
    ends
}

/// Each commit to its run: the commits up to a recorded head go to the run that ended on it (the latest run of that
/// stage that started before the end); the rest to the latest run that started at or before the commit (a commit's
/// time has whole seconds, a run's milliseconds), kept when it is an Agent stage's.
pub fn attribute(commits: &[Commit], runs: &[StageRun], ends: &[RunEnd]) -> StageFiles {
    let mut out: Vec<RunFiles> = Vec::new();
    let mut used = vec![false; ends.len()];
    let mut pending: Vec<&Commit> = Vec::new();
    for commit in commits {
        pending.push(commit);
        let Some(i) = (0..ends.len()).find(|&i| !used[i] && ends[i].head == commit.sha) else {
            continue;
        };
        used[i] = true;
        let end = &ends[i];
        let run = runs
            .iter()
            .filter(|r| r.stage == end.stage && r.kind == "agent" && r.started_at <= end.at)
            .max_by_key(|r| r.started_at);
        if let Some(run) = run {
            for commit in &pending {
                add(&mut out, run, commit);
            }
        }
        pending.clear();
    }
    for commit in pending {
        let Some(run) = runs
            .iter()
            .filter(|r| r.started_at <= commit.at + 999)
            .max_by_key(|r| r.started_at)
        else {
            continue;
        };
        if run.kind == "agent" {
            add(&mut out, run, commit);
        }
    }
    out.sort_by_key(|r| r.started_at);
    StageFiles { runs: out }
}

/// `commit`'s files to `run`'s share, each once.
fn add(out: &mut Vec<RunFiles>, run: &StageRun, commit: &Commit) {
    let at = match out
        .iter()
        .position(|r| r.stage == run.stage && r.started_at == run.started_at)
    {
        Some(at) => at,
        None => {
            out.push(RunFiles {
                stage: run.stage.clone(),
                started_at: run.started_at,
                files: Vec::new(),
            });
            out.len() - 1
        }
    };
    let files = &mut out[at].files;
    for file in &commit.files {
        if !files.iter().any(|f| f.path == file.path) {
            files.push(file.clone());
        }
    }
}

/// The commits of `range` in `cwd`, oldest first (`git log --name-status`).
async fn commits(git: &crate::git::Git, cwd: &Path, range: &str) -> Result<Vec<Commit>, GitError> {
    let out = git
        .run(
            cwd,
            &[
                "-c",
                "core.quotePath=false",
                "--no-optional-locks",
                "log",
                "--reverse",
                "--no-renames",
                "--format=%x1e%H %ct",
                "--name-status",
                range,
            ],
        )
        .await?;
    Ok(parse_log(&out))
}

/// `git log --format='%x1e%H %ct' --name-status`'s output as commits.
fn parse_log(out: &str) -> Vec<Commit> {
    out.split('\u{1e}')
        .filter(|c| !c.trim().is_empty())
        .filter_map(|c| {
            let mut lines = c.lines();
            let (sha, secs) = lines.next()?.trim().split_once(' ')?;
            let at = secs.parse::<i64>().ok()? * 1000;
            let files = lines
                .filter_map(|line| {
                    let (letter, path) = line.split_once('\t')?;
                    let status = match letter.chars().next()? {
                        'A' => FileStatus::Added,
                        'D' => FileStatus::Deleted,
                        _ => FileStatus::Modified,
                    };
                    Some(StageFile {
                        path: path.to_owned(),
                        status,
                    })
                })
                .collect();
            Some(Commit {
                sha: sha.to_owned(),
                at,
                files,
            })
        })
        .collect()
}

impl Board {
    /// `files.stages`: what each run of an Agent stage committed on the ticket's branch since it left its base.
    pub async fn stage_files(&self, key: &str) -> Result<StageFiles, BoardError> {
        let ticket = self.ticket(key).await?;
        let source = self.files_source(&ticket).await?;
        let Some(base) = source.from.clone().or(source.info.base_sha.clone()) else {
            return Ok(StageFiles::default());
        };
        let git = self.git()?;
        let commits = commits(git, &source.cwd, &format!("{base}..{}", source.rev)).await?;
        if commits.is_empty() {
            return Ok(StageFiles::default());
        }
        let runs = self.stage_runs(key).await?;
        let ends = run_ends(&self.history(key).await?);
        Ok(attribute(&commits, &runs, &ends))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(stage: &str, kind: &str, started_at: i64) -> StageRun {
        StageRun {
            stage: stage.into(),
            kind: kind.into(),
            session: None,
            agent: None,
            model: None,
            round: None,
            result: None,
            summary: None,
            started_at,
            ended_at: None,
        }
    }

    fn file(path: &str, status: FileStatus) -> StageFile {
        StageFile {
            path: path.into(),
            status,
        }
    }

    fn commit(sha: &str, at: i64, files: Vec<StageFile>) -> Commit {
        Commit {
            sha: sha.into(),
            at,
            files,
        }
    }

    #[test]
    fn the_log_reads_as_commits() {
        let out = "\u{1e}aaa 1700000000\n\nA\tdecisions/0140-share.md\nM\tDESIGN.md\n\u{1e}bbb 1700000060\n\nD\told.md\n";
        assert_eq!(
            parse_log(out),
            [
                commit(
                    "aaa",
                    1_700_000_000_000,
                    vec![
                        file("decisions/0140-share.md", FileStatus::Added),
                        file("DESIGN.md", FileStatus::Modified)
                    ]
                ),
                commit(
                    "bbb",
                    1_700_000_060_000,
                    vec![file("old.md", FileStatus::Deleted)]
                ),
            ]
        );
    }

    /// Commits made in one second across three stages (an agent commits, `stage next`, the next starts): each goes to
    /// the run that ended on its head, not to the run that started last in that second.
    #[test]
    fn recorded_heads_split_commits_made_in_one_second() {
        let runs = [
            run("product-spec", "agent", 1_000_100),
            run("tech-spec", "agent", 1_000_400),
            run("milestone-plan", "agent", 1_000_600),
            run("spec-review", "you", 1_000_800),
        ];
        let end = |stage: &str, head: &str, at: i64| RunEnd {
            stage: stage.into(),
            head: head.into(),
            at,
        };
        let ends = [
            end("product-spec", "a", 1_000_390),
            end("tech-spec", "b", 1_000_590),
            end("milestone-plan", "c", 1_000_790),
        ];
        let commits = [
            commit(
                "a",
                1_000_000,
                vec![file("decisions/0001.md", FileStatus::Added)],
            ),
            commit("b", 1_000_000, vec![file("SDD.md", FileStatus::Modified)]),
            commit("c", 1_000_000, vec![file("SDD.md", FileStatus::Modified)]),
        ];
        let attributed = attribute(&commits, &runs, &ends);
        let got: Vec<(&str, Vec<&str>)> = attributed
            .runs
            .iter()
            .map(|r| {
                (
                    r.stage.as_str(),
                    r.files.iter().map(|f| f.path.as_str()).collect(),
                )
            })
            .collect();
        let expected: [(&str, Vec<&str>); 3] = [
            ("product-spec", vec!["decisions/0001.md"]),
            ("tech-spec", vec!["SDD.md"]),
            ("milestone-plan", vec!["SDD.md"]),
        ];
        assert_eq!(got, expected);
    }

    /// Without a recorded head, each commit goes to the run that started last before it; one made in a human check,
    /// or before any run, to none. A file a run touched twice is listed once, as it first changed.
    #[test]
    fn without_heads_each_commit_goes_to_the_run_that_holds_it() {
        let runs = [
            run("product-spec", "agent", 1_000_500),
            run("tech-spec", "agent", 2_000_000),
            run("spec-review", "you", 3_000_000),
        ];
        let commits = [
            commit("p", 500_000, vec![file("before.md", FileStatus::Added)]),
            // Made in the same second the run started: the run's.
            commit("q", 1_000_000, vec![file("a.md", FileStatus::Added)]),
            commit("r", 1_500_000, vec![file("a.md", FileStatus::Modified)]),
            commit("s", 2_100_000, vec![file("SDD.md", FileStatus::Modified)]),
            commit("t", 3_100_000, vec![file("by-you.md", FileStatus::Added)]),
        ];
        assert_eq!(
            attribute(&commits, &runs, &[]).runs,
            [
                RunFiles {
                    stage: "product-spec".into(),
                    started_at: 1_000_500,
                    files: vec![file("a.md", FileStatus::Added)],
                },
                RunFiles {
                    stage: "tech-spec".into(),
                    started_at: 2_000_000,
                    files: vec![file("SDD.md", FileStatus::Modified)],
                },
            ]
        );
    }
}
