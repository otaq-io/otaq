//! Proposals (FLW-03, FLW-04, FLW-05, FLW-09, FLW-10; d0108): a flow document waiting in the flow editor for one project,
//! with where it came from ([`Source`]) and who proposed it ([`ProposedBy`]). A project has at most one open
//! proposal; a new one replaces it ([`Board::open_proposal`]). **During Flow setup** (ASK-03, ASK-04) it keeps one
//! open proposal per flow agent instead, one for each agent tool asked, and the user's own beside them: a new one
//! replaces only the open one from the same tool (the user's, the user's).
//!
//! **The checks** ([`ProposalProblem`]): STG-02's rules ([`super::check()`]), the agents the stages name
//! ([`super::check_agents`]), each agent's model against its tool's list, written stage ids starting with `_`
//! (Otaq's own, Q267), and a stage skill that is a tool's built-in command (`/security-review`) rather than a skill,
//! unless the user has a skill of that name ([`super::check_skills`]). Each problem names its stage or agent. A
//! Project agent's proposal with problems is refused (it can act on them); a user's opens with them marked, and Apply
//! waits for their fixes ([`Fix`]).
//!
//! **A stage naming a skill the user doesn't have** isn't a problem: the flow still applies, and the stage carries
//! the warning "You don't have the skill `<name>`" in the diff's warnings ([`Board::skill_warnings`]). Nor is a stage
//! whose agent's tool doesn't have its skill: it warns "Codex doesn’t have the skill `/<name>`", and the stage is
//! held when it is due to start on that tool. Otaq's own skills never warn.
//!
//! **The draft** ([`super::draft`], FED-02): outside Flow setup the open proposal is the project's draft, which the
//! flow editor and a Project agent's `flow propose` both edit; during it, the user's own one is.
//!
//! **Stage ids** ([`match_ids`]): a written id is kept; otherwise a stage takes the id of the stage with its name in
//! the flow it changes, else of the stage at its position (when that stage's name is gone), so "rename Review to QA"
//! is a rename. The agent never writes ids.
//!
//! **The diff** ([`FlowDiff`]) is against **the flow as it will be**: the waiting change when one exists (FLW-06),
//! else the current flow. It lists the stage changes with when each applies ([`Applies`], FLW-05), the agents it adds
//! or changes with every project that uses them, the import clashes (Keep mine · Take theirs · Add as), warnings, and
//! the tickets whose chosen option it removes. Nothing is stored about it: it is read again on every request, so it
//! always speaks of the board as it is.
//!
//! **Sources** (FLW-08…FLW-10): a document (a Project agent's, or a file the user imports) and a preset bring stages
//! and agents; another project's flow and an earlier version bring stages only (agents are shared; Back never
//! restores agents, second gap review rule 13); the shipped agents file brings agents only.

use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

use super::draft::DraftMemory;
use super::model::{Delivery, Flow, FlowAgent, FlowFile, Stage, StageKind, StageOption};
use super::yaml;
use crate::agent_tools::Tool;
use crate::agents::in_use::StageUse;
use crate::agents::{Agent, AgentModel, ModelCatalog, in_use, shipped};
use crate::board::{Board, BoardError, Refusal};
use crate::store::now_ms;

/// What the flow editor says when nothing differs (FLW-04).
pub const SAME: &str = "This is the same as your current flow.";

/// Where a proposal's document came from: the flow editor's title follows it ("A flow for shop-web", "Changes to
/// billing-api's flow", "Back to shop-web's previous flow", "Use a preset for shop-web").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Source {
    /// A flow document: a Project agent's (`otaq flow propose -f | --stdin`), or a file the user imports (Import
    /// flow…), `file` its name ("team.yaml", for Add as "Rust developer (team.yaml)").
    Document {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        file: Option<String>,
    },
    /// A shipped preset file ("spec-first.yaml"; [`super::presets`]).
    Preset { file: String },
    /// Another project's stages (Copy from another project…, `otaq flow propose --from`): its key and name.
    Project { key: String, name: String },
    /// One of the project's earlier flows (Back to the previous flow): its stages only.
    Previous { version: u32 },
    /// The shipped `otaq-agents.yaml` (Apply the shipped agents file again): its agents only.
    AgentsFile,
}

/// What a proposal can change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// Stages and agents.
    Whole,
    /// The project's stages; agents stay as they are.
    Stages,
    /// Agents; the stages stay as they are.
    Agents,
}

impl Scope {
    pub fn stages(self) -> bool {
        self != Self::Agents
    }

    pub fn agents(self) -> bool {
        self != Self::Stages
    }
}

impl Source {
    pub fn scope(&self) -> Scope {
        match self {
            Self::Document { .. } | Self::Preset { .. } => Scope::Whole,
            Self::Project { .. } | Self::Previous { .. } => Scope::Stages,
            Self::AgentsFile => Scope::Agents,
        }
    }

    /// The file an agent clash's third choice names: Add as "Rust developer (team.yaml)".
    fn file(&self) -> Option<&str> {
        match self {
            Self::Document { file } => file.as_deref(),
            Self::Preset { file } => Some(file),
            _ => None,
        }
    }

    /// The preset file a version applied from it records.
    pub fn preset(&self) -> Option<&str> {
        match self {
            Self::Preset { file } => Some(file),
            _ => None,
        }
    }
}

/// Who proposed it: the user (the app, their own terminal), or a Project agent's session, which gets the answer
/// (Apply, Cancel, Ask for more changes) as its next message (FLW-03; T-0414 delivers it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", rename_all = "snake_case")]
pub enum ProposedBy {
    User,
    ProjectAgent {
        /// The Project agent's id.
        agent: String,
        /// Its session, where the answer goes.
        session: String,
        /// Its name ("Checkout slowness"), for "Changed by Noyan".
        name: String,
        /// The agent tool it runs on: "Recommended by OpenCode" during Flow setup (ASK-03).
        tool: Tool,
    },
}

impl ProposedBy {
    /// The tool of the Project agent that proposed it; `None` for the user's own.
    pub fn tool(&self) -> Option<Tool> {
        match self {
            Self::User => None,
            Self::ProjectAgent { tool, .. } => Some(*tool),
        }
    }
}

/// A proposal to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewProposal {
    /// The flow document. For [`Scope::Stages`] its `agents:` are ignored; for [`Scope::Agents`] its `flow:`.
    pub yaml: String,
    pub source: Source,
    pub proposed_by: ProposedBy,
}

/// What a problem is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum About {
    /// The whole flow ("The flow has no Delivery stage…").
    Flow,
    Stage {
        id: String,
        name: String,
    },
    Agent {
        name: String,
    },
}

/// What the user can do in the flow editor about a problem (FLW-04); Apply takes it as an [`ApplyChoices`] entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fix {
    /// An agent whose model isn't in its tool's list: pick a local model ([`ApplyChoices::models`]).
    PickModel { agent: String },
}

/// One thing wrong with a proposal, naming its stage or agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposalProblem {
    pub about: About,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

/// When a change takes effect (FLW-05, FED-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "when", rename_all = "snake_case")]
pub enum Applies {
    /// A swap to another flow's shape change (FED-08), or any stage change merged into a waiting one (FLW-06).
    QuietBoard,
    /// Removing or reordering stages, or a stage's kind (FED-08): once no ticket is inside the affected span, from
    /// `first` to `last` ([`super::apply::affected_span`]).
    Span { first: String, last: String },
    /// A stage's agent, skill, options or rounds: when that stage next starts.
    StageStarts { stage: String },
    /// A stage's name and column, a Delivery's mode or target: the next card that arrives.
    AtOnce,
    /// An agent's instructions, skills or model: each session's next start; running ones keep theirs (d0088).
    NextSession,
}

