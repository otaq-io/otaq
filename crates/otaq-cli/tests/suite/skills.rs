//! The plugin's skills only teach commands the CLI has (T-0140): every `otaq <command> [<subcommand>]` written in
//! a skill is accepted by the built CLI (`--help` on it succeeds).

use std::path::Path;
use std::process::Command;

use otaq_core::agent_tools::claude::find_plugin_dir;
use otaq_core::naming;

const CB: &str = env!("CARGO_BIN_EXE_otaq");

/// The skill folder and the commands the epic's removal took out of the CLI (d0133); T-0534 rewrites the skills that
/// still name them, and these go with it.
const REMOVED_EPIC_LEAD: &str = "epic-lead";
const REMOVED_COMMANDS: &[&str] = &["notes"];

/// The command words after each `otaq ` in a text: lowercase words, up to two.
fn commands(text: &str) -> Vec<Vec<String>> {
    let lead = format!("{} ", naming::COMMAND_NAME);
    let mut found = Vec::new();
    for (at, _) in text.match_indices(&lead) {
        // Skip `OTAQ_…`, `/otaq:…` and paths: the name must start a word.
        let before = text[..at].chars().last();
        if before.is_some_and(|c| c.is_alphanumeric() || matches!(c, '_' | '/' | ':' | '-')) {
            continue;
        }
        let words: Vec<String> = text[at + lead.len()..]
            .split([' ', '\n', '`', '"'])
            .take_while(|w| {
                w.starts_with(|c: char| c.is_ascii_lowercase())
                    && w.chars().all(|c| c.is_ascii_lowercase() || c == '-')
            })
            .take(2)
            .map(str::to_owned)
            .collect();
        if !words.is_empty() {
            found.push(words);
        }
    }
    found
}

#[test]
fn every_command_a_skill_names_exists() {
    let skills = find_plugin_dir(Path::new("/nonexistent"))
        .expect("the repository's plugin folder")
        .join("skills");
    let mut checked = 0;
    for entry in std::fs::read_dir(&skills).unwrap() {
        let folder = entry.unwrap().path();
        if !folder.is_dir() || folder.ends_with(REMOVED_EPIC_LEAD) {
            continue;
        }
        let file = folder.join("SKILL.md");
        let text = std::fs::read_to_string(&file).unwrap();
        for words in commands(&text) {
            if REMOVED_COMMANDS.contains(&words[0].as_str()) {
                continue;
            }
            let ok = Command::new(CB)
                .args(&words)
                .arg("--help")
                .output()
                .unwrap()
                .status
                .success();
            assert!(
                ok,
                "{}: `{} {}` isn't a command",
                file.display(),
                naming::COMMAND_NAME,
                words.join(" ")
            );
            checked += 1;
        }
    }
    assert!(checked > 20, "only {checked} commands found");
}

#[test]
fn command_words_are_found() {
    let cmd = naming::COMMAND_NAME;
    let text = format!(
        "Run `{cmd} prime`, then `{cmd} pr propose --stdin` and {cmd} answer <ID>. Not /{cmd}:commit or $OTAQ_X"
    );
    assert_eq!(
        commands(&text),
        vec![
            vec!["prime".to_owned()],
            vec!["pr".to_owned(), "propose".to_owned()],
            vec!["answer".to_owned()]
        ]
    );
}

/// T-0286: the M17 commands the epic-lead, work-ticket and commit skills teach are among those checked above; T-0409
/// (AGM-10): work-ticket ends its stage with `stage next`, or `stage done` with nothing to deliver. T-0445 (CMT-09):
/// the lead posts `comment`s, and both read reports with `report`.
#[test]
fn the_lead_and_decision_commands_are_checked() {
    let skills = find_plugin_dir(Path::new("/nonexistent"))
        .expect("the repository's plugin folder")
        .join("skills");
    let named = |skill: &str| {
        commands(&std::fs::read_to_string(skills.join(skill).join("SKILL.md")).unwrap())
            .into_iter()
            .map(|words| words.join(" "))
            .collect::<Vec<_>>()
    };
    let lead = named("epic-lead");
    for command in [
        "prime",
        "create",
        "update",
        "dep add",
        "dep remove",
        "decision mark",
        "note",
        "questions",
        "answer",
        "ask human",
        "suggest close",
        "suggest delete",
        "comment",
        "report",
    ] {
        assert!(lead.contains(&command.to_owned()), "epic-lead: {command}");
    }
    let work = named("work-ticket");
    for command in [
        "prime",
        "notes",
        "decision record",
        "stage next",
        "stage done",
        "report",
    ] {
        assert!(work.contains(&command.to_owned()), "work-ticket: {command}");
    }
    assert!(named("commit").contains(&"notes".to_owned()));
}
