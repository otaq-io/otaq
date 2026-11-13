//! The skills the user already has, found where the agent tools load them from and only read (T3).
//!
//! Otaq never copies, imports or renames a user's skills: it reads the folders the tools read and lists what it
//! finds, for the flow editor's **Pick a skill…** and for the warning on a stage whose skill nobody has.
//!
//! - **Personal:** `$CLAUDE_CONFIG_DIR/skills` (else `~/.claude/skills`), `~/.agents/skills`, `$CODEX_HOME/skills`
//!   (else `~/.codex/skills`), `$COPILOT_HOME/skills` (else `~/.copilot/skills`), `$XDG_CONFIG_HOME/opencode/skills`
//!   (else `~/.config/opencode/skills`). Each tool's folder is found by its adapter, its variable followed as the
//!   tool follows it (an empty one is unset; F2).
//! - **Project** (the project's checkout): `.claude/skills`, `.agents/skills`, `.github/skills`, `.opencode/skills`.
//!
//! A skill is a folder holding a `SKILL.md`. Its name is the front matter's `name:`, else the folder's name; its
//! description the front matter's `description:`, on one line. Every folder holding a `SKILL.md` is listed: front
//! matter that is missing or doesn't read gives the folder's name and no description (F4). A name found in several
//! folders is listed once, with every place. Otaq's own skills (the plugin's, [`crate::agents::shipped::SKILLS`])
//! are listed too, marked as Otaq's; a name that matches one of them always means Otaq's skill.
//!
//! Links are followed, as the tools follow them: a skill folder (or its `SKILL.md`) linked from anywhere is read
//! (F3). A skill reached through several folders (the same real folder) is one place, at the first folder it is
//! reached from, with its real folder beside it. Only one level is read, and each folder's real place comes from
//! `canonicalize`, so a link loop is a folder that doesn't read, never a loop.
//!
//! **Plugins:** the skills of the plugins installed in Claude Code, Codex and Copilot are listed too, as
//! `<plugin>:<skill>` (each adapter knows where its tool keeps them, [`agent_tools::plugin_skill_folders`]). Otaq's
//! own plugin isn't among them.
//!
//! **Which tool has a skill** ([`FoundSkill::tools`]): a tool has a skill when it finds it in the ticket's worktree
//! ([`ProjectFolders`]): in a personal folder that tool reads, in the project's folders that tool reads when the skill
//! is in the worktree (committed at the base, when there is no worktree yet), or, for Claude Code only, in the
//! checkout's `.claude/skills`, which it reads from any worktree. A skill in another tool's folder doesn't count:
//! `~/.claude/skills/x` is Claude Code's, not Codex's or Copilot's. A plugin's skills are its tool's. Otaq's own
//! skills are had by every tool. Which folders each tool reads: [`personal_tools`], [`PROJECT`].
//! A tool that doesn't load a `SKILL.md` starting with a UTF-8 byte order mark (Codex) doesn't have such a skill
//! ([`Tool::reads_skill_with_bom`]); it is still listed, with that tool among the ones that skip it
//! ([`FoundSkill::skipped`]), so the user is told why instead of where to put a skill that is already there.
//!
//! Each tool's variables (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`, `COPILOT_HOME`, `XDG_CONFIG_HOME`) are read from the
//! environment Otaq launches the tool with ([`Homes::with_env`]; the service passes the project's variables over its
//! own), so the finder looks where the tool will.
//!
//! Reading only: nothing is written and a missing folder is normal. Blocking file work: callers run it off the
//! service's async threads.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent_tools::{self, EnvVar, PersonalSkillFolders, Tool};
use crate::agents::shipped;

/// The file every skill folder has.
pub const SKILL_FILE: &str = "SKILL.md";

