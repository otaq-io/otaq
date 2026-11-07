//! The copies an older Otaq wrote, found and removed. Otaq never writes skills into a worktree or a checkout any more
//! (T2); before that it copied each skill a session used as `.agents/skills/otaq-<name>/` and
//! `.claude/skills/otaq-<name>/`, each folder holding **Otaq's marker** ([`marker_file`]). Only folders carrying the
//! marker are ever removed: a user's own folder of the same name is left alone. M27's still older `otaq-custom-*`
//! copies, which had no marker, are recognised by their exact names. What an older Otaq left goes at service start
//! ([`remove_everywhere`]) and with `otaq uninstall`. Blocking file work: callers run it off the async threads.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::folder;
use crate::board::Board;
use crate::naming;

/// Where the copies went, relative to the working folder: the folder every tool but Claude Code reads, then Claude
/// Code's.
pub const SKILL_ROOTS: [&str; 2] = [".agents/skills", ".claude/skills"];

/// M27's copies (`otaq-custom-<step>`, WSK-05), which had no marker: recognised by these exact names and removed.
const OLD_STEPS: [&str; 4] = ["commit", "push", "open-pr", "validate"];

/// The marker file in each of Otaq's copies: `.otaq-copy`. Only a folder holding it is Otaq's to remove.
pub fn marker_file() -> String {
    format!(".{}-copy", naming::COMMAND_NAME)
}

/// Whether `folder` is one of Otaq's copies (it holds the marker).
pub fn is_marked(folder: &Path) -> bool {
    folder.join(marker_file()).is_file()
}

/// Removes the copy `name` from both roots when it carries the marker (an unmarked folder of that name is never
/// touched), then the roots' folders Otaq's copies left empty.
pub fn remove_copy(root: &Path, name: &str) {
    for parent in SKILL_ROOTS.map(|r| root.join(r)) {
        let target = parent.join(name);
        if is_marked(&target) {
            folder::remove(&target);
        }
    }
    prune(root);
}

/// Removes every marked copy in `root` (a worktree or a project's checkout). Folders without the marker are never
/// touched. Returns the names removed.
pub fn remove_marked_copies(root: &Path) -> Vec<String> {
    let names = marked_copies(root);
    for name in &names {
        remove_copy(root, name);
    }
    names
}

/// The names of the marked copies in `root`, under either root, sorted and once each.
pub fn marked_copies(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = SKILL_ROOTS
        .iter()
        .filter_map(|r| fs::read_dir(root.join(r)).ok())
        .flat_map(|entries| entries.flatten())
        .filter(|entry| is_marked(&entry.path()))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names.dedup();
    names
}

/// M27's copies, `otaq-custom-commit` and its kind, relative to the working folder: what [`remove_old_copies`]
/// removes and the warm build leaves out.
pub fn old_copies() -> Vec<PathBuf> {
    OLD_STEPS
        .into_iter()
        .flat_map(|step| {
            SKILL_ROOTS
                .map(|r| Path::new(r).join(format!("{}-custom-{step}", naming::COMMAND_NAME)))
        })
        .collect()
}

/// Removes M27's copies from `root` (their exact `otaq-custom-*` names), then the roots' folders they left empty.
/// Returns the names removed, once each.
pub fn remove_old_copies(root: &Path) -> Vec<String> {
    let mut names = Vec::new();
    for old in old_copies() {
        let path = root.join(&old);
        if path.exists() {
            folder::remove(&path);
            if let Some(name) = old.file_name() {
                names.push(name.to_string_lossy().into_owned());
            }
        }
    }
    prune(root);
    names.sort();
    names.dedup();
    names
}

