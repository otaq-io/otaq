//! Applying a proposal, and when a change reaches tickets (FLW-05, FLW-06, FLW-09, FLW-10, FED-08; d0108 "A flow
//! change waits for a quiet board", second gap review rules 11 and 13; d0115 "When an applied change reaches
//! tickets").
//!
//! **What applies when** ([`Board::apply_proposal`]): agent changes and skill imports apply now (each session's next
//! start; running ones keep what they started with, d0088). **Adding a stage or editing one applies at once**: stored
//! as a new version now, so a ticket that hasn't reached that point follows it (Plan added after Product spec: SHOP-12
//! in Product spec goes to Plan next) and tickets in or past it finish as they are; a stage's agent, skill, options and
//! rounds are read when that stage next starts, its name, column and a Delivery's mode or target by the next card that
//! arrives. **Removing or reordering stages, or a stage's kind, waits for its affected span** ([`affected_span`]): the
//! parts that don't wait are stored now ([`super::proposal::applied_now`]) and the whole proposed flow becomes the
//! project's **waiting change**. **A swap to another flow** (a Start from pick, `flow propose --from`) waits for the
//! whole board, as before: only its edits apply now ([`super::proposal::edits_only`]).
//!
//! **The wait** ([`still_working`], [`Wait`]): it applies once no ticket inside what it waits for is at work. A
//! ticket is quiet in Backlog, Ready for Dev or Done, waiting in a human check the new flow still has, waiting in the
//! Delivery stage with no Delivery session at work (To deliver, PR open). Cards in an Agent stage, an agent check, a running Delivery session or a stage the change
//! removes hold it. For a span only the cards inside it count (Security moved before Validate: Dev to Review); tickets
//! before it move on freely and those past it are untouched. **Meanwhile no ticket enters it** ([`Hold`]): a ticket
//! that would waits at its edge, before its first stage, "Waiting for the flow change": the scheduler doesn't start it
//! ([`Board::settle_flow_changes`]; other projects and the rest of the board keep starting), and a drag past the edge
//! stops there ([`Board::span_edge`]). For the whole board no ticket starts from Ready for Dev. When it is quiet the
//! new flow is stored and starts resume. **A change while another waits** was compared
//! with the waiting flow, so it shows only the new part; Apply merges it into the waiting change, even its parts that
//! would otherwise apply at once; **Cancel change** drops both, and nothing else. **Apply now** forces it, with a stage
//! picked for each ticket in a removed stage: [`super::apply_now`].
//!
//! **A removed choice** ([`Board::drop_removed_choice`]): when a stage starts for a ticket whose chosen option it no
//! longer offers, the stage's own agent runs and History says so ("Quick developer is no longer an option for Dev ·
//! runs Rust developer").

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::draft::DraftMemory;
use super::engine::Target;
use super::model::{Flow, FlowAgent, FlowFile, Stage, StageOption};
use super::proposal::{
    AgentChangeKind, ApplyChoices, ClashChoice, Proposal, ProposedBy, SkillSource, Source,
    applied_now, edits_only, effective_file, flow_agent_of, skill_source,
};
use super::store::{AppliedBy, FlowVersion, NewFlowVersion};
use super::yaml;
use crate::agents::store::NewAgent;
use crate::agents::{Agent, CameFrom, ModelCatalog, Role, SkillRef, shipped};
use crate::board::moves::DropOn;
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Actor, Place, StatusSet, TicketKey};
use crate::store::{Store, now_ms};

/// Why a ticket in Ready for Dev doesn't start while its project's flow change waits (the card's footer, FLW-06).
pub const WAITING_FOR_THE_FLOW_CHANGE: &str = "Waiting for the flow change";

/// The **affected span** of a removal or reorder (FED-08): the stages from the first to the last one it touches, in
/// the flow in use. Moving Security before Validate touches Dev to Review.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Span {
    /// The stages' ids, first to last. Empty: nothing is left to wait for.
    pub stages: Vec<String>,
    /// The first and last stages' names: "Dev", "Review".
    pub first: String,
    pub last: String,
}

impl Span {
    pub fn contains(&self, id: &str) -> bool {
        self.stages.iter().any(|s| s == id)
    }

    /// Whether a ticket at `place` is inside it: in one of its stages, or waiting before one of them but the first
    /// (before the first it waits at the edge, outside).
    pub fn has_inside(&self, place: &Place) -> bool {
        match place {
            Place::InStage(id) => self.contains(id),
            Place::Waiting(Some(id)) => self.contains(id) && self.stages.first() != Some(id),
            _ => false,
        }
    }

    /// Whether a ticket at `place` comes before it in `flow`: in or waiting before an earlier stage.
    fn is_before(&self, flow: &Flow, place: &Place) -> bool {
        let Some(first) = self.stages.first().and_then(|id| flow.position(id)) else {
            return false;
        };
        place
            .stage_id()
            .and_then(|id| flow.position(id))
            .is_some_and(|at| at < first)
    }
}