/// Otaq's own skills as shipped in its plugin (`plugin/skills/<name>/SKILL.md`), embedded at build time: their
/// names and descriptions need no installed bundle. The same names as [`shipped::SKILLS`] (tested).
const SHIPPED_TEXTS: &[(&str, &str)] = &[
    (
        "address-review",
        include_str!("../../../../plugin/skills/address-review/SKILL.md"),
    ),
    (
        "answer-question",
        include_str!("../../../../plugin/skills/answer-question/SKILL.md"),
    ),
    (
        "commit",
        include_str!("../../../../plugin/skills/commit/SKILL.md"),
    ),
    (
        "flow",
        include_str!("../../../../plugin/skills/flow/SKILL.md"),
    ),
    (
        "milestone-plan",
        include_str!("../../../../plugin/skills/milestone-plan/SKILL.md"),
    ),
    (
        "noyan",
        include_str!("../../../../plugin/skills/noyan/SKILL.md"),
    ),
    (
        "open-pr",
        include_str!("../../../../plugin/skills/open-pr/SKILL.md"),
    ),
    (
        "product-spec",
        include_str!("../../../../plugin/skills/product-spec/SKILL.md"),
    ),
    (
        "push",
        include_str!("../../../../plugin/skills/push/SKILL.md"),
    ),
    (
        "restack",
        include_str!("../../../../plugin/skills/restack/SKILL.md"),
    ),
    (
        "tech-spec",
        include_str!("../../../../plugin/skills/tech-spec/SKILL.md"),
    ),
    (
        "validate",
        include_str!("../../../../plugin/skills/validate/SKILL.md"),
    ),
    (
        "work-ticket",
        include_str!("../../../../plugin/skills/work-ticket/SKILL.md"),
    ),
];

/// Where the personal folders are: the user's home folder (`~/.agents/skills`) and each tool's own skills folder.
/// `Default` reads no personal folder (tests, parts of the service built on their own).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Homes {
    pub home: Option<PathBuf>,
    /// Each tool's personal skills folder, as its adapter finds it ([`agent_tools::personal_skill_folders`]).
    pub tools: Option<PersonalSkillFolders>,
    /// The personal folders each tool reads, as its adapter says ([`Tool::personal_skill_folders`]).
    pub readers: Vec<(Tool, Vec<PathBuf>)>,
}

