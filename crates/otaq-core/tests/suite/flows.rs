//! The flow format (STG-01, STG-02, STG-03, STG-12; d0108, UX-65, UX-73): the default file and the test flow read and
//! write back unchanged, each STG-02 rule refuses its broken file in `tests/flows/broken/` with its message, and an
//! older file (`tests/flows/older/`: agents named by the stages) reads as each stage's own model and instructions. A
//! project's stored flow is `flow_store`'s.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::Tool;
use otaq_core::flow::{
    self, CheckBy, Delivers, Delivery, DeliveryMode, Rule, StageKind, WhoDoesIt, check, yaml,
};
use otaq_core::naming;

fn flows() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows")
}

fn file(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn every_kind() -> String {
    file(&flows().join("every-kind.yaml"))
}

/// The test flow as it was written before stages said who does them: its agents listed, each stage naming one.
fn older_every_kind() -> String {
    file(&flows().join("older/every-kind.yaml"))
}

fn refused(text: &str) -> String {
    yaml::read(text).expect_err("refused").to_string()
}

#[test]
fn the_default_file_is_the_shipped_one_and_round_trips() {
    let on_disk = file(
        &Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("flows")
            .join(naming::DEFAULT_FLOW_FILE),
    );
    assert_eq!(on_disk, naming::DEFAULT_FLOW_YAML);
    let read = yaml::read(&on_disk).unwrap();
    assert_eq!(yaml::write(&read), on_disk);
    assert_eq!(&read, flow::default_file());
}

#[test]
fn the_test_flow_round_trips() {
    let text = every_kind();
    let read = yaml::read(&text).unwrap();
    assert_eq!(yaml::write(&read), text);
    let who = |id: &str| read.flow.stage(id).unwrap().who.clone();
    assert_eq!(
        who("product-spec").instructions.as_deref(),
        Some(
            "Ask one question at a time.\nWrite what was agreed into the ticket.\n\nNever write code."
        )
    );
    assert_eq!(
        who("security").instructions.as_deref(),
        Some("Never edit files.\nFail on unescaped SQL.")
    );
    assert_eq!(who("dev").model.as_deref(), Some("default"));
    assert_eq!(who("qa"), WhoDoesIt::default());
    assert_eq!(read.flow.delivery.who.model.as_deref(), Some("default"));
}

/// UX-73: an older file's `agents:` and the agents its stages and delivery name (`agent:`) read as each stage's own
/// model and instructions; written again it is the file in today's shape, which reads back the same.
#[test]
fn an_older_file_reads_as_each_stage_on_its_agents_model() {
    let older = older_every_kind();
    let read = yaml::read(&older).unwrap();
    assert_eq!(read, yaml::read(&every_kind()).unwrap());
    assert_eq!(
        yaml::upgraded(&older).as_deref(),
        Some(every_kind().as_str())
    );
    assert_eq!(yaml::upgraded(&every_kind()), None, "already today's shape");
    // An agent's model goes onto each stage naming it; two stages naming one agent each get their own copy.
    let shared = older.replace(
        "    agent: Security reviewer\n",
        "    agent: Rust developer\n",
    );
    let read = yaml::read(&shared).unwrap();
    let (dev, security) = (
        read.flow.stage("dev").unwrap(),
        read.flow.stage("security").unwrap(),
    );
    assert_eq!(dev.who, security.who);
    assert_eq!(
        security.who.instructions.as_deref(),
        Some("Follow our error-handling rules: no unwrap in library code.")
    );
    // The migration's own lookup comes first: the stored agent's model and effort, else the file's.
    let read = yaml::read_older(
        &older,
        &|name: &str| {
            (name == "Rust developer").then(|| WhoDoesIt {
                model: Some("codex:gpt-5.5".into()),
                effort: Some("high".into()),
                instructions: None,
            })
        },
        None,
    )
    .unwrap();
    assert_eq!(
        read.flow.stage("dev").unwrap().who,
        WhoDoesIt {
            model: Some("codex:gpt-5.5".into()),
            effort: Some("high".into()),
            instructions: None,
        }
    );
    assert_eq!(
        read.flow.stage("validate").unwrap().who.model.as_deref(),
        Some("default")
    );
}

/// An older file's agent `role:`, `skills:` and `why:` still read, are ignored, and are never written back.
#[test]
fn an_old_files_agent_role_skills_and_why_read_and_are_dropped() {
    let old = older_every_kind().replacen(
        "  - name: Quick developer\n    model: default\n",
        "  - name: Quick developer\n    role: noyan\n    skills: [work-ticket, commit]\n    model: default\n    \
         why: From team.yaml\n",
        1,
    );
    assert!(old.contains("role: noyan"));
    let read = yaml::read(&old).expect("an old file still opens");
    assert_eq!(yaml::write(&read), every_kind());
}

#[test]
fn the_default_flow_is_dev_validate_review_then_delivered() {
    let flow = flow::default_flow();
    let ids: Vec<&str> = flow.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["dev", "validate", "review"]);
    let [dev, validate, review] = &flow.stages[..] else {
        unreachable!()
    };
    assert_eq!(
        (dev.kind, dev.model(), dev.skill.as_deref()),
        (StageKind::Agent, Some("default"), Some("otaq:work-ticket"))
    );
    assert_eq!(
        (
            validate.kind,
            validate.model(),
            validate.skill.as_deref(),
            validate.rounds
        ),
        (
            StageKind::Check { by: CheckBy::Agent },
            Some("default"),
            Some("otaq:validate"),
            Some(3)
        )
    );
    assert!(review.is_human_check() && review.model().is_none() && review.fail_to.is_none());
    // Delivered on Review's Approve, by a local merge or a PR: a setting of the flow, not a stage.
    assert_eq!(
        flow.delivery,
        Delivery {
            mode: DeliveryMode::Click,
            delivers: Delivers::Either,
            who: WhoDoesIt::on("default"),
            skill: Some("otaq:open-pr".to_owned()),
        }
    );
    assert_eq!(flow.approve_stage().map(|s| s.id.as_str()), Some("review"));
    assert_eq!(flow.first_agent_stage().map(|s| s.id.as_str()), Some("dev"));
}

