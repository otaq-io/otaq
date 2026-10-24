//! The scheduler (SCH-01, SCH-02, STG-07; d0007, d0016, d0049, d0065, d0066, d0108): a loop in the service that
//! starts the tickets **waiting before an Agent stage** (Queued) once every dependency is merged (beads-style
//! readiness, [`crate::board::readiness`]), in the **start order**: pinned → priority 1→4 → ready the longest
//! ([`crate::board::order::start_order`]). It keeps each queued ticket's `ready_since` up to date. The wait is the
//! same before every Agent stage (d0108): Ready for Dev before the first, and between two Agent stages before the
//! next (a ticket that finished Grill waits before Dev, keeping its worktree); the start enters the stage it waited
//! before ([`crate::start::sequence`]).
//!
//! It runs a pass on every state change (the service's event bus) plus a slow tick ([`DEFAULT_TICK`]) that
//! catches anything missed. A pass is [`plan`]: pure, from all projects' tickets. The scheduler:
//! - **never queues a ticket by itself**: only the user's drag puts a ticket in Queued (SCH-02). It only looks at
//!   tickets already in Queued's own sub-states and moves them among those (Queued ↔ Parent closed);
//! - never touches **Start interrupted** or **Held** (diverged base) tickets: only the user's Retry start
//!   puts them back to Queued (d0018, d0059);
//! - starts each ticket once: passes never overlap, tickets whose start is running are skipped, and the start
//!   sequence claims a ticket before it reads its status.
//!
//! **Optional limits** (SCH-03; d0016, d0042), off by default, global across projects (d0024): with
//! `max_concurrency` on, a pass starts at most `max − running` tickets; **every ticket with a live agent session
//! holds a slot, whatever its stage** (d0108 second gap review rule 8: [`HOLDS_A_SLOT`], starts that are running,
//! and PR Delivery sessions, [`Scheduler::busy`]), and waiting cards don't; with back-pressure
//! reached (that many needs-you items), it starts none. Ready tickets that don't start show "Waiting for a slot".
//! Limits only hold back **starts**: nothing running is stopped or paused, and an agent the user unblocks
//! always continues. Settings changes wake the scheduler, so they apply at once.
//!
//! **Per-tool holds** (SCH-04 `[M23]`, USG-10; d0086): the scheduler holds nothing itself. Each start runs the check
//! before every launch ([`crate::agents::gate`]), so only a ticket whose agent would use a tool at its limit or
//! not available (or a model no longer listed) waits as **Held**, and tickets on other tools keep starting. Held
//! tickets are skipped; the gate lifts their holds when the cause goes, never on a tick.
//!
//! **A waiting flow change** (FLW-06, FED-08; d0108, d0115): each pass first applies the waiting changes nothing holds
//! any more ([`crate::board::Board::settle_flow_changes`]); until one applies or is cancelled, no ticket of its
//! project enters what it waits for ([`plan_holding`]): a swap starts nothing from Ready for Dev, a removal or reorder
//! starts nothing into its span.
//!
//! **A paused project** (PAU-04, d0112) starts nothing at all: neither from Ready for Dev nor between two Agent stages
//! ([`plan_holding`]'s `paused`). Its queued tickets wait as they are, and the project's Resume wakes the scheduler.
//!
//! **The epic lead's turn** (LEAD-04; d0124): while an epic's lead is live, a child that a sibling's merge (or close)
//! made ready waits, "Waiting for the lead's update", until the lead's turn about that sibling ends or the lead ends
//! ([`waiting_for_lead`], from [`crate::epic_lead::wake::LeadWait`]). Both publish a lead change, which runs a pass
//! while a ticket waits for a lead.
//! With no live lead (never opened, ended, "Ended by restart") nothing waits for it. A pause still wins.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::broadcast::error::{RecvError, TryRecvError};
use tokio::sync::{Mutex, Notify, watch};

use crate::board::needs_you::{ProjectTickets, needs_you_count, needs_you_items_with};
use crate::board::order::{ready_since, start_order};
use crate::board::readiness::{ReadyState, readiness};
use crate::board::settings::AgentLimits;
use crate::board::tickets::{Base, EventKind, Ticket, record_event};
use crate::board::{Board, BoardError};
use crate::domain::{Actor, StartStep, Status, TicketKey};
use crate::epic_lead::wake::LeadWait;
use crate::flow::apply::Hold;
use crate::git::BaseSync;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::session::Start;
use crate::start::Starter;
use crate::store::now_ms;
use crate::store::tickets::set_state_since;

