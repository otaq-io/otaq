//! The prefixed copy of Otaq's shipped skills (decisions 072, 079, 092): Copilot and OpenCode run Otaq's skills under
//! names that carry the prefix in the skill's own name, `otaq-validate`, so a repository's or the user's own
//! `validate` can never hide one of Otaq's or be taken for it.
//!
//! - **Made from the shipped skills** by the runtime copy ([`crate::runtime`]), on the user's machine, never
//!   committed: each `<name>/` becomes `otaq-<name>/` ([`name`]), real files only (no links: Codex drops them, and a
//!   link needs admin rights on Windows).
//! - **Renamed inside too** ([`rewrite`]): the front matter's `name:` and every reference to another of Otaq's skills
//!   written the skills' way, "Otaq's `commit` skill", become `otaq-<name>`, so a skill sends the agent to the copy
//!   it is in. Flow files and their examples keep `otaq:validate` (decision 092): only those two forms change.
//! - **The `tuq:` skills** Otaq still ships (`product-spec`, `tech-spec`, `milestone-plan`) are typed as Otaq's until
//!   they run on the user's tuq, so they are prefixed the same way, `otaq-product-spec`.
//! - **How each tool types it**: Copilot lists a plugin's skills under the plugin's name, `/otaq:otaq-validate`
//!   (hands-on check, Copilot CLI 1.0.109, Windows, 2026-11-14: a typed `/otaq-validate` answered "Unknown command");
//!   OpenCode has no namespace and loads `otaq-validate` by name.

use std::fs;
use std::io;
use std::path::Path;

/// A shipped skill's name in the prefixed copy: `validate` → `otaq-validate`.
pub fn name(folder: &str) -> String {
    format!("{}-{folder}", super::OTAQ_NAMESPACE)
}

/// The prefixed name of Otaq's own skill `skill` (`otaq:validate` → `otaq-validate`, `tuq:product-spec` →
/// `otaq-product-spec`); `None` for any other name, which is the user's (decision 063).
pub fn own_name(skill: &str) -> Option<String> {
    super::own_folder(skill).map(name)
}

/// `text` (a skill's Markdown) with Otaq's skills renamed for the prefixed copy: the front matter's
/// `name: <shipped>` and every "Otaq's `<shipped>`" become `otaq-<shipped>`. Anything else, `otaq:validate` in a flow
/// example included, stays as it is.
pub fn rewrite(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    let mut front_matter = false;
    for (i, line) in text.split_inclusive('\n').enumerate() {
        let bare = line.trim_end_matches(['\n', '\r']);
        if bare == "---" && (i == 0 || front_matter) {
            front_matter = i == 0;
            out.push_str(line);
            continue;
        }
        match bare.strip_prefix("name:").map(str::trim) {
            Some(folder) if front_matter && is_shipped(folder) => {
                out.push_str(&format!("name: {}", name(folder)));
                out.push_str(&line[bare.len()..]);
            }
            _ => out.push_str(&rewrite_references(line)),
        }
    }
    out
}

/// "Otaq's `commit`" → "Otaq's `otaq-commit`", for every shipped skill named that way in `line`.
fn rewrite_references(line: &str) -> String {
    let lead = reference_lead();
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find(&lead) {
        let (before, after) = rest.split_at(at + lead.len());
        out.push_str(before);
        rest = after;
        if let Some(end) = rest.find('`')
            && is_shipped(&rest[..end])
        {
            out.push_str(&name(&rest[..end]));
            rest = &rest[end..];
        }
    }
    out.push_str(rest);
    out
}

/// What comes before a reference to one of Otaq's skills in a skill's text: "Otaq's `".
pub fn reference_lead() -> String {
    format!("{}'s `", crate::naming::PRODUCT_NAME)
}

fn is_shipped(folder: &str) -> bool {
    crate::agents::shipped::SKILLS.contains(&folder)
}

/// Writes the prefixed copy of the skills in `skills` (`<name>/SKILL.md`, …) into `dest`, which must not exist:
/// `dest/otaq-<name>/`, Markdown files rewritten ([`rewrite`]), any other file copied as it is.
pub fn write_copy(skills: &Path, dest: &Path) -> io::Result<()> {
    fs::create_dir_all(dest)?;
    for entry in fs::read_dir(skills)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().into_owned();
        copy_rewritten(&entry.path(), &dest.join(name(&folder)))?;
    }
    Ok(())
}

fn copy_rewritten(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if fs::metadata(&source)?.is_dir() {
            copy_rewritten(&source, &target)?;
        } else if source.extension().is_some_and(|e| e == "md") {
            fs::write(&target, rewrite(&fs::read_to_string(&source)?))?;
        } else {
            fs::copy(&source, &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::naming::{PLUGIN_NAMESPACE as NS, PRODUCT_NAME as PRODUCT};

    #[test]
    fn names() {
        assert_eq!(name("validate"), format!("{NS}-validate"));
        assert_eq!(
            own_name(&format!("{NS}:validate")),
            Some(format!("{NS}-validate"))
        );
        assert_eq!(
            own_name("tuq:product-spec"),
            Some(format!("{NS}-product-spec"))
        );
        assert_eq!(own_name("validate"), None);
        assert_eq!(own_name("team-validate"), None);
    }

    /// The front matter's name and "Otaq's `<skill>`" change; a flow example's `otaq:open-pr`, a name outside the
    /// front matter and the user's skills stay.
    #[test]
    fn rewrite_renames_the_name_and_references() {
        let text = format!(
            "---\r\nname: work-ticket\r\ndescription: Works.\r\n---\r\n\
             name: commit\n\
             Commit with {PRODUCT}'s `commit` skill, then {PRODUCT}'s `push` skill.\n\
             Not {PRODUCT}'s `team-validate` skill, nor {PRODUCT}'s `commit\n\
             skill: {NS}:open-pr\n"
        );
        assert_eq!(
            rewrite(&text),
            format!(
                "---\r\nname: {NS}-work-ticket\r\ndescription: Works.\r\n---\r\n\
                 name: commit\n\
                 Commit with {PRODUCT}'s `{NS}-commit` skill, then {PRODUCT}'s `{NS}-push` skill.\n\
                 Not {PRODUCT}'s `team-validate` skill, nor {PRODUCT}'s `commit\n\
                 skill: {NS}:open-pr\n"
            )
        );
    }

    #[test]
    fn write_copy_prefixes_folders_with_real_files() {
        let dir = tempfile::tempdir().unwrap();
        let skills = dir.path().join("skills");
        fs::create_dir_all(skills.join("commit/scripts")).unwrap();
        fs::write(
            skills.join("commit/SKILL.md"),
            "---\nname: commit\n---\nGo.\n",
        )
        .unwrap();
        fs::write(skills.join("commit/scripts/run.sh"), "echo `commit`").unwrap();
        fs::write(skills.join("README.md"), "not a skill").unwrap();
        let dest = dir.path().join("prefixed");
        write_copy(&skills, &dest).unwrap();
        let copied = dest.join(name("commit"));
        assert_eq!(
            fs::read_to_string(copied.join("SKILL.md")).unwrap(),
            format!("---\nname: {NS}-commit\n---\nGo.\n")
        );
        assert_eq!(
            fs::read_to_string(copied.join("scripts/run.sh")).unwrap(),
            "echo `commit`"
        );
        assert!(!dest.join("README.md").exists());
        assert!(!dest.join("commit").exists());
        let meta = fs::symlink_metadata(copied.join("SKILL.md")).unwrap();
        assert!(meta.is_file());
    }
}