#[test]
fn the_default_and_the_test_flow_pass_the_checks() {
    assert_eq!(check(&flow::default_flow()), Ok(()));
    let every = yaml::read(&every_kind()).unwrap();
    assert_eq!(check(&every.flow), Ok(()));
    assert!(every.flow.stage("dev").unwrap().options.is_empty());
    assert!(every.flow.stage("product-spec").unwrap().can_skip());
    assert_eq!(every.flow.delivery.delivers, Delivers::Pr);
}

/// An older file still loads: its Delivery stage becomes the flow's `delivery:` setting and its `column:` is
/// ignored (every stage is its own column); written again, it is in today's form, and reads back the same.
/// A stage an agent runs may name no skill: Otaq's own skill for that stage runs (`otaq:work-ticket` for an Agent stage,
/// `otaq:validate` for an agent check), so the file passes the checks and is written back as it is.
#[test]
fn a_stage_with_no_skill_passes_and_runs_otaqs_own() {
    let text = file(&flows().join("no-skill.yaml"));
    let read = yaml::read(&text).unwrap();
    assert_eq!(yaml::write(&read), text);
    assert_eq!(check(&read.flow), Ok(()));
    let plan = read.flow.stage("plan").unwrap();
    assert_eq!(plan.skill, None);
    assert_eq!(plan.own_skill(), Some("otaq:work-ticket"));
    let check_stage = flow::default_flow();
    let validate = check_stage.stage("validate").unwrap();
    assert_eq!(validate.own_skill(), Some("otaq:validate"));
    assert_eq!(check_stage.stage("review").unwrap().own_skill(), None);
}

