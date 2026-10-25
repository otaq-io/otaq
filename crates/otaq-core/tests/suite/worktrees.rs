//! Worktrees, temporary branches and base_sha (T-0048), against temporary repos.

use std::path::{Path, PathBuf};

use crate::common::Fixture;
use otaq_core::data_dir::DataDir;
use otaq_core::git::Git;
use otaq_core::git::testing::{commit, git, snapshot};
use otaq_core::start::prepare_worktree;
use otaq_core::start::worktree::create_branch_unless_exists;

fn data(f: &Fixture) -> DataDir {
    DataDir::new(f.home.path().join("home"))
}

fn repo(f: &Fixture) -> PathBuf {
    f.repos.root().join("shop-web")
}

/// The user's repository outside `.git`.
fn user_files(repo: &Path) -> Vec<String> {
    snapshot(repo)
        .into_iter()
        .filter(|p| !p.starts_with(".git"))
        .collect()
}

#[tokio::test]
async fn fresh_start_creates_branch_and_worktree_outside_the_repo() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout API").await;
    let before = user_files(&repo(&f));
    let main = git(&repo(&f), &["rev-parse", "main"]);

    let wt = prepare_worktree(&f.board, &data(&f), &t.key.to_string())
        .await
        .unwrap();
    let expected = data(&f).worktree_path("shop-web", "SHOP-1").unwrap();
    assert_eq!(wt.path, expected);
    assert_eq!(wt.branch, "otaq/SHOP-1");
    assert_eq!(wt.base_branch, "main");
    assert_eq!(wt.base_sha, main);
    assert!(wt.path.join("file1.txt").is_file(), "checked out");
    assert_eq!(
        git(&wt.path, &["rev-parse", "--abbrev-ref", "HEAD"]),
        "otaq/SHOP-1"
    );
    // No upstream: the temporary branch tracks nothing.
    assert_eq!(
        git(
            &repo(&f),
            &[
                "for-each-ref",
                "--format=%(upstream)",
                "refs/heads/otaq/SHOP-1"
            ]
        ),
        ""
    );

    let row = f.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.branch_name.as_deref(), Some("otaq/SHOP-1"));
    assert_eq!(row.base_sha.as_deref(), Some(main.as_str()));
    assert_eq!(
        row.worktree_path.as_deref(),
        Some(expected.display().to_string().as_str())
    );
    assert!(row.branch_locked);
    assert_eq!(user_files(&repo(&f)), before, "nothing written in the repo");
}

#[tokio::test]
async fn rerunning_converges_from_any_partial_state() {
    let f = Fixture::shop().await;
    let t = f.create("t").await;
    let key = t.key.to_string();
    let first = prepare_worktree(&f.board, &data(&f), &key).await.unwrap();

    // Plain re-run.
    assert_eq!(
        prepare_worktree(&f.board, &data(&f), &key).await.unwrap(),
        first
    );

    // The worktree folder vanished but git still has it registered.
    std::fs::remove_dir_all(&first.path).unwrap();
    assert_eq!(
        prepare_worktree(&f.board, &data(&f), &key).await.unwrap(),
        first
    );
    assert!(first.path.join("file1.txt").is_file());

    // The worktree is gone entirely, only the branch exists; and base_sha was never stored.
    let path = first.path.display().to_string();
    git(&repo(&f), &["worktree", "remove", "--force", &path]);
    sqlx::query("UPDATE tickets SET base_sha = NULL, worktree_path = NULL WHERE id = ?")
        .bind(&t.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    // The base moved on meanwhile: base_sha stays where the branch left it.
    commit(&repo(&f), "later.txt", "later\n");
    let again = prepare_worktree(&f.board, &data(&f), &key).await.unwrap();
    assert_eq!(again, first);

    // An empty folder at the path (left by a failed add) is fine too.
    git(&repo(&f), &["worktree", "remove", "--force", &path]);
    std::fs::create_dir_all(&first.path).unwrap();
    assert_eq!(
        prepare_worktree(&f.board, &data(&f), &key).await.unwrap(),
        first
    );
}

#[tokio::test]
async fn a_foreign_folder_in_the_way_is_reported_not_removed() {
    let f = Fixture::shop().await;
    let t = f.create("t").await;
    let path = data(&f).worktree_path("shop-web", "SHOP-1").unwrap();
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("mine.txt"), "keep me").unwrap();
    let err = prepare_worktree(&f.board, &data(&f), &t.key.to_string())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("isn't a worktree"), "{err}");
    assert_eq!(
        std::fs::read_to_string(path.join("mine.txt")).unwrap(),
        "keep me"
    );
}

#[tokio::test]
async fn an_existing_branch_is_reused_not_recreated() {
    let f = Fixture::shop().await;
    let repo = repo(&f);
    let git_bin = Git::find().unwrap();
    let main = git(&repo, &["rev-parse", "main"]);
    commit(&repo, "later.txt", "later\n");
    let later = git(&repo, &["rev-parse", "main"]);

    assert!(
        create_branch_unless_exists(&git_bin, &repo, "otaq/SHOP-9", &main)
            .await
            .unwrap()
    );
    assert_eq!(git(&repo, &["rev-parse", "otaq/SHOP-9"]), main);

    // Already there (another start made it): reported as not created, and left where it was.
    assert!(
        !create_branch_unless_exists(&git_bin, &repo, "otaq/SHOP-9", &later)
            .await
            .unwrap()
    );
    assert_eq!(git(&repo, &["rev-parse", "otaq/SHOP-9"]), main);

    // Any other failure is returned as it was.
    let err = create_branch_unless_exists(&git_bin, &repo, "otaq/SHOP-10", "not-a-commit")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("not-a-commit"), "{err}");
}
