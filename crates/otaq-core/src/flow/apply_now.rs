//! **Apply now** on a waiting flow change (APN-02…APN-05; d0108 "A flow change waits for a quiet board" with its Apply
//! now of 2026-10-21, "Dragging a card back to any earlier stage", "Other drags", "After a Fail"; d0045).
//!
//! **The plan** ([`plan`], `flow.apply_now_plan`): from the waiting change ([`Board::waiting_change`]; a merged change
//! counts whole) and [`super::apply::still_working`]: the change's summary ("Removes Grill and Security from shop-web's
//! flow"); the **moves**, one per ticket in a stage the new flow doesn't have (in it, or waiting before it), each with
//! whether its agent will stop, its state line ("failed ×3"), its choices and its default; the tickets that **keep**
//! their place (every other ticket still working inside what the change waits for, its span or the whole board,
//! FED-08: "keeps running" or "keeps its place"); and a **stamp** of the board
//! the plan was made from.
//!
//! **The removed stage's place** in the new flow is just before the first surviving stage that followed it in the old
//! flow ([`removed_place`]). A ticket's choices are Backlog, Ready for Dev and every stage of the new flow.
//! A stage before the place is **back**, the first one the ticket runs after it is the **next** (the default), any
//! later one is **forward**. Each is decided as a drag from the place would be (FLW-18): the moves are computed by
//! [`crate::flow::engine::dragged`] on the new flow with the removed stage standing at its place ([`with_removed`]).
//!
//! **Applying** ([`Board::apply_now_writes`], carried out by `crate::review::Review::apply_now`): a stale stamp or a
//! pick that isn't one of its ticket's choices is refused with the fresh plan. Otherwise the new flow is stored, the
//! waiting change goes and every move is written in **one transaction**; only then do the sessions in removed stages
//! stop (ended by Otaq, so the board's session-end handling leaves their tickets alone) and starts resume. A failure in
//! any move leaves the board and the sessions as they were. The runs of removed stages end stopped; each move's
//! History names the stage that went ("Moved to Review · by you · Security was removed", [`removed_note`]). Tickets
//! in surviving stages are not touched.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

use super::apply::{Wait, still_working};
use super::engine::{self, At, RunResult, skipped_by_drag};
use super::model::{Flow, Stage};
use super::store::{FlowVersion, NewFlowVersion};
use super::yaml;
use crate::board::moves::{DragWrite, DropOn, DropTarget, record_drag_in};
use crate::board::send_back::{SentBackTo, send_back_in};
use crate::board::stage_runs::{Moved, end_open};
use crate::board::tickets::{EventKind, Ticket, record_event};
use crate::board::{Board, BoardError};
use crate::domain::{Actor, MovedPast, Place, Status, TicketKey};
use crate::store::tickets::set_state_since;
use crate::store::{Store, now_ms};

/// "Moving forward skips the stages in between, as on the card" (APN-02's hint on a forward pick).
pub const FORWARD_HINT: &str = "Moving forward skips the stages in between, as on the card";

/// "Security was removed": a move's History note (APN-04).
pub fn removed_note(stage: &str) -> String {
    format!("{stage} was removed")
}

/// "Removes Grill and Security from shop-web's flow" (APN-02).
pub fn removes_summary(stages: &[String], project: &str) -> String {
    let names = match stages {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    };
    format!("Removes {names} from {project}\u{2019}s flow")
}

/// The removed stages as the summary names them: a human check with "(your check)" after it ("QA (your check)").
fn removed_labels(old: &Flow, new: &Flow) -> Vec<String> {
    old.stages
        .iter()
        .filter(|s| new.stage(&s.id).is_none())
        .map(|s| match s.is_human_check() {
            true => format!("{} (your check)", s.name),
            false => s.name.clone(),
        })
        .collect()
}

/// "Dev resumes SHOP-19's earlier session" (APN-02's hint on a pick back to an Agent stage that ran).
pub fn resumes_hint(stage: &str, key: &TicketKey) -> String {
    format!("{stage} resumes {key}\u{2019}s earlier session")
}

/// "Dev starts a new session for SHOP-12": a pick back to an Agent stage the ticket never ran (it skipped it).
pub fn fresh_hint(stage: &str, key: &TicketKey) -> String {
    format!("{stage} starts a new session for {key}")
}

