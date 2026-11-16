//! Reading and writing flow files (STG-01, d0108, UX-65). Reading goes through serde (`serde-saphyr`); an unknown
//! field is an error naming it, and a field that doesn't belong to the stage's kind is refused with the stage's name.
//! Writing is Otaq's own, so every file it writes has one layout and reads back unchanged:
//!
//! ```yaml
//! flow:
//!   - id: dev
//!     name: Dev
//!     kind: agent
//!     model: claude:opus
//!     effort: high
//!     skill: otaq:work-ticket
//!     instructions: Keep the public API stable.
//!     options: [skip]
//!   - id: validate
//!     name: Validate
//!     kind: check
//!     by: agent
//!     model: codex
//!     skill: otaq:validate
//!     rounds: 3
//!     fail_to: Dev
//! delivery:
//!   delivers: either
//!   mode: click
//!   model: claude:opus
//!   effort: high
//!   skill: otaq:open-pr
//! ```
//!
//! Fields come in a fixed order (stages: `id name kind by model effort skill instructions options rounds fail_to
//! description`; the delivery: `delivers mode model effort skill instructions`); a field with nothing in it is left
//! out. A stage an agent works or checks, and the delivery, say who does it (UX-65, UX-69): `model:` is a tool
//! (`claude`, its default model), `tool:model` (`claude:opus`) or, in Otaq's shipped files and older ones, `default`
//! (the account's default model's tool, set when the flow is applied); `effort:` is one of the tool's levels;
//! `instructions:` are added to the stage's skill (UX-67). Every flow says how its work is delivered after the last
//! stage (`delivery:`): it is a setting of the flow, not a stage. Every stage is its own column. A proposal may add a
//! top-level `why:` (stage id or name → its reason, shown under Why these changes); it is read only by `flow propose`
//! ([`read_proposal`]) and never written, so the flow keeps its descriptions and no reasons.
//!
//! Older files still read, and are written in today's shape (UX-73): a top-level `agents:` list and stages (and the
//! delivery) naming one of them with `agent:` read as each stage's own model and instructions from that list (an
//! `agent:` the file doesn't define is refused); their Delivery stage (`kind: delivery` with `mode` and `delivers`)
//! reads as the flow's `delivery:`, and a stage's `column:` (`own`, `with-previous`) is read and ignored. An older
//! check's `agents: [Validator]` (one name) reads as `agent: Validator`, and its check with several agents reads as one
//! check stage per agent ([`Split`]). A check's `fail_to` names a stage by its name; Otaq keeps it by the stage's id, so
//! a rename carries it along, and a stage removed leaves the check on the nearest Agent stage (d0131). Comments are not
//! kept.

use std::fmt;

use serde::Deserialize;

use super::model::{
    CheckBy, Delivers, Delivery, DeliveryMode, Flow, FlowFile, Stage, StageKind, StageOption,
    WhoDoesIt,
};
use crate::agents::AgentModel;

/// Why a flow file can't be read: the YAML's own error (with the line), or a stage field that doesn't fit its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatError(pub String);

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FormatError {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    /// An older file's agents (UX-73), which its stages name: read into each stage's own model and instructions.
    #[serde(default)]
    agents: Vec<RawAgent>,
    flow: Vec<RawStage>,
    delivery: Option<RawDelivery>,
    /// A proposal's reasons, by stage (its id or name): "Why these changes". Never part of the flow: read only by
    /// `flow propose` ([`read_proposal`]) and never written.
    #[serde(default)]
    why: std::collections::BTreeMap<String, String>,
}

/// The flow's `delivery:`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDelivery {
    delivers: Option<Delivers>,
    mode: Option<DeliveryMode>,
    model: Option<String>,
    effort: Option<String>,
    instructions: Option<String>,
    /// An older file's agent name (UX-73).
    agent: Option<String>,
    skill: Option<String>,
}

/// An older stage `column:`: read so the file still opens, then ignored (every stage is its own column).
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
enum RawColumn {
    Own,
    WithPrevious,
}