impl Homes {
    /// The folders under `home`, each tool's variable read with `var`: the service passes the environment the
    /// project's sessions get ([`crate::board::Board::skill_tools`]).
    pub fn with_env(home: Option<&Path>, var: EnvVar<'_>) -> Self {
        Self {
            home: home.map(Path::to_path_buf),
            tools: home.map(|home| agent_tools::personal_skill_folders(home, var)),
            readers: home
                .map(|home| {
                    Tool::ALL
                        .iter()
                        .map(|&tool| (tool, tool.personal_skill_folders(home, var)))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// The folders under `home` with none of the tools' variables set.
    pub fn in_home(home: &Path) -> Self {
        Self::with_env(Some(home), &|_| None)
    }
}

/// Whether a skills folder is the user's own (under a home folder), the project's (in its checkout or the ticket's
/// worktree), or a plugin's installed in a tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillScope {
    Personal,
    Project,
    Plugin,
}

/// Where a ticket's tools find the project's skills (S6, "Which tool has a skill").
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectFolders {
    /// The project's checkout. Claude Code reads its `.claude/skills` from any worktree, committed or not; a Claude
    /// Code plugin installed for the project counts here.
    pub checkout: Option<PathBuf>,
    /// The ticket's worktree, when it has one: every tool reads the project's skills folders there, whatever is
    /// committed.
    pub worktree: Option<PathBuf>,
    /// Without a worktree: the skill folders committed at the ticket's base, relative to the checkout
    /// (`.agents/skills/team-validate`), which a new worktree gets. A skill in the checkout's folders that isn't
    /// among them is had only by Claude Code, and only from `.claude/skills`. `None` when git couldn't say: every
    /// skill in the checkout counts.
    pub committed: Option<HashSet<PathBuf>>,
}

impl ProjectFolders {
    /// Only the checkout, every skill in it counting as committed.
    pub fn checkout(checkout: Option<&Path>) -> Self {
        Self {
            checkout: checkout.map(Path::to_path_buf),
            ..Self::default()
        }
    }
}

/// One place a skill was found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillPlace {
    pub scope: SkillScope,
    /// The skills folder it is in (`/Users/me/.claude/skills`, `/repo/.github/skills`).
    pub folder: PathBuf,
    /// The skill's own folder in it.
    pub path: PathBuf,
    /// Where the skill's folder really is, when `path` is a link to somewhere else (F3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub real: Option<PathBuf>,
    /// The tools that find the skill here, in [`Tool::ALL`]'s order (S6): empty for a project skill that isn't
    /// committed, in a folder only the other tools read.
    #[serde(default)]
    pub tools: Vec<Tool>,
}

/// A skill found, once per name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FoundSkill {
    pub name: String,
    /// The front matter's `description:` on one line (Otaq's own for one of Otaq's names; else the first place's
    /// that has one). `None` when none says one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// One of Otaq's own skills, which come from its plugin.
    #[serde(default)]
    pub otaq: bool,
    /// Every folder it was found in, in the order the folders are read (personal first). Empty for Otaq's own
    /// skills when the user has none of that name.
    #[serde(default)]
    pub places: Vec<SkillPlace>,
    /// The tools that have it, in [`Tool::ALL`]'s order: every place's, and every tool for Otaq's own (S6).
    #[serde(default)]
    pub tools: Vec<Tool>,
    /// The tools that find it in a folder they read but don't load it, because its `SKILL.md` starts with a UTF-8
    /// byte order mark ([`Tool::reads_skill_with_bom`]), in [`Tool::ALL`]'s order: none of [`Self::tools`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<Tool>,
}

impl FoundSkill {
    /// Whether `tool` has it.
    pub fn has(&self, tool: Tool) -> bool {
        self.tools.contains(&tool)
    }
}

/// The skills folders read for `checkout`, in order: personal (under `homes`), then the project's (in `checkout`).
/// A folder that two of them name (a checkout that is the home folder) is read once. Plugins' folders aren't
/// among them.
pub fn folders(homes: &Homes, checkout: Option<&Path>) -> Vec<(SkillScope, PathBuf)> {
    sources(homes, &ProjectFolders::checkout(checkout), false)
        .into_iter()
        .map(|source| (source.scope, source.folder))
        .collect()
}

/// One skills folder the finder reads, and who finds a skill in it.
struct Source {
    scope: SkillScope,
    folder: PathBuf,
    /// The tools that find a skill here.
    tools: Vec<Tool>,
    /// A plugin's folder: its skills are named `<plugin>:<skill>`.
    plugin: Option<String>,
    /// For the checkout's folders when there is no worktree: the checkout and what is committed at the base. A skill
    /// that isn't committed is found only by [`Self::uncommitted`].
    committed: Option<(PathBuf, HashSet<PathBuf>)>,
    uncommitted: Vec<Tool>,
}

impl Source {
    fn new(scope: SkillScope, folder: PathBuf, tools: &[Tool]) -> Self {
        Self {
            scope,
            folder,
            tools: tools.to_vec(),
            plugin: None,
            committed: None,
            uncommitted: Vec::new(),
        }
    }

