//! An epic in flight (EPC-11, EPC-13; d0080): its queue mode, its place on the board, and the child groups
//! the queue and take-back rules read.
//!
//! The epic moves as one thing: a **card** in Backlog before it starts (its children are rows inside it), a
//! **track** once a child is queued (in flight: its children are cards of their own, and with Lanes: Epic the epic
//! is a lane, EPL-01), and a **delivery card** in Ready for Review, Approved or Done when every child is done
//! (EPC-04). [`epic_place`] decides which, from the epic and its children only, so the view models can call it
//! without the database. The queue mode is stored on the epic: `Whole` lets new children join the queue, `Single`
//! keeps them "not queued" (EPC-13).
//!
//! **On stages** (STG-11, d0108 rule 7): the groups below read the child's place and its status's set (the kind of
//! the stage it is in), not a board column. An epic skips the Agent and Check stages: it waits in Backlog for its
//! children, then stops at its project flow's last human check, or at the implicit **Review epic** stop
//! ([`REVIEW_EPIC`]) when the flow has none, and is delivered as the flow's Delivery says ([`epic_flow`]). It waits
//! there quietly ([`epic_is_quiet`]). A child's Delivery always Local-merges into the epic's branch, never a PR
//! ([`delivery_of`]).
//!
//! **Queueing** (EPC-12, EPC-14; d0080): [`Board::queue_epic`] (the epic card or track dropped on Ready for Dev or
//! Dev, ⇧⌘D) queues every child still in Backlog in one transaction; a parked one resumes its conversation through
//! the same Parked → Dev path as a dragged ticket (d0052). [`Board::queue_epic_child`] (one row of the card, or a
//! "not queued" chip) queues only that child. Nothing starts here: the scheduler then applies the dependencies
//! (d0049), the optional limit (d0016) and the start order (d0065), and the user's drag is the go-ahead (d0066).
//!
//! **Taking back** (EPC-14, EPC-15; d0080): [`take_back_plan`] says what dropping the track on Backlog would do,
//! with the words the drop hint and the Park-all confirmation show. Unqueueing is
//! [`Board::unqueue_epic_children`]; Park all is [`crate::review::Review::take_back_epic`], which parks each running
//! child with the existing Park (d0052). [`Board::undo_epic_queue`] is the queue toast's Undo: it takes back only
//! what is still waiting and never parks.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Sqlite, Transaction};

use super::moves::{DropTarget, MoveAction, MoveCheck, check_move_in, queue_in, unqueue_in};
use super::tickets::{EventKind, Ticket, record_event};
use super::{Board, BoardError, Refusal};
use crate::domain::{Actor, Place, Status, StatusSet, TicketKey, stored_enum};
use crate::flow::{CheckBy, Delivers, Delivery, Flow, Stage, StageKind};
use crate::store::now_ms;
use crate::store::tickets::TicketRow;

stored_enum! {
    /// How the user queued an epic (`tickets.epic_queue_mode`, migration 0030).
    #[derive(Default)]
    pub enum EpicQueueMode ("epic queue mode") {
        /// Never queued, or taken back.
        #[default]
        None = "none",
        /// The whole epic was queued (its card or track dropped on Ready for Dev / Dev, ⇧⌘D, the menu): new
        /// children join the queue on their own.
        Whole = "whole",
        /// One child row was queued: new children wait as "not queued".
        Single = "single",
    }
}

impl EpicQueueMode {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
}

/// Where an epic shows on the board (EPC-11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpicPlace {
    /// A card in Backlog with its children as rows: nothing is active, and either nothing ever started or a child
    /// is Parked (after Park all, or a single Park).
    Card,
    /// In flight: its children are on the board as cards (with Lanes: Epic, in its lane, EPL-01), until
    /// the epic reaches Ready for Review.
    Track,
    /// The epic itself is in Ready for Review, Approved or Done (EPC-04): its delivery card.
    Delivery,
}

/// Has this ticket been to Dev (any stage of its flow)? Pure: from its place, its status and its work (branch or
/// worktree). A queued ticket that never reached its first stage has **not** (unlike [`Ticket::has_started`], which
/// a queue sets): it is still "never started", a row or a "not queued" chip that queueing or taking back moves
/// freely (d0080). One waiting before a later Agent stage ran an earlier one. A Closed ticket counts if it was ever
/// queued and kept that mark, or has work.
pub fn been_to_dev(ticket: &Ticket) -> bool {
    let worked = ticket
        .work
        .as_ref()
        .is_some_and(|w| w.branch.is_some() || w.worktree.is_some());
    match &ticket.place {
        Place::InStage(_) | Place::Waiting(Some(_)) => true,
        Place::Backlog => ticket.status == Status::Parked || worked,
        Place::Waiting(None) => worked,
        Place::Done => ticket.status == Status::Merged || worked || ticket.has_started(),
    }
}