impl Applies {
    /// "waits for a quiet board", "applies when Security next starts".
    pub fn text(&self) -> String {
        match self {
            Self::QuietBoard => "waits for a quiet board".to_owned(),
            Self::Span { first, last } if first == last => {
                format!("waits until no ticket is in {first}")
            }
            Self::Span { first, last } => {
                format!("waits until no ticket is between {first} and {last}")
            }
            Self::StageStarts { stage } => format!("applies when {stage} next starts"),
            Self::AtOnce => "applies at once".to_owned(),
            Self::NextSession => "applies to the next session that starts".to_owned(),
        }
    }
}

/// What changes about one stage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    /// A new stage, after `after` (`None`: first).
    Added {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
    },
    Removed,
    /// Its place in the order changed: now after `after` (`None`: first).
    Moved {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        after: Option<String>,
    },
    Renamed {
        from: String,
    },
    /// Agent stage ↔ check, agent ↔ human check, as words ("your check").
    Kind {
        from: String,
        to: String,
    },
    Agent {
        from: Option<String>,
        to: Option<String>,
    },
    /// An agent check's several agents, from before a check had one ([`Self::Agent`] now): still read in a waiting
    /// change stored then, never made.
    Agents {
        from: Vec<String>,
        to: Vec<String>,
    },
    Skill {
        from: Option<String>,
        to: Option<String>,
    },
    Options {
        from: Vec<String>,
        to: Vec<String>,
    },
    Rounds {
        from: Option<u32>,
        to: Option<u32>,
    },
    /// A check's fails-go-to stage (d0131), by name; `None` for the nearest Agent stage before it. `gone`: the stage
    /// it named was removed, so it is the nearest again.
    FailTo {
        from: Option<String>,
        to: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        gone: bool,
    },
    Column {
        from: String,
        to: String,
    },
    Mode {
        from: String,
        to: String,
    },
    Delivers {
        from: String,
        to: String,
    },
    /// What the stage is for: every agent reads it in `prime`, so a new one is a change, applying at once.
    Description {
        from: Option<String>,
        to: Option<String>,
    },
}

impl Change {
    /// Adding, removing or reordering stages, or a stage's kind (FLW-05): what a swap to another flow waits for the
    /// whole board with (FED-08).
    pub fn is_shape(&self) -> bool {
        self.reorders() || matches!(self, Self::Added { .. })
    }

    /// Removing or reordering stages, or a stage's kind (FED-08): it waits for its affected span. Adding a stage
    /// applies at once.
    pub fn reorders(&self) -> bool {
        matches!(self, Self::Removed | Self::Moved { .. } | Self::Kind { .. })
    }

    /// When it applies on its own, with no waiting change (FLW-05, FED-08); `span` is the change's affected span.
    fn applies(&self, stage: &str, span: Option<&super::apply::Span>) -> Applies {
        match self {
            _ if self.reorders() => span.map_or(Applies::AtOnce, |s| Applies::Span {
                first: s.first.clone(),
                last: s.last.clone(),
            }),
            Self::Agent { .. }
            | Self::Agents { .. }
            | Self::Skill { .. }
            | Self::Options { .. }
            | Self::Rounds { .. } => Applies::StageStarts {
                stage: stage.to_owned(),
            },
            _ => Applies::AtOnce,
        }
    }
}

/// One stage change, as the flow editor lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageChange {
    pub stage_id: String,
    /// The stage's name in the proposal (its old one when removed).
    pub stage: String,
    pub change: Change,
    /// "stage QA (your check) after Security", "stage Review → QA", "Security · agent: A → B".
    pub text: String,
    /// What the banner calls it while it waits: "QA after Security", "Review can be skipped".
    pub label: String,
    pub applies: Applies,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentChangeKind {
    /// Not one of the agents yet: AGENTS IT ADDS.
    Added,
    /// An agent with that name, with different content.
    Changed,
    /// An agent with that name and the same content: "Already there".
    Same,
}

/// One field of a changed agent: "model: opus → fable".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentField {
    /// `instructions`, `skills` or `model`.
    pub field: String,
    pub from: String,
    pub to: String,
}

/// An agent of the same name with other content brought in by a file, a preset or the agents file (second gap
/// review rule 3): **Keep mine** (the default) · **Take theirs** · **Add as "<name> (<file>)"**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Clash {
    /// The third choice's name; `None` where it isn't offered (the agents file: Keep mine · Take theirs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add_as: Option<String>,
}

/// An agent the proposal defines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentChange {
    pub name: String,
    pub change: AgentChangeKind,
    /// What differs, for [`AgentChangeKind::Changed`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<AgentField>,
    /// Every stage of every project that uses it now ("Used in: shop-web (Security)").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub used_in: Vec<StageUse>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clash: Option<Clash>,
}

/// The user's pick for an import clash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClashChoice {
    #[default]
    KeepMine,
    TakeTheirs,
    AddAs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Amber,
    Red,
}

/// A plain warning on the proposed flow (FLW-04): amber for a stage that acts without asking, red for an automatic
/// Delivery with no check before it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    pub level: Level,
    /// The stage's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    pub text: String,
}

/// What a proposal changes, against the flow as it will be.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FlowDiff {
    /// Compared with the waiting change (FLW-06) rather than the current flow: only the new part shows.
    pub against_waiting: bool,
    /// Nothing differs: [`SAME`].
    pub same: bool,
    pub stages: Vec<StageChange>,
    /// The agents the document defines, in its order: added, changed, or already there.
    pub agents: Vec<AgentChange>,
    pub warnings: Vec<Warning>,
    /// Apply makes a waiting change, or merges into the one there is.
    pub waits: bool,
}

/// The open proposal of a project, checked and compared now (`flow.proposal`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Proposal {
    /// "Proposal 3".
    pub id: u32,
    /// The project's key and name.
    pub project: String,
    pub project_name: String,
    pub source: Source,
    pub proposed_by: ProposedBy,
    /// The document as proposed, its stage ids matched ([`match_ids`]).
    pub yaml: String,
    pub problems: Vec<ProposalProblem>,
    pub diff: FlowDiff,
    /// The document's why lines, stages in order, then agents: each stage's description, then each agent's `why:`
    /// (FLW-04: where it came from). An agent's is not a change; a stage's description is ([`Change::Description`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub why: Vec<WhyLine>,
    pub created_at: i64,
}

/// One why line: what it is about (a stage or an agent) and its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WhyLine {
    pub about: About,
    pub text: String,
}

/// The why lines of `file`: its stages' (in order), then its agents'.
pub fn why_lines(file: &FlowFile) -> Vec<WhyLine> {
    let stages = file.flow.stages.iter().filter_map(|s| {
        s.description.as_ref().map(|text| WhyLine {
            about: About::Stage {
                id: s.id.clone(),
                name: s.name.clone(),
            },
            text: text.clone(),
        })
    });
    let agents = file.agents.iter().filter_map(|a| {
        a.why.as_ref().map(|text| WhyLine {
            about: About::Agent {
                name: a.name.clone(),
            },
            text: text.clone(),
        })
    });
    stages.chain(agents).collect()
}

impl Proposal {
    pub fn scope(&self) -> Scope {
        self.source.scope()
    }
}

/// The user's answers in the flow editor, sent with Apply: a pick per import clash (by the agent's name; Keep mine when
/// left out), a local model per agent whose model isn't listed (`claude:sonnet`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ApplyChoices {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub clashes: BTreeMap<String, ClashChoice>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub models: BTreeMap<String, String>,
}

fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

/// Gives each proposed stage its permanent id (d0108 "Identity and deleting"): a written id is kept; otherwise the id
/// of `base`'s stage with the same name, else of `base`'s stage at the same position when that stage's name is gone
/// from the proposal, else a new one from its name. `written[i]` says whether stage `i` had its id written. A check's
/// `fail_to` keeps naming the same stage of the proposal under its matched id.
pub fn match_ids(base: &Flow, proposed: &mut Flow, written: &[bool]) {
    let targets: Vec<Option<usize>> = proposed
        .stages
        .iter()
        .map(|s| s.fail_to.as_deref().and_then(|id| proposed.position(id)))
        .collect();
    assign_ids(base, proposed, written);
    for (i, target) in targets.into_iter().enumerate() {
        if let Some(t) = target {
            proposed.stages[i].fail_to = Some(proposed.stages[t].id.clone());
        }
    }
}

