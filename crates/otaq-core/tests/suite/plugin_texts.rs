//! Otaq's skill texts work on every tool (ADP-05, d0088): each `plugin/skills/*/SKILL.md` has front matter every
//! tool's YAML parser reads (Copilot rejects an unquoted `: ` in a value), and no text names a skill in one tool's
//! syntax: the agent uses the skill that `prime` names.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::claude::find_plugin_dir;
use otaq_core::naming;

/// Every SKILL.md of the plugin, with its folder name.
fn skill_files() -> Vec<(String, PathBuf)> {
    let skills = otaq_core::runtime::shipped_skills(
        &find_plugin_dir(Path::new("/nonexistent")).expect("the repository's plugin folder"),
    );
    let mut files: Vec<(String, PathBuf)> = std::fs::read_dir(&skills)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_dir())
        .map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            (name, e.path().join("SKILL.md"))
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no skills in {}", skills.display());
    files
}

/// The front matter's `key: value` lines and the body.
fn split(name: &str, text: &str) -> (Vec<(String, String)>, String) {
    let rest = text
        .strip_prefix("---\n")
        .unwrap_or_else(|| panic!("{name}: no front matter"));
    let (front, body) = rest
        .split_once("\n---\n")
        .unwrap_or_else(|| panic!("{name}: front matter not closed"));
    let fields = front
        .lines()
        .map(|line| {
            let (key, value) = line
                .split_once(": ")
                .unwrap_or_else(|| panic!("{name}: {line:?} isn't `key: value`"));
            (key.to_owned(), value.to_owned())
        })
        .collect();
    (fields, body.to_owned())
}

/// A YAML quoted scalar on one line: `'…'` with inner quotes doubled, or `"…"` with inner quotes escaped.
fn quoted(value: &str) -> bool {
    let single = value.len() >= 2 && value.starts_with('\'') && value.ends_with('\'');
    let double = value.len() >= 2 && value.starts_with('"') && value.ends_with('"');
    if single {
        let inner = &value[1..value.len() - 1];
        return inner.replace("''", "").find('\'').is_none();
    }
    if double {
        let inner = &value[1..value.len() - 1];
        return inner
            .replace("\\\\", "")
            .replace("\\\"", "")
            .find('"')
            .is_none();
    }
    false
}

#[test]
fn every_skill_has_front_matter_with_name_and_description() {
    for (name, path) in skill_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        let (fields, _) = split(&name, &text);
        let field = |key: &str| fields.iter().find(|(k, _)| k == key).map(|(_, v)| v);
        assert_eq!(field("name").map(String::as_str), Some(name.as_str()));
        assert!(
            field("description").is_some_and(|d| !d.trim().is_empty()),
            "{name}: no description"
        );
        // GitHub Copilot CLI refuses to load a skill whose description is longer (1.0.109: "Skill description must
        // be at most 1024 characters"), and the stage then never starts its skill.
        let length = field("description").map_or(0, |d| d.chars().count());
        assert!(
            length <= 1024,
            "{name}: its description has {length} characters, more than the 1024 Copilot loads"
        );
    }
}

#[test]
fn a_value_with_a_colon_and_space_is_quoted() {
    for (name, path) in skill_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        for (key, value) in split(&name, &text).0 {
            if value.contains(": ") {
                assert!(quoted(&value), "{name}: {key} must be quoted: {value}");
            }
        }
    }
}

#[test]
fn no_skill_text_names_a_skill_in_one_tools_syntax() {
    let ns = naming::PLUGIN_NAMESPACE;
    // Claude Code and Copilot `/otaq:commit`, Codex `$otaq:commit`.
    let syntaxes = [format!("/{ns}:"), format!("${ns}:")];
    for (name, path) in skill_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        for syntax in &syntaxes {
            assert!(!text.contains(syntax), "{name} contains {syntax:?}");
        }
    }
}

