//! The Files tab's data (T-0467; FTB-01…FTB-04): the tree, search, a file and all changes of a ticket's worktree with
//! its uncommitted edits, and a never-started ticket's base branch, against temporary repositories.

use std::path::Path;

use crate::common::Fixture;
use otaq_core::board::Refusal;
use otaq_core::board::files::{EntryKind, FilesFrom};
use otaq_core::domain::Status;
use otaq_core::git::diff::{FileStatus, LineKind, MAX_FILE_LINES};
use otaq_core::git::testing::{commit, git};

fn index_mtime(worktree: &Path) -> std::time::SystemTime {
    let index = git(worktree, &["rev-parse", "--git-path", "index"]);
    std::fs::metadata(worktree.join(index))
        .unwrap()
        .modified()
        .unwrap()
}

#[tokio::test]
async fn the_tree_marks_committed_and_uncommitted_changes() {
    let f = Fixture::shop().await;
    let t = f.create("Login redirect").await;
    let wt = f.start_worktree(&t).await;
    std::fs::create_dir_all(wt.join("src/lib")).unwrap();
    commit(&wt, "src/lib/session.ts", "a\nb\n");
    commit(&wt, "README.md", "readme\n");
    // Not committed: an edit, a staged new file, an untracked file in a new folder, a deleted file.
    std::fs::write(wt.join("src/lib/session.ts"), "a\nB\nc\n").unwrap();
    std::fs::write(wt.join("staged.ts"), "s\n").unwrap();
    git(&wt, &["add", "staged.ts"]);
    std::fs::create_dir_all(wt.join("e2e")).unwrap();
    std::fs::write(wt.join("e2e/login.spec.ts"), "one\ntwo\n").unwrap();
    std::fs::remove_file(wt.join("file1.txt")).unwrap();
    let user_checkout = git(&f.repo(), &["status", "--porcelain"]);
    let index_before = index_mtime(&wt);

    let top = f.board.files_tree("SHOP-1", "").await.unwrap();
    assert_eq!(top.source.from, FilesFrom::Worktree);
    assert_eq!(top.source.branch, "otaq/SHOP-1");
    assert_eq!(top.source.base_branch, "main");
    assert_eq!(top.changed, 5);
    let rows: Vec<_> = top
        .entries
        .iter()
        .map(|e| {
            (
                e.name.as_str(),
                e.kind,
                e.change.as_ref().map(|c| (c.status, c.uncommitted)),
                e.changed_below,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("e2e", EntryKind::Folder, None, 1),
            ("src", EntryKind::Folder, None, 1),
            (
                "file1.txt",
                EntryKind::File,
                Some((FileStatus::Deleted, true)),
                0
            ),
            (
                "README.md",
                EntryKind::File,
                Some((FileStatus::Added, false)),
                0
            ),
            (
                "staged.ts",
                EntryKind::File,
                Some((FileStatus::Added, true)),
                0
            ),
        ]
    );

    let lib = f.board.files_tree("SHOP-1", "src/lib/").await.unwrap();
    assert_eq!(lib.path, "src/lib");
    let session = &lib.entries[0];
    assert_eq!(session.path, "src/lib/session.ts");
    let change = session.change.as_ref().unwrap();
    // Added on the branch and edited since: an addition against the base, partly uncommitted.
    assert_eq!(
        (
            change.status,
            change.added,
            change.removed,
            change.uncommitted
        ),
        (FileStatus::Added, 3, 0, true)
    );
    let e2e = f.board.files_tree("SHOP-1", "e2e").await.unwrap();
    let spec = e2e.entries[0].change.as_ref().unwrap();
    assert_eq!(
        (spec.status, spec.added, spec.uncommitted),
        (FileStatus::Added, 2, true)
    );

    let missing = f.board.files_tree("SHOP-1", "nope").await.unwrap_err();
    assert!(missing.refusal().is_none());
    let out = f.board.files_tree("SHOP-1", "../").await.unwrap_err();
    assert_eq!(out.refusal(), Some(Refusal::BadPath));

    // Read-only: the user's checkout and the worktree's index are as they were.
    assert_eq!(git(&f.repo(), &["status", "--porcelain"]), user_checkout);
    assert_eq!(index_mtime(&wt), index_before);
}

#[tokio::test]
async fn search_finds_names_across_the_worktree_changed_first() {
    let f = Fixture::shop().await;
    let t = f.create("Login").await;
    let wt = f.start_worktree(&t).await;
    for dir in ["e2e", "src/app/login", "src/lib", "public"] {
        std::fs::create_dir_all(wt.join(dir)).unwrap();
    }
    commit(&wt, "src/app/login/page.tsx", "page\n");
    commit(&wt, "src/lib/logger.ts", "log\n");
    commit(&wt, "public/logo.svg", "<svg/>\n");
    commit(&wt, "src/lib/catalog.ts", "c\n");
    // Changed after the base moved: base_sha is the branch's head now, so only these count as changes.
    let head = git(&wt, &["rev-parse", "HEAD"]);
    sqlx::query("UPDATE tickets SET base_sha = ? WHERE id = ?")
        .bind(&head)
        .bind(&t.id)
        .execute(f.board.store().pool())
        .await
        .unwrap();
    std::fs::write(wt.join("e2e/login.spec.ts"), "new\n").unwrap();
    std::fs::write(wt.join("src/app/login/page.tsx"), "page\nmore\n").unwrap();

    let found = f.board.files_search("SHOP-1", " LOG ").await.unwrap();
    assert_eq!(found.query, "LOG");
    let paths: Vec<_> = found.matches.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "e2e/login.spec.ts",
            "public/logo.svg",
            "src/lib/logger.ts",
            "src/app/login/page.tsx",
            "src/lib/catalog.ts",
        ]
    );
    assert_eq!(found.total, 5);
    let spec = &found.matches[0];
    assert_eq!(
        (spec.name.as_str(), spec.folder.as_str()),
        ("login.spec.ts", "e2e")
    );
    assert_eq!(spec.hit, [4, 7]);
    assert!(spec.change.as_ref().unwrap().uncommitted);
    let page = &found.matches[3];
    assert_eq!(page.hit, [8, 11]);
    assert_eq!(page.change.as_ref().unwrap().status, FileStatus::Modified);
    assert!(found.matches[1].change.is_none());

    let none = f.board.files_search("SHOP-1", "  ").await.unwrap();
    assert!(none.matches.is_empty());
}