fn assign_ids(base: &Flow, proposed: &mut Flow, written: &[bool]) {
    let stages = &mut proposed.stages;
    let mut decided: Vec<bool> = (0..stages.len())
        .map(|i| written.get(i).copied().unwrap_or(false))
        .collect();
    let mut taken: HashSet<String> = stages
        .iter()
        .zip(&decided)
        .filter(|(_, d)| **d)
        .map(|(s, _)| s.id.clone())
        .collect();
    // By name.
    for (stage, decided) in stages.iter_mut().zip(decided.iter_mut()) {
        if *decided {
            continue;
        }
        if let Some(b) = base
            .stages
            .iter()
            .find(|b| same_name(&b.name, &stage.name) && !taken.contains(&b.id))
        {
            stage.id = b.id.clone();
            taken.insert(b.id.clone());
            *decided = true;
        }
    }
    // By position, when that stage's name is gone.
    let names: Vec<String> = stages.iter().map(|s| s.name.to_lowercase()).collect();
    for (i, (stage, decided)) in stages.iter_mut().zip(decided.iter_mut()).enumerate() {
        if *decided {
            continue;
        }
        if let Some(b) = base
            .stages
            .get(i)
            .filter(|b| !taken.contains(&b.id) && !names.contains(&b.name.to_lowercase()))
        {
            stage.id = b.id.clone();
            taken.insert(b.id.clone());
            *decided = true;
        }
    }
    // A new stage: an id from its name that no stage has, or had.
    let base_ids: HashSet<&str> = base.stages.iter().map(|s| s.id.as_str()).collect();
    for (stage, decided) in stages.iter_mut().zip(&decided) {
        if *decided {
            continue;
        }
        let slug = yaml::slug(&stage.name);
        let mut id = slug.clone();
        let mut n = 2;
        while taken.contains(&id) || base_ids.contains(id.as_str()) {
            id = format!("{slug}-{n}");
            n += 1;
        }
        stage.id = id.clone();
        taken.insert(id);
    }
}

/// A stage's kind in the flow editor's words: "agent stage", "agent check", "your check".
pub fn kind_word(kind: StageKind) -> &'static str {
    match kind {
        StageKind::Agent => "agent stage",
        StageKind::Check {
            by: super::CheckBy::Agent,
        } => "agent check",
        StageKind::Check {
            by: super::CheckBy::You,
        } => "your check",
    }
}

fn options_words(stage: &Stage) -> Vec<String> {
    stage
        .options
        .iter()
        .map(|o| o.as_str().to_owned())
        .collect()
}

/// The common ids of `base` and `new` that keep their relative order (a longest common subsequence); the others
/// moved.
fn kept_in_order(base: &[&str], new: &[&str]) -> HashSet<String> {
    let (n, m) = (base.len(), new.len());
    let mut lcs = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i][j] = if base[i] == new[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut kept = HashSet::new();
    while i < n && j < m {
        if base[i] == new[j] {
            kept.insert(base[i].to_owned());
            i += 1;
            j += 1;
        } else if lcs[i + 1][j] >= lcs[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    kept
}

/// A check's fails-go-to stage left unset: the nearest Agent stage before it (d0131), as the flow editor's picker says.
pub const NEAREST: &str = "nearest";

fn or_none(value: Option<&str>) -> String {
    value.unwrap_or("none").to_owned()
}

fn list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_owned()
    } else {
        values.join(", ")
    }
}

/// The stage changes from `base` to `new` (ids matched), in `new`'s order, then the removed stages in `base`'s.
/// `waiting`: a change already waits, so every one of them waits too (FLW-06). A reorder waits for the span it
/// touches once what applies at once has ([`applied_now`], [`super::apply::affected_span`]).
pub fn stage_changes(base: &Flow, new: &Flow, waiting: bool) -> Vec<StageChange> {
    let span = super::apply::affected_span(&applied_now(base, new), new);
    let common_base: Vec<&str> = base
        .stages
        .iter()
        .filter(|s| new.stage(&s.id).is_some())
        .map(|s| s.id.as_str())
        .collect();
    let common_new: Vec<&str> = new
        .stages
        .iter()
        .filter(|s| base.stage(&s.id).is_some())
        .map(|s| s.id.as_str())
        .collect();
    let kept = kept_in_order(&common_base, &common_new);
    let mut changes = Vec::new();
    let mut push = |stage: &Stage, change: Change| {
        let (text, label) = describe(stage, &change);
        let applies = if waiting {
            Applies::QuietBoard
        } else {
            change.applies(&stage.name, span.as_ref())
        };
        changes.push(StageChange {
            stage_id: stage.id.clone(),
            stage: stage.name.clone(),
            change,
            text,
            label,
            applies,
        });
    };
    for (i, stage) in new.stages.iter().enumerate() {
        let after = i
            .checked_sub(1)
            .and_then(|p| new.stages.get(p))
            .map(|s| s.name.clone());
        let Some(old) = base.stage(&stage.id) else {
            push(stage, Change::Added { after });
            continue;
        };
        if old.name != stage.name {
            push(
                stage,
                Change::Renamed {
                    from: old.name.clone(),
                },
            );
        }
        if !kept.contains(&stage.id) {
            push(stage, Change::Moved { after });
        }
        if old.kind != stage.kind {
            push(
                stage,
                Change::Kind {
                    from: kind_word(old.kind).to_owned(),
                    to: kind_word(stage.kind).to_owned(),
                },
            );
            // A new kind brings its own fields: the kind change says it all.
            continue;
        }
        if old.agent != stage.agent {
            push(
                stage,
                Change::Agent {
                    from: old.agent.clone(),
                    to: stage.agent.clone(),
                },
            );
        }
        // By the stages' names, each in its own flow: a rename of the target alone is no change here.
        let target = |flow: &Flow, s: &Stage| {
            s.fail_to
                .as_deref()
                .and_then(|id| flow.stage(id))
                .map(|t| t.name.clone())
        };
        let (was, now) = (target(base, old), target(new, stage));
        let same_id = old.fail_to.is_some() && old.fail_to == stage.fail_to;
        if was != now && !(same_id && now.is_some()) {
            let gone = was.is_some()
                && now.is_none()
                && old
                    .fail_to
                    .as_deref()
                    .is_some_and(|id| new.stage(id).is_none());
            push(
                stage,
                Change::FailTo {
                    from: was,
                    to: now,
                    gone,
                },
            );
        }
        if old.skill != stage.skill {
            push(
                stage,
                Change::Skill {
                    from: old.skill.clone(),
                    to: stage.skill.clone(),
                },
            );
        }
        if old.options != stage.options {
            push(
                stage,
                Change::Options {
                    from: options_words(old),
                    to: options_words(stage),
                },
            );
        }
        if old.rounds != stage.rounds {
            push(
                stage,
                Change::Rounds {
                    from: old.rounds,
                    to: stage.rounds,
                },
            );
        }
        if old.description != stage.description {
            push(
                stage,
                Change::Description {
                    from: old.description.clone(),
                    to: stage.description.clone(),
                },
            );
        }
    }
    for old in base.stages.iter().filter(|s| new.stage(&s.id).is_none()) {
        push(old, Change::Removed);
    }
    changes.extend(delivery_changes(&base.delivery, &new.delivery, waiting));
    changes
}

/// The delivery's line in the stage changes: its id and name ([`crate::agents::facts::DELIVERY_STEP_ID`]).
const DELIVERY_ID: &str = crate::agents::facts::DELIVERY_STEP_ID;
const DELIVERY_NAME: &str = crate::agents::facts::DELIVERY_STEP;

