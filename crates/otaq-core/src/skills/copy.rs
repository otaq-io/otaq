//! The copy writer (AGM-07; d0098, d0092): Otaq's copy of a skill in a working folder (a ticket's worktree, or the
//! project's checkout while a Project agent runs there, 0023's exception) is written twice, as
//! `.agents/skills/<name>/` (where Codex, Copilot and OpenCode look) and `.claude/skills/<name>/` (Claude Code and
//! Copilot), `<name>` being [`crate::naming::copy_name`]. Each folder holds the skill's files with their modes, a
//! `.gitignore` holding `*` (so neither shows in `git status` or a commit) and **Otaq's marker** ([`marker_file`]);
//! its SKILL.md's `name:` is the copy's name and `disable-model-invocation` is left out (d0098: the flag breaks
//! Copilot's skill tool), every other byte as it was.
//!
//! A copy is written under a temporary name beside its place and renamed into it, so a half-written one is never
//! visible; a copy already there is replaced the same way. Only folders carrying the marker are ever replaced or
//! removed: a user's own folder of the same name is left alone (the write is refused). Nothing else in the folder
//! or its `.git` is written, and no git command runs. Blocking file work: callers run it off the async threads.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::folder::{self, SKILL_FILE};
use crate::naming;

/// Where the copies go, relative to the working folder: the folder every tool but Claude Code reads, then Claude
/// Code's.
pub const SKILL_ROOTS: [&str; 2] = [".agents/skills", ".claude/skills"];

/// The `.gitignore` written into each copy: everything in the folder, itself too.
const IGNORE_ALL: &str = "*\n";

/// M27's copies (`otaq-custom-<step>`, WSK-05), which had no marker: recognised by these exact names and removed.
const OLD_STEPS: [&str; 4] = ["commit", "push", "open-pr", "validate"];

/// The marker file in each of Otaq's copies: `.otaq-copy`. Only a folder holding it is Otaq's to replace or remove.
pub fn marker_file() -> String {
    format!(".{}-copy", naming::COMMAND_NAME)
}

/// What a copy is for, written into its marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// A session in a ticket's worktree.
    Worktree,
    /// A Project agent in the project's checkout: removed when no running one needs it.
    Checkout,
}

impl Marker {
    fn text(self) -> String {
        let product = naming::PRODUCT_NAME;
        match self {
            Self::Worktree => {
                format!("{product}'s copy of a skill for the agents in this worktree.\n")
            }
            Self::Checkout => format!(
                "{product}'s copy of a skill for an agent running in this folder; {product} removes it when the \
                 agent ends.\n"
            ),
        }
    }
}

/// `<root>/.agents/skills/<name>` and `<root>/.claude/skills/<name>`.
pub fn copy_folders(root: &Path, name: &str) -> [PathBuf; 2] {
    SKILL_ROOTS.map(|r| root.join(r).join(name))
}

/// Whether `folder` is one of Otaq's copies (it holds the marker).
pub fn is_marked(folder: &Path) -> bool {
    folder.join(marker_file()).is_file()
}

/// `.otaq-commit.partial`: where a copy is written before it is renamed into place.
fn partial_name(name: &str) -> String {
    format!(".{name}.partial")
}

/// `.otaq-commit.old`: where a copy being replaced goes for the moment of the swap.
fn old_name(name: &str) -> String {
    format!(".{name}.old")
}