#[tokio::test]
async fn read_gives_text_binary_and_huge_files_and_stays_inside() {
    let f = Fixture::shop().await;
    let t = f.create("Reads").await;
    let wt = f.start_worktree(&t).await;
    commit(&wt, "app.ts", "one\n");
    std::fs::write(wt.join("app.ts"), "one\ntwo\n").unwrap();
    std::fs::write(wt.join("logo.png"), [0u8, 159, 146, 150, 0, 1, 2, 3]).unwrap();
    let huge: String = (0..MAX_FILE_LINES + 5)
        .map(|n| format!("line {n}\n"))
        .collect();
    std::fs::write(wt.join("huge.txt"), &huge).unwrap();

    let text = f.board.files_read("SHOP-1", "app.ts").await.unwrap();
    assert_eq!(text.text, "one\ntwo\n");
    assert_eq!(text.size, 8);
    assert!(!text.binary && !text.truncated);
    let change = text.change.unwrap();
    assert_eq!(
        (change.status, change.added, change.uncommitted),
        (FileStatus::Added, 2, true)
    );
    let unchanged = f.board.files_read("SHOP-1", "file1.txt").await.unwrap();
    assert!(unchanged.change.is_none());

    let logo = f.board.files_read("SHOP-1", "logo.png").await.unwrap();
    assert!(logo.binary);
    assert_eq!((logo.size, logo.text.as_str()), (8, ""));

    let big = f.board.files_read("SHOP-1", "huge.txt").await.unwrap();
    assert!(big.truncated);
    assert_eq!(big.text.lines().count(), MAX_FILE_LINES);
    assert_eq!(big.size, huge.len() as u64);

    // Never outside the worktree: `..`, an absolute path, a symlink out, a symlinked folder out.
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("secret.txt"), "secret\n").unwrap();
    std::os::unix::fs::symlink(outside.path().join("secret.txt"), wt.join("link.txt")).unwrap();
    std::os::unix::fs::symlink(outside.path(), wt.join("away")).unwrap();
    for path in [
        "../secret.txt",
        "src/../../x",
        "/etc/hosts",
        "link.txt",
        "away/secret.txt",
        ".git",
    ] {
        let err = f.board.files_read("SHOP-1", path).await.unwrap_err();
        assert_eq!(err.refusal(), Some(Refusal::BadPath), "{path}");
    }
    // A link inside reads as its target.
    std::os::unix::fs::symlink("app.ts", wt.join("alias.ts")).unwrap();
    let alias = f.board.files_read("SHOP-1", "alias.ts").await.unwrap();
    assert_eq!(alias.text, "one\ntwo\n");
    let gone = f.board.files_read("SHOP-1", "nope.ts").await.unwrap_err();
    assert!(gone.refusal().is_none());
}