/// What changed in the flow's delivery (`delivery:`): what it delivers, its mode, the agent that opens the PR and its
/// skill, as the stage changes list them under "Delivery". They apply at once: a ticket reads the delivery when it
/// passes its last stage (unless a change already waits, `waiting`).
fn delivery_changes(was: &Delivery, now: &Delivery, waiting: bool) -> Vec<StageChange> {
    let mut found = Vec::new();
    if was.delivers != now.delivers {
        found.push(Change::Delivers {
            from: was.delivers.as_str().to_owned(),
            to: now.delivers.as_str().to_owned(),
        });
    }
    if was.mode != now.mode {
        found.push(Change::Mode {
            from: was.mode.as_str().to_owned(),
            to: now.mode.as_str().to_owned(),
        });
    }
    if was.agent != now.agent {
        found.push(Change::Agent {
            from: was.agent.clone(),
            to: now.agent.clone(),
        });
    }
    if was.skill != now.skill {
        found.push(Change::Skill {
            from: was.skill.clone(),
            to: now.skill.clone(),
        });
    }
    found
        .into_iter()
        .map(|change| {
            let (text, label) = describe_named(DELIVERY_NAME, StageKind::Agent, &change);
            StageChange {
                stage_id: DELIVERY_ID.to_owned(),
                stage: DELIVERY_NAME.to_owned(),
                change,
                text,
                label,
                applies: match waiting {
                    true => Applies::QuietBoard,
                    false => Applies::AtOnce,
                },
            }
        })
        .collect()
}

/// The flow editor's line and the banner's label for one change.
fn describe(stage: &Stage, change: &Change) -> (String, String) {
    describe_named(&stage.name, stage.kind, change)
}

/// [`describe`] by the stage's name and kind.
fn describe_named(name: &str, kind: StageKind, change: &Change) -> (String, String) {
    let detail = |field: &str, from: String, to: String| {
        format!("{name} \u{b7} {field}: {from} \u{2192} {to}")
    };
    match change {
        Change::Added { after } => {
            let place = match after {
                Some(after) => format!("after {after}"),
                None => "first".to_owned(),
            };
            (
                format!("stage {name} ({}) {place}", kind_word(kind)),
                format!("{name} {place}"),
            )
        }
        Change::Removed => (format!("stage {name} removed"), format!("{name} removed")),
        Change::Moved { after } => {
            let place = match after {
                Some(after) => format!("after {after}"),
                None => "first".to_owned(),
            };
            (
                format!("stage {name} moved {place}"),
                format!("{name} moved {place}"),
            )
        }
        Change::Renamed { from } => (
            format!("stage {from} \u{2192} {name}"),
            format!("{from} \u{2192} {name}"),
        ),
        Change::Kind { from, to } => {
            let text = format!("{name}: {from} \u{2192} {to}");
            (text.clone(), text)
        }
        Change::Agent { from, to } => {
            let text = detail("agent", or_none(from.as_deref()), or_none(to.as_deref()));
            (text.clone(), text)
        }
        Change::Skill { from, to } => {
            let text = detail("skill", or_none(from.as_deref()), or_none(to.as_deref()));
            (text.clone(), text)
        }
        Change::Agents { from, to } => {
            let text = detail("agents", list_or_none(from), list_or_none(to));
            (text.clone(), text)
        }
        Change::FailTo {
            to: None,
            gone: true,
            ..
        } => {
            let text = format!("{name}'s fails go to the nearest Agent stage now");
            (text.clone(), text)
        }
        Change::FailTo { from, to, .. } => {
            let text = detail(
                "fails go to",
                from.clone().unwrap_or_else(|| NEAREST.to_owned()),
                to.clone().unwrap_or_else(|| NEAREST.to_owned()),
            );
            (text.clone(), text)
        }
        Change::Options { from, to } => {
            let text = detail("options", list_or_none(from), list_or_none(to));
            let skip = StageOption::SKIP.to_owned();
            let label = if to.contains(&skip) && !from.contains(&skip) {
                format!("{name} can be skipped")
            } else if from.contains(&skip) && !to.contains(&skip) {
                format!("{name} can't be skipped")
            } else {
                text.clone()
            };
            (text, label)
        }
        Change::Rounds { from, to } => {
            let show = |r: &Option<u32>| r.map_or("none".to_owned(), |r| r.to_string());
            let text = detail("rounds", show(from), show(to));
            (text.clone(), text)
        }
        Change::Column { from, to } | Change::Mode { from, to } | Change::Delivers { from, to } => {
            let field = match change {
                Change::Column { .. } => "column",
                Change::Mode { .. } => "mode",
                _ => "delivers",
            };
            let text = detail(field, from.clone(), to.clone());
            (text.clone(), text)
        }
        // The text itself can be long: the change only says what happened to it.
        Change::Description { from, to } => {
            let text = match (from, to) {
                (None, _) => format!("{name} \u{b7} description added"),
                (_, None) => format!("{name} \u{b7} description removed"),
                _ => format!("{name} \u{b7} description changed"),
            };
            (text.clone(), text)
        }
    }
}

/// `base` with `new`'s changes that don't wait (FED-08): its edits ([`edits_only`]) and its added stages, each just
/// after the stage before it in `new` (first when none is); no stage is removed or moved. What applies now while a
/// removal or reorder waits for its span. Plan added after Product spec: SHOP-12 in Product spec goes to Plan next,
/// SHOP-33 in Dev never meets it.
pub fn applied_now(base: &Flow, new: &Flow) -> Flow {
    let mut flow = edits_only(base, new);
    for (i, stage) in new.stages.iter().enumerate() {
        if base.stage(&stage.id).is_some() {
            continue;
        }
        let at = new.stages[..i]
            .iter()
            .rev()
            .find_map(|s| flow.position(&s.id))
            .map_or(0, |p| p + 1);
        flow.stages.insert(at, stage.clone());
    }
    flow
}

/// `base` with `new`'s edits (FLW-05): each stage `new` keeps with the same kind takes its new fields (name, agent,
/// skill, options, rounds, description), and the delivery is `new`'s; stages stay where they are, none is added or
/// removed. What a swap to another flow applies now while the rest waits for a quiet board (FED-08).
pub fn edits_only(base: &Flow, new: &Flow) -> Flow {
    Flow {
        delivery: new.delivery.clone(),
        stages: base
            .stages
            .iter()
            .map(|old| match new.stage(&old.id) {
                Some(now) if now.kind == old.kind => now.clone(),
                _ => old.clone(),
            })
            .collect(),
    }
}

/// The warning on a stage whose `skill:` names a skill the user doesn't have: "You don't have the skill `<name>`".
pub fn unknown_skill_text(skill: &str) -> String {
    format!("You don't have the skill `{skill}`")
}

/// The warning on a stage whose agent's tool doesn't have the stage's skill, and Settings → Agents' "What this agent
/// reads" for such a skill: "Copilot doesn’t have the skill `/team-validate`".
pub fn missing_skill_text(tool: Tool, skill: &str) -> String {
    format!(
        "{} doesn\u{2019}t have the skill `/{skill}`",
        tool.display_name()
    )
}

impl Board {
    /// One amber warning per stage of `file` whose `skill:` the stage can't run with: a skill neither Otaq ships nor
    /// the user has in `project` ([`unknown_skill_text`]), or one the tool of the stage's agent doesn't have
    /// ([`missing_skill_text`]; such a stage is held until the tool has it or the ticket picks another agent). Only
    /// warnings: the flow still applies. Otaq's own skills never warn. The agent's tool is a stored agent's (`stored`,
    /// on the default model when it names none), else the one the document brings.
    pub async fn skill_warnings(
        &self,
        project: &str,
        file: &FlowFile,
        stored: &[Agent],
    ) -> Vec<Warning> {
        let default = self.store().default_model().await.ok().flatten();
        let tool_of = |name: &str| -> Option<Tool> {
            let model = match stored.iter().find(|a| a.name.eq_ignore_ascii_case(name)) {
                Some(agent) => agent.model.clone(),
                None => file
                    .agents
                    .iter()
                    .find(|a| a.name.eq_ignore_ascii_case(name))
                    .and_then(|a| shipped::model_of(&a.model).ok())?,
            };
            model.on(default.as_ref()).map(|on| on.tool)
        };
        let mut out = Vec::new();
        // Every stage's skill, and the delivery's PR skill.
        let delivery = &file.flow.delivery;
        let uses = file
            .flow
            .stages
            .iter()
            .map(|s| (s.name.as_str(), s.agent.as_deref(), s.skill.as_deref()))
            .chain(std::iter::once((
                DELIVERY_NAME,
                delivery.agent.as_deref(),
                delivery.skill.as_deref(),
            )));
        for (name, agent, skill) in uses {
            let Some(skill) = skill.map(str::trim) else {
                continue;
            };
            if skill.is_empty() {
                continue;
            }
            let tools = self.skill_tools(project, None, skill).await;
            // A built-in command nobody has as a skill is a problem already ([`super::check_skills`]).
            if tools.is_empty() && !Tool::with_built_in_command(skill).is_empty() {
                continue;
            }
            let text = if tools.is_empty() {
                unknown_skill_text(skill)
            } else {
                match agent.and_then(tool_of) {
                    Some(tool) if !tools.contains(&tool) => missing_skill_text(tool, skill),
                    _ => continue,
                }
            };
            out.push(Warning {
                level: Level::Amber,
                stage: Some(name.to_owned()),
                text,
            });
        }
        out
    }
}