/// T-0409, T-0416 (AGM-12, AGM-10, AGM-11, FLW-20, FLW-21; d0108): no shipped skill names a removed command or
/// flag. Stages end with `stage next` / `stage done`, checks with `check pass|fail`, tickets set stage choices with
/// `--stage`, and a running check's terminal is `--terminal --stage <name>`. `--validator` is gone everywhere, from
/// every file of the plugin.
#[test]
fn no_skill_names_a_removed_command_or_flag() {
    let ns = naming::PLUGIN_NAMESPACE;
    let gone = [
        "review submit".to_owned(),
        "validate pass".to_owned(),
        "validate fail".to_owned(),
        "--developer".to_owned(),
        "--validator".to_owned(),
        format!("/{ns}:"),
    ];
    for (name, path) in skill_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        for word in &gone {
            assert!(!text.contains(word.as_str()), "{name} names {word:?}");
        }
    }
    let plugin =
        find_plugin_dir(Path::new("/nonexistent")).expect("the repository's plugin folder");
    for path in files_under(&plugin) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        assert!(
            !text.contains("--validator"),
            "{} names --validator",
            path.display()
        );
    }
}

/// Every file under `dir`, at any depth.
fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files.extend(files_under(&path));
        } else {
            files.push(path);
        }
    }
    files
}

/// T-0416 (FLW-21): Noyan reads a running check's terminal with `--stage`.
#[test]
fn the_reading_skills_teach_the_stage_terminal() {
    let cmd = naming::COMMAND_NAME;
    let readers: Vec<_> = skill_files()
        .into_iter()
        .filter(|(n, _)| n == "noyan")
        .collect();
    assert_eq!(readers.len(), 1);
    for (name, path) in readers {
        let text = std::fs::read_to_string(path).unwrap();
        assert!(
            text.contains(&format!("`{cmd} show <KEY> --terminal --stage <name>`")),
            "{name} must teach --stage"
        );
    }
}

/// T-0409 (AGM-12, AGM-07): the skills that end an Agent stage name `stage next`, the check ends with `check pass|fail`,
/// and a skill another skill names is named as Otaq's, "Otaq's `commit` skill" (T4).
#[test]
fn skills_end_stages_with_the_stage_commands() {
    let cmd = naming::COMMAND_NAME;
    let read = |name: &str| {
        let (_, path) = skill_files()
            .into_iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("no skill {name}"));
        std::fs::read_to_string(path).unwrap()
    };
    for name in ["work-ticket", "address-review", "restack", "commit"] {
        assert!(
            read(name).contains(&format!("{cmd} stage next")),
            "{name} must name `stage next`"
        );
    }
    for name in ["work-ticket", "address-review", "restack"] {
        assert!(
            read(name).contains("Otaq's `commit` skill"),
            "{name} must name Otaq's commit skill"
        );
    }
    let validate = read("validate");
    for verdict in ["pass", "fail"] {
        assert!(validate.contains(&format!("{cmd} check {verdict} --stdin")));
    }
    for name in ["validate", "answer-question"] {
        assert!(
            read(name).contains(&format!("don't run `{cmd} stage next`")),
            "{name} must say not to run `stage next`"
        );
    }
}

/// Otaq never writes skills into a worktree, and its own skills come from its plugin (T4): no file of a shipped skill
/// names another by a copy's name (`otaq-commit`) or points into a working folder's skills (`.claude/skills`,
/// `.agents/skills`).
#[test]
fn no_skill_text_names_a_copy() {
    let skills = otaq_core::runtime::shipped_skills(
        &find_plugin_dir(Path::new("/nonexistent")).expect("the repository's plugin folder"),
    );
    let names: Vec<String> = skill_files().into_iter().map(|(name, _)| name).collect();
    let mut needles: Vec<String> = names.iter().map(|name| format!("otaq-{name}")).collect();
    needles.extend([".claude/skills".to_owned(), ".agents/skills".to_owned()]);
    let mut folders = vec![skills];
    let mut read = 0;
    while let Some(folder) = folders.pop() {
        for entry in std::fs::read_dir(&folder).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                folders.push(path);
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            read += 1;
            for needle in &needles {
                assert!(
                    !text.contains(needle.as_str()),
                    "{} must not say {needle:?}",
                    path.display()
                );
            }
        }
    }
    assert!(read >= names.len(), "read {read} files");
}