/// A never-started child: it has never been to Dev and is a row of the epic card, a card in Ready for Dev, or a
/// "not queued" chip on its track.
pub fn is_never_started(ticket: &Ticket) -> bool {
    (ticket.place.is_backlog() || ticket.place.is_waiting()) && !been_to_dev(ticket)
}

/// An active child: waiting before an Agent stage (Ready for Dev), in an Agent stage or an agent check (Dev), or in
/// a human check (Ready for Review).
pub fn is_active(ticket: &Ticket) -> bool {
    ticket.place.is_waiting()
        || ticket.status.at_work()
        || ticket.status.set() == StatusSet::HumanCheck
}

/// A running child: in an Agent stage or an agent check (Dev), where its agent session (or checker) works or waits
/// for the user.
pub fn is_running(ticket: &Ticket) -> bool {
    ticket.status.at_work()
}

/// A parked child: "Parked · has work" in Backlog (d0052).
pub fn is_parked(ticket: &Ticket) -> bool {
    ticket.status == Status::Parked
}

/// The never-started children, in the given order.
pub fn never_started(children: &[Ticket]) -> Vec<&Ticket> {
    children.iter().filter(|c| is_never_started(c)).collect()
}

/// The active children (Ready for Dev, Dev, Ready for Review).
pub fn active(children: &[Ticket]) -> Vec<&Ticket> {
    children.iter().filter(|c| is_active(c)).collect()
}

/// The running children (Dev).
pub fn running(children: &[Ticket]) -> Vec<&Ticket> {
    children.iter().filter(|c| is_running(c)).collect()
}

/// The parked children.
pub fn parked(children: &[Ticket]) -> Vec<&Ticket> {
    children.iter().filter(|c| is_parked(c)).collect()
}

/// The epic's children among a project's tickets, in the given order.
pub fn children_of<'a>(epic: &Ticket, tickets: &'a [Ticket]) -> Vec<&'a Ticket> {
    tickets
        .iter()
        .filter(|t| t.epic.as_ref() == Some(&epic.key))
        .collect()
}

/// Where the epic shows (EPC-11). `tickets` may be just its children or the whole project: only the epic's own
/// children count.
///
/// SHOP-45 with nothing started is a card. SHOP-45.1 merged and SHOP-45.2 taken back is a track ("SHOP-45.1 has
/// started, so the track stays"). After Park all it is a card again, its rows "Parked · has work". A child reopened
/// after Review makes it a track again (the epic is back in Backlog, EPC-05).
pub fn epic_place(epic: &Ticket, tickets: &[Ticket]) -> EpicPlace {
    // Past Backlog an epic is only ever at its stop (a human check), in its Delivery or Done (d0108 rule 7).
    if matches!(
        epic.status.set(),
        StatusSet::HumanCheck | StatusSet::Delivery | StatusSet::Done
    ) {
        return EpicPlace::Delivery;
    }
    let children = children_of(epic, tickets);
    let any_active = children.iter().any(|c| is_active(c));
    let any_started = children.iter().any(|c| been_to_dev(c));
    let any_parked = children.iter().any(|c| is_parked(c));
    if !any_active && (!any_started || any_parked) {
        EpicPlace::Card
    } else {
        EpicPlace::Track
    }
}

/// The id of the implicit **Review epic** stop (d0108 rule 7): the human check an epic waits at when its flow has
/// none of its own. Only epics use it; ids Otaq makes from stage names never start with `_`.
pub const REVIEW_EPIC: &str = "_review-epic";

/// "Review epic", as the epic's track shows the stop.
pub const REVIEW_EPIC_NAME: &str = "Review epic";

