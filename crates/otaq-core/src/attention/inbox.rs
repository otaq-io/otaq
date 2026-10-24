//! The global "Needs you" inbox in the service (ATT-06…ATT-10; d0016, d0024, d0045, d0048).
//!
//! **Listing**: the items are [`crate::board::needs_you`]'s, across all projects, in inbox order, with counts in
//! total and per project (the header and the project switcher, ATT-10). They are derived from the tickets every
//! time, so an item answered elsewhere (`otaq attach`, the terminal) simply isn't there at the next
//! derivation (ATT-09). The service publishes `inbox_changed` whenever the derived set changes, and only then.
//!
//! **Answering** an Asks you or Idle item types the answer into the agent's session as the user's click (d0048): it
//! is never held, but unsent text in the input line asks *Keep it and cancel / Clear it and send* first. For an
//! **Interrupted** ticket that was waiting for an answer (d0045) the same click resumes the conversation and
//! delivers the answer in one step: the answer becomes the resumed conversation's first prompt, so it arrives once
//! the CLI is ready. The answer clears the needs-you state (ATT-02) and so the inbox item; an answered question is
//! recorded with its answer.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, broadcast, watch};

use super::focus::Looking;
use super::notify::pace::{Clock, Pace};
use super::notify::{self, ItemId, Kind, Notifier, OsNotification, PermissionState, Planned};
use crate::agent_tools::Tool;
use crate::agent_tools::connected::ConnectedTools;
use crate::board::needs_you::{NeedsYouGroup, NeedsYouItem};
use crate::board::questions::{Choice, format_answer};
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError, Refusal};
use crate::domain::{Actor, Status, TicketKey};
use crate::protocol::Event;
use crate::review::Review;
use crate::service::EventBus;
use crate::session::deliveries::{Deliveries, Delivery};
use crate::session::{SessionKind, Sessions};
use crate::signals::Signals;
use crate::store::now_ms;

/// How long the inbox waits for a burst of changes to settle before it derives the items again.
const SETTLE: Duration = Duration::from_millis(50);

/// The inbox: every needs-you item with the counts (`inbox.list`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxList {
    pub items: Vec<NeedsYouItem>,
    /// Project agents waiting for the user (group 1, PAG-07), labelled with the project and the agent's name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub project_agents: Vec<crate::project_agents::AgentNeedsYou>,
    /// Tickets' and Project agents' items together.
    pub counts: InboxCounts,
}

/// The inbox count in total and per project (ATT-10). Every project is listed, with 0 when nothing waits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxCounts {
    pub total: u32,
    pub projects: Vec<ProjectCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectCount {
    /// The project's key.
    pub project: String,
    pub count: u32,
}

impl InboxCounts {
    /// The counts of `items`, with every project of `projects` (keys) listed.
    pub fn of(items: &[NeedsYouItem], projects: &[String]) -> Self {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        Self {
            total: count(items.len()),
            projects: projects
                .iter()
                .map(|p| ProjectCount {
                    project: p.clone(),
                    count: count(items.iter().filter(|i| &i.project == p).count()),
                })
                .collect(),
        }
    }

    /// Adds the waiting Project agents to the counts (ATT-10: the header and the project switcher count them).
    pub fn add_agents(&mut self, agents: &[crate::project_agents::AgentNeedsYou]) {
        let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        self.total = self.total.saturating_add(count(agents.len()));
        for p in &mut self.projects {
            p.count = p.count.saturating_add(count(
                agents.iter().filter(|a| a.project == p.project).count(),
            ));
        }
    }

    /// One project's count.
    pub fn of_project(&self, project: &str) -> u32 {
        self.projects
            .iter()
            .find(|p| p.project == project)
            .map_or(0, |p| p.count)
    }
}

/// What makes two derivations the same set: everything but the ages, which grow every second.
type Signature = (
    Vec<(String, NeedsYouGroup, Status, String, u32, TicketKey)>,
    Vec<(String, Status, String, String)>,
);

fn signature(list: &InboxList) -> Signature {
    let agents = list
        .project_agents
        .iter()
        .map(|a| (a.agent.clone(), a.status, a.reason.clone(), a.name.clone()))
        .collect();
    let items = list
        .items
        .iter()
        .map(|i| {
            (
                i.id.clone(),
                i.group,
                i.status,
                i.reason.clone(),
                i.blocked,
                i.key.clone(),
            )
        })
        .collect();
    (items, agents)
}

/// The inbox's actions. Cheap to clone.
#[derive(Clone)]
pub struct Inbox {
    inner: Arc<Inner>,
}

