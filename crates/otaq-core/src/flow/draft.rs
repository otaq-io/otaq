//! The draft (FED-02, FED-08; d0115 "The user edits stages directly", "Both edit the same draft"): the flow a project
//! is being changed to, in the flow editor, before the user applies it.
//!
//! **Where it lives**: the project's open proposal (`flow_proposals`), so nothing new is stored: one per project, or
//! during Flow setup the user's own beside one entry per asked tool (ASK-03, FED-03). What it remembers besides its
//! document ([`DraftMemory`]: the marks, the draft before the agent's changes, whether it is a swap) is kept in the
//! row's `source_json`. With no open proposal the draft is the flow as it will be, unchanged, and nothing is stored
//! until something changes.
//!
//! **Who edits it**: the editor reads it ([`Board::draft`], `flow.draft`) and replaces it whole
//! ([`Board::update_draft`], `flow.draft_update`), as the CLI does. A Project agent's `otaq flow propose` replaces its
//! content too ([`Board::propose_into_draft`]) instead of opening a separate preview: each stage the agent changed is
//! **marked** "changed by <agent>" ([`StageMark`]: Added, New, Changed, Moved, Removed, with its why line), and the
//! draft before the agent's first change is kept, so **Undo** takes one stage back and **Undo all** all of them. A
//! later change of the user's keeps a mark while that stage still differs from the draft before. **Start from**
//! copies another flow in ([`DraftEdit::StartFrom`], [`DraftEdit::Entry`]): applying it is a **swap**, which waits
//! for the whole board (FED-08). **Cancel** drops it (`flow.cancel_proposal`); **Apply** applies it (`flow.apply`).
//!
//! **When it applies** ([`DraftApplies`], the editor's change line): additions and edits at once; a removal or reorder
//! once no ticket is inside its affected span ([`super::apply::affected_span`]); a swap once the board is quiet; with a
//! change already waiting, together with it.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::apply::{ProposeFrom, Span, Wait, affected_span, still_working};
use super::model::{Flow, FlowFile, Stage};
use super::proposal::{
    ApplyChoices, Change, Proposal, ProposedBy, Row, Source, applied_now, edits_only,
    effective_file, match_ids, stage_changes,
};
use super::yaml;
use crate::agents::{Agent, ModelCatalog};
use crate::board::{Board, BoardError, Refusal};
use crate::domain::TicketKey;
use crate::store::now_ms;

/// What a Project agent did to a stage of the draft (the editor's tint and tag, `s27-04`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mark {
    /// A stage it added whose agents are already there ("Added").
    Added,
    /// A stage it added with an agent the draft brings ("New": its agent and skill are created on Apply).
    New,
    /// A stage it edited: name, kind, agent, skill, options, rounds, column, a Delivery's mode or target.
    Changed,
    /// A stage it put elsewhere in the order.
    Moved,
    /// A stage it took out.
    Removed,
}

/// One stage the agent changed: "changed by Checkout slowness", with its why line on hover.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageMark {
    pub stage_id: String,
    /// The stage's name in the draft (in the draft before, when removed).
    pub stage: String,
    pub mark: Mark,
    /// The Project agent's name.
    pub by: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

/// What the draft remembers besides its document, in its proposal row's `source_json`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DraftMemory {
    /// The stages a Project agent changed, in the draft's order, then the removed ones.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<StageMark>,
    /// The draft before the agent's first change still marked (Undo all goes back to it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    /// Another flow copied in from Start from: applying it is a swap, which waits for the whole board (FED-08).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub swap: bool,
    /// The stages whose skill the Project agent is writing (FED-06).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writing: Vec<SkillWriting>,
    /// The skills the Project agent wrote for stages, Otaq's copies (FED-06).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub written: Vec<WrittenSkill>,
}

/// A stage whose skill the Project agent is writing (FED-06, `s27-06`: "Being written by Noyan…" with
/// Stop): from the user's **Ask the agent to write it** until the agent hands the folder in, or the user stops it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillWriting {
    pub stage_id: String,
    /// The Project agent asked (its id), when known: Stop interrupts it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

