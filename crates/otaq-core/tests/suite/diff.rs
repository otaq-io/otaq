//! The diff engine (T-0084): a ticket's branch against its `base_sha`, against temporary repositories.

use crate::common::Fixture;
use otaq_core::board::Refusal;
use otaq_core::domain::Status;
use otaq_core::git::diff::{FileStatus, LineKind};
use otaq_core::git::testing::{commit, git};

#[tokio::test]
async fn a_branch_shows_every_kind_of_change_against_its_base_sha() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout API").await;
    let wt = f.start_worktree(&t).await;
    // On main already: file1.txt. Add files the branch then changes.
    std::fs::create_dir(wt.join("lib")).unwrap();
    commit(&wt, "lib/pricing.ts", "a\nb\nc\n");
    commit(&wt, "old_name.ts", "same\ncontent\nhere\nfor\nrename\n");
    commit(&wt, "gone.ts", "bye\n");
    let base_before = git(&f.repo(), &["rev-parse", "main"]);
    // Pretend those three were already on the base: move base_sha to the branch's current head.
    let head = git(&wt, &["rev-parse", "HEAD"]);
    sqlx::query("UPDATE tickets SET base_sha = ? WHERE id = ?")
        .bind(&head)
        .bind(&t.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();

    commit(&wt, "lib/pricing.ts", "a\nB\nc\nd\n");
    commit(&wt, "new_file.ts", "one\ntwo\n");
    git(&wt, &["rm", "--quiet", "gone.ts"]);
    git(&wt, &["mv", "old_name.ts", "new_name.ts"]);
    std::fs::write(wt.join("logo.png"), [0u8, 159, 146, 150, 0, 1, 2, 3]).unwrap();
    git(&wt, &["add", "logo.png"]);
    git(&wt, &["commit", "--quiet", "-m", "more"]);

    let diff = f.board.diff_summary("SHOP-1").await.unwrap();
    assert_eq!(diff.base_branch, "main");
    assert_eq!(diff.base_sha, head);
    assert_eq!(diff.base_short, &head[..7]);
    assert_eq!(diff.branch, "otaq/SHOP-1");
    let by_path = |p: &str| diff.files.iter().find(|f| f.path == p).unwrap().clone();
    assert_eq!(by_path("lib/pricing.ts").status, FileStatus::Modified);
    assert_eq!(
        (
            by_path("lib/pricing.ts").added,
            by_path("lib/pricing.ts").removed
        ),
        (2, 1)
    );
    assert_eq!(by_path("new_file.ts").status, FileStatus::Added);
    assert_eq!(by_path("gone.ts").status, FileStatus::Deleted);
    let renamed = by_path("new_name.ts");
    assert_eq!(renamed.status, FileStatus::Renamed);
    assert_eq!(renamed.old_path.as_deref(), Some("old_name.ts"));
    let logo = by_path("logo.png");
    assert!(logo.binary);
    assert_eq!(logo.size, Some(8));
    assert_eq!(diff.files.len(), 5);
    assert_eq!((diff.added, diff.removed), (4, 2));
    assert_ne!(base_before, head);

    let file = f.board.diff_file("SHOP-1", "lib/pricing.ts").await.unwrap();
    let lines: Vec<_> = file.hunks[0]
        .lines
        .iter()
        .map(|l| (l.kind, l.old, l.new, l.text.as_str()))
        .collect();
    assert_eq!(
        lines,
        [
            (LineKind::Context, Some(1), Some(1), "a"),
            (LineKind::Removed, Some(2), None, "b"),
            (LineKind::Added, None, Some(2), "B"),
            (LineKind::Context, Some(3), Some(3), "c"),
            (LineKind::Added, None, Some(4), "d"),
        ]
    );
    let logo = f.board.diff_file("SHOP-1", "logo.png").await.unwrap();
    assert!(logo.file.binary && logo.hunks.is_empty());
    let renamed = f.board.diff_file("SHOP-1", "new_name.ts").await.unwrap();
    assert_eq!(renamed.file.old_path.as_deref(), Some("old_name.ts"));
}

#[tokio::test]
async fn a_base_that_moves_on_doesnt_change_the_diff() {
    let f = Fixture::shop().await;
    let t = f.create("Price table").await;
    let wt = f.start_worktree(&t).await;
    commit(&wt, "price.ts", "1\n");
    let before = f.board.diff_summary("SHOP-1").await.unwrap();

    // Someone else's work lands on main after the ticket left it.
    commit(&f.repo(), "other.ts", "theirs\n");
    let after = f.board.diff_summary("SHOP-1").await.unwrap();
    assert_eq!(after, before);
    assert_eq!(after.files.len(), 1);
    assert_eq!(after.files[0].path, "price.ts");
}

