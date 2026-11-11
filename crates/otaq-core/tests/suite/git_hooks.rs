//! Otaq's own git steps say which of the user's hooks runs, since when, and which one refused with its whole output
//! (the paths under the home shortened). Real hooks in temporary repositories, through `core.hooksPath`.

use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::git::checkpoint::UpdateError;
use otaq_core::git::merge::{HookChanges, MergeOutcome, MergeRequest};
use otaq_core::git::testing::{TempRepos, commit, git};
use otaq_core::git::{Git, GitError, GitStep, GitWatch};

/// A repository with `main` and a branch `otaq/HOOK-1` one commit ahead, and the folder its hooks are read from.
fn setup(repos: &TempRepos) -> (PathBuf, PathBuf) {
    let repo = repos.repo("shop", 1);
    git(&repo, &["checkout", "--quiet", "-b", "otaq/HOOK-1"]);
    commit(&repo, "calc.js", "add\n");
    git(&repo, &["checkout", "--quiet", "main"]);
    let hooks = repos.root().join("hooks");
    std::fs::create_dir_all(&hooks).unwrap();
    git(
        &repo,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    (repo, hooks)
}

fn hook(hooks: &Path, name: &str, body: &str) {
    let path = hooks.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

fn request(repos: &TempRepos, repo: &Path) -> MergeRequest {
    MergeRequest {
        repo: repo.to_path_buf(),
        project_name: "shop".into(),
        key: "HOOK-1".into(),
        branch: "otaq/HOOK-1".into(),
        worktree: None,
        target: "main".into(),
        sync_target: false,
        goes_on_without_remote: false,
        scratch: repos.root().join("scratch"),
        message: "HOOK-1 Add subtraction".into(),
        check: None,
        recheck: false,
    }
}

/// What the hooks print about the user's home: it must come back as `~`.
fn home() -> String {
    std::env::home_dir().unwrap().display().to_string()
}

#[tokio::test]
async fn a_refusing_pre_commit_is_named_with_its_whole_output_and_the_home_shortened() {
    let repos = TempRepos::new();
    let (repo, hooks) = setup(&repos);
    hook(
        &hooks,
        "pre-commit",
        "echo 'lint: TODO found in src/calc.js:3' >&2\necho \"config: $HOME/.config/lint.json\"\necho 'commit refused'\nexit 1",
    );
    let main = git(&repo, &["rev-parse", "main"]);
    let watch = GitWatch::new();

    let err = Git::find()
        .unwrap()
        .watched(&watch)
        .local_merge(&request(&repos, &repo))
        .await
        .unwrap_err();

    let refusal = err.hook_refusal().expect("a hook refused");
    assert_eq!(refusal.name(), "pre-commit");
    assert_eq!(refusal.step(), Some(GitStep::Commit));
    assert_eq!(
        refusal.output(),
        "lint: TODO found in src/calc.js:3\nconfig: ~/.config/lint.json\ncommit refused"
    );
    assert!(!refusal.output().contains(&home()));
    assert!(!err.to_string().contains(&home()), "{err}");
    assert_eq!(git(&repo, &["rev-parse", "main"]), main, "main never moved");
    let activity = watch.activity();
    assert_eq!(activity.step, Some(GitStep::Commit));
    assert_eq!(activity.hook, None, "nothing runs any more");
    assert_eq!(activity.hooks.last().unwrap().name, "pre-commit");
}

#[tokio::test]
async fn a_refusing_commit_msg_is_told_apart_from_the_pre_commit_that_passed() {
    let repos = TempRepos::new();
    let (repo, hooks) = setup(&repos);
    // The pre-commit passes, printing on its way; it waits so its lines arrive well before it ends.
    hook(
        &hooks,
        "pre-commit",
        "echo 'lint: all good'\nsleep 0.3\nexit 0",
    );
    hook(
        &hooks,
        "commit-msg",
        "echo \"commit-msg: the message must start with feat: or fix: (got: $(head -1 \"$1\"))\" >&2\necho \"rules: $HOME/shop/CONTRIBUTING.md\" >&2\nexit 1",
    );
    let watch = GitWatch::new();

    let err = Git::find()
        .unwrap()
        .watched(&watch)
        .local_merge(&request(&repos, &repo))
        .await
        .unwrap_err();

    let refusal = err.hook_refusal().expect("a hook refused");
    assert_eq!(refusal.name(), "commit-msg");
    assert_eq!(refusal.hook.exit, Some(1));
    assert_eq!(
        refusal.output(),
        "commit-msg: the message must start with feat: or fix: (got: HOOK-1 Add subtraction)\nrules: ~/shop/CONTRIBUTING.md"
    );
    let activity = watch.activity();
    let ran: Vec<(&str, bool)> = activity
        .hooks
        .iter()
        .map(|h| (h.name.as_str(), h.passed()))
        .filter(|(name, _)| matches!(*name, "pre-commit" | "commit-msg"))
        .collect();
    assert_eq!(ran, [("pre-commit", true), ("commit-msg", false)]);
    let pre_commit = activity
        .hooks
        .iter()
        .find(|h| h.name == "pre-commit")
        .unwrap();
    assert_eq!(pre_commit.output, "lint: all good");
}

#[tokio::test]
async fn a_refusing_commit_msg_is_named_without_a_watch_too() {
    let repos = TempRepos::new();
    let (repo, hooks) = setup(&repos);
    hook(&hooks, "commit-msg", "echo 'commit-msg: no' >&2\nexit 1");

    let err = Git::find()
        .unwrap()
        .local_merge(&request(&repos, &repo))
        .await
        .unwrap_err();

    let refusal = err.hook_refusal().expect("a hook refused");
    assert_eq!(refusal.name(), "commit-msg");
    assert_eq!(refusal.step(), None, "no watch said a step");
    assert_eq!(refusal.output(), "commit-msg: no");
}

#[tokio::test]
async fn a_refusing_pre_merge_commit_on_the_merge_of_main_is_named_and_changes_nothing() {
    let repos = TempRepos::new();
    let (repo, hooks) = setup(&repos);
    let wt = repos.root().join("wt");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            wt.to_str().unwrap(),
            "otaq/HOOK-1",
        ],
    );
    commit(&repo, "main.txt", "moved on\n");
    let log = repos.root().join("hooks.log");
    hook(
        &hooks,
        "pre-rebase",
        &format!("echo pre-rebase >> '{}'\nexit 1", log.display()),
    );
    hook(
        &hooks,
        "pre-merge-commit",
        "echo \"pre-merge-commit: merges need a review (see $HOME/team/rules.md)\" >&2\nexit 1",
    );
    let head = git(&wt, &["rev-parse", "HEAD"]);
    let watch = GitWatch::new();

    let err = Git::find()
        .unwrap()
        .watched(&watch)
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap_err();

    let UpdateError::Git(GitError::HookRefused(refusal)) = err else {
        panic!("expected a hook's refusal, got {err}");
    };
    assert_eq!(refusal.name(), "pre-merge-commit");
    assert_eq!(refusal.step(), Some(GitStep::UpdateBranch));
    assert!(
        refusal
            .output()
            .contains("pre-merge-commit: merges need a review (see ~/team/rules.md)"),
        "{}",
        refusal.output()
    );
    // Nothing merged: the branch, the index and the files as they were, no merge left in progress.
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]), head);
    assert_eq!(git(&wt, &["status", "--porcelain"]), "");
    let merge_head = git(&wt, &["rev-parse", "--git-path", "MERGE_HEAD"]);
    assert!(!wt.join(merge_head).exists(), "no merge in progress");
    assert!(!log.exists(), "pre-rebase never ran");
}

