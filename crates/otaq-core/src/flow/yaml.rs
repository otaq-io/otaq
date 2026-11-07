//! Reading and writing flow files (STG-01, d0108). Reading goes through serde (`serde-saphyr`); an unknown field is
//! an error naming it, and a field that doesn't belong to the stage's kind is refused with the stage's name.
//! Writing is Otaq's own, so every file it writes has one layout and reads back unchanged:
//!
//! ```yaml
//! flow:
//!   - id: dev
//!     name: Dev
//!     kind: agent
//!     column: own
//!     agent: Developer
//!     skill: work-ticket
//!     options: [skip]
//!   - id: validate
//!     name: Validate
//!     kind: check
//!     by: agent
//!     column: own
//!     agent: Validator
//!     skill: validate
//!     rounds: 3
//!     fail_to: Dev
//! ```
//!
//! Fields come in a fixed order (stages: `id name kind by column agent skill options rounds fail_to mode delivers
//! description`; agents: `name instructions model why`); a field with nothing in it is left out, except `column`.
//! `agents:` is left out when the file defines none. An agent check names its one agent with `agent:`, like an Agent
//! stage; an older file's `agents: [Validator]` (one name) still reads as `agent: Validator`, and its check with
//! several agents reads as one check stage per agent ([`Split`]). A check's `fail_to`
//! names a stage by its name, as `agent:` names agents; Otaq keeps it by the stage's id, so a rename carries it
//! along, and a stage removed leaves the check on the nearest Agent stage (d0131). Comments are not kept.

use std::fmt;

use serde::Deserialize;

use super::model::{
    CheckBy, Column, Delivers, Delivery, DeliveryMode, Flow, FlowAgent, FlowFile, Stage, StageKind,
    StageOption,
};

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
    #[serde(default)]
    agents: Vec<RawAgent>,
    flow: Vec<RawStage>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgent {
    name: String,
    /// An old file's agent role (`noyan`): read so the file still opens, then ignored (an agent has no role; the
    /// agent Noyan uses is an app setting).
    #[serde(default, rename = "role")]
    _role: serde::de::IgnoredAny,
    #[serde(default)]
    instructions: String,
    /// An old file's agent skills: read so the file still opens, then ignored (an agent has no skills).
    #[serde(default, rename = "skills")]
    _skills: serde::de::IgnoredAny,
    model: String,
    why: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStage {
    id: Option<String>,
    name: String,
    kind: RawKind,
    by: Option<CheckBy>,
    column: Option<Column>,
    agent: Option<String>,
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
    Delivery,
}

/// An agents file (`otaq-agents.yaml`, AGM-02): only `agents:`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawAgentsFile {
    agents: Vec<RawAgent>,
}

/// The YAML's own error: its first line ("line 5 column 5: unknown field `colour`, expected one of id, name, …");
/// the source snippet under it is left out.
fn format_error(e: impl fmt::Display) -> FormatError {
    let text = e.to_string();
    let first = text.lines().next().unwrap_or_default();
    FormatError(first.strip_prefix("error: ").unwrap_or(first).to_owned())
}

fn agent(a: RawAgent) -> FlowAgent {
    FlowAgent {
        name: a.name,
        instructions: a.instructions,
        model: a.model,
        why: why(a.why),
    }
}

/// Reads a flow file. The STG-02 rules are [`super::check`]'s; this only refuses what isn't a flow at all.
pub fn read(text: &str) -> Result<FlowFile, FormatError> {
    read_with_written_ids(text).map(|(file, _)| file)
}

/// [`read`], also saying which stages had their id written (in stage order): the others got one from their name,
/// which a proposal matches to the flow it changes instead ([`super::proposal`]). A copy split off an older check
/// ([`Split`]) counts as written when its stage's id was.
pub fn read_with_written_ids(text: &str) -> Result<(FlowFile, Vec<bool>), FormatError> {
    read_all(text).map(|(file, written, _)| (file, written))
}

/// [`read`], also saying which check stages were split because they listed several agents ([`Split`]).
pub fn read_split(text: &str) -> Result<(FlowFile, Vec<Split>), FormatError> {
    read_all(text).map(|(file, _, splits)| (file, splits))
}

