//! The project Files view's data (T-0476; FIL-02, FIL-03, FIL-04, FIL-07, FIL-08, FIL-10): the project's base branch
//! from git with every ticket's changes on top, a ticket on another base, an epic child, the picker's rows, All
//! changes by ticket and the per-ticket cache, against temporary repositories.

use std::path::Path;

use crate::common::{Fixture, me};
use otaq_core::board::Refusal;
use otaq_core::board::files::{EntryKind, FilesFrom};
use otaq_core::board::project_files::{FileOwner, ProjectEntry, ProjectFile, ProjectTree};
use otaq_core::board::projects::AddProject;
use otaq_core::board::tickets::NewTicketInput;
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

fn entry<'a>(tree: &'a ProjectTree, name: &str) -> &'a ProjectEntry {
    tree.entries
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("{name} in {:?}", tree.entries))
}

/// Each owner as (key, file status, uncommitted, other base).
fn owners(owners: &[FileOwner]) -> Vec<(String, FileStatus, bool, bool)> {
    owners
        .iter()
        .map(|o| {
            (
                o.key.to_string(),
                o.file.status,
                o.uncommitted,
                o.other_base,
            )
        })
        .collect()
}

/// `shop-web`'s base with a pricing module, a file to delete, a binary and a huge lock file, committed on `main`.
fn seed_base(repo: &Path) -> String {
    std::fs::create_dir_all(repo.join("src/lib")).unwrap();
    commit(repo, "src/lib/pricing.ts", "a\nb\nc\nd\n");
    commit(repo, "src/lib/legacy-price.ts", "old\n");
    std::fs::write(repo.join("hero.png"), [0u8, 159, 146, 150, 0, 1]).unwrap();
    git(repo, &["add", "hero.png"]);
    git(repo, &["commit", "--quiet", "-m", "hero"]);
    let huge: String = (0..MAX_FILE_LINES + 5)
        .map(|n| format!("lock {n}\n"))
        .collect();
    commit(repo, "pnpm-lock.yaml", &huge);
    huge
}

