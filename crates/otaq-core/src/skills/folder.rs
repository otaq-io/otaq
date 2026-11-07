//! A skill folder on disk: its SKILL.md's name, deleting a folder, and the fingerprint that tells whether a
//! folder changed since (a written skill's Undo, [`crate::flow::write_skill`]). Otaq never copies or imports a skill
//! (T6). Blocking file work: callers run it off the service's async threads.

use std::fs;
use std::io;
use std::path::Path;

use sha2::{Digest, Sha256};

pub use super::found::SKILL_FILE;

/// Deletes a skill folder (or anything at `path`); a missing one is fine.
pub fn remove(path: &Path) {
    let removed = match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
        Ok(_) => fs::remove_file(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    };
    if let Err(e) = removed {
        tracing::warn!(path = %path.display(), error = %e, "can't delete a skill folder");
    }
}

/// The `name:` of a SKILL.md's front matter ([`super::found::front_matter`], whose rules it keeps): an error when
/// the front matter doesn't read or has no `name:`.
pub fn skill_name(text: &str) -> Result<String, String> {
    super::found::front_matter(text)?
        .name
        .ok_or_else(|| format!("{SKILL_FILE}'s front matter has no name:"))
}

/// A hash of the folder's files: their paths, modes and contents (and any link's target), in path order.
pub fn fingerprint(folder: &Path) -> io::Result<String> {
    let mut hash = Sha256::new();
    hash_dir(folder, Path::new(""), &mut hash)?;
    Ok(hash.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

fn hash_dir(folder: &Path, rel: &Path, hash: &mut Sha256) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(folder)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let rel = rel.join(entry.file_name());
        let meta = fs::symlink_metadata(&path)?;
        let mut head = |kind: &str| {
            hash.update(kind.as_bytes());
            hash.update(rel.to_string_lossy().as_bytes());
            hash.update([0]);
            hash.update(mode(&meta).to_le_bytes());
        };
        if meta.is_symlink() {
            head("L");
            hash.update(fs::read_link(&path)?.to_string_lossy().as_bytes());
        } else if meta.is_dir() {
            head("D");
            hash_dir(&path, &rel, hash)?;
        } else {
            head("F");
            let contents = fs::read(&path)?;
            hash.update((contents.len() as u64).to_le_bytes());
            hash.update(&contents);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn mode(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode() & 0o7777
}

#[cfg(not(unix))]
fn mode(meta: &fs::Metadata) -> u32 {
    u32::from(meta.permissions().readonly())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_comes_from_the_front_matter() {
        let ok = "---\nname: my-commit\ndescription: >\n  Commits.\ntags:\n  - a\n---\nBody\n";
        assert_eq!(skill_name(ok).unwrap(), "my-commit");
        assert_eq!(skill_name("---\nname: \"my pr\"\n---\n").unwrap(), "my pr");
        for (bad, why) in [
            ("No front matter", "it must start with a --- line"),
            ("---\nname: x\n", "it has no closing --- line"),
            ("---\nname my-commit\n---\n", "line 2 isn't key: value"),
            (
                "---\n  indented: x\nname: x\n---\n",
                "line 2 continues no key",
            ),
            ("---\nname: \"x\n---\n", "line 2 has an unclosed quote"),
        ] {
            assert_eq!(
                skill_name(bad).unwrap_err(),
                format!("SKILL.md's front matter can't be read: {why}"),
                "{bad}"
            );
        }
        assert_eq!(
            skill_name("---\ndescription: x\n---\n").unwrap_err(),
            "SKILL.md's front matter has no name:"
        );
    }
}
