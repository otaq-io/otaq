//! Otaq's plugin folder `plugin/` (T-0073, PLG-01…PLG-05, GIT-08; AGM-07): Claude Code's hooks plugin in `plugin/claude/`
//! is valid JSON, every hook calls a
//! `otaq hook --tool claude <event>` the Claude Code adapter knows, and the skills exist with the steps the decisions require. Real
//! `claude` behaviour is checked by hand.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::WORK_TICKET_SKILL;
use otaq_core::agent_tools::claude::find_plugin_dir;
use otaq_core::agent_tools::claude::hooks::HookEvent;
use otaq_core::board::settings::WorkflowStep;
use otaq_core::naming;
use serde_json::Value;

/// How the skills name the commit skill: the one `prime` names, by its copy's name `otaq-commit`, never one tool's
/// syntax (ADP-05, AGM-07).
const COMMIT_STEP: &str = "with the commit skill `prime` names (`otaq-commit`";

fn plugin() -> PathBuf {
    find_plugin_dir(Path::new("/nonexistent")).expect("the repository's plugin folder")
}

fn json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The front matter's `name:` and the body of a SKILL.md.
fn skill(name: &str) -> (String, String) {
    let path = plugin().join("skills").join(name).join("SKILL.md");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let rest = text.strip_prefix("---\n").expect("front matter");
    let (front, body) = rest.split_once("\n---\n").expect("front matter end");
    let field = |key: &str| {
        front
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{key}: ")))
            .unwrap_or_else(|| panic!("{name}: no {key}"))
            .to_owned()
    };
    assert!(!field("description").is_empty());
    (field("name"), body.to_owned())
}

/// The front matter of a SKILL.md, as `key: value` lines.
fn front_matter(name: &str) -> Vec<(String, String)> {
    let path = plugin().join("skills").join(name).join("SKILL.md");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let rest = text.strip_prefix("---\n").expect("front matter");
    let (front, _) = rest.split_once("\n---\n").expect("front matter end");
    front
        .lines()
        .filter_map(|l| l.split_once(": "))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect()
}

/// Every skill folder in the plugin.
fn skill_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(plugin().join("skills"))
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// AGM-07: Claude Code's plugin folder holds the hooks only, so no skill appears twice (Claude Code always loads a
/// plugin's `skills/`); the shipped skills sit beside it, in `plugin/skills/`.
#[test]
fn the_manifest_names_the_plugin_namespace() {
    let claude = plugin().join("claude");
    let manifest = json(&claude.join(".claude-plugin").join("plugin.json"));
    assert_eq!(manifest["name"], naming::PLUGIN_NAMESPACE);
    assert_eq!(manifest["license"], "FSL-1.1-MIT");
    assert!(manifest.get("skills").is_none());
    let mut entries: Vec<String> = std::fs::read_dir(&claude)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    assert_eq!(entries, [".claude-plugin", "hooks"]);
    assert!(plugin().join("skills").join(WORK_TICKET_SKILL).is_dir());
}

#[test]
fn every_hook_runs_an_accepted_otaq_hook_event() {
    let hooks = json(&plugin().join("claude").join("hooks").join("hooks.json"));
    let events = hooks["hooks"].as_object().expect("hooks object");
    let mut wired: Vec<HookEvent> = Vec::new();
    for (event, groups) in events {
        let parsed: HookEvent = event
            .parse()
            .unwrap_or_else(|_| panic!("{event} isn't an event `hook` accepts"));
        wired.push(parsed);
        for group in groups.as_array().unwrap() {
            for hook in group["hooks"].as_array().unwrap() {
                assert_eq!(hook["type"], "command");
                let command = hook["command"].as_str().unwrap();
                let call = format!(
                    "{} hook --tool {} {event}",
                    naming::COMMAND_NAME,
                    Tool::Claude.as_str()
                );
                assert!(command.ends_with(&call), "{command}");
                // Without the command installed, the hook is a no-op rather than an error.
                assert!(
                    command.starts_with(&format!("command -v {} ", naming::COMMAND_NAME)),
                    "{command}"
                );
                assert!(hook["timeout"].as_u64().unwrap() <= 10);
            }
        }
    }
    wired.sort();
    assert_eq!(
        wired,
        HookEvent::ALL,
        "Notification, Stop, StopFailure, UserPromptSubmit, SessionStart"
    );
}

#[test]
fn work_ticket_teaches_the_whole_path_to_the_next_stage() {
    let (name, body) = skill(WORK_TICKET_SKILL);
    assert_eq!(name, WORK_TICKET_SKILL);
    assert_eq!(naming::copy_name(&name), "otaq-work-ticket");
    let cmd = naming::COMMAND_NAME;
    let prime = body.find(&format!("`{cmd} prime`")).expect("prime first");
    let commit = body.find(COMMIT_STEP).expect("the Commit skill");
    let submit = body
        .find(&format!("{cmd} stage next --stdin"))
        .expect("stage next with stdin");
    let done = body
        .find(&format!("{cmd} stage done --stdin"))
        .expect("stage done when there is nothing to deliver");
    assert!(
        prime < commit && commit < submit && submit < done,
        "prime → commit → stage next, then when stage done fits"
    );
    for needle in [
        "Follow the Agent instructions",
        "never remove",
        "you committed nothing",
        "It is refused once the branch has commits",
        &format!("{cmd} ask human --stdin"),
        "Never push",
        "Long text always goes through stdin",
    ] {
        assert!(body.contains(needle), "work-ticket must say {needle:?}");
    }
}