struct Inner {
    board: Board,
    sessions: Sessions,
    deliveries: Deliveries,
    signals: Signals,
    review: Review,
    events: EventBus,
    /// The set last published (or found at start).
    last: Mutex<Option<Signature>>,
    /// Something outside the event stream changed the set (the plan-limit switch).
    changed: Notify,
    /// OS notifications for new blocking items (ATT-11), and the items already seen.
    notifier: Arc<dyn Notifier>,
    seen: Mutex<HashSet<ItemId>>,
    /// What a click on a notification opens.
    opener: notify::Opener,
    /// The quiet period after a pop-up and what it holds (d0090), in `clock`'s time.
    pace: Mutex<Pace>,
    clock: Arc<dyn Clock>,
    /// The app windows that are focused: while any is, nothing is posted (d0090).
    looking: Looking,
    /// A notification would have gone out while macOS has Otaq's notifications turned off (d0090's banner trigger).
    blocked: std::sync::atomic::AtomicBool,
    /// Woken when `blocked` becomes set (the banner's state follows, T-0372).
    blocked_wake: std::sync::OnceLock<Arc<Notify>>,
    /// The Project agents that wait for the user (set by the service).
    project_agents: Mutex<Option<crate::project_agents::ProjectAgents>>,
    /// The tools that aren't available while tickets wait on them or moved off them, since when (USG-10):
    /// notified once each.
    unavailable: Mutex<HashMap<Tool, i64>>,
    /// When each tool was last seen available; moves off a tool count from then (USG-10).
    available_at: Mutex<HashMap<Tool, i64>>,
    /// When the inbox started: the floor for a tool never seen available.
    started_at: i64,
    /// The check before every launch (which tools aren't available) and the tools' states (set by the service).
    tools: std::sync::OnceLock<(crate::agents::gate::LaunchGate, ConnectedTools)>,
}

impl Inbox {
    pub fn new(
        board: Board,
        sessions: Sessions,
        deliveries: Deliveries,
        signals: Signals,
        review: Review,
        events: EventBus,
        notifications: notify::Notifications,
    ) -> Self {
        let notify::Notifications {
            notifier,
            opener,
            clock,
        } = notifications;
        Self {
            inner: Arc::new(Inner {
                board,
                sessions,
                deliveries,
                signals,
                review,
                events,
                last: Mutex::new(None),
                changed: Notify::new(),
                notifier,
                seen: Mutex::new(HashSet::new()),
                opener,
                pace: Mutex::new(Pace::default()),
                clock,
                looking: Looking::default(),
                blocked: std::sync::atomic::AtomicBool::new(false),
                blocked_wake: std::sync::OnceLock::new(),
                project_agents: Mutex::new(None),
                unavailable: Mutex::new(HashMap::new()),
                available_at: Mutex::new(HashMap::new()),
                started_at: now_ms(),
                tools: std::sync::OnceLock::new(),
            }),
        }
    }

    /// A client's app window gained or lost focus (`app.focus`, d0090): while any is focused, no OS notification goes
    /// out, and what comes meanwhile never notifies later.
    pub fn set_looking(&self, client: crate::attention::ClientId, focused: bool) {
        self.inner.looking.set(client, focused);
    }

    /// A client disconnected: its window doesn't count as looking any more.
    pub fn client_gone(&self, client: crate::attention::ClientId) {
        self.inner.looking.client_gone(client);
    }

    /// Whether the user is looking at Otaq (any app window focused).
    pub fn looking(&self) -> bool {
        self.inner.looking.any()
    }

