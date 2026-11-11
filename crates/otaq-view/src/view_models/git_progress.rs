//! What the card and the Merging dialog say while Approve & merge or a start runs its git steps: the step git is
//! really on, and while one of the user's hooks runs, which one and for how long ("Merging · running your pre-commit
//! hook · 40 s"). Otaq never stops a hook on its own, so the time only grows; the page asks again every second
//! while a hook runs ([`crate::view_models::board_page::BoardPage::seconds`]).

use otaq_core::domain::{GitHookRunning, GitOperation, GitProgress};
use otaq_core::git::GitStep;

/// The card's head while a merge runs its git steps (and the Merge check): "Merging…".
pub const MERGING: &str = "Merging";
/// The card's head while a start runs its git steps.
pub const STARTING: &str = "Starting";

/// A step as the card says it, after "Merging · " or "Starting · ": "squashing into main".
pub fn step_words(step: GitStep, target: &str) -> String {
    match step {
        GitStep::AddWorktree => "making its worktree".to_owned(),
        GitStep::SaveUncommitted => "keeping its uncommitted files aside".to_owned(),
        GitStep::UpdateBranch => format!("merging {target} into the branch"),
        GitStep::RestoreUncommitted => "putting its uncommitted files back".to_owned(),
        GitStep::UpdateTarget => format!("bringing {target} up to date"),
        GitStep::PrepareScratch => "preparing the scratch worktree".to_owned(),
        GitStep::Squash => format!("squashing into {target}"),
        GitStep::Commit => format!("committing to {target}"),
        GitStep::MergeCheck => "running the merge check".to_owned(),
        GitStep::MoveTarget => format!("moving {target} to the new commit"),
        GitStep::Reword => "rewording the commits".to_owned(),
    }
}

/// Whether the running hook's line names its folder: only where it runs in the user's own folder rather than in
/// Otaq's worktrees. None does yet; the post-merge hook that runs where the target branch moves will.
pub fn names_folder(_step: Option<GitStep>) -> bool {
    false
}

/// "40 s", "1 min 5 s", "12 min": how long a hook has run.
pub fn seconds(ms: i64) -> String {
    let secs = ms.max(0) / 1000;
    match (secs / 60, secs % 60) {
        (0, s) => format!("{s} s"),
        (m, 0) => format!("{m} min"),
        (m, s) if m < 10 => format!("{m} min {s} s"),
        (m, _) => format!("{m} min"),
    }
}

/// "running your pre-commit hook · 40 s"; with its folder named, "running your post-merge hook in ~/shop-web · 20 s".
pub fn hook_line(hook: &GitHookRunning, step: Option<GitStep>, now_ms: i64) -> String {
    let place = match names_folder(step) {
        true => format!(" in {}", hook.folder),
        false => String::new(),
    };
    format!(
        "running your {} hook{place} \u{b7} {}",
        hook.name,
        seconds(now_ms - hook.since_ms)
    )
}

/// The card's head for the operation: "Merging", "Starting".
pub fn head(progress: &GitProgress) -> &'static str {
    match progress.operation {
        GitOperation::Merge => MERGING,
        GitOperation::Start => STARTING,
    }
}

/// The card's line: "Merging · running your pre-commit hook · 40 s" while a hook runs, else the step,
/// "Merging · squashing into main", "Starting · making its worktree"; only the head before the first step.
pub fn card_line(progress: &GitProgress, now_ms: i64) -> String {
    let head = head(progress);
    let what = match (&progress.hook, progress.step) {
        (Some(hook), step) => Some(hook_line(hook, step, now_ms)),
        (None, Some(step)) => Some(step_words(step, &progress.target)),
        (None, None) => None,
    };
    match what {
        Some(what) => format!("{head} \u{b7} {what}"),
        None => format!("{head}\u{2026}"),
    }
}

/// Whether a running hook's time shows: the page asks again every second meanwhile.
pub fn shows_seconds(progress: Option<&GitProgress>) -> bool {
    progress.is_some_and(|p| p.hook.is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn merging(step: Option<GitStep>, hook: Option<&str>) -> GitProgress {
        GitProgress {
            operation: GitOperation::Merge,
            target: "main".into(),
            step,
            earlier: Vec::new(),
            hook: hook.map(|name| GitHookRunning {
                name: name.into(),
                folder: "~/code/shop-web".into(),
                since_ms: 1_000,
            }),
        }
    }

    #[test]
    fn the_card_says_the_hook_that_runs_and_for_how_long() {
        let p = merging(Some(GitStep::Commit), Some("pre-commit"));
        assert_eq!(
            card_line(&p, 41_000),
            "Merging \u{b7} running your pre-commit hook \u{b7} 40 s"
        );
        assert_eq!(
            card_line(&p, 66_500),
            "Merging \u{b7} running your pre-commit hook \u{b7} 1 min 5 s"
        );
        assert!(shows_seconds(Some(&p)));
    }

    #[test]
    fn without_a_hook_the_card_says_the_step() {
        let p = merging(Some(GitStep::Squash), None);
        assert_eq!(card_line(&p, 0), "Merging \u{b7} squashing into main");
        assert!(!shows_seconds(Some(&p)));
        assert_eq!(card_line(&merging(None, None), 0), "Merging\u{2026}");
        let start = GitProgress {
            operation: GitOperation::Start,
            step: Some(GitStep::AddWorktree),
            ..merging(None, None)
        };
        assert_eq!(card_line(&start, 0), "Starting \u{b7} making its worktree");
    }

    #[test]
    fn times_read_in_seconds_then_minutes() {
        assert_eq!(seconds(0), "0 s");
        assert_eq!(seconds(59_999), "59 s");
        assert_eq!(seconds(120_000), "2 min");
        assert_eq!(seconds(9 * 60_000 + 59_000), "9 min 59 s");
        assert_eq!(seconds(12 * 60_000 + 30_000), "12 min");
    }
}