/// Where an epic stops for the user when every child is done (d0108 rule 7): the flow's last human check, or the
/// implicit Review epic stop when the flow has none. An epic never skips it (its `skip` option doesn't apply).
pub fn epic_stop(flow: &Flow) -> Stage {
    let mut stop = flow
        .stages
        .iter()
        .rev()
        .find(|s| s.is_human_check())
        .cloned()
        .unwrap_or_else(|| Stage {
            id: REVIEW_EPIC.to_owned(),
            name: REVIEW_EPIC_NAME.to_owned(),
            kind: StageKind::Check { by: CheckBy::You },
            // Its own column (`column: own`).
            column: Default::default(),
            agent: None,
            agents: Vec::new(),
            skill: None,
            options: Vec::new(),
            rounds: None,
            fail_to: None,
            delivery: None,
            why: None,
        });
    stop.options.clear();
    stop
}

/// The flow an epic walks (d0108 rule 7): it skips the Agent and Check stages, so it is its stop ([`epic_stop`])
/// and then the project flow's Delivery. With no Agent stage before the stop, a Fail has nowhere to go: "No Fail on
/// an epic". On the default flow it is Review → Done, as today.
pub fn epic_flow(flow: &Flow) -> Flow {
    Flow {
        stages: std::iter::once(epic_stop(flow))
            .chain(flow.stages.iter().filter(|s| s.is_delivery()).cloned())
            .collect(),
    }
}

/// How a ticket's Delivery delivers (d0108 rule 7): as its flow's Delivery stage says, but an epic child always
/// Local-merges into its epic's branch (never a PR), in the stage's mode. `None` when the flow has no Delivery.
pub fn delivery_of(flow: &Flow, ticket: &Ticket) -> Option<Delivery> {
    let delivery = flow.stages.iter().find_map(|s| s.delivery)?;
    Some(match ticket.epic {
        Some(_) => Delivery {
            delivers: Delivers::Local,
            ..delivery
        },
        None => delivery,
    })
}

/// An epic waiting at its stop for the user's review counts as quiet (d0108 second gap review rule 11): a flow change
/// doesn't wait for it, as for any card waiting in a human check.
pub fn epic_is_quiet(epic: &Ticket, flow: &Flow) -> bool {
    epic.is_epic()
        && epic.status.set() == StatusSet::HumanCheck
        && epic.place.is_in(&epic_stop(flow).id)
}

/// What a queue did, for the queue toast (EPC-15): "Moved 2 children of SHOP-45 to Ready for Dev · Undo", and with Auto-merge Off
/// "Moved 3 tickets to Ready for Dev. Auto-merge is off for shop-web, so each one waits for your review." Undo takes back
/// `queued` and `resumed` (T-0264).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpicQueueOutcome {
    /// The epic, e.g. SHOP-45.
    pub epic: TicketKey,
    /// Never-started (or reopened) children that went to Ready for Dev, in key order.
    pub queued: Vec<TicketKey>,
    /// Parked children that go back to Dev with their conversations (d0052), in key order.
    pub resumed: Vec<TicketKey>,
    /// The project's name, e.g. shop-web.
    pub project: String,
    /// Queueing the whole epic started its lead (LEAD-02): "· its lead started". Undo doesn't stop it.
    #[serde(default)]
    pub lead_started: bool,
}

impl EpicQueueOutcome {
    /// Every child the queue moved: queued, then resumed.
    pub fn keys(&self) -> impl Iterator<Item = &TicketKey> {
        self.queued.iter().chain(&self.resumed)
    }
}

/// The refusal when an epic has nothing left to queue or resume.
pub fn nothing_to_queue(epic: &TicketKey) -> String {
    format!("Every child of {epic} is already queued or started")
}

/// The children a whole-epic queue moves: every child still in Backlog (never started, parked, or reopened). Active
/// and done children stay where they are.
pub fn to_queue<'a>(epic: &Ticket, tickets: &'a [Ticket]) -> Vec<&'a Ticket> {
    children_of(epic, tickets)
        .into_iter()
        .filter(|c| c.place.is_backlog())
        .collect()
}

/// What dropping an epic's track on Backlog would do (EPC-14), with the words the screen shows (T-0271).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum TakeBackPlan {
    /// Children waiting in Ready for Dev go back to Backlog (d0066: nothing of theirs had started); running ones
    /// keep going.
    Unqueue {
        epic: TicketKey,
        /// The waiting children, in key order.
        unqueue: Vec<TicketKey>,
        /// Of those, the ones that never started (the rest worked before: a parked one resumed, a reopened one).
        never_started: Vec<TicketKey>,
        /// The running children (Dev), in key order.
        keep_running: Vec<TicketKey>,
    },
    /// Nothing waits and some children run: after one confirmation each running child is parked (d0052).
    ParkAll {
        epic: TicketKey,
        /// The running children (Dev), in key order.
        park: Vec<TicketKey>,
    },
    /// Nothing waits and nothing runs.
    Refused { epic: TicketKey, reason: String },
}