    /// Whether a notification was left out because macOS has Otaq's notifications turned off (d0090; read by the
    /// banner).
    pub fn notification_blocked(&self) -> bool {
        self.inner.blocked.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Forgets a blocked notification: macOS allowed notifications again, so only a new one shows the banner (d0090).
    pub fn clear_notification_blocked(&self) {
        self.inner
            .blocked
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    /// What to wake when a notification is first blocked (set before [`Self::run`]).
    pub fn on_notification_blocked(&self, wake: Arc<Notify>) {
        let _ = self.inner.blocked_wake.set(wake);
    }

    /// The notifier's permission state as last read (d0090); never waits for the OS.
    pub fn notification_permission(&self) -> PermissionState {
        self.inner.notifier.permission()
    }

    /// The Project agents whose waits are inbox items (d0068).
    pub fn set_project_agents(&self, agents: crate::project_agents::ProjectAgents) {
        *self.inner.project_agents.lock().expect("project agents") = Some(agents);
    }

    /// Every needs-you item with the counts (`inbox.list`).
    pub async fn list(&self) -> Result<InboxList, BoardError> {
        let board = &self.inner.board;
        let items = board.needs_you_items(now_ms()).await?;
        let projects: Vec<String> = board.projects().await?.into_iter().map(|p| p.key).collect();
        let agents = self
            .inner
            .project_agents
            .lock()
            .expect("project agents")
            .clone();
        let project_agents = match agents {
            Some(agents) => agents.needs_you().await?,
            None => Vec::new(),
        };
        let mut counts = InboxCounts::of(&items, &projects);
        counts.add_agents(&project_agents);
        Ok(InboxList {
            counts,
            items,
            project_agents,
        })
    }

    /// The counts in total and per project (`inbox.counts`).
    pub async fn counts(&self) -> Result<InboxCounts, BoardError> {
        Ok(self.list().await?.counts)
    }

    /// `tool`'s limit switch (ATT-09, per tool from M23; turned on and off with its limit line): while on, the
    /// agents its limit stopped raise no items. Other tools' agents keep theirs.
    pub fn set_suppress_limit_stopped(&self, tool: Tool, on: bool) {
        if self.inner.board.limit_stops().set_suppress(tool, on) {
            self.inner.changed.notify_one();
        }
    }

    /// Marks a ticket's agent as stopped by `tool`'s limit, or (`None`) not any more.
    pub fn mark_limit_stopped(&self, ticket_id: &str, by: Option<Tool>) {
        self.inner.board.limit_stops().mark(ticket_id, by);
        self.inner.changed.notify_one();
    }

    /// Gives the inbox the tools' availability, for a tool that isn't available (its one notification). Set before
    /// [`Self::run`].
    pub fn set_tools(&self, gate: crate::agents::gate::LaunchGate, connected: ConnectedTools) {
        let _ = self.inner.tools.set((gate, connected));
    }

    /// When `tool` was last seen available (or the inbox started): moves off it count from then.
    pub fn available_floor(&self, tool: Tool) -> i64 {
        self.inner
            .available_at
            .lock()
            .expect("available tools")
            .get(&tool)
            .copied()
            .unwrap_or(self.inner.started_at)
    }

    /// Since when tickets wait on `tool` (or moved off it) while it isn't available, as last derived; `None` when
    /// none do.
    pub fn unavailable_since(&self, tool: Tool) -> Option<i64> {
        self.inner
            .unavailable
            .lock()
            .expect("unavailable tools")
            .get(&tool)
            .copied()
    }

    /// Derives the items again, publishes `inbox_changed` if the set differs from the last one and posts OS
    /// notifications for new blocking items. Returns whether the set changed.
    pub async fn refresh(&self) -> Result<bool, BoardError> {
        let list = self.list().await?;
        self.notify(&list.items, &list.project_agents).await?;
        let now = signature(&list);
        let changed = {
            let mut last = self.inner.last.lock().expect("inbox");
            let changed = last.as_ref().is_some_and(|l| *l != now);
            *last = Some(now);
            changed
        };
        if changed {
            self.inner.events.publish(Event::InboxChanged {
                counts: list.counts,
            });
        }
        Ok(changed)
    }

    /// OS notifications for the blocking items not seen before (ATT-11); the settings are read every time, and the
    /// quiet period, the focus of the app's windows and macOS's permission decide what goes out now (d0090).
    async fn notify(
        &self,
        items: &[NeedsYouItem],
        agents: &[crate::project_agents::AgentNeedsYou],
    ) -> Result<(), BoardError> {
        let board = &self.inner.board;
        let choices = notify::Choices::of(&board.global_settings().await?);
        let mut texts = HashMap::new();
        for item in items.iter().filter(|i| notify::should_notify(i)) {
            let ticket = board.ticket_by_id(&item.id).await?;
            let text = if ticket.status == Status::CheckFailed {
                ticket.flags.verdict.clone()
            } else {
                ticket
                    .question
                    .map(|q| q.text)
                    .or_else(|| ticket.signal.and_then(|s| s.text))
            };
            texts.insert(
                item.id.clone(),
                notify::ItemText {
                    text,
                    failed_times: ticket.validation_round,
                },
            );
        }
        // A lead's subtitle names its epic by title ("shop-web · Wishlist").
        let mut epic_titles = HashMap::new();
        for epic in agents.iter().filter_map(|a| a.epic.as_ref()) {
            if let Ok(ticket) = board.ticket(&epic.to_string()).await {
                epic_titles.insert(epic.to_string(), ticket.title);
            }
        }
        let mut fresh = {
            let mut seen = self.inner.seen.lock().expect("notified items");
            let mut out = notify::plan(items, &texts, &mut seen, &choices);
            out.extend(notify::plan_agents(
                agents,
                &epic_titles,
                &mut seen,
                &choices,
            ));
            out
        };
        // A tool that isn't available while tickets wait on it or moved off it: one notification when it first
        // happens (the first hold or the first move), none per ticket, and none again until it clears (USG-10).
        let unavailable = match self.inner.tools.get() {
            Some((gate, _)) => gate.facts().await?.unavailable,
            None => HashMap::new(),
        };
        {
            let now = now_ms();
            let mut seen = self.inner.available_at.lock().expect("available tools");
            for &tool in Tool::ALL {
                if !unavailable.contains_key(&tool) {
                    seen.insert(tool, now);
                }
            }
        }
        let waiting_tools =
            crate::agents::limits::unavailable_situations(board, &unavailable, |tool| {
                self.available_floor(tool)
            })
            .await?;
        let (new_tools, tools_waiting): (Vec<_>, Vec<String>) = {
            let mut known = self.inner.unavailable.lock().expect("unavailable tools");
            known.retain(|tool, _| waiting_tools.iter().any(|w| w.tool == *tool));
            let new: Vec<&crate::agents::limits::UnavailableSituation> = waiting_tools
                .iter()
                .filter(|w| match known.entry(w.tool) {
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(w.since);
                        true
                    }
                    std::collections::hash_map::Entry::Occupied(_) => false,
                })
                .collect();
            let still = known
                .iter()
                .map(|(tool, since)| notify::tool_presence(*tool, *since))
                .collect();
            (new, still)
        };
        if choices.allows(Kind::ToolUnavailable) {
            fresh.extend(new_tools.into_iter().map(|w| {
                let mut notification = notify::tool_unavailable(w.tool, w.tickets, w.on_fallback);
                notification.sound = choices.sound;
                Planned {
                    notification,
                    item: notify::Held {
                        presence: notify::tool_presence(w.tool, w.since),
                        project: None,
                        label: w.tool.display_name().to_owned(),
                        ticket: false,
                    },
                }
            }));
        }
        let waiting: HashSet<String> = items
            .iter()
            .filter(|i| notify::should_notify(i))
            .map(notify::presence_of)
            .chain(agents.iter().map(notify::agent_presence))
            .chain(tools_waiting)
            .collect();
        // Something may go out: read macOS's permission again for the next time (this time uses the last read; the
        // read never waits, T-0372).
        if !fresh.is_empty() {
            self.inner.notifier.refresh_permission();
        }
        let decided = {
            let mut pace = self.inner.pace.lock().expect("quiet period");
            decide(
                &mut pace,
                &Moment {
                    now: self.inner.clock.now(),
                    looking: self.inner.looking.any(),
                    enabled: choices.enabled,
                    permission: self.inner.notifier.permission(),
                    waiting: &waiting,
                },
                fresh,
            )
        };
        if decided.blocked
            && !self
                .inner
                .blocked
                .swap(true, std::sync::atomic::Ordering::SeqCst)
            && let Some(wake) = self.inner.blocked_wake.get()
        {
            wake.notify_one();
        }
        for notification in &decided.posts {
            tracing::info!(title = %notification.title, "notification");
            let (opener, target) = (self.inner.opener.clone(), notification.target.clone());
            self.inner
                .notifier
                .post(notification, Box::new(move || opener.open(target)));
        }
        Ok(())
    }

    /// When the quiet period ends with something held: the inbox derives again then.
    fn wake_at(&self) -> Option<std::time::Instant> {
        self.inner.pace.lock().expect("quiet period").wake_at()
    }

    /// Watches the changes that can change the set (tickets, projects, terminal focus) until the service stops.
    pub async fn run(self, mut stop: watch::Receiver<bool>) {
        let mut listener = self.inner.events.listen();
        let mut tools = self.inner.tools.get().map(|(_, c)| c.subscribe());
        if let Err(e) = self.refresh().await {
            tracing::warn!(error = %e, "can't derive the inbox");
        }
        loop {
            let wake = self.wake_at().map(|at| self.inner.clock.sleep_until(at));
            let relevant = tokio::select! {
                biased;
                _ = stop.wait_for(|s| *s) => return,
                () = self.inner.changed.notified() => true,
                // The quiet period ended with items held: post them now (d0090).
                () = async {
                    match wake {
                        Some(wake) => wake.await,
                        None => std::future::pending().await,
                    }
                } => true,
                // A tool's state changed: a tool that isn't available any more, or again.
                changed = async {
                    match tools.as_mut() {
                        Some(rx) => rx.changed().await,
                        None => std::future::pending().await,
                    }
                } => {
                    if changed.is_err() {
                        tools = None;
                    }
                    true
                }
                event = listener.receiver.recv() => match event {
                    Ok(event) => touches_inbox(&event),
                    Err(broadcast::error::RecvError::Lagged(_)) => true,
                    Err(broadcast::error::RecvError::Closed) => return,
                },
            };
            if !relevant {
                continue;
            }
            // Let the burst settle, then derive once; a stopping service posts nothing more.
            tokio::time::sleep(SETTLE).await;
            while let Ok(event) = listener.receiver.try_recv() {
                let _ = event;
            }
            if *stop.borrow() {
                return;
            }
            if let Err(e) = self.refresh().await {
                tracing::warn!(error = %e, "can't derive the inbox");
            }
        }
    }

    /// **Answer** an Asks you or Idle item (or an Interrupted one that was waiting for an answer). `clear` is the
    /// user's "Clear it and send" after [`Delivery::NeedsConfirmation`].
    pub async fn answer(
        &self,
        key: &str,
        choice: &Choice,
        clear: bool,
        actor: &Actor,
    ) -> Result<Delivery, BoardError> {
        let inner = &self.inner;
        let ticket = inner.board.ticket(key).await?;
        let not_waiting = || {
            BoardError::refused(
                Refusal::MoveRefused,
                format!("{} isn't waiting for an answer", ticket.key),
            )
        };
        let waiting = answerable(&ticket).ok_or_else(not_waiting)?;
        // Only the open question takes an answer (d0119): Asks you without one (answered in the terminal, replaced
        // or closed meanwhile) is refused, and an Idle agent's answer is free text, never shaped by an older question.
        let question = match waiting.label {
            Status::AsksYou => Some(
                inner
                    .board
                    .open_user_question(&ticket.id)
                    .await?
                    .ok_or_else(not_waiting)?,
            ),
            _ => None,
        };
        let text = answer_text(question.as_ref(), choice)?;
        let session = inner.sessions.running_of(&ticket.id, SessionKind::Agent);
        let delivery = match (waiting.how, session) {
            (Waiting::Live, Some(session)) => {
                let typed = if clear {
                    self.inner
                        .deliveries
                        .clear_and_send(&session.id, Some(&text))
                        .await
                } else {
                    self.inner
                        .deliveries
                        .deliver_from_click(&session.id, &text)
                        .await
                };
                typed.map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?
            }
            (Waiting::Live, None) => {
                return Err(BoardError::refused(
                    Refusal::SessionEnded,
                    format!("{}'s session has ended", ticket.key),
                ));
            }
            (Waiting::Interrupted, _) => {
                self.inner
                    .review
                    .resume_with_answer(key, &text, actor)
                    .await?;
                Delivery::Sent { count: 1 }
            }
        };
        if !matches!(delivery, Delivery::Sent { .. }) {
            return Ok(delivery);
        }
        // The question closes first: while it is open it holds Asks you (d0119).
        if let Some(question) = &question {
            inner.board.record_answer(&question.id, &text).await?;
        }
        inner.signals.answered(&ticket.id).await?;
        if question.is_some() {
            // A question that went through its epic's lead is logged on the epic too (LEAD-06): its Messages refresh.
            if let Some(epic) = &ticket.epic
                && let Ok(epic) = inner.board.ticket(&epic.to_string()).await
            {
                inner.events.publish(Event::TicketChanged {
                    project: epic.key.project.to_string(),
                    id: epic.id,
                    key: epic.key,
                });
            }
        }
        tracing::info!(ticket = %ticket.key, ?delivery, "inbox answer");
        Ok(delivery)
    }
}

/// What one derivation knows besides the new items (d0090).
struct Moment<'a> {
    now: std::time::Instant,
    /// Any app window is focused.
    looking: bool,
    /// The main switch.
    enabled: bool,
    permission: PermissionState,
    /// Every item that waits now ([`notify::Held::presence`]): a held item not among them was answered.
    waiting: &'a HashSet<String>,
}

