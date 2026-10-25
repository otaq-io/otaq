//! The commands each plugin skill teaches (T-0140, T-0286, T-0539). That every command and flag a skill names exists
//! in the CLI is checked against clap's command tree, in the binary's own `skill_commands` tests.

use std::path::Path;

use otaq_core::agent_tools::claude::find_plugin_dir;
use otaq_core::naming;

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

/// T-0286: the commands the work-ticket skill teaches; T-0409 (AGM-10): work-ticket
/// ends its stage with `stage next`, or `stage done` with nothing to deliver. T-0445 (CMT-09): it reads reports with
/// `report`. T-0534 (d0133): no skill names the lead's commands any more.
#[test]
fn the_work_ticket_commands_are_checked() {
    let skills = find_plugin_dir(Path::new("/nonexistent"))
        .expect("the repository's plugin folder")
        .join("skills");
    let named = |skill: &str| {
        commands(&std::fs::read_to_string(skills.join(skill).join("SKILL.md")).unwrap())
            .into_iter()
            .map(|words| words.join(" "))
            .collect::<Vec<_>>()
    };
    let work = named("work-ticket");
    for command in [
        "prime",
        "decision record",
        "stage next",
        "stage done",
        "report",
    ] {
        assert!(work.contains(&command.to_owned()), "work-ticket: {command}");
    }
    assert!(
        !skills.join("epic-lead").exists(),
        "the epic-lead skill is gone"
    );
    for skill in ["work-ticket", "commit"] {
        assert!(
            !named(skill).contains(&"notes".to_owned()),
            "{skill}: notes"
        );
    }
    // T-0539 (d0133): the agent in charge runs its helpers, a helper hands its piece back; a Send back may start a
    // helper; Noyan writes a Plan.
    for command in [
        "helper start",
        "helper list",
        "helper diff",
        "helper merge",
        "helper back",
        "helper answer",
        "helper pass-on",
        "helper done",
        "ask human",
    ] {
        assert!(work.contains(&command.to_owned()), "work-ticket: {command}");
    }
    assert!(named("address-review").contains(&"helper start".to_owned()));
    let noyan = std::fs::read_to_string(skills.join("noyan").join("SKILL.md")).unwrap();
    assert!(noyan.contains("--plan-file"), "noyan: --plan-file");
    assert!(
        noyan.contains("Wishlist   Plan \u{b7} 3 pieces"),
        "noyan: the table's Plan row"
    );
}