#[tokio::test]
async fn the_base_carries_every_tickets_changes() {
    let f = Fixture::shop().await;
    let repo = f.repo();
    let huge = seed_base(&repo);

    // SHOP-1 (Dev, working): committed edits, a deleted file, a changed binary.
    let price = f.create("Price table").await;
    let price_wt = f.start_worktree(&price).await;
    commit(&price_wt, "src/lib/pricing.ts", "a\nb\nC\nd\n");
    git(&price_wt, &["rm", "--quiet", "src/lib/legacy-price.ts"]);
    git(&price_wt, &["commit", "--quiet", "-m", "drop legacy"]);
    std::fs::write(price_wt.join("hero.png"), [0u8, 1, 2, 3, 4, 5, 6, 7]).unwrap();
    git(&price_wt, &["commit", "--quiet", "-am", "hero"]);
    f.force_status(&price, Status::Working).await;
    // SHOP-2 (needs you): an uncommitted edit of the same file and an untracked file in a new folder.
    let coupon = f.create("Coupon field").await;
    let coupon_wt = f.start_worktree(&coupon).await;
    std::fs::write(coupon_wt.join("src/lib/pricing.ts"), "A\nb\nc\nd\n").unwrap();
    std::fs::create_dir_all(coupon_wt.join("e2e")).unwrap();
    std::fs::write(coupon_wt.join("e2e/login.spec.ts"), "one\ntwo\n").unwrap();
    f.force_status(&coupon, Status::NeedsYou).await;
    // SHOP-3: never started. SHOP-4: a worktree without changes yet.
    let backlog = f.create("Later").await;
    let idle = f.create("Fresh").await;
    f.start_worktree(&idle).await;
    f.force_status(&idle, Status::Working).await;

    let checkout = git(&repo, &["status", "--porcelain"]);
    let indexes = (index_mtime(&price_wt), index_mtime(&coupon_wt));
    let cache = f.board.project_files();

    let top = f.board.files_project_tree("SHOP", "", false).await.unwrap();
    assert_eq!(top.source.base_branch, "main");
    assert_eq!(top.source.base_sha, git(&repo, &["rev-parse", "main"]));
    // SHOP-1, SHOP-2 and SHOP-4 read once each; SHOP-3 has nothing to read.
    assert_eq!(cache.recomputes(), 3);
    let names: Vec<_> = top
        .entries
        .iter()
        .map(|e| (e.name.as_str(), e.kind, e.changed_below))
        .collect();
    assert_eq!(
        names,
        [
            ("e2e", EntryKind::Folder, 1),
            ("src", EntryKind::Folder, 2),
            ("file1.txt", EntryKind::File, 0),
            ("hero.png", EntryKind::File, 0),
            ("pnpm-lock.yaml", EntryKind::File, 0),
        ]
    );
    let hero = &entry(&top, "hero.png").owners[0];
    assert!(hero.file.binary);
    assert_eq!((hero.file.old_size, hero.file.size), (Some(6), Some(8)));
    let counts = top.counts.unwrap();
    assert_eq!(
        (
            counts.tickets,
            counts.changed,
            counts.needs_you,
            counts.needs_you_tickets,
            counts.same_file
        ),
        (2, 4, 2, 1, 1)
    );

    let lib = f
        .board
        .files_project_tree("SHOP", "src/lib", false)
        .await
        .unwrap();
    // The second request reads nothing again.
    assert_eq!(cache.recomputes(), 3);
    let pricing = entry(&lib, "pricing.ts");
    // The ticket that needs you first.
    assert_eq!(
        owners(&pricing.owners),
        [
            ("SHOP-2".into(), FileStatus::Modified, true, false),
            ("SHOP-1".into(), FileStatus::Modified, false, false),
        ]
    );
    assert!(pricing.owners[0].needs_you);
    assert_eq!(pricing.owners[1].branch, "otaq/SHOP-1");
    assert_eq!(pricing.owners[1].base_branch, "main");
    let legacy = entry(&lib, "legacy-price.ts");
    assert_eq!(
        owners(&legacy.owners),
        [("SHOP-1".into(), FileStatus::Deleted, false, false)]
    );
    let e2e = f
        .board
        .files_project_tree("SHOP", "e2e/", false)
        .await
        .unwrap();
    let spec = &e2e.entries[0];
    assert_eq!(spec.path, "e2e/login.spec.ts");
    assert_eq!(
        owners(&spec.owners),
        [("SHOP-2".into(), FileStatus::Added, true, false)]
    );
    // Added by a ticket on the base: not "not on main".
    assert!(!spec.not_on_base);

    // The base alone, for the Loading state: no marks, nothing read.
    let bare = f
        .board
        .files_project_tree("SHOP", "src/lib", true)
        .await
        .unwrap();
    assert!(bare.counts.is_none());
    assert!(bare.entries.iter().all(|e| e.owners.is_empty()));
    let missing = f
        .board
        .files_project_tree("SHOP", "nope", false)
        .await
        .unwrap_err();
    assert!(missing.refusal().is_none());
    let out = f
        .board
        .files_project_tree("SHOP", "../", false)
        .await
        .unwrap_err();
    assert_eq!(out.refusal(), Some(Refusal::BadPath));

    // A file as it is on the base, with its owners; a deleted one too.
    let read = f
        .board
        .files_project_read("SHOP", "src/lib/pricing.ts")
        .await
        .unwrap();
    assert!(read.on_base);
    assert_eq!(read.text, "a\nb\nc\nd\n");
    assert_eq!(read.owners.len(), 2);
    assert!(read.diff.is_none());
    let deleted = f
        .board
        .files_project_read("SHOP", "src/lib/legacy-price.ts")
        .await
        .unwrap();
    assert_eq!(deleted.text, "old\n");
    assert_eq!(deleted.owners[0].file.status, FileStatus::Deleted);
    let binary = f
        .board
        .files_project_read("SHOP", "hero.png")
        .await
        .unwrap();
    assert!(binary.binary);
    assert_eq!((binary.size, binary.text.as_str()), (6, ""));
    let lock = f
        .board
        .files_project_read("SHOP", "pnpm-lock.yaml")
        .await
        .unwrap();
    assert!(lock.truncated);
    assert_eq!(lock.text.lines().count(), MAX_FILE_LINES);
    assert_eq!(lock.size, huge.len() as u64);
    // A file the base doesn't have reads as its ticket's diff.
    let added = f
        .board
        .files_project_read("SHOP", "e2e/login.spec.ts")
        .await
        .unwrap();
    assert!(!added.on_base && !added.not_on_base);
    assert_eq!(added.diff_of.unwrap().to_string(), "SHOP-2");
    assert_eq!(added.diff.unwrap().file.added, 2);
    for bad in ["../x", "/etc/hosts", ".git/config"] {
        let err = f.board.files_project_read("SHOP", bad).await.unwrap_err();
        assert_eq!(err.refusal(), Some(Refusal::BadPath), "{bad}");
    }

    // One owner's diff.
    let diff = f
        .board
        .files_project_diff("SHOP", "src/lib/pricing.ts", "SHOP-1")
        .await
        .unwrap();
    let changed: Vec<_> = diff.hunks[0]
        .lines
        .iter()
        .filter(|l| l.kind != LineKind::Context)
        .map(|l| (l.kind, l.text.as_str()))
        .collect();
    assert_eq!(changed, [(LineKind::Removed, "c"), (LineKind::Added, "C")]);
    let uncommitted = f
        .board
        .files_project_diff("SHOP", "src/lib/pricing.ts", "SHOP-2")
        .await
        .unwrap();
    assert_eq!(uncommitted.file.added, 1);
    let not_owner = f
        .board
        .files_project_diff("SHOP", "src/lib/pricing.ts", &backlog.key.to_string())
        .await
        .unwrap_err();
    assert!(not_owner.refusal().is_none());

    // All changes by ticket: the one that needs you first; SHOP-4 has none.
    let changes = f.board.files_project_changes("SHOP").await.unwrap();
    let groups: Vec<_> = changes
        .groups
        .iter()
        .map(|g| {
            (
                g.ticket.key.to_string(),
                g.ticket.needs_you,
                g.ticket.files,
                g.files
                    .iter()
                    .map(|e| (e.file.path.as_str(), e.uncommitted))
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    assert_eq!(
        groups,
        [
            (
                "SHOP-2".to_owned(),
                true,
                2,
                vec![("e2e/login.spec.ts", true), ("src/lib/pricing.ts", true)]
            ),
            (
                "SHOP-1".to_owned(),
                false,
                3,
                vec![
                    ("hero.png", false),
                    ("src/lib/legacy-price.ts", false),
                    ("src/lib/pricing.ts", false)
                ]
            ),
        ]
    );
    let price_group = &changes.groups[1].ticket;
    assert_eq!((price_group.added, price_group.removed), (1, 2));
    assert_eq!(price_group.stage.as_deref(), Some("Dev"));
    assert_eq!(price_group.from, FilesFrom::Worktree);
    assert_eq!(changes.counts.changed, 4);

    // The picker: tickets with a worktree in flow order, and the one without.
    let sources = f.board.files_project_sources("SHOP").await.unwrap();
    let tickets: Vec<_> = sources
        .tickets
        .iter()
        .map(|t| (t.key.to_string(), t.files))
        .collect();
    assert_eq!(
        tickets,
        [
            ("SHOP-1".to_owned(), 3),
            ("SHOP-2".to_owned(), 2),
            ("SHOP-4".to_owned(), 0)
        ]
    );
    let none: Vec<_> = sources.no_worktree.iter().map(|t| t.key.clone()).collect();
    assert_eq!(none, std::slice::from_ref(&backlog.key));
    assert_eq!(sources.no_worktree[0].status, Status::Backlog);
    assert!(sources.epics.is_empty());

    // Search: names on the base and the owners' new files, changed first.
    let found = f.board.files_project_search("SHOP", "pric").await.unwrap();
    let paths: Vec<_> = found.matches.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths, ["src/lib/pricing.ts", "src/lib/legacy-price.ts"]);
    assert_eq!(found.matches[0].owners.len(), 2);
    let login = f.board.files_project_search("SHOP", "login").await.unwrap();
    assert_eq!(login.matches[0].path, "e2e/login.spec.ts");
    assert_eq!(login.matches[0].hit, [4, 9]);

    // Read-only: the user's checkout and the worktrees' indexes are as they were.
    assert_eq!(git(&repo, &["status", "--porcelain"]), checkout);
    assert_eq!((index_mtime(&price_wt), index_mtime(&coupon_wt)), indexes);
}

#[tokio::test]
async fn a_ticket_on_another_base_and_an_epic_child_are_tagged_with_their_branch() {
    let f = Fixture::shop().await;
    let repo = f.repo();
    git(&repo, &["branch", "release/2.3"]);

    // SHOP-1 on release/2.3: a file only there, and a file main has too.
    let vat = f
        .create_with(NewTicketInput {
            title: "VAT rounding hotfix".into(),
            base_branch: Some("release/2.3".into()),
            ..crate::common::shop_input()
        })
        .await;
    let vat_wt = f.start_worktree(&vat).await;
    std::fs::create_dir_all(vat_wt.join("src/lib")).unwrap();
    commit(&vat_wt, "src/lib/vat.ts", "vat\n");
    commit(&vat_wt, "file1.txt", "1\nhotfix\n");
    f.force_status(&vat, Status::Working).await;
    // SHOP-2 on main changes the same file: not "same file", the bases differ.
    let main = f.create("On main").await;
    let main_wt = f.start_worktree(&main).await;
    commit(&main_wt, "file1.txt", "1\nmain\n");
    // The Wishlist epic (SHOP-3) and its running child, on the epic's branch.
    let epic = f.epic("Wishlist").await;
    let button = f.child(&epic, "Wishlist button").await;
    let button_wt = f.start_worktree(&button).await;
    commit(&button_wt, "button.tsx", "b\n");
    f.force_status(&button, Status::Working).await;

    let top = f.board.files_project_tree("SHOP", "", false).await.unwrap();
    // `src` once, from the ticket on release/2.3 alone.
    let srcs = top.entries.iter().filter(|e| e.name == "src").count();
    assert_eq!(srcs, 1);
    assert_eq!(entry(&top, "src").changed_below, 1);
    let file1 = entry(&top, "file1.txt");
    let mut tagged = owners(&file1.owners);
    tagged.sort();
    assert_eq!(
        tagged,
        [
            ("SHOP-1".into(), FileStatus::Modified, false, true),
            ("SHOP-2".into(), FileStatus::Modified, false, false),
        ]
    );
    let on_release = file1
        .owners
        .iter()
        .find(|o| o.key.to_string() == "SHOP-1")
        .unwrap();
    assert_eq!(on_release.base_branch, "release/2.3");
    assert!(!file1.not_on_base);
    // The epic child: its epic's branch, a file only there.
    let child_file = entry(&top, "button.tsx");
    assert!(child_file.not_on_base);
    assert_eq!(child_file.owners[0].key, button.key);
    assert_eq!(child_file.owners[0].base_branch, "otaq/SHOP-3");
    assert!(child_file.owners[0].other_base);
    let counts = top.counts.unwrap();
    assert_eq!((counts.tickets, counts.same_file), (3, 0));

    let lib = f
        .board
        .files_project_tree("SHOP", "src/lib", false)
        .await
        .unwrap();
    let vat_entries: Vec<_> = lib.entries.iter().filter(|e| e.name == "vat.ts").collect();
    assert_eq!(vat_entries.len(), 1);
    assert!(vat_entries[0].not_on_base);
    assert_eq!(
        owners(&vat_entries[0].owners),
        [("SHOP-1".into(), FileStatus::Added, false, true)]
    );
    // It reads as the ticket's diff.
    let read = f
        .board
        .files_project_read("SHOP", "src/lib/vat.ts")
        .await
        .unwrap();
    assert!(read.not_on_base);
    assert_eq!(read.diff_of.as_ref(), Some(&vat.key));
    assert_eq!(read.diff.unwrap().hunks[0].lines[0].text, "vat");

    let sources = f.board.files_project_sources("SHOP").await.unwrap();
    let epics: Vec<_> = sources
        .epics
        .iter()
        .map(|e| (e.key.to_string(), e.branch.as_str(), e.on_top.clone()))
        .collect();
    assert_eq!(
        epics,
        [("SHOP-3".to_owned(), "otaq/SHOP-3", vec![button.key.clone()])]
    );
    // The project's base first, then the other bases, each in flow order; the epic isn't an owner.
    let tickets: Vec<_> = sources
        .tickets
        .iter()
        .map(|t| (t.key.to_string(), t.other_base))
        .collect();
    assert_eq!(
        tickets,
        [
            ("SHOP-2".to_owned(), false),
            ("SHOP-1".to_owned(), true),
            ("SHOP-3.1".to_owned(), true),
        ]
    );
    assert!(sources.no_worktree.is_empty());
}

#[tokio::test]
async fn a_project_on_develop_reads_develop_whatever_the_checkout() {
    let f = Fixture::new().await;
    let repo = f.repos.repo("billing-api", 1);
    git(&repo, &["checkout", "--quiet", "-b", "develop"]);
    commit(&repo, "invoice.rs", "fn invoice() {}\n");
    git(&repo, &["checkout", "--quiet", "main"]);
    // The checkout is on main, with an edit of its own.
    std::fs::write(repo.join("file1.txt"), "dirty\n").unwrap();
    f.board
        .add_project(&AddProject {
            path: repo.display().to_string(),
            key: Some("BILL".into()),
            base_branch: Some("develop".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    let bill = f
        .board
        .create_ticket(
            &NewTicketInput {
                project: "BILL".into(),
                title: "Invoice totals".into(),
                ..Default::default()
            },
            &me(),
        )
        .await
        .unwrap();
    let wt = f.start_worktree(&bill).await;
    commit(&wt, "invoice.rs", "fn invoice() { total() }\n");

    let top = f.board.files_project_tree("BILL", "", false).await.unwrap();
    assert_eq!(top.source.base_branch, "develop");
    assert_eq!(top.source.base_sha, git(&repo, &["rev-parse", "develop"]));
    let invoice = entry(&top, "invoice.rs");
    assert_eq!(invoice.owners[0].base_branch, "develop");
    assert!(!invoice.owners[0].other_base);
    let file = f
        .board
        .files_project_read("BILL", "file1.txt")
        .await
        .unwrap();
    assert_eq!(file.text, "1\n");
    assert_eq!(git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]), "main");
    assert_eq!(git(&repo, &["status", "--porcelain"]), "M file1.txt");
    let sources = f.board.files_project_sources("BILL").await.unwrap();
    assert_eq!(sources.source.base_branch, "develop");
}

#[tokio::test]
async fn the_cache_reads_a_ticket_again_only_when_it_moved() {
    let f = Fixture::shop().await;
    let a = f.create("A").await;
    let a_wt = f.start_worktree(&a).await;
    commit(&a_wt, "a.ts", "a\n");
    f.force_status(&a, Status::PrOpen).await;
    let b = f.create("B").await;
    let b_wt = f.start_worktree(&b).await;
    commit(&b_wt, "b.ts", "b\n");
    f.force_status(&b, Status::PrOpen).await;
    let cache = f.board.project_files();
    let changed = |tree: &ProjectTree| tree.counts.unwrap().changed;

    let tree = f.board.files_project_tree("SHOP", "", false).await.unwrap();
    assert_eq!((cache.recomputes(), changed(&tree)), (2, 2));
    f.board.files_project_changes("SHOP").await.unwrap();
    f.board.files_project_sources("SHOP").await.unwrap();
    assert_eq!(cache.recomputes(), 2);

    // A commit in A's worktree moves its HEAD: A alone is read again, and the new file shows.
    commit(&a_wt, "a2.ts", "a2\n");
    let tree = f.board.files_project_tree("SHOP", "", false).await.unwrap();
    assert_eq!((cache.recomputes(), changed(&tree)), (3, 3));
    assert_eq!(entry(&tree, "a2.ts").owners[0].key, a.key);

    // Staging in B's worktree moves its index.
    std::fs::write(b_wt.join("b2.ts"), "b2\n").unwrap();
    git(&b_wt, &["add", "b2.ts"]);
    let tree = f.board.files_project_tree("SHOP", "", false).await.unwrap();
    assert_eq!((cache.recomputes(), changed(&tree)), (4, 4));
    assert!(entry(&tree, "b2.ts").owners[0].uncommitted);

    // The ticket changed (a turn ended, a merge…): read again.
    f.force_status(&a, Status::NeedsYou).await;
    f.board.files_project_tree("SHOP", "", false).await.unwrap();
    assert_eq!(cache.recomputes(), 5);

    // Done: no longer an owner, and nothing read for it.
    git(
        &f.repo(),
        &["worktree", "remove", "--force", b_wt.to_str().unwrap()],
    );
    f.force_status(&b, Status::Merged).await;
    let tree = f.board.files_project_tree("SHOP", "", false).await.unwrap();
    assert_eq!(cache.recomputes(), 5);
    assert_eq!(tree.counts.unwrap().tickets, 1);
}

/// `pricing.ts` on the base: `applyDiscount` from line 30 to 66, `applyTier` from 68 to 76 (bodies indented, so git
/// names the function each change is in).
fn pricing_base() -> Vec<String> {
    let mut lines: Vec<String> = (1..30).map(|n| format!("// note {n}")).collect();
    lines.push("export function applyDiscount(total: Money, coupon?: Coupon): Money {".into());
    lines.extend((31..66).map(|n| format!("  const step{n} = {n}")));
    lines.push("}".into());
    lines.push(String::new());
    lines.push("export function applyTier(total: Money, tier?: Tier): Money {".into());
    lines.extend((69..76).map(|n| format!("  const rate{n} = {n}")));
    lines.push("}".into());
    lines
}

/// The base's lines with `first..=last` (1-based) rewritten; `0, 0` rewrites none.
fn rewrite(base: &[String], first: usize, last: usize) -> String {
    base.iter()
        .enumerate()
        .map(|(i, line)| match (first..=last).contains(&(i + 1)) {
            true => format!("{line} * 2\n"),
            false => format!("{line}\n"),
        })
        .collect()
}

fn ranges(file: &ProjectFile, key: &str) -> Vec<(u32, u32)> {
    file.lines
        .iter()
        .find(|l| l.key.to_string() == key)
        .unwrap_or_else(|| panic!("{key}'s lines in {:?}", file.lines))
        .ranges
        .iter()
        .map(|r| (r.start, r.lines))
        .collect()
}

#[tokio::test]
async fn two_tickets_on_one_file_get_the_note_with_their_lines() {
    let f = Fixture::shop().await;
    let repo = f.repo();
    std::fs::create_dir_all(repo.join("src/lib")).unwrap();
    let base = pricing_base();
    commit(&repo, "src/lib/pricing.ts", &rewrite(&base, 0, 0));
    let legacy: String = (1..=31).map(|n| format!("old {n}\n")).collect();
    commit(&repo, "src/lib/legacy-price.ts", &legacy);
    commit(&repo, "src/lib/cart.ts", "1\n2\n3\n4\n5\n6\n7\n8\n9\n10\n");

    // SHOP-19 changes lines 47–58 of applyDiscount and line 70 of applyTier, deletes legacy-price.ts, and
    // changes line 2 of cart.ts.
    f.skip_to(19).await;
    let first = f.create("Coupon expiry").await;
    assert_eq!(first.key.to_string(), "SHOP-19");
    let first_wt = f.start_worktree(&first).await;
    commit(&first_wt, "src/lib/pricing.ts", &{
        let mut text = rewrite(&base, 47, 58);
        text = text.replace("  const rate70 = 70\n", "  const rate70 = 70 * 3\n");
        text
    });
    git(&first_wt, &["rm", "--quiet", "src/lib/legacy-price.ts"]);
    git(&first_wt, &["commit", "--quiet", "-m", "drop legacy"]);
    commit(
        &first_wt,
        "src/lib/cart.ts",
        "1\ntwo\n3\n4\n5\n6\n7\n8\n9\n10\n",
    );
    f.force_status(&first, Status::Working).await;
    // SHOP-29 changes lines 41–52, uncommitted, and line 9 of cart.ts.
    f.skip_to(29).await;
    let second = f.create("Tier discount").await;
    assert_eq!(second.key.to_string(), "SHOP-29");
    let second_wt = f.start_worktree(&second).await;
    std::fs::write(second_wt.join("src/lib/pricing.ts"), rewrite(&base, 41, 52)).unwrap();
    commit(
        &second_wt,
        "src/lib/cart.ts",
        "1\n2\n3\n4\n5\n6\n7\n8\nnine\n10\n",
    );
    f.force_status(&second, Status::NeedsYou).await;

    let pricing = f
        .board
        .files_project_read("SHOP", "src/lib/pricing.ts")
        .await
        .unwrap();
    // Group order (the one that needs you first); the lines on the base's numbering.
    assert_eq!(ranges(&pricing, "SHOP-29"), [(41, 12)]);
    assert_eq!(ranges(&pricing, "SHOP-19"), [(47, 12), (70, 1)]);
    assert_eq!(pricing.lines[0].key, second.key);
    let [note] = pricing.same_file.as_slice() else {
        panic!("one note: {:?}", pricing.same_file);
    };
    // Furthest along first in the note (a tie here: by key), the spans merged, the name from git's hunk header.
    assert_eq!(
        note.note,
        "SHOP-19 and SHOP-29 both edit applyDiscount (lines 41\u{2013}58). Whichever merges second will rebase on the other."
    );
    assert_eq!(note.base_branch, "main");
    assert_eq!(note.function.as_deref(), Some("applyDiscount"));
    assert_eq!(
        note.spans
            .iter()
            .map(|s| (s.first, s.last))
            .collect::<Vec<_>>(),
        [(41, 58)]
    );

    let cart = f
        .board
        .files_project_read("SHOP", "src/lib/cart.ts")
        .await
        .unwrap();
    assert_eq!(ranges(&cart, "SHOP-19"), [(2, 1)]);
    assert_eq!(ranges(&cart, "SHOP-29"), [(9, 1)]);
    assert_eq!(
        cart.same_file[0].note,
        "SHOP-19 and SHOP-29 both change this file."
    );

    // A deleted file: the deleting ticket's whole range, as on the base, and no note.
    let deleted = f
        .board
        .files_project_read("SHOP", "src/lib/legacy-price.ts")
        .await
        .unwrap();
    assert_eq!(ranges(&deleted, "SHOP-19"), [(1, 31)]);
    assert!(deleted.same_file.is_empty());

    // The Same file count still comes from paths.
    let tree = f.board.files_project_tree("SHOP", "", false).await.unwrap();
    assert_eq!(tree.counts.unwrap().same_file, 2);
}

#[tokio::test]
async fn three_owners_are_named_furthest_along_first_and_other_bases_are_never_compared() {
    let f = Fixture::shop().await;
    let repo = f.repo();
    let money = "export function round(m: Money): Money {\n  const r2 = 2\n  const r3 = 3\n  const r4 = 4\n  \
                 const r5 = 5\n  const r6 = 6\n  const r7 = 7\n}\n";
    commit(&repo, "money.ts", money);
    git(&repo, &["branch", "release/2.3"]);

    // SHOP-1 waits for you in Review (needs you, later in the flow): lines 4–6.
    let review = f.create("Rounding mode").await;
    let review_wt = f.start_worktree(&review).await;
    commit(
        &review_wt,
        "money.ts",
        &money
            .replace("r4 = 4", "r4 = 40")
            .replace("r5 = 5", "r5 = 50")
            .replace("r6 = 6", "r6 = 60"),
    );
    f.force_status(&review, Status::WaitingForYou).await;
    // SHOP-2 in Dev: line 5.
    let dev = f.create("Half up").await;
    let dev_wt = f.start_worktree(&dev).await;
    commit(&dev_wt, "money.ts", &money.replace("r5 = 5", "r5 = 5.0"));
    f.force_status(&dev, Status::Working).await;
    // SHOP-3 in Dev: two new lines after line 5, inside SHOP-1's 4–6.
    let adds = f.create("Bankers rounding").await;
    let adds_wt = f.start_worktree(&adds).await;
    commit(
        &adds_wt,
        "money.ts",
        &money.replace("r5 = 5\n", "r5 = 5\n  const a = 1\n  const b = 2\n"),
    );
    f.force_status(&adds, Status::Working).await;
    // SHOP-4 on release/2.3 changes line 5 too: never compared.
    let hotfix = f
        .create_with(NewTicketInput {
            title: "Rounding hotfix".into(),
            base_branch: Some("release/2.3".into()),
            ..crate::common::shop_input()
        })
        .await;
    let hotfix_wt = f.start_worktree(&hotfix).await;
    commit(&hotfix_wt, "money.ts", &money.replace("r5 = 5", "r5 = 5.5"));
    f.force_status(&hotfix, Status::Working).await;

    let file = f
        .board
        .files_project_read("SHOP", "money.ts")
        .await
        .unwrap();
    assert_eq!(ranges(&file, "SHOP-3"), [(5, 0)]);
    assert_eq!(ranges(&file, "SHOP-4"), [(5, 1)]);
    let [note] = file.same_file.as_slice() else {
        panic!("one note, for main only: {:?}", file.same_file);
    };
    assert_eq!(note.base_branch, "main");
    assert_eq!(
        note.note,
        "SHOP-1, SHOP-2 and SHOP-3 all edit round (lines 4\u{2013}6). Each one that merges later will rebase on the ones before it."
    );
    assert!(!note.keys.contains(&hotfix.key));
}

/// #43: the user's own worktrees of the repository are in the picker's YOUR WORKTREES (not the main checkout, not a
/// ticket's), and one picked reads from git at its HEAD, read-only: committed files, not its edits on disk; any
/// other folder is refused.
#[tokio::test]
async fn your_own_worktrees_are_listed_and_read_from_git() {
    let f = Fixture::shop().await;
    let repo = f.repo();
    seed_base(&repo);
    let ticket = f.create("Price table").await;
    let ticket_wt = f.start_worktree(&ticket).await;
    f.force_status(&ticket, Status::Working).await;

    let experiment = f.repos.root().join("habitat-experiment");
    let bisect = f.repos.root().join("bisect");
    let path = |p: &Path| p.display().to_string();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "experiment/accent",
            &path(&experiment),
        ],
    );
    commit(&experiment, "accent.css", ":root { --accent: violet; }\n");
    std::fs::write(experiment.join("accent.css"), "edited on disk\n").unwrap();
    std::fs::write(experiment.join("scratch.txt"), "untracked\n").unwrap();
    git(
        &repo,
        &[
            "worktree",
            "add",
            "--quiet",
            "--detach",
            &path(&bisect),
            "main",
        ],
    );
    let checkout = git(&experiment, &["status", "--porcelain"]);
    let head = git(&experiment, &["rev-parse", "HEAD"]);

    let sources = f.board.files_project_sources("SHOP").await.unwrap();
    let yours: Vec<_> = sources
        .yours
        .iter()
        .map(|w| (w.label().to_owned(), w.head == head))
        .collect();
    let main_short = &git(&repo, &["rev-parse", "main"])[..7];
    assert_eq!(
        yours,
        [
            (main_short.to_owned(), false),
            ("experiment/accent".to_owned(), true)
        ],
        "{:?}",
        sources.yours
    );
    assert!(
        sources
            .yours
            .iter()
            .all(|w| Path::new(&w.path) != ticket_wt)
    );
    let picked = sources.yours[1].path.clone();

    let tree = f
        .board
        .files_worktree_tree("SHOP", &picked, "")
        .await
        .unwrap();
    assert_eq!(tree.source.base_branch, "experiment/accent");
    assert_eq!(tree.source.worktree.as_deref(), Some(picked.as_str()));
    assert_eq!(tree.source.base_sha, head);
    let names: Vec<_> = tree.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "src",
            "accent.css",
            "file1.txt",
            "hero.png",
            "pnpm-lock.yaml"
        ]
    );
    assert!(tree.entries.iter().all(|e| e.owners.is_empty()));

    let file = f
        .board
        .files_worktree_read("SHOP", &picked, "accent.css")
        .await
        .unwrap();
    assert_eq!(file.text, ":root { --accent: violet; }\n");
    assert!(file.owners.is_empty() && file.on_base);
    let found = f
        .board
        .files_worktree_search("SHOP", &picked, "accent")
        .await
        .unwrap();
    assert_eq!(found.matches[0].path, "accent.css");
    assert!(
        f.board
            .files_worktree_read("SHOP", &picked, "scratch.txt")
            .await
            .is_err()
    );
    // Nothing was written in the worktree.
    assert_eq!(git(&experiment, &["status", "--porcelain"]), checkout);

    for other in [path(&repo), path(&ticket_wt), path(&f.repos.root())] {
        let refused = f.board.files_worktree_tree("SHOP", &other, "").await;
        assert!(refused.is_err(), "{other}");
    }
}