/// What one derivation posts.
#[derive(Debug, Default)]
struct Decided {
    posts: Vec<OsNotification>,
    /// Something would have been posted, but macOS has Otaq's notifications turned off.
    blocked: bool,
}

/// What goes out now (d0090): nothing while the user looks at Otaq or the switch is off (the new items are only
/// remembered, and anything held is let go: those never notify later); nothing while macOS has notifications turned
/// off (that is recorded); otherwise what the quiet period lets through: the end of a period first, then the new
/// items.
fn decide(pace: &mut Pace, moment: &Moment<'_>, fresh: Vec<Planned>) -> Decided {
    if moment.looking || !moment.enabled {
        pace.drop_held();
        return Decided::default();
    }
    if moment.permission == PermissionState::TurnedOff {
        let blocked = !fresh.is_empty() || pace.holds();
        pace.drop_held();
        return Decided {
            posts: Vec::new(),
            blocked,
        };
    }
    let mut posts = pace.due(moment.now, moment.waiting);
    posts.extend(pace.offer(moment.now, fresh));
    Decided {
        posts,
        blocked: false,
    }
}

/// Whether an event can change the needs-you set.
fn touches_inbox(event: &Event) -> bool {
    matches!(
        event,
        Event::TicketChanged { .. }
            | Event::CardStatusChanged { .. }
            | Event::TicketDeleted { .. }
            | Event::ProjectsChanged
            | Event::SessionFocus { .. }
            | Event::ProjectAgentChanged { .. }
    )
}

