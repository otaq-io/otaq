//! The banner area and the toasts (T-0231; UI-07, SVC-03, SVC-04, USG-06, REC-06), as the GPUI app's root kept them:
//! one [`BannerModel`] and one [`ToastModel`] for the window, fed by the link's states (service banners, the
//! after-restart summary, "Reconnected"), the plan limit (`usage.summary` on `usage_changed`, `ticket_changed` and
//! resyncs), the folder trust note and warning (`onboarding.state` on [`TRUST_EVENTS`] and resyncs; PRJ-08) and the
//! service events that show a toast ([`EVENT_TOASTS`]), and one toast per burst of tickets agents created
//! (`created_by_agent`, #47). A one-second tick times the success toasts out and counts the
//! usage limit down. The notification ask and the blocked banner follow `notifications.state` (on connect and resyncs)
//! and `notifications_changed` (ATT-13, ATT-14). The flow change banner (FLW-06) is the project on screen's (the page
//! says which, `notices.project`): every project's waiting change is fetched (`flow.waiting`) on connect, resyncs and
//! [`FLOW_EVENTS`]; the toasts say when one applied or was cancelled.
//!
//! The page draws what [`Notices::view`] returns: it loads it with `notices.view` and gets every change as a
//! `notices` event. Nothing here restarts the service or resumes an agent on its own: those are the page's requests,
//! sent on the user's click (d0022, d0020).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use otaq_core::agent_tools::Tool;
use otaq_core::agent_tools::models::ModelList;
use otaq_core::agents::limits::ToolLines;
use otaq_core::agents::pause::{PauseCounts, PauseScope};
use otaq_core::attention::notify::{Target, open_env_var};
use otaq_core::attention::os_permission::NotificationsState;
use otaq_core::client::{Client, Compatibility};
use otaq_core::domain::TicketKey;
use otaq_core::flow::apply::{FlowEvent, WaitingChange};
use otaq_core::protocol::{
    Event, ModelsParams, OnboardingState, ProjectParams, Reply, Request, UsageParams,
};
use otaq_core::store::now_ms;
use otaq_view::view_models::after_restart::{ALREADY_RUNNING_MS, OnConnect, on_connect};
use otaq_view::view_models::agent_activity::{Created, CreationBatch};
use otaq_view::view_models::banners::{
    BannerModel, BannerView, Situation, SituationKind, TrustFailure,
};
use otaq_view::view_models::connection::{self, ConnectionViewModel, LinkState};
use otaq_view::view_models::limit_banner::{self, LimitLine};
use otaq_view::view_models::restart;
use otaq_view::view_models::toasts::{NewToast, RECONNECTED, ToastLevel, ToastModel, ToastView};
use otaq_view::view_models::{flow_settings, pause};
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use ts_rs::TS;

use crate::events::{EventHub, PageEvent};
use crate::service::ServiceLink;

/// How often success toasts are timed out and the usage limit's countdown is recomputed.
const TICK: Duration = Duration::from_secs(1);
/// A `usage.summary` that takes longer is given up; the next change asks again.
const USAGE_TIMEOUT: Duration = Duration::from_secs(5);
/// The service events after which the plan limit and the agents waiting for it are fetched again.
const USAGE_EVENTS: &[&str] = &["usage_changed", "ticket_changed"];
/// The service events after which the folder trust note and the projects whose trust failed are fetched again (a
/// removed project leaves the failed list without a trust event).
const TRUST_EVENTS: &[&str] = &["trust_changed", "projects_changed"];
/// The service events after which every project's waiting flow change is fetched again: a change waits, applies or
/// is cancelled, or the tickets it waits for moved (FLW-06).
const FLOW_EVENTS: &[&str] = &["flow_changed", "ticket_changed", "projects_changed"];
/// The service events after which every project's pause counts are fetched again: a project paused or resumed, or
/// what it stopped moved (PAU-08).
const PAUSE_EVENTS: &[&str] = &[
    "pause_changed",
    "projects_changed",
    "ticket_changed",
    "session_state",
];

/// The service events that show a toast: one registration per kind, each turning its event into the toast (with the
/// wording the service sent) or passing.
pub const EVENT_TOASTS: &[fn(&Event) -> Option<NewToast>] = &[
    merged_automatically,
    validation_cancelled,
    otaq_view::view_models::run_page::stopped_toast,
];

/// "SHOP-40.1 merged automatically into the Wishlist branch" · "Validated · round 1 · local, not pushed · SHOP-40.2
/// can start" (UI-07, d0064; *11 → Success toasts*). A click opens the ticket.
fn merged_automatically(event: &Event) -> Option<NewToast> {
    let Event::MergedAutomatically {
        key,
        title,
        message,
        ..
    } = event
    else {
        return None;
    };
    Some(ticket_toast(key, title, message))
}

/// "Validation cancelled: SHOP-29’s code changed" · "Round not counted · the agent was told to submit again" (UI-07,
/// d0063).
fn validation_cancelled(event: &Event) -> Option<NewToast> {
    let Event::ValidationCancelled {
        key,
        title,
        message,
        ..
    } = event
    else {
        return None;
    };
    Some(ticket_toast(key, title, message))
}

fn ticket_toast(key: &TicketKey, title: &str, message: &str) -> NewToast {
    NewToast {
        ticket: Some(key.to_string()),
        ..NewToast::new(
            ToastLevel::Success,
            title,
            (!message.is_empty()).then(|| message.to_owned()),
        )
    }
}

/// The banner rows and the toasts as the page draws them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "Notices")]
pub struct NoticesView {
    /// At most two rows, top first.
    pub banners: Vec<BannerView>,
    /// Oldest first; the newest sits at the bottom.
    pub toasts: Vec<ToastView>,
    /// "Restart required": everything else that talks to the service is disabled.
    pub only_restart_allowed: bool,
    /// Whether the board, rail and panel take input: not while connecting, reconnecting or stopped (SVC-04), nor
    /// while only Restart works. The page dims them and swallows their clicks meanwhile.
    pub input_enabled: bool,
    /// The service start the after-restart summary was raised for: the page opens the summary sheet once per
    /// value (*16 → Opening after a reboot*, T-0235).
    #[ts(type = "number | null")]
    pub summary_for: Option<i64>,
    /// What a terminal says over its scrollback while the link is down: "Reconnecting… agents keep running", or
    /// "Service stopped" once the service has ended (#58); `null` while connected.
    pub terminal_note: Option<connection::TerminalNote>,
    /// The link is down (reconnecting, stopped or failed): the header's running count is last known, so it dims.
    pub service_down: bool,
}

/// What a notification click opens (`OTAQ_OPEN`, or the `open_from_notification` event; d0090 "What a click
/// opens"), with the project to switch to: a ticket's or a validator's terminal, a Project agent in the pane, an
/// epic's lead terminal, the next item that needs you (the Needs you strip's Next, d0114), or the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "NotificationTarget")]
pub enum OpenTarget {
    /// The ticket's sheet on Session's agent, its terminal focused.
    Ticket { project: String, ticket: String },
    /// The ticket's sheet on Session's check, its validator's terminal focused.
    Validator { project: String, ticket: String },
    /// The agent in the Project agent pane, focused.
    ProjectAgent { project: String, agent: String },
    /// The epic's sheet on its Lead tab, the lead's terminal focused.
    Lead { project: String, epic: String },
    /// A grouped notification or a summary: the item the Needs you strip's Next opens (NYS-03).
    NeedsYou,
    /// The board, of `project` when one is named (a tool that isn't available: its limit banner explains it).
    Board { project: Option<String> },
}

