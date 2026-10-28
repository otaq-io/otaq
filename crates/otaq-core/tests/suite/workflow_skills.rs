//! Imported skills on agents (T-0378, T-0405; WSK-02…WSK-04, AGM-01; d0092, d0108): Import skill folder… checks the
//! folder, keeps Otaq's copy in `$OTAQ_HOME/skills/<id>/` and adds it to the agent's skills; Re-import copies the
//! remembered original again for every agent holding it; the requests (`agents.import_skill`,
//! `agents.reimport_skill`) and the clean-up of copies no agent holds. Skill folders live under the test's own
//! temporary folders, and `OTAQ_HOME` is one too (the fixture's and the test service's data folder).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::common::Fixture;
#[cfg(unix)]
use otaq_core::agents::SkillRef;
use otaq_core::agents::{Agent, AgentFields, Agents, FixedModels, ImportedSkill, skills};
use otaq_core::board::{BoardError, Refusal};
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::protocol::{Reply, Request};
use otaq_core::service::testing::TestService;
use serde_json::json;

/// The example's team skill folders under `root/team-skills/`: `my-commit` (SKILL.md, reference.md and an
/// executable `scripts/check-staged.sh`), `commit-helpers` (no SKILL.md), `broken-front-matter` and
/// `missing-script` (its SKILL.md names a script that isn't there).
fn team_skills(root: &Path) -> PathBuf {
    let team = root.join("team-skills");
    let my_commit = team.join("my-commit");
    std::fs::create_dir_all(my_commit.join("scripts")).unwrap();
    std::fs::write(
        my_commit.join("SKILL.md"),
        "---\nname: my-commit\ndescription: Commits in the team's style.\n---\n\
         Read [the style](reference.md), run `bash scripts/check-staged.sh`, then commit with a message in the style \
         `reference.md` gives.\n",
    )
    .unwrap();
    std::fs::write(
        my_commit.join("reference.md"),
        "Commit messages: `<area>: <what changed>`, for example `notes: sort by date`\n",
    )
    .unwrap();
    let script = my_commit.join("scripts/check-staged.sh");
    std::fs::write(&script, "#!/bin/sh\necho CHECK-STAGED-OK\n").unwrap();
    // Windows files have no mode bits.
    #[cfg(unix)]
    set_mode(&script, 0o755);

    std::fs::create_dir_all(team.join("commit-helpers")).unwrap();
    std::fs::write(team.join("commit-helpers/notes.md"), "no skill here\n").unwrap();

    std::fs::create_dir_all(team.join("broken-front-matter")).unwrap();
    std::fs::write(
        team.join("broken-front-matter/SKILL.md"),
        "---\nname my-commit\ndescription: x\n---\nBody\n",
    )
    .unwrap();

    std::fs::create_dir_all(team.join("missing-script")).unwrap();
    std::fs::write(
        team.join("missing-script/SKILL.md"),
        "---\nname: missing-script\ndescription: x\n---\nRun `bash scripts/check-staged.sh` first.\n",
    )
    .unwrap();
    team
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
fn mode(path: &Path) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

/// The folders under `skills/`.
fn copies(f: &Fixture) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(f.board.data().skills_dir())
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn refusal(result: Result<Agent, BoardError>) -> (Refusal, String) {
    match result {
        Err(BoardError::Refused { code, message }) => (code, message),
        other => panic!("{other:?}"),
    }
}

async fn agent(f: &Fixture, name: &str) -> Agent {
    f.board.store().agent_named(name).await.unwrap().unwrap()
}

/// The agent's one imported skill.
fn only_import(agent: &Agent) -> &ImportedSkill {
    let [skill] = &agent.imported[..] else {
        panic!("one import: {agent:?}")
    };
    skill
}

// Unix only: it makes symbolic links (an administrator's right on Windows) and checks Unix file modes.
#[cfg(unix)]
#[tokio::test]
async fn the_example_skill_is_copied_whole_with_its_modes() {
    let f = Fixture::shop().await;
    let team = team_skills(f.home.path());
    let original = team.join("my-commit");
    // A link inside the folder is copied as its file; one that leaves the folder is a warning, not followed.
    std::os::unix::fs::symlink(original.join("reference.md"), original.join("style.md")).unwrap();
    std::os::unix::fs::symlink(
        team.join("commit-helpers/notes.md"),
        original.join("outside.md"),
    )
    .unwrap();

    let developer = agent(&f, "Developer").await;
    let after = skills::import(&f.board, &developer.id, &original)
        .await
        .unwrap();
    let skill = only_import(&after);
    assert_eq!(skill.name, "my-commit");
    assert_eq!(skill.original, original.to_str().unwrap());
    assert!(skill.imported_at > 0);
    assert_eq!(
        skill.warnings,
        ["outside.md links outside the folder; not copied"]
    );
    assert_eq!(
        after.skills.last(),
        Some(&SkillRef::Imported(skill.id.clone())),
        "at the end of its skills"
    );
    let copy = f.board.data().skill_path(&skill.id).unwrap();
    assert_eq!(copies(&f), [skill.id.as_str()], "an id of Otaq's own");
    assert_ne!(skill.id, "my-commit");
    assert_eq!(mode(&copy), 0o700);
    for file in ["SKILL.md", "reference.md", "scripts/check-staged.sh"] {
        assert_eq!(
            std::fs::read(copy.join(file)).unwrap(),
            std::fs::read(original.join(file)).unwrap(),
            "{file}"
        );
    }
    assert_eq!(mode(&copy.join("scripts/check-staged.sh")), 0o755);
    let style = copy.join("style.md");
    assert!(!std::fs::symlink_metadata(&style).unwrap().is_symlink());
    assert_eq!(
        std::fs::read(style).unwrap(),
        std::fs::read(original.join("reference.md")).unwrap()
    );
    assert!(!copy.join("outside.md").exists());

    // A stage naming it by its own name runs the copy (WSK-03, Q296: a flow names the skill); `commit` stays the
    // shipped one…
    let developer = agent(&f, "Developer").await;
    let sources = otaq_core::skills::Sources {
        shipped: None,
        data: f.board.data(),
    };
    assert_eq!(
        sources.runs(Some(&developer), "my-commit"),
        otaq_core::skills::Source {
            own: "my-commit".into(),
            folder: copy.clone()
        }
    );
    assert_ne!(sources.runs(Some(&developer), "commit").folder, copy);
    // …and a missing or broken copy means the shipped skill of that name (WSK-04).
    std::fs::write(copy.join("SKILL.md"), "no front matter any more\n").unwrap();
    assert_ne!(sources.runs(Some(&developer), "my-commit").folder, copy);
}

#[tokio::test]
async fn refused_folders_change_nothing_and_missing_files_are_a_warning() {
    let f = Fixture::shop().await;
    let team = team_skills(f.home.path());
    let developer = agent(&f, "Developer").await;
    skills::import(&f.board, &developer.id, &team.join("my-commit"))
        .await
        .unwrap();
    let before = (agent(&f, "Developer").await, copies(&f));

    for (folder, why) in [
        ("commit-helpers", "The folder has no SKILL.md"),
        (
            "broken-front-matter",
            "SKILL.md's front matter can't be read: line 2 isn't key: value",
        ),
    ] {
        let (code, message) =
            refusal(skills::import(&f.board, &developer.id, &team.join(folder)).await);
        assert_eq!((code, message.as_str()), (Refusal::BadSkillFolder, why));
        assert_eq!(
            (agent(&f, "Developer").await, copies(&f)),
            before,
            "{folder}"
        );
    }
    let (code, _) =
        refusal(skills::import(&f.board, &developer.id, Path::new("team-skills/my-commit")).await);
    assert_eq!(code, Refusal::BadSkillFolder, "a relative path");
    assert_eq!(copies(&f), before.1);

    let validator = agent(&f, "Validator").await;
    let after = skills::import(&f.board, &validator.id, &team.join("missing-script"))
        .await
        .unwrap();
    assert_eq!(
        only_import(&after).warnings,
        ["SKILL.md mentions scripts/check-staged.sh, which isn't in the folder"]
    );
}

/// Each import is a copy of its own; Duplicate's agent shares its source's copy, and Re-import renews it for both;
/// with the original gone it is refused and the copy kept.
#[tokio::test]
async fn re_import_renews_the_copy_for_every_agent_holding_it() {
    let f = Fixture::shop().await;
    let team = team_skills(f.home.path());
    let original = team.join("my-commit");
    let developer = agent(&f, "Developer").await;
    let validator = agent(&f, "Validator").await;
    let first = skills::import(&f.board, &developer.id, &original)
        .await
        .unwrap();
    skills::import(&f.board, &validator.id, &original)
        .await
        .unwrap();
    assert_eq!(copies(&f).len(), 2, "two copies of the same folder");

    let rules = Agents::new(f.board.clone(), Arc::new(FixedModels::sample()));
    let draft = rules.duplicate(&developer.id).await.unwrap();
    let twin = rules
        .add(&AgentFields {
            name: draft.name.clone(),
            ..draft
        })
        .await
        .unwrap();
    let before = only_import(&first).clone();
    assert_eq!(only_import(&twin).id, before.id, "the same copy");

    std::fs::write(original.join("reference.md"), "New style\n").unwrap();
    let again = skills::reimport(&f.board, &developer.id, &before.id)
        .await
        .unwrap();
    let after = only_import(&again).clone();
    assert_ne!(after.id, before.id);
    assert_ne!(after.fingerprint, before.fingerprint, "renewed");
    assert_eq!(after.original, before.original);
    assert_eq!(
        only_import(&agent(&f, "Developer copy").await).id,
        after.id,
        "the twin follows"
    );
    assert!(!f.board.data().skill_path(&before.id).unwrap().exists());
    let copy = f.board.data().skill_path(&after.id).unwrap();
    assert_eq!(
        std::fs::read_to_string(copy.join("reference.md")).unwrap(),
        "New style\n"
    );

    // The original gone: refused, the copy kept.
    std::fs::rename(&original, team.join("moved")).unwrap();
    let (code, message) = refusal(skills::reimport(&f.board, &developer.id, &after.id).await);
    assert_eq!(code, Refusal::OriginalNotFound);
    assert_eq!(
        message,
        format!("Original not found: {}", original.display())
    );
    assert!(copy.join("SKILL.md").is_file());
    // Not one of the agent's imports: nothing to re-import.
    let (code, _) = refusal(skills::reimport(&f.board, &validator.id, &after.id).await);
    assert_eq!(code, Refusal::NotImported);
}

/// The requests over the socket, and the clean-up at service start: the folders no agent holds go (a deleted agent's
/// copy, one left by a crash).
#[tokio::test(flavor = "multi_thread")]
async fn the_requests_and_the_clean_up() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let team = team_skills(w.home.path());
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let ask = |method: &str, params: serde_json::Value| {
        let request: Request =
            serde_json::from_value(json!({ "method": method, "params": params })).unwrap();
        client.request(request)
    };
    let developer = w
        .board
        .store()
        .agent_named("Developer")
        .await
        .unwrap()
        .unwrap();

    let Reply::Agent(after) = ask(
        "agents.import_skill",
        json!({ "id": developer.id, "folder": team.join("my-commit") }),
    )
    .await
    .unwrap() else {
        panic!()
    };
    let kept = only_import(&after).id.clone();
    let Err(ClientError::Service(e)) = ask(
        "agents.import_skill",
        json!({ "id": developer.id, "folder": team.join("commit-helpers") }),
    )
    .await
    else {
        panic!("refused")
    };
    assert_eq!(e.message, "The folder has no SKILL.md");
    assert_eq!(e.data.unwrap()["refusal"], "bad_skill_folder");
    let Reply::Agent(again) = ask(
        "agents.reimport_skill",
        json!({ "id": developer.id, "skill": kept }),
    )
    .await
    .unwrap() else {
        panic!()
    };
    let kept = w.data.skill_path(&only_import(&again).id).unwrap();

    // A spare agent with an import of its own, then deleted; and a stray folder.
    let Reply::Agent(spare) = ask(
        "agents.add",
        json!({ "name": "Spare", "model": {"kind": "default"} }),
    )
    .await
    .unwrap() else {
        panic!()
    };
    let Reply::Agent(spare) = ask(
        "agents.import_skill",
        json!({ "id": spare.id, "folder": team.join("missing-script") }),
    )
    .await
    .unwrap() else {
        panic!()
    };
    let spares = w.data.skill_path(&only_import(&spare).id).unwrap();
    ask("agents.delete", json!({ "id": spare.id }))
        .await
        .unwrap();
    drop(client);
    let stray = w.data.skill_path("left-by-a-crash").unwrap();
    std::fs::create_dir_all(&stray).unwrap();
    let w = w.restart().await;
    assert!(!stray.exists(), "removed at start");
    assert!(!spares.exists(), "no agent holds it");
    assert!(kept.join("SKILL.md").is_file(), "held by Developer");
    w.stop().await;
}