#[test]
fn an_old_file_with_a_delivery_stage_and_with_previous_loads() {
    let old = "flow:
  - id: dev
    name: Dev
    kind: agent
    column: own
    model: default
    skill: work-ticket
  - id: validate
    name: Validate
    kind: check
    by: agent
    column: with-previous
    model: default
    skill: validate
    rounds: 3
  - id: review
    name: Review
    kind: check
    by: you
    column: own
  - id: deliver
    name: Done
    kind: delivery
    column: with-previous
    model: default
    skill: open-pr
    mode: click
    delivers: either
";
    let read = yaml::read(old).unwrap();
    let ids: Vec<&str> = read.flow.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["dev", "validate", "review"]);
    // Its bare `open-pr` is Otaq's once renamed (tuq decision 075).
    let mut renamed = read.clone();
    assert!(flow::skill_names::name_own_skills(&mut renamed));
    assert_eq!(renamed.flow.delivery, flow::default_flow().delivery);
    let upgraded = yaml::upgraded(old).expect("an old form");
    assert!(!upgraded.contains("column") && !upgraded.contains("kind: delivery"));
    assert!(upgraded.contains("delivery:\n  delivers: either\n  mode: click\n"));
    assert_eq!(yaml::read(&upgraded).unwrap(), read);
    assert_eq!(yaml::upgraded(&upgraded), None, "already today's form");

    // A file with no delivery at all doesn't read; one with two doesn't either.
    let no_delivery = "flow:\n  - {name: Dev, kind: agent, model: default, skill: work-ticket}\n";
    assert!(refused(no_delivery).contains("doesn't say how its work is delivered"));
    let both = format!("{old}delivery:\n  delivers: local\n  mode: click\n");
    assert!(refused(&both).contains("Done is a Delivery stage and the flow has `delivery:` too"));
}

/// A problem as a test expects it: the rule, the stage's id and the message.
type Expected<'a> = (Rule, Option<&'a str>, &'a str);