/// The warnings on a flow (FLW-04): an automatic delivery acts without asking (amber), or with no check before it
/// (red). `branch` is what a Local merge merges into, `provider` where a PR opens ("GitHub").
pub fn warnings(flow: &Flow, branch: &str, provider: Option<&str>) -> Vec<Warning> {
    let delivery = &flow.delivery;
    if delivery.mode != super::DeliveryMode::Automatic {
        return Vec::new();
    }
    let what = match delivery.delivers {
        super::Delivers::Local => format!("merges into {branch}"),
        super::Delivers::Pr | super::Delivers::Either => match provider {
            Some(provider) => format!("opens a PR on {provider}"),
            None => "opens a PR".to_owned(),
        },
    };
    let checked = flow
        .stages
        .iter()
        .any(|s| matches!(s.kind, StageKind::Check { .. }));
    let name = DELIVERY_NAME;
    vec![if checked {
        Warning {
            level: Level::Amber,
            stage: Some(name.to_owned()),
            text: format!("{name} {what} without asking you."),
        }
    } else {
        Warning {
            level: Level::Red,
            stage: Some(name.to_owned()),
            text: format!("{name} {what} without any check before it."),
        }
    }]
}

/// A stored agent as a flow file writes it: its model as `default`, `<tool>` or `<tool>:<model>`.
pub fn flow_agent_of(agent: &Agent) -> FlowAgent {
    FlowAgent {
        name: agent.name.clone(),
        instructions: agent.instructions.clone(),
        model: model_text(&agent.model),
        why: None,
    }
}

/// `default`, `claude`, `claude:opus`.
pub fn model_text(model: &AgentModel) -> String {
    match model {
        AgentModel::Default => "default".to_owned(),
        AgentModel::Tool { tool, model: None } => tool.as_str().to_owned(),
        AgentModel::Tool {
            tool,
            model: Some(m),
        } => format!("{}:{m}", tool.as_str()),
    }
}

/// The proposal's document as the checks and the diff read it: for a stages-only source its agents go, for the
/// agents file its stages are `base`'s; `choices` set the picked local models and rename the agents added under
/// another name (Add as).
pub(crate) fn effective_file(
    file: &FlowFile,
    scope: Scope,
    base: &Flow,
    source: &Source,
    choices: &ApplyChoices,
) -> FlowFile {
    let mut file = file.clone();
    match scope {
        Scope::Stages => file.agents.clear(),
        Scope::Agents => file.flow = base.clone(),
        Scope::Whole => {}
    }
    for agent in &mut file.agents {
        if let Some((_, model)) = choices
            .models
            .iter()
            .find(|(name, _)| same_name(name, &agent.name))
        {
            agent.model = model.clone();
        }
    }
    for (name, choice) in &choices.clashes {
        if *choice != ClashChoice::AddAs {
            continue;
        }
        if let Some(file_name) = source.file() {
            super::store::rename_agent(&mut file, name, &add_as_name(name, file_name));
        }
    }
    file
}

/// Add as "Security reviewer (team.yaml)".
pub fn add_as_name(name: &str, file: &str) -> String {
    format!("{name} ({file})")
}

impl Board {
    /// The flow a new proposal for `project` is compared with (FLW-06): the waiting change's when one exists, else
    /// the current one. Whether it is the waiting one.
    pub async fn flow_as_it_will_be(&self, project: &str) -> Result<(FlowFile, bool), BoardError> {
        if let Some(waiting) = self.waiting_change(project).await? {
            let file = yaml::read(&waiting.yaml).map_err(|e| {
                BoardError::NotFound(format!("{project}'s waiting flow change doesn't read: {e}"))
            })?;
            return Ok((file, true));
        }
        Ok((self.current_flow_file(project).await?, false))
    }

    /// Opens a proposal for `project`, replacing the open one (FLW-03; during Flow setup only the open one from the
    /// same tool, ASK-03): its stage ids matched to the flow as it will be, then checked. A Project agent's proposal with problems is refused with them, one per line, so it can fix
    /// them; a document that doesn't read is refused for everyone. The proposal as the flow editor shows it.
    pub async fn open_proposal(
        &self,
        project: &str,
        new: &NewProposal,
        models: &dyn ModelCatalog,
    ) -> Result<Proposal, BoardError> {
        let (file, written) = yaml::read_with_written_ids(&new.yaml).map_err(|e| {
            BoardError::refused(Refusal::BadFlow, format!("The flow doesn't read: {e}"))
        })?;
        self.open_read_proposal(
            project,
            file,
            &written,
            &new.source,
            &new.proposed_by,
            &DraftMemory::default(),
            models,
        )
        .await
    }

    /// [`Self::open_proposal`] for a document already read; `written[i]` says whether stage `i` keeps its id, `memory`
    /// what the draft remembers with it (a Start from pick is a swap, FED-08).
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn open_read_proposal(
        &self,
        project: &str,
        mut file: FlowFile,
        written: &[bool],
        source: &Source,
        proposed_by: &ProposedBy,
        memory: &DraftMemory,
        models: &dyn ModelCatalog,
    ) -> Result<Proposal, BoardError> {
        let refuse = |message: String| BoardError::refused(Refusal::BadFlow, message);
        let project = self.project(project).await?;
        let (base, _) = self.flow_as_it_will_be(&project.key).await?;
        match source.scope() {
            Scope::Agents => file.flow = base.flow.clone(),
            _ => match_ids(&base.flow, &mut file.flow, written),
        }
        let text = yaml::write(&file);
        let row = Row {
            id: 0,
            project: project.key.clone(),
            yaml: text,
            source: source.clone(),
            proposed_by: proposed_by.clone(),
            draft: memory.clone(),
            created_at: now_ms(),
        };
        let proposal = self.view(&row, models, &ApplyChoices::default()).await?;
        if matches!(proposed_by, ProposedBy::ProjectAgent { .. }) && !proposal.problems.is_empty() {
            let lines: Vec<&str> = proposal
                .problems
                .iter()
                .map(|p| p.message.as_str())
                .collect();
            return Err(refuse(lines.join("\n")));
        }
        let tool = proposed_by.tool().map(Tool::as_str);
        let mut tx = self.store().begin_write().await?;
        // Outside Flow setup one proposal per project (FLW-03); during it one per tool, and the user's (ASK-03).
        let replaced = if project.flow_setup_since.is_some() {
            sqlx::query("DELETE FROM flow_proposals WHERE project_id = ? AND tool IS ?")
                .bind(&project.id)
                .bind(tool)
        } else {
            sqlx::query("DELETE FROM flow_proposals WHERE project_id = ?").bind(&project.id)
        };
        replaced.execute(&mut *tx).await?;
        let id: i64 = sqlx::query_scalar(
            "INSERT INTO flow_proposals (project_id, tool, yaml, source_json, proposed_by_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?) RETURNING id",
        )
        .bind(&project.id)
        .bind(tool)
        .bind(&row.yaml)
        .bind(row.source_json())
        .bind(serde_json::to_string(&row.proposed_by).expect("a proposer serializes"))
        .bind(row.created_at)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(Proposal {
            id: u32::try_from(id).unwrap_or(0),
            ..proposal
        })
    }