/// A skill the Project agent wrote for a stage (FED-06, `s27-06`: tinted, "Written by the agent", Open, Undo):
/// imported as Otaq's own copy (`$OTAQ_HOME/skills/<copy>/`, d0092), never into the repo (d0023). The draft names it
/// as the stage's skill, and the stage's agent brings the copy's folder, which Apply gives it ([`super::apply`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrittenSkill {
    pub stage_id: String,
    /// Its own name (its SKILL.md's `name:`): `plan-ticket`.
    pub skill: String,
    /// Otaq's id for the copy: `skills/<copy>/`.
    pub copy: String,
    /// The Project agent's name.
    pub by: String,
    /// The stage's skill before (Undo puts it back): none for a new stage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub was: Option<String>,
}

impl DraftMemory {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A ticket inside what a change waits for, and the stage it is in (or waits before).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inside {
    pub ticket: TicketKey,
    pub stage: String,
}

/// When the draft would reach tickets if applied now (FED-08; the change line: "applies at once", "This waits until
/// no ticket is between Dev and Review: the 6 there now…").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "applies", rename_all = "snake_case")]
pub enum DraftApplies {
    /// Nothing differs from the flow as it will be.
    Nothing,
    /// Additions and edits only (or nothing inside what it would wait for).
    AtOnce,
    /// A removal or reorder: it waits while `inside` (the tickets at work in the span now) isn't empty, and no
    /// ticket enters the span meanwhile.
    Span { span: Span, inside: Vec<Inside> },
    /// A swap to another flow: it waits for the whole board, `inside` the tickets at work now.
    WholeBoard { inside: Vec<Inside> },
    /// A change already waits: this one merges into it and applies with it (FLW-06).
    WithTheWaitingChange,
}

/// The project's draft (`flow.draft`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Draft {
    /// The draft checked and compared with the flow as it will be (its problems on their stages, the changes). Its
    /// `id` is the open proposal holding it, which Apply takes; `0` while nothing is stored (the flow as it will be).
    pub proposal: Proposal,
    /// The stages a Project agent changed.
    pub marks: Vec<StageMark>,
    /// Undo all has something to go back to.
    pub can_undo_all: bool,
    /// Copied from another flow: applying it is a swap.
    pub swap: bool,
    pub applies: DraftApplies,
    /// The stages whose skill the Project agent is writing.
    pub writing: Vec<SkillWriting>,
    /// The skills it wrote.
    pub written: Vec<WrittenSkill>,
}

/// A change to the draft (`flow.draft_update`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "edit", rename_all = "snake_case")]
pub enum DraftEdit {
    /// The whole document, as the editor has it now (as `flow propose --stdin` takes it).
    Document { yaml: String },
    /// Undo one stage the agent changed: back to the draft before.
    UndoStage { stage: String },
    /// Undo every stage the agent changed.
    UndoAll,
    /// Start from a preset, another project, an earlier flow or a file (Import a flow…): copied in, a swap.
    StartFrom { source: ProposeFrom },
    /// Start from a tool's proposal under Made for <project> (Flow setup, FED-03): copied in, a swap.
    Entry { proposal: u32 },
    /// Start from the flow in use: the draft goes back to it.
    FlowInUse,
    /// **Ask the agent to write it** (FED-06): the stage's skill is being written by the Project agent `agent`.
    SkillWriting {
        stage: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        agent: Option<String>,
    },
    /// **Stop** on a skill being written: the field is empty again and the folder the agent writes in goes.
    StopWriting { stage: String },
    /// **Undo** on a skill the agent wrote: the stage's skill as it was, and Otaq's copy deleted.
    UndoSkill { stage: String },
}

/// `flow.draft_update`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DraftUpdateParams {
    pub project: String,
    pub edit: DraftEdit,
}