/// Agents have no skill list, and a stage's `skill:` is the only place a skill is picked (T4): no agent the `flow`
/// skill shows carries a `skills:` key, and its example security reviewer's skill is its stage's.
#[test]
fn the_flow_skills_agents_have_no_skill_list() {
    let flow = read_skill("flow");
    let lines: Vec<&str> = flow.lines().collect();
    for (n, line) in lines.iter().enumerate() {
        assert!(
            !line.trim_start().starts_with("skills:"),
            "flow line {}: {line:?}",
            n + 1
        );
    }
    assert!(
        flow.contains("agent: Security reviewer\n       skill: team-security-review\n"),
        "the security reviewer's skill is its stage's"
    );
    assert!(
        !flow.contains("- **`skills`**"),
        "no `skills` field for agents"
    );
}

/// The text of the shipped skill `name`.
/// A stage's agent with nothing to add for a ticket finishes its stage at once with a short note, read from the
/// flow's descriptions in `prime`: an ordinary finish, never a skip, since only the user skips a stage. Every stage
/// skill says so, and Noyan reads the descriptions without ever skipping for a ticket.
#[test]
fn stage_skills_finish_at_once_when_there_is_nothing_to_add() {
    for name in ["work-ticket", "validate"] {
        let skill = read_skill(name).replace("\n   ", " ");
        for text in [
            "\"## The flow\"",
            "at once with a short note saying so",
            "never a skip: only the user skips a stage",
        ] {
            assert!(skill.contains(text), "{name} must say {text:?}");
        }
    }
    let noyan = read_skill("noyan").replace("\n   ", " ");
    for text in ["**the flow**", "never a reason for you to skip it"] {
        assert!(noyan.contains(text), "noyan must say {text:?}");
    }
}

fn read_skill(name: &str) -> String {
    let (_, path) = skill_files()
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("no skill {name}"));
    std::fs::read_to_string(path).unwrap()
}

/// A skill's text with every run of whitespace as one space, so a phrase reads the same wherever the text wraps.
fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// product-spec, tech-spec and milestone-plan are tuq's (decision 102): their texts are checked in tuq's repository.

/// Decisions 081, 098: in a stage whose skill isn't Otaq's, `work-ticket` gets the context in its first turn and ends
/// it without starting the task skill (Otaq types it), then says how the task skill's steps meet Otaq: asking through
/// the ticket, committing at the end, pushing only when the user asks, ending with `stage next`. Decision 089: it and
/// `noyan` never start a skill on their own initiative.
#[test]
fn work_ticket_runs_a_task_skill_the_otaq_way() {
    let cmd = naming::COMMAND_NAME;
    let work = flat(&read_skill("work-ticket"));
    for text in [
        "## With a task skill".to_owned(),
        "Follow \"With a task skill\" below in place of steps 3 and 4".to_owned(),
        "**never start the task skill yourself**".to_owned(),
        "Otaq types the task skill into this session next".to_owned(),
        format!("every question goes to the user through `{cmd} ask human --stdin`"),
        "never with the tool's own question tool (AskUserQuestion)".to_owned(),
        "commit everything at the end of the stage with Otaq's `commit` skill".to_owned(),
        "its \"push\" or \"open a PR\" is not a request from the user".to_owned(),
        format!("end your stage instead (step 6): `{cmd} stage next --stdin`"),
        "a skill that was started may call the skills it names".to_owned(),
        "about how, this skill wins; about what, the task skill wins".to_owned(),
        "**Never start a skill on your own initiative.**".to_owned(),
        // Decision 098: whether the stage has anything to do is the task skill's call (what), never the first turn's.
        "Don't judge yet whether your stage has anything to add: that is the task skill's call"
            .to_owned(),
        "When the task skill finds nothing to do, end the stage then".to_owned(),
    ] {
        assert!(work.contains(&text), "work-ticket must say {text:?}");
    }
    assert!(
        !work.contains(
            "When your stage has nothing to add, end it at once with a short note, as step 1 says"
        ),
        "work-ticket's first turn must not end a task-skill stage before its task skill runs"
    );
    let noyan = flat(&read_skill("noyan"));
    for text in [
        "**Never start a skill on your own initiative.**",
        "never for `tuq:product-spec` or any other skill",
    ] {
        assert!(noyan.contains(text), "noyan must say {text:?}");
    }
}