/// Each broken file breaks one rule and says so.
#[test]
fn each_rule_refuses_its_broken_file() {
    let cases: &[(&str, &[Expected])] = &[
        (
            "no-agent-stage",
            &[(
                Rule::AgentStage,
                None,
                "The flow has no Agent stage: at least one stage needs an agent working on the ticket",
            )],
        ),
        (
            "human-check-first",
            &[(
                Rule::HumanCheckFirst,
                Some("review"),
                "Review is a check by you before any Agent stage: a Fail would have nowhere to go",
            )],
        ),
        (
            "either-automatic",
            &[(
                Rule::EitherClickOnly,
                None,
                "The delivery is `either` automatically: choosing between a Local merge and a PR needs your click, \
                 so use click mode or deliver `local` or `pr`",
            )],
        ),
        (
            "rounds",
            &[(
                Rule::Rounds,
                Some("validate"),
                "Validate has 7 rounds: use 1 to 5",
            )],
        ),
        (
            "skip-leaves-no-agent",
            &[(
                Rule::SkipLeavesNoAgent,
                None,
                "If Product spec and Dev are both skipped, no Agent stage runs",
            )],
        ),
        (
            "skip-strands-human-check",
            &[(
                Rule::SkipStrandsHumanCheck,
                Some("review"),
                "If Product spec is skipped, a Fail at Review has nowhere to go",
            )],
        ),
        (
            // Every check's fails go to an Agent stage before it (d0131): not a later stage, a check by you or an
            // agent check.
            "fail-to",
            &[
                (
                    Rule::FailTo,
                    Some("spec-review"),
                    "Spec review's fails go to Validate, which isn't an Agent stage before it",
                ),
                (
                    Rule::FailTo,
                    Some("validate"),
                    "Validate's fails go to Review, which isn't an Agent stage before it",
                ),
                (
                    Rule::FailTo,
                    Some("qa"),
                    "QA's fails go to Spec review, which isn't an Agent stage before it",
                ),
            ],
        ),
        (
            // The editor's draft holds an agent check with no model yet: a stage an agent works or checks has one.
            "no-model",
            &[(
                Rule::NoModel,
                Some("validate"),
                "Validate has no model: say who does it with `model:` (a tool, or `tool:model`)",
            )],
        ),
        (
            "duplicate",
            &[
                (
                    Rule::Duplicate,
                    Some("dev"),
                    "Two stages have the id `dev`: each stage needs its own",
                ),
                (
                    Rule::Duplicate,
                    Some("dev"),
                    "Two stages are named Dev: each stage needs its own name",
                ),
            ],
        ),
    ];
    let dir = flows().join("broken");
    // The files that say what they are refused with are refusals other than STG-02's (`flow_changes`).
    let on_disk = std::fs::read_dir(&dir)
        .unwrap()
        .filter(|e| !file(&e.as_ref().unwrap().path()).starts_with("# Refused: "))
        .count();
    assert_eq!(
        on_disk,
        cases.len(),
        "one case per file in {}",
        dir.display()
    );
    for (name, expected) in cases {
        let text = file(&dir.join(format!("{name}.yaml")));
        let read = yaml::read(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        // Broken files are still written in Otaq's layout.
        assert_eq!(yaml::write(&read), text, "{name}");
        let problems = check(&read.flow).expect_err(name);
        let got: Vec<Expected> = problems
            .iter()
            .map(|p| (p.rule, p.stage.as_deref(), p.message.as_str()))
            .collect();
        assert_eq!(&got, expected, "{name}");
    }
}

#[test]
fn every_skip_combination_is_walked() {
    // Three Agent stages, all skippable: skipping all three leaves none.
    let text = "flow:
  - {name: Product spec, kind: agent, model: claude, skill: product-spec, options: [skip]}
  - {name: Plan, kind: agent, model: claude, skill: plan, options: [skip]}
  - {name: Dev, kind: agent, model: claude, skill: work-ticket, options: [skip]}
  - {name: Done, kind: delivery, mode: click, delivers: local}
";
    let problems = check(&yaml::read(text).unwrap().flow).unwrap_err();
    assert_eq!(
        problems
            .iter()
            .map(|p| p.message.as_str())
            .collect::<Vec<_>>(),
        ["If Product spec, Plan and Dev are all skipped, no Agent stage runs"]
    );

    // A skippable human check after only skippable Agent stages is allowed (0129 row 4: a ticket that skips them
    // skips it too); a required one there isn't.
    let text = "flow:
  - {name: Product spec, kind: agent, model: claude, skill: product-spec, options: [skip]}
  - {name: Spec review, kind: check, by: you, options: [skip]}
  - {name: Dev, kind: agent, model: claude, skill: work-ticket}
  - {name: Code review, kind: check, by: you}
  - {name: Done, kind: delivery, mode: click, delivers: local}
";
    assert_eq!(check(&yaml::read(text).unwrap().flow), Ok(()));
    let required = text.replace("by: you, options: [skip]", "by: you");
    let problems = check(&yaml::read(&required).unwrap().flow).unwrap_err();
    assert_eq!(
        problems
            .iter()
            .map(|p| (p.rule, p.message.as_str()))
            .collect::<Vec<_>>(),
        [(
            Rule::SkipStrandsHumanCheck,
            "If Product spec is skipped, a Fail at Spec review has nowhere to go"
        )]
    );
}

#[test]
fn an_old_with_previous_stage_is_its_own_column_and_passes() {
    // Polish once folded into Review's column, which was refused; every stage is its own column now, so it passes.
    let text = "flow:
  - {name: Dev, kind: agent, model: claude, skill: work-ticket}
  - {name: Review, kind: check, by: you}
  - {name: Polish, kind: check, by: agent, column: with-previous, model: claude, skill: validate, rounds: 1}
  - {name: Done, kind: delivery, column: with-previous, mode: click, delivers: local}
";
    let read = yaml::read(text).unwrap();
    assert_eq!(check(&read.flow), Ok(()));
    assert_eq!(read.flow.stages.len(), 3);
}

#[test]
fn the_format_refuses_what_isnt_a_flow() {
    assert_eq!(
        refused("flow:\n  - id: dev\n    name: Dev\n    colour: blue\n"),
        "line 4 column 5: unknown field `colour`, expected one of id, name, kind, by, column, model, effort, \
         instructions, agent, agents, skill, options, rounds, fail_to, mode, delivers, description, why"
    );
    assert_eq!(
        refused("flows: []\n"),
        "line 1 column 1: unknown field `flows`, expected one of agents, flow, delivery, why"
    );

    let cases = [
        (
            "{name: Dev, kind: agent, by: you, model: claude, skill: s}",
            "Dev is an Agent stage: `by` belongs to a check",
        ),
        (
            "{name: Check, kind: check, model: claude, skill: s, rounds: 3}",
            "Check is a check: say who does it with `by: agent` or `by: you`",
        ),
        (
            "{name: Validate, kind: check, by: agent, agents: [V], skill: validate}",
            "Validate is an agent check: set its `rounds` (1 to 5)",
        ),
        (
            "{name: Dev, kind: agent, model: gpt-9, skill: work-ticket}",
            "Dev's model: gpt-9 isn't a model: say a tool (claude, codex, copilot, opencode) or `tool:model` \
             (`claude:opus`)",
        ),
        (
            "{name: Dev, kind: agent, effort: high, skill: work-ticket}",
            "Dev has an effort but no model: say which model it runs with `model:`",
        ),
        (
            "{name: Dev, kind: agent, model: opencode, effort: high, skill: work-ticket}",
            "Dev's effort: OpenCode has no effort level",
        ),
        (
            "{name: Dev, kind: agent, agent: Developer, model: claude, skill: work-ticket}",
            "Dev names the agent Developer and its own model: say who does it once, with `model:`",
        ),
        (
            "{name: Dev, kind: agent, agent: Developer, skill: work-ticket}",
            "Dev names the agent Developer, which the file doesn't define: say who does it with `model:`, and \
             `effort:` and `instructions:` if it has them",
        ),
        (
            "{name: Dev, kind: agent, model: claude, skill: s, rounds: 2}",
            "Dev is an Agent stage: `rounds` belongs to an agent check",
        ),
        (
            // A check has one agent: named both ways is refused.
            "{name: Validate, kind: check, by: agent, model: claude, agents: [Validator], skill: validate, \
             rounds: 3}",
            "Validate names the agent Validator and its own model: say who does it once, with `model:`",
        ),
        (
            "{name: Dev, kind: agent, model: claude, agents: [D], skill: s}",
            "Dev is an Agent stage: `agents` belongs to an agent check",
        ),
        (
            "{name: Review, kind: check, by: you, agents: [Validator]}",
            "Review is a check by you: `agents` belongs to an agent check",
        ),
        (
            "{name: Dev, kind: agent, model: claude, skill: s, fail_to: Dev}",
            "Dev is an Agent stage: `fail_to` belongs to a check",
        ),
        (
            "{name: Done, kind: delivery, mode: click, delivers: local, fail_to: Dev}",
            "Done is a Delivery stage: `fail_to` belongs to a check",
        ),
        (
            "{name: Review, kind: check, by: you, fail_to: Planning}",
            "Review's fails go to Planning, which isn't a stage of this flow: name an Agent stage before it with \
             `fail_to:`, or leave it out for the nearest one",
        ),
        (
            "{name: Review, kind: check, by: you, model: claude}",
            "Review is a check by you: `model` belongs to a stage an agent works or checks",
        ),
        (
            "{name: Review, kind: check, by: you, instructions: Look closely}",
            "Review is a check by you: `instructions` belongs to a stage an agent works or checks",
        ),
        (
            "{name: Review, kind: check, by: you, mode: click}",
            "Review is a check by you: `mode` belongs to the flow's `delivery:`",
        ),
        (
            "{name: Done, kind: delivery, mode: click}",
            "Done needs its `mode` (click or automatic) and what it `delivers` (local, pr or \
             either)",
        ),
        (
            "{name: Done, kind: delivery, mode: click, delivers: local, skill: open-pr}",
            "Done is a Local merge, which opens no PR: `skill` belongs to a delivery that opens a PR",
        ),
        (
            "{name: Done, kind: delivery, mode: click, delivers: pr}",
            "Done can open a PR: say which model writes and opens it with `model:`",
        ),
        (
            "{name: ' ', kind: agent, model: claude, skill: s}",
            "A stage has no name",
        ),
    ];
    for (stage, expected) in cases {
        assert_eq!(
            refused(&format!("flow:\n  - {stage}\n")),
            expected,
            "{stage}"
        );
    }
    // An effort is one of its tool's levels (UX-75).
    let levels = Tool::Claude.effort_levels().join(", ");
    assert_eq!(
        refused("flow:\n  - {name: Dev, kind: agent, model: claude:opus, effort: ludicrous}\n"),
        format!("Dev's effort: ludicrous isn't one of Claude Code's effort levels: {levels}")
    );
    // The delivery's: an older one names an agent its file defines, a PR-capable one names a model.
    assert_eq!(
        refused(
            "flow:\n  - {name: Dev, kind: agent, model: claude}\ndelivery: {delivers: pr, mode: click, agent: Shipper}\n"
        ),
        "The delivery names the agent Shipper, which the file doesn't define: say who does it with `model:`, and \
         `effort:` and `instructions:` if it has them"
    );
}

/// A stage lists no agents: the agent names an older file lists in `options` are read and ignored, so `options` holds
/// only `skip`, and the file writes back without them.
#[test]
fn the_agents_in_options_are_read_and_ignored() {
    let text = "flow:
  - {name: Dev, kind: agent, model: claude, skill: work-ticket, options: [Quick developer]}
  - {name: Review, kind: check, by: you, options: [Quick developer, skip]}
  - {name: Done, kind: delivery, mode: click, delivers: local}
";
    let read = yaml::read(text).unwrap();
    assert!(read.flow.stages[0].options.is_empty());
    assert_eq!(
        read.flow.stages[1].options,
        [otaq_core::flow::StageOption::Skip]
    );
    let written = yaml::write(&read);
    assert!(!written.contains("Quick developer"), "{written}");
}

#[test]
fn ids_are_kept_as_written_and_given_when_left_out() {
    let text = "flow:
  - {id: build, name: Dev, kind: agent, model: claude, skill: work-ticket}
  - {name: Code review, kind: check, by: you}
  - {name: Code review!, kind: check, by: you}
  - {name: Done, kind: delivery, mode: click, delivers: local}
";
    let read = yaml::read(text).unwrap();
    let ids: Vec<&str> = read.flow.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["build", "code-review", "code-review-2"]);
    // Written back, the ids are in the file and read back the same.
    let written = yaml::write(&read);
    assert_eq!(yaml::read(&written).unwrap(), read);
    assert_eq!(yaml::write(&yaml::read(&written).unwrap()), written);
}