/// The marks of the changes from `before` to `now` (one per stage: Added, New and Removed first, then Changed over
/// Moved), by the agent `by`; `keep`, when given, limits them to those stages.
fn marks_between(
    before: &FlowFile,
    now: &FlowFile,
    by: &str,
    stored: &[Agent],
    keep: Option<&HashSet<String>>,
) -> Vec<StageMark> {
    let mut marks: Vec<StageMark> = Vec::new();
    for change in stage_changes(&before.flow, &now.flow, false) {
        if keep.is_some_and(|k| !k.contains(&change.stage_id)) {
            continue;
        }
        let stage = now.flow.stage(&change.stage_id);
        let mark = match change.change {
            Change::Added { .. } if stage.is_some_and(|s| brings_an_agent(s, stored)) => Mark::New,
            Change::Added { .. } => Mark::Added,
            Change::Removed => Mark::Removed,
            Change::Moved { .. } => Mark::Moved,
            _ => Mark::Changed,
        };
        if let Some(earlier) = marks.iter_mut().find(|m| m.stage_id == change.stage_id) {
            if earlier.mark == Mark::Moved && mark == Mark::Changed {
                earlier.mark = Mark::Changed;
            }
            continue;
        }
        marks.push(StageMark {
            stage_id: change.stage_id,
            stage: change.stage,
            mark,
            by: by.to_owned(),
            why: stage.and_then(|s| s.why.clone()),
        });
    }
    marks
}

/// Whether `stage` names an agent that isn't one of the stored ones (the draft brings it).
fn brings_an_agent(stage: &Stage, stored: &[Agent]) -> bool {
    stage
        .every_agent()
        .any(|name| !stored.iter().any(|a| a.name.eq_ignore_ascii_case(name)))
}

/// `draft` with the stage `id` as it was in `before`: an added stage goes, a removed or moved one is put back after
/// the stage before it in `before` (first when none is there), an edited one takes its old fields where it is. The
/// agents the document brings that no stage names any more, and that `before` didn't have, go too.
fn undo_stage(before: &FlowFile, draft: &FlowFile, id: &str) -> FlowFile {
    let mut file = draft.clone();
    let stages = &mut file.flow.stages;
    match before.flow.stage(id) {
        None => stages.retain(|s| s.id != id),
        Some(old) => match stages.iter().position(|s| s.id == id) {
            Some(at) if same_place(&before.flow, &file.flow, id) => {
                file.flow.stages[at] = old.clone()
            }
            _ => {
                file.flow.stages.retain(|s| s.id != id);
                let at = before.flow.position(id).unwrap_or(0);
                let after = before.flow.stages[..at]
                    .iter()
                    .rev()
                    .find_map(|s| file.flow.position(&s.id))
                    .map_or(0, |p| p + 1);
                file.flow.stages.insert(after, old.clone());
            }
        },
    }
    let named: HashSet<String> = file
        .flow
        .stages
        .iter()
        .flat_map(Stage::every_agent)
        .map(|n| n.to_lowercase())
        .collect();
    file.agents.retain(|a| {
        named.contains(&a.name.to_lowercase())
            || before
                .agents
                .iter()
                .any(|b| b.name.eq_ignore_ascii_case(&a.name))
    });
    file
}

/// Whether stage `id` follows the same stage in `a` and `b` (it wasn't moved).
fn same_place(a: &Flow, b: &Flow, id: &str) -> bool {
    let before = |f: &Flow| {
        f.position(id)
            .and_then(|i| i.checked_sub(1))
            .map(|i| f.stages[i].id.clone())
    };
    before(a) == before(b)
}

pub(super) fn read(text: &str) -> Result<FlowFile, BoardError> {
    yaml::read(text)
        .map_err(|e| BoardError::refused(Refusal::BadFlow, format!("The flow doesn't read: {e}")))
}