    /// `flow.proposal`: the project's open proposal, checked and compared now; `None` when there is none. During Flow
    /// setup, the one the flow editor shows ([`Self::proposal_row`]); [`Self::proposals`] lists them all.
    pub async fn proposal(
        &self,
        project: &str,
        models: &dyn ModelCatalog,
    ) -> Result<Option<Proposal>, BoardError> {
        match self.proposal_row(project).await? {
            Some(row) => Ok(Some(
                self.view(&row, models, &ApplyChoices::default()).await?,
            )),
            None => Ok(None),
        }
    }

    /// Every open proposal of `project`, oldest first, checked and compared now: one at most outside Flow setup, one
    /// per flow agent and the user's during it (ASK-03).
    pub async fn proposals(
        &self,
        project: &str,
        models: &dyn ModelCatalog,
    ) -> Result<Vec<Proposal>, BoardError> {
        let mut proposals = Vec::new();
        for row in self.proposal_rows(project).await? {
            proposals.push(self.view(&row, models, &ApplyChoices::default()).await?);
        }
        Ok(proposals)
    }

    /// `flow.cancel_proposal`: drops the open proposal (nothing else changes). The one it dropped.
    pub async fn cancel_proposal(&self, project: &str) -> Result<Option<u32>, BoardError> {
        let Some(row) = self.proposal_row(project).await? else {
            return Ok(None);
        };
        self.drop_proposal(row.id).await?;
        Ok(Some(row.id))
    }

    pub(crate) async fn drop_proposal(&self, id: u32) -> Result<(), BoardError> {
        sqlx::query("DELETE FROM flow_proposals WHERE id = ?")
            .bind(i64::from(id))
            .execute(self.store().pool())
            .await?;
        Ok(())
    }

    /// The open proposal the flow editor shows: the only one outside Flow setup; during it the user's own draft (a
    /// preset, or a tool's entry copied in, FED-02) when there is one, else the newest.
    pub(crate) async fn proposal_row(&self, project: &str) -> Result<Option<Row>, BoardError> {
        if let Some(draft) = self.draft_row(project).await? {
            return Ok(Some(draft));
        }
        Ok(self.proposal_rows(project).await?.into_iter().last())
    }

    /// The open proposal `id` of `project`, if it is open.
    pub(crate) async fn proposal_row_by_id(
        &self,
        project: &str,
        id: u32,
    ) -> Result<Option<Row>, BoardError> {
        Ok(self
            .proposal_rows(project)
            .await?
            .into_iter()
            .find(|r| r.id == id))
    }

    /// Every open proposal of `project`, oldest first.
    pub(crate) async fn proposal_rows(&self, project: &str) -> Result<Vec<Row>, BoardError> {
        let project = self.project(project).await?;
        let rows: Vec<(i64, String, String, String, i64)> = sqlx::query_as(
            "SELECT id, yaml, source_json, proposed_by_json, created_at FROM flow_proposals WHERE project_id = ?
             ORDER BY id",
        )
        .bind(&project.id)
        .fetch_all(self.store().pool())
        .await?;
        rows.into_iter()
            .map(|(id, yaml, source, proposed_by, created_at)| {
                let read = |what: &str, e: serde_json::Error| {
                    BoardError::NotFound(format!("proposal {id}'s {what} doesn't read: {e}"))
                };
                let stored: StoredSource =
                    serde_json::from_str(&source).map_err(|e| read("source", e))?;
                Ok(Row {
                    id: u32::try_from(id).unwrap_or(0),
                    project: project.key.clone(),
                    yaml,
                    source: stored.source,
                    proposed_by: serde_json::from_str(&proposed_by)
                        .map_err(|e| read("proposer", e))?,
                    draft: stored.draft,
                    created_at,
                })
            })
            .collect()
    }

    /// The proposal `row` checked and compared now, with the user's `choices` taken (Apply's own check).
    pub(crate) async fn view(
        &self,
        row: &Row,
        models: &dyn ModelCatalog,
        choices: &ApplyChoices,
    ) -> Result<Proposal, BoardError> {
        let project = self.project(&row.project).await?;
        let (base, against_waiting) = self.flow_as_it_will_be(&project.key).await?;
        let read = yaml::read(&row.yaml).map_err(|e| {
            BoardError::refused(Refusal::BadFlow, format!("The flow doesn't read: {e}"))
        })?;
        let scope = row.source.scope();
        let file = effective_file(&read, scope, &base.flow, &row.source, choices);
        let stored = self.store().agents().await?;
        // A stage naming a tool's built-in command is refused unless the user has a skill of that name.
        let mut had = HashSet::new();
        for skill in file.flow.stages.iter().filter_map(|s| s.skill.as_deref()) {
            let skill = skill.trim().trim_start_matches('/');
            if !Tool::with_built_in_command(skill).is_empty()
                && self.has_skill(&project.key, skill).await
            {
                had.insert(skill.to_owned());
            }
        }
        let problems = problems(&file, scope, &stored, models, |skill| had.contains(skill));

        let mut stages = if scope.stages() {
            stage_changes(&base.flow, &file.flow, against_waiting)
        } else {
            Vec::new()
        };
        // A swap to another flow waits for the whole board (FED-08): its shape changes say so.
        if row.draft.swap {
            for change in stages.iter_mut().filter(|c| c.change.is_shape()) {
                change.applies = Applies::QuietBoard;
            }
        }
        let mut agents = Vec::new();
        for agent in &file.agents {
            agents.push(self.agent_change(agent, row, &stored).await?);
        }
        let warnings = if scope.stages() {
            let provider = self
                .pr_provider_from_remote(&project.key)
                .await
                .ok()
                .and_then(|p| p.provider)
                .map(|p| p.name());
            let mut found = warnings(&file.flow, &project.base_branch, provider);
            found.extend(self.skill_warnings(&project.key, &file, &stored).await);
            found
        } else {
            Vec::new()
        };
        let same = stages.is_empty() && agents.iter().all(|a| a.change == AgentChangeKind::Same);
        let waits = !stages.is_empty()
            && (against_waiting
                || stages.iter().any(|c| match row.draft.swap {
                    true => c.change.is_shape(),
                    false => c.change.reorders(),
                }));
        Ok(Proposal {
            id: row.id,
            project: project.key.clone(),
            project_name: project.name.clone(),
            source: row.source.clone(),
            proposed_by: row.proposed_by.clone(),
            yaml: row.yaml.clone(),
            problems,
            diff: FlowDiff {
                against_waiting,
                same,
                stages,
                agents,
                warnings,
                waits,
            },
            why: why_lines(&file),
            created_at: row.created_at,
        })
    }

    /// One agent of the document against the stored agent of its name.
    async fn agent_change(
        &self,
        agent: &FlowAgent,
        row: &Row,
        stored: &[Agent],
    ) -> Result<AgentChange, BoardError> {
        let Some(mine) = stored.iter().find(|a| same_name(&a.name, &agent.name)) else {
            return Ok(AgentChange {
                name: agent.name.clone(),
                change: AgentChangeKind::Added,
                fields: Vec::new(),
                used_in: Vec::new(),
                clash: None,
            });
        };
        let ours = flow_agent_of(mine);
        let mut fields = Vec::new();
        if ours.instructions.trim() != agent.instructions.trim() {
            fields.push(AgentField {
                field: "instructions".into(),
                from: ours.instructions.clone(),
                to: agent.instructions.clone(),
            });
        }
        let model_differs = match shipped::model_of(&agent.model) {
            Ok(model) => model != mine.model,
            Err(_) => true,
        };
        if model_differs {
            fields.push(AgentField {
                field: "model".into(),
                from: ours.model.clone(),
                to: agent.model.trim().to_owned(),
            });
        }
        if fields.is_empty() {
            return Ok(AgentChange {
                name: mine.name.clone(),
                change: AgentChangeKind::Same,
                fields,
                used_in: Vec::new(),
                clash: None,
            });
        }
        let used_in = in_use::in_use(self, mine).await?.stages;
        // A Project agent's own document changes the agent, and so does the user's own (the editor's draft; their
        // terminal); anything brought in (a file, a preset) clashes, Keep mine by default.
        let agent_document = match (&row.proposed_by, &row.source) {
            (ProposedBy::ProjectAgent { .. }, Source::Document { .. }) => true,
            (ProposedBy::User, Source::Document { file }) => file.is_none(),
            _ => false,
        };
        let clash = (!agent_document).then(|| Clash {
            add_as: row.source.file().map(|f| add_as_name(&mine.name, f)),
        });
        Ok(AgentChange {
            name: mine.name.clone(),
            change: AgentChangeKind::Changed,
            fields,
            used_in,
            clash,
        })
    }
}