/// Writes the skill folder `source` into `root` as the copy `name` under both [`SKILL_ROOTS`], replacing a marked
/// copy of that name. Refused, with nothing left behind, when `name` isn't a copy's name, `source` has no readable
/// SKILL.md, or a folder of that name that isn't Otaq's is there; a failure on the second root removes the first.
pub fn write_copy(root: &Path, name: &str, source: &Path, marker: Marker) -> io::Result<()> {
    if name.is_empty()
        || name.len() > naming::SKILL_NAME_MAX
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} isn't a skill copy's name"),
        ));
    }
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} isn't a folder", root.display()),
        ));
    }
    let text = fs::read_to_string(source.join(SKILL_FILE))?;
    let skill_md = prepared(&text, name).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{SKILL_FILE}'s front matter can't be read"),
        )
    })?;
    for target in copy_folders(root, name) {
        if target.exists() && !is_marked(&target) {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "{} is there and isn't {}'s",
                    target.display(),
                    naming::PRODUCT_NAME
                ),
            ));
        }
    }
    let mut written = Vec::new();
    for parent in SKILL_ROOTS.map(|r| root.join(r)) {
        match write_one(source, &parent, name, &skill_md, marker) {
            Ok(()) => written.push(parent.join(name)),
            Err(e) => {
                for done in &written {
                    folder::remove(done);
                }
                return Err(e);
            }
        }
    }
    Ok(())
}

/// One copy in `parent` (`<root>/.agents/skills`), created when missing: written whole under its partial name, then
/// swapped into place.
fn write_one(
    source: &Path,
    parent: &Path,
    name: &str,
    skill_md: &str,
    marker: Marker,
) -> io::Result<()> {
    fs::create_dir_all(parent)?;
    let partial = parent.join(partial_name(name));
    folder::remove(&partial);
    fs::create_dir(&partial)?;
    let made = (|| {
        // Ignored from the first moment, even while partial.
        fs::write(partial.join(".gitignore"), IGNORE_ALL)?;
        folder::copy_into(source, &partial)?;
        // The skill's own `.gitignore`, if it has one, gives way.
        fs::write(partial.join(".gitignore"), IGNORE_ALL)?;
        fs::write(partial.join(marker_file()), marker.text())?;
        fs::write(partial.join(SKILL_FILE), skill_md)?;
        fs::set_permissions(&partial, fs::metadata(source)?.permissions())?;
        let target = parent.join(name);
        if target.exists() {
            let old = parent.join(old_name(name));
            folder::remove(&old);
            fs::rename(&target, &old)?;
            let swapped = fs::rename(&partial, &target);
            folder::remove(&old);
            swapped
        } else {
            fs::rename(&partial, &target)
        }
    })();
    if made.is_err() {
        folder::remove(&partial);
    }
    made
}

/// `text` (a SKILL.md) as a copy holds it: its front matter's `name:` line is `name: <name>` (added after the
/// opening line when it has none) and its `disable-model-invocation:` line is gone, line endings kept and every
/// other byte as it was. `None` when it has no front matter or the result wouldn't read.
pub fn prepared(text: &str, name: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len() + name.len());
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_start_matches('\u{feff}').trim_end() != "---" {
        return None;
    }
    let ending = &first[first.trim_end_matches(['\r', '\n']).len()..];
    let ending = if ending.is_empty() { "\n" } else { ending };
    out.push_str(first);
    if !first.ends_with('\n') {
        out.push('\n');
    }
    let name_line = |ending: &str| format!("name: {name}{ending}");
    let mut named = false;
    let mut in_front_matter = true;
    for line in lines {
        if in_front_matter && line.trim_end() == "---" {
            in_front_matter = false;
            if !named {
                out.push_str(&name_line(ending));
                named = true;
            }
        } else if in_front_matter && line.starts_with("name:") {
            let ending = &line[line.trim_end_matches(['\r', '\n']).len()..];
            out.push_str(&name_line(ending));
            named = true;
            continue;
        } else if in_front_matter && line.starts_with("disable-model-invocation:") {
            continue;
        }
        out.push_str(line);
    }
    (folder::skill_name(&out).as_deref() == Ok(name)).then_some(out)
}

