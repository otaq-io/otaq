//! The skills the user already has (T3): Otaq reads the folders the agent tools load skills from, in a temporary
//! home and a temporary checkout, and lists what it finds without writing anything. Otaq never copies, imports or
//! renames a user's skills; its own skills come from its plugin and are listed marked as Otaq's.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use otaq_core::agent_tools::Tool;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::testing::TestService;
use otaq_core::skills::found::{
    FoundSkill, Homes, ProjectFolders, SkillPlace, SkillScope, folders, list, list_in, tool_has,
    tools_with,
};
use tempfile::TempDir;

/// Writes `<folder>/<dir>/SKILL.md` holding `text`; returns the skill's folder.
fn skill(folder: &Path, dir: &str, text: &str) -> PathBuf {
    let path = folder.join(dir);
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(path.join("SKILL.md"), text).unwrap();
    path
}

fn named(name: &str, description: &str) -> String {
    format!("---\nname: {name}\ndescription: {description}\n---\n\nBody.\n")
}

fn find<'a>(skills: &'a [FoundSkill], name: &str) -> Option<&'a FoundSkill> {
    skills.iter().find(|s| s.name == name)
}

/// Every file and folder under `root`, to tell nothing was written.
fn tree(root: &Path) -> Vec<PathBuf> {
    let mut all = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(path.clone());
            }
            all.push(path);
        }
    }
    all.sort();
    all
}

#[test]
fn the_folders_are_the_tools_personal_and_project_ones() {
    let home = PathBuf::from("/home/me");
    let homes = Homes::in_home(&home);
    let repo = PathBuf::from("/repo");
    let read: Vec<(SkillScope, PathBuf)> = folders(&homes, Some(&repo));
    let expected = [
        (SkillScope::Personal, home.join(".claude/skills")),
        (SkillScope::Personal, home.join(".agents/skills")),
        (SkillScope::Personal, home.join(".codex/skills")),
        (SkillScope::Personal, home.join(".copilot/skills")),
        (SkillScope::Personal, home.join(".config/opencode/skills")),
        (SkillScope::Project, repo.join(".claude/skills")),
        (SkillScope::Project, repo.join(".agents/skills")),
        (SkillScope::Project, repo.join(".github/skills")),
        (SkillScope::Project, repo.join(".opencode/skills")),
    ];
    assert_eq!(read, expected);
    // No home: only the project's.
    assert_eq!(folders(&Homes::default(), Some(&repo)).len(), 4);
}

/// The environment `vars` says, nothing else.
fn env(vars: &[(&str, &Path)]) -> impl Fn(&str) -> Option<OsString> + use<> {
    let vars: HashMap<String, OsString> = vars
        .iter()
        .map(|(k, v)| ((*k).to_owned(), v.as_os_str().to_owned()))
        .collect();
    move |name| vars.get(name).cloned()
}

/// F2: each tool's variable moves its personal folder, as the tool itself reads it; an empty one is unset.
#[test]
fn each_tools_variable_moves_its_personal_folder() {
    let home = PathBuf::from("/home/me");
    let var = env(&[
        ("CLAUDE_CONFIG_DIR", Path::new("/cfg/claude")),
        ("CODEX_HOME", Path::new("/cfg/codex")),
        ("COPILOT_HOME", Path::new("/cfg/copilot")),
        ("XDG_CONFIG_HOME", Path::new("/cfg/xdg")),
    ]);
    let read = folders(&Homes::with_env(Some(&home), &var), None);
    let personal: Vec<PathBuf> = read.into_iter().map(|(_, f)| f).collect();
    assert_eq!(
        personal,
        [
            PathBuf::from("/cfg/claude/skills"),
            home.join(".agents/skills"),
            PathBuf::from("/cfg/codex/skills"),
            PathBuf::from("/cfg/copilot/skills"),
            PathBuf::from("/cfg/xdg/opencode/skills"),
        ]
    );

    // Empty variables are unset: the folders under the home folder.
    let empty = env(&[
        ("CLAUDE_CONFIG_DIR", Path::new("")),
        ("CODEX_HOME", Path::new("")),
        ("COPILOT_HOME", Path::new("")),
        ("XDG_CONFIG_HOME", Path::new("")),
    ]);
    assert_eq!(
        folders(&Homes::with_env(Some(&home), &empty), None),
        folders(&Homes::in_home(&home), None)
    );
}