/// What a waiting change waits for (FED-08).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "until", rename_all = "snake_case")]
pub enum Wait {
    /// A swap to another flow: a quiet board (FLW-06).
    WholeBoard,
    /// A removal or reorder: no ticket at work inside the span.
    Span(Span),
}

impl Wait {
    /// Whether a ticket at `place` counts for it.
    pub fn covers(&self, place: &Place) -> bool {
        match self {
            Self::WholeBoard => true,
            Self::Span(span) => span.has_inside(place),
        }
    }

    /// The starts it holds back in `flow` (the flow in use).
    pub fn hold(&self, flow: &Flow) -> Hold {
        match self {
            Self::WholeBoard => Hold::READY_FOR_DEV,
            Self::Span(span) => Hold {
                ready_for_dev: flow
                    .first_agent_stage()
                    .is_some_and(|s| span.contains(&s.id)),
                before: span
                    .stages
                    .first()
                    .filter(|id| flow.stage(id).is_some_and(Stage::is_agent_stage))
                    .cloned(),
            },
        }
    }
}

/// The starts a waiting change holds back in its project ("Waiting for the flow change"): from Ready for Dev (the
/// whole board, or a span that holds the first Agent stage), and of the tickets waiting at the span's edge.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Hold {
    pub ready_for_dev: bool,
    /// The span's first stage: tickets waiting before it don't start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
}

impl Hold {
    /// No ticket starts from Ready for Dev (the whole board's wait).
    pub const READY_FOR_DEV: Self = Self {
        ready_for_dev: true,
        before: None,
    };

    /// Whether a ticket waiting at `place` is held.
    pub fn holds(&self, place: &Place) -> bool {
        match place {
            Place::Waiting(None) => self.ready_for_dev,
            Place::Waiting(Some(id)) => self.before.as_deref() == Some(id.as_str()),
            _ => false,
        }
    }
}

/// The affected span of changing `old` into `new` (FED-08), `None` when nothing in it waits. A stage is touched when
/// a step from it or into it in `old` is gone from `new` (it or its neighbour removed, or another stage now between
/// them), or when its kind changes; stages `new` adds don't count, they apply at once. The span runs from the first
/// touched stage to the last, starting at the Agent stage at or before it: a ticket waits before an Agent stage
/// (STG-07), so that is its edge. Security moved before Validate: Dev → Validate, Validate → Security and Security →
/// Review are gone, so Dev to Review.
pub fn affected_span(old: &Flow, new: &Flow) -> Option<Span> {
    // `new`'s order of the stages `old` has.
    let kept: Vec<&str> = new
        .stages
        .iter()
        .filter(|s| old.stage(&s.id).is_some())
        .map(|s| s.id.as_str())
        .collect();
    let next = |id: &str| -> Option<Option<&str>> {
        let at = kept.iter().position(|k| *k == id)?;
        Some(kept.get(at + 1).copied())
    };
    let n = old.stages.len();
    let mut touched = vec![false; n];
    // The step into the first stage, each step between two, and the step out of the last.
    if kept.first().copied() != old.stages.first().map(|s| s.id.as_str()) {
        touched[0] = true;
    }
    for i in 0..n {
        let after = old.stages.get(i + 1).map(|s| s.id.as_str());
        if next(&old.stages[i].id) != Some(after) {
            touched[i] = true;
            if i + 1 < n {
                touched[i + 1] = true;
            }
        }
    }
    for (i, stage) in old.stages.iter().enumerate() {
        if new.stage(&stage.id).is_some_and(|s| s.kind != stage.kind) {
            touched[i] = true;
        }
    }
    let first = touched.iter().position(|t| *t)?;
    let last = touched.iter().rposition(|t| *t)?;
    let first = old.stages[..=first]
        .iter()
        .rposition(Stage::is_agent_stage)
        .unwrap_or(first);
    let stages = &old.stages[first..=last];
    Some(Span {
        stages: stages.iter().map(|s| s.id.clone()).collect(),
        first: stages[0].name.clone(),
        last: stages[stages.len() - 1].name.clone(),
    })
}

/// A project's waiting change (`flow.waiting`): the flow as it will be, and what holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitingChange {
    /// The project's key.
    pub project: String,
    /// The flow document it applies.
    pub yaml: String,
    pub applied_by: AppliedBy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// What it changes, every merged change's parts in order: "QA after Security", "Review can be skipped".
    pub changes: Vec<String>,
    /// Since when it waits, in Unix milliseconds.
    pub since: i64,
    /// What it waits for: its span, or the whole board for a swap (FED-08).
    pub wait: Wait,
    /// The starts it holds back meanwhile.
    pub hold: Hold,
    /// The tickets it waits for, in key order ("8 tickets still working", Show the 8 tickets).
    pub waiting_for: Vec<TicketKey>,
}

/// A waiting change's `changes_json`: what it changes, and whether a swap merged into it (it then waits for the whole
/// board, FED-08).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct WaitingMemo {
    changes: Vec<String>,
    #[serde(default)]
    whole_board: bool,
}