/// The slow tick: a pass at least this often, whatever happened.
pub const DEFAULT_TICK: Duration = Duration::from_secs(30);

/// A Queued sub-state the scheduler moves a ticket into.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusChange {
    pub id: String,
    pub key: TicketKey,
    pub from: Status,
    pub to: Status,
}

/// A ticket a pass starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToStart {
    pub id: String,
    pub key: TicketKey,
}

/// What one pass does.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// Sub-state bookkeeping, applied first.
    pub changes: Vec<StatusChange>,
    /// Tickets to start, in the start order.
    pub start: Vec<ToStart>,
    /// When queued tickets became ready (`None`: waiting again), for those whose stored time is out of date.
    pub ready_since: Vec<(String, Option<i64>)>,
}

/// The waiting statuses the scheduler looks at, before any Agent stage (STG-07). Held (a diverged base, or the check
/// before the launch) and Start interrupted are left alone: the user's Retry, or the gate's lift, puts them back.
const SCHEDULED: &[Status] = &[Status::Queued, Status::WaitingForSlot, Status::ParentClosed];

/// Whether the check before the launch held this ticket because its agent's tool is at its limit (SCH-04 `[M23]`):
/// it starts once the limit is resumed.
pub fn held_by_tool_limit(ticket: &Ticket) -> bool {
    ticket.status == Status::Held
        && ticket
            .start_problem
            .as_ref()
            .is_some_and(|p| p.step == StartStep::ToolLimit)
}

fn scheduled(ticket: &Ticket) -> bool {
    SCHEDULED.contains(&ticket.status)
}

/// The statuses that hold a slot of the optional agent limit (d0016, d0042; d0108 rule 8: a live agent session,
/// whatever the stage): any Agent stage's Working **including** the needs-you family, Working in background and
/// Waiting for limit reset; any agent check's (one slot per ticket, not per session); a `restack` fix (Merge
/// conflict · fixing, rule 12) and a Delivery's stop. Waiting cards (before any Agent stage), human checks,
/// Delivery waits and Parked tickets don't, and neither do Interrupted or Resume failed ones (no agent runs).
/// Tickets whose start is running, and PR Delivery sessions, hold one too ([`running_count`]).
pub const HOLDS_A_SLOT: &[Status] = &[
    Status::Working,
    Status::WorkingInBackground,
    Status::AsksYou,
    Status::NeedsYou,
    Status::Idle,
    Status::WaitingForLimitReset,
    // One slot per ticket, not per session: the validator doesn't take a second one (d0063).
    Status::Checking,
    Status::CheckerNeedsYou,
    Status::CheckFailed,
    Status::CheckNoResult,
    Status::MergeConflictFixing,
    Status::DeliveryStopped,
];

/// What may hold new starts back (SCH-03): the optional limits and the number of needs-you items
/// ([`crate::board::needs_you::needs_you_count`]). The default lets everything start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Gate {
    pub limits: AgentLimits,
    pub needs_you: usize,
}

impl Gate {
    /// How many tickets may start now; `None` when the limits are off. Limits only control **starting**:
    /// nothing running is ever stopped, so this is never below zero. A stored 0 (flagged in Settings) acts as 1.
    pub fn free_slots(&self, running: usize) -> Option<usize> {
        if !self.limits.enabled {
            return None;
        }
        let back_pressure = self.limits.back_pressure.max(1) as usize;
        if self.needs_you >= back_pressure {
            return Some(0);
        }
        Some((self.limits.max_concurrency.max(1) as usize).saturating_sub(running))
    }
}

/// How many builds a repository's stock keeps where builds are renamed, not cloned (M19-a, plan §4.2): the tickets
/// of `repo_tickets` in Ready for Dev whose dependencies are done and that wait only for a slot
/// ([`Status::WaitingForSlot`]) or their tool's limit ([`held_by_tool_limit`]); at least 1. `all_tickets` holds their
/// projects' tickets, for readiness. A ticket waiting for a dependency, a closed parent or a diverged base isn't
/// counted: it can't start soon, and neither is one waiting before a later Agent stage: it keeps its worktree.
pub fn stock_limit<'a>(
    repo_tickets: impl IntoIterator<Item = &'a Ticket>,
    all_tickets: &[Ticket],
) -> usize {
    repo_tickets
        .into_iter()
        .filter(|t| t.place.is_ready_for_dev())
        .filter(|t| t.status == Status::WaitingForSlot || held_by_tool_limit(t))
        .filter(|t| readiness(t, all_tickets).is_ready())
        .count()
        .max(1)
}