/// The Park-all confirmation (state 11), built like the Park confirmation (d0052).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParkAllConfirmation {
    /// "Park 2 running tickets?"
    pub title: String,
    /// "Their work is kept. SHOP-45.1 and SHOP-45.2 stop and go back to Backlog with their conversations."
    pub body: String,
    /// "Cancel", the default.
    pub cancel: String,
    /// "Park 2 tickets"
    pub confirm: String,
}

impl TakeBackPlan {
    pub fn epic(&self) -> &TicketKey {
        match self {
            Self::Unqueue { epic, .. }
            | Self::ParkAll { epic, .. }
            | Self::Refused { epic, .. } => epic,
        }
    }

    /// The drop hint: "Unqueue 1 never-started child (45.2)", "Park 2 running children", or the refusal ("Nothing
    /// of SHOP-45 is queued or running").
    pub fn hint(&self) -> String {
        match self {
            Self::Unqueue {
                unqueue,
                never_started,
                ..
            } => {
                let n = unqueue.len();
                let what = if never_started.len() == n {
                    "never-started"
                } else {
                    "queued"
                };
                let short: Vec<String> = unqueue.iter().map(short_key).collect();
                format!(
                    "Unqueue {n} {what} {} ({})",
                    plural(n, "child", "children"),
                    short.join(", ")
                )
            }
            Self::ParkAll { park, .. } => {
                let n = park.len();
                format!("Park {n} running {}", plural(n, "child", "children"))
            }
            Self::Refused { reason, .. } => reason.clone(),
        }
    }

    /// The hint's second line when children keep running: "SHOP-45.1 keeps running".
    pub fn note(&self) -> Option<String> {
        match self {
            Self::Unqueue { keep_running, .. } if !keep_running.is_empty() => Some(format!(
                "{} {}",
                and_list(keep_running),
                plural(keep_running.len(), "keeps running", "keep running")
            )),
            _ => None,
        }
    }

    /// The confirmation Park all asks first.
    pub fn confirmation(&self) -> Option<ParkAllConfirmation> {
        let Self::ParkAll { park, .. } = self else {
            return None;
        };
        let n = park.len();
        let body = if n == 1 {
            format!(
                "Its work is kept. {} stops and goes back to Backlog with its conversation.",
                and_list(park)
            )
        } else {
            format!(
                "Their work is kept. {} stop and go back to Backlog with their conversations.",
                and_list(park)
            )
        };
        Some(ParkAllConfirmation {
            title: format!("Park {n} running {}?", plural(n, "ticket", "tickets")),
            body,
            cancel: "Cancel".to_owned(),
            confirm: format!("Park {n} {}", plural(n, "ticket", "tickets")),
        })
    }
}

/// The refusal when an epic has nothing to take back.
pub fn nothing_to_take_back(epic: &TicketKey) -> String {
    format!("Nothing of {epic} is queued or running")
}

/// What dropping the epic's track on Backlog would do (EPC-14). Pure, from the epic and its project's tickets.
/// Children waiting in Ready for Dev are unqueued and running ones keep going; when nothing waits, the running
/// ones are parked after a confirmation; when nothing waits or runs, it is refused.
pub fn take_back_plan(epic: &Ticket, tickets: &[Ticket]) -> TakeBackPlan {
    take_back_plan_starting(epic, tickets, &HashSet::new())
}