impl Board {
    /// `flow.draft`: the project's draft, checked and compared now, with its marks and when it would apply.
    pub async fn draft(
        &self,
        project: &str,
        models: &dyn ModelCatalog,
    ) -> Result<Draft, BoardError> {
        let row = match self.draft_row(project).await? {
            Some(row) => row,
            None => self.undrafted(project).await?,
        };
        let proposal = self.view(&row, models, &ApplyChoices::default()).await?;
        let applies = self.draft_applies(&row, &proposal).await?;
        Ok(Draft {
            proposal,
            marks: row.draft.marks.clone(),
            can_undo_all: row.draft.before.is_some(),
            swap: row.draft.swap,
            applies,
            writing: row.draft.writing.clone(),
            written: row.draft.written.clone(),
        })
    }

    /// `flow.draft_update`: changes the draft by `edit`, stored as the project's open proposal. The user's document
    /// may have problems (the editor shows them on their stages, and Apply waits for their fixes); one that doesn't
    /// read is refused.
    pub async fn update_draft(
        &self,
        project: &str,
        edit: &DraftEdit,
        models: &dyn ModelCatalog,
    ) -> Result<Draft, BoardError> {
        let key = self.project(project).await?.key;
        let existing = self.draft_row(&key).await?;
        let mut row = match &existing {
            Some(row) => row.clone(),
            None => self.undrafted(&key).await?,
        };
        let stored_agents = self.store().agents().await?;
        match edit {
            DraftEdit::Document { yaml: text } => {
                let (file, written) = yaml::read_with_written_ids(text).map_err(|e| {
                    BoardError::refused(Refusal::BadFlow, format!("The flow doesn't read: {e}"))
                })?;
                let (base, _) = self.flow_as_it_will_be(&key).await?;
                let mut file = file;
                match_ids(&base.flow, &mut file.flow, &written);
                row.yaml = yaml::write(&file);
                row.draft.marks = kept_marks(&row.draft, &file, &stored_agents, None)?;
            }
            DraftEdit::UndoStage { stage } => {
                let Some(before) = &row.draft.before else {
                    return Err(BoardError::refused(
                        Refusal::BadFlow,
                        "Nothing the agent changed is left to undo",
                    ));
                };
                if !row.draft.marks.iter().any(|m| &m.stage_id == stage) {
                    return Err(BoardError::refused(
                        Refusal::BadFlow,
                        format!("The agent didn't change the stage {stage}"),
                    ));
                }
                let file = undo_stage(&read(before)?, &read(&row.yaml)?, stage);
                row.yaml = yaml::write(&file);
                row.draft.marks =
                    kept_marks(&row.draft, &file, &stored_agents, Some(stage.as_str()))?;
            }
            DraftEdit::UndoAll => {
                if let Some(before) = row.draft.before.take() {
                    row.yaml = before;
                }
                row.draft.marks.clear();
            }
            DraftEdit::StartFrom { source } => {
                self.propose_from(&key, source, models).await?;
                return self.draft(&key, models).await;
            }
            DraftEdit::Entry { proposal } => {
                let entry = self
                    .proposal_row_by_id(&key, *proposal)
                    .await?
                    .filter(|r| Some(r.id) != existing.as_ref().map(|e| e.id))
                    .ok_or_else(|| {
                        BoardError::NotFound(format!("Proposal {proposal} isn't open any more"))
                    })?;
                row.yaml = entry.yaml;
                row.source = entry.source;
                row.proposed_by = entry.proposed_by;
                row.draft = DraftMemory {
                    swap: true,
                    ..DraftMemory::default()
                };
            }
            DraftEdit::FlowInUse => {
                let fresh = self.undrafted(&key).await?;
                row.yaml = fresh.yaml;
                row.source = fresh.source;
                row.draft = DraftMemory::default();
            }
            DraftEdit::SkillWriting { stage, agent } => {
                let id = super::write_skill::stage_of(&read(&row.yaml)?, stage)?
                    .id
                    .clone();
                row.draft.writing.retain(|w| w.stage_id != id);
                row.draft.writing.push(SkillWriting {
                    stage_id: id,
                    agent: agent.clone(),
                });
            }
            DraftEdit::StopWriting { stage } => {
                let id = super::write_skill::stage_of(&read(&row.yaml)?, stage)?
                    .id
                    .clone();
                row.draft.writing.retain(|w| w.stage_id != id);
                self.drop_skill_folder(&key, &id).await;
            }
            DraftEdit::UndoSkill { stage } => {
                let file = read(&row.yaml)?;
                let id = super::write_skill::stage_of(&file, stage)?.id.clone();
                let Some(at) = row.draft.written.iter().position(|w| w.stage_id == id) else {
                    return Err(BoardError::refused(
                        Refusal::BadFlow,
                        format!("The agent wrote no skill for {stage}"),
                    ));
                };
                let written = row.draft.written.remove(at);
                row.yaml = yaml::write(&self.without_written(file, &written)?);
                self.drop_copy(&written.copy).await;
            }
        }
        self.keep_skills(&mut row, &stored_agents).await?;
        if row.draft.marks.is_empty() {
            row.draft.before = None;
        }
        self.write_draft(&key, existing.map(|e| e.id), &row).await?;
        self.draft(&key, models).await
    }

