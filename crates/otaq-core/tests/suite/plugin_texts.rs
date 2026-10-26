//! Otaq's skill texts work on every tool (ADP-05, d0088): each `plugin/skills/*/SKILL.md` has front matter every
//! tool's YAML parser reads (Copilot rejects an unquoted `: ` in a value), and no text names a skill in one tool's
//! syntax: the agent uses the skill that `prime` names.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::claude::find_plugin_dir;
use otaq_core::naming;

/// Every SKILL.md of the plugin, with its folder name.
fn skill_files() -> Vec<(String, PathBuf)> {
    let skills = find_plugin_dir(Path::new("/nonexistent"))
        .expect("the repository's plugin folder")
        .join("skills");
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
/// and a skill another skill names is named by its copy, `otaq-<name>`.
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
            read(name).contains(&format!("(`{}`", naming::copy_name("commit"))),
            "{name} must name the commit skill by its copy"
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

/// The text of the shipped skill `name`.
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

/// T-0546 (d0129, d0084, d0131 Q514): `product-spec` finds where the project keeps its decisions (asking once when the
/// notes don't say), asks one question at a time with `ask human`, commits the decisions and docs, ends with
/// `stage next` naming what it wrote, and knows the plain Send back sentence.
#[test]
fn product_spec_asks_one_at_a_time_commits_and_moves_on() {
    let cmd = naming::COMMAND_NAME;
    let spec = flat(&read_skill("product-spec"));
    for step in [
        format!("{cmd} prime"),
        "Notes every agent in".to_owned(),
        "ask the user once".to_owned(),
        "record the answer in the ticket's description".to_owned(),
        format!("{cmd} update SHOP-40 --stdin"),
        format!("{cmd} ask human"),
        "Ask one question at a time".to_owned(),
        "what to build, for whom and why".to_owned(),
        format!(
            "with the commit skill `prime` names (`{}`",
            naming::copy_name("commit")
        ),
        format!("{cmd} stage next --stdin"),
        "Product spec: decisions/0140 (the share link), DESIGN.md".to_owned(),
        format!("Spec review sent SHOP-40 back to you with a comment. Run `{cmd} prime`."),
        "Only when the user agrees the ticket isn't needed".to_owned(),
    ] {
        assert!(spec.contains(&step), "product-spec must say {step:?}");
    }
    let ask = spec.find(&format!("{cmd} ask human")).unwrap();
    let commit = spec.find("**Commit everything**").unwrap();
    let next = spec.find(&format!("{cmd} stage next --stdin")).unwrap();
    assert!(
        ask < commit && commit < next,
        "ask, then commit, then stage next"
    );
    assert!(
        !spec.contains(&format!("{cmd} check ")),
        "product-spec gives no verdict"
    );
}

/// T-0546 (d0129, d0084): `tech-spec` reads what Product spec committed, asks only where a choice changes what gets
/// built and decides the rest, commits the architecture decisions and docs, and ends with `stage next`; when the card
/// walks its stage again it changes only what the change touches.
#[test]
fn tech_spec_reads_the_product_spec_decides_commits_and_moves_on() {
    let cmd = naming::COMMAND_NAME;
    let spec = flat(&read_skill("tech-spec"));
    for step in [
        format!("{cmd} prime"),
        "git diff main...HEAD".to_owned(),
        "Read what Product spec committed".to_owned(),
        "Ask the user only where a choice changes what gets built".to_owned(),
        "Decide everything else yourself".to_owned(),
        format!("{cmd} ask human"),
        format!(
            "with the commit skill `prime` names (`{}`",
            naming::copy_name("commit")
        ),
        format!("{cmd} stage next --stdin"),
        format!(
            "SHOP-40 is back in Tech spec: Spec review sent it to Product spec, which finished. Run `{cmd} prime`."
        ),
    ] {
        assert!(spec.contains(&step), "tech-spec must say {step:?}");
    }
    assert!(
        !spec.contains(&format!("{cmd} check ")),
        "tech-spec gives no verdict"
    );
}

/// T-0546 (d0129 as d0133 refines it): `milestone-plan` writes the ticket's Plan with `update --plan-file`, files no
/// tickets, writes the plan into the project's docs and commits, ends with `stage next`, and after a Send back starts
/// from the Plan as it is now, keeping the user's edits and changing only what the comment asks.
#[test]
fn milestone_plan_writes_the_plan_files_nothing_and_keeps_the_users_edits() {
    let cmd = naming::COMMAND_NAME;
    let plan = flat(&read_skill("milestone-plan"));
    for step in [
        format!("{cmd} prime"),
        format!("{cmd} update SHOP-40 --plan-file \"$plan\""),
        "**You file nothing.**".to_owned(),
        "**File nothing**".to_owned(),
        "they ask Noyan to file them".to_owned(),
        "**Start from the Plan as `prime` shows it now**".to_owned(),
        "**Keep the user's edits**".to_owned(),
        "unless their comment says otherwise".to_owned(),
        "**Change only what the comment asks**".to_owned(),
        format!(
            "with the commit skill `prime` names (`{}`",
            naming::copy_name("commit")
        ),
        format!("{cmd} stage next --stdin"),
        "Milestone plan: 4 pieces; docs/milestones/wishlist.md".to_owned(),
    ] {
        assert!(plan.contains(&step), "milestone-plan must say {step:?}");
    }
    assert!(
        !plan.contains(&format!("{cmd} create")),
        "milestone-plan files nothing"
    );
    assert!(
        !plan.contains(&format!("{cmd} check ")),
        "milestone-plan gives no verdict"
    );
    let write = plan.find("**Write the ticket's Plan**").unwrap();
    let next = plan.find("**End your stage**").unwrap();
    assert!(write < next, "the Plan, then stage next");
}

/// T-0546 (d0133 "The Plan has one fixed Pieces list"): the Plan `milestone-plan` shows reads with Otaq's own parser
/// as the pieces, titles and waits it means, so the skill and `domain::plan` can't drift apart.
#[test]
fn the_milestone_plan_example_parses_as_its_pieces() {
    let skill = read_skill("milestone-plan");
    let start = skill.find("<<'PLAN'\n").expect("the Plan example") + "<<'PLAN'\n".len();
    let end = start
        + skill[start..]
            .find("\nPLAN\n")
            .expect("the Plan example's end");
    let pieces = otaq_core::domain::plan::pieces(&skill[start..end]);
    let read: Vec<(u32, &str, &[u32])> = pieces
        .iter()
        .map(|p| (p.number, p.title.as_str(), p.waits_for.as_slice()))
        .collect();
    assert_eq!(
        read,
        [
            (1, "Wishlist API", &[][..]),
            (2, "Wishlist button", &[1][..]),
            (3, "Wishlist page", &[1][..]),
            (4, "Share link", &[2, 3][..]),
        ]
    );
    assert!(
        pieces.iter().all(|p| !p.text.is_empty()),
        "each piece has its details"
    );
    assert_eq!(
        otaq_core::domain::plan::order_line(&pieces).as_deref(),
        Some("Pieces: 4 (1 \u{2192} 2, 3 \u{2192} 4)")
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
        "next message".to_owned(),
        "Ask for more changes".to_owned(),
    ] {
        assert!(flow.contains(&text), "flow must say {text:?}");
    }
    let fields = [
        "id", "name", "kind", "by", "column", "agent", "skill", "options", "rounds", "mode",
        "delivers",
    ];
    for field in fields {
        assert!(
            flow.contains(&format!("| `{field}` |")),
            "flow must document `{field}`"
        );
    }
    for field in ["role", "instructions", "skills", "why", "model"] {
        assert!(
            flow.contains(&format!("**`{field}`**")),
            "flow must document the agent's `{field}`"
        );
    }
}

/// T-0472 (FED-06; d0092, d0023): `flow` writes a stage's skill only in the folder `flow write-skill` gives, and
/// hands it in with `--done`; it never writes in the repo for it.
#[test]
fn flow_writes_a_stages_skill_only_in_otaqs_folder() {
    let cmd = naming::COMMAND_NAME;
    let flow = read_skill("flow");
    for text in [
        format!("{cmd} flow write-skill <stage>"),
        format!("{cmd} flow write-skill <stage> --done"),
        "Write nothing anywhere else".to_owned(),
        "never in this repo".to_owned(),
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
    assert!(flow.contains("Backlog \u{2192} Ready \u{2192}\nProduct spec"));
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
    assert!(read_skill("open-pr").contains(&format!("`{}` skill", naming::copy_name("push"))));
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
        "\"From 41 /security-review runs in billing-api\n(Claude Code)\"".to_owned(),
        "\"No evidence found\"".to_owned(),
        "`skip` options".to_owned(),
        "an automatic Delivery with no check before it".to_owned(),
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