/// An older file's agent (UX-73): its name, instructions and model, which the stages naming it take.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgent {
    name: String,
    /// An old file's agent role (`noyan`): read so the file still opens, then ignored.
    #[serde(default, rename = "role")]
    _role: serde::de::IgnoredAny,
    #[serde(default)]
    instructions: String,
    /// An old file's agent skills: read so the file still opens, then ignored.
    #[serde(default, rename = "skills")]
    _skills: serde::de::IgnoredAny,
    model: String,
    /// Why the flow had it: read, then ignored.
    #[serde(default, rename = "why")]
    _why: serde::de::IgnoredAny,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStage {
    id: Option<String>,
    name: String,
    kind: RawKind,
    by: Option<CheckBy>,
    #[serde(default)]
    column: Option<RawColumn>,
    model: Option<String>,
    effort: Option<String>,
    instructions: Option<String>,
    /// An older file's agent name (UX-73).
    agent: Option<String>,
    /// An older check's agents (UX-73, [`Split`]).
    agents: Option<Vec<String>>,
    skill: Option<String>,
    #[serde(default)]
    options: Vec<String>,
    rounds: Option<u32>,
    fail_to: Option<String>,
    mode: Option<DeliveryMode>,
    delivers: Option<Delivers>,
    /// What the stage is for; an older file's `why:` reads as it.
    #[serde(alias = "why")]
    description: Option<String>,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "kebab-case")]
enum RawKind {
    Agent,
    Check,
    /// An older file's Delivery stage, read as the flow's `delivery:`.
    Delivery,
}

/// The stage templates file (`stage-templates.yaml`): each template.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTemplatesFile {
    templates: Vec<RawTemplate>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTemplate {
    title: Option<String>,
    line: String,
    stage: RawStage,
}

/// One stage template as read: the palette's title (the stage's name unless the file gives one), its line, and the
/// stage it adds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StageTemplate {
    pub title: String,
    pub line: String,
    pub stage: Stage,
}

/// Reads a stage templates file: each stage read as a flow file's stage is, with its own model (a template has no
/// `fail_to`, which names another stage, and names no agent).
pub fn read_templates(text: &str) -> Result<Vec<StageTemplate>, FormatError> {
    let raw: RawTemplatesFile = serde_saphyr::from_str(text).map_err(format_error)?;
    let mut templates = Vec::with_capacity(raw.templates.len());
    for t in raw.templates {
        if t.stage.fail_to.is_some() {
            return Err(FormatError(format!(
                "{} is a template: `fail_to` names another stage, which a template can't know",
                t.stage.name
            )));
        }
        let (mut stage, older) = stage(t.stage)?;
        if !older.is_empty() {
            return Err(FormatError(format!(
                "{} is a template: say who does it with `model:`",
                stage.name
            )));
        }
        if stage.id.is_empty() {
            stage.id = slug(&stage.name);
        }
        templates.push(StageTemplate {
            title: t.title.unwrap_or_else(|| stage.name.clone()),
            line: t.line,
            stage,
        });
    }
    Ok(templates)
}

/// The YAML's own error: its first line ("line 5 column 5: unknown field `colour`, expected one of id, name, …");
/// the source snippet under it is left out.
fn format_error(e: impl fmt::Display) -> FormatError {
    let text = e.to_string();
    let first = text.lines().next().unwrap_or_default();
    FormatError(first.strip_prefix("error: ").unwrap_or(first).to_owned())
}

/// A lookup of an older file's agent by its name: the stored agents the migration reads ([`read_older`]).
pub type AgentLookup<'a> = &'a dyn Fn(&str) -> Option<WhoDoesIt>;

/// What an older file's agent name stands for (UX-71, UX-73): looked up by `account` first (the stored agents the
/// migration reads), then among the file's own `agents:`, by name ignoring case; one neither has is `unknown` when the
/// migration gives one, else refused.
struct Older<'a> {
    file: Vec<RawAgent>,
    account: Option<AgentLookup<'a>>,
    unknown: Option<&'a WhoDoesIt>,
}

impl Older<'_> {
    /// Who does the work of something that names the agent `name` (`what`: "Dev", "The delivery").
    fn who(&self, what: &str, name: &str) -> Result<WhoDoesIt, FormatError> {
        if let Some(who) = self.account.and_then(|account| account(name)) {
            return Ok(who);
        }
        let folded = name.trim().to_lowercase();
        let Some(agent) = self
            .file
            .iter()
            .find(|a| a.name.trim().to_lowercase() == folded)
        else {
            return match self.unknown {
                Some(who) => Ok(who.clone()),
                None => Err(FormatError(unknown_agent_message(what, name))),
            };
        };
        Ok(WhoDoesIt {
            model: Some(agent.model.trim().to_owned()).filter(|m| !m.is_empty()),
            effort: None,
            instructions: why(Some(agent.instructions.clone())),
        })
    }
}