    /// A proposal from `otaq flow propose` outside Flow setup (FED-02): it replaces the draft's content instead of
    /// opening a separate preview. A Project agent's marks each stage it changed against the draft before its first
    /// change still marked, and becomes the one the answer goes to; the user's own (their terminal) keeps a mark only
    /// where that stage still differs. `--from` another project copies another flow in: a swap. `file` was checked.
    pub(crate) async fn propose_into_draft(
        &self,
        key: &str,
        file: FlowFile,
        source: &Source,
        by: &ProposedBy,
        models: &dyn ModelCatalog,
    ) -> Result<Proposal, BoardError> {
        let existing = self.draft_row(key).await?;
        let mut row = match &existing {
            Some(row) => row.clone(),
            None => self.undrafted(key).await?,
        };
        let stored_agents = self.store().agents().await?;
        row.draft.swap |= matches!(source, Source::Project { .. });
        match by {
            ProposedBy::ProjectAgent { name, .. } => {
                let before = match (&row.draft.before, row.draft.marks.is_empty()) {
                    (Some(before), false) => before.clone(),
                    _ => row.yaml.clone(),
                };
                row.draft.marks = marks_between(&read(&before)?, &file, name, &stored_agents, None);
                row.draft.before = Some(before);
                row.proposed_by = by.clone();
            }
            ProposedBy::User => {
                row.draft.marks = kept_marks(&row.draft, &file, &stored_agents, None)?;
            }
        }
        row.yaml = yaml::write(&file);
        row.source = source.clone();
        self.keep_skills(&mut row, &stored_agents).await?;
        if row.draft.marks.is_empty() {
            row.draft.before = None;
        }
        let id = self.write_draft(key, existing.map(|e| e.id), &row).await?;
        let row = Row { id, ..row };
        let proposal = self.view(&row, models, &ApplyChoices::default()).await?;
        tracing::info!(project = %key, proposal = id, marks = row.draft.marks.len(), "a proposal landed in the draft");
        Ok(proposal)
    }

    /// The row holding the draft: the open proposal; during Flow setup the user's own (no tool), beside the tools'
    /// entries.
    pub(crate) async fn draft_row(&self, project: &str) -> Result<Option<Row>, BoardError> {
        let project = self.project(project).await?;
        let id: Option<i64> = match project.flow_setup_since {
            Some(_) => {
                sqlx::query_scalar(
                    "SELECT id FROM flow_proposals WHERE project_id = ? AND tool IS NULL ORDER BY id DESC LIMIT 1",
                )
                .bind(&project.id)
                .fetch_optional(self.store().pool())
                .await?
            }
            None => {
                sqlx::query_scalar(
                    "SELECT id FROM flow_proposals WHERE project_id = ? ORDER BY id DESC LIMIT 1",
                )
                .bind(&project.id)
                .fetch_optional(self.store().pool())
                .await?
            }
        };
        match id {
            Some(id) => {
                self.proposal_row_by_id(&project.key, u32::try_from(id).unwrap_or(0))
                    .await
            }
            None => Ok(None),
        }
    }