impl WaitingChange {
    /// The banner's words for it: "QA after Security · Review can be skipped".
    pub fn summary(&self) -> String {
        self.changes.join(" \u{b7} ")
    }
}

/// What Apply did (`flow.apply`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Applied {
    /// The proposal applied.
    pub proposal: u32,
    /// The flow version stored now: the whole change, the part of it that doesn't wait, or a waiting change the board
    /// was already quiet for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<FlowVersion>,
    /// The change still waiting for a quiet board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<WaitingChange>,
    /// The agents added or changed, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agents: Vec<String>,
}

/// What `flow.propose_from` proposes (FLW-08…FLW-10).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "from", rename_all = "snake_case")]
pub enum ProposeFrom {
    /// Back to the previous flow: the version before the current one, or `version`; stages only.
    Previous {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u32>,
    },
    /// Copy from another project (by key or name): its stages.
    Project { project: String },
    /// A shipped preset file ("otaq-default.yaml"); Reset to the default flow is this with the default's file.
    Preset { file: String },
    /// Import flow…: a file's name and contents.
    File { name: String, yaml: String },
    /// Apply the shipped agents file again: its agents.
    AgentsFile,
}

/// `flow.apply`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyParams {
    pub project: String,
    /// The proposal's number: Apply on a proposal that was replaced meanwhile is refused.
    pub proposal: u32,
    #[serde(default)]
    pub choices: ApplyChoices,
}

/// `flow.propose_from`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposeFromParams {
    pub project: String,
    pub source: ProposeFrom,
}

/// A change in a project's flow, for the app (`flow_changed`): the flow editor refetches a proposal, the banner follows the
/// waiting change, the toast says "shop-web's flow changed".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "what", rename_all = "snake_case")]
pub enum FlowEvent {
    /// A proposal opened (or replaced the open one).
    Proposed { proposal: u32 },
    /// The open proposal was applied or cancelled.
    ProposalClosed { proposal: u32 },
    /// A change waits for a quiet board, or another merged into it.
    Waiting,
    /// The project's draft changed (FED-02): a Project agent's proposal landed in it, or the user edited it. No
    /// preview opens for it: the flow editor shows the draft.
    DraftChanged { proposal: u32 },
    /// A new version is the current flow.
    Applied { version: u32 },
    /// Cancel change dropped the waiting change.
    Cancelled,
}

/// Whether `ticket` lets a change to `new` apply (FLW-06): in Backlog, Ready for Dev or Done; waiting in
/// a human check `new` still has; waiting in the Delivery stage (To deliver, PR open) with no Delivery session at
/// work, since a Delivery is never removed (Q-new-1, d0108: "Cards in an Agent stage or an agent check, and cards in
/// a stage the change removes, block it"). `delivering`: the ids of
/// tickets a Delivery session works on now (an automatic PR Delivery's `open-pr`), which hold it.
pub fn is_quiet(ticket: &Ticket, new: &Flow, delivering: &HashSet<String>) -> bool {
    match &ticket.place {
        Place::Backlog | Place::Waiting(None) | Place::Done => true,
        Place::InStage(id) => match ticket.status.set() {
            StatusSet::HumanCheck => new.stage(id).is_some_and(Stage::is_human_check),
            StatusSet::Delivery => !delivering.contains(&ticket.id),
            _ => false,
        },
        Place::Waiting(Some(_)) => false,
    }
}

/// The tickets a change to `new` waits for, in `tickets`' order: those `wait` covers that aren't quiet
/// ([`is_quiet`]).
pub fn still_working<'t>(
    tickets: &'t [Ticket],
    new: &Flow,
    delivering: &HashSet<String>,
    wait: &Wait,
) -> Vec<&'t Ticket> {
    tickets
        .iter()
        .filter(|t| wait.covers(&t.place) && !is_quiet(t, new, delivering))
        .collect()
}

/// What a scheduler pass learns about the waiting changes ([`Board::settle_flow_changes`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settled {
    /// The versions stored now, one per project whose board was quiet.
    pub applied: Vec<FlowVersion>,
    /// The projects whose change still waits, by key, with the starts it holds back.
    pub held: HashMap<String, Hold>,
}

type WaitingRow = (String, String, Option<String>, Option<String>, String, i64);

impl Board {
    /// `flow.waiting`: the project's waiting change, with the tickets it waits for now; `None` when nothing waits.
    pub async fn waiting_change(&self, project: &str) -> Result<Option<WaitingChange>, BoardError> {
        let project = self.project(project).await?;
        let row: Option<WaitingRow> = sqlx::query_as(
            "SELECT yaml, applied_by, agent_name, preset, changes_json, since FROM flow_waiting WHERE project_id = ?",
        )
        .bind(&project.id)
        .fetch_optional(self.store().pool())
        .await?;
        let Some((text, applied_by, agent_name, preset, memo, since)) = row else {
            return Ok(None);
        };
        let memo: WaitingMemo = serde_json::from_str(&memo).unwrap_or_default();
        let new = yaml::read(&text)
            .map_err(|e| {
                BoardError::NotFound(format!(
                    "{}'s waiting flow change doesn't read: {e}",
                    project.key
                ))
            })?
            .flow;
        let old = self.effective_flow(&project.key).await?;
        let wait = wait_of(&old, &new, memo.whole_board);
        let tickets = self.tickets(&project.key).await?;
        let delivering = self.delivering().await?;
        let mut waiting_for: Vec<TicketKey> = still_working(&tickets, &new, &delivering, &wait)
            .into_iter()
            .map(|t| t.key.clone())
            .collect();
        waiting_for.sort();
        Ok(Some(WaitingChange {
            project: project.key,
            yaml: text,
            applied_by: applied_by.parse().unwrap_or(AppliedBy::User),
            agent_name,
            preset,
            changes: memo.changes,
            since,
            hold: wait.hold(&old),
            wait,
            waiting_for,
        }))
    }