/// Decision 093: `work-ticket` copies the text between a tuq plan file's markers onto the ticket as its Plan, with the
/// command it gives, and the text it copies reads as the Plan's pieces.
#[cfg(unix)]
#[test]
fn work_ticket_copies_the_plan_files_plan_onto_the_ticket() {
    let cmd = naming::COMMAND_NAME;
    let work = read_skill("work-ticket");
    let lines: Vec<&str> = work.lines().map(str::trim).collect();
    let at = lines
        .iter()
        .position(|l| l.starts_with("awk '"))
        .expect("the awk line");
    let awk = lines[at];
    assert!(awk.ends_with(" > \"$plan\""), "{awk}");
    assert_eq!(lines[at - 1], "plan=\"$(mktemp)\"");
    assert_eq!(lines[at + 1], format!("{cmd} update --plan-file \"$plan\""));
    let plan = "1. Share token — waits for nothing\n   A token per list.\n   Check: tokens pass.\n\
                2. Share link — waits for 1\n   Check: the link opens.";
    let file = format!(
        "# Plan · wishlist\n\n- **Ticket:** SHOP-40\n\n<!-- tuq:plan:start -->\n\nWhy it is split.\n\n## Pieces\n\n\
         {plan}\n\n<!-- tuq:plan:end -->\n"
    );
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join("tuq/plans")).unwrap();
    std::fs::write(dir.path().join("tuq/plans/wishlist.md"), file).unwrap();
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(format!("F=wishlist\nplan=copied.md\n{awk}"))
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let copied = std::fs::read_to_string(dir.path().join("copied.md")).unwrap();
    assert_eq!(
        copied,
        format!("\nWhy it is split.\n\n## Pieces\n\n{plan}\n\n")
    );
    let pieces = otaq_core::domain::plan::pieces(&copied);
    assert_eq!(
        otaq_core::domain::plan::order_line(&pieces).as_deref(),
        Some("Pieces: 2 (1 \u{2192} 2)")
    );
}

/// T-0415 (FLW-11, FLW-21; d0108 "The CLI proposes a whole document", "Every why line names its source"): `flow`
/// reads with `flow show`, proposes with `flow propose --stdin`, waits for the answer as its next message, and
/// documents every field of the format.
#[test]
fn flow_teaches_show_propose_the_format_and_the_wait() {
    let cmd = naming::COMMAND_NAME;
    let flow = read_skill("flow");
    for text in [
        format!("{cmd} flow show"),
        format!("{cmd} flow propose --stdin"),
        format!("{cmd} flow presets"),
        "names its source".to_owned(),
        // A stage's description says what it settles; the reason for a change, with its evidence, goes in the
        // proposal's `why:`, shown under Why these changes and never stored in the flow.
        "only what the stage settles and when it matters".to_owned(),
        "**Why these changes**".to_owned(),
        "never stored in the flow".to_owned(),
        "next message".to_owned(),
        "Ask for more changes".to_owned(),
    ] {
        assert!(flow.contains(&text), "flow must say {text:?}");
    }
    let fields = [
        "id",
        "name",
        "kind",
        "by",
        "agent",
        "skill",
        "options",
        "rounds",
        "mode",
        "delivers",
        "description",
    ];
    for field in fields {
        assert!(
            flow.contains(&format!("| `{field}` |")),
            "flow must document `{field}`"
        );
    }
    // Agents have no skill list (T4): the agent's fields leave `skills` out.
    for field in ["role", "instructions", "why", "model"] {
        assert!(
            flow.contains(&format!("**`{field}`**")),
            "flow must document the agent's `{field}`"
        );
    }
}