    /// The draft while nothing is stored: the flow as it will be, as a document with the agents it names.
    pub(super) async fn undrafted(&self, project: &str) -> Result<Row, BoardError> {
        let key = self.project(project).await?.key;
        let (base, _) = self.flow_as_it_will_be(&key).await?;
        Ok(Row {
            id: 0,
            project: key,
            yaml: self.document(&base.flow).await?,
            source: Source::Document { file: None },
            proposed_by: ProposedBy::User,
            draft: DraftMemory::default(),
            created_at: now_ms(),
        })
    }

    /// Stores `row` as the draft: in place of the row `existing`, else as a new open proposal (outside Flow setup it
    /// replaces any other, FLW-03). Its id.
    pub(super) async fn write_draft(
        &self,
        project: &str,
        existing: Option<u32>,
        row: &Row,
    ) -> Result<u32, BoardError> {
        let project = self.project(project).await?;
        let mut tx = self.store().begin_write().await?;
        let id = match existing {
            Some(id) => {
                sqlx::query(
                    "UPDATE flow_proposals SET yaml = ?, source_json = ?, proposed_by_json = ? WHERE id = ?",
                )
                .bind(&row.yaml)
                .bind(row.source_json())
                .bind(serde_json::to_string(&row.proposed_by).expect("a proposer serializes"))
                .bind(i64::from(id))
                .execute(&mut *tx)
                .await?;
                id
            }
            None => {
                let replaced = match project.flow_setup_since {
                    Some(_) => "DELETE FROM flow_proposals WHERE project_id = ? AND tool IS NULL",
                    None => "DELETE FROM flow_proposals WHERE project_id = ?",
                };
                sqlx::query(replaced)
                    .bind(&project.id)
                    .execute(&mut *tx)
                    .await?;
                let id: i64 = sqlx::query_scalar(
                    "INSERT INTO flow_proposals (project_id, tool, yaml, source_json, proposed_by_json, created_at)
                     VALUES (?, NULL, ?, ?, ?, ?) RETURNING id",
                )
                .bind(&project.id)
                .bind(&row.yaml)
                .bind(row.source_json())
                .bind(serde_json::to_string(&row.proposed_by).expect("a proposer serializes"))
                .bind(now_ms())
                .fetch_one(&mut *tx)
                .await?;
                u32::try_from(id).unwrap_or(0)
            }
        };
        tx.commit().await?;
        Ok(id)
    }

    /// When the draft `row` (checked as `proposal`) would reach tickets if applied now ([`Board::apply_proposal`]'s
    /// rule).
    async fn draft_applies(
        &self,
        row: &Row,
        proposal: &Proposal,
    ) -> Result<DraftApplies, BoardError> {
        if proposal.diff.same {
            return Ok(DraftApplies::Nothing);
        }
        if proposal.diff.stages.is_empty() {
            return Ok(DraftApplies::AtOnce);
        }
        if proposal.diff.against_waiting {
            return Ok(DraftApplies::WithTheWaitingChange);
        }
        let (base, _) = self.flow_as_it_will_be(&row.project).await?;
        let file = effective_file(
            &read(&row.yaml)?,
            row.source.scope(),
            &base.flow,
            &row.source,
            &ApplyChoices::default(),
        );
        let swap = row.draft.swap;
        let now = match swap {
            true => edits_only(&base.flow, &file.flow),
            false => applied_now(&base.flow, &file.flow),
        };
        let wait = match swap {
            true => proposal.diff.waits.then_some(Wait::WholeBoard),
            false => affected_span(&now, &file.flow).map(Wait::Span),
        };
        let Some(wait) = wait else {
            return Ok(DraftApplies::AtOnce);
        };
        let tickets = self.tickets(&row.project).await?;
        let delivering = self.delivering().await?;
        let mut inside: Vec<Inside> = still_working(&tickets, &file.flow, &delivering, &wait)
            .into_iter()
            .map(|t| Inside {
                ticket: t.key.clone(),
                stage: t.place.stage_id().unwrap_or_default().to_owned(),
            })
            .collect();
        inside.sort_by(|a, b| a.ticket.cmp(&b.ticket));
        Ok(match wait {
            Wait::WholeBoard => DraftApplies::WholeBoard { inside },
            Wait::Span(span) => DraftApplies::Span { span, inside },
        })
    }
}