/// F2 on disk: a skill under each variable's folder is found, and the folder it moved away from isn't read.
#[test]
fn skills_are_read_where_each_variable_points() {
    let home = TempDir::new().unwrap();
    let cfg = TempDir::new().unwrap();
    let (h, c) = (home.path(), cfg.path());
    let (claude, codex, copilot, xdg) = (
        c.join("claude"),
        c.join("codex"),
        c.join("copilot"),
        c.join("xdg"),
    );
    skill(&claude.join("skills"), "a", &named("from-claude-dir", "A."));
    skill(&codex.join("skills"), "b", &named("from-codex-home", "B."));
    skill(
        &copilot.join("skills"),
        "c",
        &named("from-copilot-home", "C."),
    );
    skill(
        &xdg.join("opencode/skills"),
        "d",
        &named("from-xdg-config", "D."),
    );
    for dir in [".claude", ".codex", ".copilot", ".config/opencode"] {
        skill(
            &h.join(dir).join("skills"),
            "old",
            &named(&format!("old{}", dir.replace(['.', '/'], "-")), "Old."),
        );
    }
    let var = env(&[
        ("CLAUDE_CONFIG_DIR", &claude),
        ("CODEX_HOME", &codex),
        ("COPILOT_HOME", &copilot),
        ("XDG_CONFIG_HOME", &xdg),
    ]);
    let skills = list(&Homes::with_env(Some(h), &var), None);
    for (name, folder) in [
        ("from-claude-dir", claude.join("skills")),
        ("from-codex-home", codex.join("skills")),
        ("from-copilot-home", copilot.join("skills")),
        ("from-xdg-config", xdg.join("opencode/skills")),
    ] {
        let found = find(&skills, name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(found.places[0].folder, folder);
    }
    assert!(
        skills.iter().all(|s| !s.name.starts_with("old")),
        "{skills:?}"
    );
}

#[test]
fn every_skill_in_the_folders_is_listed_once_with_every_place() {
    let home = TempDir::new().unwrap();
    let repo = TempDir::new().unwrap();
    let (h, r) = (home.path(), repo.path());
    let homes = Homes::in_home(h);

    // One name in two places: personal for Claude Code, and the project's for Copilot.
    let personal = skill(
        &h.join(".claude/skills"),
        "team-validate",
        &named("team-validate", "Validate the team's way."),
    );
    let project = skill(
        &r.join(".github/skills"),
        "team-validate",
        &named("team-validate", "Another copy."),
    );
    // No name: the folder's name; a description on several lines is folded.
    skill(
        &r.join(".opencode/skills"),
        "release-notes",
        "---\ndescription: >\n  Write the release\n  notes.\n---\n",
    );
    // Codex's personal folder, and OpenCode's.
    skill(&h.join(".codex/skills"), "x", &named("codex-only", "C."));
    skill(
        &h.join(".config/opencode/skills"),
        "oc",
        &named("oc-only", "O."),
    );
    // F4: a SKILL.md whose front matter doesn't read, or that has none, is listed by its folder's name with no
    // description; the rest of its folder still lists.
    skill(&h.join(".agents/skills"), "broken", "---\nname: other\n");
    skill(
        &h.join(".agents/skills"),
        "plain",
        "# Just a body\n\nDo it.\n",
    );
    skill(&h.join(".agents/skills"), "fine", &named("fine", "Fine."));
    // A folder with no SKILL.md isn't a skill, and neither is a loose file.
    std::fs::create_dir_all(r.join(".agents/skills/empty")).unwrap();
    std::fs::write(r.join(".agents/skills/notes.md"), "notes").unwrap();
    // The user's skill named like one of Otaq's is still Otaq's.
    skill(
        &r.join(".claude/skills"),
        "validate",
        &named("validate", "Mine."),
    );

    let before = (tree(h), tree(r));
    let skills = list(&homes, Some(r));
    assert_eq!(
        (tree(h), tree(r)),
        before,
        "reading writes nothing anywhere"
    );

    let team = find(&skills, "team-validate").unwrap();
    assert!(!team.otaq);
    assert_eq!(
        team.description.as_deref(),
        Some("Validate the team's way.")
    );
    assert_eq!(
        team.places,
        [
            SkillPlace {
                scope: SkillScope::Personal,
                folder: h.join(".claude/skills"),
                path: personal,
                real: None,
                tools: vec![Tool::Claude, Tool::OpenCode],
            },
            SkillPlace {
                scope: SkillScope::Project,
                folder: r.join(".github/skills"),
                path: project,
                real: None,
                tools: vec![Tool::Copilot],
            },
        ]
    );
    let notes = find(&skills, "release-notes").unwrap();
    assert_eq!(
        notes.description.as_deref(),
        Some("Write the release notes.")
    );
    assert_eq!(notes.places[0].scope, SkillScope::Project);
    assert_eq!(find(&skills, "codex-only").unwrap().places.len(), 1);
    assert_eq!(find(&skills, "oc-only").unwrap().places.len(), 1);
    assert!(find(&skills, "fine").is_some());
    for name in ["broken", "plain"] {
        let found = find(&skills, name).unwrap_or_else(|| panic!("{name}"));
        assert_eq!(found.description, None, "{name}");
        assert_eq!(found.places[0].folder, h.join(".agents/skills"), "{name}");
    }
    assert!(find(&skills, "other").is_none());
    assert!(find(&skills, "empty").is_none());
    assert!(find(&skills, "notes.md").is_none());

    let validate = find(&skills, "validate").unwrap();
    assert!(validate.otaq);
    assert_ne!(validate.description.as_deref(), Some("Mine."));
    assert_eq!(validate.places.len(), 1);

    // Listed once per name, by name.
    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(names, sorted);
}

#[test]
fn otaqs_own_skills_are_listed_and_marked_with_no_folders_at_all() {
    let skills = list(&Homes::default(), None);
    let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
    let mut shipped = otaq_core::agents::shipped::SKILLS.to_vec();
    shipped.sort_unstable();
    assert_eq!(names, shipped);
    for skill in &skills {
        assert!(skill.otaq, "{}", skill.name);
        assert!(skill.places.is_empty());
        assert!(skill.description.is_some(), "{}", skill.name);
    }
}

// Unix only: it makes symbolic links (an administrator's right on Windows).
/// F3: links are followed as the tools follow them, and a skill reached through several folders is listed once.
#[cfg(unix)]
#[test]
fn links_are_followed_and_a_skill_reached_twice_is_listed_once() {
    use std::os::unix::fs::symlink;
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let h = home.path();
    let claude = h.join(".claude/skills");
    let agents = h.join(".agents/skills");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::create_dir_all(&agents).unwrap();
    // A skill kept in the user's own repo, linked into two personal folders: listed once, at the first folder read
    // (Claude Code's), with its real folder.
    let away = skill(outside.path(), "away", &named("away", "Out."));
    symlink(&away, claude.join("away")).unwrap();
    symlink(&away, agents.join("away-too")).unwrap();
    // A SKILL.md linked from outside.
    let half_text = skill(outside.path(), "half-src", &named("half", "Half."));
    std::fs::create_dir_all(claude.join("half")).unwrap();
    symlink(half_text.join("SKILL.md"), claude.join("half/SKILL.md")).unwrap();
    // A link to a skill in the same folder is the same skill.
    skill(&claude, "real", &named("real", "In."));
    symlink(claude.join("real"), claude.join("alias")).unwrap();
    // Links that loop, or lead nowhere, aren't skills and don't stop the list.
    symlink(claude.join("loop-b"), claude.join("loop-a")).unwrap();
    symlink(claude.join("loop-a"), claude.join("loop-b")).unwrap();
    symlink(claude.join("self"), claude.join("self")).unwrap();
    symlink(outside.path().join("gone"), claude.join("dangling")).unwrap();
    // A skill folder holding a link back to its skills folder: only one level is read.
    let deep = skill(&agents, "deep", &named("deep", "Deep."));
    symlink(&agents, deep.join("up")).unwrap();

    let found = list(&Homes::in_home(h), None);
    let away_found = find(&found, "away").unwrap();
    assert_eq!(
        away_found.places,
        [SkillPlace {
            scope: SkillScope::Personal,
            folder: claude.clone(),
            path: claude.join("away"),
            real: Some(std::fs::canonicalize(&away).unwrap()),
            tools: vec![Tool::Claude, Tool::OpenCode],
        }]
    );
    let half = find(&found, "half").unwrap();
    assert_eq!(half.description.as_deref(), Some("Half."));
    assert_eq!(half.places[0].path, claude.join("half"));
    assert_eq!(half.places[0].real, None, "the folder itself isn't a link");
    let real = find(&found, "real").unwrap();
    assert_eq!(real.places.len(), 1, "{:?}", real.places);
    assert_eq!(find(&found, "deep").unwrap().places.len(), 1);
    for name in ["loop-a", "loop-b", "self", "dangling", "up"] {
        assert!(find(&found, name).is_none(), "{name}");
    }
}

/// A skills folder that is itself a link (kept with the user's dotfiles) holds skills that aren't links.
#[cfg(unix)]
#[test]
fn a_linked_skills_folder_holds_plain_skills() {
    let home = TempDir::new().unwrap();
    let dotfiles = TempDir::new().unwrap();
    let kept = dotfiles.path().join("skills");
    skill(&kept, "mine", &named("mine", "Mine."));
    std::fs::create_dir_all(home.path().join(".claude")).unwrap();
    std::os::unix::fs::symlink(&kept, home.path().join(".claude/skills")).unwrap();
    let found = list(&Homes::in_home(home.path()), None);
    let mine = find(&found, "mine").unwrap();
    assert_eq!(mine.places[0].path, home.path().join(".claude/skills/mine"));
    assert_eq!(mine.places[0].real, None);
}

#[tokio::test]
async fn skills_list_answers_with_the_users_and_otaqs_skills() {
    let tool_home = TempDir::new().unwrap();
    let personal = tool_home.path().join(".claude/skills");
    skill(&personal, "mine", &named("mine", "Personal."));
    let home = tool_home.path().to_path_buf();
    let w = TestService::start_with(move |mut config| {
        config.tool_home = Some(home);
        config
    })
    .await;
    let project = w.add_project("shop-web").await;
    skill(
        &Path::new(&project.repo_path).join(".claude/skills"),
        "team-pr",
        &named("team-pr", "Ours."),
    );

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let skills = match client
        .request(Request::SkillsList(otaq_core::protocol::ProjectParams {
            project: project.key.clone(),
        }))
        .await
        .unwrap()
    {
        Reply::Skills(skills) => skills,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        find(&skills, "mine").unwrap().places[0].scope,
        SkillScope::Personal
    );
    assert_eq!(
        find(&skills, "team-pr").unwrap().places[0].scope,
        SkillScope::Project
    );
    assert!(find(&skills, "validate").unwrap().otaq);

    // Each skill names the tools that have it: Otaq's own every tool; the personal `mine` in ~/.claude/skills
    // Claude Code (and OpenCode, which reads that folder), not Codex; the uncommitted `team-pr` only Claude Code.
    assert_eq!(find(&skills, "validate").unwrap().tools, Tool::ALL);
    assert_eq!(
        find(&skills, "mine").unwrap().tools,
        [Tool::Claude, Tool::OpenCode]
    );
    assert_eq!(find(&skills, "team-pr").unwrap().tools, [Tool::Claude]);

    assert!(w.board.has_skill(&project.key, "team-pr").await);
    assert!(w.board.has_skill(&project.key, "mine").await);
    assert!(w.board.has_skill(&project.key, "validate").await);
    assert!(!w.board.has_skill(&project.key, "nobody-has-it").await);
    w.stop().await;
}

#[tokio::test]
async fn has_skill_knows_otaqs_own_with_no_folders_at_all() {
    let w = TestService::start().await;
    assert!(w.board.has_skill("NOPE", "validate").await);
    assert!(w.board.has_skill("NOPE", "work-ticket").await);
    assert!(!w.board.has_skill("NOPE", "team-validate").await);
    w.stop().await;
}

/// Which tool has a skill (S6): a personal skill is had by the tools that read its folder, not by the others.
#[test]
fn a_personal_skill_is_had_only_by_the_tools_that_read_its_folder() {
    let home = TempDir::new().unwrap();
    let h = home.path();
    skill(&h.join(".claude/skills"), "x", &named("x", "Claude's."));
    skill(&h.join(".codex/skills"), "y", &named("y", "Codex's."));
    skill(&h.join(".copilot/skills"), "z", &named("z", "Copilot's."));
    skill(
        &h.join(".agents/skills"),
        "shared",
        &named("shared", "Shared."),
    );
    let homes = Homes::in_home(h);
    let none = ProjectFolders::default();

    assert!(tool_has(&homes, &none, Tool::Claude, "x"));
    assert!(!tool_has(&homes, &none, Tool::Codex, "x"));
    assert!(!tool_has(&homes, &none, Tool::Copilot, "x"));
    assert_eq!(tools_with(&homes, &none, "y"), [Tool::Codex]);
    assert_eq!(tools_with(&homes, &none, "z"), [Tool::Copilot]);
    assert_eq!(
        tools_with(&homes, &none, "shared"),
        [Tool::Codex, Tool::Copilot, Tool::OpenCode]
    );
    assert_eq!(tools_with(&homes, &none, "validate"), Tool::ALL);
    assert!(tools_with(&homes, &none, "nobody-has-it").is_empty());

    // With CLAUDE_CONFIG_DIR set, Claude Code's folder is only Claude Code's: OpenCode reads ~/.claude/skills.
    let cfg = TempDir::new().unwrap();
    skill(&cfg.path().join("skills"), "moved", &named("moved", "M."));
    let var = env(&[("CLAUDE_CONFIG_DIR", cfg.path())]);
    let moved = Homes::with_env(Some(h), &var);
    assert_eq!(tools_with(&moved, &none, "moved"), [Tool::Claude]);
}

/// A tool has a skill only when it loads its SKILL.md: Codex skips one that starts with a UTF-8 byte order mark
/// (Windows PowerShell 5 writes one), while OpenCode loads it, so a skill in the shared `~/.agents/skills` with a
/// byte order mark is had by Copilot and OpenCode, not Codex. The skill is still listed, by its front matter's name.
#[test]
fn a_skill_starting_with_a_byte_order_mark_is_not_codexs() {
    let home = TempDir::new().unwrap();
    let h = home.path();
    let text = format!("\u{feff}{}", named("bommed", "Written by PowerShell."));
    skill(&h.join(".agents/skills"), "bommed-folder", &text);
    skill(
        &h.join(".codex/skills"),
        "codex-only",
        &format!("\u{feff}{}", named("codex-only", "C.")),
    );
    skill(
        &h.join(".codex/skills"),
        "plain",
        &named("plain", "No mark."),
    );
    let homes = Homes::in_home(h);
    let none = ProjectFolders::default();

    assert_eq!(
        tools_with(&homes, &none, "bommed"),
        [Tool::Copilot, Tool::OpenCode]
    );
    assert!(!tool_has(&homes, &none, Tool::Codex, "codex-only"));
    assert!(tool_has(&homes, &none, Tool::Codex, "plain"));
    let skills = list_in(&homes, &none);
    let listed = find(&skills, "codex-only").expect("still listed");
    assert!(listed.tools.is_empty());
    assert_eq!(listed.description.as_deref(), Some("C."));
    // Codex finds them but skips them, so the user can be told why.
    assert_eq!(listed.skipped, [Tool::Codex]);
    assert_eq!(find(&skills, "bommed").unwrap().skipped, [Tool::Codex]);
    assert!(find(&skills, "plain").unwrap().skipped.is_empty());
}

/// A personal Codex skill in Codex's own folder (`~/.codex/skills`, `CODEX_HOME` unset or empty) is Codex's, and
/// only Codex's, on every platform: the folder is built with the platform's separators.
#[test]
fn a_personal_codex_skill_is_codexs() {
    let home = TempDir::new().unwrap();
    let h = home.path();
    skill(
        &h.join(".codex").join("skills"),
        "reltest-docs",
        &named("reltest-docs", "Release-test skill."),
    );
    let unset = env(&[]);
    let empty = |name: &str| (name == "CODEX_HOME").then(OsString::new);
    for homes in [
        Homes::with_env(Some(h), &unset),
        Homes::with_env(Some(h), &empty),
    ] {
        let skills = list(&homes, None);
        let docs = find(&skills, "reltest-docs").unwrap();
        assert_eq!(docs.tools, [Tool::Codex]);
        assert!(docs.skipped.is_empty());
        assert_eq!(docs.places[0].scope, SkillScope::Personal);
        assert_eq!(docs.places[0].folder, h.join(".codex").join("skills"));
    }
}

/// Which tool has a skill (S6): the project's skills count for a tool when it reads their folder in the ticket's
/// worktree, which has only what is committed; Claude Code also reads the checkout's `.claude/skills`.
#[test]
fn an_uncommitted_project_skill_is_had_only_by_claude_code() {
    let repo = TempDir::new().unwrap();
    let r = repo.path();
    skill(
        &r.join(".claude/skills"),
        "draft",
        &named("draft", "Not committed."),
    );
    skill(
        &r.join(".claude/skills"),
        "team-pr",
        &named("team-pr", "Committed."),
    );
    skill(
        &r.join(".agents/skills"),
        "loose",
        &named("loose", "Not committed."),
    );
    skill(
        &r.join(".agents/skills"),
        "team-check",
        &named("team-check", "Committed."),
    );
    skill(&r.join(".github/skills"), "gh", &named("gh", "Committed."));
    let committed = [
        Path::new(".claude").join("skills").join("team-pr"),
        Path::new(".agents").join("skills").join("team-check"),
        Path::new(".github").join("skills").join("gh"),
    ]
    .into_iter()
    .collect();
    let project = ProjectFolders {
        checkout: Some(r.to_path_buf()),
        worktree: None,
        committed: Some(committed),
    };
    let homes = Homes::default();
    assert_eq!(tools_with(&homes, &project, "draft"), [Tool::Claude]);
    assert_eq!(
        tools_with(&homes, &project, "team-pr"),
        [Tool::Claude, Tool::Copilot, Tool::OpenCode]
    );
    assert!(tools_with(&homes, &project, "loose").is_empty());
    assert_eq!(
        tools_with(&homes, &project, "team-check"),
        [Tool::Codex, Tool::Copilot, Tool::OpenCode]
    );
    assert_eq!(tools_with(&homes, &project, "gh"), [Tool::Copilot]);
    // Still listed, with no tool: Pick a skill… shows every skill found.
    let skills = list_in(&homes, &project);
    assert!(find(&skills, "loose").unwrap().tools.is_empty());

    // The ticket's worktree: what is in it counts for the tools that read its folders; the checkout's
    // `.claude/skills` only for Claude Code.
    let worktree = TempDir::new().unwrap();
    let wt = worktree.path();
    skill(
        &wt.join(".agents/skills"),
        "team-check",
        &named("team-check", "In the worktree."),
    );
    let ticket = ProjectFolders {
        checkout: Some(r.to_path_buf()),
        worktree: Some(wt.to_path_buf()),
        committed: None,
    };
    assert_eq!(
        tools_with(&homes, &ticket, "team-check"),
        [Tool::Codex, Tool::Copilot, Tool::OpenCode]
    );
    assert_eq!(tools_with(&homes, &ticket, "draft"), [Tool::Claude]);
    assert!(tools_with(&homes, &ticket, "loose").is_empty());
    assert!(tools_with(&homes, &ticket, "gh").is_empty());
}

/// The skills of the plugins installed in Claude Code and Codex are found, as `<plugin>:<skill>`, each had only by
/// its tool; Otaq's own plugin isn't listed as one.
#[test]
fn a_plugins_skill_is_found_for_its_tool() {
    let home = TempDir::new().unwrap();
    let h = home.path();
    let claude_copy = h.join(".claude/plugins/cache/team/tuq/1.0.0");
    skill(
        &claude_copy.join("skills"),
        "product-spec",
        &named("product-spec", "Spec it."),
    );
    std::fs::write(
        h.join(".claude/plugins/installed_plugins.json"),
        serde_json::json!({
            "version": 2,
            "plugins": {"tuq@team": [{"scope": "user", "installPath": claude_copy.display().to_string()}]}
        })
        .to_string(),
    )
    .unwrap();
    let codex_copy = h.join(".codex/plugins/cache/team/review/0.2.0");
    skill(
        &codex_copy.join("skills"),
        "deep",
        &named("deep", "Deep review."),
    );
    let otaq_copy = h.join(".codex/plugins/cache/otaq-local/otaq/0.1.0");
    skill(
        &otaq_copy.join("skills"),
        "commit",
        &named("commit", "Otaq's."),
    );
    std::fs::write(
        h.join(".codex/config.toml"),
        "[plugins.\"review@team\"]\nenabled = true\n\n[plugins.\"otaq@otaq-local\"]\nenabled = true\n",
    )
    .unwrap();

    let homes = Homes::in_home(h);
    let none = ProjectFolders::default();
    let skills = list_in(&homes, &none);
    let spec = find(&skills, "tuq:product-spec").unwrap();
    assert_eq!(spec.tools, [Tool::Claude]);
    assert_eq!(spec.description.as_deref(), Some("Spec it."));
    assert_eq!(spec.places[0].scope, SkillScope::Plugin);
    assert_eq!(tools_with(&homes, &none, "review:deep"), [Tool::Codex]);
    assert!(find(&skills, "otaq:commit").is_none());
}

/// The board answers per ticket: from its worktree once it has one, else from what is committed at its base; each
/// tool's folder variables come from the environment its sessions get (the project's variables over the service's).
#[tokio::test]
async fn the_board_answers_per_tool_for_a_ticket() {
    let tool_home = TempDir::new().unwrap();
    let home = tool_home.path().to_path_buf();
    let w = TestService::start_with(move |mut config| {
        config.tool_home = Some(home);
        config
    })
    .await;
    let project = w.add_project("shop-web").await;
    let repo = PathBuf::from(&project.repo_path);
    skill(
        &repo.join(".agents/skills"),
        "team-check",
        &named("team-check", "Ours."),
    );
    skill(
        &repo.join(".claude/skills"),
        "draft",
        &named("draft", "Mine."),
    );
    let key = project.key.as_str();

    // Not committed: no tool has team-check; draft only Claude Code.
    assert!(
        w.board
            .skill_tools(key, None, "team-check")
            .await
            .is_empty()
    );
    assert_eq!(
        w.board.skill_tools(key, None, "draft").await,
        [Tool::Claude]
    );
    assert!(!w.board.has_skill(key, "team-check").await);

    // Committed at the base: every tool that reads `.agents/skills`.
    otaq_core::git::testing::git(&repo, &["add", "--", ".agents"]);
    otaq_core::git::testing::git(&repo, &["commit", "--quiet", "-m", "team skill"]);
    assert_eq!(
        w.board.skill_tools(key, None, "team-check").await,
        [Tool::Codex, Tool::Copilot, Tool::OpenCode]
    );
    assert!(
        w.board
            .tool_has_skill(key, None, Tool::Codex, "team-check")
            .await
    );
    assert!(
        !w.board
            .tool_has_skill(key, None, Tool::Codex, "draft")
            .await
    );

    // A ticket with a worktree: what its worktree has.
    let ticket = w.create(key, "Check it").await;
    let worktree = w.give_worktree(&ticket).await;
    let ticket_key = ticket.key.to_string();
    assert!(
        w.board
            .skill_tools(key, Some(&ticket_key), "team-check")
            .await
            .is_empty()
    );
    skill(
        &worktree.join(".github/skills"),
        "gh",
        &named("gh", "Copilot's."),
    );
    assert_eq!(
        w.board.skill_tools(key, Some(&ticket_key), "gh").await,
        [Tool::Copilot]
    );
    assert_eq!(
        w.board.skill_tools(key, Some(&ticket_key), "draft").await,
        [Tool::Claude]
    );

    // CODEX_HOME set for the project's sessions: the finder looks there for Codex.
    let codex_home = TempDir::new().unwrap();
    skill(
        &codex_home.path().join("skills"),
        "elsewhere",
        &named("elsewhere", "E."),
    );
    assert!(w.board.skill_tools(key, None, "elsewhere").await.is_empty());
    let mut settings = w.board.project_settings(key).await.unwrap();
    settings.env_vars = vec![otaq_core::board::settings::EnvVar::new(
        "CODEX_HOME",
        &codex_home.path().display().to_string(),
    )];
    w.board.set_project_settings(key, &settings).await.unwrap();
    assert_eq!(
        w.board.skill_tools(key, None, "elsewhere").await,
        [Tool::Codex]
    );
    w.stop().await;
}