impl From<Target> for OpenTarget {
    fn from(target: Target) -> Self {
        match target {
            Target::Ticket { key } => Self::ticket(&key),
            Target::Validator { key } => Self::Validator {
                project: key.project.to_string(),
                ticket: key.to_string(),
            },
            Target::ProjectAgent { project, agent } => Self::ProjectAgent { project, agent },
            Target::Lead { epic } => Self::Lead {
                project: epic.project.to_string(),
                epic: epic.to_string(),
            },
            Target::Inbox => Self::NeedsYou,
            Target::Board { project } => Self::Board { project },
        }
    }
}

impl OpenTarget {
    /// From a notification's value ([`Target::to_open_value`]): a ticket key, `inbox`, `validator:<key>`… Anything
    /// else opens nothing.
    pub fn parse(value: &str) -> Option<Self> {
        Target::parse_open_value(value).map(Self::from)
    }

    pub fn ticket(key: &TicketKey) -> Self {
        Self::Ticket {
            project: key.project.to_string(),
            ticket: key.to_string(),
        }
    }
}

pub struct Notices {
    state: Mutex<State>,
    events: Arc<EventHub>,
}

struct State {
    banners: BannerModel,
    toasts: ToastModel,
    link: LinkState,
    /// Every tool's banner line and the model lists of the tools Move waiting to… offers, as last fetched.
    usage: Option<LimitInputs>,
    limit: Vec<LimitLine>,
    /// When the app opened, the service start a summary was shown for, whether the app has connected yet and
    /// whether it was disconnected at the last state (REC-06).
    opened_ms: i64,
    summary_for: Option<i64>,
    connected_once: bool,
    was_disconnected: bool,
    /// The summary banner as raised, until the startup report adds its Project agents (T-0244).
    summary: Option<Situation>,
    /// A start or restart the user asked for is running.
    busy: bool,
    /// What a notification click asked this app to open when it started, until the page takes it.
    open_at_start: Option<OpenTarget>,
    published: Option<NoticesView>,
    /// The project on screen, which the flow change banner is about (the page tells, `notices.project`).
    project: Option<String>,
    /// Every project's name and its waiting flow change, as last fetched (FLW-06): the banner of the project on
    /// screen, and the toasts' words when one applies or is cancelled.
    flows: BTreeMap<String, FlowWatch>,
    /// Every project's pause counts, as last fetched (PAU-08): the paused banner of the project on screen.
    pause: Option<PauseCounts>,
    /// The service start the "… restarted; a pause never lifts on its own" toast was shown for (PAU-07).
    paused_toast_for: Option<i64>,
    /// Tickets agents created, until a quiet moment makes their toast (#47).
    creations: CreationBatch,
}

/// A project as the flow change banner and toasts name it, and its waiting change if one waits.
#[derive(Debug, Clone, Default)]
struct FlowWatch {
    name: String,
    waiting: Option<WaitingChange>,
}

impl Notices {
    /// The notices of a window, following `service` from now on. Call inside the app's tokio runtime; the task ends
    /// when the notices are dropped.
    pub fn start(service: &ServiceLink, events: Arc<EventHub>) -> Arc<Self> {
        let open_at_start = std::env::var(open_env_var())
            .ok()
            .and_then(|v| OpenTarget::parse(&v));
        let notices = Arc::new(Self::new(events.clone(), now_ms(), open_at_start));
        let (sender, receiver) = mpsc::unbounded_channel();
        events.subscribe(Box::new(move |event| sender.send(event).is_ok()));
        tokio::spawn(follow(
            Arc::downgrade(&notices),
            service.states(),
            service.clients(),
            receiver,
        ));
        notices
    }