/// Removes the copy `name` from both roots when it carries the marker (an unmarked folder of that name is never
/// touched), with any partial or old folder of it, then the roots' folders Otaq's copies left empty.
pub fn remove_copy(root: &Path, name: &str) {
    for parent in SKILL_ROOTS.map(|r| root.join(r)) {
        let target = parent.join(name);
        if is_marked(&target) {
            folder::remove(&target);
        }
        folder::remove(&parent.join(partial_name(name)));
        folder::remove(&parent.join(old_name(name)));
    }
    prune(root);
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

/// Removes M27's copies from `root` (AGM-07: "old `otaq-custom-*` copies are removed").
pub fn remove_old_copies(root: &Path) {
    for old in old_copies() {
        let path = root.join(old);
        if path.exists() {
            folder::remove(&path);
        }
    }
}

/// Every marked copy under `root`'s skill roots, relative to `root`: what the warm build never keeps or seeds
/// (d0082), with M27's copies.
pub fn copies_to_skip(root: &Path) -> Vec<PathBuf> {
    let mut skip = old_copies();
    for name in marked_copies(root) {
        skip.extend(SKILL_ROOTS.map(|r| Path::new(r).join(&name)));
    }
    skip
}

/// Removes `.agents/skills`, `.claude/skills` and then `.agents` and `.claude` when they are empty: the folders a
/// copy created in a checkout go with the last copy. A folder with anything in it stays.
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
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    fn skill(dir: &Path, text: &str) -> PathBuf {
        let source = dir.join("source");
        fs::create_dir_all(source.join("scripts")).unwrap();
        fs::write(source.join(SKILL_FILE), text).unwrap();
        let script = source.join("scripts").join("run.sh");
        fs::write(&script, "#!/bin/sh\necho hi\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(source.join(".gitignore"), "target/\n").unwrap();
        source
    }

    const TEXT: &str = "---\nname: commit-compact\ndescription: Commits.\ndisable-model-invocation: true\n---\nname: in the body\ndisable-model-invocation: stays here\n";

    #[test]
    fn the_name_is_set_and_the_flag_left_out_byte_for_byte() {
        assert_eq!(
            prepared(TEXT, "x-commit-compact").unwrap(),
            "---\nname: x-commit-compact\ndescription: Commits.\n---\nname: in the body\n\
             disable-model-invocation: stays here\n"
        );
        assert_eq!(
            prepared(
                "---\r\nname: \"my pr\"\r\ndescription: x\r\n---\r\nbody\r\n",
                "x-my-pr"
            )
            .unwrap(),
            "---\r\nname: x-my-pr\r\ndescription: x\r\n---\r\nbody\r\n"
        );
        assert_eq!(
            prepared("---\ndescription: x\n---\nbody\n", "x-x").unwrap(),
            "---\ndescription: x\nname: x-x\n---\nbody\n",
            "a front matter without a name gets one"
        );
        assert_eq!(prepared("no front matter\nname: x\n", "x-x"), None);
        assert_eq!(prepared("---\nname: x\n", "x-x"), None, "never closed");
    }

    #[test]
    fn both_roots_with_the_ignore_file_the_marker_and_the_modes() {
        let dir = tempfile::tempdir().unwrap();
        let source = skill(dir.path(), TEXT);
        let root = dir.path().join("wt");
        fs::create_dir(&root).unwrap();
        write_copy(&root, "x-commit-compact", &source, Marker::Worktree).unwrap();
        for folder in copy_folders(&root, "x-commit-compact") {
            assert_eq!(
                fs::read_to_string(folder.join(".gitignore")).unwrap(),
                "*\n"
            );
            assert!(is_marked(&folder));
            assert!(
                fs::read_to_string(folder.join(marker_file()))
                    .unwrap()
                    .contains("worktree")
            );
            let md = fs::read_to_string(folder.join(SKILL_FILE)).unwrap();
            assert!(md.starts_with("---\nname: x-commit-compact\ndescription: Commits.\n---"));
            assert!(!md.contains("disable-model-invocation: true"));
            let mode = fs::metadata(folder.join("scripts/run.sh"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o755);
        }
        assert_eq!(marked_copies(&root), ["x-commit-compact"]);
        // Written again: replaced in place, still once.
        write_copy(&root, "x-commit-compact", &source, Marker::Worktree).unwrap();
        assert_eq!(marked_copies(&root), ["x-commit-compact"]);
        let left: Vec<_> = fs::read_dir(root.join(".agents/skills"))
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(left, ["x-commit-compact"], "no partial or old folder left");
    }

    #[test]
    fn a_failed_copy_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("wt");
        fs::create_dir(&root).unwrap();
        // No SKILL.md.
        let empty = dir.path().join("empty");
        fs::create_dir(&empty).unwrap();
        assert!(write_copy(&root, "x-x", &empty, Marker::Worktree).is_err());
        // A source whose file can't be read while copying (no permission).
        let source = skill(dir.path(), TEXT);
        let secret = source.join("secret");
        fs::write(&secret, "x").unwrap();
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&secret).is_err() {
            assert!(write_copy(&root, "x-commit-compact", &source, Marker::Worktree).is_err());
        }
        fs::set_permissions(&secret, fs::Permissions::from_mode(0o644)).unwrap();
        for r in SKILL_ROOTS {
            let entries: Vec<_> = fs::read_dir(root.join(r))
                .map(|e| e.flatten().map(|e| e.file_name()).collect())
                .unwrap_or_default();
            assert!(entries.is_empty(), "{r}: {entries:?}");
        }
        assert!(write_copy(&root, "Bad Name", &source, Marker::Worktree).is_err());
    }

    #[test]
    fn a_folder_that_isnt_otaqs_is_never_replaced_or_removed() {
        let dir = tempfile::tempdir().unwrap();
        let source = skill(dir.path(), TEXT);
        let root = dir.path().join("repo");
        let mine = root.join(".claude/skills/x-commit-compact");
        fs::create_dir_all(&mine).unwrap();
        fs::write(
            mine.join(SKILL_FILE),
            "---\nname: x-commit-compact\n---\nmine\n",
        )
        .unwrap();
        let refused = write_copy(&root, "x-commit-compact", &source, Marker::Checkout);
        assert_eq!(refused.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        assert!(!root.join(".agents/skills/x-commit-compact").exists());
        remove_copy(&root, "x-commit-compact");
        assert!(mine.join(SKILL_FILE).is_file());
        assert!(marked_copies(&root).is_empty());
    }

    #[test]
    fn removing_takes_the_copies_and_the_folders_they_made() {
        let dir = tempfile::tempdir().unwrap();
        let source = skill(dir.path(), TEXT);
        let root = dir.path().join("repo");
        fs::create_dir_all(root.join(".claude")).unwrap();
        fs::write(root.join(".claude/settings.json"), "{}").unwrap();
        write_copy(&root, "x-a", &source, Marker::Checkout).unwrap();
        write_copy(&root, "x-b", &source, Marker::Checkout).unwrap();
        assert_eq!(marked_copies(&root), ["x-a", "x-b"]);
        remove_copy(&root, "x-a");
        assert_eq!(marked_copies(&root), ["x-b"]);
        remove_copy(&root, "x-b");
        assert!(!root.join(".agents").exists());
        assert!(!root.join(".claude/skills").exists());
        assert!(
            root.join(".claude/settings.json").is_file(),
            "the user's own files stay"
        );
    }

    #[test]
    fn old_copies_are_removed_and_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let cmd = naming::COMMAND_NAME;
        let old = root.join(format!(".claude/skills/{cmd}-custom-commit"));
        fs::create_dir_all(&old).unwrap();
        fs::write(old.join(SKILL_FILE), "x").unwrap();
        let mine = root.join(format!(".claude/skills/{cmd}-custom-mine"));
        fs::create_dir_all(&mine).unwrap();
        assert!(old_copies().contains(&PathBuf::from(format!(
            ".agents/skills/{cmd}-custom-open-pr"
        ))));
        remove_old_copies(root);
        assert!(!old.exists());
        assert!(mine.exists(), "only M27's exact names");
    }
}