/// T-0472 (FED-06): `flow` names the skill and writes it only in the folder `flow write-skill --name` gives, inside
/// the project, and hands it in with `--done`; "Ask the agent to write it" saves into the project, and the user
/// commits it, never the agent (T6).
#[test]
fn flow_writes_a_stages_skill_only_in_the_folder_it_is_given() {
    let cmd = naming::COMMAND_NAME;
    let flow = read_skill("flow");
    for text in [
        format!("{cmd} flow write-skill <stage> --name <name>"),
        format!("{cmd} flow write-skill <stage> --done"),
        "Write nothing anywhere else".to_owned(),
        "inside\nthis project".to_owned(),
        "Never commit it".to_owned(),
    ] {
        assert!(flow.contains(&text), "flow must say {text:?}");
    }
}

/// T-0415 (FLW-11), T-0546 (d0129, Q556): the `flow` skill's whole example is a flow `propose` reads and the checks
/// pass: the Spec first preset without its `skip` options, Spec review's fails going to Product spec.
#[test]
fn the_flow_skills_whole_example_reads_and_passes_the_checks() {
    let flow = read_skill("flow");
    let from = flow.find("A whole example").expect("a whole example");
    let start = from + flow[from..].find("```yaml\n").unwrap() + "```yaml\n".len();
    let end = start + flow[start..].find("```").unwrap();
    let file = otaq_core::flow::yaml::read(&flow[start..end]).expect("the example reads");
    assert_eq!(otaq_core::flow::check(&file.flow), Ok(()));
    let mut spec_first = otaq_core::flow::presets::preset("spec-first.yaml")
        .unwrap()
        .read();
    for stage in &mut spec_first.flow.stages {
        stage.options.clear();
    }
    assert_eq!(file, spec_first);
    let review = file.flow.stage("spec-review").unwrap();
    assert_eq!(review.fail_to.as_deref(), Some("product-spec"));
    assert!(flow.contains("Backlog \u{2192}\nProduct spec"));
}

/// T-0415 (FLW-21, FLW-15; d0108 rule 8, d0050): `open-pr` describes both modes: click (the user confirms in the
/// dialog) and automatic (started with `automatic: …`, Otaq confirms at once, no dialog); `push`, which pushes again
/// to the same PR after a Send back, does too. The words are T-0419's (`pr::service`).
#[test]
fn open_pr_and_push_describe_click_and_automatic_mode() {
    let cmd = naming::COMMAND_NAME;
    for (name, confirmed) in [
        ("open-pr", "The pull request was confirmed automatically"),
        ("push", "The push was confirmed automatically"),
    ] {
        let text = read_skill(name);
        for needle in [
            "**Click mode:**".to_owned(),
            "**Automatic mode:**".to_owned(),
            "starts with the word `automatic`".to_owned(),
            "- Mode: automatic (no dialog): nobody reviews your proposal; Otaq confirms it at once"
                .to_owned(),
            confirmed.to_owned(),
            format!("{cmd} pr propose --stdin"),
            format!("{cmd} pr opened"),
        ] {
            assert!(text.contains(&needle), "{name} must say {needle:?}");
        }
    }
    assert!(read_skill("open-pr").contains("Otaq's `push` skill"));
}

/// Commit messages follow the project's own rules: `commit` (an agent's own commits), `open-pr` and `push` (the PR's
/// per-commit messages) send the agent to the same places as the delivery's message prompt before it writes, keep the
/// key where the rules allow, and lead with the key only when nothing sets a format.
#[test]
fn commit_open_pr_and_push_follow_the_projects_message_rules() {
    let places = flat(otaq_core::pr::message::MESSAGE_RULES_PLACES);
    for name in ["commit", "open-pr", "push"] {
        let text = flat(&read_skill(name));
        for needle in [
            places.as_str(),
            "the Agent instructions, and the style of recent commits",
            "where they allow it",
        ] {
            assert!(text.contains(needle), "{name} must say {needle:?}");
        }
        assert!(
            text.to_lowercase().contains("when nothing sets a format"),
            "{name} must say what happens when nothing sets a format"
        );
    }
    let commit = flat(&read_skill("commit"));
    let rules = commit.find(&places).unwrap();
    let example = commit.find("git commit -m").unwrap();
    assert!(
        rules < example,
        "commit finds the rules before its first commit"
    );
}