/// [`take_back_plan`] knowing which tickets' start is running now (`starting`, ticket ids): such a child, still in
/// Ready for Dev, isn't unqueued but keeps going as a running one does (d0080). It is never parked mid-start, so
/// with nothing else to take back the drop is refused.
pub fn take_back_plan_starting(
    epic: &Ticket,
    tickets: &[Ticket],
    starting: &HashSet<String>,
) -> TakeBackPlan {
    let mut children = children_of(epic, tickets);
    children.sort_by(|a, b| a.key.cmp(&b.key));
    let keys = |f: &dyn Fn(&Ticket) -> bool| -> Vec<TicketKey> {
        children
            .iter()
            .filter(|c| f(c))
            .map(|c| c.key.clone())
            .collect()
    };
    let is_starting = |c: &Ticket| c.place.is_waiting() && starting.contains(&c.id);
    let waiting = |c: &Ticket| c.place.is_waiting() && !starting.contains(&c.id);
    let unqueue = keys(&waiting);
    let running = keys(&is_running);
    let now_starting = keys(&is_starting);
    let epic = epic.key.clone();
    if !unqueue.is_empty() {
        let mut keep_running = [running, now_starting].concat();
        keep_running.sort();
        TakeBackPlan::Unqueue {
            epic,
            never_started: keys(&|c| waiting(c) && is_never_started(c)),
            unqueue,
            keep_running,
        }
    } else if !running.is_empty() {
        TakeBackPlan::ParkAll {
            epic,
            park: running,
        }
    } else if !now_starting.is_empty() {
        TakeBackPlan::Refused {
            reason: format!(
                "{} {} starting; take {} back once {} running",
                and_list(&now_starting),
                plural(now_starting.len(), "is", "are"),
                plural(now_starting.len(), "it", "them"),
                plural(now_starting.len(), "it's", "they're"),
            ),
            epic,
        }
    } else {
        TakeBackPlan::Refused {
            reason: nothing_to_take_back(&epic),
            epic,
        }
    }
}

/// What the queue toast's Undo did (EPC-15): `taken_back` went back to Backlog; `already_started` had reached Dev
/// (or further), so the toast says "SHOP-45.1 already started".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UndoQueueOutcome {
    pub epic: TicketKey,
    /// In key order.
    pub taken_back: Vec<TicketKey>,
    /// In key order.
    pub already_started: Vec<TicketKey>,
}

/// "45.2" for SHOP-45.2.
fn short_key(key: &TicketKey) -> String {
    match key.child {
        Some(child) => format!("{}.{child}", key.number),
        None => key.number.to_string(),
    }
}