#[test]
fn the_commit_skill_commits_everything_and_never_pushes() {
    let (name, body) = skill("commit");
    assert_eq!(name, "commit");
    for needle in [
        "git add -A",
        "SHOP-19: ",
        "Never push",
        "never use\n   `--no-verify`",
    ] {
        assert!(body.contains(needle), "commit must say {needle:?}");
    }
}

/// A skill's body with each command continued on the next line (a `\` line end) joined into one line.
fn joined(body: &str) -> String {
    let mut out = String::new();
    let mut continued = false;
    for line in body.lines() {
        let line = if continued { line.trim_start() } else { line };
        match line.strip_suffix('\\') {
            Some(part) => out.push_str(part),
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
        continued = line.ends_with('\\');
    }
    out
}

/// Every `ask human` command in a skill keeps the question on stdin (PLG-05), also when it continues over lines.
fn assert_every_ask_human_reads_stdin(body: &str) {
    let cmd = naming::COMMAND_NAME;
    for line in joined(body).lines().filter(|l| l.contains("ask human")) {
        assert!(
            !line.contains(&format!("{cmd} ask human")) || line.contains("--stdin"),
            "{line}"
        );
    }
}

/// T-0164, T-0087 (PLG-06, d0061), T-0376 (ATT-17): a skill teaches both forms of a question to the user, with the
/// rules for options and a pros-and-cons note on each, and keeps the question text on stdin (PLG-05).
fn assert_teaches_answer_options(skill_name: &str, options_example: &str, open_example: &str) {
    let (_, body) = skill(skill_name);
    let cmd = naming::COMMAND_NAME;
    let with_options = format!("{cmd} ask human {options_example} --stdin");
    assert!(
        joined(&body).contains(&with_options),
        "{skill_name}: the options example"
    );
    assert!(
        body.contains(&format!(
            "{cmd} ask human --stdin <<'QUESTION'\n{open_example}"
        )),
        "{skill_name}: the open-question example"
    );
    for needle in [
        "**2–4 options**",
        "short label",
        "recommend one when there is a sensible default",
        "Never invent options for an open question",
        "The question text always goes through stdin",
        "Agent instructions take precedence",
        "each option's pros and\n  cons in its `--note`, right after its `--option`",
        "Give every option one",
        "at most about 200 characters",
        "only the short labels and notes are arguments",
    ] {
        assert!(body.contains(needle), "{skill_name} must say {needle:?}");
    }
    assert_every_ask_human_reads_stdin(&body);
}

#[test]
fn work_ticket_teaches_answer_options() {
    assert_teaches_answer_options(
        WORK_TICKET_SKILL,
        "--option \"Postmark\" --note \"Already sends password resets: no new account or keys. Costs more per email.\" \
         --option \"SES\" --note \"Cheapest at volume, but needs AWS setup and domain checks first.\" \
         --option \"Resend\" --note \"Simple API, but a new account and a new vendor to manage.\" --recommended 1",
        "What should",
    );
}

/// T-0376 (DEC-07, d0084): work-ticket checks the ticket for gaps before any code, asks what changes what gets
/// built one question at a time with a note on every option, and asks nothing about a clear ticket.
#[test]
fn work_ticket_checks_the_ticket_before_any_code() {
    let (_, body) = skill(WORK_TICKET_SKILL);
    let text = flat(&body);
    let order = [
        "3. **Check the ticket before you build**",
        "4. **Plan, then implement**",
        "## Check the ticket first",
        "**It changes what gets built**",
        "**It only changes how you build it**",
        "**Most tickets are clear: then ask nothing and go straight on.**",
        "**One question at a time, most important first.**",
        "\"Question 1 of 3\"",
        "**Offer options, each with its pros and cons**",
        "**Write no code while a gap-check question is open.**",
        "Question 1 of 2: SHOP-31 says",
        "## Product questions while you work: record, or ask only if you can't go on",
    ];
    let mut at = 0;
    for step in order {
        let found = text[at..]
            .find(step)
            .unwrap_or_else(|| panic!("work-ticket must say {step:?} after the step before"));
        at += found;
    }
}

/// T-0087 (REV-05, PLG-02, PLG-03; AGM-12): address-review reads the feedback after prime, then commits and ends its
/// stage with `stage next`.
#[test]
fn address_review_reads_the_feedback_fixes_commits_and_submits() {
    let (name, body) = skill("address-review");
    assert_eq!(name, "address-review");
    let cmd = naming::COMMAND_NAME;
    let order = [
        format!("`{cmd} prime`"),
        "Follow the Agent instructions".to_owned(),
        format!("`{cmd} review feedback`"),
        "Fix every comment".to_owned(),
        COMMIT_STEP.to_owned(),
        format!("{cmd} stage next --stdin"),
    ];
    let mut at = 0;
    for step in &order {
        let found = body[at..]
            .find(step.as_str())
            .unwrap_or_else(|| panic!("address-review must say {step:?} after the step before"));
        at += found;
    }
    assert!(body.contains("Never push"));
    assert_teaches_answer_options(
        "address-review",
        "--option \"Add a setting\" --note \"Users can pick their page size; a new Settings row and a migration.\" \
         --option \"Keep 20\" --note \"No new setting to maintain, but the review comment stays unaddressed.\" \
         --recommended 1",
        "What should",
    );
}

/// T-0090 (GIT-03, d0038; AGM-12): restack rebases onto the target prime names, resolves, commits and ends its stage
/// with `stage next`.
#[test]
fn restack_rebases_resolves_commits_and_submits() {
    let (name, body) = skill("restack");
    assert_eq!(name, "restack");
    let cmd = naming::COMMAND_NAME;
    let order = [
        format!("`{cmd} prime`"),
        "Follow the Agent instructions".to_owned(),
        "Rebase onto the target".to_owned(),
        "Resolve every conflict".to_owned(),
        COMMIT_STEP.to_owned(),
        format!("{cmd} stage next --stdin"),
    ];
    let mut at = 0;
    for step in &order {
        let found = body[at..]
            .find(step.as_str())
            .unwrap_or_else(|| panic!("restack must say {step:?} after the step before"));
        at += found;
    }
    for needle in [
        "Never push",
        "never stash",
        "Long text always goes through stdin",
    ] {
        assert!(body.contains(needle), "restack must say {needle:?}");
    }
}

/// T-0140 (REV-10, PLG-05, d0050; T-0415 FLW-21): open-pr proposes after prime, waits for the confirmation (the
/// user's in click mode, Otaq's at once in automatic mode), and only then pushes and reports the PR; provider
/// detection follows the remote URL.
#[test]
fn open_pr_proposes_first_and_pushes_only_after_the_confirmation() {
    let (name, body) = skill("open-pr");
    assert_eq!(name, "open-pr");
    let cmd = naming::COMMAND_NAME;
    let order = [
        format!("`{cmd} prime`"),
        "Follow the Agent instructions".to_owned(),
        "github.com".to_owned(),
        "dev.azure.com".to_owned(),
        "your own Open PR skill".to_owned(),
        format!("{cmd} pr propose --stdin"),
        "End your turn".to_owned(),
        "The user confirmed".to_owned(),
        "git push".to_owned(),
        "gh pr create".to_owned(),
        "az repos pr create".to_owned(),
        format!("{cmd} pr opened"),
    ];
    let mut at = 0;
    for step in &order {
        let found = body[at..]
            .find(step.as_str())
            .unwrap_or_else(|| panic!("open-pr must say {step:?} after the step before"));
        at += found;
    }
    for needle in [
        "Never push before Otaq tells you the proposal is confirmed",
        "visualstudio.com",
        &format!("{}/SHOP-18-", naming::PLUGIN_NAMESPACE),
        "Never store, print or ask for a token",
        "Long text always goes through stdin",
    ] {
        assert!(body.contains(needle), "open-pr must say {needle:?}");
    }
}

/// T-0140 (REV-14, d0041, d0050): push proposes the new commits' messages, then pushes after the confirmation.
#[test]
fn push_proposes_the_new_commits_and_pushes_after_the_confirmation() {
    let (name, body) = skill("push");
    assert_eq!(name, "push");
    let cmd = naming::COMMAND_NAME;
    let order = [
        format!("`{cmd} prime`"),
        "Follow the Agent instructions".to_owned(),
        format!("{cmd} pr propose --stdin"),
        "End your turn".to_owned(),
        "The user confirmed".to_owned(),
        "git push origin".to_owned(),
        format!("{cmd} pr opened"),
    ];
    let mut at = 0;
    for step in &order {
        let found = body[at..]
            .find(step.as_str())
            .unwrap_or_else(|| panic!("push must say {step:?} after the step before"));
        at += found;
    }
    assert!(body.contains("never with `--force`"));
}

/// T-0140 (MSG-03, d0030): answer-question answers only and replies with `answer <ID> --stdin`.
#[test]
fn answer_question_changes_nothing_and_ends_with_answer() {
    let (name, body) = skill("answer-question");
    assert_eq!(name, "answer-question");
    let cmd = naming::COMMAND_NAME;
    let prime = body.find(&format!("`{cmd} prime`")).expect("prime first");
    let answer = body
        .find(&format!("{cmd} answer "))
        .expect("answer with the ID");
    assert!(prime < answer);
    for needle in [
        "Follow the Agent instructions",
        "You change nothing",
        &format!("don't run `{cmd} stage next`"),
        "don't commit",
        "don't push",
        "--stdin <<'ANSWER'",
    ] {
        assert!(body.contains(needle), "answer-question must say {needle:?}");
    }
}

/// T-0191 (PLG-08, d0063): the validate skill is the default Validate skill, user-invoked only, a read-only judge
/// that reads `prime --validate`, checks the description, the checks and leftovers, writes notes for gaps another
/// ticket covers or this one doesn't ask for, and ends with `check pass|fail --stdin`.
#[test]
fn validate_is_a_read_only_judge_that_ends_with_a_verdict() {
    let (name, body) = skill("validate");
    assert_eq!(name, "validate");
    assert_eq!(WorkflowStep::Validate.own_skill(), name);
    assert!(
        front_matter("validate")
            .contains(&("disable-model-invocation".to_owned(), "true".to_owned()))
    );
    let cmd = naming::COMMAND_NAME;
    let order = [
        "read-only judge".to_owned(),
        format!("`{cmd} prime --validate`"),
        "Follow the Agent instructions".to_owned(),
        "against the description".to_owned(),
        "infer reasonable ones".to_owned(),
        "**each review comment**".to_owned(),
        "Run the checks".to_owned(),
        "obvious ones".to_owned(),
        "leftovers and out-of-scope edits".to_owned(),
        "one line per finding, with its file and line".to_owned(),
        "**covered by a linked ticket** is a **note**, never a failure".to_owned(),
        "Note: UI for adding items: covered by SHOP-40.2".to_owned(),
        "**not asked for by this ticket**".to_owned(),
        "Note: Not covered by any ticket: removing items".to_owned(),
        format!("{cmd} check fail --stdin"),
        format!("{cmd} check pass --stdin"),
    ];
    let mut at = 0;
    for step in &order {
        let found = body[at..]
            .find(step.as_str())
            .unwrap_or_else(|| panic!("validate must say {step:?} after the step before"));
        at += found;
    }
    for needle in [
        "Never edit, create or delete files, never commit, and never fix anything",
        "Long text always goes through stdin",
        &format!("don't run `{cmd} stage next`"),
    ] {
        assert!(body.contains(needle), "validate must say {needle:?}");
    }
    // The example notes are what the service reads as notes (the rest are findings).
    let notes: Vec<&str> = body
        .lines()
        .filter_map(otaq_core::board::validation::note_of)
        .collect();
    assert!(
        notes.contains(&"Not covered by any ticket: removing items"),
        "{notes:?}"
    );
}

/// T-0191 (PLG-07, d0067; AGM-12): no skill mentions "Done when"; the agent's skills say to end the turn after
/// `stage next` and to fix and run it again after a failed validation. T-0445 (CMT-08, CMT-09; d0109): the failed
/// check's message says only "Run `otaq prime`": the findings are the checker's comment, its run the report.
#[test]
fn no_skill_mentions_done_when_and_the_agent_waits_for_validation() {
    let names = skill_names();
    assert!(names.contains(&"validate".to_owned()), "{names:?}");
    for name in &names {
        let (_, body) = skill(name);
        assert!(
            !body.to_lowercase().contains("done when"),
            "{name} mentions Done when"
        );
    }
    let cmd = naming::COMMAND_NAME;
    for name in [WORK_TICKET_SKILL, "address-review"] {
        let (_, body) = skill(name);
        let submit = body
            .find(&format!("{cmd} stage next --stdin"))
            .expect("stage next");
        let rest = flat(&body[submit..]);
        for needle in [
            "**end your turn** and wait",
            "arrives in this session as a message",
            "If validation failed",
            &format!("run `{cmd} prime`, read the checker's comment"),
            "search its report when you need the details",
            &format!("fix them, commit everything and run `{cmd} stage next` again"),
        ] {
            assert!(
                rest.contains(needle),
                "{name} must say {needle:?} after submitting"
            );
        }
    }
    let (_, work) = skill(WORK_TICKET_SKILL);
    assert!(work.contains("until the ticket is done, as its description describes"));
}

/// T-0445 (CMT-09, CMT-08; d0109): the shipped skills keep the hand-over short and long output in reports. Each says
/// to send long command output to a file in `$OTAQ_RESOURCE_DIR` (the epic lead, which has no ticket, to a folder outside
/// the repository), never the worktree, to search it and never print it in full or retype it, and how to read a report
/// (`report <KEY> <N> --path`). The skills that end a stage keep `prime` first, say Otaq refuses the stage end without
/// it, and attach the run with `--report-file`. No skill pipes anything into an `otaq` command: stdin carries only the
/// short text, from a heredoc.
#[test]
fn the_skills_hand_over_short_comments_and_keep_long_output_in_reports() {
    let cmd = naming::COMMAND_NAME;
    let resource = format!("`${}`", naming::resource_dir_env_var());
    let ends_stage = [
        WORK_TICKET_SKILL,
        "grill",
        "validate",
        "address-review",
        "restack",
    ];
    for name in ends_stage
        .iter()
        .chain(&["answer-question", "commit", "epic-lead"])
    {
        let (_, body) = skill(name);
        let text = flat(&body);
        assert!(
            text.contains("never print it in full or retype it"),
            "{name}: the long-output rule"
        );
        if *name != "epic-lead" {
            assert!(text.contains(&resource), "{name}: {resource}");
        }
        if *name != "commit" {
            assert!(
                text.contains(&format!("$({cmd} report ")) && text.contains(" --path)\""),
                "{name}: reading a report by its path"
            );
        }
    }
    for name in ends_stage {
        let (_, body) = skill(name);
        let text = flat(&body);
        let prime = text.find(&format!("`{cmd} prime")).expect("prime");
        let gate = text
            .find(&format!("Run `{cmd} prime` first: SHOP-"))
            .unwrap_or_else(|| panic!("{name}: the read gate's refusal"));
        assert!(prime < gate, "{name}: prime is step 1");
        assert!(
            text.contains("Otaq refuses"),
            "{name}: Otaq refuses the stage end"
        );
        assert!(
            text.to_lowercase().contains("never in the worktree"),
            "{name}: never the worktree"
        );
        assert!(
            text.contains(&format!(
                "--report-file \"${}/",
                naming::resource_dir_env_var()
            )),
            "{name}: attaches the run with --report-file"
        );
    }
    let (_, validate) = skill("validate");
    let validate = flat(&validate);
    for verdict in ["fail", "pass"] {
        assert!(
            validate.contains(&format!(
                "{cmd} check {verdict} --stdin --report-file \"$OTAQ_RESOURCE_DIR/test-run.txt\""
            )),
            "validate: check {verdict} attaches the run"
        );
    }
    for needle in [
        "The one place you may write is `$OTAQ_RESOURCE_DIR`",
        "one or two lines",
        "is the one exception",
    ] {
        assert!(validate.contains(needle), "validate must say {needle:?}");
    }
    let (_, review) = skill("address-review");
    let review = flat(&review);
    for needle in [
        "the user's comment from the Send back is the latest one",
        &format!("Run `{cmd} prime`, then `{cmd} review feedback`."),
        "prints the line comments of the latest round",
        "the user's comment in `prime` is all the feedback",
    ] {
        assert!(
            review.contains(needle),
            "address-review must say {needle:?}"
        );
    }
    for gone in ["the user's note", "general note"] {
        assert!(!review.contains(gone), "address-review still says {gone:?}");
    }
    let (_, lead) = skill(otaq_core::agent_tools::EPIC_LEAD_SKILL);
    let lead = flat(&lead);
    for needle in [
        &format!("{cmd} comment SHOP-40.2 --stdin <<'COMMENT'"),
        "**later stages**",
        "the agent running now doesn't see it (that is what a note is for, step 6)",
        "at most one note to a running child",
        "last comment",
    ] {
        assert!(lead.contains(needle), "epic-lead must say {needle:?}");
    }
    assert!(!lead.contains("summary"), "epic-lead still reads a summary");
    // Stdin carries only short text from a heredoc: nothing is piped into an `otaq` command.
    for name in skill_names() {
        let (_, body) = skill(&name);
        for line in joined(&body).lines() {
            assert!(
                !line.contains(&format!("| {cmd} ")),
                "{name} pipes into {cmd}: {line}"
            );
        }
    }
}

/// T-0202 (PLG-09, PLG-01, PLG-02, PLG-05; d0068, d0040): the project-agent skill is user-invoked only (the session
/// starts with it), reads `prime --project` first, follows the Project agent instructions, names itself after the
/// first request, investigates without editing or committing, files Backlog tickets with the CLI (long text on
/// stdin, batches with `create -f`), uses the user's skills for imports, asks with `ask <KEY>` and `ask human`, and
/// tells the user what it would need when the CLI refuses.
#[test]
fn project_agent_investigates_and_files_backlog_tickets() {
    let (name, body) = skill(otaq_core::agent_tools::PROJECT_AGENT_SKILL);
    assert_eq!(name, "project-agent");
    assert!(
        front_matter("project-agent")
            .contains(&("disable-model-invocation".to_owned(), "true".to_owned()))
    );
    let cmd = naming::COMMAND_NAME;
    let order = [
        format!("`{cmd} prime --project`"),
        "Follow the Project agent instructions".to_owned(),
        "Imports from trackers use the user's own\n   skills**".to_owned(),
        "the text after\n   this skill's invocation".to_owned(),
        "without waiting for another prompt".to_owned(),
        "Right after you\n   read the first request".to_owned(),
        format!("{cmd} agent name \"Checkout slowness\""),
        "Rename yourself only when the user asks".to_owned(),
        "ticket worktrees".to_owned(),
        "**Never edit files and never commit**".to_owned(),
        format!("{cmd} create \"Slow checkout query\" -p 2 --deps SHOP-19 --stdin"),
        format!("`{cmd} create -f tickets.md`"),
        "--parent".to_owned(),
        "**still in Backlog**".to_owned(),
        format!("`{cmd} dep add SHOP-35 SHOP-19`"),
        format!("`{cmd} ask SHOP-18 --stdin`"),
        format!("`{cmd} ask human --stdin`"),
    ];
    let mut at = 0;
    for step in &order {
        let found = body[at..]
            .find(step.as_str())
            .unwrap_or_else(|| panic!("project-agent must say {step:?} after the step before"));
        at += found;
    }
    for needle in [
        "**Never edit files, never commit, never push.**",
        "You can't move, start, approve, merge, close or delete tickets",
        "tell the user what you would need",
        otaq_core::policy::BACKLOG_ONLY,
        "**Long text always goes through stdin**",
        "**2–4 options**",
        "Never invent options for an open question",
        "The question text always goes through stdin",
    ] {
        assert!(body.contains(needle), "project-agent must say {needle:?}");
    }
    assert_every_ask_human_reads_stdin(&body);
}

/// The text of the first `<<'TAG'` heredoc after `from` in a skill, with each line's indent removed.
fn heredoc(body: &str, from: &str, tag: &str) -> String {
    let start = body.find(from).unwrap_or_else(|| panic!("no {from:?}"));
    let open = format!("<<'{tag}'\n");
    let rest = &body[start..];
    let rest = &rest[rest.find(&open).expect("heredoc") + open.len()..];
    rest.lines()
        .map(str::trim)
        .take_while(|l| *l != tag)
        .collect::<Vec<_>>()
        .join("\n")
}

/// T-0286 (DEC-01, LEAD-10; d0084, d0083, d0040): work-ticket reads its notes at each new step, records a product
/// question it can go on without (the example parses as a complete decision), asks only when it can't go on, lets
/// the Agent instructions add to the rule, and ends its submit summary with notes for later tickets.
#[test]
fn work_ticket_records_product_choices_and_reads_its_notes() {
    let (_, body) = skill(WORK_TICKET_SKILL);
    let cmd = naming::COMMAND_NAME;
    let order = [
        format!("`{cmd} prime`"),
        format!("Run `{cmd} notes` each time you start a new step"),
        COMMIT_STEP.to_owned(),
        format!("{cmd} stage next --stdin"),
        "Notes for later tickets:".to_owned(),
        "If you can't go on without the answer".to_owned(),
        "Otherwise decide and record it".to_owned(),
        format!("{cmd} decision record --stdin <<'DECISION'"),
        "Agent instructions may add to this rule".to_owned(),
        format!("{cmd} ask human --stdin"),
    ];
    let mut at = 0;
    for step in &order {
        let found = body[at..]
            .find(step.as_str())
            .unwrap_or_else(|| panic!("work-ticket must say {step:?} after the step before"));
        at += found;
    }
    let example = heredoc(&body, &format!("{cmd} decision record"), "DECISION");
    let decision = otaq_core::decisions::NewDecision::parse(&example).expect("the example parses");
    assert!(decision.missing().is_empty(), "{example}");
    assert_eq!(decision.built, "only in the profile form");
}

/// T-0286 (LEAD-10, d0083): the Commit skill reads the lead's notes before it stages anything.
#[test]
fn the_commit_skill_reads_notes_first() {
    let (_, body) = skill("commit");
    let cmd = naming::COMMAND_NAME;
    let notes = body
        .find(&format!("Run `{cmd} notes` first"))
        .expect("notes");
    let status = body.find("`git status`").expect("status");
    let add = body.find("git add -A").expect("add");
    assert!(notes < status && status < add, "notes → status → add");
    assert!(body.contains("apply it to your work before you commit"));
}

/// T-0496 (d0124): the lead reads the open children for the same scope before it adds one, sends a running child's
/// lesson as a note instead of a sibling, and knows the children the merge freed wait for its merge turn.
#[test]
fn epic_lead_notes_a_running_child_instead_of_adding_a_sibling() {
    let (_, body) = skill(otaq_core::agent_tools::EPIC_LEAD_SKILL);
    let cmd = naming::COMMAND_NAME;
    let order = [
        "**Before you add a child, read the open children for the same scope:**".to_owned(),
        format!("(`{cmd} show SHOP-40.3`)"),
        format!("(`{cmd} comments SHOP-40.3`)"),
        "**A lesson for a child that is already running goes to it as a note, never as a\n  new child**".to_owned(),
        format!("{cmd} create \"Wishlist empty state\" --parent SHOP-40"),
        "## The merge turn".to_owned(),
        "don't start until this turn ends (\"Waiting for the lead's update\" on their cards)".to_owned(),
        "only after reading the open children for the same scope".to_owned(),
        "not into a new child".to_owned(),
        format!("{cmd} note SHOP-40.4 --stdin"),
    ];
    let mut at = 0;
    for step in &order {
        let found = body[at..]
            .find(step.as_str())
            .unwrap_or_else(|| panic!("epic-lead must say {step:?} after the step before"));
        at += found;
    }
}

/// T-0286 (LEAD-01, LEAD-05…LEAD-10, DEC-04; d0083): the epic-lead skill is user-invoked only, reads `prime --epic`
/// first, stays read-only and CLI-only, plans with `create --parent` and dependencies, passes on what merged, marks
/// contradicted decisions with a source, answers or passes on questions from written sources, sends one note with
/// `--view` to a stuck child, suggests closes, never does what is the user's, and ends every turn idle. It contains
/// no instruction that edits files: every command in its examples is an `otaq` command.
#[test]
fn epic_lead_directs_through_the_cli_and_never_edits_files() {
    let (name, body) = skill(otaq_core::agent_tools::EPIC_LEAD_SKILL);
    assert_eq!(name, "epic-lead");
    assert!(
        front_matter("epic-lead")
            .contains(&("disable-model-invocation".to_owned(), "true".to_owned()))
    );
    let cmd = naming::COMMAND_NAME;
    let order = [
        format!("`{cmd} prime --epic`"),
        "**Never edit, create or delete files, never commit.**".to_owned(),
        "Never use subagents or agent teams".to_owned(),
        "name the source".to_owned(),
        "never answer a permission prompt".to_owned(),
        "don't look for a way around it".to_owned(),
        "**End every turn idle.**".to_owned(),
        format!("{cmd} create \"Wishlist empty state\" --parent SHOP-40 --deps SHOP-40.3 --stdin"),
        format!("`{cmd} dep add SHOP-40.3 SHOP-40.2`"),
        format!("`{cmd} update SHOP-40.3 --stdin`"),
        "## Checking the plan".to_owned(),
        "## The merge turn".to_owned(),
        "Notes for later tickets".to_owned(),
        format!("{cmd} update SHOP-40.3 --stdin <<'BODY'"),
        "Never\n   overrule it".to_owned(),
        format!("{cmd} decision mark SHOP-40.1#1 --source \"PAY-123\" --stdin"),
        "only when it could\n   otherwise start before the other merges".to_owned(),
        "at most one note to a running child".to_owned(),
        format!("{cmd} note SHOP-40.4 --stdin"),
        format!("`{cmd} questions`"),
        format!("{cmd} answer 7 --stdin"),
        format!("{cmd} ask human --pass-on 7"),
        "--recommended 1 --stdin <<'REASON'".to_owned(),
        "Never guess".to_owned(),
        format!("{cmd} note SHOP-40.3 --view \""),
        "**Don't retry:**".to_owned(),
        format!("{cmd} suggest close SHOP-40.6 --stdin"),
        format!("`{cmd} suggest delete <KEY> --stdin`"),
        "dismissed by the user".to_owned(),
        format!("{cmd} ask human \\\n  --option \"One child\" --note"),
    ];
    let mut at = 0;
    for step in &order {
        let found = body[at..]
            .find(step.as_str())
            .unwrap_or_else(|| panic!("epic-lead must say {step:?} after the step before"));
        at += found;
    }
    for needle in [
        "never move,\n  queue, take back, park, approve or merge",
        "never edit a started child",
        "**Long text always goes through stdin**",
    ] {
        assert!(body.contains(needle), "epic-lead must say {needle:?}");
    }
    // No file-editing instruction anywhere in the text.
    let lower = body.to_lowercase();
    for banned in [
        "git add",
        "git commit",
        "git push",
        "git merge",
        "git rebase",
        "git stash",
        "git reset",
        "git checkout",
        "sed -i",
        "rm -",
        "mkdir",
        "edit tool",
        "write tool",
        "/commit",
    ] {
        assert!(!lower.contains(banned), "epic-lead mentions {banned:?}");
    }
    // Every command line in the examples (outside heredoc bodies, with `\` continuations joined) is an `otaq`
    // command, with no redirection into a file.
    let mut in_block = false;
    let mut heredoc_end: Option<String> = None;
    let mut commands = 0;
    for line in joined(&body).lines().map(str::trim) {
        if line.starts_with("```") {
            in_block = !in_block;
            continue;
        }
        if !in_block {
            continue;
        }
        if let Some(end) = &heredoc_end {
            if line == end {
                heredoc_end = None;
            }
            continue;
        }
        assert!(
            line.starts_with(&format!("{cmd} ")),
            "not a CLI command: {line}"
        );
        let open = line.find("<<'");
        let without_heredoc = open.map_or(line, |o| &line[..o]);
        assert!(
            !without_heredoc.contains('>'),
            "redirects into a file: {line}"
        );
        if let Some(o) = open {
            let tag = &line[o + 3..];
            heredoc_end = Some(tag.trim_end_matches('\'').to_owned());
        }
        commands += 1;
    }
    assert!(heredoc_end.is_none() && !in_block);
    assert!(commands >= 8, "only {commands} example commands");
    assert_every_ask_human_reads_stdin(&body);
}

/// A skill's body with every run of whitespace as one space, so a phrase reads the same wherever the text wraps.
fn flat(body: &str) -> String {
    body.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// T-0339 (PAG-03; d0087), T-0409 (AGM-12, AGM-09; d0108): the project-agent skill proposes two or more tickets, or
/// an epic, with a table (# · Title · Description · Choices; the short table above about 10), creates one explicit
/// ticket at once with its choice in the reply, sets stage choices by difficulty with a reason ("typo fix → Dev: Quick
/// developer"), tells *set by you* from its own choice, and leaves the Choices column out when no stage has options
/// (the default flow). The real flags only. T-0376 (PAG-08, d0091): it decides the shape and writes the plan into the
/// epic.
#[test]
fn project_agent_proposes_with_a_table_and_sets_stage_choices() {
    let (_, body) = skill(otaq_core::agent_tools::PROJECT_AGENT_SKILL);
    let text = flat(&body);
    let cmd = naming::COMMAND_NAME;
    for needle in [
        "Two or more tickets, or an epic:** propose first, then wait for the user's go-ahead",
        "One ticket the user asked for explicitly**",
        &format!(
            "{cmd} create \"Fix README typo\" --stage \"Dev=Quick developer\" --reason \"typo fix\" --stdin"
        ),
        "\"Created SHOP-41 · typo fix → Dev: Quick developer\"",
        "\"I'm proposing one epic and 2 tickets\"",
        "**# · Title · Description · Choices**",
        "2 Order detail page items, totals, reorder Dev: Quick developer · Review: skip",
        "change any row by its number",
        "(\"2 needs a review\", \"drop 2\")",
        "joined with \" · \" (\"Dev: Quick developer · Review: skip\")",
        "It reads \"—\" when the ticket keeps every stage's own agent",
        "**Above about 10 tickets**",
        "**without the Description column** (**# · Title · Choices**)",
        "**first 10 rows**",
        "\"…50 more (say 'show all' to list them)\"",
        "When the user says \"show all\", list every row",
        "**Only stages with options have choices.** `prime --project` lists them under **Stage choices**",
        "a cheaper agent for a minor fix, a documentation change or a small config change (\"typo fix → Dev: Quick developer\")",
        "**If you're unsure, leave the stage's own agent**",
        "**Every choice has a reason**, passed with `--reason`",
        "with no second approval",
        "a choice the user set is never changed back by an agent",
        "**A choice the user named** (\"2 needs a review\", \"use Quick developer for all of these\", \"keep these choices\") is the user's: add `--by-user`",
        "**A plain approval of your own suggestion** (\"yes\", \"go\") leaves it yours",
        &format!(
            "`{cmd} create \"Order history\" --type epic --stdin` with the plan as its description"
        ),
        "You are the one who splits epics: the epic's lead only runs the plan you write",
        "**Decide the shape, then propose before you create.**",
        "## One ticket, several, or an epic",
        "sent by the epic card's **Split with the Project agent**",
        "### Write the plan into the epic",
        "the epic's description **is the plan**: what the epic delivers, then one line per child with why it exists and why it comes where it does, then the sources",
        &format!("{cmd} create \"Wishlist\" --type epic --stdin <<'BODY'"),
        &format!("(`{cmd} update SHOP-40 --stdin`), keeping what the user wrote above it"),
        "--parent SHOP-42",
        "There is no `--epic` flag",
        "- **Stage:** Dev=Quick developer · typo fix",
        "When `prime --project` lists no **Stage choices** (the default flow has none), there is nothing to pick: **leave the Choices column out** (**# · Title · Description**), and pass no `--stage` flags",
        "Otaq shows no dialog for it",
    ] {
        assert!(text.contains(needle), "project-agent must say {needle:?}");
    }
    assert!(
        !text.contains("--agent ") && !text.contains("--why "),
        "old flags"
    );
    assert!(!text.contains("create --epic"), "no --epic flag");
    for gone in [
        "Agent column",
        "\"Use for\"",
        "Light: 34",
        "Project default",
    ] {
        assert!(!text.contains(gone), "project-agent still says {gone:?}");
    }
}

/// T-0339 (LEAD-03; d0087), T-0409 (AGM-12, AGM-08; d0108): the epic-lead skill takes the drag as the go-ahead for
/// its children's stage choices, may change a never-started child's choice after a merge with a reason, never one the
/// user set, and sets none when no stage has options (the default flow). T-0376 (LEAD-13, d0091): it doesn't plan an
/// epic from scratch (no proposal table); asked to split, it points to the Project agent.
#[test]
fn epic_lead_points_splits_to_the_project_agent_and_changes_choices_after_a_merge() {
    let (_, body) = skill(otaq_core::agent_tools::EPIC_LEAD_SKILL);
    let text = flat(&body);
    let cmd = naming::COMMAND_NAME;
    let order = [
        "## The plan",
        "**you don't plan an epic from scratch**",
        "The epic's description **is the plan**",
        "**If the user asks you to split the epic, or to split it again**",
        "say that the Project agent does that (from the Ask bar, or Split with the Project agent on an epic card with no children), and end your turn",
        "**Keeping the plan current is yours:**",
        "## Stage choices",
        "## Checking the plan",
        "**Dragging the epic is the go-ahead for its children and their stage choices**",
        "## The merge turn",
        "**Change a never-started child's stage choice**",
    ];
    let mut at = 0;
    for step in order {
        let found = text[at..]
            .find(step)
            .unwrap_or_else(|| panic!("epic-lead must say {step:?} after the step before"));
        at += found;
    }
    for needle in [
        "`prime --epic` lists them under **Stage choices**",
        "**When it lists none (the default flow has none), there is nothing to pick:** pass no `--stage` flags",
        "**Choose by how hard the work is**",
        "(\"copy only → Dev: Quick developer\")",
        "leave the stage's own agent (no flag)",
        "**Every choice has a reason** (`--reason \"…\"`)",
        &format!("`{cmd} update SHOP-40.4 --stage \"Dev=Quick developer\" --reason \"…\"`"),
        "(\"2 needs a review\", \"use Quick developer for all of these\", \"keep these choices\") is the user's: add `--by-user`",
        "A plain approval of your own suggestion (\"yes\", \"go\") leaves it your choice",
        "Never change a choice the user set",
        "never wait for a separate approval",
        "(\"40.3 is only a config change: Dev → Quick developer\")",
        "**A choice the user made themselves is never changed by you**",
        &format!("{cmd} update SHOP-40.3 --stage \"Dev=Quick developer\" --reason \""),
    ] {
        assert!(text.contains(needle), "epic-lead must say {needle:?}");
    }
    for gone in [
        "Planning with the user",
        "propose the plan first",
        "**# · Title · Description · Agent**",
        "Agent picks",
        "\"Use for\"",
        "Epic lead row",
    ] {
        assert!(!text.contains(gone), "epic-lead still says {gone:?}");
    }
}