/// How many agents hold a slot: tickets in [`HOLDS_A_SLOT`] plus `busy`, the ids of tickets with a live agent
/// session their status doesn't show: their start is running, or a PR Delivery session works on them
/// ([`Scheduler::busy`]). One slot per ticket.
pub fn running_count(tickets: &[Ticket], busy: &HashSet<String>) -> usize {
    tickets
        .iter()
        .filter(|t| HOLDS_A_SLOT.contains(&t.status) || busy.contains(&t.id))
        .count()
}

/// The scheduler's view of the limits now: what the header's "3 / 5 agents" and Settings' "Right now: 3 running ·
/// 2 queued tickets waiting for a slot · needs you 7 (new starts paused)" show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SchedulerStatus {
    pub limits: AgentLimits,
    /// Agents holding a slot ([`HOLDS_A_SLOT`] and starts that are running), across all projects.
    pub holding: u32,
    /// Queued tickets waiting for a slot.
    pub waiting_for_slot: u32,
    /// Needs-you items across all projects (what back-pressure counts).
    pub needs_you: u32,
    /// Back-pressure is reached: no new starts until items are cleared.
    pub starts_paused: bool,
}

/// Decides one pass from every project's tickets. `starting` holds the ids of tickets whose start runs now, and of
/// those a PR Delivery session works on ([`running_count`]'s `busy`): neither is started.
/// Ready tickets beyond the free slots wait as "waiting for a slot", oldest first when slots free up.
pub fn plan(tickets: &[Ticket], starting: &HashSet<String>, gate: &Gate) -> Plan {
    plan_holding(tickets, starting, gate, &HashMap::new(), &HashSet::new())
}

/// [`plan`], with the projects whose flow change waits, by key, and the starts each holds back (FLW-06, FED-08):
/// their tickets in Ready for Dev (the whole board's wait, or a span holding the first Agent stage) and those waiting
/// at the span's edge don't start ("Waiting for the flow change"); other tickets, and other projects, start as usual.
/// The `paused` projects' tickets don't start anywhere (PAU-04): they wait as they are.
pub fn plan_holding(
    tickets: &[Ticket],
    starting: &HashSet<String>,
    gate: &Gate,
    held: &HashMap<String, Hold>,
    paused: &HashSet<String>,
) -> Plan {
    plan_with(
        tickets,
        starting,
        gate,
        &Holds {
            flow: held,
            paused,
            lead: &HashSet::new(),
        },
    )
}

/// What holds starts back besides the limits ([`plan_with`]).
#[derive(Debug, Clone, Copy)]
pub struct Holds<'a> {
    /// The projects whose flow change waits, by key, and the starts each holds back (FLW-06).
    pub flow: &'a HashMap<String, Hold>,
    /// The paused projects, by key (PAU-04).
    pub paused: &'a HashSet<String>,
    /// The tickets that wait for their epic lead's update, by id ([`waiting_for_lead`], d0124): they wait in Queued.
    pub lead: &'a HashSet<String>,
}