/// "Dev names the agent Rust developer, which the file doesn't define: say who does it with `model:`".
pub fn unknown_agent_message(what: &str, name: &str) -> String {
    format!(
        "{what} names the agent {name}, which the file doesn't define: say who does it with `model:`, and \
         `effort:` and `instructions:` if it has them"
    )
}

/// Reads a flow file. The STG-02 rules are [`super::check`]'s; this only refuses what isn't a flow at all.
pub fn read(text: &str) -> Result<FlowFile, FormatError> {
    read_with_written_ids(text).map(|(file, _)| file)
}

/// [`read`] for the migration (UX-71): an older file's agent names are looked up by `account` (the stored agents)
/// first, then in the file's own `agents:`; a name neither has is `unknown` when given, else refused.
pub fn read_older(
    text: &str,
    account: &dyn Fn(&str) -> Option<WhoDoesIt>,
    unknown: Option<&WhoDoesIt>,
) -> Result<FlowFile, FormatError> {
    read_all(text, Some(account), unknown).map(|read| read.file)
}

/// [`read`], also saying which stages had their id written (in stage order): the others got one from their name,
/// which a proposal matches to the flow it changes instead ([`super::proposal`]). A copy split off an older check
/// ([`Split`]) counts as written when its stage's id was.
pub fn read_with_written_ids(text: &str) -> Result<(FlowFile, Vec<bool>), FormatError> {
    read_all(text, None, None).map(|read| (read.file, read.written))
}

/// A proposed document as `flow propose` reads it: [`read_with_written_ids`], plus its top-level `why:`, the reason for
/// each change by stage id or name ("security: From 41 /security-review runs in shop-web (Claude Code)."), which the
/// draft shows under Why these changes. The flow itself never holds them.
pub fn read_proposal(text: &str) -> Result<(FlowFile, Vec<bool>, Whys), FormatError> {
    read_all(text, None, None).map(|read| (read.file, read.written, read.whys))
}

/// A proposal's reasons, by stage id or name, trimmed; empty lines dropped.
pub type Whys = std::collections::BTreeMap<String, String>;

/// [`read`], also saying which check stages were split because they listed several agents ([`Split`]).
pub fn read_split(text: &str) -> Result<(FlowFile, Vec<Split>), FormatError> {
    read_all(text, None, None).map(|read| (read.file, read.splits))
}

/// An older document (agents, a Delivery stage or a stage's `column:`), as Otaq writes it now (each stage's own model,
/// the delivery as the flow's `delivery:`, no columns); `None` for a document already in today's form, or one that
/// doesn't read.
pub fn upgraded(text: &str) -> Option<String> {
    read_all(text, None, None)
        .ok()
        .filter(|read| read.old_form)
        .map(|read| write(&read.file))
}

/// A document as [`read_all`] reads it.
struct Read {
    file: FlowFile,
    /// Which stages had their id written ([`read_with_written_ids`]).
    written: Vec<bool>,
    splits: Vec<Split>,
    /// It has agents, a Delivery stage or a stage's `column:` ([`upgraded`]).
    old_form: bool,
    /// A proposal's reasons ([`read_proposal`]).
    whys: Whys,
}

/// An older flow's agent check that listed several agents (`agents: [Security reviewer, Validator]`), as read: one
/// check stage per agent, in the listed order, one after the other. The first keeps the stage's id and name; each
/// other copy gets the id `<id>-<agent>` (the agent's name as an id: `security-validator`, with `-2` added if that
/// id is taken) and the name `<stage> · <agent>` ("Security · Validator", with " 2" added if that name is taken), so
/// every read of the same document gives the same ids. Each copy keeps the stage's kind, skill, rounds, fails go to,
/// options and why, takes its agent's model and instructions, and is its own column, as every stage is. The split is
/// never stored by reading: a document written from the flow ([`write`]) holds it, so the user's next change saves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// The stage's id, which its first copy keeps.
    pub stage: String,
    /// The other copies' ids, in order.
    pub copies: Vec<String>,
}