/// An agent check's `agent` (one, like an Agent stage's) and a check's `fail_to` (d0131) read and write back in their
/// place in the field order (`fail_to` after `rounds`); `fail_to` is read by name and kept as the stage's id.
#[test]
fn a_check_agent_and_fails_go_to_round_trip() {
    let text = "flow:
  - id: plan
    name: Product spec
    kind: agent
    model: claude
    skill: work-ticket
  - id: dev
    name: Dev
    kind: agent
    model: claude
    skill: work-ticket
  - id: checks
    name: Checks
    kind: check
    by: agent
    model: claude
    skill: validate
    options: [skip]
    rounds: 2
    fail_to: Product spec
  - id: review
    name: Review
    kind: check
    by: you
    fail_to: dev
delivery:
  delivers: local
  mode: click
";
    let read = yaml::read(text).unwrap();
    let checks = read.flow.stage("checks").unwrap();
    assert_eq!(checks.model(), Some("claude"));
    assert_eq!(checks.fail_to.as_deref(), Some("plan"), "kept by id");
    // A name is matched ignoring case when no stage has it exactly; written back as the stage's own name.
    assert_eq!(
        read.flow.stage("review").unwrap().fail_to.as_deref(),
        Some("dev")
    );
    assert_eq!(
        yaml::write(&read),
        text.replace("fail_to: dev", "fail_to: Dev")
    );
    assert_eq!(check(&read.flow), Ok(()));

    // The test flow: Security has its own instructions, QA's fails go to Product spec rather than the nearest (Dev).
    let every = yaml::read(&every_kind()).unwrap();
    let security = every.flow.stage("security").unwrap();
    assert!(security.who.instructions.is_some());
    let qa = every.flow.stage("qa").unwrap();
    assert_eq!(
        every.flow.fail_to_of(qa).map(|s| s.id.as_str()),
        Some("product-spec")
    );
}