#[tokio::test]
async fn a_refusing_post_checkout_is_named_when_a_worktree_is_added() {
    let repos = TempRepos::new();
    let (repo, hooks) = setup(&repos);
    hook(
        &hooks,
        "post-checkout",
        "echo \"post-checkout: can't install into $HOME/.cache/shop\" >&2\necho 'npm ERR! code EACCES' >&2\nexit 3",
    );
    let watch = GitWatch::new();

    // Local merge's scratch worktree is added first: post-checkout runs there and refuses.
    let err = Git::find()
        .unwrap()
        .watched(&watch)
        .local_merge(&request(&repos, &repo))
        .await
        .unwrap_err();

    let refusal = err.hook_refusal().expect("a hook refused");
    assert_eq!(refusal.name(), "post-checkout");
    assert_eq!(refusal.step(), Some(GitStep::PrepareScratch));
    assert_eq!(refusal.hook.exit, Some(3));
    assert_eq!(
        refusal.output(),
        "post-checkout: can't install into ~/.cache/shop\nnpm ERR! code EACCES"
    );
}

#[tokio::test]
async fn a_slow_hook_is_reported_running_by_name_while_it_runs() {
    let repos = TempRepos::new();
    let (repo, hooks) = setup(&repos);
    hook(&hooks, "pre-commit", "echo 'testing…'\nsleep 2\nexit 0");
    let watch = GitWatch::new();
    let mut changes = watch.subscribe();
    let git = Git::find().unwrap().watched(&watch);
    let request = request(&repos, &repo);
    let before_ms = now_ms();

    let merging = tokio::spawn(async move { git.local_merge(&request).await });
    let running = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            changes.changed().await.unwrap();
            if let Some(hook) = changes.borrow_and_update().hook.clone() {
                break hook;
            }
        }
    })
    .await
    .expect("a running hook is reported");

    assert_eq!(running.name, "pre-commit");
    assert_eq!(running.step, Some(GitStep::Commit));
    assert!(running.since_ms >= before_ms - 1000 && running.since_ms <= now_ms());
    assert!(!merging.is_finished(), "reported while it runs");
    let outcome = merging.await.unwrap().unwrap();
    assert!(
        matches!(outcome, MergeOutcome::Merged { .. }),
        "{outcome:?}"
    );
    let activity = watch.activity();
    assert_eq!(activity.hook, None);
    let ran = activity
        .hooks
        .iter()
        .find(|h| h.name == "pre-commit")
        .unwrap();
    assert!(ran.passed());
    assert!(ran.ended_ms - ran.started_ms >= 1900, "{ran:?}");
    assert_eq!(ran.output, "testing…");
}