fn read_all(
    text: &str,
    account: Option<AgentLookup<'_>>,
    unknown: Option<&WhoDoesIt>,
) -> Result<Read, FormatError> {
    let raw: RawFile = serde_saphyr::from_str(text).map_err(format_error)?;
    let mut old_form = !raw.agents.is_empty();
    let older = Older {
        file: raw.agents,
        account,
        unknown,
    };
    let mut stages = Vec::with_capacity(raw.flow.len());
    let mut others = Vec::with_capacity(raw.flow.len());
    // An older file's Delivery stage, by its name: the flow's delivery.
    let mut old_delivery: Option<(String, Delivery)> = None;
    for raw in raw.flow {
        old_form |= raw.column.is_some() || matches!(raw.kind, RawKind::Delivery);
        if matches!(raw.kind, RawKind::Delivery) {
            let (name, delivery) = old_delivery_stage(raw, &older)?;
            if let Some((first, _)) = &old_delivery {
                return Err(FormatError(format!(
                    "{first} and {name} are both Delivery stages: a flow is delivered one way, set once with \
                     `delivery:`"
                )));
            }
            old_delivery = Some((name, delivery));
            continue;
        }
        let (mut stage, names) = stage(raw)?;
        let mut more = Vec::new();
        let mut names = names.into_iter();
        if let Some(first) = names.next() {
            old_form = true;
            stage.who = checked_who(&stage.name, older.who(&stage.name, &first)?)?;
        }
        for name in names {
            let who = checked_who(&stage.name, older.who(&stage.name, &name)?)?;
            more.push((name, who));
        }
        stages.push(stage);
        others.push(more);
    }
    let mut written: Vec<bool> = stages.iter().map(|s| !s.id.is_empty()).collect();
    give_ids(&mut stages);
    fail_to_ids(&mut stages)?;
    let delivery = match (raw.delivery, old_delivery) {
        (Some(raw), None) => {
            old_form |= raw.agent.is_some();
            delivery(raw, &older)?
        }
        (None, Some((_, delivery))) => delivery,
        (Some(_), Some((name, _))) => {
            return Err(FormatError(format!(
                "{name} is a Delivery stage and the flow has `delivery:` too: say how the work is delivered once, \
                 with `delivery:`"
            )));
        }
        (None, None) => {
            return Err(FormatError(
                "The flow doesn't say how its work is delivered: add `delivery:` with what it `delivers` (local, \
                 pr or either) and its `mode` (click or automatic)"
                    .to_owned(),
            ));
        }
    };
    let splits = split(&mut stages, &mut written, others);
    let whys = raw
        .why
        .into_iter()
        .filter_map(|(stage, why)| {
            let why = why.trim();
            (!why.is_empty()).then(|| (stage.trim().to_owned(), why.to_owned()))
        })
        .collect();
    Ok(Read {
        file: FlowFile {
            flow: Flow { stages, delivery },
        },
        written,
        splits,
        old_form,
        whys,
    })
}

/// Splits each agent check that listed several agents ([`Split`]): `others[i]` are stage `i`'s agents after its
/// first, each with who it stands for. Runs once every stage has its id and its fails go to are ids, so a copy keeps
/// them.
fn split(
    stages: &mut Vec<Stage>,
    written: &mut Vec<bool>,
    others: Vec<Vec<(String, WhoDoesIt)>>,
) -> Vec<Split> {
    let mut splits = Vec::new();
    let mut at = 0;
    for more in others {
        let first = stages[at].clone();
        let was_written = written[at];
        at += 1;
        if more.is_empty() {
            continue;
        }
        let mut copies = Vec::with_capacity(more.len());
        for (agent, who) in more {
            let id = unique(&format!("{}-{}", first.id, slug(&agent)), "-", |id| {
                stages.iter().any(|s| s.id == id)
            });
            let name = unique(&format!("{} · {agent}", first.name), " ", |name| {
                stages.iter().any(|s| s.name == name)
            });
            let copy = Stage {
                id: id.clone(),
                name,
                who,
                ..first.clone()
            };
            stages.insert(at, copy);
            written.insert(at, was_written);
            at += 1;
            copies.push(id);
        }
        splits.push(Split {
            stage: first.id,
            copies,
        });
    }
    splits
}

/// `base`, else `base` with ` 2`, ` 3`… (`sep` and the number) added, the first that isn't `taken`.
fn unique(base: &str, sep: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut out = base.to_owned();
    let mut n = 2;
    while taken(&out) {
        out = format!("{base}{sep}{n}");
        n += 1;
    }
    out
}