/// [`plan_holding`], with the tickets that wait for their epic lead's update too (d0124). A pause wins over all.
pub fn plan_with(
    tickets: &[Ticket],
    starting: &HashSet<String>,
    gate: &Gate,
    holds: &Holds<'_>,
) -> Plan {
    let Holds {
        flow: held,
        paused,
        lead,
    } = *holds;
    let mut plan = Plan::default();
    let mut ready: Vec<&Ticket> = Vec::new();
    for ticket in tickets {
        if !scheduled(ticket) || starting.contains(&ticket.id) {
            continue;
        }
        let state = readiness(ticket, tickets);
        let ready_at = ready_since(ticket, tickets);
        if ticket.ready_since.is_some() != ready_at.is_some() {
            plan.ready_since.push((ticket.id.clone(), ready_at));
        }
        match state {
            ReadyState::ParentClosed { .. } => change(&mut plan, ticket, Status::ParentClosed),
            ReadyState::Waiting { .. } => change(&mut plan, ticket, Status::Queued),
            ReadyState::Ready if paused.contains(ticket.key.project.as_str()) => {}
            ReadyState::Ready
                if held
                    .get(ticket.key.project.as_str())
                    .is_some_and(|h| h.holds(&ticket.place)) => {}
            ReadyState::Ready if lead.contains(&ticket.id) => {
                change(&mut plan, ticket, Status::Queued)
            }
            ReadyState::Ready => ready.push(ticket),
        }
    }
    // Pinned → priority 1→4 → ready the longest (d0065).
    ready.sort_by_cached_key(|t| start_order(t, tickets));
    let slots = gate
        .free_slots(running_count(tickets, starting))
        .unwrap_or(usize::MAX);
    for (i, ticket) in ready.into_iter().enumerate() {
        if i < slots {
            // A ticket whose closed parent was unlinked starts from Queued.
            if ticket.status == Status::ParentClosed {
                change(&mut plan, ticket, Status::Queued);
            }
            plan.start.push(ToStart {
                id: ticket.id.clone(),
                key: ticket.key.clone(),
            });
        } else {
            change(&mut plan, ticket, Status::WaitingForSlot);
        }
    }
    plan
}

/// The tickets that wait for their epic lead's update (d0124): waiting (Queued's sub-states) children of an epic in
/// `waits` (a live lead) that a sibling held back which finished (merged or closed) since the lead's session started
/// and whose turn hasn't ended, or that the finished sibling held back when it finished (`freed`, so a link the lead
/// removes in that turn doesn't start them early). Whether they are ready is the plan's to say.
pub fn waiting_for_lead(tickets: &[Ticket], waits: &[LeadWait]) -> HashSet<String> {
    let by_key: HashMap<&TicketKey, &Ticket> = tickets.iter().map(|t| (&t.key, t)).collect();
    let mut waiting = HashSet::new();
    for wait in waits {
        let unreleased = |sibling: &Ticket| {
            sibling.epic.as_ref() == Some(&wait.epic)
                && matches!(sibling.status, Status::Merged | Status::ClosedNotMerged)
                && sibling.status_since.is_some_and(|at| {
                    at >= wait.since && wait.released.get(&sibling.id).is_none_or(|r| at > *r)
                })
        };
        for ticket in tickets
            .iter()
            .filter(|t| t.epic.as_ref() == Some(&wait.epic) && scheduled(t))
        {
            let held = wait.freed.contains(&ticket.id)
                || ticket
                    .depends_on
                    .iter()
                    .filter_map(|k| by_key.get(k))
                    .any(|s| unreleased(s));
            if held {
                waiting.insert(ticket.id.clone());
            }
        }
    }
    waiting
}

/// The ids of `child`'s waiting siblings that depend on it (d0124): what its merge or close frees, or would.
pub async fn held_back_by(board: &Board, child: &Ticket) -> Result<Vec<String>, BoardError> {
    let Some(epic) = &child.epic else {
        return Ok(Vec::new());
    };
    Ok(board
        .tickets(child.key.project.as_str())
        .await?
        .into_iter()
        .filter(|t| {
            t.epic.as_ref() == Some(epic) && t.depends_on.contains(&child.key) && scheduled(t)
        })
        .map(|t| t.id)
        .collect())
}

fn change(plan: &mut Plan, ticket: &Ticket, to: Status) {
    if ticket.status != to {
        plan.changes.push(StatusChange {
            id: ticket.id.clone(),
            key: ticket.key.clone(),
            from: ticket.status,
            to,
        });
    }
}

/// Starts tickets for the scheduler. The service uses the start sequence ([`Starter`]); tests inject their own.
pub trait StartTickets: Send + Sync + 'static {
    /// Starts the ticket in the background and returns at once.
    fn spawn_start(&self, ticket: &ToStart);
    /// The ids of tickets whose start is running.
    fn starting(&self) -> HashSet<String>;
}

impl StartTickets for Starter {
    fn spawn_start(&self, ticket: &ToStart) {
        let starter = self.clone();
        let key = ticket.key.to_string();
        tokio::spawn(async move {
            // The user's drag queued it; the scheduler only picks the moment (SCH-02). It is automatic all the same: a
            // paused project's tickets wait (PAU-04).
            if let Err(e) = starter
                .start(&key, &Actor::current_user(), Start::Auto)
                .await
            {
                tracing::error!(ticket = %key, error = %e, "start failed");
            }
        });
    }

