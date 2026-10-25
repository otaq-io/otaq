//! The flow format (STG-01, STG-02, STG-03, STG-12; d0108): the default file and the test flow read and write back
//! unchanged, and each STG-02 rule refuses its broken file in `tests/flows/broken/` with its message. A project's
//! stored flow is `flow_store`'s.

use std::path::{Path, PathBuf};

use otaq_core::flow::{
    self, CheckBy, Column, Delivers, Delivery, DeliveryMode, Rule, StageKind, StageOption, check,
    yaml,
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
    assert_eq!(read.agents[6].role.as_deref(), Some("noyan"));
}

#[test]
fn the_default_flow_is_dev_validate_review_done() {
    let flow = flow::default_flow();
    let ids: Vec<&str> = flow.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["dev", "validate", "review", "deliver"]);
    let [dev, validate, review, deliver] = &flow.stages[..] else {
        unreachable!()
    };
    assert_eq!(
        (
            dev.kind,
            dev.column,
            dev.agent.as_deref(),
            dev.skill.as_deref()
        ),
        (
            StageKind::Agent,
            Column::Own,
            Some("Developer"),
            Some("work-ticket")
        )
    );
    assert_eq!(
        (
            validate.kind,
            validate.column,
            validate.agent.as_deref(),
            validate.check_agents(),
            validate.skill.as_deref(),
            validate.rounds
        ),
        (
            StageKind::Check { by: CheckBy::Agent },
            Column::Own,
            None,
            &["Validator".to_owned()][..],
            Some("validate"),
            Some(3)
        )
    );
    assert!(
        review.is_human_check()
            && !review.has_agent()
            && review.fail_to.is_none()
            && review.column == Column::Own
    );
    assert_eq!(
        (
            deliver.name.as_str(),
            deliver.column,
            deliver.agent.as_deref(),
            deliver.delivery
        ),
        (
            "Done",
            Column::WithPrevious,
            Some("Developer"),
            Some(Delivery {
                mode: DeliveryMode::Click,
                delivers: Delivers::Either
            })
        )
    );
    // Validate has its own column (FLW-01), Done shows in Review's (its Approved section).
    assert_eq!(flow.column_owner(1), Some(1));
    assert_eq!(flow.column_owner(3), Some(2));
    assert_eq!(flow.first_agent_stage().map(|s| s.id.as_str()), Some("dev"));
}

#[test]
fn the_default_and_the_test_flow_pass_the_checks() {
    assert_eq!(check(&flow::default_flow()), Ok(()));
    let every = yaml::read(&every_kind()).unwrap();
    assert_eq!(check(&every.flow), Ok(()));
    let dev = every.flow.stage("dev").unwrap();
    assert_eq!(
        dev.options,
        [StageOption::Agent("Quick developer".to_owned())]
    );
    assert!(every.flow.stage("grill").unwrap().can_skip());
    // Validate shows in Dev's column; Security has its own.
    assert_eq!(every.flow.column_owner(2), Some(1));
    assert_eq!(every.flow.column_owner(3), Some(3));
}

/// A problem as a test expects it: the rule, the stage's id and the message.
type Expected<'a> = (Rule, Option<&'a str>, &'a str);