#[tokio::test]
async fn all_changes_include_uncommitted_files_with_their_diffs() {
    let f = Fixture::shop().await;
    let t = f.create("Middleware").await;
    let wt = f.start_worktree(&t).await;
    commit(&wt, "middleware.ts", "a\nb\nc\n");
    std::fs::write(wt.join("middleware.ts"), "a\nB\nc\nd\n").unwrap();
    std::fs::write(wt.join("notes.md"), "x\ny\n").unwrap();
    commit(&wt, "done.ts", "done\n");

    let changes = f.board.files_changes("SHOP-1").await.unwrap();
    let rows: Vec<_> = changes
        .files
        .iter()
        .map(|e| {
            (
                e.file.path.as_str(),
                e.file.status,
                e.file.added,
                e.file.removed,
                e.uncommitted,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("done.ts", FileStatus::Added, 1, 0, false),
            ("middleware.ts", FileStatus::Added, 4, 0, true),
            ("notes.md", FileStatus::Added, 2, 0, true),
        ]
    );
    assert_eq!((changes.added, changes.removed), (7, 0));
    // The committed-only diff (`board/diff.rs`) doesn't see them; Files does.
    let committed = f.board.diff_summary("SHOP-1").await.unwrap();
    assert_eq!(committed.files.len(), 2);

    let middleware = f.board.files_diff("SHOP-1", "middleware.ts").await.unwrap();
    let added: Vec<_> = middleware.hunks[0]
        .lines
        .iter()
        .map(|l| (l.kind, l.text.as_str()))
        .collect();
    assert_eq!(
        added,
        [
            (LineKind::Added, "a"),
            (LineKind::Added, "B"),
            (LineKind::Added, "c"),
            (LineKind::Added, "d"),
        ]
    );
    let notes = f.board.files_diff("SHOP-1", "notes.md").await.unwrap();
    assert_eq!(notes.file.status, FileStatus::Added);
    assert_eq!(notes.hunks[0].lines[1].new, Some(2));
    let unchanged = f.board.files_diff("SHOP-1", "file1.txt").await.unwrap_err();
    assert!(unchanged.refusal().is_none());
    let out = f.board.files_diff("SHOP-1", "../x").await.unwrap_err();
    assert_eq!(out.refusal(), Some(Refusal::BadPath));
}

#[tokio::test]
async fn a_ticket_that_never_started_shows_its_base_branch() {
    let f = Fixture::shop().await;
    f.create("Backlog").await;
    let tree = f.board.files_tree("SHOP-1", "").await.unwrap();
    assert_eq!(tree.source.from, FilesFrom::Base);
    assert_eq!(tree.source.branch, "main");
    assert!(tree.source.base_sha.is_none());
    assert_eq!(tree.changed, 0);
    let names: Vec<_> = tree.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["file1.txt"]);
    let file = f.board.files_read("SHOP-1", "file1.txt").await.unwrap();
    assert_eq!(file.text, "1\n");
    assert!(
        f.board
            .files_changes("SHOP-1")
            .await
            .unwrap()
            .files
            .is_empty()
    );
    let found = f.board.files_search("SHOP-1", "file").await.unwrap();
    assert_eq!(found.matches.len(), 1);
}

#[tokio::test]
async fn a_done_ticket_reads_its_branch_once_the_worktree_is_gone() {
    let f = Fixture::shop().await;
    let t = f.create("Done").await;
    let wt = f.start_worktree(&t).await;
    commit(&wt, "done.ts", "x\n");
    git(&f.repo(), &["worktree", "remove", wt.to_str().unwrap()]);
    f.force_status(&t, Status::Merged).await;
    let tree = f.board.files_tree("SHOP-1", "").await.unwrap();
    assert_eq!(tree.source.from, FilesFrom::Branch);
    assert_eq!(tree.changed, 1);
    let done = f.board.files_read("SHOP-1", "done.ts").await.unwrap();
    assert_eq!(done.text, "x\n");
    let diff = f.board.files_diff("SHOP-1", "done.ts").await.unwrap();
    assert_eq!(diff.hunks[0].lines[0].text, "x");
}