    fn new(events: Arc<EventHub>, opened_ms: i64, open_at_start: Option<OpenTarget>) -> Self {
        Self {
            state: Mutex::new(State {
                banners: BannerModel::default(),
                toasts: ToastModel::default(),
                link: LinkState::Connecting,
                usage: None,
                limit: Vec::new(),
                opened_ms,
                summary_for: None,
                connected_once: false,
                was_disconnected: false,
                summary: None,
                busy: false,
                open_at_start,
                published: None,
                project: None,
                flows: BTreeMap::new(),
                pause: None,
                paused_toast_for: None,
                creations: CreationBatch::default(),
            }),
            events,
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().expect("notices")
    }

    pub fn view(&self) -> NoticesView {
        self.lock().view()
    }

    /// Tells the page when what it draws changed.
    pub fn publish(&self) {
        let view = {
            let mut state = self.lock();
            let view = state.view();
            if state.published.as_ref() == Some(&view) {
                return;
            }
            state.published = Some(view.clone());
            view
        };
        self.events.publish(&PageEvent::Notices { notices: view });
    }

    /// The link's state changed: raises and clears the service banners, the after-restart summary and the
    /// "Reconnected" toast. Returns whether the summary was raised.
    pub fn on_link(&self, link: &LinkState) -> bool {
        let summarized = self.lock().on_link(link, Instant::now());
        self.publish();
        summarized
    }

    pub fn toast(&self, toast: NewToast) -> u64 {
        let id = self.lock().toasts.push_toast(toast, Instant::now());
        self.publish();
        id
    }

    pub fn dismiss_toast(&self, id: u64) {
        self.lock().toasts.dismiss(id);
        self.publish();
    }

    pub fn hold_toast(&self, id: u64, held: bool) {
        self.lock().toasts.hold(id, held, Instant::now());
        self.publish();
    }

    /// The user closed a banner. Banners that can't be closed stay.
    pub fn dismiss_banner(&self, kind: SituationKind) {
        self.lock().banners.dismiss(kind);
        self.publish();
    }

    pub fn raise(&self, situation: Situation) {
        self.lock().banners.raise(situation);
        self.publish();
    }

    pub fn clear(&self, kind: SituationKind) {
        self.lock().banners.clear(kind);
        self.publish();
    }

    /// The agent counts for the restart confirmation (0, 0 while not connected).
    pub fn agent_counts(&self) -> (u32, u32) {
        match &self.lock().link {
            LinkState::Connected(info) => (info.status.agents_working, info.status.agents_waiting),
            _ => (0, 0),
        }
    }

    /// A start or restart begins: its button says so and can't be pressed again. Returns the service start the
    /// summary was last shown for, to tell afterwards whether the restart raised one.
    pub fn begin_restart(&self) -> Option<i64> {
        let summary_for = {
            let mut state = self.lock();
            state.busy = true;
            state.summary_for
        };
        self.publish();
        summary_for
    }

    /// The start or restart ended. `link` is the state now (the link may not have reported it yet): a summary banner
    /// says what was interrupted; otherwise a toast says it worked, or why not.
    pub fn end_restart(
        &self,
        outcome: Result<(), String>,
        summary_before: Option<i64>,
        link: &LinkState,
    ) {
        {
            let mut state = self.lock();
            let now = Instant::now();
            state.on_link(link, now);
            state.busy = false;
            match outcome {
                Ok(()) if state.summary_for != summary_before => {}
                Ok(()) => {
                    state.toasts.push(
                        ToastLevel::Success,
                        restart::RESTARTED,
                        Some(restart::RESTARTED_TEXT.to_owned()),
                        now,
                    );
                }
                Err(reason) => {
                    state.toasts.push(
                        ToastLevel::Error,
                        restart::RESTART_FAILED,
                        Some(reason),
                        now,
                    );
                }
            }
        }
        self.publish();
    }

    /// Every tool's banner line, freshly fetched (USG-10).
    pub fn on_usage(&self, inputs: LimitInputs) {
        {
            let mut state = self.lock();
            state.usage = Some(inputs);
            state.sync_limit(now_ms());
        }
        self.publish();
    }

    /// The folder trust from the service's notices state (PRJ-08): the one-time note while pending, and the warning
    /// for the projects whose trust couldn't be recorded.
    pub fn on_trust(&self, state: &OnboardingState) {
        let failed = state
            .trust_failed
            .iter()
            .map(|f| TrustFailure {
                project: f.project_key.clone(),
                reason: f.reason.clone(),
            })
            .collect();
        self.lock()
            .banners
            .set_trust(state.trust_note, state.trust_note_tools.clone(), failed);
        self.publish();
    }

    /// The notification banners from the service's `notifications.state` (ATT-13, ATT-14): the ask while pending, the
    /// blocked banner while the service shows it.
    pub fn on_notifications(&self, state: &NotificationsState) {
        self.lock()
            .banners
            .set_notifications(state.ask_pending, state.blocked_banner);
        self.publish();
    }

    /// The target `OTAQ_OPEN` named when the app started, once.
    /// The startup report was read: the summary banner counts the Project agents the restart ended too
    /// ("8 sessions were interrupted · 2 Project agents ended", T-0244), and the epic leads it ended, counted on
    /// their own ("· 1 epic lead ended", T-0289).
    pub fn on_startup_agents(&self, ended: u32, leads_ended: u32) {
        {
            let mut state = self.lock();
            if let Some(Situation::SessionsInterrupted {
                count,
                unclean,
                at,
                agents,
                leads,
            }) = state.summary.clone()
                && (agents, leads) != (ended, leads_ended)
                && state
                    .banners
                    .rows()
                    .iter()
                    .any(|r| r.kind == SituationKind::SessionsInterrupted)
            {
                let situation = Situation::SessionsInterrupted {
                    count,
                    unclean,
                    at,
                    agents: ended,
                    leads: leads_ended,
                };
                state.summary = Some(situation.clone());
                state.banners.raise(situation);
            }
        }
        self.publish();
    }

    /// The project on screen, for the flow change banner's Cancel change.
    pub fn project(&self) -> Option<String> {
        self.lock().project.clone()
    }

    /// The project on screen changed: the flow change banner is its own (FLW-06).
    pub fn set_project(&self, project: Option<String>) {
        {
            let mut state = self.lock();
            state.project = project;
            state.sync_flow();
            state.sync_pause();
        }
        self.publish();
    }

    /// Every project with its name and waiting change, freshly fetched (`flow.waiting`). A project that's gone is
    /// forgotten.
    pub fn on_flows(&self, flows: Vec<(String, String, Option<WaitingChange>)>) {
        {
            let mut state = self.lock();
            state.flows = flows
                .into_iter()
                .map(|(key, name, waiting)| (key, FlowWatch { name, waiting }))
                .collect();
            state.sync_flow();
        }
        self.publish();
    }

    /// A `flow_changed` event: when the waiting change of `project` applied, "shop-web’s flow changed"; when it was
    /// cancelled, "Flow change cancelled" (FLW-06, *11 → Applied, or cancelled*). The banner goes with it. A change
    /// that applied at once (nothing waited) shows no toast: the flow editor's Apply said it.
    pub fn on_flow_event(&self, project: &str, change: &FlowEvent) {
        {
            let mut state = self.lock();
            let now = Instant::now();
            let Some(watch) = state.flows.get_mut(project) else {
                return;
            };
            let name = if watch.name.is_empty() {
                project.to_owned()
            } else {
                watch.name.clone()
            };
            let toast = match change {
                FlowEvent::Applied { .. } => watch
                    .waiting
                    .take()
                    .map(|w| flow_settings::applied_toast(&name, &w.summary())),
                FlowEvent::Cancelled => watch
                    .waiting
                    .take()
                    .map(|_| flow_settings::cancelled_toast(&name)),
                _ => None,
            };
            if let Some(toast) = toast {
                state.toasts.push_toast(toast, now);
            }
            state.sync_flow();
        }
        self.publish();
    }

    /// Every project's pause counts, freshly fetched (`agents.pause_counts`): the paused banner follows (PAU-08).
    pub fn on_pause(&self, counts: PauseCounts) {
        {
            let mut state = self.lock();
            state.pause = Some(counts);
            state.sync_pause();
        }
        self.publish();
    }

    /// A ticket's agent or an epic lead created a ticket (`created_by_agent`, #47): it joins the batch the tick turns
    /// into one toast once the burst is over.
    pub fn on_created_by_agent(&self, created: Created, now: Instant) {
        self.lock().creations.add(created, now);
    }

    /// The agent-created tickets' toasts, once nothing new came for a moment.
    fn flush_creations(&self, now: Instant) {
        let mut state = self.lock();
        for toast in state.creations.take_if_quiet(now) {
            state.toasts.push_toast(toast, now);
        }
    }

    pub fn take_open_at_start(&self) -> Option<OpenTarget> {
        self.lock().open_at_start.take()
    }

    fn tick(&self) {
        self.flush_creations(Instant::now());
        {
            let mut state = self.lock();
            state.toasts.expire(Instant::now());
            state.sync_limit(now_ms());
        }
        self.publish();
    }
}

impl State {
    fn view(&self) -> NoticesView {
        NoticesView {
            banners: self
                .banners
                .rows()
                .iter()
                .map(|row| row.view(self.busy))
                .collect(),
            toasts: self.toasts.items().iter().map(|t| t.view()).collect(),
            only_restart_allowed: self.banners.only_restart_allowed(),
            input_enabled: ConnectionViewModel::new(&self.link).input_enabled
                && !self.banners.only_restart_allowed(),
            summary_for: self.summary_for,
            terminal_note: connection::terminal_note(&self.link),
            service_down: connection::service_down(&self.link),
        }
    }