/// A check has one model: an older file's list form with one agent, `agents: [Validator]`, still reads as `agent:
/// Validator`, so as the check's own model; naming it both ways is refused.
#[test]
fn a_check_reads_its_one_agent_from_the_old_list_form() {
    let old = older_every_kind().replace("agent: Validator\n", "agents: [Validator]\n");
    assert!(old.contains("agents: [Validator]"));
    let read = yaml::read(&old).unwrap();
    assert_eq!(read, yaml::read(&every_kind()).unwrap());
    let written = yaml::write(&read);
    assert!(!written.contains("agents"), "{written}");

    let both = older_every_kind().replace(
        "agent: Validator\n",
        "agent: Validator\n    agents: [Validator]\n",
    );
    assert_eq!(
        yaml::read(&both).unwrap_err().to_string(),
        "Validate is an agent check: say who does it once, with `model:`"
    );
}

/// An older flow whose check lists several agents (`tests/flows/several-check-agents.yaml`: Security runs Security
/// reviewer and Validator) reads as one check stage per agent, in order: the first keeps the id and name, the other
/// is `security-validator`, "Security · Validator", its own column, with its skill, rounds and fails go to.
/// Every read gives the same split; the document written from it holds the split and reads back the same, with no
/// split left. A name or id the flow already has gets " 2" or "-2".
#[test]
fn an_older_check_with_several_agents_reads_as_one_check_per_agent() {
    let several = file(&flows().join("older/several-check-agents.yaml"));
    let (read, splits) = yaml::read_split(&several).unwrap();
    // Each copy runs on its own agent's model and instructions (only Security reviewer has instructions).
    let row = |s: &otaq_core::flow::Stage| {
        (
            s.id.clone(),
            s.name.clone(),
            s.who.instructions.clone().unwrap_or_default(),
        )
    };
    let security: Vec<_> = read
        .flow
        .stages
        .iter()
        .filter(|s| s.id.starts_with("security"))
        .map(row)
        .collect();
    assert_eq!(
        security,
        [
            (
                "security".to_owned(),
                "Security".to_owned(),
                "Never edit files.\nFail on unescaped SQL.".to_owned()
            ),
            (
                "security-validator".to_owned(),
                "Security · Validator".to_owned(),
                String::new()
            ),
        ]
    );
    let ids: Vec<&str> = read.flow.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "product-spec",
            "dev",
            "validate",
            "security",
            "security-validator",
            "qa"
        ]
    );
    assert_eq!(
        splits,
        [yaml::Split {
            stage: "security".into(),
            copies: vec!["security-validator".into()],
        }]
    );
    let (first, copy) = (
        read.flow.stage("security").unwrap(),
        read.flow.stage("security-validator").unwrap(),
    );
    assert!(copy.is_agent_check());
    assert_eq!(
        (
            &copy.skill,
            copy.rounds,
            &copy.fail_to,
            &copy.options,
            &copy.description
        ),
        (
            &first.skill,
            first.rounds,
            &first.fail_to,
            &first.options,
            &first.description
        )
    );
    assert_eq!(
        read.flow.fail_to_of(copy).map(|s| s.id.as_str()),
        Some("dev")
    );
    assert_eq!(check(&read.flow), Ok(()));
    assert_eq!(yaml::read(&several).unwrap(), read, "every read the same");
    let written = yaml::write(&read);
    assert!(!written.contains("agents"), "{written}");
    let (again, none) = yaml::read_split(&written).unwrap();
    assert_eq!(again, read);
    assert!(none.is_empty());
    // A proposal matches the copy by its id when the stage's was written, by its name when it wasn't.
    let (_, ids_written) = yaml::read_with_written_ids(&several).unwrap();
    assert!(ids_written[4]);

    // Taken: another stage is already named "Security · Validator" with the id `security-validator`.
    let taken = several.replace(
        "  - id: qa\n    name: QA\n",
        "  - id: security-validator\n    name: Security · Validator\n",
    );
    let read = yaml::read(&taken).unwrap();
    let copy = &read.flow.stages[4];
    assert_eq!(
        (copy.id.as_str(), copy.name.as_str()),
        ("security-validator-2", "Security · Validator 2")
    );
}

