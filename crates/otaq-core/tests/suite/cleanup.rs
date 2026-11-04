//! Cleanup scan and delete (T-0116, REC-04; d0018, d0026) against temporary repositories: an orphan worktree, an
//! orphan branch, the kept dirty worktree of a Closed ticket, a leaked port and an orphan session are listed; the
//! scan changes nothing; delete removes exactly the confirmed selection and never the user's own branches.

use std::path::Path;
use std::process::Command;

use otaq_core::board::Refusal;
use otaq_core::domain::Status;
use otaq_core::recovery::cleanup::{CleanupGroup, CleanupItem, CleanupTarget};
use otaq_core::service::testing::TestService;
use otaq_core::session::SessionKind;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Every file under `dir` with its size.
fn files(dir: &Path) -> Vec<(String, u64)> {
    let mut all = Vec::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(at) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&at) else {
            continue;
        };
        for entry in entries.flatten() {
            let meta = entry.path().symlink_metadata().unwrap();
            if meta.is_dir() {
                stack.push(entry.path());
            }
            all.push((entry.path().display().to_string(), meta.len()));
        }
    }
    all.sort();
    all
}

fn summary(items: &[CleanupItem]) -> Vec<(CleanupGroup, String)> {
    items.iter().map(|i| (i.group, i.label.clone())).collect()
}

fn target(items: &[CleanupItem], group: CleanupGroup, label_part: &str) -> CleanupTarget {
    items
        .iter()
        .find(|i| i.group == group && i.label.contains(label_part))
        .unwrap_or_else(|| panic!("{group:?} {label_part} not listed"))
        .target()
}