/// "Security runs again on SHOP-19": a pick back to an agent check.
pub fn runs_again_hint(stage: &str, key: &TicketKey) -> String {
    format!("{stage} runs again on {key}")
}

/// What Apply now would do right now (`flow.apply_now_plan`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyNowPlan {
    /// The project's key.
    pub project: String,
    /// The project's name ("shop-web").
    pub project_name: String,
    /// "Removes Grill and Security from shop-web's flow", "Removes QA (your check) from shop-web's flow"; the waiting
    /// change's own words ("QA after Security") when it removes no stage.
    pub summary: String,
    /// What the waiting change changes, every merged change's parts in order (the banner's words).
    pub changes: Vec<String>,
    /// The names of the stages it removes, in the old flow's order.
    pub removed: Vec<String>,
    /// One per ticket in a stage the new flow doesn't have, in key order.
    pub moves: Vec<PlannedMove>,
    /// Every other ticket still working, in key order ("Keep their place").
    pub keep: Vec<KeptTicket>,
    /// The board the plan was made from: `flow.apply_now` with another one is refused with the fresh plan.
    pub stamp: String,
}

/// One ticket in a removed stage, and where it can go.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedMove {
    pub ticket: TicketKey,
    pub title: String,
    /// The removed stage's id and name ("in Grill (removed)").
    pub stage: String,
    pub stage_name: String,
    /// Waiting before that (Agent) stage rather than in it.
    pub waiting: bool,
    /// A session runs there and stops ("· agent will stop").
    pub agent_will_stop: bool,
    pub status: Status,
    /// Its state when one shows on the card ("failed ×3").
    pub state: Option<String>,
    /// Where it can go, in board order: Backlog, Ready for Dev, then the new flow's stages.
    pub choices: Vec<MoveChoice>,
    /// The choice the picker starts on: the first stage it runs after the removed stage's place.
    pub default: DropOn,
}

impl PlannedMove {
    /// The choice for `to`, if it is one.
    pub fn choice(&self, to: &DropOn) -> Option<&MoveChoice> {
        self.choices.iter().find(|c| c.to == *to)
    }
}

/// One entry of a move's picker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveChoice {
    /// `"backlog"`, `"ready_for_dev"` or `{"stage": "<id>"}`.
    pub to: DropOn,
    /// "Backlog", "Ready for Dev" or the stage's name.
    pub name: String,
    pub way: Way,
    /// The hint line under the picker when it is chosen ("Dev resumes SHOP-19's earlier session").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

/// What a pick does (APN-03), as a drag from the removed stage's place would.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Way {
    /// Parked (d0052): its agent stops, its work is kept.
    Backlog,
    /// Queued to start the flow again from its first stage, its work kept.
    ReadyForDev,
    /// A stage before the place: an Agent stage resumes its session (a human Fail), an agent check runs again, a human
    /// check waits ("Moved back by you").
    Back,
    /// The first stage it runs after the place.
    Next,
    /// A later stage: the stages between are skipped by you, a Delivery then waits for a click.
    Forward,
}

/// A ticket still working in a stage the change keeps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptTicket {
    pub ticket: TicketKey,
    pub title: String,
    /// The stage it is in or waits before, and its name.
    pub stage: String,
    pub stage_name: String,
    /// A session runs for it ("keeps running"), else "keeps its place".
    pub keeps_running: bool,
}

/// `flow.apply_now`'s parameters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplyNowParams {
    pub project: String,
    /// The plan's stamp.
    pub stamp: String,
    /// The user's picks; a move without one goes to its default.
    #[serde(default)]
    pub picks: Vec<Pick>,
}

/// The user's choice for one move.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    pub ticket: TicketKey,
    /// `"backlog"`, `"ready_for_dev"` or `{"stage": "<id>"}`, one of the move's choices.
    pub to: DropOn,
}

/// What `flow.apply_now` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ApplyNowAnswer {
    /// The change applied: the version stored, the tickets moved and those whose agent stopped, in key order. Toast:
    /// "shop-web's flow changed".
    Applied {
        version: Box<FlowVersion>,
        moved: Vec<TicketKey>,
        stopped: Vec<TicketKey>,
    },
    /// Nothing applied: the board changed since the plan, or a pick isn't one of its ticket's choices. The window
    /// shows the fresh plan.
    Refreshed {
        plan: Box<ApplyNowPlan>,
        why: Refreshed,
    },
    /// The change applied on its own meanwhile (the board went quiet): the window closes with the usual toast.
    AlreadyApplied { version: Box<FlowVersion> },
    /// No change waits any more (Cancel change meanwhile): the window closes.
    NothingWaits,
}