/// Writes who does a stage's or the delivery's work into `item`: `model`, `effort`, then `skill` and `instructions`.
fn write_who(item: &mut Item<'_>, who: &WhoDoesIt, skill: Option<&String>) {
    if let Some(model) = &who.model {
        item.field("model", model);
    }
    if let Some(effort) = &who.effort {
        item.field("effort", effort);
    }
    if let Some(skill) = skill {
        item.field("skill", skill);
    }
    if let Some(instructions) = &who.instructions {
        item.text("instructions", instructions);
    }
}

/// Writes a flow file in Otaq's layout (above).
pub fn write(file: &FlowFile) -> String {
    let mut out = String::new();
    out.push_str("flow:\n");
    for stage in &file.flow.stages {
        let mut item = Item::new(&mut out, 1);
        item.field("id", &stage.id);
        item.field("name", &stage.name);
        item.field("kind", stage.kind.as_str());
        if let StageKind::Check { by } = stage.kind {
            item.field("by", by.as_str());
        }
        write_who(&mut item, &stage.who, stage.skill.as_ref());
        item.list("options", stage.options.iter().map(StageOption::as_str));
        if let Some(rounds) = stage.rounds {
            item.number("rounds", rounds);
        }
        // By the stage's name; a stage gone from the flow writes nothing: the nearest Agent stage again.
        let fail_to = stage
            .fail_to
            .as_deref()
            .filter(|_| matches!(stage.kind, StageKind::Check { .. }));
        if let Some(target) = fail_to.and_then(|id| file.flow.stage(id)) {
            item.field("fail_to", &target.name);
        }
        if let Some(description) = &stage.description {
            item.text("description", description);
        }
    }
    let delivery = &file.flow.delivery;
    out.push_str("delivery:\n");
    let mut item = Item::new(&mut out, 0);
    item.field("delivers", delivery.delivers.as_str());
    item.field("mode", delivery.mode.as_str());
    write_who(&mut item, &delivery.who, delivery.skill.as_ref());
    out
}

/// The flow's `delivery:`: what it delivers and when, and who writes the message the delivery uses while a ticket
/// waits in Review (the squash commit's, and a PR's title and description) and opens the PR (with an optional skill).
/// A delivery that can open a PR names its model; a local merge may, and without one its message is Otaq's plain one.
fn delivery(raw: RawDelivery, older: &Older<'_>) -> Result<Delivery, FormatError> {
    let who = WhoDoesIt {
        model: raw.model,
        effort: raw.effort,
        instructions: raw.instructions,
    };
    checked_delivery(
        "The delivery",
        raw.mode,
        raw.delivers,
        who,
        raw.agent,
        raw.skill,
        older,
    )
}

/// An older file's Delivery stage (`kind: delivery`), as the flow's delivery; its name, for the errors.
fn old_delivery_stage(raw: RawStage, older: &Older<'_>) -> Result<(String, Delivery), FormatError> {
    let name = raw.name.trim().to_owned();
    let refuse = |field: &str| {
        Err(FormatError(format!(
            "{name} is a Delivery stage: `{field}` belongs to a check"
        )))
    };
    if raw.by.is_some() {
        return refuse("by");
    }
    if raw.rounds.is_some() {
        return refuse("rounds");
    }
    if raw.fail_to.is_some() {
        return refuse("fail_to");
    }
    if raw.agents.is_some() {
        return refuse("agents");
    }
    let who = WhoDoesIt {
        model: raw.model,
        effort: raw.effort,
        instructions: raw.instructions,
    };
    let delivery = checked_delivery(
        &name,
        raw.mode,
        raw.delivers,
        who,
        raw.agent,
        raw.skill,
        older,
    )?;
    Ok((name, delivery))
}