/// How an item waits for an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Waiting {
    /// Asks you or Idle, with its session.
    Live,
    /// Interrupted while it asked or was idle (d0045).
    Interrupted,
}

/// What an answerable ticket waits with: how, and its label (Asks you or Idle).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Answerable {
    how: Waiting,
    label: Status,
}

/// Whether the ticket takes an answer: Asks you and Idle (a permission prompt is answered in the terminal, and
/// simply asks again after a resume).
fn answerable(ticket: &Ticket) -> Option<Answerable> {
    let takes_answer = |s: Status| matches!(s, Status::AsksYou | Status::Idle);
    if takes_answer(ticket.status) {
        Some(Answerable {
            how: Waiting::Live,
            label: ticket.status,
        })
    } else if ticket.status == Status::Interrupted
        && let Some(label) = ticket.interrupted_from.filter(|s| takes_answer(*s))
    {
        Some(Answerable {
            how: Waiting::Interrupted,
            label,
        })
    } else {
        None
    }
}

/// The text typed for an answer: a question with options gets [`format_answer`]'s wording ("Answer to your
/// question: Postmark (option 1)."); otherwise the free text as written.
pub fn answer_text(
    question: Option<&crate::board::questions::HumanQuestion>,
    choice: &Choice,
) -> Result<String, BoardError> {
    let text = match (question.filter(|q| !q.options.is_empty()), choice) {
        (Some(q), Choice::Option(n)) if *n >= 1 && (*n as usize) <= q.options.len() => {
            format_answer(q, choice)
        }
        (_, Choice::Option(n)) => {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                format!("There is no option {n} to choose"),
            ));
        }
        (Some(q), Choice::Other(_)) => format_answer(q, choice),
        (None, Choice::Other(text)) => text.trim().to_owned(),
    };
    if text.trim().is_empty() || matches!(choice, Choice::Other(t) if t.trim().is_empty()) {
        return Err(BoardError::refused(
            Refusal::EmptyField,
            "Write the answer first",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    //! The notifications' pace in the inbox (T-0371, d0090), table-driven: each case is a script of moments (seconds
    //! on a fake clock) with what changed, and what was posted when. Every moment runs the inbox's own path: the
    //! settings' kinds ([`notify::plan`]), then [`decide`] (looking, the switch, macOS's permission, the quiet
    //! period, the 3-at-once rule).

    use std::time::Duration;

    use super::*;
    use crate::attention::notify::pace::{Clock, FakeClock, QUIET};
    use crate::attention::notify::{Choices, ItemText};

    #[derive(Debug, Clone)]
    enum Change {
        /// New items wait (`KEY:status`, e.g. `SHOP-1:asks`).
        Wait(&'static [&'static str]),
        /// These items were answered.
        Answer(&'static [&'static str]),
        /// The app window gained (true) or lost focus.
        Look(bool),
        /// The kind's checkbox.
        Kind(Kind, bool),
        /// The main switch.
        Switch(bool),
        /// macOS's permission as last read.
        Permission(PermissionState),
        /// Nothing changed: the inbox looks again (the run loop woke at the period's end).
        Tick,
    }

    struct Case {
        name: &'static str,
        script: &'static [(u64, Change)],
        /// (second, title) of everything posted, in order.
        posted: &'static [(u64, &'static str)],
        /// For the summaries: (title, body).
        bodies: &'static [(&'static str, &'static str)],
        blocked: bool,
    }

    fn status(word: &str) -> Status {
        match word {
            "asks" => Status::AsksYou,
            "perm" => Status::NeedsYou,
            "idle" => Status::Idle,
            "int" => Status::Interrupted,
            other => panic!("{other}"),
        }
    }

    fn item(spec: &str, since: i64) -> NeedsYouItem {
        let (key, word) = spec.split_once(':').unwrap();
        let project = key.split('-').next().unwrap();
        NeedsYouItem {
            group: NeedsYouGroup::Family,
            project: project.into(),
            project_name: if project == "BILL" {
                "billing-api".into()
            } else {
                "shop-web".into()
            },
            id: key.into(),
            key: key.parse().unwrap(),
            title: "Add a search box".into(),
            status: status(word),
            interrupted_from: None,
            question: None,
            reason: String::new(),
            since,
            age_ms: 0,
            blocked: 0,
            message: None,
            suggestion: None,
            lead_note: None,
            change_agent: None,
            waiting_in: None,
        }
    }

    /// Runs a case's script; returns (second, notification) for everything posted and whether a post was blocked.
    fn run(case: &Case) -> (Vec<(u64, OsNotification)>, bool) {
        let clock = FakeClock::new();
        let mut pace = Pace::default();
        let mut seen = HashSet::new();
        let mut choices = Choices::default();
        let mut items: Vec<NeedsYouItem> = Vec::new();
        let (mut looking, mut permission, mut blocked) = (false, PermissionState::Allowed, false);
        let mut posted = Vec::new();
        let mut at = 0;
        for (second, change) in case.script {
            clock.advance(Duration::from_secs(second - at));
            at = *second;
            match change {
                Change::Wait(specs) => items.extend(
                    specs
                        .iter()
                        .map(|s| item(s, i64::try_from(*second).unwrap())),
                ),
                Change::Answer(keys) => items.retain(|i| !keys.contains(&i.id.as_str())),
                Change::Look(on) => looking = *on,
                Change::Kind(kind, on) => choices.kinds.set(*kind, *on),
                Change::Switch(on) => choices.enabled = *on,
                Change::Permission(state) => permission = *state,
                Change::Tick => {}
            }
            let texts: HashMap<String, ItemText> = HashMap::new();
            let fresh = notify::plan(&items, &texts, &mut seen, &choices);
            let waiting: HashSet<String> = items.iter().map(notify::presence_of).collect();
            let decided = decide(
                &mut pace,
                &Moment {
                    now: clock.now(),
                    looking,
                    enabled: choices.enabled,
                    permission,
                    waiting: &waiting,
                },
                fresh,
            );
            blocked |= decided.blocked;
            posted.extend(decided.posts.into_iter().map(|n| (*second, n)));
        }
        (posted, blocked)
    }

    const TWO_MIN: u64 = QUIET.as_secs();

    const CASES: &[Case] = &[
        Case {
            name: "the first notification goes out at once",
            script: &[(0, Change::Wait(&["SHOP-1:perm"]))],
            posted: &[(0, "SHOP-1 needs permission")],
            bodies: &[],
            blocked: false,
        },
        Case {
            name: "looking at the app: nothing posted, and nothing later when the user switches away",
            script: &[
                (0, Change::Look(true)),
                (5, Change::Wait(&["SHOP-1:perm", "SHOP-2:asks"])),
                (10, Change::Look(false)),
                (TWO_MIN + 10, Change::Tick),
                (TWO_MIN + 20, Change::Wait(&["SHOP-3:asks"])),
            ],
            posted: &[(TWO_MIN + 20, "SHOP-3 asks a question")],
            bodies: &[],
            blocked: false,
        },
        Case {
            name: "four items in 90 s: one pop-up at 0:00, one summary at 2:00 with three keys per project",
            script: &[
                (0, Change::Wait(&["SHOP-1:perm"])),
                (30, Change::Wait(&["SHOP-2:asks"])),
                (60, Change::Wait(&["BILL-6:perm"])),
                (90, Change::Wait(&["SHOP-3:idle"])),
                (TWO_MIN - 1, Change::Tick),
                (TWO_MIN, Change::Tick),
            ],
            posted: &[
                (0, "SHOP-1 needs permission"),
                (TWO_MIN, "3 more agents need you"),
            ],
            bodies: &[(
                "3 more agents need you",
                "shop-web: SHOP-2, SHOP-3 · billing-api: BILL-6",
            )],
            blocked: false,
        },
        Case {
            name: "one held item goes out as itself at the end",
            script: &[
                (0, Change::Wait(&["SHOP-1:perm"])),
                (45, Change::Wait(&["BILL-6:asks"])),
                (TWO_MIN, Change::Tick),
            ],
            posted: &[
                (0, "SHOP-1 needs permission"),
                (TWO_MIN, "BILL-6 asks a question"),
            ],
            bodies: &[],
            blocked: false,
        },
        Case {
            name: "a held item answered meanwhile is dropped; the summary never names it",
            script: &[
                (0, Change::Wait(&["SHOP-1:perm"])),
                (
                    20,
                    Change::Wait(&["SHOP-2:asks", "SHOP-3:asks", "SHOP-4:idle"]),
                ),
                (50, Change::Answer(&["SHOP-2"])),
                (TWO_MIN, Change::Tick),
            ],
            posted: &[
                (0, "SHOP-1 needs permission"),
                (TWO_MIN, "2 more agents need you"),
            ],
            bodies: &[("2 more agents need you", "shop-web: SHOP-3, SHOP-4")],
            blocked: false,
        },
        Case {
            name: "every held item answered: nothing at the end, and the next one goes out at once",
            script: &[
                (0, Change::Wait(&["SHOP-1:perm"])),
                (20, Change::Wait(&["SHOP-2:asks"])),
                (40, Change::Answer(&["SHOP-2"])),
                (TWO_MIN, Change::Tick),
                (TWO_MIN + 5, Change::Wait(&["SHOP-5:asks"])),
            ],
            posted: &[
                (0, "SHOP-1 needs permission"),
                (TWO_MIN + 5, "SHOP-5 asks a question"),
            ],
            bodies: &[],
            blocked: false,
        },
        Case {
            name: "nothing held: nothing at the end of the period",
            script: &[
                (0, Change::Wait(&["SHOP-1:perm"])),
                (TWO_MIN, Change::Tick),
                (3 * TWO_MIN, Change::Tick),
            ],
            posted: &[(0, "SHOP-1 needs permission")],
            bodies: &[],
            blocked: false,
        },
        Case {
            name: "after a summary a new period starts: the next item waits for its end",
            script: &[
                (0, Change::Wait(&["SHOP-1:perm"])),
                (10, Change::Wait(&["SHOP-2:asks", "SHOP-3:asks"])),
                (TWO_MIN, Change::Tick),
                (TWO_MIN + 30, Change::Wait(&["SHOP-4:asks"])),
                (2 * TWO_MIN - 1, Change::Tick),
                (2 * TWO_MIN, Change::Tick),
            ],
            posted: &[
                (0, "SHOP-1 needs permission"),
                (TWO_MIN, "2 more agents need you"),
                (2 * TWO_MIN, "SHOP-4 asks a question"),
            ],
            bodies: &[],
            blocked: false,
        },
        Case {
            name: "the 3-at-once rule still groups what goes out at once",
            script: &[
                (
                    0,
                    Change::Wait(&[
                        "SHOP-1:int",
                        "SHOP-2:int",
                        "SHOP-3:int",
                        "SHOP-4:int",
                        "BILL-6:perm",
                    ]),
                ),
                (10, Change::Wait(&["SHOP-5:asks"])),
                (TWO_MIN, Change::Tick),
            ],
            posted: &[
                (0, "4 agents need you"),
                (0, "BILL-6 needs permission"),
                (TWO_MIN, "SHOP-5 asks a question"),
            ],
            bodies: &[("4 agents need you", "SHOP-1, SHOP-2, SHOP-3, SHOP-4")],
            blocked: false,
        },
        Case {
            name: "a kind turned off: never, and not later when it is turned on",
            script: &[
                (0, Change::Kind(Kind::TurnEnded, false)),
                (1, Change::Wait(&["SHOP-1:idle"])),
                (5, Change::Kind(Kind::TurnEnded, true)),
                (TWO_MIN, Change::Tick),
                (TWO_MIN + 1, Change::Wait(&["SHOP-2:idle"])),
            ],
            posted: &[(TWO_MIN + 1, "SHOP-2 ended its turn")],
            bodies: &[],
            blocked: false,
        },
        Case {
            name: "the user looks at the app during the period: the held items never notify",
            script: &[
                (0, Change::Wait(&["SHOP-1:perm"])),
                (30, Change::Wait(&["SHOP-2:asks", "SHOP-3:asks"])),
                (60, Change::Look(true)),
                (90, Change::Look(false)),
                (TWO_MIN, Change::Tick),
            ],
            posted: &[(0, "SHOP-1 needs permission")],
            bodies: &[],
            blocked: false,
        },
        Case {
            name: "the main switch off: nothing, and nothing later when it is on again",
            script: &[
                (0, Change::Switch(false)),
                (1, Change::Wait(&["SHOP-1:perm"])),
                (2, Change::Switch(true)),
                (TWO_MIN, Change::Tick),
            ],
            posted: &[],
            bodies: &[],
            blocked: false,
        },
        Case {
            name: "macOS has notifications turned off: nothing posted, and the block is recorded",
            script: &[
                (0, Change::Permission(PermissionState::TurnedOff)),
                (1, Change::Wait(&["SHOP-1:perm"])),
            ],
            posted: &[],
            bodies: &[],
            blocked: true,
        },
        Case {
            name: "turned off with nothing to post: no block recorded",
            script: &[
                (0, Change::Permission(PermissionState::TurnedOff)),
                (1, Change::Tick),
            ],
            posted: &[],
            bodies: &[],
            blocked: false,
        },
    ];

    #[test]
    fn the_pace_of_notifications() {
        for case in CASES {
            let (posted, blocked) = run(case);
            let titles: Vec<(u64, &str)> = posted
                .iter()
                .map(|(at, n)| (*at, n.title.as_str()))
                .collect();
            assert_eq!(titles, case.posted, "{}", case.name);
            for (title, body) in case.bodies {
                let n = posted
                    .iter()
                    .map(|(_, n)| n)
                    .find(|n| n.title == *title)
                    .unwrap_or_else(|| panic!("{}: no {title}", case.name));
                assert_eq!(n.body, *body, "{}", case.name);
                assert_eq!(n.target, notify::Target::Inbox, "{}", case.name);
            }
            assert_eq!(blocked, case.blocked, "{}", case.name);
        }
    }

    /// The summary sits in no project's thread; one held item keeps its own.
    #[test]
    fn the_summary_has_no_thread() {
        let (posted, _) = run(&CASES[2]);
        assert_eq!(posted[0].1.thread.as_deref(), Some("SHOP"));
        assert_eq!(posted[1].1.thread, None);
        let (posted, _) = run(&CASES[3]);
        assert_eq!(posted[1].1.thread.as_deref(), Some("BILL"));
    }

    /// The inbox's run loop wakes at the end of a period only while something is held.
    #[test]
    fn the_run_loop_wakes_only_for_held_items() {
        let clock = FakeClock::new();
        let mut pace = Pace::default();
        let planned = |key: &str| {
            notify::plan(
                &[item(&format!("{key}:asks"), 1)],
                &HashMap::new(),
                &mut HashSet::new(),
                &Choices::default(),
            )
        };
        assert_eq!(pace.offer(clock.now(), planned("SHOP-1")).len(), 1);
        assert_eq!(pace.wake_at(), None, "nothing held");
        clock.advance(Duration::from_secs(5));
        assert!(pace.offer(clock.now(), planned("SHOP-2")).is_empty());
        let start = clock.now() - Duration::from_secs(5);
        assert_eq!(pace.wake_at(), Some(start + QUIET));
        pace.drop_held();
        assert_eq!(pace.wake_at(), None);
    }
}