#[tokio::test]
async fn an_empty_diff_is_an_empty_list() {
    let f = Fixture::shop().await;
    let t = f.create("Nothing yet").await;
    f.start_worktree(&t).await;
    let diff = f.board.diff_summary("SHOP-1").await.unwrap();
    assert!(diff.files.is_empty());
    assert_eq!((diff.added, diff.removed), (0, 0));
}

#[tokio::test]
async fn a_ticket_that_never_started_has_no_diff() {
    let f = Fixture::shop().await;
    f.create("Backlog").await;
    let err = f.board.diff_summary("SHOP-1").await.unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NoBranch));
}

#[tokio::test]
async fn the_diff_works_from_the_branch_once_the_worktree_is_gone() {
    let f = Fixture::shop().await;
    let t = f.create("Done").await;
    let wt = f.start_worktree(&t).await;
    commit(&wt, "done.ts", "x\n");
    git(&f.repo(), &["worktree", "remove", wt.to_str().unwrap()]);
    assert!(!wt.exists());
    f.force_status(&t, Status::Merged).await;
    let diff = f.board.diff_summary("SHOP-1").await.unwrap();
    assert_eq!(diff.files.len(), 1);
    let file = f.board.diff_file("SHOP-1", "done.ts").await.unwrap();
    assert_eq!(file.hunks[0].lines[0].text, "x");
}

/// A branch rebased onto a newer base by hand (or by the agent's `restack`) counts from where it stands now: the
/// base's newer commits aren't the ticket's, in the Files tab, Open PR's commits or the commit count.
#[tokio::test]
async fn a_branch_rebased_outside_otaq_counts_from_its_new_base() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout API").await;
    let wt = f.start_worktree(&t).await;
    let started_on = git(&f.repo(), &["rev-parse", "main"]);
    commit(&wt, "mine_one.ts", "one\n");
    commit(&wt, "mine_two.ts", "two\n");
    // main moves on, and the branch is rebased onto it outside Otaq.
    commit(&f.repo(), "theirs.ts", "theirs\n");
    let main = git(&f.repo(), &["rev-parse", "main"]);
    git(&wt, &["rebase", "--quiet", "main"]);

    let diff = f.board.diff_summary("SHOP-1").await.unwrap();
    let paths: Vec<_> = diff.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["mine_one.ts", "mine_two.ts"]);
    assert_eq!(diff.base_sha, main);
    assert_ne!(diff.base_sha, started_on);
    let ticket = f.board.ticket("SHOP-1").await.unwrap();
    assert_eq!(
        ticket.work.as_ref().unwrap().base_sha.as_deref(),
        Some(main.as_str())
    );
    assert_eq!(f.board.commits_beyond_base(&ticket).await.unwrap(), 2);
    let view = f.board.pr_flow_view("SHOP-1").await.unwrap();
    assert_eq!(view.commits.len(), 2);
}

/// The same onto the base's remote branch, ahead of the local one: the newest of the two counts.
#[tokio::test]
async fn a_branch_rebased_onto_the_remote_base_counts_from_there() {
    let f = Fixture::shop().await;
    let t = f.create("Checkout API").await;
    let wt = f.start_worktree(&t).await;
    commit(&wt, "mine.ts", "mine\n");
    commit(&f.repo(), "theirs.ts", "theirs\n");
    let remote = git(&f.repo(), &["rev-parse", "main"]);
    git(
        &f.repo(),
        &["update-ref", "refs/remotes/origin/main", &remote],
    );
    git(&f.repo(), &["reset", "--quiet", "--hard", "HEAD~1"]);
    git(&wt, &["rebase", "--quiet", "origin/main"]);

    let diff = f.board.diff_summary("SHOP-1").await.unwrap();
    let paths: Vec<_> = diff.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["mine.ts"]);
    assert_eq!(diff.base_sha, remote);
    let changes = f.board.files_changes("SHOP-1").await.unwrap();
    assert_eq!(changes.source.base_sha.as_deref(), Some(remote.as_str()));
    let paths: Vec<_> = changes.files.iter().map(|e| e.file.path.as_str()).collect();
    assert_eq!(paths, ["mine.ts"]);
}
