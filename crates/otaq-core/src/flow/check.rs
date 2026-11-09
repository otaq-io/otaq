//! The checks on a flow (STG-02; d0108's fixed rules and smaller rule 9, the second gap review's rules 1, 2 (as 0129
//! row 4 refines it) and 6).
//! Each problem names the stage and the rule in plain words, as `otaq flow propose` and the flow editor show it.

use std::collections::HashSet;

use crate::agent_tools::Tool;

use super::model::{Delivers, DeliveryMode, Flow, Stage, StageKind};

/// The rounds an agent check may have (d0063): 1–5; anything else is a [`Rule::Rounds`] problem.
pub const VALIDATION_ROUNDS: std::ops::RangeInclusive<u32> = 1..=5;
/// An agent check's rounds when its stage doesn't say.
pub const DEFAULT_VALIDATION_ROUNDS: u32 = 3;

/// The rule a problem breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rule {
    /// At least one Agent stage.
    AgentStage,
    /// No human check before the first Agent stage.
    HumanCheckFirst,
    /// The delivery delivers `either` only in click mode.
    EitherClickOnly,
    /// An agent check's rounds are 1–5.
    Rounds,
    /// With the skips taken, no Agent stage runs.
    SkipLeavesNoAgent,
    /// With the skips taken, a human check that can't be skipped has no running Agent stage before it (a skippable
    /// one is fine: the ticket waits there, or the user skips it too).
    SkipStrandsHumanCheck,
    /// Two stages with one id or one name.
    Duplicate,
    /// A stage or the delivery names an agent that isn't one of the agents (AGM-01).
    UnknownAgent,
    /// A check's fails go to a stage that isn't an Agent stage before it (d0131).
    FailTo,
    /// An agent check with no agent (it has one).
    NoCheckAgent,
    /// A stage's skill is a tool's built-in command (`/security-review`), which isn't a skill, and the user has no
    /// skill of that name.
    BuiltInCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Problem {
    pub rule: Rule,
    /// The id of the stage it is about, when it is about one.
    pub stage: Option<String>,
    pub message: String,
}

/// "Dev names the agent Rust developer, which isn't one of your agents".
pub fn unknown_agent_message(stage: &Stage, name: &str) -> String {
    format!(
        "{} names the agent {name}, which isn't one of your agents",
        stage.name
    )
}

