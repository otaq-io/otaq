//! Skill folders for tests (any crate's): written under a temporary folder the test owns, never in a real home.

use std::path::{Path, PathBuf};

/// A skill folder `<root>/team-skills/<name>/` whose SKILL.md is named `name`; returns the folder.
pub fn skill_folder(root: &Path, name: &str) -> PathBuf {
    let folder = root.join("team-skills").join(name);
    std::fs::create_dir_all(&folder).expect("skill folder");
    std::fs::write(
        folder.join(super::folder::SKILL_FILE),
        format!("---\nname: {name}\ndescription: {name}\n---\nDo the step.\n"),
    )
    .expect("SKILL.md");
    folder
}

/// A copy an older Otaq left in `root` (a worktree or a checkout): `<root>/.agents/skills/<name>/` and
/// `<root>/.claude/skills/<name>/`, each with a SKILL.md and Otaq's marker. Otaq no longer writes these; tests of the
/// leftover tidy and the warm build make them.
pub fn leftover_copy(root: &Path, name: &str) {
    for skills in super::copy::SKILL_ROOTS {
        let folder = root.join(skills).join(name);
        std::fs::create_dir_all(&folder).expect("copy folder");
        std::fs::write(
            folder.join(super::folder::SKILL_FILE),
            format!("---\nname: {name}\ndescription: {name}\n---\nDo the step.\n"),
        )
        .expect("SKILL.md");
        std::fs::write(folder.join(".gitignore"), "*\n").expect(".gitignore");
        std::fs::write(
            folder.join(super::copy::marker_file()),
            "left by an older version\n",
        )
        .expect("marker");
    }
}

/// A skill as a Claude Code stand-in sees it typed: Otaq's own `x` as `/otaq:x`, the user's `team-validate` as
/// `/team-validate` ([`crate::agent_tools::AgentTool::skill_reference`]).
pub fn claude_skill(name: &str) -> String {
    crate::agent_tools::namespaced_reference("/", name)
}