/// Each broken file breaks one rule (two-deliveries breaks both halves of the first) and says so.
#[test]
fn each_rule_refuses_its_broken_file() {
    let cases: &[(&str, &[Expected])] = &[
        (
            "no-delivery",
            &[(
                Rule::OneDelivery,
                None,
                "The flow has no Delivery stage: it must end in one that merges or opens a PR",
            )],
        ),
        (
            "two-deliveries",
            &[
                (
                    Rule::OneDelivery,
                    Some("deliver"),
                    "Merge and Done are both Delivery stages: a flow has exactly one",
                ),
                (
                    Rule::DeliveryLast,
                    Some("merge"),
                    "Merge is the Delivery stage but isn't last: the flow ends with it",
                ),
            ],
        ),
        (
            "delivery-not-last",
            &[(
                Rule::DeliveryLast,
                Some("deliver"),
                "Done is the Delivery stage but isn't last: the flow ends with it",
            )],
        ),
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
            "fold-first",
            &[(
                Rule::FoldFirst,
                Some("dev"),
                "Dev is the first stage: it can't share the previous column, since Ready for Dev comes before it",
            )],
        ),
        (
            "fold-into-human-check",
            &[(
                Rule::FoldIntoHumanCheck,
                Some("notes"),
                "Release notes can't share Review's column: that column belongs to a check by you",
            )],
        ),
        (
            "either-automatic",
            &[(
                Rule::EitherClickOnly,
                Some("deliver"),
                "Done delivers `either` automatically: choosing between a Local merge and a PR needs your click, \
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
            "skip-delivery",
            &[(
                Rule::SkipDelivery,
                Some("deliver"),
                "Done is the Delivery stage: it can't be skipped",
            )],
        ),
        (
            "skip-leaves-no-agent",
            &[(
                Rule::SkipLeavesNoAgent,
                None,
                "If Grill and Dev are both skipped, no Agent stage runs",
            )],
        ),
        (
            "skip-strands-human-check",
            &[(
                Rule::SkipStrandsHumanCheck,
                Some("review"),
                "If Grill is skipped, a Fail at Review has nowhere to go",
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
            // The editor's draft holds an agent check with no check agent yet (d0130: one or more).
            "no-check-agent",
            &[(
                Rule::NoCheckAgent,
                Some("validate"),
                "Validate is an agent check with no check agent: name one or more",
            )],
        ),
        (
            "check-agent-twice",
            &[(
                Rule::Duplicate,
                Some("validate"),
                "Validate lists validator twice: each check agent runs once",
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
  - {name: Grill, kind: agent, agent: Griller, skill: grill, options: [skip]}
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
        ["If Grill, Plan and Dev are all skipped, no Agent stage runs"]
    );

    // A skippable human check still needs a running Agent stage for the times it isn't skipped.
    let text = "flow:
  - {name: Grill, kind: agent, agent: Griller, skill: grill, options: [skip]}
  - {name: Look, kind: check, by: you, options: [skip]}
  - {name: Dev, kind: agent, agent: Developer, skill: work-ticket}
  - {name: Review, kind: check, by: you}
  - {name: Done, kind: delivery, mode: click, delivers: local}
";
    let problems = check(&yaml::read(text).unwrap().flow).unwrap_err();
    assert_eq!(
        problems
            .iter()
            .map(|p| (p.rule, p.message.as_str()))
            .collect::<Vec<_>>(),
        [(
            Rule::SkipStrandsHumanCheck,
            "If Grill is skipped, a Fail at Look has nowhere to go"
        )]
    );
}

#[test]
fn folding_follows_the_chain_to_its_owner() {
    // Polish folds into Review through nothing in between: refused; the Delivery after it may fold (as the default
    // flow's Done does).
    let text = "flow:
  - {name: Dev, kind: agent, agent: Developer, skill: work-ticket}
  - {name: Review, kind: check, by: you}
  - {name: Polish, kind: check, by: agent, column: with-previous, agents: [Validator], skill: validate, rounds: 1}
  - {name: Done, kind: delivery, column: with-previous, mode: click, delivers: local}
";
    let problems = check(&yaml::read(text).unwrap().flow).unwrap_err();
    assert_eq!(
        problems
            .iter()
            .map(|p| (p.rule, p.stage.as_deref()))
            .collect::<Vec<_>>(),
        [(Rule::FoldIntoHumanCheck, Some("polish"))]
    );
}

#[test]
fn the_format_refuses_what_isnt_a_flow() {
    assert_eq!(
        refused("flow:\n  - id: dev\n    name: Dev\n    colour: blue\n"),
        "line 4 column 5: unknown field `colour`, expected one of id, name, kind, by, column, agent, agents, \
         skill, options, rounds, fail_to, mode, delivers, why"
    );
    assert_eq!(
        refused("flows: []\n"),
        "line 1 column 1: unknown field `flows`, expected one of agents, flow"
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
            // d0130: an agent check lists its agents; `agent:` on one is refused, never read.
            "{name: Validate, kind: check, by: agent, agent: Validator, skill: validate, rounds: 3}",
            "Validate is an agent check: an agent check lists its agents: `agents: [Validator]`",
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
            "{name: Review, kind: check, by: you, options: [Quick developer]}",
            "Review is a check by you: no agent runs it, so its only option can be `skip`",
        ),
        (
            "{name: Review, kind: check, by: you, mode: click}",
            "Review is a check by you: `mode` belongs to the Delivery stage",
        ),
        (
            "{name: Done, kind: delivery, mode: click}",
            "Done is the Delivery stage: set its `mode` (click or automatic) and what it `delivers` (local, pr or \
             either)",
        ),
        (
            "{name: Done, kind: delivery, mode: click, delivers: local, agent: Developer}",
            "Done is a Local merge, which runs no agent: `agent` belongs to a stage an agent runs",
        ),
        (
            "{name: Done, kind: delivery, mode: click, delivers: pr}",
            "Done is a Delivery stage: name the agent that runs it with `agent:`",
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
    assert_eq!(ids, ["build", "code-review", "code-review-2", "done"]);
    // Written back, the ids are in the file and read back the same.
    let written = yaml::write(&read);
    assert_eq!(yaml::read(&written).unwrap(), read);
    assert_eq!(yaml::write(&yaml::read(&written).unwrap()), written);
}

/// d0130, d0131: an agent check's `agents` and a check's `fail_to` read and write back in their place in the field
/// order (`agents` where `agent` sits, `fail_to` after `rounds`); `fail_to` is read by name and kept as the stage's id.
#[test]
fn check_agents_and_fails_go_to_round_trip() {
    let text = "flow:
  - id: plan
    name: Product spec
    kind: agent
    column: own
    agent: Developer
    skill: work-ticket
  - id: dev
    name: Dev
    kind: agent
    column: own
    agent: Developer
    skill: work-ticket
  - id: checks
    name: Checks
    kind: check
    by: agent
    column: with-previous
    agents: [Security reviewer, Performance reviewer]
    skill: validate
    options: [skip]
    rounds: 2
    fail_to: Product spec
  - id: review
    name: Review
    kind: check
    by: you
    column: own
    fail_to: dev
  - id: deliver
    name: Done
    kind: delivery
    column: own
    mode: click
    delivers: local
";
    let read = yaml::read(text).unwrap();
    let checks = read.flow.stage("checks").unwrap();
    assert_eq!(
        checks.check_agents(),
        ["Security reviewer", "Performance reviewer"]
    );
    assert_eq!(checks.agent, None);
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

    // The test flow: Security has two check agents, QA's fails go to Grill rather than the nearest (Dev).
    let every = yaml::read(&every_kind()).unwrap();
    let security = every.flow.stage("security").unwrap();
    assert_eq!(security.check_agents(), ["Security reviewer", "Validator"]);
    let qa = every.flow.stage("qa").unwrap();
    assert_eq!(
        every.flow.fail_to_of(qa).map(|s| s.id.as_str()),
        Some("grill")
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
    assert_eq!(target("qa").as_deref(), Some("grill"));
    let mut gone = flow.clone();
    gone.stages.retain(|s| s.id != "grill");
    let qa = gone.stage("qa").unwrap();
    assert_eq!(qa.fail_to.as_deref(), Some("grill"), "still set in memory");
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

/// d0130: each listed check agent is checked against the agents (AGM-01), every one named.
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
        [
            (
                Rule::UnknownAgent,
                Some("validate"),
                "Validate names the agent Validator, which isn't one of your agents"
            ),
            (
                Rule::UnknownAgent,
                Some("security"),
                "Security names the agent Validator, which isn't one of your agents"
            ),
        ]
    );
}