/// The agents a flow names (AGM-01: a stage's `agent`, an agent check's too, and the delivery's that opens a PR),
/// each against `known` (whether an agent of that name is there); a problem per unknown name, in stage order, the
/// delivery's last.
pub fn check_agents(flow: &Flow, known: impl Fn(&str) -> bool) -> Result<(), Vec<Problem>> {
    let mut problems = Vec::new();
    for stage in &flow.stages {
        for name in stage.every_agent() {
            if !known(name) {
                problems.push(Problem {
                    rule: Rule::UnknownAgent,
                    stage: Some(stage.id.clone()),
                    message: unknown_agent_message(stage, name),
                });
            }
        }
    }
    if let Some(name) = flow.delivery.agent.as_deref().filter(|n| !known(n)) {
        problems.push(Problem {
            rule: Rule::UnknownAgent,
            stage: None,
            message: format!("The delivery names the agent {name}, which isn't one of your agents"),
        });
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// "Security: `/security-review` is Claude Code's built-in command, not a skill. …": `tools` are the tools that have
/// the command (at least one).
pub fn built_in_command_message(stage: &Stage, command: &str, tools: &[Tool]) -> String {
    let owners: Vec<String> = tools
        .iter()
        .map(|t| format!("{}'s", t.display_name()))
        .collect();
    let owners = match owners.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!(
        "{}: `/{command}` is {owners} built-in command, not a skill. Write a small skill that runs it, and name that \
         skill here",
        stage.name
    )
}

/// The stages whose skill is a tool's built-in command ([`Tool::with_built_in_command`]) rather than a skill, each
/// against `has` (whether the user has a skill of that name, which then is theirs and fine); a problem per stage.
pub fn check_skills(flow: &Flow, has: impl Fn(&str) -> bool) -> Result<(), Vec<Problem>> {
    let mut problems = Vec::new();
    for stage in &flow.stages {
        let Some(skill) = stage
            .skill
            .as_deref()
            .map(|s| s.trim().trim_start_matches('/'))
        else {
            continue;
        };
        let tools = Tool::with_built_in_command(skill);
        if tools.is_empty() || has(skill) {
            continue;
        }
        problems.push(Problem {
            rule: Rule::BuiltInCommand,
            stage: Some(stage.id.clone()),
            message: built_in_command_message(stage, skill, &tools),
        });
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// Every problem with the flow, in stage order; `Ok` when there is none.
pub fn check(flow: &Flow) -> Result<(), Vec<Problem>> {
    let mut problems = Vec::new();
    let stages = &flow.stages;
    let mut add = |rule, stage: Option<&Stage>, message: String| {
        problems.push(Problem {
            rule,
            stage: stage.map(|s| s.id.clone()),
            message,
        });
    };

    let mut ids = HashSet::new();
    let mut names = HashSet::new();
    for stage in stages {
        if !ids.insert(stage.id.as_str()) {
            add(
                Rule::Duplicate,
                Some(stage),
                format!(
                    "Two stages have the id `{}`: each stage needs its own",
                    stage.id
                ),
            );
        }
        if !names.insert(stage.name.as_str()) {
            add(
                Rule::Duplicate,
                Some(stage),
                format!(
                    "Two stages are named {}: each stage needs its own name",
                    stage.name
                ),
            );
        }
    }

    for stage in stages.iter().filter(|s| s.is_agent_check()) {
        if stage.check_agent().is_none() {
            add(
                Rule::NoCheckAgent,
                Some(stage),
                format!("{} is an agent check with no agent: name one", stage.name),
            );
        }
    }

    // A stage an agent runs may name no skill: Otaq's own skill for that stage runs ([`Stage::own_skill`]).

    // At least one Agent stage, and no human check before the first one.
    let first_agent = stages.iter().position(Stage::is_agent_stage);
    if first_agent.is_none() {
        add(
            Rule::AgentStage,
            None,
            "The flow has no Agent stage: at least one stage needs an agent working on the ticket"
                .to_owned(),
        );
    }
    for stage in &stages[..first_agent.unwrap_or(stages.len())] {
        if stage.is_human_check() {
            add(
                Rule::HumanCheckFirst,
                Some(stage),
                format!(
                    "{} is a check by you before any Agent stage: a Fail would have nowhere to go",
                    stage.name
                ),
            );
        }
    }

    let delivery = &flow.delivery;
    if delivery.delivers == Delivers::Either && delivery.mode == DeliveryMode::Automatic {
        add(
            Rule::EitherClickOnly,
            None,
            "The delivery is `either` automatically: choosing between a Local merge and a PR needs your click, \
             so use click mode or deliver `local` or `pr`"
                .to_owned(),
        );
    }

    for stage in stages {
        if let Some(rounds) = stage.rounds.filter(|r| !VALIDATION_ROUNDS.contains(r)) {
            add(
                Rule::Rounds,
                Some(stage),
                format!(
                    "{} has {rounds} rounds: use {} to {}",
                    stage.name,
                    VALIDATION_ROUNDS.start(),
                    VALIDATION_ROUNDS.end()
                ),
            );
        }
    }

    // Where each check's fails go (d0131): an Agent stage before it. A target no stage has any more is the nearest
    // one again (a flow file can't name one; the flow editor's removed stage, [`super::yaml`]).
    for (i, stage) in stages.iter().enumerate() {
        let target = stage
            .fail_to
            .as_deref()
            .filter(|_| matches!(stage.kind, StageKind::Check { .. }))
            .and_then(|id| flow.stage(id));
        let Some(target) = target else {
            continue;
        };
        let fits = target.is_agent_stage() && flow.position(&target.id).is_some_and(|t| t < i);
        if !fits {
            add(Rule::FailTo, Some(stage), fail_to_message(stage, target));
        }
    }

    // Skips. A rule broken by some combination of skips is broken by the combination that skips every skippable
    // Agent stage up to that point and nothing else (more skips only remove running Agent stages; a skipped human
    // check has nothing to send back), so checking those cases covers every combination, and names the smallest.
    let skippable = |s: &Stage| s.is_agent_stage() && s.can_skip();
    let agents: Vec<&Stage> = stages.iter().filter(|s| s.is_agent_stage()).collect();
    if !agents.is_empty() && agents.iter().all(|s| skippable(s)) {
        add(
            Rule::SkipLeavesNoAgent,
            None,
            format!("If {} skipped, no Agent stage runs", skipped(&agents)),
        );
    }
    // A skippable human check after only skippable Agent stages is allowed: a ticket that skips every one of them
    // waits there all the same (nothing is skipped on its own), and the user skips the check too if they want.
    for (i, stage) in stages.iter().enumerate() {
        let before: Vec<&Stage> = stages[..i].iter().filter(|s| s.is_agent_stage()).collect();
        // With none before it at all, HumanCheckFirst already says so.
        let required = stage.is_human_check() && !stage.can_skip();
        if required && !before.is_empty() && before.iter().all(|s| skippable(s)) {
            add(
                Rule::SkipStrandsHumanCheck,
                Some(stage),
                format!(
                    "If {} skipped, a Fail at {} has nowhere to go",
                    skipped(&before),
                    stage.name
                ),
            );
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// "Spec review's fails go to Validate, which isn't an Agent stage before it".
pub fn fail_to_message(check: &Stage, target: &Stage) -> String {
    format!(
        "{}'s fails go to {}, which isn't an Agent stage before it",
        check.name, target.name
    )
}

/// "Tech spec is", "Tech spec and Dev are both", "Product spec, Tech spec and Dev are all".
fn skipped(stages: &[&Stage]) -> String {
    let names: Vec<&str> = stages.iter().map(|s| s.name.as_str()).collect();
    match names.as_slice() {
        [one] => format!("{one} is"),
        [a, b] => format!("{a} and {b} are both"),
        [rest @ .., last] => format!("{} and {last} are all", rest.join(", ")),
        [] => unreachable!("a skip case names at least one stage"),
    }
}