/// The marks of `memory` that still hold for the draft `now`: those stages that still differ from the draft before
/// (by the agent that changed them), less `undone`.
fn kept_marks(
    memory: &DraftMemory,
    now: &FlowFile,
    stored: &[Agent],
    undone: Option<&str>,
) -> Result<Vec<StageMark>, BoardError> {
    let Some(before) = &memory.before else {
        return Ok(Vec::new());
    };
    let keep: HashSet<String> = memory
        .marks
        .iter()
        .map(|m| m.stage_id.clone())
        .filter(|id| Some(id.as_str()) != undone)
        .collect();
    let mut marks = marks_between(&read(before)?, now, "", stored, Some(&keep));
    for mark in &mut marks {
        if let Some(earlier) = memory.marks.iter().find(|m| m.stage_id == mark.stage_id) {
            mark.by.clone_from(&earlier.by);
        }
    }
    Ok(marks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::default_flow;

    fn file(flow: Flow) -> FlowFile {
        FlowFile {
            agents: Vec::new(),
            flow,
        }
    }

    /// Undo puts an added stage out, a removed or moved one back where it was, an edited one's old fields in place.
    #[test]
    fn undo_one_stage() {
        let before = file(default_flow());
        let mut now = before.clone();
        let mut plan = now.flow.stages[0].clone();
        plan.id = "plan".into();
        plan.name = "Plan".into();
        now.flow.stages.insert(0, plan);
        now.flow.stages.retain(|s| s.id != "review");
        now.flow.stages.swap(1, 2);
        now.flow.stages[2].agent = Some("Rust developer".into());
        let ids =
            |f: &FlowFile| -> Vec<String> { f.flow.stages.iter().map(|s| s.id.clone()).collect() };
        assert_eq!(ids(&now), ["plan", "validate", "dev", "deliver"]);
        let marks = marks_between(&before, &now, "Checkout slowness", &[], None);
        let tags: Vec<(&str, Mark)> = marks
            .iter()
            .map(|m| (m.stage_id.as_str(), m.mark))
            .collect();
        assert_eq!(
            tags,
            [
                ("plan", Mark::New),
                ("dev", Mark::Changed),
                ("review", Mark::Removed)
            ]
        );
        assert!(marks.iter().all(|m| m.by == "Checkout slowness"));

        let without_plan = undo_stage(&before, &now, "plan");
        assert_eq!(ids(&without_plan), ["validate", "dev", "deliver"]);
        let review_back = undo_stage(&before, &now, "review");
        assert_eq!(
            ids(&review_back),
            ["plan", "validate", "review", "dev", "deliver"]
        );
        let dev_back = undo_stage(&before, &now, "dev");
        assert_eq!(ids(&dev_back), ["dev", "plan", "validate", "deliver"]);
        assert_eq!(dev_back.flow.stage("dev"), before.flow.stage("dev"));
    }

    /// The memory reads back from a row's `source_json`, beside the source's own fields.
    #[test]
    fn the_memory_rides_in_the_source() {
        let row = Row {
            id: 3,
            project: "SHOP".into(),
            yaml: String::new(),
            source: Source::Preset {
                file: "grill-first.yaml".into(),
            },
            proposed_by: ProposedBy::User,
            draft: DraftMemory {
                swap: true,
                ..DraftMemory::default()
            },
            created_at: 0,
        };
        let json = row.source_json();
        assert_eq!(
            json,
            r#"{"kind":"preset","file":"grill-first.yaml","draft":{"swap":true}}"#
        );
        let plain: Source = serde_json::from_str(&json).unwrap();
        assert_eq!(plain, row.source);
    }
}