/// Why Apply now answered with a fresh plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "why", rename_all = "snake_case")]
pub enum Refreshed {
    /// A ticket entered a removed stage, one finished, a session started or stopped…
    BoardChanged,
    /// The pick for `ticket` isn't one of its choices (or `ticket` has no move).
    NotAChoice { ticket: TicketKey },
}

/// The sessions running now, by ticket id: agents and checkers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Live {
    pub agents: HashSet<String>,
    pub checkers: HashSet<String>,
}

/// What [`plan`] reads.
#[derive(Debug, Clone, Copy)]
pub struct PlanFacts<'a> {
    pub project: &'a str,
    pub project_name: &'a str,
    pub old: &'a Flow,
    pub new: &'a Flow,
    /// The waiting change's document and its parts.
    pub yaml: &'a str,
    pub changes: &'a [String],
    pub tickets: &'a [Ticket],
    /// The tickets a Delivery session works on now.
    pub delivering: &'a HashSet<String>,
    /// What the change waits for: only the tickets inside it are listed (FED-08).
    pub wait: &'a Wait,
    pub live: &'a Live,
    /// The Agent stages each ticket ran in a session of its own, by ticket id (they can resume).
    pub ran: &'a HashMap<String, HashSet<String>>,
    /// The stages each ticket skips by its choices, by ticket id.
    pub skips: &'a HashMap<String, Vec<String>>,
}

/// The removed stage `id`'s place in `new`: the index of the first stage that followed it in `old` and survives (the
/// Delivery at the latest). A stage `old` doesn't have stands before the Delivery.
pub fn removed_place(old: &Flow, new: &Flow, id: &str) -> usize {
    let end = new
        .stages
        .iter()
        .position(Stage::is_delivery)
        .unwrap_or(new.stages.len());
    match old.position(id) {
        Some(at) => old.stages[at + 1..]
            .iter()
            .find_map(|s| new.position(&s.id))
            .unwrap_or(end),
        None => end,
    }
}

/// `walk` (the new flow) with `removed` standing at `place` in `new`'s indices: where the
/// moves out of it are decided, as drags from there. Its index in the result too.
pub fn with_removed(walk: &Flow, new: &Flow, removed: &Stage, place: usize) -> (Flow, usize) {
    let at_or_after = |s: &Stage| {
        let at = match new.position(&s.id) {
            Some(at) => at,
            // A stage the new flow doesn't have stands just before the Delivery.
            None => new
                .stages
                .iter()
                .position(Stage::is_delivery)
                .unwrap_or(new.stages.len()),
        };
        at >= place
    };
    let index = walk
        .stages
        .iter()
        .position(at_or_after)
        .unwrap_or(walk.stages.len());
    let mut stages = walk.stages.clone();
    stages.insert(index, removed.clone());
    (Flow { stages }, index)
}

/// The removed stage a ticket is in or waits before, if it is one: a stage of the old flow the new flow doesn't have.
fn removed_stage(old: &Flow, new: &Flow, ticket: &Ticket) -> Option<Stage> {
    let id = ticket.place.stage_id()?;
    match new.stage(id) {
        Some(_) => None,
        None => old.stage(id).cloned(),
    }
}

/// The card's state line for a ticket in a removed stage, when one shows ("failed ×3").
pub fn state_line(ticket: &Ticket) -> Option<String> {
    Some(match ticket.status {
        Status::CheckFailed => format!("failed \u{d7}{}", ticket.validation_round.max(1)),
        Status::CheckNoResult => "no result".to_owned(),
        Status::AsksYou => "asks you".to_owned(),
        Status::NeedsYou | Status::CheckerNeedsYou => "needs you".to_owned(),
        Status::Idle => "idle".to_owned(),
        Status::Interrupted => "interrupted".to_owned(),
        Status::ResumeFailed => "resume failed".to_owned(),
        Status::WaitingForLimitReset => "waiting for the limit".to_owned(),
        Status::Held => "held".to_owned(),
        _ => return None,
    })
}