/// An older flow's agent check that listed several agents (`agents: [Security reviewer, Validator]`), as read: one
/// check stage per agent, in the listed order, one after the other. The first keeps the stage's id and name; each
/// other copy gets the id `<id>-<agent>` (the agent's name as an id: `security-validator`, with `-2` added if that
/// id is taken) and the name `<stage> · <agent>` ("Security · Validator", with " 2" added if that name is taken), so
/// every read of the same document gives the same ids. Each copy keeps the stage's kind, skill, rounds, fails go to,
/// options and why, and shows in the first copy's column (`with-previous`). The split is never stored by reading:
/// a document written from the flow ([`write`]) holds it, so the user's next change saves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Split {
    /// The stage's id, which its first copy keeps.
    pub stage: String,
    /// The other copies' ids, in order.
    pub copies: Vec<String>,
}

fn read_all(text: &str) -> Result<(FlowFile, Vec<bool>, Vec<Split>), FormatError> {
    let raw: RawFile = serde_saphyr::from_str(text).map_err(format_error)?;
    let agents = raw.agents.into_iter().map(agent).collect();
    let mut stages = Vec::with_capacity(raw.flow.len());
    let mut others = Vec::with_capacity(raw.flow.len());
    for raw in raw.flow {
        let (stage, more) = stage(raw)?;
        stages.push(stage);
        others.push(more);
    }
    let mut written: Vec<bool> = stages.iter().map(|s| !s.id.is_empty()).collect();
    give_ids(&mut stages);
    fail_to_ids(&mut stages)?;
    let splits = split(&mut stages, &mut written, others);
    Ok((
        FlowFile {
            agents,
            flow: Flow { stages },
        },
        written,
        splits,
    ))
}