/// A stored proposal.
#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub id: u32,
    pub project: String,
    pub yaml: String,
    pub source: Source,
    pub proposed_by: ProposedBy,
    /// What the draft remembers besides its document (FED-02).
    pub draft: DraftMemory,
    pub created_at: i64,
}

impl Row {
    /// Its `source_json`: the source, and the draft's memory beside it ([`StoredSource`]).
    pub(crate) fn source_json(&self) -> String {
        serde_json::to_string(&StoredSource {
            source: self.source.clone(),
            draft: self.draft.clone(),
        })
        .expect("a source serializes")
    }
}

/// A proposal row's `source_json`: its [`Source`], with the draft's memory beside the source's own fields (FED-02;
/// no column of its own, M38 adds no migration for it).
#[derive(Serialize, Deserialize)]
struct StoredSource {
    #[serde(flatten)]
    source: Source,
    #[serde(default, skip_serializing_if = "DraftMemory::is_empty")]
    draft: DraftMemory,
}

/// Every problem of a proposal's document (FLW-03), each naming its stage or agent.
pub fn problems(
    file: &FlowFile,
    scope: Scope,
    stored: &[Agent],
    models: &dyn ModelCatalog,
    has_skill: impl Fn(&str) -> bool,
) -> Vec<ProposalProblem> {
    let mut out = Vec::new();
    let stage_about = |id: &str| {
        let name = file
            .flow
            .stage(id)
            .map_or_else(|| id.to_owned(), |s| s.name.clone());
        About::Stage {
            id: id.to_owned(),
            name,
        }
    };
    if scope.stages() {
        for stage in file.flow.stages.iter().filter(|s| s.id.starts_with('_')) {
            out.push(ProposalProblem {
                about: stage_about(&stage.id),
                message: format!(
                    "{} has the id `{}`: ids starting with _ are {}'s own, so leave the id out or pick another",
                    stage.name,
                    stage.id,
                    crate::naming::PRODUCT_NAME
                ),
                fix: None,
            });
        }
        if let Err(found) = super::check(&file.flow) {
            out.extend(found.into_iter().map(|p| ProposalProblem {
                about: p.stage.as_deref().map_or(About::Flow, stage_about),
                message: p.message,
                fix: None,
            }));
        }
        let known = |name: &str| {
            stored.iter().any(|a| same_name(&a.name, name))
                || file.agents.iter().any(|a| same_name(&a.name, name))
        };
        if let Err(found) = super::check_agents(&file.flow, known) {
            out.extend(found.into_iter().map(|p| ProposalProblem {
                about: p.stage.as_deref().map_or(About::Flow, stage_about),
                message: p.message,
                fix: None,
            }));
        }
        if let Err(found) = super::check_skills(&file.flow, has_skill) {
            out.extend(found.into_iter().map(|p| ProposalProblem {
                about: p.stage.as_deref().map_or(About::Flow, stage_about),
                message: p.message,
                fix: None,
            }));
        }
    }
    if scope.agents() {
        for agent in &file.agents {
            out.extend(agent_problems(agent, models));
        }
    }
    out
}