#[tokio::test(flavor = "multi_thread")]
async fn scan_lists_leftovers_and_delete_removes_only_the_selection() {
    let w = TestService::start().await;
    let project = w.add_project("shop-web").await;
    let repo = Path::new(&project.repo_path).to_owned();
    let working = w.create("SHOP", "Working on it").await; // SHOP-1: owns its worktree and branch
    let closed = w.create("SHOP", "Closed with changes").await; // SHOP-2
    let merged = w.create("SHOP", "Merged long ago").await; // SHOP-3: still holds ports
    let parked = w.create("SHOP", "Back in Backlog").await; // SHOP-4: a shell left running
    w.start_worktree(&working).await;
    w.force_status(&working, Status::Working).await;
    let kept = w.start_worktree(&closed).await;
    std::fs::write(kept.join("draft.ts"), "wip\n").unwrap();
    w.force_status(&closed, Status::ClosedNotMerged).await;
    sqlx::query("UPDATE tickets SET worktree_kept = TRUE WHERE id = ?")
        .bind(&closed.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    sqlx::query("UPDATE tickets SET agent_port = 4031 WHERE id = ?")
        .bind(&merged.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    sqlx::query("INSERT INTO app_ports (ticket_id, app, port) VALUES (?, 'app', 4032)")
        .bind(&merged.id)
        .execute(w.board.store().pool())
        .await
        .unwrap();
    w.force_status(&merged, Status::Merged).await;
    let shell = w.shell(&parked, SessionKind::Shell).await;
    // Leftovers no ticket owns, and the user's own branch.
    let orphan = w.data.worktrees_dir().join("shop-web").join("SHOP-99");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "otaq/SHOP-99",
            &orphan.display().to_string(),
        ],
    );
    git(&repo, &["branch", "otaq/OLD-1"]);
    git(&repo, &["branch", "feature/mine"]);

    let before = (files(w.data.root()), git(&repo, &["branch", "--list"]));
    let items = w.cleanup.scan().await.unwrap();
    assert_eq!(
        summary(&items),
        [
            (CleanupGroup::Sessions, "SHOP-4 · shell session".into()),
            (CleanupGroup::Worktrees, "SHOP-99".into()),
            (CleanupGroup::Branches, "shop-web · otaq/OLD-1".into()),
            (CleanupGroup::Branches, "shop-web · otaq/SHOP-99".into()),
            (CleanupGroup::Ports, "SHOP-3 · ports 4031, 4032".into()),
            (
                CleanupGroup::KeptWorktrees,
                "SHOP-2 · Closed with changes".into()
            ),
        ]
    );
    let kept_item = items
        .iter()
        .find(|i| i.group == CleanupGroup::KeptWorktrees)
        .unwrap();
    assert_eq!(kept_item.uncommitted, ["draft.ts"]);
    assert!(kept_item.size_bytes.unwrap() > 0 && kept_item.age_ms.is_some());
    assert_eq!(
        kept_item.path.as_deref(),
        Some(kept.display().to_string().as_str())
    );
    let worktree_item = items
        .iter()
        .find(|i| i.group == CleanupGroup::Worktrees)
        .unwrap();
    assert_eq!(worktree_item.why, "no ticket uses this worktree");
    // The scan changes nothing, however often it runs.
    w.cleanup.scan().await.unwrap();
    assert_eq!(
        (files(w.data.root()), git(&repo, &["branch", "--list"])),
        before
    );
    assert!(w.sessions.info(&shell.session).unwrap().is_running());

    // Without the confirmation nothing happens; it names what would go.
    let old = target(&items, CleanupGroup::Branches, "OLD-1");
    let kept_target = target(&items, CleanupGroup::KeptWorktrees, "SHOP-2");
    let err = w
        .cleanup
        .delete(&[old.clone(), kept_target.clone()], false)
        .await
        .unwrap_err();
    assert_eq!(err.refusal(), Some(Refusal::NeedsConfirmation));
    assert!(err.to_string().contains("otaq/OLD-1") && err.to_string().contains("SHOP-2"));
    assert!(git(&repo, &["branch", "--list"]).contains("otaq/OLD-1"));

    // Confirmed: exactly those two go, the dirty kept worktree included because it was listed.
    let outcome = w
        .cleanup
        .delete(&[old.clone(), kept_target.clone()], true)
        .await
        .unwrap();
    assert_eq!(outcome.deleted, [old, kept_target]);
    assert!(outcome.failed.is_empty());
    let branches = git(&repo, &["branch", "--list"]);
    assert!(!branches.contains("otaq/OLD-1"));
    assert!(branches.contains("otaq/SHOP-99") && branches.contains("feature/mine"));
    assert!(branches.contains("otaq/SHOP-1"), "a ticket's branch stays");
    assert!(!kept.exists());
    let row = w.board.store().ticket(&closed.id).await.unwrap().unwrap();
    assert!(!row.worktree_kept && row.worktree_path.is_none());
    assert!(orphan.is_dir(), "not selected, not deleted");
    assert!(w.sessions.info(&shell.session).unwrap().is_running());

    // The user's own branch is never a candidate, even when asked for by name.
    let outcome = w
        .cleanup
        .delete(
            &[CleanupTarget {
                group: CleanupGroup::Branches,
                id: "SHOP:feature/mine".into(),
            }],
            true,
        )
        .await
        .unwrap();
    assert_eq!(outcome.failed.len(), 1);
    assert!(git(&repo, &["branch", "--list"]).contains("feature/mine"));

    // The rest: the orphan worktree (then its branch), the leaked ports, the session.
    let items = w.cleanup.scan().await.unwrap();
    let outcome = w
        .cleanup
        .delete(
            &[
                target(&items, CleanupGroup::Worktrees, "SHOP-99"),
                target(&items, CleanupGroup::Ports, "SHOP-3"),
                target(&items, CleanupGroup::Sessions, "SHOP-4"),
            ],
            true,
        )
        .await
        .unwrap();
    assert_eq!(outcome.deleted.len(), 3, "{outcome:?}");
    assert!(!orphan.exists());
    assert!(!git(&repo, &["worktree", "list"]).contains("SHOP-99"));
    let row = w.board.store().ticket(&merged.id).await.unwrap().unwrap();
    assert_eq!((row.agent_port, row.run_port), (None, None));
    assert!(!w.sessions.info(&shell.session).unwrap().is_running());
    let items = w.cleanup.scan().await.unwrap();
    let outcome = w
        .cleanup
        .delete(&[target(&items, CleanupGroup::Branches, "SHOP-99")], true)
        .await
        .unwrap();
    assert_eq!(outcome.deleted.len(), 1);
    assert!(w.cleanup.scan().await.unwrap().is_empty());
    // The working ticket's worktree was never touched.
    assert!(
        Path::new(
            &w.board
                .ticket("SHOP-1")
                .await
                .unwrap()
                .work
                .unwrap()
                .worktree
                .unwrap()
        )
        .is_dir()
    );
    w.stop().await;
}