/// Splits each agent check that listed several agents ([`Split`]): `others[i]` are stage `i`'s agents after its
/// first. Runs once every stage has its id and its fails go to are ids, so a copy keeps them.
fn split(stages: &mut Vec<Stage>, written: &mut Vec<bool>, others: Vec<Vec<String>>) -> Vec<Split> {
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
        for agent in more {
            let id = unique(&format!("{}-{}", first.id, slug(&agent)), "-", |id| {
                stages.iter().any(|s| s.id == id)
            });
            let name = unique(&format!("{} · {agent}", first.name), " ", |name| {
                stages.iter().any(|s| s.name == name)
            });
            let copy = Stage {
                id: id.clone(),
                name,
                column: Column::WithPrevious,
                agent: Some(agent),
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

/// Reads an agents file (`otaq-agents.yaml`, AGM-02): its `agents:`, as a flow file's.
pub fn read_agents(text: &str) -> Result<Vec<FlowAgent>, FormatError> {
    let raw: RawAgentsFile = serde_saphyr::from_str(text).map_err(format_error)?;
    Ok(raw.agents.into_iter().map(agent).collect())
}

/// Writes an agents file in Otaq's layout (a flow file's `agents:`).
pub fn write_agents(agents: &[FlowAgent]) -> String {
    let mut out = String::new();
    write_agents_into(&mut out, agents);
    out
}

fn write_agents_into(out: &mut String, agents: &[FlowAgent]) {
    if agents.is_empty() {
        return;
    }
    out.push_str("agents:\n");
    for agent in agents {
        let mut item = Item::new(out);
        item.field("name", &agent.name);
        if !agent.instructions.is_empty() {
            item.text("instructions", &agent.instructions);
        }
        item.field("model", &agent.model);
        if let Some(why) = &agent.why {
            item.text("why", why);
        }
    }
}

/// Writes a flow file in Otaq's layout (above).
pub fn write(file: &FlowFile) -> String {
    let mut out = String::new();
    write_agents_into(&mut out, &file.agents);
    out.push_str("flow:\n");
    for stage in &file.flow.stages {
        let mut item = Item::new(&mut out);
        item.field("id", &stage.id);
        item.field("name", &stage.name);
        item.field("kind", stage.kind.as_str());
        if let StageKind::Check { by } = stage.kind {
            item.field("by", by.as_str());
        }
        item.field("column", stage.column.as_str());
        if let Some(agent) = &stage.agent {
            item.field("agent", agent);
        }
        if let Some(skill) = &stage.skill {
            item.field("skill", skill);
        }
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
        if let Some(delivery) = stage.delivery {
            item.field("mode", delivery.mode.as_str());
            item.field("delivers", delivery.delivers.as_str());
        }
        if let Some(description) = &stage.description {
            item.text("description", description);
        }
    }
    out
}

/// A stage from its raw fields: the kind's own fields must be there, and no other kind's. With an older agent
/// check's agents after its first (`agents: [Security reviewer, Validator]`), which [`split`] makes copies of.
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
        (RawKind::Delivery, Some(_)) => return refuse("a Delivery stage", "by", "a check"),
        (RawKind::Agent, None) => StageKind::Agent,
        (RawKind::Delivery, None) => StageKind::Delivery,
    };
    let what = match kind {
        StageKind::Agent => "an Agent stage",
        StageKind::Check { by: CheckBy::Agent } => "an agent check",
        StageKind::Check { by: CheckBy::You } => "a check by you",
        StageKind::Delivery => "a Delivery stage",
    };
    if kind != StageKind::Delivery {
        if raw.mode.is_some() {
            return refuse(what, "mode", "the Delivery stage");
        }
        if raw.delivers.is_some() {
            return refuse(what, "delivers", "the Delivery stage");
        }
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
    // An agent check has one agent, named with `agent:`; an older file's `agents: [X]` reads as `agent: X`, and its
    // `agents: [X, Y]` as `agent: X` with a copy of the stage for Y after it ([`split`]).
    let mut more = Vec::new();
    let agent = match (raw.agent, raw.agents) {
        (agent, None) => agent.map(|a| a.trim().to_owned()),
        (Some(_), Some(_)) => {
            return Err(FormatError(format!(
                "{name} is an agent check: name its agent once, with `agent:`"
            )));
        }
        (None, Some(agents)) => {
            let mut agents = agents
                .into_iter()
                .map(|a| a.trim().to_owned())
                .filter(|a| !a.is_empty());
            let first = agents.next();
            for agent in agents {
                if first.as_ref() != Some(&agent) && !more.contains(&agent) {
                    more.push(agent);
                }
            }
            first
        }
    };
    let delivery = match kind {
        StageKind::Delivery => {
            let (Some(mode), Some(delivers)) = (raw.mode, raw.delivers) else {
                return Err(FormatError(format!(
                    "{name} is the Delivery stage: set its `mode` (click or automatic) and what it `delivers` \
                     (local, pr or either)"
                )));
            };
            Some(Delivery { mode, delivers })
        }
        _ => None,
    };
    // Who runs the stage: an agent for an Agent stage, an agent check and a Delivery that can open a PR.
    let runs_agent = match kind {
        StageKind::Agent | StageKind::Check { by: CheckBy::Agent } => true,
        StageKind::Check { by: CheckBy::You } => false,
        StageKind::Delivery => delivery.is_some_and(|d| d.delivers != Delivers::Local),
    };
    // An Agent stage or an agent check with no `skill:` reads (the editor's draft holds one until its skill is
    // picked or written, FED-06); the checks refuse it ("Plan needs a skill", [`super::check`]). So does an agent
    // check with no agent yet (Rule::NoCheckAgent).
    if runs_agent {
        if agent.is_none() && !agent_check {
            return Err(FormatError(format!(
                "{name} is {what}: name the agent that runs it with `agent:`"
            )));
        }
    } else {
        let belongs = "a stage an agent runs";
        let what = if kind == StageKind::Delivery {
            "a Local merge, which runs no agent"
        } else {
            what
        };
        if agent.is_some() {
            return refuse(what, "agent", belongs);
        }
        if raw.skill.is_some() {
            return refuse(what, "skill", belongs);
        }
    }
    if kind == (StageKind::Check { by: CheckBy::Agent }) && raw.rounds.is_none() {
        return Err(FormatError(format!(
            "{name} is an agent check: set its `rounds` (1 to 5)"
        )));
    }
    // `skip` is the only option: the agent names older files list are read and ignored (a ticket picks any agent
    // whose tool has the stage's skill).
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
        column: raw.column.unwrap_or_default(),
        agent,
        skill: raw.skill,
        options,
        rounds: raw.rounds,
        // A stage's name until [`fail_to_ids`] makes it the stage's id.
        fail_to: raw.fail_to.map(|n| n.trim().to_owned()),
        delivery,
        description: why(raw.description),
    };
    Ok((stage, more))
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

/// One `- ` item of a top-level list: the first field opens it, the others line up under it.
struct Item<'a> {
    out: &'a mut String,
    first: bool,
}

impl<'a> Item<'a> {
    fn new(out: &'a mut String) -> Self {
        Self { out, first: true }
    }

    fn key(&mut self, key: &str) {
        self.out.push_str(if self.first { "  - " } else { "    " });
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
                self.out.push_str("      ");
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
        && value
            .chars()
            .all(|c| c.is_alphanumeric() || " -_./()·".contains(c))
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
