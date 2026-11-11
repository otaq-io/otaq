//! Changed since review (T-0097, REV-06, GIT-05): new commits after a ticket entered Ready for Review need an
//! explicit Approve anyway; Otaq merging the base into the branch doesn't count.

use otaq_core::service::testing::said;
use std::path::{Path, PathBuf};

use otaq_core::board::review::{ApproveCheck, changed_message};
use otaq_core::board::tickets::Ticket;
use otaq_core::domain::{Actor, Status};
use otaq_core::git::Git;
use otaq_core::git::testing::{commit, git};
use otaq_core::review::LocalMergeOutcome;
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

/// A ticket with one commit, submitted for review by its agent (the review head is recorded).
async fn submitted(w: &TestService) -> (Ticket, PathBuf) {
    let t = w.create("SHOP", "Checkout API").await;
    let wt = w.start_worktree(&t).await;
    commit(&wt, "api.ts", "v1\n");
    w.force_status(&t, Status::Working).await;
    w.board
        .submit_for_review(
            "SHOP-1",
            said(&w.board, "SHOP-1", "Adds the API").await.as_ref(),
            &Actor::Agent(t.key.clone()),
        )
        .await
        .unwrap();
    (t, wt)
}

fn hook(repo: &Path, name: &str, body: &str) {
    let path = repo.join(".git/hooks").join(name);
    std::fs::write(&path, body).unwrap();
    // Git for Windows runs hooks with its own sh; only macOS and Linux need the executable bit.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[tokio::test]
async fn no_new_commits_means_unchanged() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (t, wt) = submitted(&w).await;
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(
        row.review_head_sha.as_deref(),
        Some(git(&wt, &["rev-parse", "HEAD"]).as_str())
    );
    assert!(row.review_entered_at.is_some());
    assert_eq!(
        w.board.approve_check("SHOP-1").await.unwrap(),
        ApproveCheck::Unchanged
    );
    w.stop().await;
}

#[tokio::test]
async fn a_new_commit_needs_approve_anyway_and_shows_only_itself() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let (_, wt) = submitted(&w).await;
    // The agent woke up (a question, the user's typing) and committed again.
    commit(&wt, "late.ts", "late\n");

    let check = w.board.approve_check("SHOP-1").await.unwrap();
    let ApproveCheck::Changed {
        new_commits,
        files,
        reviewed,
        ..
    } = &check
    else {
        panic!("expected changed, got {check:?}");
    };
    let only_new = w
        .board
        .diff_file_since("SHOP-1", "late.ts", reviewed)
        .await
        .unwrap();
    assert_eq!(only_new.hunks[0].lines[0].text, "late");
    assert!(
        w.board
            .diff_file_since("SHOP-1", "api.ts", reviewed)
            .await
            .is_err(),
        "api.ts was reviewed already"
    );
    assert_eq!(new_commits.len(), 1);
    assert_eq!(new_commits[0].subject, "edit late.ts");
    let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["late.ts"], "only the new commit, not api.ts");
    assert_eq!(
        changed_message("SHOP-18", 1),
        "SHOP-18 changed since you reviewed it \u{b7} 1 new commit"
    );

    let first = w
        .review
        .local_merge("SHOP-1", "m", false, &me())
        .await
        .unwrap();
    assert_eq!(first, LocalMergeOutcome::Changed { check });
    assert_eq!(
        w.status("SHOP-1").await,
        Status::WaitingForYou,
        "nothing merged"
    );
    let anyway = w
        .review
        .local_merge("SHOP-1", "m", true, &me())
        .await
        .unwrap();
    assert!(
        matches!(anyway, LocalMergeOutcome::Merged { .. }),
        "{anyway:?}"
    );
    w.stop().await;
}

#[tokio::test]
async fn otaqs_own_merge_of_a_moved_base_is_no_change() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (t, wt) = submitted(&w).await;
    commit(&repo, "other.ts", "main moved\n");
    let before = git(&wt, &["rev-parse", "HEAD"]);
    // The squash commit is refused after main was already merged into the branch (its merge commit passes).
    hook(
        &repo,
        "commit-msg",
        "#!/bin/sh\ngrep -q '^Merge branch' \"$1\" || exit 1\n",
    );

    let refused = w.review.local_merge("SHOP-1", "m", false, &me()).await;
    assert!(
        matches!(refused, Ok(LocalMergeOutcome::HookRefused { .. })),
        "the hook refused the squash commit: {refused:?}"
    );
    let after = git(&wt, &["rev-parse", "HEAD"]);
    assert_ne!(before, after, "main was merged into the branch");
    assert_eq!(
        git(&wt, &["rev-parse", "HEAD^"]),
        before,
        "on top of its own head"
    );
    assert_eq!(
        w.board.approve_check("SHOP-1").await.unwrap(),
        ApproveCheck::Unchanged
    );
    let row = w.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.review_head_sha.as_deref(), Some(after.as_str()));
    w.stop().await;
}

#[tokio::test]
async fn a_merge_of_main_keeps_new_commits_new() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    let (t, wt) = submitted(&w).await;
    commit(&wt, "late.ts", "late\n");
    commit(&repo, "other.ts", "main moved\n");
    let old_head = git(&wt, &["rev-parse", "HEAD"]);

    Git::find()
        .unwrap()
        .merge_with_checkpoint(&wt, "main")
        .await
        .unwrap();
    let new_head = git(&wt, &["rev-parse", "HEAD"]);
    w.board
        .record_otaq_update(&t.id, &old_head, &new_head)
        .await
        .unwrap();

    let ApproveCheck::Changed {
        new_commits, files, ..
    } = w.board.approve_check("SHOP-1").await.unwrap()
    else {
        panic!("the late commit is still new");
    };
    assert_eq!(new_commits.len(), 1);
    let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(
        paths,
        ["late.ts"],
        "main's change isn't shown as the ticket's"
    );
    w.stop().await;
}