/// The plan (APN-02) from what the board holds now.
pub fn plan(facts: &PlanFacts<'_>) -> ApplyNowPlan {
    let PlanFacts { old, new, .. } = *facts;
    let mut tickets: Vec<&Ticket> = facts.tickets.iter().collect();
    tickets.sort_by(|a, b| a.key.cmp(&b.key));
    let mut moves = Vec::new();
    let mut moving = HashSet::new();
    for ticket in &tickets {
        if let Some(stage) = removed_stage(old, new, ticket) {
            moving.insert(ticket.id.clone());
            moves.push(planned_move(facts, ticket, &stage));
        }
    }
    let working: HashSet<&str> = still_working(facts.tickets, new, facts.delivering, facts.wait)
        .into_iter()
        .map(|t| t.id.as_str())
        .collect();
    let keep: Vec<KeptTicket> = tickets
        .iter()
        .filter(|t| working.contains(t.id.as_str()) && !moving.contains(&t.id))
        .map(|t| {
            let stage = t.place.stage_id().unwrap_or_default().to_owned();
            KeptTicket {
                ticket: t.key.clone(),
                title: t.title.clone(),
                stage_name: new
                    .stage(&stage)
                    .map_or_else(|| stage.clone(), |s| s.name.clone()),
                stage,
                keeps_running: facts.live.agents.contains(&t.id)
                    || facts.live.checkers.contains(&t.id),
            }
        })
        .collect();
    let removed: Vec<String> = old
        .stages
        .iter()
        .filter(|s| new.stage(&s.id).is_none())
        .map(|s| s.name.clone())
        .collect();
    let summary = match removed.is_empty() {
        true => facts.changes.join(" \u{b7} "),
        false => removes_summary(&removed_labels(old, new), facts.project_name),
    };
    let stamp = stamp_of(facts.yaml, &moves, &keep);
    ApplyNowPlan {
        project: facts.project.to_owned(),
        project_name: facts.project_name.to_owned(),
        summary,
        changes: facts.changes.to_vec(),
        removed,
        moves,
        keep,
        stamp,
    }
}

fn planned_move(facts: &PlanFacts<'_>, ticket: &Ticket, stage: &Stage) -> PlannedMove {
    let new = facts.new;
    let no_skips = Vec::new();
    let skips = facts.skips.get(&ticket.id).unwrap_or(&no_skips);
    let place = removed_place(facts.old, new, &stage.id);
    let (virtual_flow, from) = with_removed(new, new, stage, place);
    let ran = facts.ran.get(&ticket.id);
    let mut choices = vec![
        MoveChoice {
            to: DropTarget::Backlog.into(),
            name: Status::Backlog.label().to_owned(),
            way: Way::Backlog,
            hint: None,
        },
        MoveChoice {
            to: DropTarget::ReadyForDev.into(),
            name: Status::Queued.label().to_owned(),
            way: Way::ReadyForDev,
            hint: None,
        },
    ];
    for (i, s) in virtual_flow.stages.iter().enumerate() {
        if i == from {
            continue;
        }
        let way = if i < from {
            Way::Back
        } else if skipped_by_drag(&virtual_flow, skips, from, i, false).is_empty() {
            Way::Next
        } else {
            Way::Forward
        };
        let hint = match way {
            Way::Back if s.is_agent_stage() => Some(match ran.is_some_and(|r| r.contains(&s.id)) {
                true => resumes_hint(&s.name, &ticket.key),
                false => fresh_hint(&s.name, &ticket.key),
            }),
            Way::Back if s.is_agent_check() => Some(runs_again_hint(&s.name, &ticket.key)),
            Way::Forward => Some(FORWARD_HINT.to_owned()),
            _ => None,
        };
        choices.push(MoveChoice {
            to: DropOn::stage(&s.id),
            name: s.name.clone(),
            way,
            hint,
        });
    }
    let default = choices
        .iter()
        .find(|c| c.way == Way::Next)
        .or(choices.last())
        .map_or_else(|| DropTarget::Backlog.into(), |c| c.to.clone());
    let in_it = matches!(&ticket.place, Place::InStage(_));
    let agent_will_stop = in_it
        && match stage.is_agent_check() {
            true => facts.live.checkers.contains(&ticket.id),
            false => stage.is_agent_stage() && facts.live.agents.contains(&ticket.id),
        };
    PlannedMove {
        ticket: ticket.key.clone(),
        title: ticket.title.clone(),
        stage: stage.id.clone(),
        stage_name: stage.name.clone(),
        waiting: !in_it,
        agent_will_stop,
        status: ticket.status,
        state: state_line(ticket),
        choices,
        default,
    }
}