    /// The tools that find the skill whose folder is `path` (an entry of [`Self::folder`]).
    fn tools_for(&self, path: &Path) -> Vec<Tool> {
        match &self.committed {
            Some((checkout, committed))
                if !path
                    .strip_prefix(checkout)
                    .is_ok_and(|rel| committed.contains(rel)) =>
            {
                self.uncommitted.clone()
            }
            _ => self.tools.clone(),
        }
    }
}

/// The folders read for `project`, in order: personal, the project's, then (with `plugins`) the tools' plugins'.
/// A folder named twice is read once, as the first.
fn sources(homes: &Homes, project: &ProjectFolders, plugins: bool) -> Vec<Source> {
    let mut all = Vec::new();
    let mut personal = |folder: PathBuf| {
        let tools = personal_tools(homes, &folder);
        all.push(Source::new(SkillScope::Personal, folder, &tools));
    };
    if let Some(tools) = homes.tools.as_ref() {
        personal(tools.claude.clone());
    }
    if let Some(home) = homes.home.as_ref() {
        personal(under(home, AGENTS));
    }
    if let Some(tools) = homes.tools.as_ref() {
        personal(tools.codex.clone());
        personal(tools.copilot.clone());
        personal(tools.opencode.clone());
    }
    match (&project.worktree, &project.checkout) {
        (Some(worktree), checkout) => {
            all.extend(PROJECT.iter().map(|parts| {
                Source::new(
                    SkillScope::Project,
                    under(worktree, parts),
                    &project_tools(parts),
                )
            }));
            if let Some(checkout) = checkout {
                all.extend(PROJECT.iter().filter_map(|parts| {
                    let tools = checkout_tools(parts);
                    (!tools.is_empty())
                        .then(|| Source::new(SkillScope::Project, under(checkout, parts), &tools))
                }));
            }
        }
        (None, Some(checkout)) => {
            for parts in PROJECT {
                let mut source = Source::new(
                    SkillScope::Project,
                    under(checkout, parts),
                    &project_tools(parts),
                );
                if let Some(committed) = &project.committed {
                    source.committed = Some((checkout.clone(), committed.clone()));
                    source.uncommitted = checkout_tools(parts);
                }
                all.push(source);
            }
        }
        (None, None) => {}
    }
    if plugins && let Some(tools) = homes.tools.as_ref() {
        for (tool, plugin) in agent_tools::plugin_skill_folders(tools, project.checkout.as_deref())
        {
            let mut source = Source::new(SkillScope::Plugin, plugin.folder, &[tool]);
            source.plugin = Some(plugin.plugin);
            all.push(source);
        }
    }
    let mut seen = Vec::new();
    all.into_iter()
        .filter(|source| {
            if seen.contains(&source.folder) {
                return false;
            }
            seen.push(source.folder.clone());
            true
        })
        .collect()
}

// The skills folders Otaq names itself, each as its parts: a part never holds a separator, so a path built from
// them uses the platform's own throughout. Each tool's personal folder comes from its adapter.
/// Under the home folder: the shared `~/.agents/skills`.
const AGENTS: &[&str] = &[".agents", "skills"];
/// In the project's checkout (or the ticket's worktree). Which tools read each, their adapters say
/// ([`Tool::project_skill_folders`]).
pub const PROJECT: [&[&str]; 4] = [
    &[".claude", "skills"],
    &[".agents", "skills"],
    &[".github", "skills"],
    &[".opencode", "skills"],
];

/// The tools that read the personal skills folder `folder` ([`Homes::readers`]): `~/.claude/skills` is Claude
/// Code's (and OpenCode's), not Codex's or Copilot's.
pub fn personal_tools(homes: &Homes, folder: &Path) -> Vec<Tool> {
    homes
        .readers
        .iter()
        .filter(|(_, folders)| folders.iter().any(|f| f == folder))
        .map(|(tool, _)| *tool)
        .collect()
}

/// The tools that read the project's skills folder `parts` in the ticket's worktree.
pub fn project_tools(parts: &[&str]) -> Vec<Tool> {
    Tool::ALL
        .iter()
        .copied()
        .filter(|tool| tool.project_skill_folders().contains(&parts))
        .collect()
}

/// The tools that read the project's skills folder `parts` in the main checkout from a worktree, committed or not.
fn checkout_tools(parts: &[&str]) -> Vec<Tool> {
    Tool::ALL
        .iter()
        .copied()
        .filter(|tool| tool.checkout_skill_folders().contains(&parts))
        .collect()
}

/// `base` with `parts` joined one by one, so the path uses the platform's separator throughout (`C:\\Users\\me\\.copilot\\skills`,
/// never `C:\\Users\\me\\.copilot/skills`).
fn under(base: &Path, parts: &[&str]) -> PathBuf {
    parts
        .iter()
        .fold(base.to_owned(), |path, part| path.join(part))
}

/// Every skill in [`folders`] (and the tools' plugins) plus Otaq's own, once per name, by name, for `checkout`:
/// every skill in it counts as committed ([`ProjectFolders::checkout`]).
pub fn list(homes: &Homes, checkout: Option<&Path>) -> Vec<FoundSkill> {
    list_in(homes, &ProjectFolders::checkout(checkout))
}

/// Every skill the tools find for `project` plus Otaq's own, once per name, by name, each with the tools that have
/// it ([`FoundSkill::tools`]).
pub fn list_in(homes: &Homes, project: &ProjectFolders) -> Vec<FoundSkill> {
    let mut skills: BTreeMap<String, FoundSkill> = shipped_skills()
        .into_iter()
        .map(|skill| (skill.name.clone(), skill))
        .collect();
    // The real folders already listed: a skill linked into several folders is one place, at the first; the tools
    // that reach it from the others still have it.
    let mut seen = HashSet::new();
    for source in sources(homes, project, true) {
        for read in read_folder(&source.folder) {
            let name = match &source.plugin {
                Some(plugin) => format!("{plugin}:{}", read.name),
                None => read.name.clone(),
            };
            // A tool that doesn't load a SKILL.md starting with a byte order mark doesn't have it: it skips it.
            let (tools, skipped): (Vec<Tool>, Vec<Tool>) = source
                .tools_for(&read.path)
                .into_iter()
                .partition(|tool| !read.bom || tool.reads_skill_with_bom());
            if !seen.insert(read.real.clone()) {
                tracing::debug!(skill = %read.path.display(), real = %read.real.display(), "a skill already listed from another folder");
                if let Some(skill) = skills.get_mut(&name) {
                    skill.tools.extend(tools);
                    skill.skipped.extend(skipped);
                }
                continue;
            }
            let skill = skills.entry(name.clone()).or_insert_with(|| FoundSkill {
                name,
                description: None,
                otaq: false,
                places: Vec::new(),
                tools: Vec::new(),
                skipped: Vec::new(),
            });
            if skill.description.is_none() {
                skill.description = read.description;
            }
            let linked = read.real != read.unlinked;
            skill.tools.extend(tools.iter().copied());
            skill.skipped.extend(skipped);
            skill.places.push(SkillPlace {
                scope: source.scope,
                folder: source.folder.clone(),
                path: read.path,
                real: linked.then_some(read.real),
                tools,
            });
        }
    }
    let mut skills: Vec<FoundSkill> = skills.into_values().collect();
    for skill in &mut skills {
        if skill.otaq {
            skill.tools = Tool::ALL.to_vec();
        }
        skill.tools.sort();
        skill.tools.dedup();
        let had = skill.tools.clone();
        skill.skipped.retain(|tool| !had.contains(tool));
        skill.skipped.sort();
        skill.skipped.dedup();
    }
    skills
}

/// The skill named `name` as the tools find it for `project` ([`list_in`]), if anyone has it or skips it; one of
/// Otaq's own is had by every tool.
pub fn find(homes: &Homes, project: &ProjectFolders, name: &str) -> Option<FoundSkill> {
    list_in(homes, project)
        .into_iter()
        .find(|skill| skill.name == name)
}

/// The tools that have `name` for `project`: every tool for one of Otaq's own skills.
pub fn tools_with(homes: &Homes, project: &ProjectFolders, name: &str) -> Vec<Tool> {
    if is_shipped(name) {
        return Tool::ALL.to_vec();
    }
    find(homes, project, name)
        .map(|skill| skill.tools)
        .unwrap_or_default()
}

/// Whether `tool` has `name` for `project`.
pub fn tool_has(homes: &Homes, project: &ProjectFolders, tool: Tool, name: &str) -> bool {
    tools_with(homes, project, name).contains(&tool)
}

/// Whether some tool has `name` for `checkout` (every skill in it counting as committed): one of Otaq's own
/// skills, or one found in [`folders`] or a tool's plugin.
pub fn has(homes: &Homes, checkout: Option<&Path>, name: &str) -> bool {
    !tools_with(homes, &ProjectFolders::checkout(checkout), name).is_empty()
}

/// Whether `name` is one of Otaq's own skills.
pub fn is_shipped(name: &str) -> bool {
    shipped::SKILLS.contains(&name)
}

/// Otaq's own skills, marked as Otaq's, with their shipped descriptions.
fn shipped_skills() -> Vec<FoundSkill> {
    shipped::SKILLS
        .iter()
        .map(|&name| FoundSkill {
            name: name.to_owned(),
            description: SHIPPED_TEXTS
                .iter()
                .find(|(n, _)| *n == name)
                .and_then(|(_, text)| front_matter(text).ok())
                .and_then(|front| front.description),
            otaq: true,
            places: Vec::new(),
            tools: Tool::ALL.to_vec(),
            skipped: Vec::new(),
        })
        .collect()
}

/// A skill read from a skills folder.
struct Read {
    name: String,
    description: Option<String>,
    /// The skill's folder in the skills folder.
    path: PathBuf,
    /// Where it really is: every link followed.
    real: PathBuf,
    /// Where it would be if it weren't a link itself: the skills folder's real place, and its name.
    unlinked: PathBuf,
    /// Whether its `SKILL.md` starts with a UTF-8 byte order mark, which some tools don't load
    /// ([`Tool::reads_skill_with_bom`]).
    bom: bool,
}

/// The skills in one skills folder, by folder name. A missing folder has none.
fn read_folder(folder: &Path) -> Vec<Read> {
    let entries = match fs::read_dir(folder) {
        Ok(entries) => entries,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::debug!(folder = %folder.display(), error = %e, "can't read a skills folder");
            }
            return Vec::new();
        }
    };
    // The folder itself may be a link (a skills folder kept with the user's dotfiles): a skill in it isn't a link
    // of its own.
    let real_folder = fs::canonicalize(folder).unwrap_or_else(|_| folder.to_path_buf());
    let mut found: Vec<_> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| read_skill(&real_folder, &entry.path()))
        .collect();
    found.sort_by(|a, b| a.path.cmp(&b.path));
    found
}