/// A formatter's pre-commit: a header line on top of each staged `.js`, staged again.
const HEADER_HOOK: &str = r#"for f in $(git diff --cached --name-only --diff-filter=ACM -- '*.js'); do
  { echo '// formatted'; cat "$f"; } > "$f.tmp" && mv "$f.tmp" "$f"
  git add "$f"
done"#;

#[tokio::test]
async fn a_pre_commit_that_changes_the_squash_commit_lands_and_is_said() {
    let repos = TempRepos::new();
    let (repo, hooks) = setup(&repos);
    git(&repo, &["checkout", "--quiet", "otaq/HOOK-1"]);
    commit(&repo, "util.js", "util\n");
    commit(&repo, "notes.md", "notes\n");
    git(&repo, &["checkout", "--quiet", "main"]);
    // main moved on: what is compared is the branch with main merged in, not main.
    commit(&repo, "readme.md", "readme\n");
    hook(&hooks, "pre-commit", HEADER_HOOK);
    hook(&hooks, "commit-msg", "exit 0");

    // Nothing watches it: the commit's hooks are read even so.
    let outcome = Git::find()
        .unwrap()
        .local_merge(&request(&repos, &repo))
        .await
        .unwrap();

    let MergeOutcome::Merged {
        commit: landed,
        checked,
        hook_changed: Some(changes),
        ..
    } = outcome
    else {
        panic!("expected merged with a hook's changes, got {outcome:?}");
    };
    assert!(!checked, "the Merge check doesn't run because of it");
    assert_eq!(changes.hook.as_deref(), Some("pre-commit"));
    assert_eq!(changes.files, ["calc.js", "util.js"]);
    assert_eq!(changes.sentence(), "Your pre-commit hook changed 2 files");
    assert_eq!(changes.landed, landed);
    // It landed as the hook left it.
    assert_eq!(git(&repo, &["rev-parse", "main"]), landed);
    assert_eq!(git(&repo, &["show", "main:calc.js"]), "// formatted\nadd");
    // What was compared is what was squashed: the branch with main merged in.
    assert_eq!(changes.reviewed, git(&repo, &["rev-parse", "otaq/HOOK-1"]));
    assert_eq!(
        git(&repo, &["show", &format!("{}:readme.md", changes.reviewed)]),
        "readme"
    );
    // Show changes: only the hook's two lines.
    let diff = git(
        &repo,
        &["diff", "--no-color", "-U0", &changes.reviewed, &landed],
    );
    let added: Vec<&str> = diff
        .lines()
        .filter(|l| l.starts_with('+') && !l.starts_with("+++"))
        .collect();
    assert_eq!(added, ["+// formatted", "+// formatted"]);
    assert!(
        !diff
            .lines()
            .any(|l| l.starts_with('-') && !l.starts_with("---"))
    );
}

#[tokio::test]
async fn a_pre_commit_that_changes_nothing_says_nothing() {
    let repos = TempRepos::new();
    let (repo, hooks) = setup(&repos);
    hook(&hooks, "pre-commit", "echo 'lint: clean'\nexit 0");

    let outcome = Git::find()
        .unwrap()
        .local_merge(&request(&repos, &repo))
        .await
        .unwrap();

    assert!(
        matches!(
            outcome,
            MergeOutcome::Merged {
                hook_changed: None,
                ..
            }
        ),
        "{outcome:?}"
    );
}

#[test]
fn a_hooks_changes_are_said_in_one_sentence() {
    let changes = |hook: Option<&str>, n: usize| HookChanges {
        hook: hook.map(str::to_owned),
        files: (0..n).map(|i| format!("f{i}.js")).collect(),
        reviewed: "a".into(),
        landed: "b".into(),
    };
    assert_eq!(
        changes(Some("pre-commit"), 1).sentence(),
        "Your pre-commit hook changed 1 file"
    );
    assert_eq!(changes(None, 3).sentence(), "A hook changed 3 files");
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