/// "SHOP-45.1", "SHOP-45.1 and SHOP-45.2", "SHOP-45.1, SHOP-45.2 and SHOP-45.3".
fn and_list(keys: &[TicketKey]) -> String {
    let keys: Vec<String> = keys.iter().map(ToString::to_string).collect();
    match keys.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        Some((last, _)) => last.clone(),
        None => String::new(),
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

impl Board {
    /// **Queue an epic** (EPC-12): the epic card or track dropped on Ready for Dev or Dev, or ⇧⌘D on it. Every
    /// child in Backlog goes to Ready for Dev in one transaction, as the user's drag ("Queued with the epic"); a
    /// parked one resumes (d0052). The queue mode becomes Whole. Refused for an epic in Ready for Review, Approved
    /// or Done (its delivery card), and when nothing is left to queue.
    pub async fn queue_epic(
        &self,
        epic: &str,
        actor: &Actor,
    ) -> Result<EpicQueueOutcome, BoardError> {
        let epic = self.ticket(epic).await?;
        if !epic.is_epic() {
            return Err(BoardError::refused(
                Refusal::NotAnEpic,
                format!("{} isn't an epic", epic.key),
            ));
        }
        let flow = self.effective_flow(epic.key.project.as_str()).await?;
        match check_move_in(&flow, &epic, DropTarget::ReadyForDev) {
            MoveCheck::Allowed {
                action: MoveAction::QueueEpic,
            } => {}
            MoveCheck::Refused { reason } => {
                return Err(BoardError::refused(Refusal::MoveRefused, reason));
            }
            _ => {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    nothing_to_queue(&epic.key),
                ));
            }
        }
        let tickets = self.tickets(&epic.key.project.to_string()).await?;
        let children = to_queue(&epic, &tickets);
        let mut outcome = self
            .queue_children(&epic, &children, EpicQueueMode::Whole, actor)
            .await?;
        // An epic with no lead at all gets one (LEAD-02): the drag is the go-ahead for its tokens too.
        outcome.lead_started = self.leads().epic_queued(&epic.key).await;
        Ok(outcome)
    }

    /// **Queue one child** (EPC-12): a row of the epic card, or a "not queued" chip on its track, dropped on Ready
    /// for Dev or Dev. Only that child moves (a parked one resumes). The queue mode None becomes Single; Whole and
    /// Single stay. The outcome has one key.
    pub async fn queue_epic_child(
        &self,
        child: &str,
        actor: &Actor,
    ) -> Result<EpicQueueOutcome, BoardError> {
        let child = self.ticket(child).await?;
        let Some(epic_key) = &child.epic else {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                format!("{} isn't an epic child", child.key),
            ));
        };
        let flow = self.effective_flow(child.key.project.as_str()).await?;
        match check_move_in(&flow, &child, DropTarget::ReadyForDev) {
            MoveCheck::Allowed {
                action: MoveAction::Start,
            } => {}
            MoveCheck::Refused { reason } => {
                return Err(BoardError::refused(Refusal::MoveRefused, reason));
            }
            _ => {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    format!("{} is already queued or started", child.key),
                ));
            }
        }
        let epic = self.ticket(&epic_key.to_string()).await?;
        let mode = match epic.epic_queue_mode {
            EpicQueueMode::None => EpicQueueMode::Single,
            kept => kept,
        };
        self.queue_children(&epic, &[&child], mode, actor).await
    }

    /// Moves `children` to Ready for Dev and sets the epic's queue mode, in one transaction.
    async fn queue_children(
        &self,
        epic: &Ticket,
        children: &[&Ticket],
        mode: EpicQueueMode,
        actor: &Actor,
    ) -> Result<EpicQueueOutcome, BoardError> {
        if children.is_empty() {
            return Err(BoardError::refused(
                Refusal::MoveRefused,
                nothing_to_queue(&epic.key),
            ));
        }
        let project_key = epic.key.project.to_string();
        let project = self.project(&project_key).await?;
        let with_epic = json!({"with_epic": epic.key});
        let mut returns_to = Vec::with_capacity(children.len());
        for child in children {
            returns_to.push(self.parked_return(child).await?);
        }
        let mut tx = self.store.begin_write().await?;
        for (child, returns_to) in children.iter().zip(&returns_to) {
            queue_in(
                &mut tx,
                child,
                Some(&epic.id),
                returns_to.as_deref(),
                actor,
                with_epic.clone(),
            )
            .await?;
        }
        if epic.epic_queue_mode != mode {
            set_queue_mode_in(&mut tx, &epic.id, epic.epic_queue_mode, mode, actor).await?;
        }
        tx.commit().await?;
        let keys = |parked: bool| {
            children
                .iter()
                .filter(|c| is_parked(c) == parked)
                .map(|c| c.key.clone())
                .collect()
        };
        let outcome = EpicQueueOutcome {
            epic: epic.key.clone(),
            queued: keys(false),
            resumed: keys(true),
            project: project.name,
            lead_started: false,
        };
        tracing::info!(
            epic = %epic.key, mode = %mode, queued = outcome.queued.len(), resumed = outcome.resumed.len(),
            "epic queued"
        );
        Ok(outcome)
    }

    /// **Unqueue** the epic's waiting children (EPC-14): each goes back to Backlog (a resumed parked one to
    /// Parked) and loses its "joined the queue" mark, and the epic's queue mode becomes None, in one transaction.
    /// Running children keep going. Returns the epic.
    pub async fn unqueue_epic_children(
        &self,
        epic: &str,
        children: &[TicketKey],
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let epic = self.epic_ticket(epic).await?;
        let mut waiting = Vec::new();
        for key in children {
            let child = self.ticket(&key.to_string()).await?;
            if child.epic.as_ref() != Some(&epic.key) {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    format!("{} isn't a child of {}", child.key, epic.key),
                ));
            }
            if child.place.is_waiting() {
                let row = self.waiting_row(&child).await?;
                waiting.push((child, row));
            }
        }
        let with_epic = json!({"with_epic": epic.key});
        let mut tx = self.store.begin_write().await?;
        for (child, row) in &waiting {
            unqueue_in(&mut tx, child, row, actor, with_epic.clone()).await?;
        }
        if !epic.epic_queue_mode.is_none() {
            set_queue_mode_in(
                &mut tx,
                &epic.id,
                epic.epic_queue_mode,
                EpicQueueMode::None,
                actor,
            )
            .await?;
        }
        tx.commit().await?;
        tracing::info!(epic = %epic.key, unqueued = waiting.len(), "epic taken back");
        self.ticket_by_id(&epic.id).await
    }

    /// **Undo** a queue toast (EPC-15): takes back the outcome's `keys` that still wait in Ready for Dev, never
    /// parks. The ones that reached Dev (or further) are `already_started`; one taken back meanwhile is skipped.
    /// The queue mode becomes None when nothing had started; otherwise it stays.
    pub async fn undo_epic_queue(
        &self,
        epic: &str,
        keys: &[String],
        actor: &Actor,
    ) -> Result<UndoQueueOutcome, BoardError> {
        let epic = self.epic_ticket(epic).await?;
        let mut waiting = Vec::new();
        let mut already_started = Vec::new();
        for key in keys {
            let child = self.ticket(key).await?;
            if child.epic.as_ref() != Some(&epic.key) {
                return Err(BoardError::refused(
                    Refusal::MoveRefused,
                    format!("{} isn't a child of {}", child.key, epic.key),
                ));
            }
            match &child.place {
                Place::Waiting(_) => {
                    let row = self.waiting_row(&child).await?;
                    waiting.push((child, row));
                }
                Place::Backlog => {}
                _ => already_started.push(child.key),
            }
        }
        let with_epic = json!({"with_epic": epic.key});
        let mut tx = self.store.begin_write().await?;
        for (child, row) in &waiting {
            unqueue_in(&mut tx, child, row, actor, with_epic.clone()).await?;
        }
        if already_started.is_empty() && !epic.epic_queue_mode.is_none() {
            set_queue_mode_in(
                &mut tx,
                &epic.id,
                epic.epic_queue_mode,
                EpicQueueMode::None,
                actor,
            )
            .await?;
        }
        tx.commit().await?;
        let mut taken_back: Vec<TicketKey> = waiting.into_iter().map(|(c, _)| c.key).collect();
        taken_back.sort();
        already_started.sort();
        tracing::info!(
            epic = %epic.key, taken_back = taken_back.len(), already_started = already_started.len(),
            "epic queue undone"
        );
        Ok(UndoQueueOutcome {
            epic: epic.key,
            taken_back,
            already_started,
        })
    }

    /// The stored row of a waiting child, which the unqueue write reads.
    async fn waiting_row(&self, child: &Ticket) -> Result<TicketRow, BoardError> {
        self.store
            .ticket(&child.id)
            .await?
            .ok_or_else(|| BoardError::NotFound(child.key.to_string()))
    }

    /// The epic by key; refused when the ticket isn't one.
    async fn epic_ticket(&self, key: &str) -> Result<Ticket, BoardError> {
        let epic = self.ticket(key).await?;
        if !epic.is_epic() {
            return Err(BoardError::refused(
                Refusal::NotAnEpic,
                format!("{} isn't an epic", epic.key),
            ));
        }
        Ok(epic)
    }

    /// Sets an epic's queue mode and logs it in its History ("Queued the whole epic", "Taken back"). Setting the
    /// mode it already has changes nothing. Queueing and taking back (T-0263, T-0264) call it, or
    /// [`set_queue_mode_in`] inside their own transaction.
    pub async fn set_queue_mode(
        &self,
        epic: &str,
        mode: EpicQueueMode,
        actor: &Actor,
    ) -> Result<Ticket, BoardError> {
        let epic = self.ticket(epic).await?;
        if !epic.is_epic() {
            return Err(BoardError::refused(
                Refusal::NotAnEpic,
                format!("{} isn't an epic", epic.key),
            ));
        }
        if epic.epic_queue_mode == mode {
            return Ok(epic);
        }
        let mut tx = self.store.begin_write().await?;
        set_queue_mode_in(&mut tx, &epic.id, epic.epic_queue_mode, mode, actor).await?;
        tx.commit().await?;
        tracing::info!(epic = %epic.key, from = %epic.epic_queue_mode, to = %mode, "epic queue mode");
        self.ticket_by_id(&epic.id).await
    }
}

/// Writes an epic's queue mode and its History line inside a transaction. `from` is the mode it had (for the log).
pub(crate) async fn set_queue_mode_in(
    tx: &mut Transaction<'_, Sqlite>,
    epic_id: &str,
    from: EpicQueueMode,
    to: EpicQueueMode,
    actor: &Actor,
) -> Result<(), BoardError> {
    let now = now_ms();
    sqlx::query(
        "UPDATE tickets SET epic_queue_mode = ?, epic_queue_mode_at = ?, updated_by = ?, updated_at = ?
         WHERE id = ?",
    )
    .bind(to.as_str())
    .bind(now)
    .bind(actor.to_string())
    .bind(now)
    .bind(epic_id)
    .execute(&mut **tx)
    .await?;
    record_event(
        tx,
        epic_id,
        EventKind::EpicQueueModeSet,
        actor,
        json!({"from": from, "to": to}),
    )
    .await
}