/// The skill in `path` (an entry of the skills folder whose real place is `real_folder`), if it is a folder holding
/// a `SKILL.md`. Links are followed; one that leads nowhere (or loops) isn't a skill.
fn read_skill(real_folder: &Path, path: &Path) -> Option<Read> {
    let file = path.join(SKILL_FILE);
    if !path.is_dir() || !file.is_file() {
        return None;
    }
    let real = match fs::canonicalize(path) {
        Ok(real) => real,
        Err(e) => {
            tracing::debug!(skill = %path.display(), error = %e, "a skill's folder can't be resolved");
            return None;
        }
    };
    let folder_name = path.file_name()?;
    // A SKILL.md that doesn't read, or whose front matter doesn't, still makes a skill: the folder's name, no
    // description (F4).
    let mut bom = false;
    let front = match fs::read(&file) {
        Ok(bytes) => {
            bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
            front_matter(&String::from_utf8_lossy(&bytes)).unwrap_or_else(|why| {
            tracing::debug!(file = %file.display(), why, "a SKILL.md's front matter doesn't read; listed by its folder's name");
            FrontMatter::default()
        })
        }
        Err(e) => {
            tracing::debug!(file = %file.display(), error = %e, "a SKILL.md can't be read; listed by its folder's name");
            FrontMatter::default()
        }
    };
    let name = front
        .name
        .unwrap_or_else(|| folder_name.to_string_lossy().into_owned());
    Some(Read {
        name,
        description: front.description,
        path: path.to_path_buf(),
        unlinked: real_folder.join(folder_name),
        real,
        bom,
    })
}