    /// The tickets a Delivery session works on now (Open PR, an automatic PR Delivery): they hold a change.
    pub(crate) async fn delivering(&self) -> Result<HashSet<String>, BoardError> {
        Ok(sqlx::query_scalar("SELECT ticket_id FROM pr_flows")
            .fetch_all(self.store().pool())
            .await?
            .into_iter()
            .collect())
    }

    /// Whether nothing `wait` covers on `project`'s board is at work now for a change to `new` ([`still_working`]).
    pub(crate) async fn is_quiet_for(
        &self,
        project: &str,
        new: &Flow,
        wait: &Wait,
    ) -> Result<bool, BoardError> {
        let tickets = self.tickets(project).await?;
        let delivering = self.delivering().await?;
        Ok(still_working(&tickets, new, &delivering, wait).is_empty())
    }

    /// Where a drop of `ticket` on `target` lands while its project's change waits for a span (FED-08): a forward drag
    /// from before the span to a stage inside it, past its first stage, stops at the edge (dropped on the first stage,
    /// it waits before it there, "Waiting for the flow change"). Any other drop as it is.
    pub async fn span_edge(&self, ticket: &Ticket, target: &DropOn) -> Result<DropOn, BoardError> {
        let project = ticket.key.project.as_str();
        let Some(Wait::Span(span)) = self.waiting_change(project).await?.map(|w| w.wait) else {
            return Ok(target.clone());
        };
        let flow = self.effective_flow(project).await?;
        let Some(first) = span.stages.first() else {
            return Ok(target.clone());
        };
        let inside = match target.in_flow(&flow) {
            Some(Target::Stage(stage)) => span.contains(&stage.id) && stage.id != *first,
            _ => false,
        };
        if inside && span.is_before(&flow, &ticket.place) {
            tracing::info!(ticket = %ticket.key, edge = %first, "a drop into the waiting span stops at its edge");
            return Ok(DropOn::stage(first.clone()));
        }
        Ok(target.clone())
    }

    /// `flow.propose_from` (FLW-08…FLW-10, FED-02): a proposal from an earlier version, another project, a preset, a
    /// file or the shipped agents file, by the user: a Start from pick copied into the draft, replacing it.
    pub async fn propose_from(
        &self,
        project: &str,
        from: &ProposeFrom,
        models: &dyn ModelCatalog,
    ) -> Result<Proposal, BoardError> {
        let refuse = |message: String| BoardError::refused(Refusal::BadFlow, message);
        let read = |text: &str| {
            yaml::read_with_written_ids(text)
                .map_err(|e| refuse(format!("The flow doesn't read: {e}")))
        };
        let me = ProposedBy::User;
        // Another flow copied into the draft: applying it is a swap, which waits for the whole board (FED-08).
        let swap = DraftMemory {
            swap: true,
            ..DraftMemory::default()
        };
        let key = self.project(project).await?.key;
        match from {
            ProposeFrom::Previous { version } => {
                let versions = self.flow_versions(&key).await?;
                let current = versions.iter().find(|v| v.current).map_or(0, |v| v.version);
                let wanted = version.unwrap_or_else(|| previous_version(&versions, current));
                let Some(earlier) = versions
                    .iter()
                    .find(|v| v.version == wanted && v.version != current)
                else {
                    return Err(refuse(match version {
                        Some(v) => format!("{key} has no earlier flow {v}"),
                        None => format!("{key} has no previous flow"),
                    }));
                };
                let (file, written) = read(&earlier.yaml)?;
                let source = Source::Previous {
                    version: earlier.version,
                };
                self.open_read_proposal(&key, file, &written, &source, &me, &swap, models)
                    .await
            }
            ProposeFrom::Project { project: other } => {
                let other = self.project_named(other).await?;
                let file = FlowFile {
                    agents: Vec::new(),
                    flow: self.effective_flow(&other.key).await?,
                };
                // Another project's ids mean nothing here: its stages match this flow's by name, then position.
                let written = vec![false; file.flow.stages.len()];
                let source = Source::Project {
                    key: other.key,
                    name: other.name,
                };
                self.open_read_proposal(&key, file, &written, &source, &me, &swap, models)
                    .await
            }
            ProposeFrom::Preset { file } => {
                let preset = super::presets::preset(file)
                    .ok_or_else(|| refuse(format!("{file} isn't one of the presets")))?;
                let (read, written) = read(preset.yaml)?;
                let source = Source::Preset {
                    file: preset.file.to_owned(),
                };
                self.open_read_proposal(&key, read, &written, &source, &me, &swap, models)
                    .await
            }
            ProposeFrom::File { name, yaml: text } => {
                let (file, written) = read(text)?;
                let source = Source::Document {
                    file: Some(name.clone()),
                };
                self.open_read_proposal(&key, file, &written, &source, &me, &swap, models)
                    .await
            }
            ProposeFrom::AgentsFile => {
                let file = FlowFile {
                    agents: shipped::agents_file().to_vec(),
                    flow: Flow::default(),
                };
                self.open_read_proposal(
                    &key,
                    file,
                    &[],
                    &Source::AgentsFile,
                    &me,
                    &DraftMemory::default(),
                    models,
                )
                .await
            }
        }
    }