/// Everything an older Otaq left in `root` (a worktree or a project's checkout): its marked copies and M27's
/// `otaq-custom-*` ones, removed, then the empty folders they made. Returns the names removed. Refused, with nothing
/// removed, when a skill root is there but can't be read; a `root` that is gone, or has no skill roots, has nothing
/// to remove.
pub fn remove_leftovers(root: &Path) -> io::Result<Vec<String>> {
    for r in SKILL_ROOTS {
        match fs::read_dir(root.join(r)) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    let mut names = remove_marked_copies(root);
    names.extend(remove_old_copies(root));
    names.sort();
    names.dedup();
    Ok(names)
}

/// The folders an older Otaq may have left copies in: every project's checkout (a Project agent and an epic lead
/// work there), every folder under `wt/<repo>/` (tickets', helpers' and kept worktrees; a delivery runs in its
/// ticket's) and every merge worktree under `scratch/`.
async fn folders(board: &Board) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = match board.projects().await {
        Ok(projects) => projects
            .into_iter()
            .map(|p| PathBuf::from(p.repo_path))
            .collect(),
        Err(e) => {
            tracing::warn!(error = %e, "can't list the projects to remove the skill copies left in their checkouts");
            Vec::new()
        }
    };
    let data = board.data();
    let (wt, scratch) = (data.worktrees_dir(), data.root().join("scratch"));
    let found = tokio::task::spawn_blocking(move || {
        let mut found = Vec::new();
        for repo in subfolders(&wt) {
            found.extend(subfolders(&repo));
        }
        found.extend(subfolders(&scratch));
        found
    })
    .await
    .unwrap_or_default();
    folders.extend(found);
    folders
}

/// The folders directly in `dir`, in path order. A `dir` that is gone has none; one that can't be read is logged.
fn subfolders(dir: &Path) -> Vec<PathBuf> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::warn!(folder = %dir.display(), error = %e, "can't read a folder to remove the skill copies left in it");
            return Vec::new();
        }
    };
    let mut folders: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path())
        .collect();
    folders.sort();
    folders
}

/// At service start, before any session runs: everything an older Otaq left ([`remove_leftovers`]) goes from every
/// worktree it knows and every project's checkout, off the async threads. Each folder's count is logged; a folder
/// that can't be read is logged and skipped. Otaq never writes skills into a worktree or a checkout, so once gone
/// they don't come back.
pub async fn remove_everywhere(board: &Board) {
    let folders = folders(board).await;
    let _ = tokio::task::spawn_blocking(move || {
        for folder in folders {
            match remove_leftovers(&folder) {
                Ok(names) if names.is_empty() => {}
                Ok(names) => tracing::info!(
                    folder = %folder.display(),
                    removed = names.len(),
                    "skill copies an older version left removed"
                ),
                Err(e) => tracing::warn!(
                    folder = %folder.display(),
                    error = %e,
                    "can't read a folder's skills to remove the copies an older version left; skipped"
                ),
            }
        }
    })
    .await;
}

/// Every marked copy under `root`'s skill roots, relative to `root`, with M27's copies: what the warm build never
/// keeps or seeds, so a leftover the service-start tidy hasn't reached yet never spreads to another worktree.
pub fn copies_to_skip(root: &Path) -> Vec<PathBuf> {
    let mut skip = old_copies();
    for name in marked_copies(root) {
        skip.extend(SKILL_ROOTS.map(|r| Path::new(r).join(&name)));
    }
    skip
}