fn checked_delivery(
    what: &str,
    mode: Option<DeliveryMode>,
    delivers: Option<Delivers>,
    who: WhoDoesIt,
    agent: Option<String>,
    skill: Option<String>,
    older: &Older<'_>,
) -> Result<Delivery, FormatError> {
    let (Some(mode), Some(delivers)) = (mode, delivers) else {
        return Err(FormatError(format!(
            "{what} needs its `mode` (click or automatic) and what it `delivers` (local, pr or either)"
        )));
    };
    let agent = agent.map(|a| a.trim().to_owned()).filter(|a| !a.is_empty());
    let skill = skill.map(|s| s.trim().to_owned()).filter(|s| !s.is_empty());
    let who = match agent {
        Some(name) if who != WhoDoesIt::default() => {
            return Err(FormatError(format!(
                "{what} names the agent {name} and its own model: say who does it once, with `model:`"
            )));
        }
        Some(name) => older.who(what, &name)?,
        None => who,
    };
    let who = checked_who(what, who)?;
    if delivers == Delivers::Local {
        // A local merge may name the model that writes its squash message while the ticket waits in Review; the
        // skill is the one a PR is opened with.
        if skill.is_some() {
            return Err(FormatError(format!(
                "{what} is a Local merge, which opens no PR: `skill` belongs to a delivery that opens a PR"
            )));
        }
    } else if who.model.is_none() {
        return Err(FormatError(format!(
            "{what} can open a PR: say which model writes and opens it with `model:`"
        )));
    }
    Ok(Delivery {
        mode,
        delivers,
        who,
        skill,
    })
}

/// Who does a stage's (or the delivery's, `what`) work, as kept: each field trimmed, none when empty; its model one
/// that reads ([`AgentModel::parse`]) and its effort one of its tool's levels.
fn checked_who(what: &str, who: WhoDoesIt) -> Result<WhoDoesIt, FormatError> {
    let model = who
        .model
        .map(|m| m.trim().to_owned())
        .filter(|m| !m.is_empty());
    let effort = who
        .effort
        .map(|e| e.trim().to_owned())
        .filter(|e| !e.is_empty());
    let parsed = match &model {
        Some(text) => {
            Some(AgentModel::parse(text).map_err(|e| FormatError(format!("{what}'s model: {e}")))?)
        }
        None => None,
    };
    if let Some(level) = &effort {
        match &parsed {
            None => {
                return Err(FormatError(format!(
                    "{what} has an effort but no model: say which model it runs with `model:`"
                )));
            }
            Some(model) => {
                if let Some(refused) = crate::agents::effort_refusal(model, level) {
                    return Err(FormatError(format!("{what}'s effort: {refused}")));
                }
            }
        }
    }
    Ok(WhoDoesIt {
        model,
        effort,
        instructions: why(who.instructions),
    })
}