    fn on_link(&mut self, link: &LinkState, now: Instant) -> bool {
        let situations = match link {
            LinkState::Connecting => Vec::new(),
            LinkState::Connected(info) => {
                let (working, waiting) = (info.status.agents_working, info.status.agents_waiting);
                match info.compatibility {
                    Compatibility::Same => Vec::new(),
                    Compatibility::ServiceOlder => {
                        vec![Situation::ServiceUpdateReady { working, waiting }]
                    }
                    Compatibility::Incompatible => {
                        vec![Situation::RestartRequired { working, waiting }]
                    }
                }
            }
            LinkState::Reconnecting { .. } => vec![Situation::Reconnecting],
            LinkState::Stopped => vec![Situation::ServiceStopped],
            // Q40: the stopped banner says why the service couldn't start.
            LinkState::Failed(reason) => vec![Situation::ServiceCouldNotStart {
                reason: reason.clone(),
            }],
        };
        for kind in [
            SituationKind::ServiceStopped,
            SituationKind::RestartRequired,
            SituationKind::ServiceUpdateReady,
            SituationKind::Reconnecting,
        ] {
            if !situations.iter().any(|s| s.kind() == kind) {
                self.banners.clear(kind);
            }
        }
        for situation in situations {
            self.banners.raise(situation);
        }
        let disconnected = matches!(
            link,
            LinkState::Reconnecting { .. } | LinkState::Stopped | LinkState::Failed(_)
        );
        // After a reboot, crash or restart: the summary of what was interrupted; after a normal reopen a quiet
        // "Reconnected" (REC-06).
        let mut summarized = false;
        if let LinkState::Connected(info) = link {
            match on_connect(
                &info.status,
                self.opened_ms,
                !self.connected_once,
                self.summary_for,
            ) {
                OnConnect::Summary(situation) => {
                    self.summary_for = Some(info.status.started_at);
                    self.summary = Some(situation.clone());
                    self.banners.raise(situation);
                    summarized = true;
                }
                OnConnect::Reconnected => {
                    self.toasts
                        .push(ToastLevel::Success, RECONNECTED, None, now);
                }
                OnConnect::Nothing => {}
            }
            self.connected_once = true;
            // A service that started again while projects were paused (PAU-07): once per service start.
            let fresh = info.status.started_at + ALREADY_RUNNING_MS >= self.opened_ms;
            if fresh && self.paused_toast_for != Some(info.status.started_at) {
                self.paused_toast_for = Some(info.status.started_at);
                if let Some(toast) = pause::restart_toast(&info.status) {
                    self.toasts.push_toast(toast, now);
                }
            }
            if self.was_disconnected && !self.busy && !summarized {
                self.toasts
                    .push(ToastLevel::Success, RECONNECTED, None, now);
            }
        }
        self.was_disconnected = disconnected;
        self.link = link.clone();
        self.sync_pause();
        summarized
    }

    /// Raises, updates or clears the paused banner: the project on screen's, "still paused" after a restart (PAU-07,
    /// PAU-08). Kept as it is while not connected (the service's start is unknown then).
    fn sync_pause(&mut self) {
        let LinkState::Connected(info) = &self.link else {
            return;
        };
        let banner = self.pause.as_ref().and_then(|counts| {
            pause::banner(self.project.as_deref(), counts, info.status.started_at)
        });
        match banner {
            Some(situation) => self.banners.raise(situation),
            None => self.banners.clear(SituationKind::Paused),
        }
    }

    /// Raises, updates or clears the one usage-limit banner with its line per tool (USG-06, USG-10, P6).
    fn sync_limit(&mut self, now_ms: i64) {
        let lines = self
            .usage
            .as_ref()
            .map(|u| limit_banner::lines(&u.lines, &u.lists, now_ms))
            .unwrap_or_default();
        if lines == self.limit {
            return;
        }
        match lines.first() {
            Some(first) => self.banners.raise(Situation::UsageLimit {
                title: first.banner.title.clone(),
                text: first.banner.text.clone(),
                resume: first.banner.resume.clone(),
                lines: lines.iter().map(|l| l.view.clone()).collect(),
            }),
            None => self.banners.clear(SituationKind::UsageLimit),
        }
        self.limit = lines;
    }

    /// Raises, updates or clears the flow change banner: the project on screen's waiting change (FLW-06).
    fn sync_flow(&mut self) {
        let waiting = self
            .project
            .as_ref()
            .and_then(|p| self.flows.get(p))
            .and_then(|w| w.waiting.as_ref());
        match waiting {
            Some(w) => self.banners.raise(Situation::FlowWaiting {
                summary: w.summary(),
                count: w.waiting_for.len(),
            }),
            None => self.banners.clear(SituationKind::FlowWaiting),
        }
    }