    fn starting(&self) -> HashSet<String> {
        Starter::starting(self)
    }
}

/// The scheduler of one service. Cheap to clone.
#[derive(Clone)]
pub struct Scheduler {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    starts: Arc<dyn StartTickets>,
    events: EventBus,
    tick: Duration,
    /// One pass at a time.
    pass: Mutex<()>,
    wake: Notify,
}

impl Scheduler {
    pub fn new(
        board: Board,
        starts: Arc<dyn StartTickets>,
        events: EventBus,
        tick: Duration,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                board,
                starts,
                events,
                tick,
                pass: Mutex::new(()),
                wake: Notify::new(),
            }),
        }
    }

    /// Asks the loop for a pass soon (for changes that publish no ticket event, such as settings).
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Runs one pass now: applies the sub-state changes and starts the ready tickets, oldest first.
    pub async fn pass(&self) -> Result<Plan, BoardError> {
        let _one_at_a_time = self.inner.pass.lock().await;
        self.pass_locked().await
    }

    /// **A parent was merged** (SCH-05; d0049): the user's Local merge (M6) or Mark merged (M10) calls this after
    /// setting the parent Done · Merged. Before its waiting children start, the base they start from is brought
    /// up to date: the parent's base branch is fetched (and fast-forwarded when only behind); an epic's branch is
    /// local and isn't fetched. Then a pass starts the children that became ready, from the merged code.
    ///
    /// If the base diverged from its remote, or the fetch fails on a flow that delivers by PR, those children are
    /// **held** in Queued with the message; only the user's Retry start runs them again (nothing retries silently,
    /// d0018). A failed fetch on any other flow starts them from the local base, with the warning (d0121).
    pub async fn on_parent_merged(&self, parent: &str) -> Result<Plan, BoardError> {
        let _one_at_a_time = self.inner.pass.lock().await;
        let board = &self.inner.board;
        let parent = board.ticket(parent).await?;
        let tickets = board.tickets(parent.key.project.as_str()).await?;
        let starting = self.inner.starts.starting();
        let children: Vec<&Ticket> = tickets
            .iter()
            .filter(|t| {
                t.depends_on.contains(&parent.key)
                    && scheduled(t)
                    && !starting.contains(&t.id)
                    && readiness(t, &tickets).is_ready()
            })
            .collect();
        if let (false, Base::Branch(branch)) = (children.is_empty(), &parent.base) {
            let project = board.project(parent.key.project.as_str()).await?;
            let synced = board
                .project_git(&project.key)
                .await?
                .sync_base(Path::new(&project.repo_path), branch)
                .await;
            let problem = match synced {
                Ok(BaseSync::Diverged { message }) => Some(message),
                // d0121: unless the flow delivers by PR, they start from the local base; each start records the
                // warning.
                Ok(BaseSync::FetchFailed(failure))
                    if !board.goes_on_without_remote(&parent).await? =>
                {
                    Some(format!("Fetching `{branch}` failed: {}", failure.reason()))
                }
                Ok(_) => None,
                Err(e) => Some(format!("Fetching `{branch}` failed: {e}")),
            };
            if let Some(message) = problem {
                for child in children {
                    self.hold(child, &message).await?;
                }
                tracing::warn!(parent = %parent.key, %message, "children held after the parent merged");
            }
        }
        self.pass_locked().await
    }

    /// Holds a ticket in Queued with the base's problem (step "sync base"), as the start sequence does.
    async fn hold(&self, ticket: &Ticket, message: &str) -> Result<(), BoardError> {
        let now = now_ms();
        let held = Status::Held;
        let mut tx = self.inner.board.store().begin_write().await?;
        sqlx::query(concat!(
            "UPDATE tickets SET state = ?, start_problem_step = ?, start_problem_message = ?,
                                updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ?"
        ))
        .bind(held.as_str())
        .bind(StartStep::SyncBase.as_str())
        .bind(message)
        .bind(now)
        .bind(held.as_str())
        .bind(now)
        .bind(&ticket.id)
        .execute(&mut *tx)
        .await?;
        record_event(
            &mut tx,
            &ticket.id,
            EventKind::Held,
            &Actor::current_user(),
            json!({ "step": StartStep::SyncBase, "message": message }),
        )
        .await?;
        tx.commit().await?;
        self.publish(&ticket.id, &ticket.key, held);
        Ok(())
    }

    /// The limits and how full they are now.
    pub async fn status(&self) -> Result<SchedulerStatus, BoardError> {
        let board = &self.inner.board;
        let (tickets, needs_you) = self.load(true).await?;
        let limits = board.global_settings().await?.agent_limits;
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Ok(SchedulerStatus {
            limits,
            holding: count(running_count(&tickets, &self.busy().await?)),
            waiting_for_slot: count(
                tickets
                    .iter()
                    .filter(|t| t.status == Status::WaitingForSlot)
                    .count(),
            ),
            needs_you: count(needs_you),
            starts_paused: limits.enabled && needs_you >= limits.back_pressure.max(1) as usize,
        })
    }

    /// The tickets with a live agent session their status doesn't show (d0108 rule 8): those whose start is running,
    /// and those a **PR Delivery session** works on, from the Open PR or Push click until the PR is reported or the
    /// flow cancelled (`pr_flows`; the card waits in Review meanwhile, or an epic's delivery session runs).
    pub async fn busy(&self) -> Result<HashSet<String>, BoardError> {
        let mut busy = self.inner.starts.starting();
        let delivering: Vec<(String,)> = sqlx::query_as("SELECT ticket_id FROM pr_flows")
            .fetch_all(self.inner.board.store().pool())
            .await?;
        busy.extend(delivering.into_iter().map(|(id,)| id));
        Ok(busy)
    }

    /// Every project's tickets, and the number of needs-you items when `count_needs_you`.
    async fn load(&self, count_needs_you: bool) -> Result<(Vec<Ticket>, usize), BoardError> {
        let board = &self.inner.board;
        let projects = board.projects().await?;
        let mut loaded = Vec::with_capacity(projects.len());
        for project in &projects {
            loaded.push(board.tickets(&project.key).await?);
        }
        let needs_you = if count_needs_you {
            let mut flows = Vec::with_capacity(projects.len());
            for project in &projects {
                flows.push(board.effective_flow(&project.key).await?);
            }
            let views: Vec<ProjectTickets<'_>> = projects
                .iter()
                .zip(&loaded)
                .zip(&flows)
                .map(|((p, tickets), flow)| ProjectTickets {
                    key: &p.key,
                    name: &p.name,
                    tickets,
                    flow,
                })
                .collect();
            needs_you_count(&needs_you_items_with(
                &views,
                now_ms(),
                &board.limit_stops().snapshot(),
            ))
        } else {
            0
        };
        Ok((loaded.into_iter().flatten().collect(), needs_you))
    }

    async fn pass_locked(&self) -> Result<Plan, BoardError> {
        // A waiting flow change applies once its board is quiet; until then its project's starts are held (FLW-06).
        let settled = self.inner.board.settle_flow_changes().await?;
        for version in &settled.applied {
            self.inner.events.publish(Event::FlowChanged {
                project: version.project.clone(),
                change: crate::flow::apply::FlowEvent::Applied {
                    version: version.version,
                },
            });
        }
        let limits = self.inner.board.global_settings().await?.agent_limits;
        let (tickets, needs_you) = self.load(limits.enabled).await?;
        // A paused project starts nothing (PAU-04).
        let paused: HashSet<String> =
            crate::agents::pause::paused_projects(self.inner.board.store())
                .await?
                .into_iter()
                .map(|(key, _)| key)
                .collect();
        // Children a sibling's merge freed wait for their lead's turn about it (d0124).
        let lead = waiting_for_lead(&tickets, &self.inner.board.leads().lead_waits().await);
        let plan = plan_with(
            &tickets,
            &self.busy().await?,
            &Gate { limits, needs_you },
            &Holds {
                flow: &settled.held,
                paused: &paused,
                lead: &lead,
            },
        );
        // The cards say so; a paused project's say it is paused.
        let shown = tickets
            .iter()
            .filter(|t| lead.contains(&t.id) && !paused.contains(t.key.project.as_str()))
            .map(|t| t.id.clone())
            .collect();
        for id in self.inner.board.leads().set_waiting(shown) {
            if let Some(t) = tickets.iter().find(|t| t.id == id) {
                self.inner.events.publish(Event::TicketChanged {
                    project: t.key.project.to_string(),
                    id: t.id.clone(),
                    key: t.key.clone(),
                });
            }
        }
        for change in &plan.changes {
            if let Err(e) = self.apply(change).await {
                tracing::warn!(ticket = %change.key, error = %e, "can't change the queued state");
            }
        }
        for (id, at) in &plan.ready_since {
            let set = sqlx::query("UPDATE tickets SET ready_since = ? WHERE id = ?")
                .bind(at)
                .bind(id)
                .execute(self.inner.board.store().pool())
                .await;
            if let Err(e) = set {
                tracing::warn!(ticket = %id, error = %e, "can't keep the ready time");
            }
        }
        for ticket in &plan.start {
            tracing::info!(ticket = %ticket.key, "scheduler starts it");
            self.inner.starts.spawn_start(ticket);
        }
        Ok(plan)
    }

    /// Runs passes on every state change and every tick until `stop` is set.
    pub async fn run(self, mut stop: watch::Receiver<bool>) {
        let mut listener = self.inner.events.listen();
        let events = &mut listener.receiver;
        let mut tick = tokio::time::interval(self.inner.tick);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = stop.wait_for(|s| *s) => return,
                _ = tick.tick() => {}
                _ = self.inner.wake.notified() => {}
                event = events.recv() => match event {
                    Ok(event) if !self.triggers_pass(&event) => continue,
                    Ok(_) | Err(RecvError::Lagged(_)) => {
                        // One pass covers everything that arrived meanwhile.
                        while matches!(events.try_recv(), Ok(_) | Err(TryRecvError::Lagged(_))) {}
                    }
                    Err(RecvError::Closed) => return,
                },
            }
            if let Err(e) = self.pass().await {
                tracing::warn!(error = %e, "scheduler pass failed");
            }
            self.refresh_overlaps().await;
        }
    }

    /// Whether `event` calls for a pass ([`triggers_pass`]); an epic lead's change (its turn ended, it ended) does only
    /// while a ticket waits for a lead's update (d0124).
    fn triggers_pass(&self, event: &Event) -> bool {
        triggers_pass(event)
            || matches!(event, Event::LeadChanged { .. }) && self.inner.board.leads().any_waiting()
    }

    /// Children changing the same file (LEAD-09): their files are read again after each pass (cheap when nothing
    /// changed), and the cards whose "same file" flag changed are refreshed. It wakes no one.
    async fn refresh_overlaps(&self) {
        match crate::epic_lead::overlap::refresh(&self.inner.board).await {
            Ok(changed) => {
                for (id, key) in changed {
                    self.inner.events.publish(Event::TicketChanged {
                        project: key.project.to_string(),
                        id,
                        key,
                    });
                }
            }
            Err(e) => tracing::warn!(error = %e, "can't read the children's changed files"),
        }
    }

    /// Moves a ticket between Queued's sub-states, only if it is still where the plan found it.
    async fn apply(&self, change: &StatusChange) -> Result<(), BoardError> {
        let now = now_ms();
        let mut tx = self.inner.board.store().begin_write().await?;
        let updated = sqlx::query(concat!(
            "UPDATE tickets SET state = ?, updated_at = ?, ",
            set_state_since!(),
            " WHERE id = ? AND state = ?"
        ))
        .bind(change.to.as_str())
        .bind(now)
        .bind(change.to.as_str())
        .bind(now)
        .bind(&change.id)
        .bind(change.from.as_str())
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if updated == 0 {
            return Ok(());
        }
        // Parent closed is worth a History line; waiting for a slot comes and goes.
        if change.from == Status::ParentClosed || change.to == Status::ParentClosed {
            record_event(
                &mut tx,
                &change.id,
                EventKind::Moved,
                &Actor::current_user(),
                json!({ "from": change.from, "to": change.to }),
            )
            .await?;
        }
        tx.commit().await?;
        self.publish(&change.id, &change.key, change.to);
        Ok(())
    }

    fn publish(&self, id: &str, key: &TicketKey, status: Status) {
        let project = key.project.to_string();
        self.inner.events.publish(Event::TicketChanged {
            project: project.clone(),
            id: id.to_owned(),
            key: key.clone(),
        });
        self.inner.events.publish(Event::CardStatusChanged {
            project,
            id: id.to_owned(),
            key: key.clone(),
            status,
        });
    }
}

/// Events that can change what a pass decides: any ticket change (a status, a link) or a project going.
fn triggers_pass(event: &Event) -> bool {
    matches!(
        event,
        Event::TicketChanged { .. }
            | Event::CardStatusChanged { .. }
            | Event::TicketDeleted { .. }
            | Event::ProjectsChanged
    )
}