/// A host's PR lists the tickets merged into it: `open-pr` and `push` read prime's "Tickets merged into this one" and
/// list each of them in the PR's description; `push` edits the open PR's description to add them.
#[test]
fn open_pr_and_push_list_the_merged_in_tickets_in_the_description() {
    for name in ["open-pr", "push"] {
        let text = flat(&read_skill(name));
        for needle in [
            "**Tickets merged into this one**",
            "description lists each of them, with its key, its title and a line on what it changed",
        ] {
            assert!(text.contains(needle), "{name} must say {needle:?}");
        }
    }
    let push = flat(&read_skill("push"));
    assert!(push.contains("gh pr edit 481 --body-file"));
    assert!(push.contains("az repos pr update --id 481 --description"));
}

/// T-0437 (ASK-09; d0108 "Each connected tool's agent looks for itself", "Every why line names its source"): `flow`
/// has "A first flow" between its steps and the file: its own tool's history (found by itself, only what the user
/// typed, never credentials or another tool's data), the repo with its AI files, nothing else (no `gh`, no home-folder
/// skills), the closest preset, why lines naming source and tool or "No evidence found", no `skip` or automatic
/// Delivery unasked, then `flow propose --stdin` and the end of the turn. No `otaq history` (ASK-01).
#[test]
fn flow_reads_its_own_history_and_the_repo_for_a_first_flow() {
    let cmd = naming::COMMAND_NAME;
    let flow = read_skill("flow");
    let at = |heading: &str| {
        flow.find(&format!("\n## {heading}\n"))
            .unwrap_or_else(|| panic!("flow has no {heading:?}"))
    };
    let (steps, first, file) = (at("Steps"), at("A first flow"), at("The file"));
    assert!(
        steps < first && first < file,
        "A first flow comes between Steps and The file"
    );
    let section = &flow[first..file];
    for text in [
        "**Your own tool's history on this Mac**".to_owned(),
        "is for you to find".to_owned(),
        "**only what the user typed**".to_owned(),
        "login, token or credential data".to_owned(),
        "**never another tool's data**".to_owned(),
        "**This repo**".to_owned(),
        "`AGENTS.md`".to_owned(),
        "`CLAUDE.md`".to_owned(),
        "`.claude/`".to_owned(),
        "`.cursor/rules`".to_owned(),
        "`git log`".to_owned(),
        "branch names".to_owned(),
        "CI files".to_owned(),
        "PR templates".to_owned(),
        "`CONTRIBUTING.md`".to_owned(),
        "**Nothing else.**".to_owned(),
        "no `gh`".to_owned(),
        "not the skills, plugins or\ninstruction files in the home folder".to_owned(),
        "closest preset".to_owned(),
        format!("{cmd} flow presets"),
        "names its source **and your tool**".to_owned(),
        "\"From 41 /security-review runs in billing-api (Claude Code)\"".to_owned(),
        "the proposal's `why:`".to_owned(),
        "\"No evidence found\"".to_owned(),
        "`skip` options".to_owned(),
        "an automatic delivery with no check before it".to_owned(),
        format!("{cmd} flow propose --stdin"),
        "**end your turn**".to_owned(),
    ] {
        assert!(section.contains(&text), "A first flow must say {text:?}");
    }
    for gone in [
        format!("{cmd} history"),
        "~/.claude/skills/".to_owned(),
        "glab".to_owned(),
    ] {
        assert!(!flow.contains(&gone), "flow must not say {gone:?}");
    }
}

#[test]
fn the_quoting_check_matches_yaml() {
    assert!(quoted(r#"'Otaq''s skill with "<KEY>: <what>" messages'"#));
    assert!(quoted(r#""a \"<KEY>: x\" message""#));
    assert!(!quoted(r#"Otaq's skill with "<KEY>: <what>" messages"#));
    assert!(!quoted("'it's'"));
}