/// d0131: a check that sets nothing sends its fails to the nearest Agent stage before it; one that names a stage
/// sends them there, and one whose stage is gone is the nearest again.
#[test]
fn fails_go_to_the_named_stage_else_the_nearest() {
    let every = yaml::read(&every_kind()).unwrap();
    let flow = &every.flow;
    let target = |id: &str| {
        flow.fail_to_of(flow.stage(id).unwrap())
            .map(|s| s.id.clone())
    };
    assert_eq!(target("validate").as_deref(), Some("dev"));
    assert_eq!(target("security").as_deref(), Some("dev"));
    assert_eq!(target("qa").as_deref(), Some("product-spec"));
    let mut gone = flow.clone();
    gone.stages.retain(|s| s.id != "product-spec");
    let qa = gone.stage("qa").unwrap();
    assert_eq!(
        qa.fail_to.as_deref(),
        Some("product-spec"),
        "still set in memory"
    );
    assert_eq!(gone.fail_to_of(qa).map(|s| s.id.as_str()), Some("dev"));
    assert_eq!(
        check(&gone),
        Ok(()),
        "a stage gone is the nearest again, no problem"
    );
    // Written without it: the file says nearest.
    let written = yaml::write(&flow::FlowFile { flow: gone });
    assert!(!written.contains("fail_to"), "{written}");
}

/// UX-73: an older file whose stage names an agent the file doesn't define is refused (`tests/flows/broken/
/// unknown-agent.yaml`), naming the stage and the agent.
#[test]
fn an_older_stage_naming_an_undefined_agent_is_refused() {
    let text = file(&flows().join("broken/unknown-agent.yaml"));
    assert_eq!(
        refused(&text),
        "Dev names the agent Rust developer, which the file doesn't define: say who does it with `model:`, and \
         `effort:` and `instructions:` if it has them"
    );
}

/// Every stage an agent works or checks in Otaq's shipped flows runs on `default` (UX-68): the tool of the user's
/// default model once applied.
#[test]
fn the_shipped_flows_run_on_the_default_model() {
    for preset in flow::presets::presets() {
        let read = yaml::read(preset.yaml).unwrap();
        for stage in read.flow.stages.iter().filter(|s| s.runs_agent()) {
            assert_eq!(
                stage.model(),
                Some("default"),
                "{}: {}",
                preset.file,
                stage.name
            );
        }
        assert_eq!(yaml::write(&read), preset.yaml, "{}", preset.file);
    }
}