    /// A project by its key, else by its name ("pocket-notes").
    pub(crate) async fn project_named(
        &self,
        name: &str,
    ) -> Result<crate::store::projects::Project, BoardError> {
        if let Ok(project) = self.project(name).await {
            return Ok(project);
        }
        self.store()
            .projects()
            .await?
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| BoardError::NotFound(format!("unknown project {name}")))
    }

    /// **Apply** (`flow.apply`): the open proposal `id` of `project`, with the user's `choices`; refused while it has
    /// problems they leave. Agents first (the stages may name one it adds), then the stages: stored now, or partly
    /// now and the rest waiting, or merged into the waiting change. A waiting change the board is already quiet for
    /// applies at once.
    pub async fn apply_proposal(
        &self,
        project: &str,
        id: u32,
        choices: &ApplyChoices,
        models: &dyn ModelCatalog,
    ) -> Result<Applied, BoardError> {
        let row = self
            .proposal_row_by_id(project, id)
            .await?
            .ok_or_else(|| BoardError::NotFound(format!("Proposal {id} isn't open any more")))?;
        let view = self.view(&row, models, choices).await?;
        if !view.problems.is_empty() {
            let lines: Vec<&str> = view.problems.iter().map(|p| p.message.as_str()).collect();
            return Err(BoardError::refused(Refusal::BadFlow, lines.join("\n")));
        }
        let project = self.project(project).await?;
        let scope = row.source.scope();
        let (base, against_waiting) = self.flow_as_it_will_be(&project.key).await?;
        let read = yaml::read(&row.yaml).map_err(|e| {
            BoardError::refused(Refusal::BadFlow, format!("The flow doesn't read: {e}"))
        })?;
        let file = effective_file(&read, scope, &base.flow, &row.source, choices);

        let agents = if scope.agents() {
            self.apply_agents(&file, &view, &row.source, &project.name, choices)
                .await?
        } else {
            Vec::new()
        };

        let (applied_by, agent_name) = match &row.proposed_by {
            ProposedBy::User => (AppliedBy::User, None),
            ProposedBy::ProjectAgent { name, .. } => (AppliedBy::ProjectAgent, Some(name.clone())),
        };
        let preset = row.source.preset().map(str::to_owned);
        let mut version = None;
        let changes = &view.diff.stages;
        if scope.stages() && !changes.is_empty() {
            let labels: Vec<String> = changes.iter().map(|c| c.label.clone()).collect();
            let target = self.document(&file.flow).await?;
            let whole = NewFlowVersion {
                yaml: target.clone(),
                applied_by,
                agent_name: agent_name.clone(),
                preset: preset.clone(),
            };
            let swap = row.draft.swap;
            let waiting = Waiting {
                project: &project,
                target: &target,
                labels: &labels,
                applied_by,
                agent_name: agent_name.as_deref(),
                preset: preset.as_deref(),
                whole_board: swap,
            };
            // What applies at once (FED-08): additions and edits; a swap's edits only.
            let now = match swap {
                true => edits_only(&base.flow, &file.flow),
                false => applied_now(&base.flow, &file.flow),
            };
            let now = match super::check(&now) {
                Ok(()) => now,
                Err(_) => edits_only(&base.flow, &file.flow),
            };
            let wait = match swap {
                true => view.diff.waits.then_some(Wait::WholeBoard),
                false => affected_span(&now, &file.flow).map(Wait::Span),
            };
            if against_waiting {
                self.merge_waiting(&waiting).await?;
            } else if let Some(wait) = wait
                && !self.is_quiet_for(&project.key, &file.flow, &wait).await?
            {
                // The part that doesn't wait applies now; the whole change waits for its span (or the board).
                if now != base.flow && super::check(&now).is_ok() {
                    version = Some(
                        self.store_flow(
                            &project.key,
                            &NewFlowVersion {
                                yaml: self.document(&now).await?,
                                ..whole.clone()
                            },
                        )
                        .await?,
                    );
                }
                self.merge_waiting(&waiting).await?;
            } else {
                // Nothing waits, or nothing it waits for is at work: the whole change, as one version.
                version = Some(self.store_flow(&project.key, &whole).await?);
            }
        }
        self.drop_proposal(id).await?;
        if let Some(settled) = self.settle_flow_change(&project.key).await? {
            version = Some(settled);
        }
        tracing::info!(project = %project.key, proposal = id, "flow proposal applied");
        Ok(Applied {
            proposal: id,
            version,
            waiting: self.waiting_change(&project.key).await?,
            agents,
        })
    }

    /// A flow document to store: `flow` and the agents it names as they are now (an export's content).
    pub(crate) async fn document(&self, flow: &Flow) -> Result<String, BoardError> {
        let stored = self.store().agents().await?;
        let mut agents: Vec<FlowAgent> = Vec::new();
        let named = flow.stages.iter().flat_map(Stage::every_agent);
        for name in named {
            if let Some(agent) = stored
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(name))
                .filter(|a| !agents.iter().any(|b| b.name == a.name))
            {
                agents.push(flow_agent_of(agent));
            }
        }
        Ok(yaml::write(&FlowFile {
            agents,
            flow: flow.clone(),
        }))
    }

    /// Makes the change's target the project's waiting change, adding its labels to what an earlier one changes
    /// (FLW-06: one waiting change); a swap merged in makes it wait for the whole board (FED-08).
    async fn merge_waiting(&self, change: &Waiting<'_>) -> Result<(), BoardError> {
        let Waiting {
            project,
            target,
            labels,
            applied_by,
            agent_name,
            preset,
            whole_board,
        } = *change;
        let mut tx = self.store().begin_write().await?;
        let earlier: Option<(String, i64)> =
            sqlx::query_as("SELECT changes_json, since FROM flow_waiting WHERE project_id = ?")
                .bind(&project.id)
                .fetch_optional(&mut *tx)
                .await?;
        let (mut memo, since) = match earlier {
            Some((memo, since)) => (
                serde_json::from_str::<WaitingMemo>(&memo).unwrap_or_default(),
                since,
            ),
            None => (WaitingMemo::default(), now_ms()),
        };
        memo.changes.extend(labels.iter().cloned());
        memo.whole_board |= whole_board;
        sqlx::query(
            "INSERT INTO flow_waiting (project_id, yaml, applied_by, agent_name, preset, changes_json, since)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (project_id) DO UPDATE SET
                 yaml = excluded.yaml, applied_by = excluded.applied_by, agent_name = excluded.agent_name,
                 preset = excluded.preset, changes_json = excluded.changes_json",
        )
        .bind(&project.id)
        .bind(target)
        .bind(applied_by.as_str())
        .bind(agent_name)
        .bind(preset)
        .bind(serde_json::to_string(&memo).expect("labels serialize"))
        .bind(since)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        tracing::info!(project = %project.key, changes = ?memo.changes, whole_board = memo.whole_board, "flow change waits");
        Ok(())
    }

    /// **Cancel change** (`flow.cancel_waiting`): drops the waiting change, merged ones included; starts resume.
    /// Nothing else changes. Whether there was one.
    pub async fn cancel_waiting(&self, project: &str) -> Result<bool, BoardError> {
        let project = self.project(project).await?;
        let gone = sqlx::query("DELETE FROM flow_waiting WHERE project_id = ?")
            .bind(&project.id)
            .execute(self.store().pool())
            .await?
            .rows_affected();
        if gone > 0 {
            tracing::info!(project = %project.key, "waiting flow change cancelled");
        }
        Ok(gone > 0)
    }

    /// Applies the project's waiting change if its board is quiet now: the new flow is stored as the next version and
    /// the change goes. The version stored, if any.
    pub async fn settle_flow_change(
        &self,
        project: &str,
    ) -> Result<Option<FlowVersion>, BoardError> {
        let Some(waiting) = self.waiting_change(project).await? else {
            return Ok(None);
        };
        if !waiting.waiting_for.is_empty() {
            return Ok(None);
        }
        let project = self.project(project).await?;
        let mut tx = self.store().begin_write().await?;
        // Only the change read above, once: a pass and an Apply may settle at the same moment, or a merge may have
        // changed it meanwhile (the next settle takes that one).
        let taken = sqlx::query("DELETE FROM flow_waiting WHERE project_id = ? AND yaml = ?")
            .bind(&project.id)
            .bind(&waiting.yaml)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if taken == 0 {
            return Ok(None);
        }
        Store::insert_flow_version_in(
            &mut tx,
            &project.id,
            &NewFlowVersion {
                yaml: waiting.yaml,
                applied_by: waiting.applied_by,
                agent_name: waiting.agent_name,
                preset: waiting.preset,
            },
        )
        .await?;
        tx.commit().await?;
        tracing::info!(project = %project.key, "waiting flow change applied: nothing it waits for is at work");
        Ok(Some(self.current_flow(&project.key).await?))
    }

    /// Every project's waiting change, for a scheduler pass: the ones nothing holds apply, the others hold their
    /// project's starts into what they wait for ([`Hold`]).
    pub async fn settle_flow_changes(&self) -> Result<Settled, BoardError> {
        let keys: Vec<String> = sqlx::query_scalar(
            "SELECT p.key FROM flow_waiting w JOIN projects p ON p.id = w.project_id ORDER BY p.key",
        )
        .fetch_all(self.store().pool())
        .await?;
        let mut settled = Settled::default();
        for key in keys {
            match self.settle_flow_change(&key).await {
                Ok(Some(version)) => settled.applied.push(version),
                Ok(None) => {
                    let hold = match self.waiting_change(&key).await {
                        Ok(Some(waiting)) => waiting.hold,
                        Ok(None) => continue,
                        Err(_) => Hold::READY_FOR_DEV,
                    };
                    settled.held.insert(key, hold);
                }
                Err(e) => {
                    tracing::warn!(project = %key, error = %e, "can't apply the waiting flow change");
                    settled.held.insert(key, Hold::READY_FOR_DEV);
                }
            }
        }
        Ok(settled)
    }

    /// The agents of an applied proposal (FLW-04, FLW-10): added ones are created ("Added by <project>'s flow", or
    /// "From Otaq's agents file"); a changed one is updated unless it is an import clash the user kept (Keep mine,
    /// the default); Add as was added under its new name. Skill folders are imported now (WSK-02). The names added or
    /// changed.
    async fn apply_agents(
        &self,
        file: &FlowFile,
        view: &Proposal,
        source: &Source,
        project_name: &str,
        choices: &ApplyChoices,
    ) -> Result<Vec<String>, BoardError> {
        let store = self.store();
        let came_from = match source {
            Source::AgentsFile => CameFrom::AgentsFile,
            _ => CameFrom::Flow {
                project: project_name.to_owned(),
            },
        };
        let mut done = Vec::new();
        for (agent, change) in file.agents.iter().zip(&view.diff.agents) {
            match change.change {
                AgentChangeKind::Same => continue,
                AgentChangeKind::Changed
                    if change.clash.is_some()
                        && choices
                            .clashes
                            .iter()
                            .find(|(name, _)| name.eq_ignore_ascii_case(&change.name))
                            .is_none_or(|(_, c)| *c != ClashChoice::TakeTheirs) =>
                {
                    continue;
                }
                _ => {}
            }
            let stored = store.agents().await?;
            let (skills, imported) = self.agent_skills(agent, &stored, choices).await?;
            let model =
                shipped::model_of(&agent.model).unwrap_or(crate::agents::AgentModel::Default);
            match stored
                .iter()
                .find(|a| a.name.eq_ignore_ascii_case(&agent.name))
            {
                Some(mine) => {
                    let effort = mine.effort.clone().filter(|_| mine.model == model);
                    let fields = NewAgent {
                        name: mine.name.clone(),
                        instructions: agent.instructions.clone(),
                        skills,
                        imported,
                        model: model.clone(),
                        effort,
                        role: mine.role,
                    };
                    store
                        .update_agent(&mine.id, &fields, mine.model != model)
                        .await?;
                    done.push(mine.name.clone());
                }
                None => {
                    let mut role = agent.role.as_deref().and_then(|r| r.parse::<Role>().ok());
                    if let Some(held) = role
                        && store.role_agent(held).await?.is_some()
                    {
                        role = None;
                    }
                    let new = NewAgent {
                        name: agent.name.trim().to_owned(),
                        instructions: agent.instructions.clone(),
                        skills,
                        imported,
                        model,
                        effort: None,
                        role,
                    };
                    store.insert_agent(&new, &came_from, None).await?;
                    done.push(new.name);
                }
            }
        }
        Ok(done)
    }

    /// An agent's skills as stored, importing the folders it names (or the user picked for it).
    async fn agent_skills(
        &self,
        agent: &FlowAgent,
        stored: &[Agent],
        choices: &ApplyChoices,
    ) -> Result<(Vec<SkillRef>, Vec<crate::agents::ImportedSkill>), BoardError> {
        let mut skills = Vec::new();
        let mut imported = Vec::new();
        for entry in &agent.skills {
            let (found, _) = skill_source(entry, stored, &choices.skill_folders);
            let skill = match found {
                SkillSource::Shipped(name) => SkillRef::Shipped(name),
                SkillSource::Imported(record) => {
                    let id = record.id.clone();
                    imported.push(record);
                    SkillRef::Imported(id)
                }
                SkillSource::Folder { path, name } => {
                    let record = match self.own_copy(&path, &name, stored).await {
                        Some(record) => record,
                        None => {
                            crate::agents::skills::import_folder(self, std::path::Path::new(&path))
                                .await?
                        }
                    };
                    let id = record.id.clone();
                    imported.push(record);
                    SkillRef::Imported(id)
                }
                // The checks refused the proposal before Apply.
                SkillSource::Missing => continue,
            };
            if !skills.contains(&skill) {
                skills.push(skill);
            }
        }
        Ok((skills, imported))
    }

    /// The record of Otaq's own copy at `path` (`skills/<id>/`, a skill the Project agent wrote for the draft,
    /// FED-06): the agent holding it already, or one no agent holds yet, taken as it is instead of copied again.
    /// `None` for any other folder, which is imported.
    async fn own_copy(
        &self,
        path: &str,
        name: &str,
        stored: &[crate::agents::Agent],
    ) -> Option<crate::agents::ImportedSkill> {
        let path = std::path::Path::new(path);
        if path.parent() != Some(self.data().skills_dir().as_path()) {
            return None;
        }
        let id = path.file_name()?.to_string_lossy().into_owned();
        if let Some(held) = stored.iter().find_map(|a| a.imported_skill(&id)) {
            return Some(held.clone());
        }
        let folder = path.to_owned();
        let fingerprint =
            tokio::task::spawn_blocking(move || crate::skills::folder::fingerprint(&folder))
                .await
                .ok()?
                .ok()?;
        Some(crate::agents::ImportedSkill {
            id,
            name: name.to_owned(),
            original: path.display().to_string(),
            imported_at: crate::store::now_ms(),
            fingerprint,
            warnings: Vec::new(),
        })
    }

    /// FLW-05's fallback, when `stage` starts a session for `ticket`: a choice of an agent the stage no longer offers
    /// is dropped, and History says so ("Quick developer is no longer an option for Dev · runs Rust developer"). The
    /// stage's own agent then runs ([`crate::agents::resolve::running_agent`]).
    pub async fn drop_removed_choice(
        &self,
        ticket: &Ticket,
        stage: &Stage,
    ) -> Result<(), BoardError> {
        let chosen: Option<Option<String>> = sqlx::query_scalar(
            "SELECT agent_id FROM ticket_stage_choices WHERE ticket_id = ? AND stage_id = ? AND choice = 'agent'",
        )
        .bind(&ticket.id)
        .bind(&stage.id)
        .fetch_optional(self.store().pool())
        .await?;
        let Some(chosen) = chosen else {
            return Ok(());
        };
        let agent = match &chosen {
            Some(id) => self.store().agent(id).await?,
            None => None,
        };
        if let Some(agent) = &agent {
            for option in &stage.options {
                if let StageOption::Agent(name) = option
                    && name.eq_ignore_ascii_case(&agent.name)
                {
                    return Ok(());
                }
            }
        }
        let chose = agent.map_or_else(|| "The agent it chose".to_owned(), |a| a.name);
        let runs = stage.agents_line();
        let mut tx = self.store().begin_write().await?;
        sqlx::query("DELETE FROM ticket_stage_choices WHERE ticket_id = ? AND stage_id = ?")
            .bind(&ticket.id)
            .bind(&stage.id)
            .execute(&mut *tx)
            .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::ChoiceDropped,
            &Actor::current_user(),
            json!({ "stage": stage.name, "stage_id": stage.id, "chose": chose, "runs": runs }),
        )
        .await?;
        tx.commit().await?;
        tracing::info!(ticket = %ticket.key, stage = %stage.id, %chose, %runs, "a removed choice falls back");
        Ok(())
    }
}