/// A stage from its raw fields: the kind's own fields must be there, and no other kind's. With an older stage's agent
/// names (`agent: Developer`, or a check's `agents: [Security reviewer, Validator]`, which [`split`] makes copies
/// of), which [`read_all`] reads as the stage's own model.
fn stage(raw: RawStage) -> Result<(Stage, Vec<String>), FormatError> {
    let name = raw.name.trim().to_owned();
    if name.is_empty() {
        return Err(FormatError("A stage has no name".to_owned()));
    }
    let refuse = |what: &str, field: &str, belongs: &str| {
        Err(FormatError(format!(
            "{name} is {what}: `{field}` belongs to {belongs}"
        )))
    };
    let kind = match (raw.kind, raw.by) {
        (RawKind::Check, Some(by)) => StageKind::Check { by },
        (RawKind::Check, None) => {
            return Err(FormatError(format!(
                "{name} is a check: say who does it with `by: agent` or `by: you`"
            )));
        }
        (RawKind::Agent, Some(_)) => return refuse("an Agent stage", "by", "a check"),
        (RawKind::Agent, None) => StageKind::Agent,
        // Read as the flow's delivery before a stage is made ([`old_delivery_stage`]).
        (RawKind::Delivery, _) => {
            return Err(FormatError(format!(
                "{name} is a Delivery stage: say how the work is delivered with the flow's `delivery:`"
            )));
        }
    };
    let what = match kind {
        StageKind::Agent => "an Agent stage",
        StageKind::Check { by: CheckBy::Agent } => "an agent check",
        StageKind::Check { by: CheckBy::You } => "a check by you",
    };
    if raw.mode.is_some() {
        return refuse(what, "mode", "the flow's `delivery:`");
    }
    if raw.delivers.is_some() {
        return refuse(what, "delivers", "the flow's `delivery:`");
    }
    let agent_check = kind == StageKind::Check { by: CheckBy::Agent };
    if !agent_check && raw.rounds.is_some() {
        return refuse(what, "rounds", "an agent check");
    }
    if !agent_check && raw.agents.is_some() {
        return refuse(what, "agents", "an agent check");
    }
    if !matches!(kind, StageKind::Check { .. }) && raw.fail_to.is_some() {
        return refuse(what, "fail_to", "a check");
    }
    let who = WhoDoesIt {
        model: raw.model,
        effort: raw.effort,
        instructions: raw.instructions,
    };
    // An older stage names its agent with `agent:`; an older check's `agents: [X]` reads as `agent: X`, and its
    // `agents: [X, Y]` as `agent: X` with a copy of the stage for Y after it ([`split`]).
    let mut names = Vec::new();
    match (raw.agent, raw.agents) {
        (Some(_), Some(_)) => {
            return Err(FormatError(format!(
                "{name} is an agent check: say who does it once, with `model:`"
            )));
        }
        (Some(agent), None) => names.push(agent.trim().to_owned()),
        (None, Some(agents)) => {
            for agent in agents.into_iter().map(|a| a.trim().to_owned()) {
                if !agent.is_empty() && !names.contains(&agent) {
                    names.push(agent);
                }
            }
        }
        (None, None) => {}
    }
    names.retain(|n| !n.is_empty());
    if !names.is_empty() && who != WhoDoesIt::default() {
        return Err(FormatError(format!(
            "{name} names the agent {} and its own model: say who does it once, with `model:`",
            names[0]
        )));
    }
    // Who does it: a model for an Agent stage and an agent check. One with no model yet reads (a draft's); the
    // checks refuse it (Rule::NoModel). With no `skill:` it reads and passes the checks: Otaq's own skill for that
    // stage runs.
    let runs_agent = match kind {
        StageKind::Agent | StageKind::Check { by: CheckBy::Agent } => true,
        StageKind::Check { by: CheckBy::You } => false,
    };
    let who = if runs_agent {
        checked_who(&name, who)?
    } else {
        let belongs = "a stage an agent works or checks";
        if !names.is_empty() {
            return refuse(what, "agent", belongs);
        }
        if who.model.is_some() {
            return refuse(what, "model", belongs);
        }
        if who.effort.is_some() {
            return refuse(what, "effort", belongs);
        }
        if who.instructions.is_some() {
            return refuse(what, "instructions", belongs);
        }
        if raw.skill.is_some() {
            return refuse(what, "skill", belongs);
        }
        WhoDoesIt::default()
    };
    if kind == (StageKind::Check { by: CheckBy::Agent }) && raw.rounds.is_none() {
        return Err(FormatError(format!(
            "{name} is an agent check: set its `rounds` (1 to 5)"
        )));
    }
    // `skip` is the only option: the agent names older files list are read and ignored (UX-72).
    let options: Vec<StageOption> = raw
        .options
        .iter()
        .any(|o| o.trim() == StageOption::SKIP)
        .then_some(StageOption::Skip)
        .into_iter()
        .collect();
    let stage = Stage {
        id: raw.id.map(|id| id.trim().to_owned()).unwrap_or_default(),
        name,
        kind,
        who,
        skill: raw.skill,
        options,
        rounds: raw.rounds,
        // A stage's name until [`fail_to_ids`] makes it the stage's id.
        fail_to: raw.fail_to.map(|n| n.trim().to_owned()),
        description: why(raw.description),
    };
    Ok((stage, names))
}

/// A `why:` or a stage's `description:` as kept: trimmed, none when empty.
fn why(text: Option<String>) -> Option<String> {
    text.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty())
}

/// Gives a stage written without an id one from its name (`Code review` → `code-review`), unique in the flow.
/// Ids that were written are kept as they are.
fn give_ids(stages: &mut [Stage]) {
    for i in 0..stages.len() {
        if !stages[i].id.is_empty() {
            continue;
        }
        let base = slug(&stages[i].name);
        let taken = |id: &str| stages.iter().any(|s| s.id == id);
        let mut id = base.clone();
        let mut n = 2;
        while taken(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        stages[i].id = id;
    }
}

/// Each check's `fail_to`, read as a stage's name, as that stage's id: the same name, else the same name ignoring
/// case. A name no stage has is refused; whether the stage is a fit is [`super::check`]'s [`super::Rule::FailTo`].
fn fail_to_ids(stages: &mut [Stage]) -> Result<(), FormatError> {
    for i in 0..stages.len() {
        let Some(name) = stages[i].fail_to.clone() else {
            continue;
        };
        let target = stages
            .iter()
            .find(|s| s.name == name)
            .or_else(|| stages.iter().find(|s| s.name.eq_ignore_ascii_case(&name)))
            .map(|s| s.id.clone());
        let Some(id) = target else {
            return Err(FormatError(format!(
                "{}'s fails go to {name}, which isn't a stage of this flow: name an Agent stage before it with \
                 `fail_to:`, or leave it out for the nearest one",
                stages[i].name
            )));
        };
        stages[i].fail_to = Some(id);
    }
    Ok(())
}

/// A stage id from its name: `Code review` → `code-review`.
pub(crate) fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        "stage".to_owned()
    } else {
        out.to_owned()
    }
}