    /// The tool of the first line with Resume: what the banner's single Resume button resumes.
    fn first_limited_tool(&self) -> Option<Tool> {
        self.limit
            .iter()
            .find(|l| l.banner.resume.is_some())
            .map(|l| l.tool)
    }
}

/// What the usage-limit banner is drawn from: every tool's line and the model lists of its fallbacks' tools (Use
/// fallback now names the model).
#[derive(Debug, Clone, Default)]
pub struct LimitInputs {
    pub lines: ToolLines,
    pub lists: Vec<(Tool, ModelList)>,
}

/// The inputs for `lines`, asking the service (`send`) for the model list of each fallback's tool that names a model.
/// A list it can't give leaves the model as stored.
pub async fn limit_inputs<F, Fut>(lines: ToolLines, send: F) -> LimitInputs
where
    F: Fn(Request) -> Fut,
    Fut: std::future::Future<Output = Option<Reply>>,
{
    let mut tools: Vec<Tool> = lines
        .lines
        .iter()
        .filter_map(|l| {
            l.fallback
                .as_ref()
                .filter(|f| f.model.is_some())
                .map(|f| f.tool)
        })
        .collect();
    tools.sort();
    tools.dedup();
    let mut lists = Vec::new();
    for tool in tools {
        if let Some(Reply::Models(Some(copy))) =
            send(Request::AgentsModels(ModelsParams { tool })).await
        {
            lists.push((tool, copy.list));
        }
    }
    LimitInputs { lines, lists }
}

impl Notices {
    /// The tool the banner's Resume button stands for when the page doesn't say (the first line with Resume).
    pub fn limited_tool(&self) -> Option<Tool> {
        self.lock().first_limited_tool()
    }
}

/// Follows the link's states, the page's event stream and the tick until the notices are dropped.
async fn follow(
    weak: Weak<Notices>,
    mut states: watch::Receiver<LinkState>,
    clients: watch::Receiver<Option<Client>>,
    mut events: mpsc::UnboundedReceiver<Value>,
) {
    let mut tick = tokio::time::interval(TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut usage_due = true;
    let mut trust_due = true;
    let mut notifications_due = true;
    let mut flows_due = true;
    let mut pause_due = true;
    // After the summary: the startup report names the Project agents the restart ended (T-0244).
    let mut report_due = false;
    let first = states.borrow_and_update().clone();
    if let Some(notices) = weak.upgrade() {
        report_due = notices.on_link(&first);
    }
    loop {
        tokio::select! {
            changed = states.changed() => {
                if changed.is_err() {
                    return;
                }
                let state = states.borrow_and_update().clone();
                let Some(notices) = weak.upgrade() else { return };
                if matches!(state, LinkState::Connected(_)) {
                    usage_due = true;
                    trust_due = true;
                    notifications_due = true;
                    flows_due = true;
                    pause_due = true;
                }
                report_due |= notices.on_link(&state);
            }
            event = events.recv() => {
                let Some(event) = event else { return };
                let Some(notices) = weak.upgrade() else { return };
                match event["kind"].as_str() {
                    Some("resync") => {
                        usage_due = true;
                        trust_due = true;
                        notifications_due = true;
                        flows_due = true;
                        pause_due = true;
                    }
                    Some("service") => {
                        let name = event["name"].as_str().unwrap_or_default();
                        usage_due |= USAGE_EVENTS.contains(&name);
                        trust_due |= TRUST_EVENTS.contains(&name);
                        flows_due |= FLOW_EVENTS.contains(&name);
                        pause_due |= PAUSE_EVENTS.contains(&name);
                        if let Ok(event) = serde_json::from_value::<Event>(event["event"].clone()) {
                            if let Event::NotificationsChanged { state } = &event {
                                notices.on_notifications(state);
                            }
                            if let Event::FlowChanged { project, change } = &event {
                                notices.on_flow_event(project, change);
                            }
                            if let Event::CreatedByAgent { key, by, status, .. } = &event {
                                notices.on_created_by_agent(
                                    Created { by: by.clone(), key: key.clone(), status: *status },
                                    Instant::now(),
                                );
                            }
                            for toast in EVENT_TOASTS.iter().filter_map(|to_toast| to_toast(&event)) {
                                notices.toast(toast);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ = tick.tick() => {
                let Some(notices) = weak.upgrade() else { return };
                let client = clients.borrow().clone();
                if usage_due && let Some(client) = client {
                    usage_due = false;
                    let summary = tokio::time::timeout(
                        USAGE_TIMEOUT,
                        client.request(Request::UsageSummary(UsageParams { project: None })),
                    )
                    .await;
                    // Not connected or too slow: the next change or resync asks again.
                    if let Ok(Ok(Reply::Usage(summary))) = summary {
                        let inputs = limit_inputs(summary.tool_lines, |request| {
                            let client = client.clone();
                            async move {
                                tokio::time::timeout(USAGE_TIMEOUT, client.request(request))
                                    .await
                                    .ok()
                                    .and_then(Result::ok)
                            }
                        })
                        .await;
                        notices.on_usage(inputs);
                    }
                }
                let client = clients.borrow().clone();
                if trust_due && let Some(client) = client {
                    trust_due = false;
                    let state =
                        tokio::time::timeout(USAGE_TIMEOUT, client.request(Request::OnboardingState)).await;
                    if let Ok(Ok(Reply::OnboardingState(state))) = state {
                        notices.on_trust(&state);
                    }
                }
                let client = clients.borrow().clone();
                if notifications_due && let Some(client) = client {
                    notifications_due = false;
                    let state =
                        tokio::time::timeout(USAGE_TIMEOUT, client.request(Request::NotificationsState)).await;
                    if let Ok(Ok(Reply::NotificationsState(state))) = state {
                        notices.on_notifications(&state);
                    }
                }
                let client = clients.borrow().clone();
                if flows_due && let Some(client) = client {
                    flows_due = false;
                    if let Some(flows) = fetch_flows(&client).await {
                        notices.on_flows(flows);
                    }
                }
                let client = clients.borrow().clone();
                if pause_due && let Some(client) = client {
                    pause_due = false;
                    let counts = tokio::time::timeout(
                        USAGE_TIMEOUT,
                        client.request(Request::AgentsPauseCounts(PauseScope::All)),
                    )
                    .await;
                    if let Ok(Ok(Reply::PauseCounts(counts))) = counts {
                        notices.on_pause(*counts);
                    }
                }
                let client = clients.borrow().clone();
                if report_due && let Some(client) = client {
                    report_due = false;
                    let report =
                        tokio::time::timeout(USAGE_TIMEOUT, client.request(Request::StartupReport)).await;
                    if let Ok(Ok(Reply::StartupReport(report))) = report {
                        notices.on_startup_agents(
                            report.project_agents_ended.len() as u32,
                            report.leads_ended.len() as u32,
                        );
                    }
                }
                notices.tick();
            }
        }
    }
}

/// Every project with its name and waiting flow change (`project.list`, then `flow.waiting` each). `None` when the
/// service didn't answer in time: the next change or resync asks again.
async fn fetch_flows(client: &Client) -> Option<Vec<(String, String, Option<WaitingChange>)>> {
    let ask = |request| tokio::time::timeout(USAGE_TIMEOUT, client.request(request));
    let Ok(Ok(Reply::Projects(projects))) = ask(Request::ProjectList).await else {
        return None;
    };
    let mut flows = Vec::new();
    for project in projects {
        let request = Request::FlowWaiting(ProjectParams {
            project: project.key.clone(),
        });
        let waiting = match ask(request).await {
            Ok(Ok(Reply::FlowWaiting(waiting))) => waiting.map(|w| *w),
            _ => return None,
        };
        flows.push((project.key, project.name, waiting));
    }
    Some(flows)
}

#[cfg(test)]
mod tests {
    use otaq_core::agent_tools::Tool;
    use otaq_core::agents::ModelRef;
    use otaq_core::agents::limits::{self as moves, ToolLine};
    use otaq_core::client::ServiceInfo;
    use otaq_core::protocol::{PROTOCOL_VERSION, ServiceStatus};
    use otaq_core::usage::limit::LimitKind;

    use super::*;

    const OPENED: i64 = 1_791_600_000_000;

    fn notices() -> Notices {
        Notices::new(Arc::new(EventHub::default()), OPENED, None)
    }

    fn connected(compatibility: Compatibility, status: ServiceStatus) -> LinkState {
        LinkState::Connected(ServiceInfo {
            status: ServiceStatus {
                protocol_version: PROTOCOL_VERSION,
                ..status
            },
            compatibility,
        })
    }

    fn running(working: u32, waiting: u32) -> ServiceStatus {
        ServiceStatus {
            started_at: OPENED - 3_600_000,
            agents_working: working,
            agents_waiting: waiting,
            ..Default::default()
        }
    }

    fn titles(view: &NoticesView) -> Vec<(&str, &str)> {
        view.banners
            .iter()
            .map(|b| (b.title.as_str(), b.text.as_str()))
            .collect()
    }

    fn toasts(view: &NoticesView) -> Vec<&str> {
        view.toasts.iter().map(|t| t.title.as_str()).collect()
    }

    #[test]
    fn the_notification_banners_follow_the_notifications_state() {
        use otaq_core::attention::notify::PermissionState;
        let n = notices();
        let state = |ask_pending: bool, blocked_banner: bool| NotificationsState {
            permission: PermissionState::TurnedOff,
            ask_pending,
            blocked_banner,
        };
        n.on_notifications(&state(true, true));
        let kinds: Vec<SituationKind> = n.view().banners.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [
                SituationKind::NotificationsBlocked,
                SituationKind::NotificationsAsk
            ]
        );
        n.dismiss_banner(SituationKind::NotificationsBlocked);
        n.on_notifications(&state(false, false));
        assert!(n.view().banners.is_empty());
        n.on_notifications(&state(false, true));
        assert_eq!(n.view().banners.len(), 1, "raised again by the service");
    }

    /// FLW-06: the flow change banner is the project on screen's; when its change applies the banner goes and the
    /// toast says "shop-web’s flow changed"; Cancel change says "Flow change cancelled". A change that applied at
    /// once (nothing waited) shows nothing.
    #[test]
    fn the_flow_change_banner_follows_the_project_on_screen() {
        use otaq_core::flow::store::AppliedBy;
        let n = notices();
        let waiting = |tickets: &[&str]| WaitingChange {
            project: "SHOP".into(),
            yaml: String::new(),
            applied_by: AppliedBy::User,
            agent_name: None,
            preset: None,
            changes: vec!["QA after Security".into()],
            since: 0,
            wait: otaq_core::flow::apply::Wait::WholeBoard,
            hold: otaq_core::flow::apply::Hold::READY_FOR_DEV,
            waiting_for: tickets.iter().map(|k| k.parse().unwrap()).collect(),
        };
        n.on_flows(vec![
            (
                "SHOP".into(),
                "shop-web".into(),
                Some(waiting(&["SHOP-12", "SHOP-44"])),
            ),
            ("BILL".into(), "billing-api".into(), None),
        ]);
        assert!(n.view().banners.is_empty(), "no project on screen yet");
        n.set_project(Some("BILL".into()));
        assert!(n.view().banners.is_empty(), "another project's change");
        n.set_project(Some("SHOP".into()));
        let view = n.view();
        assert_eq!(view.banners[0].kind, SituationKind::FlowWaiting);
        assert_eq!(view.banners[0].title, "Flow change waiting");
        assert!(
            view.banners[0]
                .text
                .starts_with("QA after Security \u{b7} 2 tickets still working.")
        );
        assert_eq!(view.banners[0].buttons[0].label, "Apply now");

        n.on_flow_event("SHOP", &FlowEvent::Applied { version: 3 });
        let view = n.view();
        assert!(view.banners.is_empty());
        assert_eq!(toasts(&view), ["shop-web\u{2019}s flow changed"]);
        assert_eq!(
            view.toasts[0].text.as_deref(),
            Some("QA after Security is on the board. New tickets start again.")
        );
        n.on_flow_event("SHOP", &FlowEvent::Applied { version: 4 });
        assert_eq!(n.view().toasts.len(), 1, "nothing waited: no toast");

        n.on_flows(vec![("SHOP".into(), "shop-web".into(), Some(waiting(&[])))]);
        assert_eq!(n.view().banners.len(), 1);
        n.on_flow_event("SHOP", &FlowEvent::Cancelled);
        let view = n.view();
        assert!(view.banners.is_empty());
        assert_eq!(view.toasts[1].title, "Flow change cancelled");
        assert_eq!(
            view.toasts[1].text.as_deref(),
            Some("shop-web keeps its flow. New tickets start again.")
        );
    }

    #[test]
    fn the_folder_trust_banners_follow_the_notices_state() {
        use otaq_core::agent_tools::setup::TrustFailure as Failed;
        let n = notices();
        let state = |note: bool, failed: &[&str]| OnboardingState {
            trust_note: note,
            trust_note_tools: Vec::new(),
            trust_failed: failed
                .iter()
                .map(|key| Failed {
                    project_key: (*key).to_owned(),
                    tool: otaq_core::agent_tools::Tool::Claude,
                    reason: "Claude Code's config file couldn't be written".to_owned(),
                })
                .collect(),
            has_projects: true,
            permission_warning_dismissed: false,
        };
        n.on_trust(&state(true, &["NOTE"]));
        let kinds: Vec<SituationKind> = n.view().banners.iter().map(|b| b.kind).collect();
        assert_eq!(
            kinds,
            [SituationKind::TrustFailed, SituationKind::TrustNote]
        );
        n.dismiss_banner(SituationKind::TrustNote);
        n.on_trust(&state(false, &["NOTE", "SHOP"]));
        let view = n.view();
        assert_eq!(view.banners.len(), 1);
        assert_eq!(
            view.banners[0].title,
            "Claude Code doesn’t trust 2 projects’ folders:"
        );
        n.on_trust(&state(false, &[]));
        assert!(n.view().banners.is_empty());
    }

    #[test]
    fn the_service_banners_follow_the_link() {
        let n = notices();
        n.on_link(&connected(Compatibility::ServiceOlder, running(3, 2)));
        assert_eq!(
            titles(&n.view()),
            [(
                "Service update ready",
                "Restart now (3 agents working, 2 waiting) · or later, when it suits you"
            )]
        );
        assert_eq!(n.agent_counts(), (3, 2));
        n.on_link(&connected(Compatibility::Incompatible, running(3, 2)));
        let view = n.view();
        assert_eq!(view.banners.len(), 1, "one service banner at a time");
        assert_eq!(view.banners[0].title, "Restart required");
        assert!(view.only_restart_allowed);
        assert!(!view.input_enabled);
        assert_eq!(view.terminal_note, None);
        assert!(!view.service_down);
        n.on_link(&LinkState::Reconnecting { attempt: 1 });
        assert_eq!(titles(&n.view())[0].0, "Reconnecting…");
        assert!(!n.view().input_enabled, "SVC-04");
        assert_eq!(
            n.view().terminal_note.map(|t| t.text),
            Some("Reconnecting… agents keep running")
        );
        n.on_link(&LinkState::Stopped);
        assert_eq!(titles(&n.view())[0].0, "Background service stopped");
        // #58: the terminals agree with the banner, and the header's running count dims.
        assert_eq!(
            n.view().terminal_note.map(|t| t.text),
            Some("Service stopped")
        );
        assert!(n.view().service_down);
        n.on_link(&connected(Compatibility::Same, running(0, 0)));
        assert!(n.view().banners.is_empty());
        assert!(!n.view().only_restart_allowed);
        assert!(n.view().input_enabled);
    }

    #[test]
    fn a_failed_start_says_why_in_the_stopped_banner() {
        let n = notices();
        n.on_link(&LinkState::Failed(
            "Couldn't start the background service: the data folder's path is too long".into(),
        ));
        let view = n.view();
        assert_eq!(view.banners.len(), 1);
        assert_eq!(view.banners[0].title, "Background service couldn’t start");
        assert!(view.banners[0].text.contains("path is too long"));
        assert_eq!(view.banners[0].buttons[0].label, "Start service");
    }

    #[test]
    fn a_normal_reopen_and_a_reconnect_are_a_quiet_toast() {
        let n = notices();
        n.on_link(&connected(Compatibility::Same, running(0, 0)));
        assert_eq!(toasts(&n.view()), [RECONNECTED]);
        n.dismiss_toast(n.view().toasts[0].id);
        n.on_link(&LinkState::Reconnecting { attempt: 1 });
        assert!(toasts(&n.view()).is_empty());
        n.on_link(&connected(Compatibility::Same, running(0, 0)));
        assert_eq!(toasts(&n.view()), [RECONNECTED]);
        assert!(n.view().banners.is_empty(), "reconnecting cleared");
    }

    /// PAU-07, PAU-08: the paused banner is the project on screen's, first and closed only by Resume; after the
    /// service started again it says "still paused", and a toast says a pause never lifts on its own, once.
    #[test]
    fn the_paused_banner_and_the_restart_toast() {
        use otaq_core::agents::pause::ProjectPause;
        let n = notices();
        let project = |key: &str, name: &str, since: Option<i64>, working, paused| ProjectPause {
            project: key.into(),
            name: name.into(),
            paused_since: since,
            working,
            paused,
        };
        let counts = |shop_since| PauseCounts {
            working: 0,
            paused: 0,
            projects: vec![
                project("BILL", "billing-api", None, 2, 0),
                project("SHOP", "shop-web", shop_since, 0, 5),
            ],
        };
        n.on_link(&connected(Compatibility::Same, running(2, 0)));
        n.on_notifications(&NotificationsState {
            permission: otaq_core::attention::notify::PermissionState::TurnedOff,
            ask_pending: false,
            blocked_banner: true,
        });
        n.set_project(Some("SHOP".into()));
        n.on_pause(counts(Some(OPENED)));
        let view = n.view();
        assert_eq!(
            titles(&view)[0],
            (
                "shop-web is paused.",
                "5 agents stopped; Resume continues where each one was. Queued tickets wait; questions for you stay \
                 open. billing-api keeps working."
            )
        );
        assert_eq!(view.banners[0].kind, SituationKind::Paused);
        assert_eq!(
            view.banners[0].tone,
            otaq_view::view_models::banners::BannerTone::Warning
        );
        assert!(!view.banners[0].dismissable && view.banners[0].buttons.is_empty());
        assert_eq!(
            view.banners[1].kind,
            SituationKind::NotificationsBlocked,
            "first in the stack"
        );
        n.dismiss_banner(SituationKind::Paused);
        assert_eq!(n.view().banners[0].kind, SituationKind::Paused, "no close");
        n.set_project(Some("BILL".into()));
        assert_ne!(
            n.view().banners[0].kind,
            SituationKind::Paused,
            "billing-api runs"
        );
        n.set_project(Some("SHOP".into()));
        assert_eq!(toasts(&n.view()), [RECONNECTED], "no restart yet");
        n.dismiss_toast(n.view().toasts[0].id);

        // The service starts again: the pause stays, the banner says so, one toast.
        let restarted = ServiceStatus {
            started_at: OPENED + 60_000,
            paused: vec![project("SHOP", "shop-web", Some(OPENED), 0, 5)],
            ..Default::default()
        };
        n.on_link(&LinkState::Reconnecting { attempt: 1 });
        n.on_link(&connected(Compatibility::Same, restarted.clone()));
        let view = n.view();
        assert_eq!(titles(&view)[0].0, "shop-web is still paused.");
        assert_eq!(
            view.toasts
                .iter()
                .map(|t| (t.title.as_str(), t.text.as_deref()))
                .collect::<Vec<_>>()[0],
            (
                pause::restarted_line().as_str(),
                Some("5 agents wait where each one was; Resume when you\u{2019}re ready.")
            )
        );
        let shown = n.view().toasts.len();
        n.on_link(&connected(Compatibility::Same, restarted));
        assert_eq!(n.view().toasts.len(), shown, "once per service start");

        n.on_pause(counts(None));
        assert_ne!(n.view().banners[0].kind, SituationKind::Paused, "resumed");
    }

    #[test]
    fn after_a_reboot_the_summary_shows_once() {
        let n = notices();
        let rebooted = ServiceStatus {
            started_at: OPENED + 1_000,
            interrupted_at_start: 4,
            unclean_start: true,
            ..Default::default()
        };
        assert!(n.on_link(&connected(Compatibility::Same, rebooted.clone())));
        let view = n.view();
        assert_eq!(view.banners[0].title, "4 sessions were interrupted");
        assert!(toasts(&view).is_empty(), "the summary says it, no toast");
        assert_eq!(
            view.summary_for,
            Some(OPENED + 1_000),
            "the page opens the sheet"
        );
        // The startup report adds the Project agents the restart ended (T-0244).
        n.on_startup_agents(2, 0);
        assert_eq!(
            n.view().banners[0].title,
            "4 sessions were interrupted \u{b7} 2 Project agents ended"
        );
        // And the epic leads, counted on their own (T-0289).
        n.on_startup_agents(2, 1);
        assert_eq!(
            n.view().banners[0].title,
            "4 sessions were interrupted \u{b7} 2 Project agents ended \u{b7} 1 epic lead ended"
        );
        assert!(!n.on_link(&connected(Compatibility::Same, rebooted)));
        n.dismiss_banner(SituationKind::SessionsInterrupted);
        assert!(n.view().banners.is_empty());
        n.on_startup_agents(3, 0);
        assert!(
            n.view().banners.is_empty(),
            "a dismissed summary stays dismissed"
        );
    }

    #[test]
    fn a_restart_ends_with_a_toast_or_the_summary() {
        let n = notices();
        n.on_link(&connected(Compatibility::ServiceOlder, running(3, 2)));
        n.dismiss_toast(n.view().toasts[0].id);
        let before = n.begin_restart();
        let view = n.view();
        assert_eq!(view.banners[0].buttons[0].label, "Restarting…");
        assert!(!view.banners[0].buttons[0].enabled);
        n.on_link(&LinkState::Reconnecting { attempt: 1 });
        // The new service interrupted nobody: a toast says it worked, not "Reconnected".
        let fresh = ServiceStatus {
            started_at: OPENED + 600_000,
            ..Default::default()
        };
        n.end_restart(Ok(()), before, &connected(Compatibility::Same, fresh));
        let view = n.view();
        assert!(view.banners.is_empty());
        assert_eq!(toasts(&view), [restart::RESTARTED]);

        // Interrupted sessions: the summary banner instead of the toast.
        let n = notices();
        n.on_link(&connected(Compatibility::ServiceOlder, running(3, 2)));
        let before = n.begin_restart();
        let restarted = ServiceStatus {
            started_at: OPENED + 600_000,
            interrupted_at_start: 3,
            ..Default::default()
        };
        n.end_restart(Ok(()), before, &connected(Compatibility::Same, restarted));
        let view = n.view();
        assert_eq!(view.banners[0].title, "Service restarted");
        assert_eq!(toasts(&view), [RECONNECTED], "only the first connect's");

        n.begin_restart();
        n.end_restart(Err("timed out".into()), None, &LinkState::Stopped);
        let view = n.view();
        let failed = view.toasts.last().unwrap();
        assert_eq!(failed.title, restart::RESTART_FAILED);
        assert_eq!(failed.text.as_deref(), Some("timed out"));
        assert_eq!(failed.level, ToastLevel::Error);
    }

    fn limit_line(tool: Tool, resets_at: Option<i64>) -> ToolLine {
        ToolLine {
            tool,
            situation: moves::Situation::Limit {
                kind: LimitKind::Session,
                window_minutes: None,
                resets_at,
                reset_seen: false,
                hit_at: now_ms() - 60_000,
            },
            stopped: 8,
            waiting: 2,
            on_fallback: 0,
            resume_enabled: resets_at.is_none(),
            fallback: None,
            fallback_can_start: false,
        }
    }

    fn inputs(lines: Vec<ToolLine>, several_tools: bool) -> LimitInputs {
        LimitInputs {
            lines: ToolLines {
                lines,
                several_tools,
            },
            lists: Vec::new(),
        }
    }

    #[test]
    fn the_usage_limit_is_one_banner_that_follows_the_plan() {
        let n = notices();
        n.on_usage(inputs(
            vec![limit_line(Tool::Claude, Some(now_ms() + 3 * 3_600_000))],
            false,
        ));
        let view = n.view();
        assert_eq!(view.banners.len(), 1);
        assert_eq!(view.banners[0].title, "Session limit reached");
        assert!(
            !view.banners[0].buttons[0].enabled,
            "Resume waits for the reset"
        );
        assert_eq!(view.banners[0].lines.len(), 1);
        assert_eq!(n.limited_tool(), Some(Tool::Claude));
        n.on_usage(inputs(vec![limit_line(Tool::Claude, None)], false));
        let view = n.view();
        assert_eq!(view.banners.len(), 1, "updated, never a second banner");
        assert_eq!(view.banners[0].title, "Usage limit reached");
        assert!(view.banners[0].buttons[0].enabled);
        n.on_usage(LimitInputs::default());
        assert!(n.view().banners.is_empty());
        assert_eq!(n.limited_tool(), None);
    }

    #[test]
    fn several_tools_get_a_line_each_in_one_banner() {
        let n = notices();
        let codex = ToolLine {
            situation: moves::Situation::Unavailable {
                reason: "signed out".into(),
                sign_in: true,
                hidden: false,
                since: now_ms(),
            },
            stopped: 0,
            resume_enabled: false,
            ..limit_line(Tool::Codex, None)
        };
        n.on_usage(inputs(
            vec![
                limit_line(Tool::Claude, Some(now_ms() + 3 * 3_600_000)),
                codex,
            ],
            true,
        ));
        let view = n.view();
        assert_eq!(view.banners.len(), 1, "one banner for the situation");
        let lines: Vec<&str> = view.banners[0]
            .lines
            .iter()
            .map(|l| l.title.as_str())
            .collect();
        assert_eq!(
            lines,
            ["Claude Code limit reached", "Codex isn’t available"]
        );
        assert_eq!(view.banners[0].title, "Claude Code limit reached");
        assert_eq!(view.banners[0].buttons[0].label, "Resume Claude Code");
    }

    #[tokio::test]
    async fn the_inputs_ask_for_the_fallbacks_model_list_once_per_tool() {
        use std::sync::atomic::{AtomicU32, Ordering};
        let on = |tool, model: Option<&str>| ToolLine {
            fallback: Some(ModelRef {
                tool: Tool::Codex,
                model: model.map(Into::into),
            }),
            fallback_can_start: true,
            ..limit_line(tool, None)
        };
        let lines = vec![
            on(Tool::Claude, Some("gpt-5.5-pro")),
            on(Tool::Copilot, Some("gpt-5.5")),
            on(Tool::OpenCode, None),
        ];
        let asked = AtomicU32::new(0);
        let inputs = limit_inputs(
            ToolLines {
                lines,
                several_tools: true,
            },
            |request| {
                asked.fetch_add(1, Ordering::SeqCst);
                assert!(matches!(
                    request,
                    Request::AgentsModels(ModelsParams { tool: Tool::Codex })
                ));
                async { None }
            },
        )
        .await;
        assert_eq!(asked.load(Ordering::SeqCst), 1);
        assert!(inputs.lists.is_empty(), "no list: the model as stored");
    }

    #[test]
    fn changes_reach_the_page_once() {
        let hub = Arc::new(EventHub::default());
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        hub.subscribe(Box::new(move |event| {
            record.lock().unwrap().push(event);
            true
        }));
        let n = Notices::new(hub, OPENED, None);
        n.raise(Situation::PermissionWarning);
        n.publish();
        n.tick();
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 1, "nothing changed after the raise: {seen:?}");
        assert_eq!(seen[0]["kind"], "notices");
        assert_eq!(
            seen[0]["notices"]["banners"][0]["title"],
            "Permission prompts are on"
        );
    }

    /// T-0371 (d0090): every target a notification can carry, from `OTAQ_OPEN`; the values from before M25 (a key,
    /// `inbox`) read as before.
    #[test]
    fn notification_targets() {
        assert_eq!(
            OpenTarget::parse("SHOP-22"),
            Some(OpenTarget::Ticket {
                project: "SHOP".into(),
                ticket: "SHOP-22".into()
            })
        );
        assert_eq!(
            OpenTarget::parse("SHOP-40.1"),
            Some(OpenTarget::Ticket {
                project: "SHOP".into(),
                ticket: "SHOP-40.1".into()
            })
        );
        assert_eq!(OpenTarget::parse("inbox"), Some(OpenTarget::NeedsYou));
        assert_eq!(
            OpenTarget::parse("validator:SHOP-33"),
            Some(OpenTarget::Validator {
                project: "SHOP".into(),
                ticket: "SHOP-33".into()
            })
        );
        assert_eq!(
            OpenTarget::parse("agent:SHOP:pa7"),
            Some(OpenTarget::ProjectAgent {
                project: "SHOP".into(),
                agent: "pa7".into()
            })
        );
        assert_eq!(
            OpenTarget::parse("lead:SHOP-40"),
            Some(OpenTarget::Lead {
                project: "SHOP".into(),
                epic: "SHOP-40".into()
            })
        );
        assert_eq!(
            OpenTarget::parse("board"),
            Some(OpenTarget::Board { project: None })
        );
        assert_eq!(
            serde_json::to_value(OpenTarget::parse("board:SHOP").unwrap()).unwrap(),
            serde_json::json!({"kind": "board", "project": "SHOP"})
        );
        assert_eq!(OpenTarget::parse("not a key"), None);
        let n = Notices::new(
            Arc::new(EventHub::default()),
            OPENED,
            OpenTarget::parse("SHOP-22"),
        );
        assert!(n.take_open_at_start().is_some());
        assert_eq!(n.take_open_at_start(), None, "only once");
    }

    #[test]
    fn auto_merge_and_a_cancelled_validation_show_their_toasts() {
        use otaq_core::domain::validation::{CancelledBy, cancelled_toast, merged_toast};
        let n = notices();
        let key: TicketKey = "SHOP-40.1".parse().unwrap();
        let (title, message) = merged_toast(
            "SHOP-40.1",
            "the Wishlist branch",
            Some(1),
            &["SHOP-40.2".into()],
        );
        let merged = Event::MergedAutomatically {
            project: "SHOP".into(),
            key: key.clone(),
            title,
            message,
        };
        let (title, message) = cancelled_toast("SHOP-29", CancelledBy::Code);
        let cancelled = Event::ValidationCancelled {
            project: "SHOP".into(),
            key: "SHOP-29".parse().unwrap(),
            title,
            message,
        };
        let other = Event::InboxChanged {
            counts: Default::default(),
        };
        for event in [&merged, &cancelled, &other] {
            for toast in EVENT_TOASTS.iter().filter_map(|to_toast| to_toast(event)) {
                n.toast(toast);
            }
        }
        let view = n.view();
        assert_eq!(
            toasts(&view),
            [
                "SHOP-40.1 merged automatically into the Wishlist branch",
                "Validation cancelled: SHOP-29\u{2019}s code changed"
            ]
        );
        assert_eq!(
            view.toasts[0].text.as_deref(),
            Some("Validated · round 1 · local, not pushed · SHOP-40.2 can start")
        );
        assert_eq!(view.toasts[0].ticket.as_deref(), Some("SHOP-40.1"));
        assert_eq!(
            view.toasts[0].icon,
            Some(otaq_view::icons::IconName::GitMerge)
        );
        assert_eq!(
            view.toasts[1].text.as_deref(),
            Some("Round not counted · the agent was told to submit again")
        );
        assert_eq!(view.toasts[1].icon, Some(otaq_view::icons::IconName::Ban));
        assert_eq!(view.toasts[1].level, ToastLevel::Success);
    }

    /// #47: the tickets an agent created make one toast once the burst is over; a click opens a single one.
    #[test]
    fn an_agent_s_new_tickets_make_one_toast_per_burst() {
        use otaq_core::domain::Status;
        use otaq_view::view_models::agent_activity::BATCH_QUIET;
        let n = notices();
        let t0 = Instant::now();
        for key in ["DEMO-12", "DEMO-13"] {
            n.on_created_by_agent(
                Created {
                    by: "DEMO-7".into(),
                    key: key.parse().unwrap(),
                    status: Status::Backlog,
                },
                t0,
            );
        }
        n.flush_creations(t0);
        assert!(n.view().toasts.is_empty(), "not while the burst goes on");
        n.flush_creations(t0 + BATCH_QUIET);
        let view = n.view();
        assert_eq!(toasts(&view), ["2 tickets created by agent"]);
        assert_eq!(
            view.toasts[0].text.as_deref(),
            Some("DEMO-7 created DEMO-12, DEMO-13 \u{2192} Backlog")
        );
        assert_eq!(
            view.toasts[0].icon,
            Some(otaq_view::icons::IconName::Sparkles)
        );
        n.flush_creations(t0 + BATCH_QUIET * 2);
        assert_eq!(n.view().toasts.len(), 1, "once");
    }
}