/// What a SKILL.md's front matter says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FrontMatter {
    /// Its `name:`, when it has a non-empty one.
    pub name: Option<String>,
    /// Its `description:`, folded to one line, when it has a non-empty one.
    pub description: Option<String>,
}

/// A SKILL.md's front matter: the lines between a first `---` line and the next one. Each line is `key: value`, a
/// comment, or an indented (or `- `) continuation of the key above; anything else, or no closing line, means the
/// front matter doesn't read.
pub fn front_matter(text: &str) -> Result<FrontMatter, String> {
    let unreadable = |why: &str| format!("{SKILL_FILE}'s front matter can't be read: {why}");
    let mut lines = text.trim_start_matches('\u{feff}').lines();
    if lines.next().map(str::trim_end) != Some("---") {
        return Err(unreadable("it must start with a --- line"));
    }
    let mut front = FrontMatter::default();
    let mut keyed = false;
    // The description's lines while they are read: its value, then its continuations.
    let mut description: Option<Vec<&str>> = None;
    let mut in_description = false;
    for (n, line) in lines.enumerate() {
        let number = n + 2;
        let trimmed = line.trim();
        if line.trim_end() == "---" {
            front.description = description.and_then(|parts| fold(&parts));
            return Ok(front);
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if line.starts_with([' ', '\t', '-']) {
            if !keyed {
                return Err(unreadable(&format!("line {number} continues no key")));
            }
            if in_description && let Some(parts) = description.as_mut() {
                parts.push(trimmed);
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            return Err(unreadable(&format!("line {number} isn't key: value")));
        };
        let key_ok = !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
        if !key_ok {
            return Err(unreadable(&format!("line {number} isn't key: value")));
        }
        keyed = true;
        in_description = key == "description";
        if key == "name" {
            let value = unquote(value.trim())
                .ok_or_else(|| unreadable(&format!("line {number} has an unclosed quote")))?;
            if !value.is_empty() && !matches!(value, "|" | ">") {
                front.name = Some(value.to_owned());
            }
        }
        if in_description {
            let value = value.trim();
            // A block scalar's indicator (`>`, `|-`…) isn't part of the text.
            let block = value.starts_with(['|', '>'])
                && value[1..]
                    .chars()
                    .all(|c| matches!(c, '-' | '+' | '0'..='9'));
            description = Some(if block { Vec::new() } else { vec![value] });
        }
    }
    Err(unreadable("it has no closing --- line"))
}

/// A description's lines on one line: joined, without the quotes around it, its whitespace folded.
fn fold(parts: &[&str]) -> Option<String> {
    let joined = parts.join(" ");
    let joined = joined.trim();
    let text = if let Some(inner) = joined
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
    {
        inner.replace("''", "'")
    } else if let Some(inner) = joined
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
    {
        inner.replace("\\\"", "\"").replace("\\\\", "\\")
    } else {
        joined.to_owned()
    };
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then_some(text)
}

/// A value without its matching quotes; `None` for an opening quote that isn't closed.
fn unquote(value: &str) -> Option<&str> {
    for quote in ['"', '\''] {
        if let Some(rest) = value.strip_prefix(quote) {
            return rest.strip_suffix(quote);
        }
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_skills_are_the_shipped_ones() {
        let names: Vec<&str> = SHIPPED_TEXTS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, shipped::SKILLS);
        for (name, text) in SHIPPED_TEXTS {
            let front = front_matter(text).unwrap();
            assert_eq!(front.name.as_deref(), Some(*name));
            assert!(front.description.is_some(), "{name} has a description");
        }
    }

    #[test]
    fn a_description_is_folded_to_one_line() {
        let read = |text: &str| front_matter(text).unwrap().description;
        assert_eq!(
            read("---\nname: a\ndescription: Does a thing.\n---\n").as_deref(),
            Some("Does a thing.")
        );
        assert_eq!(
            read("---\ndescription: >-\n  Two\n  lines.\nname: a\n---\n").as_deref(),
            Some("Two lines.")
        );
        assert_eq!(
            read("---\ndescription: |\n  One\n    two\n---\n").as_deref(),
            Some("One two")
        );
        assert_eq!(
            read("---\ndescription: plain\n  and more\n---\n").as_deref(),
            Some("plain and more")
        );
        assert_eq!(
            read("---\ndescription: 'The team''s own'\n---\n").as_deref(),
            Some("The team's own")
        );
        assert_eq!(
            read("---\ndescription: \"say \\\"hi\\\"\"\n---\n").as_deref(),
            Some("say \"hi\"")
        );
        assert_eq!(read("---\nname: a\ndescription:\n---\n"), None);
        assert_eq!(read("---\nname: a\n---\n"), None);
    }

    /// Each folder is joined part by part, so none of its parts holds a separator of its own: on Windows a
    /// `.copilot/skills` part would show as `C:\\Users\\me\\.copilot/skills`.
    #[test]
    fn the_folders_are_joined_part_by_part() {
        let homes = Homes::in_home(Path::new("home"));
        for parts in PROJECT.iter().chain([&AGENTS]) {
            assert!(
                parts.iter().all(|p| !p.contains(['/', '\\'])),
                "{parts:?} holds a separator"
            );
        }
        let found = folders(&homes, Some(Path::new("repo")));
        assert_eq!(found.len(), 9);
        for (_, folder) in &found {
            let parts: Vec<String> = folder
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect();
            assert!(
                parts.iter().all(|p| !p.contains('/') && !p.contains('\\')),
                "{parts:?}"
            );
            assert_eq!(
                parts.last().map(String::as_str),
                Some("skills"),
                "{parts:?}"
            );
        }
        let parts = |i: usize| -> Vec<String> {
            found[i]
                .1
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        };
        assert_eq!(parts(4), ["home", ".config", "opencode", "skills"]);
        assert_eq!(parts(5), ["repo", ".claude", "skills"]);
    }

    #[test]
    fn no_name_is_not_an_error_here() {
        let front = front_matter("---\ndescription: x\n---\n").unwrap();
        assert_eq!(front.name, None);
        assert!(front_matter("no front matter").is_err());
    }
}
