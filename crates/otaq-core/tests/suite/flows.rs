//! The flow format (STG-01, STG-02, STG-03, STG-12; d0108): the default file and the test flow read and write back
//! unchanged, and each STG-02 rule refuses its broken file in `tests/flows/broken/` with its message. A project's
//! stored flow is `flow_store`'s.

use std::path::{Path, PathBuf};

use otaq_core::flow::{
    self, CheckBy, Delivers, Delivery, DeliveryMode, Rule, StageKind, check, yaml,
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
    assert_eq!(read.agents.len(), 7);
    assert_eq!(
        read.agents[0].instructions,
        "Ask one question at a time.\nWrite what was agreed into the ticket.\n\nNever write code.\n"
    );
    assert_eq!(
        read.agents[4].instructions,
        "Never edit files.\nFail on unescaped SQL."
    );
    assert_eq!(read.agents[6].name, "Noyan");
}

/// An agent has no role: an old file's `role: noyan` still reads, is ignored, and is never written back.
#[test]
fn an_old_files_agent_role_reads_and_is_dropped() {
    let old = "agents:\n  - name: Noyan\n    role: noyan\n    model: default\n";
    let read = yaml::read_agents(old).unwrap();
    assert_eq!(read[0].name, "Noyan");
    assert_eq!(
        yaml::write_agents(&read),
        "agents:\n  - name: Noyan\n    model: default\n"
    );
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
        (dev.kind, dev.agent.as_deref(), dev.skill.as_deref()),
        (StageKind::Agent, Some("Developer"), Some("work-ticket"))
    );
    assert_eq!(
        (
            validate.kind,
            validate.agent.as_deref(),
            validate.skill.as_deref(),
            validate.rounds
        ),
        (
            StageKind::Check { by: CheckBy::Agent },
            Some("Checker"),
            Some("validate"),
            Some(3)
        )
    );
    assert!(review.is_human_check() && !review.has_agent() && review.fail_to.is_none());
    // Delivered on Review's Approve, by a local merge or a PR: a setting of the flow, not a stage.
    assert_eq!(
        flow.delivery,
        Delivery {
            mode: DeliveryMode::Click,
            delivers: Delivers::Either,
            agent: Some("Developer".to_owned()),
            skill: Some("open-pr".to_owned()),
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
#[test]
fn an_old_file_with_a_delivery_stage_and_with_previous_loads() {
    let old = "flow:
  - id: dev
    name: Dev
    kind: agent
    column: own
    agent: Developer
    skill: work-ticket
  - id: validate
    name: Validate
    kind: check
    by: agent
    column: with-previous
    agent: Validator
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
    agent: Developer
    skill: open-pr
    mode: click
    delivers: either
";
    let read = yaml::read(old).unwrap();
    let ids: Vec<&str> = read.flow.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["dev", "validate", "review"]);
    assert_eq!(read.flow.delivery, flow::default_flow().delivery);
    let upgraded = yaml::upgraded(old).expect("an old form");
    assert!(!upgraded.contains("column") && !upgraded.contains("kind: delivery"));
    assert!(upgraded.contains("delivery:\n  delivers: either\n  mode: click\n"));
    assert_eq!(yaml::read(&upgraded).unwrap(), read);
    assert_eq!(yaml::upgraded(&upgraded), None, "already today's form");

    // A file with no delivery at all doesn't read; one with two doesn't either.
    let no_delivery = "flow:\n  - {name: Dev, kind: agent, agent: Developer, skill: work-ticket}\n";
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
            // The editor's draft holds a new Agent stage with no skill yet; Apply waits for one (FED-06).
            "needs-skill",
            &[(Rule::NeedsSkill, Some("plan"), "Plan needs a skill")],
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
            // The editor's draft holds an agent check with no agent yet: a check has one.
            "no-check-agent",
            &[(
                Rule::NoCheckAgent,
                Some("validate"),
                "Validate is an agent check with no agent: name one",
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
  - {name: Product spec, kind: agent, agent: Spec writer, skill: product-spec, options: [skip]}
  - {name: Plan, kind: agent, agent: Noyan, skill: plan, options: [skip]}
  - {name: Dev, kind: agent, agent: Developer, skill: work-ticket, options: [skip]}
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
  - {name: Product spec, kind: agent, agent: Spec writer, skill: product-spec, options: [skip]}
  - {name: Spec review, kind: check, by: you, options: [skip]}
  - {name: Dev, kind: agent, agent: Developer, skill: work-ticket}
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
  - {name: Dev, kind: agent, agent: Developer, skill: work-ticket}
  - {name: Review, kind: check, by: you}
  - {name: Polish, kind: check, by: agent, column: with-previous, agent: Validator, skill: validate, rounds: 1}
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
        "line 4 column 5: unknown field `colour`, expected one of id, name, kind, by, column, agent, agents, \
         skill, options, rounds, fail_to, mode, delivers, description, why"
    );
    assert_eq!(
        refused("flows: []\n"),
        "line 1 column 1: unknown field `flows`, expected one of agents, flow, delivery"
    );

    let cases = [
        (
            "{name: Dev, kind: agent, by: you, agent: D, skill: s}",
            "Dev is an Agent stage: `by` belongs to a check",
        ),
        (
            "{name: Check, kind: check, agent: V, skill: s, rounds: 3}",
            "Check is a check: say who does it with `by: agent` or `by: you`",
        ),
        (
            "{name: Validate, kind: check, by: agent, agents: [V], skill: validate}",
            "Validate is an agent check: set its `rounds` (1 to 5)",
        ),
        (
            "{name: Dev, kind: agent, skill: work-ticket}",
            "Dev is an Agent stage: name the agent that runs it with `agent:`",
        ),
        (
            "{name: Dev, kind: agent, agent: D, skill: s, rounds: 2}",
            "Dev is an Agent stage: `rounds` belongs to an agent check",
        ),
        (
            // A check has one agent: named both ways is refused.
            "{name: Validate, kind: check, by: agent, agent: Validator, agents: [Validator], skill: validate, \
             rounds: 3}",
            "Validate is an agent check: name its agent once, with `agent:`",
        ),
        (
            "{name: Dev, kind: agent, agent: D, agents: [D], skill: s}",
            "Dev is an Agent stage: `agents` belongs to an agent check",
        ),
        (
            "{name: Review, kind: check, by: you, agents: [Validator]}",
            "Review is a check by you: `agents` belongs to an agent check",
        ),
        (
            "{name: Dev, kind: agent, agent: D, skill: s, fail_to: Dev}",
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
            "{name: Review, kind: check, by: you, agent: Developer}",
            "Review is a check by you: `agent` belongs to a stage an agent runs",
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
            "{name: Done, kind: delivery, mode: click, delivers: local, agent: Developer}",
            "Done is a Local merge, which runs no agent: `agent` belongs to a delivery that opens a PR",
        ),
        (
            "{name: Done, kind: delivery, mode: click, delivers: pr}",
            "Done can open a PR: name the agent that opens it with `agent:`",
        ),
        (
            "{name: ' ', kind: agent, agent: D, skill: s}",
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
}

/// A stage lists no agents: the agent names an older file lists in `options` are read and ignored, so `options` holds
/// only `skip`, and the file writes back without them.
#[test]
fn the_agents_in_options_are_read_and_ignored() {
    let text = "flow:
  - {name: Dev, kind: agent, agent: Developer, skill: work-ticket, options: [Quick developer]}
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
  - {id: build, name: Dev, kind: agent, agent: Developer, skill: work-ticket}
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
    agent: Developer
    skill: work-ticket
  - id: dev
    name: Dev
    kind: agent
    agent: Developer
    skill: work-ticket
  - id: checks
    name: Checks
    kind: check
    by: agent
    agent: Security reviewer
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
    assert_eq!(checks.agent.as_deref(), Some("Security reviewer"));
    assert_eq!(checks.check_agent(), Some("Security reviewer"));
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

    // The test flow: Security has its own agent, QA's fails go to Product spec rather than the nearest (Dev).
    let every = yaml::read(&every_kind()).unwrap();
    let security = every.flow.stage("security").unwrap();
    assert_eq!(security.check_agent(), Some("Security reviewer"));
    let qa = every.flow.stage("qa").unwrap();
    assert_eq!(
        every.flow.fail_to_of(qa).map(|s| s.id.as_str()),
        Some("product-spec")
    );
}

/// A check has one agent: an older file's list form with one name, `agents: [Validator]`, still reads as `agent:
/// Validator` and is written back as `agent:`; naming it both ways is refused.
#[test]
fn a_check_reads_its_one_agent_from_the_old_list_form() {
    let old = every_kind().replace("agent: Validator\n", "agents: [Validator]\n");
    assert!(old.contains("agents: [Validator]"));
    let read = yaml::read(&old).unwrap();
    let validate = read.flow.stage("validate").unwrap();
    assert_eq!(validate.agent.as_deref(), Some("Validator"));
    let written = yaml::write(&read);
    assert!(!written.contains("agents: ["), "{written}");
    assert_eq!(yaml::read(&written).unwrap(), read);

    let both = every_kind().replace(
        "agent: Validator\n",
        "agent: Validator\n    agents: [Validator]\n",
    );
    assert_eq!(
        yaml::read(&both).unwrap_err().to_string(),
        "Validate is an agent check: name its agent once, with `agent:`"
    );
}

/// An older flow whose check lists several agents (`tests/flows/several-check-agents.yaml`: Security runs Security
/// reviewer and Validator) reads as one check stage per agent, in order: the first keeps the id and name, the other
/// is `security-validator`, "Security · Validator", its own column, with its skill, rounds and fails go to.
/// Every read gives the same split; the document written from it holds the split and reads back the same, with no
/// split left. A name or id the flow already has gets " 2" or "-2".
#[test]
fn an_older_check_with_several_agents_reads_as_one_check_per_agent() {
    let several = file(&flows().join("several-check-agents.yaml"));
    let (read, splits) = yaml::read_split(&several).unwrap();
    let row = |s: &otaq_core::flow::Stage| {
        (
            s.id.clone(),
            s.name.clone(),
            s.agent.clone().unwrap_or_default(),
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
                "Security reviewer".to_owned()
            ),
            (
                "security-validator".to_owned(),
                "Security · Validator".to_owned(),
                "Validator".to_owned()
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
    assert!(!written.contains("agents: ["), "{written}");
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
    let written = yaml::write(&flow::FlowFile {
        agents: Vec::new(),
        flow: gone,
    });
    assert!(!written.contains("fail_to"), "{written}");
}

/// An agent check's one agent is checked against the agents (AGM-01), as an Agent stage's is.
#[test]
fn every_check_agent_must_be_an_agent() {
    let every = yaml::read(&every_kind()).unwrap();
    let known = |name: &str| name != "Validator";
    let problems = flow::check_agents(&every.flow, known).unwrap_err();
    let got: Vec<(Rule, Option<&str>, &str)> = problems
        .iter()
        .map(|p| (p.rule, p.stage.as_deref(), p.message.as_str()))
        .collect();
    assert_eq!(
        got,
        [(
            Rule::UnknownAgent,
            Some("validate"),
            "Validate names the agent Validator, which isn't one of your agents"
        ),]
    );
}

/// An agent has no skills (T6): an old file's agent `skills:` still reads, so the file opens, and is ignored: writing
/// the file back leaves it out.
#[test]
fn an_old_files_agent_skills_read_and_are_never_written() {
    let old = naming::DEFAULT_FLOW_YAML.replacen(
        "    model: default\n",
        "    skills: [work-ticket, commit, /Users/me/team/my-commit]\n    model: default\n",
        1,
    );
    assert_ne!(old, naming::DEFAULT_FLOW_YAML);
    let read = otaq_core::flow::yaml::read(&old).expect("an old file still opens");
    let written = otaq_core::flow::yaml::write(&read);
    assert!(!written.contains("skills:"), "{written}");
    assert_eq!(written, naming::DEFAULT_FLOW_YAML);
}