/// One `- ` item of a top-level list (`depth` 1: the first field opens it, the others line up under it), or the
/// fields of a top-level mapping (`depth` 0: `delivery:`).
struct Item<'a> {
    out: &'a mut String,
    first: bool,
    list: bool,
}

impl<'a> Item<'a> {
    fn new(out: &'a mut String, depth: u8) -> Self {
        Self {
            out,
            first: true,
            list: depth > 0,
        }
    }

    fn key(&mut self, key: &str) {
        self.out.push_str(match (self.list, self.first) {
            (true, true) => "  - ",
            (true, false) => "    ",
            (false, _) => "  ",
        });
        self.first = false;
        self.out.push_str(key);
        self.out.push(':');
    }

    fn field(&mut self, key: &str, value: &str) {
        self.key(key);
        self.out.push(' ');
        self.out.push_str(&scalar(value));
        self.out.push('\n');
    }

    fn number(&mut self, key: &str, value: u32) {
        self.key(key);
        self.out.push_str(&format!(" {value}\n"));
    }

    /// A flow list, `[skip, Quick developer]`; left out when empty.
    fn list<'v>(&mut self, key: &str, values: impl Iterator<Item = &'v str>) {
        let values: Vec<String> = values.map(scalar).collect();
        if values.is_empty() {
            return;
        }
        self.key(key);
        self.out.push_str(" [");
        self.out.push_str(&values.join(", "));
        self.out.push_str("]\n");
    }

    /// Text that may span lines: a literal block (`|`, or `|-` without a final newline) when it reads back the
    /// same, else a quoted string.
    fn text(&mut self, key: &str, value: &str) {
        let body = value.strip_suffix('\n').unwrap_or(value);
        let block = value.contains('\n')
            && !body.ends_with('\n')
            && !body.starts_with([' ', '\t'])
            && !body.contains('\r')
            && body.lines().all(|l| l == l.trim_end())
            && body
                .chars()
                .all(|c| c == '\n' || c == '\t' || !c.is_control());
        if !block {
            return self.field(key, value);
        }
        self.key(key);
        self.out.push_str(if value.ends_with('\n') {
            " |\n"
        } else {
            " |-\n"
        });
        for line in body.split('\n') {
            if !line.is_empty() {
                self.out.push_str(if self.list { "      " } else { "    " });
                self.out.push_str(line);
            }
            self.out.push('\n');
        }
    }
}

/// A scalar as written: plain when it can only read back as the same string, else double-quoted.
fn scalar(value: &str) -> String {
    let plain = value.chars().next().is_some_and(char::is_alphabetic)
        && !value.ends_with(' ')
        // A colon inside a word is plain (`otaq:validate`); one before a space or at the end would start a mapping.
        && !value.contains(": ")
        && !value.ends_with(':')
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || " -_./()·:".contains(c))
        && !matches!(
            value.to_lowercase().as_str(),
            "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "y" | "n" | "inf" | "nan"
        );
    if plain {
        return value.to_owned();
    }
    let mut out = String::from("\"");
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_read_back_as_written() {
        for value in [
            "Dev",
            "Quick developer",
            "work-ticket",
            crate::own_skill!("work-ticket"),
            "claude:opus",
            "a:",
            "a:: b",
            "yes",
            "3",
            "a: b",
            "# not a comment",
            "say \"hi\"",
            "",
            " leading",
            "[x]",
            "Claude Code · opus",
        ] {
            let text = format!("v: {}\n", scalar(value));
            let read: std::collections::BTreeMap<String, String> =
                serde_saphyr::from_str(&text).unwrap();
            assert_eq!(read["v"], value, "{text}");
        }
    }

    #[test]
    fn ids_come_from_names_when_left_out() {
        assert_eq!(slug("Code review"), "code-review");
        assert_eq!(slug("QA!"), "qa");
        assert_eq!(slug("··"), "stage");
    }
}