/// What a waiting change waits for, from the flow in use `old` to `new`: the whole board when a swap is in it, else
/// its span (empty when nothing is left to wait for).
fn wait_of(old: &Flow, new: &Flow, whole_board: bool) -> Wait {
    match whole_board {
        true => Wait::WholeBoard,
        false => Wait::Span(affected_span(old, new).unwrap_or_default()),
    }
}

/// A change to make the project's waiting one ([`Board::merge_waiting`]).
#[derive(Clone, Copy)]
struct Waiting<'a> {
    project: &'a crate::store::projects::Project,
    /// The flow document it applies.
    target: &'a str,
    labels: &'a [String],
    applied_by: AppliedBy,
    agent_name: Option<&'a str>,
    preset: Option<&'a str>,
    /// A swap: it waits for the whole board.
    whole_board: bool,
}

/// The version "Back to the previous flow" goes to from `current` (FLW-09): the one before it, except when that one
/// is only the part of the current change that applied while the rest waited (the earlier flow with the current
/// one's additions and edits, [`applied_now`], or a swap's edits, [`edits_only`]): the user never chose that
/// in-between flow, so Back skips it to the flow they had before the change.
pub fn previous_version(versions: &[FlowVersion], current: u32) -> u32 {
    let find = |v: u32| versions.iter().find(|x| x.version == v);
    let before = current.saturating_sub(1);
    let flow = |v: &FlowVersion| v.file().ok().map(|f| f.flow);
    if let (Some(now), Some(part), Some(earlier)) = (
        find(current).and_then(flow),
        find(before).and_then(flow),
        find(before.saturating_sub(1)).and_then(flow),
    ) && part != earlier
        && part != now
        && (applied_now(&earlier, &now) == part || edits_only(&earlier, &now) == part)
    {
        return before - 1;
    }
    before
}

/// "Quick developer is no longer an option for Dev · runs Rust developer" (History, FLW-05).
pub fn dropped_choice_line(chose: &str, stage: &str, runs: &str) -> String {
    format!("{chose} is no longer an option for {stage} \u{b7} runs {runs}")
}