/// An agent's problems: its name, or its model doesn't read or isn't in its tool's list.
fn agent_problems(agent: &FlowAgent, models: &dyn ModelCatalog) -> Vec<ProposalProblem> {
    let mut out = Vec::new();
    let about = || About::Agent {
        name: agent.name.clone(),
    };
    if let Err(why) = crate::agents::checked_name(&agent.name, std::iter::empty::<&str>()) {
        out.push(ProposalProblem {
            about: about(),
            message: format!("The agent {}: {why}", agent.name),
            fix: None,
        });
    }
    match shipped::model_of(&agent.model) {
        Err(why) => out.push(ProposalProblem {
            about: about(),
            message: format!("{}'s model: {why}", agent.name),
            fix: Some(Fix::PickModel {
                agent: agent.name.clone(),
            }),
        }),
        Ok(AgentModel::Tool {
            tool,
            model: Some(model),
        }) if models.pick(tool, &model).is_none() => out.push(ProposalProblem {
            about: about(),
            message: format!(
                "{}'s model {model} isn't in {}'s model list: pick a model from it",
                agent.name,
                tool.display_name()
            ),
            fix: Some(Fix::PickModel {
                agent: agent.name.clone(),
            }),
        }),
        Ok(_) => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shipped default flow without its stages' descriptions, as the documents these tests write have none.
    fn default_flow() -> Flow {
        let mut flow = crate::flow::default_flow();
        for stage in &mut flow.stages {
            stage.description = None;
        }
        flow
    }

    fn names(flow: &Flow) -> Vec<(&str, &str)> {
        flow.stages
            .iter()
            .map(|s| (s.id.as_str(), s.name.as_str()))
            .collect()
    }

    fn read(text: &str, base: &Flow) -> Flow {
        let (file, written) = yaml::read_with_written_ids(text).unwrap();
        let mut flow = file.flow;
        match_ids(base, &mut flow, &written);
        flow
    }

    const DEFAULT_RENAMED: &str = "flow:
  - name: Dev
    kind: agent
    agent: Developer
    skill: work-ticket
  - name: Validate
    kind: check
    by: agent
    agents: [Validator]
    skill: validate
    rounds: 3
  - name: QA
    kind: check
    by: you
  - name: Done
    kind: delivery
    column: with-previous
    agent: Developer
    skill: open-pr
    mode: click
    delivers: either
";

    /// "Rename Review to QA" keeps Review's id: a rename, applied at once, nothing waits.
    #[test]
    fn a_rename_is_a_rename() {
        let base = default_flow();
        let new = read(DEFAULT_RENAMED, &base);
        assert_eq!(
            names(&new),
            [("dev", "Dev"), ("validate", "Validate"), ("review", "QA"),]
        );
        let changes = stage_changes(&base, &new, false);
        assert_eq!(changes.len(), 1, "{changes:?}");
        assert_eq!(
            changes[0].change,
            Change::Renamed {
                from: "Review".into()
            }
        );
        assert_eq!(changes[0].text, "stage Review \u{2192} QA");
        assert_eq!(changes[0].applies, Applies::AtOnce);
        assert!(!changes[0].change.is_shape());
    }

    /// Adding QA after Review is a shape change with a new id, and it applies at once (FED-08).
    #[test]
    fn adding_qa_applies_at_once() {
        let base = default_flow();
        let text = DEFAULT_RENAMED.replace(
            "  - name: QA\n    kind: check\n    by: you\n",
            "  - name: Review\n    kind: check\n    by: you\n  - name: QA\n    kind: check\n    by: you\n",
        );
        let new = read(&text, &base);
        assert_eq!(new.stage("qa").unwrap().name, "QA");
        assert_eq!(new.stage("review").unwrap().name, "Review");
        let changes = stage_changes(&base, &new, false);
        assert_eq!(changes.len(), 1, "{changes:?}");
        assert_eq!(
            changes[0].change,
            Change::Added {
                after: Some("Review".into())
            }
        );
        assert!(changes[0].change.is_shape());
        assert!(!changes[0].change.reorders());
        assert_eq!(changes[0].applies, Applies::AtOnce);
        assert_eq!(changes[0].text, "stage QA (your check) after Review");
        assert_eq!(changes[0].label, "QA after Review");
        assert_eq!(changes[0].applies.text(), "applies at once");
        assert_eq!(
            applied_now(&base, &new),
            new,
            "the whole change applies now"
        );
    }

    /// Reordering moves only the stages out of order; a written id is kept as written.
    #[test]
    fn moves_and_written_ids() {
        let base = default_flow();
        let mut swapped = base.clone();
        swapped.stages.swap(0, 1);
        let changes = stage_changes(&base, &swapped, false);
        let kinds: Vec<&Change> = changes.iter().map(|c| &c.change).collect();
        assert_eq!(kinds.len(), 1, "{changes:?}");
        assert!(kinds[0].reorders());
        assert_eq!(
            changes[0].applies.text(),
            "waits until no ticket is between Dev and Review"
        );
        let new = read(
            "flow:\n  - id: build\n    name: Dev\n    kind: agent\n    agent: Developer\n    skill: work-ticket\n  - \
             name: Done\n    kind: delivery\n    mode: click\n    delivers: local\n",
            &base,
        );
        assert_eq!(names(&new), [("build", "Dev")]);
    }

    /// A stage's description, from an older file's `why:`: read, written back as `description:`, and a change that
    /// applies at once (every agent reads it).
    #[test]
    fn a_description_round_trips_and_applies_at_once() {
        let base = default_flow();
        let text = DEFAULT_RENAMED.replace(
            "  - name: QA\n",
            "  - name: Review\n    why: \"Your PRs on GitHub always get one review before merging (gh)\"\n",
        );
        let (file, written) = yaml::read_with_written_ids(&text).unwrap();
        let mut flow = file.flow.clone();
        match_ids(&base, &mut flow, &written);
        assert_eq!(
            flow.stage("review").unwrap().description.as_deref(),
            Some("Your PRs on GitHub always get one review before merging (gh)")
        );
        let changes = stage_changes(&base, &flow, false);
        let labels: Vec<(&str, &Applies)> = changes
            .iter()
            .map(|c| (c.label.as_str(), &c.applies))
            .collect();
        assert_eq!(
            labels,
            [("Review \u{b7} description added", &Applies::AtOnce)]
        );
        assert_eq!(applied_now(&base, &flow), flow);
        let again = FlowFile {
            agents: Vec::new(),
            flow,
        };
        let written = yaml::write(&again);
        assert!(written.contains("    description: "), "{written}");
        assert!(!written.contains("why:"), "{written}");
        assert_eq!(yaml::read(&written).unwrap(), again);
        let lines = why_lines(&again);
        assert_eq!(lines.len(), 1);
        assert_eq!(
            lines[0].about,
            About::Stage {
                id: "review".into(),
                name: "Review".into()
            }
        );
    }

    /// Spec first's shape (d0131): Product spec → Dev → Spec review (fails to Product spec) → Done.
    const FAILS_TO_SPEC: &str = "flow:
  - name: Product spec
    kind: agent
    agent: Developer
    skill: work-ticket
  - name: Dev
    kind: agent
    agent: Developer
    skill: work-ticket
  - name: Spec review
    kind: check
    by: agent
    agents: [Validator]
    skill: validate
    rounds: 3
    fail_to: Product spec
  - name: Done
    kind: delivery
    mode: click
    delivers: local
";

    /// d0131 with d0108's ids: `fail_to` keeps the stage's id, so renaming Product spec keeps Spec review's target
    /// (only the rename shows); the ids matched to the flow it changes carry it along.
    #[test]
    fn a_rename_keeps_where_fails_go() {
        let base = read(FAILS_TO_SPEC, &Flow::default());
        assert_eq!(
            base.stage("spec-review").unwrap().fail_to.as_deref(),
            Some("product-spec")
        );
        // Matched by position: the new name is gone from the old flow, so the id stays.
        let renamed = read(
            &FAILS_TO_SPEC.replace("Product spec", "Product brief"),
            &base,
        );
        assert_eq!(renamed.stages[0].id, "product-spec");
        assert_eq!(
            renamed.stage("spec-review").unwrap().fail_to.as_deref(),
            Some("product-spec")
        );
        let texts: Vec<String> = stage_changes(&base, &renamed, false)
            .into_iter()
            .map(|c| c.text)
            .collect();
        assert_eq!(texts, ["stage Product spec \u{2192} Product brief"]);
        // An id given from the name and then matched elsewhere: fail_to follows the stage, not the id it read with.
        let mut proposed = yaml::read(FAILS_TO_SPEC).unwrap().flow;
        let mut old = proposed.clone();
        old.stages[0].id = "spec".into();
        old.stages[0].name = "Product spec".into();
        match_ids(&old, &mut proposed, &[false; 4]);
        assert_eq!(proposed.stages[0].id, "spec");
        assert_eq!(proposed.stages[2].fail_to.as_deref(), Some("spec"));
        assert_eq!(
            proposed.fail_to_of(&proposed.stages[2]).unwrap().name,
            "Product spec"
        );
    }

    /// d0131: setting where a check's fails go, and removing that stage later: the check is on the nearest Agent
    /// stage again, and the preview says so.
    #[test]
    fn removing_the_target_resets_it_and_says_so() {
        let base = read(FAILS_TO_SPEC, &Flow::default());
        let mut gone = base.clone();
        gone.stages.retain(|s| s.id != "product-spec");
        // Written and read back, as the editor's draft is: the removed stage leaves no `fail_to`.
        let gone = yaml::read(&yaml::write(&FlowFile {
            agents: Vec::new(),
            flow: gone,
        }))
        .unwrap()
        .flow;
        assert_eq!(gone.stage("spec-review").unwrap().fail_to, None);
        let changes = stage_changes(&base, &gone, false);
        let texts: Vec<(&str, String)> = changes
            .iter()
            .map(|c| (c.text.as_str(), c.applies.text()))
            .collect();
        assert_eq!(
            texts,
            [
                (
                    "Spec review's fails go to the nearest Agent stage now",
                    "applies at once".to_owned()
                ),
                (
                    "stage Product spec removed",
                    "waits until no ticket is between Product spec and Dev".to_owned()
                ),
            ]
        );
        assert_eq!(
            changes[0].change,
            Change::FailTo {
                from: Some("Product spec".into()),
                to: None,
                gone: true
            }
        );

        // Setting it, and changing a check's agent, are changes of their own.
        let unset = read(
            &FAILS_TO_SPEC
                .replace("    fail_to: Product spec\n", "")
                .replace("agents: [Validator]", "agent: Security reviewer"),
            &base,
        );
        let texts: Vec<String> = stage_changes(&base, &unset, false)
            .into_iter()
            .map(|c| format!("{} \u{2014} {}", c.text, c.applies.text()))
            .collect();
        assert_eq!(
            texts,
            [
                "Spec review \u{b7} agent: Validator \u{2192} Security reviewer \u{2014} applies when Spec review \
                 next starts",
                "Spec review \u{b7} fails go to: Product spec \u{2192} nearest \u{2014} applies at once",
            ]
        );
    }

    #[test]
    fn warnings_for_automatic_delivery() {
        let mut flow = default_flow();
        assert_eq!(warnings(&flow, "main", None), [], "on your click");
        flow.delivery = crate::flow::Delivery {
            mode: crate::flow::DeliveryMode::Automatic,
            delivers: crate::flow::Delivers::Local,
            agent: None,
            skill: None,
        };
        assert_eq!(
            warnings(&flow, "main", None),
            [Warning {
                level: Level::Amber,
                stage: Some("Delivery".into()),
                text: "Delivery merges into main without asking you.".into()
            }]
        );
        flow.stages
            .retain(|s| !matches!(s.kind, StageKind::Check { .. }));
        assert_eq!(
            warnings(&flow, "main", Some("GitHub"))[0].text,
            "Delivery merges into main without any check before it."
        );
    }
}