/// The stamp of a plan: the waiting change's document, then what each move and kept ticket stands on. Its first part
/// is the document's alone, so a stale stamp can tell whether that change has since applied ([`yaml_part`]).
fn stamp_of(yaml: &str, moves: &[PlannedMove], keep: &[KeptTicket]) -> String {
    let mut board = Sha256::new();
    for m in moves {
        let line = format!(
            "{}|{}|{}|{}|{}|{:?}\n",
            m.ticket,
            m.stage,
            m.waiting,
            m.status.as_str(),
            m.agent_will_stop,
            m.state
        );
        board.update(line.as_bytes());
    }
    board.update(b"--\n");
    for k in keep {
        board.update(format!("{}|{}|{}\n", k.ticket, k.stage, k.keeps_running).as_bytes());
    }
    format!("{}-{}", yaml_part(yaml), short(&board.finalize()))
}

/// The part of a stamp that names the waiting change's document.
pub fn yaml_part(yaml: &str) -> String {
    short(&Sha256::digest(yaml.as_bytes()))
}

fn short(digest: &[u8]) -> String {
    digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

/// How the pick moves its ticket, decided before anything is written ([`Board::apply_now_writes`]).
#[derive(Debug, Clone)]
pub(crate) enum MoveWrite {
    /// Parked (d0052), or back to Backlog when it has no work yet.
    Park { has_work: bool, removed: String },
    /// Back to an earlier Agent stage (a human Fail): its live session there gets Send back's skill (`Working`), or
    /// it waits before the stage to resume its conversation (`Queued`), or to start there afresh when it never ran
    /// it (`fresh`).
    SendBack {
        stage: String,
        to: Status,
        fresh: bool,
        round: u32,
        event: serde_json::Value,
    },
    /// Any other move: as a drag the board carries out by itself.
    Drag(DragWrite),
}

/// One move to write: the ticket as read, and how.
#[derive(Debug, Clone)]
pub(crate) struct PlannedWrite {
    pub ticket: Ticket,
    /// The removed stage is an Agent stage: the ticket's agent works there and stops.
    pub from_agent_stage: bool,
    pub write: MoveWrite,
}

impl PlannedWrite {
    /// Whether the ticket's agent session (if one runs) stops: it works in the removed stage, or the move parks the
    /// ticket, starts it over, or resumes the stage picked back in a new session. An agent of an earlier stage that
    /// waits while a removed check ran stays, as a forward drag keeps it.
    pub(crate) fn stops_agent(&self) -> bool {
        self.from_agent_stage
            || match &self.write {
                MoveWrite::Park { .. } => true,
                MoveWrite::SendBack { to, .. } => *to != Status::Working,
                MoveWrite::Drag(drag) => drag.moved.place == Place::Waiting(None),
            }
    }
}

impl Board {
    /// `flow.apply_now_plan`: what Apply now would do on `project` now, with `live` the sessions running; `None` when no
    /// change waits.
    pub async fn apply_now_plan(
        &self,
        project: &str,
        live: &Live,
    ) -> Result<Option<ApplyNowPlan>, BoardError> {
        let Some(waiting) = self.waiting_change(project).await? else {
            return Ok(None);
        };
        let project = self.project(project).await?;
        let new = yaml::read(&waiting.yaml)
            .map_err(|e| {
                BoardError::NotFound(format!(
                    "{}'s waiting flow change doesn't read: {e}",
                    project.key
                ))
            })?
            .flow;
        let old = self.effective_flow(&project.key).await?;
        let tickets = self.tickets(&project.key).await?;
        let delivering: HashSet<String> = sqlx::query_scalar("SELECT ticket_id FROM pr_flows")
            .fetch_all(self.store().pool())
            .await?
            .into_iter()
            .collect();
        let mut ran: HashMap<String, HashSet<String>> = HashMap::new();
        let rows: Vec<(String, String)> = sqlx::query_as(
            "SELECT DISTINCT r.ticket_id, r.stage_id FROM stage_runs r JOIN tickets t ON t.id = r.ticket_id
             WHERE t.project_id = ? AND r.kind = 'agent' AND r.session_id IS NOT NULL",
        )
        .bind(&project.id)
        .fetch_all(self.store().pool())
        .await?;
        for (ticket, stage) in rows {
            ran.entry(ticket).or_default().insert(stage);
        }
        let mut skips = HashMap::new();
        for ticket in &tickets {
            if ticket
                .place
                .stage_id()
                .is_some_and(|id| new.stage(id).is_none())
            {
                skips.insert(ticket.id.clone(), self.ticket_skips(ticket, &new).await?);
            }
        }
        Ok(Some(plan(&PlanFacts {
            project: &project.key,
            project_name: &project.name,
            old: &old,
            new: &new,
            yaml: &waiting.yaml,
            changes: &waiting.changes,
            tickets: &tickets,
            delivering: &delivering,
            wait: &waiting.wait,
            live,
            ran: &ran,
            skips: &skips,
        })))
    }

    /// The current flow version when the change the stamp names applied (on its own, meanwhile); `None` otherwise.
    pub(crate) async fn applied_since(
        &self,
        project: &str,
        stamp: &str,
    ) -> Result<Option<FlowVersion>, BoardError> {
        let current = self.current_flow(project).await?;
        Ok(stamp
            .split('-')
            .next()
            .is_some_and(|part| part == yaml_part(&current.yaml))
            .then_some(current))
    }

    /// How each move of `plan` goes for the user's `picks` (a move without a pick goes to its default); `Err` with the
    /// ticket whose pick isn't one of its choices. Reads only: the checks that can refuse (a check entered needs the
    /// work committed) run here, before anything is written or stopped.
    pub(crate) async fn apply_now_writes(
        &self,
        plan: &ApplyNowPlan,
        picks: &[Pick],
        kept_session: &(dyn Fn(&Ticket, &str) -> bool + Sync),
    ) -> Result<Result<Vec<PlannedWrite>, TicketKey>, BoardError> {
        if let Some(stray) = picks
            .iter()
            .find(|p| !plan.moves.iter().any(|m| m.ticket == p.ticket))
        {
            return Ok(Err(stray.ticket.clone()));
        }
        let waiting = self.waiting_change(&plan.project).await?.ok_or_else(|| {
            BoardError::NotFound(format!("{} has no flow change waiting", plan.project))
        })?;
        let new = yaml::read(&waiting.yaml)
            .map_err(|e| {
                BoardError::NotFound(format!("the waiting flow change doesn't read: {e}"))
            })?
            .flow;
        let old = self.effective_flow(&plan.project).await?;
        let mut writes = Vec::new();
        for planned in &plan.moves {
            let to = picks
                .iter()
                .find(|p| p.ticket == planned.ticket)
                .map_or(&planned.default, |p| &p.to);
            let Some(choice) = planned.choice(to) else {
                return Ok(Err(planned.ticket.clone()));
            };
            let ticket = self.ticket(&planned.ticket.to_string()).await?;
            let write = self
                .move_write(&ticket, &old, &new, planned, choice, kept_session)
                .await?;
            let from_agent_stage = old.stage(&planned.stage).is_some_and(Stage::is_agent_stage);
            writes.push(PlannedWrite {
                ticket,
                from_agent_stage,
                write,
            });
        }
        Ok(Ok(writes))
    }

    async fn move_write(
        &self,
        ticket: &Ticket,
        old: &Flow,
        new: &Flow,
        planned: &PlannedMove,
        choice: &MoveChoice,
        kept_session: &(dyn Fn(&Ticket, &str) -> bool + Sync),
    ) -> Result<MoveWrite, BoardError> {
        let from_name = planned.stage_name.as_str();
        let event = |stage: &str, stage_id: Option<&str>, back: bool, skipped: &[&str]| {
            json!({
                "stage": stage,
                "stage_id": stage_id,
                "from": from_name,
                "back": back,
                "skipped": skipped,
                "run_by_you": false,
                "note": null,
                "removed": planned.stage_name,
            })
        };
        let ended = Some(RunResult::Cancelled);
        let stage = match &choice.to {
            DropOn::Board(DropTarget::Backlog) => {
                return Ok(MoveWrite::Park {
                    has_work: ticket.work.as_ref().is_some_and(|w| w.branch.is_some()),
                    removed: planned.stage_name.clone(),
                });
            }
            DropOn::Board(_) => {
                // Ready for Dev: it walks the flow again from its first stage, in a new session there.
                return Ok(MoveWrite::Drag(DragWrite {
                    moved: Moved {
                        place: Place::Waiting(None),
                        status: Status::Queued,
                        ended,
                        through: Vec::new(),
                        choose: None,
                        start_check: false,
                        automatic: false,
                    },
                    reset: true,
                    marks: Vec::new(),
                    check_head: None,
                    hand_over: None,
                    event: event(Status::Queued.label(), None, true, &[]),
                }));
            }
            DropOn::Stage { stage } => stage,
        };
        let removed_stage = removed_stage(old, new, ticket).ok_or_else(|| {
            BoardError::NotFound(format!("{} isn't in a removed stage", ticket.key))
        })?;
        let place = removed_place(old, new, &planned.stage);
        let (virtual_flow, from) = with_removed(new, new, &removed_stage, place);
        let to = virtual_flow.stage(stage).ok_or_else(|| {
            BoardError::NotFound(format!("{} has no stage {stage}", ticket.key.project))
        })?;
        if choice.way == Way::Back && to.is_agent_stage() {
            let ran = self.stage_ran(&ticket.id, &to.id).await?;
            let live = ran && kept_session(ticket, &to.id);
            return Ok(MoveWrite::SendBack {
                stage: to.id.clone(),
                to: if live {
                    Status::Working
                } else {
                    Status::Queued
                },
                fresh: !ran,
                round: self.current_round(&ticket.id).await?,
                event: event(&to.name, Some(&to.id), true, &[]),
            });
        }
        let skips = self.ticket_skips(ticket, new).await?;
        let place_here = Place::stage_of(&removed_stage.id);
        let at = At {
            skips: &skips,
            ..At::new(&virtual_flow, &place_here, ticket.status)
        };
        // Back onto a check it waits there (a human check) or runs again (an agent check); forward as a drag past
        // the stages between. The removed stage's run ends stopped either way.
        let mut moved = Moved::of(&engine::dragged(at, to));
        moved.ended = ended;
        let skipped: Vec<&Stage> = match choice.way {
            Way::Forward | Way::Next => {
                let at = virtual_flow
                    .position(&to.id)
                    .unwrap_or(virtual_flow.stages.len());
                skipped_by_drag(&virtual_flow, &skips, from, at, false)
            }
            _ => Vec::new(),
        };
        let names: Vec<&str> = skipped.iter().map(|s| s.name.as_str()).collect();
        let check_head = match to.is_agent_check() {
            true => Some(self.submit_head(ticket).await?),
            false => None,
        };
        Ok(MoveWrite::Drag(DragWrite {
            marks: skipped
                .iter()
                .map(|s| (s.id.clone(), MovedPast::Skipped))
                .collect(),
            moved,
            reset: choice.way == Way::Back,
            check_head,
            hand_over: None,
            event: event(&to.name, Some(&to.id), choice.way == Way::Back, &names),
        }))
    }

    /// Apply now's one transaction (APN-04): the waiting change becomes the next flow version and every move is
    /// written, or nothing is. `false` (and nothing written) when the change or a moved ticket changed since `writes`
    /// were decided.
    pub(crate) async fn write_apply_now(
        &self,
        project: &str,
        yaml: &str,
        writes: &[PlannedWrite],
        actor: &Actor,
    ) -> Result<Option<FlowVersion>, BoardError> {
        let waiting = self.waiting_change(project).await?;
        let Some(waiting) = waiting.filter(|w| w.yaml == yaml) else {
            return Ok(None);
        };
        let project = self.project(project).await?;
        let mut tx = self.store().begin_write().await?;
        let taken = sqlx::query("DELETE FROM flow_waiting WHERE project_id = ? AND yaml = ?")
            .bind(&project.id)
            .bind(yaml)
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
                yaml: waiting.yaml.clone(),
                applied_by: waiting.applied_by,
                agent_name: waiting.agent_name.clone(),
                preset: waiting.preset.clone(),
            },
        )
        .await?;
        for planned in writes {
            let PlannedWrite { ticket, write, .. } = planned;
            // Only the ticket as it was read: a move meanwhile (its agent finished) makes the plan stale.
            let now: Option<(Option<String>, String)> =
                sqlx::query_as("SELECT stage_id, state FROM tickets WHERE id = ?")
                    .bind(&ticket.id)
                    .fetch_optional(&mut *tx)
                    .await?;
            if now
                != Some((
                    ticket.place.stage_id().map(str::to_owned),
                    ticket.status.as_str().to_owned(),
                ))
            {
                return Ok(None);
            }
            // The removed stage's run ends "stopped · flow change", a check's round with it.
            end_open(&mut tx, &ticket.id, RunResult::Cancelled, None).await?;
            sqlx::query(
                "UPDATE validations SET ended_at = ?, outcome = ? WHERE ticket_id = ? AND ended_at IS NULL",
            )
            .bind(now_ms())
            .bind(crate::domain::ValidationOutcome::Stopped.as_str())
            .bind(&ticket.id)
            .execute(&mut *tx)
            .await?;
            write_move(&mut tx, ticket, write, actor).await?;
            if planned.stops_agent() {
                // The run the move opened doesn't belong to the agent that stops once this is written.
                sqlx::query(
                    "UPDATE stage_runs SET session_id = NULL, agent = NULL, model = NULL
                     WHERE ticket_id = ?1 AND ended_at IS NULL AND session_id IN
                         (SELECT id FROM sessions WHERE ticket_id = ?1 AND kind = 'agent' AND ended_at IS NULL)",
                )
                .bind(&ticket.id)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        tracing::info!(project = %project.key, moves = writes.len(), "flow change applied now");
        Ok(Some(self.current_flow(&project.key).await?))
    }
}

async fn write_move(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ticket: &Ticket,
    write: &MoveWrite,
    actor: &Actor,
) -> Result<(), BoardError> {
    match write {
        MoveWrite::Drag(drag) => record_drag_in(tx, ticket, drag.clone(), actor).await,
        MoveWrite::Park { has_work, removed } => {
            end_open(tx, &ticket.id, RunResult::Cancelled, None).await?;
            let to = match has_work {
                true => Status::Parked,
                false => Status::Backlog,
            };
            let now = now_ms();
            sqlx::query(concat!(
                "UPDATE tickets SET stage_id = NULL, state = ?, parked = ?, interrupted_from = NULL,
                                    start_problem_step = NULL, start_problem_message = NULL, signal_text = NULL,
                                    signal_since = NULL, wakeup_at = NULL, updated_by = ?, updated_at = ?, ",
                set_state_since!(),
                " WHERE id = ?"
            ))
            .bind(to.as_str())
            .bind(has_work)
            .bind(actor.to_string())
            .bind(now)
            .bind(to.as_str())
            .bind(now)
            .bind(&ticket.id)
            .execute(&mut **tx)
            .await?;
            record_event(
                tx,
                &ticket.id,
                EventKind::Parked,
                actor,
                json!({
                    "from": ticket.status,
                    "to": to,
                    "stage_id": ticket.place.stage_id(),
                    "removed": removed,
                }),
            )
            .await
        }
        MoveWrite::SendBack {
            stage,
            to,
            fresh,
            round,
            event,
        } => {
            send_back_in(
                tx,
                ticket,
                SentBackTo {
                    hand_over: None,
                    to: *to,
                    stage: Some(stage),
                    resume_skill: match (to, fresh) {
                        (Status::Queued, false) => Some(crate::review::ADDRESS_REVIEW_SKILL),
                        _ => None,
                    },
                    ended: RunResult::Cancelled,
                    event: Some((EventKind::Dragged, event.clone())),
                    tell: None,
                    // It never ran that stage (it skips it): a new conversation there (FLW-18's "run by you").
                    fresh: *fresh,
                },
                *round,
                actor,
            )
            .await?;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_summary_names_each_removed_stage() {
        let names = |n: &[&str]| n.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            removes_summary(&names(&["Grill", "Security"]), "shop-web"),
            "Removes Grill and Security from shop-web\u{2019}s flow"
        );
        assert_eq!(
            removes_summary(&names(&["Grill", "Validate", "Security"]), "shop-web"),
            "Removes Grill, Validate and Security from shop-web\u{2019}s flow"
        );
        assert_eq!(
            removes_summary(&names(&["QA"]), "pocket-notes"),
            "Removes QA from pocket-notes\u{2019}s flow"
        );
    }
}
