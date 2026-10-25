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
//!     options: [Quick developer]
//!   - id: validate
//!     name: Validate
//!     kind: check
//!     by: agent
//!     column: own
//!     agents: [Security reviewer, Performance reviewer]
//!     skill: validate
//!     rounds: 3
//!     fail_to: Dev
//! ```
//!
//! Fields come in a fixed order (stages: `id name kind by column agent agents skill options rounds fail_to mode
//! delivers why`; agents: `name role instructions skills model why`); a field with nothing in it is left out, except
//! `column`. `agents:` is left out when the file defines none. A check's `fail_to` names a stage by its name, as
//! `agent:` names agents; Otaq keeps it by the stage's id, so a rename carries it along, and a stage removed leaves
//! the check on the nearest Agent stage (d0131). Comments are not kept.

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
    role: Option<String>,
    #[serde(default)]
    instructions: String,
    #[serde(default)]
    skills: Vec<String>,
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
    why: Option<String>,
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
        role: a.role,
        instructions: a.instructions,
        skills: a.skills,
        model: a.model,
        why: why(a.why),
    }
}

/// Reads a flow file. The STG-02 rules are [`super::check`]'s; this only refuses what isn't a flow at all.
pub fn read(text: &str) -> Result<FlowFile, FormatError> {
    read_with_written_ids(text).map(|(file, _)| file)
}

/// [`read`], also saying which stages had their id written (in stage order): the others got one from their name,
/// which a proposal matches to the flow it changes instead ([`super::proposal`]).
pub fn read_with_written_ids(text: &str) -> Result<(FlowFile, Vec<bool>), FormatError> {
    let raw: RawFile = serde_saphyr::from_str(text).map_err(format_error)?;
    let agents = raw.agents.into_iter().map(agent).collect();
    let mut stages = Vec::with_capacity(raw.flow.len());
    for raw in raw.flow {
        stages.push(stage(raw)?);
    }
    let written = stages.iter().map(|s| !s.id.is_empty()).collect();
    give_ids(&mut stages);
    fail_to_ids(&mut stages)?;
    Ok((
        FlowFile {
            agents,
            flow: Flow { stages },
        },
        written,
    ))
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
        if let Some(role) = &agent.role {
            item.field("role", role);
        }
        if !agent.instructions.is_empty() {
            item.text("instructions", &agent.instructions);
        }
        item.list("skills", agent.skills.iter().map(String::as_str));
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
        if stage.is_agent_check() {
            item.list("agents", stage.agents.iter().map(String::as_str));
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
        if let Some(why) = &stage.why {
            item.text("why", why);
        }
    }
    out
}

/// A stage from its raw fields: the kind's own fields must be there, and no other kind's.
fn stage(raw: RawStage) -> Result<Stage, FormatError> {
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
    // An agent check lists its check agents (d0130); `agent:` is never read for one.
    if let (true, Some(agent)) = (agent_check, &raw.agent) {
        return Err(FormatError(format!(
            "{name} is an agent check: an agent check lists its agents: `agents: [{agent}]`"
        )));
    }
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
    // check with no check agents yet (Rule::NoCheckAgent).
    if runs_agent {
        if raw.agent.is_none() && !agent_check {
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
        if raw.agent.is_some() {
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
    let options: Vec<StageOption> = raw
        .options
        .into_iter()
        .map(|o| {
            if o == StageOption::SKIP {
                StageOption::Skip
            } else {
                StageOption::Agent(o)
            }
        })
        .collect();
    if !runs_agent && options.iter().any(|o| matches!(o, StageOption::Agent(_))) {
        return Err(FormatError(format!(
            "{name} is {what}: no agent runs it, so its only option can be `skip`"
        )));
    }
    Ok(Stage {
        id: raw.id.map(|id| id.trim().to_owned()).unwrap_or_default(),
        name,
        kind,
        column: raw.column.unwrap_or_default(),
        agent: raw.agent,
        agents: raw
            .agents
            .unwrap_or_default()
            .into_iter()
            .map(|a| a.trim().to_owned())
            .collect(),
        skill: raw.skill,
        options,
        rounds: raw.rounds,
        // A stage's name until [`fail_to_ids`] makes it the stage's id.
        fail_to: raw.fail_to.map(|n| n.trim().to_owned()),
        delivery,
        why: why(raw.why),
    })
}

/// A `why:` as kept: trimmed, none when empty.
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