/// Removes `.agents/skills`, `.claude/skills` and then `.agents` and `.claude` when they are empty: the folders the
/// copies made go with the last of them. A folder with anything in it stays.
fn prune(root: &Path) {
    for r in SKILL_ROOTS {
        let skills = root.join(r);
        let _ = fs::remove_dir(&skills);
        if let Some(parent) = skills.parent() {
            let _ = fs::remove_dir(parent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::testing::leftover_copy;

    #[test]
    fn a_folder_that_isnt_otaqs_is_never_removed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let mine = root.join(".claude/skills/x-commit-compact");
        fs::create_dir_all(&mine).unwrap();
        fs::write(
            mine.join(folder::SKILL_FILE),
            "---\nname: x-commit-compact\n---\nmine\n",
        )
        .unwrap();
        assert!(marked_copies(&root).is_empty());
        remove_copy(&root, "x-commit-compact");
        assert!(mine.join(folder::SKILL_FILE).is_file());
        assert!(remove_marked_copies(&root).is_empty());
        assert!(mine.join(folder::SKILL_FILE).is_file());
    }

    #[test]
    fn removing_takes_the_copies_and_the_folders_they_made() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        fs::create_dir_all(root.join(".claude")).unwrap();
        fs::write(root.join(".claude/settings.json"), "{}").unwrap();
        leftover_copy(&root, "x-a");
        leftover_copy(&root, "x-b");
        assert_eq!(marked_copies(&root), ["x-a", "x-b"]);
        remove_copy(&root, "x-a");
        assert_eq!(marked_copies(&root), ["x-b"]);
        assert_eq!(remove_marked_copies(&root), ["x-b"]);
        assert!(!root.join(".agents").exists());
        assert!(!root.join(".claude/skills").exists());
        assert!(
            root.join(".claude/settings.json").is_file(),
            "the user's own files stay"
        );
    }

    #[test]
    fn marked_copies_are_skipped_with_the_old_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        leftover_copy(root, "x-a");
        let skip = copies_to_skip(root);
        for r in SKILL_ROOTS {
            assert!(skip.contains(&Path::new(r).join("x-a")), "{skip:?}");
        }
        assert!(
            skip.iter()
                .any(|p| p.ends_with(format!("{}-custom-commit", naming::COMMAND_NAME)))
        );
    }

    #[test]
    fn old_copies_are_removed_and_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let cmd = naming::COMMAND_NAME;
        let old = root.join(format!(".claude/skills/{cmd}-custom-commit"));
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join(folder::SKILL_FILE), "x").unwrap();
        let mine = root.join(format!(".claude/skills/{cmd}-custom-mine"));
        fs::create_dir_all(&mine).unwrap();
        assert!(old_copies().contains(&PathBuf::from(format!(
            ".agents/skills/{cmd}-custom-open-pr"
        ))));
        assert_eq!(remove_old_copies(root), [format!("{cmd}-custom-commit")]);
        assert!(!old.exists());
        assert!(mine.exists(), "only M27's exact names");
    }

    #[test]
    fn leftovers_go_and_the_users_folders_stay() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let cmd = naming::COMMAND_NAME;
        leftover_copy(root, "x-a");
        leftover_copy(root, "x-b");
        let old = root.join(format!(".agents/skills/{cmd}-custom-push"));
        fs::create_dir_all(&old).unwrap();
        let mine = root.join(".claude/skills/x-c");
        fs::create_dir_all(&mine).unwrap();
        fs::write(mine.join(folder::SKILL_FILE), "mine").unwrap();
        assert_eq!(
            remove_leftovers(root).unwrap(),
            [format!("{cmd}-custom-push"), "x-a".into(), "x-b".into()]
        );
        assert!(!root.join(".agents").exists(), "emptied, so removed");
        assert!(mine.join(folder::SKILL_FILE).is_file());
        assert!(remove_leftovers(&root.join("gone")).unwrap().is_empty());
    }

    // Unix only: it takes the read right away with a file mode.
    #[cfg(unix)]
    #[test]
    fn a_skills_folder_that_cant_be_read_is_left_as_it_is() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        leftover_copy(root, "x-a");
        let skills = root.join(".claude/skills");
        fs::set_permissions(&skills, fs::Permissions::from_mode(0o000)).unwrap();
        let refused = fs::read_dir(&skills).is_err();
        let result = remove_leftovers(root);
        fs::set_permissions(&skills, fs::Permissions::from_mode(0o755)).unwrap();
        if refused {
            // Not when the tests run as root, who reads anything.
            assert!(result.is_err());
            assert!(
                is_marked(&root.join(".agents/skills/x-a")),
                "nothing removed"
            );
        }
    }
}
